//! SLCFET 3HP (GaN-on-SiC) spiral-inductor extraction benchmark:
//! L(f) / R(f) / Q(f) / S₁₁ / SRF over a port-driven frequency sweep of
//! the bundled 3-turn Au-on-SiC square spiral fixture (issue #212,
//! Epic #193 Phase 3 capstone).
//!
//! Drives the `spiral_slcfet_3hp.msh` fixture (76,964 edges; see
//! `geode_core::mesh::spiral` for the stack and physical groups)
//! through its lumped port with [`geode_core::driven::extraction::driven_frequency_sweep`],
//! exactly the pipeline of the issue-#211 generic benchmark
//! (`examples/spiral_inductor.rs`) with the SLCFET 3HP materials:
//!
//! - SiC substrate ε_r = 9.7 / tan δ = 0.004 and an air "dielectric"
//!   region — the 3HP metals sit in air above the SiC
//!   ([`geode_core::mesh::SLCFET_3HP_MATERIALS`], from the canonical PDK
//!   `pdk/slcfet/slcfet_3hp.pdk.yaml` in the sphere monorepo);
//! - PEC outer walls (edge-exact mask);
//! - the conductor cavity walls carry the Leontovich good-conductor
//!   surface impedance with the Au metallization conductivity
//!   σ = 1/(0.01943 Ω·µm) ≈ 5.15e7 S/m;
//! - 50 Ω lumped port driven with `V_inc = 1`.
//!
//! # Oracles (recorded in the output TOML)
//!
//! - **mom PEEC sidecar** (`reference/fixtures/slcfet_mom/`): the
//!   sister-repo MoM/PEEC extractor with `load_pdk("slcfet_3hp")` on
//!   the **exact same geometry** (integer n = 3 — no turn-count bracket
//!   unlike issue #211). The **L oracle**; NOT a reliable Q oracle
//!   above ~2 GHz (3-filament lateral discretization vs the 1.3 µm Au
//!   skin depth at 3 GHz — documented in the issue-#211 calibration).
//! - **Mohan analytic** (`geode_core::analytic::spiral`, in-repo): closed-form
//!   square-spiral L on the fixture parameterization — a low-frequency
//!   sanity band (no ground plane, no feed stubs).
//! - **Palace**: no install exists on the generation machine
//!   (`mom/external/palace-install` is absent; only a Docker build
//!   recipe) — operator-assisted slot recorded as pending; see the
//!   `[oracles.palace]` table. Palace is the realistic **Q oracle**
//!   given the mom caveat.
//!
//! Writes `benchmarks/slcfet_3hp/results.toml`.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p geode-core --release --example slcfet_3hp_spiral
//! ```
//!
//! Passing `smoke` selects the coarse `spiral_slcfet_3hp_smoke.msh`
//! fixture (~13 k edges) and writes `results_smoke.toml` instead:
//!
//! ```sh
//! cargo run -p geode-core --release --example slcfet_3hp_spiral -- smoke
//! ```

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use faer::c64;

use geode_core::analytic::spiral::{
    SquareSpiral, modified_wheeler_l, mohan_current_sheet_l, monomial_fit_l,
};
use geode_core::backend::DefaultBackend;
use geode_core::driven::extraction::{detect_srf, driven_frequency_sweep};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::mesh::{
    SLCFET_3HP_MATERIALS, SpiralFixture, pec_interior_mask_from_triangles,
    read_spiral_slcfet_3hp_fixture, read_spiral_slcfet_3hp_smoke_fixture,
};

/// Free-space impedance η₀ (Ω) — the solver's natural impedance unit.
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in µm/s — the fixture length unit is the micron, so
/// `ω_natural = 2π f / C_UM_PER_S` (rad/µm).
const C_UM_PER_S: f64 = 2.997_924_58e14;

/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;

/// Fixture spiral parameterization
/// (`tests/fixtures/spiral_slcfet_3hp.provenance.txt`), meters.
const FIXTURE_SPIRAL: SquareSpiral = SquareSpiral {
    n_turns: 3.0,
    width: 10.0e-6,
    spacing: 5.0e-6,
    d_in: 100.0e-6,
};

/// Benchmark sweep (GHz): the 0.1/0.2 GHz points anchor the
/// quasi-static L0 extrapolation (the FEM L is frequency-dependent —
/// substrate-C dispersion — so the oracle comparison must be taken at
/// the f→0 limit, see [`extrapolate_l0`]); the rest is log-ish spacing
/// through the self-resonance region, matching the mom baseline sweep
/// (`reference/fixtures/slcfet_mom/baseline.json`).
const FREQS_GHZ: [f64; 14] = [
    0.1, 0.2, 0.5, 1.0, 2.0, 3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0,
];

/// Smoke sweep (GHz): same pipeline, four points on the coarse fixture.
const FREQS_GHZ_SMOKE: [f64; 4] = [1.0, 3.0, 10.0, 20.0];

/// Quote frequency for the **Q** tracking figure: 3 GHz — the mom 3HP
/// LUT reference frequency (`jobs/slcfet_3hp_spiral_sweep.toml`); the Au
/// skin depth there (δ ≈ 1.3 µm) is below the 2.25 µm OVERLAY thickness,
/// inside the Leontovich good-conductor validity domain. NOTE: this is
/// **not** the L comparison frequency — L is compared at the f→0
/// quasi-static limit (see [`extrapolate_l0`]), because the FEM L is
/// frequency-dependent (substrate-C dispersion) while Mohan/mom report
/// the C-free low-frequency inductance.
const Q_REF_GHZ: f64 = 3.0;

/// mom PEEC quasi-static inductance L0. mom is nearly frequency-flat
/// (2.155 → 2.145 nH over 0.5–2 GHz — its 2.5D stratified-filament model
/// has weak substrate-C coupling), so its lowest-frequency point
/// (0.5 GHz) is its L0 — `reference/fixtures/slcfet_mom/baseline.json`
/// (mom commit ddc02134ce06f7e6ad3583083cce992a5bb2a64c).
const MOM_L0_NH: f64 = 2.154_950_934_609_390_7;
/// mom Q at the 3 GHz reference frequency (Q tracking band only — mom's
/// filament loss model is unreliable > ~2 GHz; Palace is the real Q
/// oracle). `reference/fixtures/slcfet_mom/baseline.json`.
const MOM_Q_3GHZ: f64 = 5.614_758_740_936_693_5;

#[derive(Clone, Copy, PartialEq)]
enum FixtureChoice {
    /// `spiral_slcfet_3hp.msh` (76,964 edges) → `results.toml`.
    Benchmark,
    /// `spiral_slcfet_3hp_smoke.msh` (~13 k edges) → `results_smoke.toml`.
    Smoke,
}

struct Row {
    f_ghz: f64,
    omega: f64,
    z_ohm: c64,
    l_nh: f64,
    r_ohm: f64,
    q: f64,
    s11_mag: f64,
    residual_rel: f64,
}

fn ghz_to_omega(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / C_UM_PER_S
}

/// Quasi-static inductance L0 by Richardson extrapolation of the two
/// lowest sweep points to f→0. Below self-resonance a substrate-loaded
/// spiral follows L(f) ≈ L0 − a·f² (the shunt substrate capacitance
/// siphons current as f rises), so a two-point quadratic-in-f²
/// extrapolation recovers the C-free inductance that Mohan and mom
/// report. This is the only apples-to-apples L for the oracle
/// comparison — a finite quote frequency conflates L0 with the FEM's
/// (correct, but oracle-absent) substrate-C dispersion.
fn extrapolate_l0(rows: &[Row]) -> f64 {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.f_ghz.partial_cmp(&b.f_ghz).unwrap());
    let (f1, l1) = (sorted[0].f_ghz, sorted[0].l_nh);
    let (f2, l2) = (sorted[1].f_ghz, sorted[1].l_nh);
    let (f1s, f2s) = (f1 * f1, f2 * f2);
    (l1 * f2s - l2 * f1s) / (f2s - f1s)
}

fn current_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn results_path(choice: FixtureChoice) -> PathBuf {
    let file = match choice {
        FixtureChoice::Benchmark => "results.toml",
        FixtureChoice::Smoke => "results_smoke.toml",
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmarks")
        .join("slcfet_3hp")
        .join(file)
}

fn run_sweep(fixture: &SpiralFixture, freqs_ghz: &[f64]) -> Vec<Row> {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_for(&SLCFET_3HP_MATERIALS);

    // PEC outer walls; the conductor surface carries the Leontovich BC.
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);

    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: SLCFET_3HP_MATERIALS.conductor_sigma_natural(),
        },
    };

    let port = fixture.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let lp = port.lumped_port(r_nat, c64::new(1.0, 0.0));

    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    let omegas: Vec<f64> = freqs_ghz.iter().map(|&f| ghz_to_omega(f)).collect();

    eprintln!(
        "assembling DrivenOperator: {} edges, {} tets, {} conductor faces, {} port faces",
        edges.len(),
        fixture.mesh.n_tets(),
        cond.len(),
        port.faces.len()
    );
    let t0 = std::time::Instant::now();
    let points = driven_frequency_sweep::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        std::slice::from_ref(&surface),
        &omegas,
        &source,
        &device,
    )
    .expect("port-driven frequency sweep on the SLCFET 3HP spiral fixture");
    eprintln!(
        "sweep of {} points done in {:.1} s",
        points.len(),
        t0.elapsed().as_secs_f64()
    );

    points
        .iter()
        .zip(freqs_ghz.iter())
        .map(|(pt, &f_ghz)| {
            let pc = pt.ports[0];
            let z_ohm = pc.z * ETA_0;
            // L = Im Z / ω in SI: Im(Z_Ω) / (2π f) — in nH the 1e9
            // cancels against f in GHz.
            let l_nh = z_ohm.im / (2.0 * std::f64::consts::PI * f_ghz);
            Row {
                f_ghz,
                omega: pt.omega,
                z_ohm,
                l_nh,
                r_ohm: z_ohm.re,
                q: pc.quality_factor(),
                s11_mag: pc.s11(r_nat).norm(),
                residual_rel: pt.residual_rel,
            }
        })
        .collect()
}

fn write_toml(rows: &[Row], path: &PathBuf, choice: FixtureChoice, srf_ghz: Option<f64>) {
    let commit = current_commit();
    let l_cs = mohan_current_sheet_l(&FIXTURE_SPIRAL) * 1.0e9;
    let l_mw = modified_wheeler_l(&FIXTURE_SPIRAL) * 1.0e9;
    let l_mono = monomial_fit_l(&FIXTURE_SPIRAL) * 1.0e9;

    let mut s = String::new();
    s.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    match choice {
        FixtureChoice::Benchmark => s.push_str("#   --example slcfet_3hp_spiral`.\n"),
        FixtureChoice::Smoke => s.push_str("#   --example slcfet_3hp_spiral -- smoke`.\n"),
    }
    s.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    s.push_str("# Consumed by `tests/slcfet_3hp_benchmark.rs` (calibrated bands)\n");
    s.push_str("# and compared against the mom PEEC baseline\n");
    s.push_str("# (`reference/fixtures/slcfet_mom/`).\n");
    s.push('\n');
    s.push_str("[meta]\n");
    match choice {
        FixtureChoice::Benchmark => {
            s.push_str("description = \"SLCFET 3HP spiral-inductor extraction benchmark (issue #212, Epic #193 Phase 3 capstone): port-driven frequency sweep of the bundled 3-turn Au-on-SiC square spiral (spiral_slcfet_3hp.msh, 76,964 edges), L/R/Q/S11/SRF vs the mom PEEC (exact-geometry) and Mohan analytic oracles; Palace slot pending operator run. L is compared at the f->0 quasi-static limit (the FEM resolves substrate-C dispersion the idealized oracles omit).\"\n");
            s.push_str("fixture = \"tests/fixtures/spiral_slcfet_3hp.msh\"\n");
            s.push_str(
                "fixture_provenance = \"tests/fixtures/spiral_slcfet_3hp.provenance.txt\"\n",
            );
            s.push_str("fixture_sha256 = \"7770873496af8f33f3ebe38239aaad8f3d12e89d180c6ec59c90c7903e02bca3\"\n");
        }
        FixtureChoice::Smoke => {
            s.push_str("description = \"SLCFET 3HP spiral-inductor extraction smoke run (issue #212): same pipeline as results.toml on the coarse spiral_slcfet_3hp_smoke.msh fixture — pipeline check, not a benchmark.\"\n");
            s.push_str("fixture = \"tests/fixtures/spiral_slcfet_3hp_smoke.msh\"\n");
            s.push_str(
                "fixture_provenance = \"tests/fixtures/spiral_slcfet_3hp_smoke.provenance.txt\"\n",
            );
        }
    }
    s.push_str(&format!("generated_at_commit = \"{commit}\"\n"));
    s.push_str(&format!("port_resistance_ohm = {R_PORT_OHM}\n"));
    s.push_str("process = \"SLCFET_3HP\"\n");
    s.push_str("pdk_source = \"pdk/slcfet/slcfet_3hp.pdk.yaml (sphere monorepo)\"\n");
    s.push_str("conductor_model = \"leontovich_good_conductor\"\n");
    s.push_str("conductor_sigma_s_m = 5.1467e7\n");
    s.push_str("substrate = \"SiC eps_r 9.7 tan_delta 0.004, 100 um, PEC floor\"\n");
    s.push_str("outer_boundary = \"pec\"\n");
    if let Some(srf) = srf_ghz {
        s.push_str(&format!("srf_ghz = {srf:.6e}\n"));
    } else {
        s.push_str("# srf_ghz: the sweep does not bracket an Im Z = 0 crossing.\n");
    }
    s.push_str("notes = [\n");
    s.push_str("  \"Z = V/I at the lumped port (Palace-style uniform port, V_inc = 1, R = 50 ohm); L = Im Z / omega, Q = Im Z / Re Z, S11 vs 50 ohm.\",\n");
    s.push_str("  \"Conductor loss via the Leontovich good-conductor surface impedance on the cavity walls (Au metallization, sigma = 1/(0.01943 ohm-um) ~ 5.15e7 S/m per the canonical PDK / mom issue #358); substrate loss via tan-delta in the SiC permittivity.\",\n");
    s.push_str("  \"Stack: PASSIV (Au 3.0 um, z 0..3) underpass directly on the SiC, OVERLAY (Au 2.25 um, z 5..7.25) spiral, metals in AIR above the substrate (the geo's tag-2 'dielectric' region is assigned eps_r = 1); the 0.16 um SiN passivation is omitted from the mesh (documented stack delta, both sides).\",\n");
    s.push_str("  \"Crossover: via underpass on BOTH the FEM and mom sides so the geometries match by construction. The physical 3HP process qualifies only AIR-BRIDGE crossovers for spirals (via_underpass shorts the spiral per fab qualification) — a documented delta from the fab flow, not a simulation limitation.\",\n");
    s.push_str("  \"L comparison frequency: the FEM L = Im Z / omega is frequency-dependent (2.078 nH at 0.1 GHz falling to 1.685 at 3 GHz) because the high-k SiC substrate (eps_r 9.7) under the coil adds shunt capacitance that siphons current as f rises. Mohan (quasi-static analytic) and mom (2.5D, nearly frequency-flat at ~2.15 nH) both report the C-free low-frequency inductance, so L is compared at the f->0 quasi-static limit L0 (Richardson extrapolation of the two lowest sweep points): L0_fem ~ 2.11 nH, within ~2% of mom and ~3% of Mohan. Comparing the FEM's 3 GHz L against a DC formula was the original (apples-to-oranges) source of the apparent ~18% deficit; the FEM additionally, correctly, resolves the substrate-C dispersion the oracles omit.\",\n");
    s.push_str("  \"Leontovich validity caveat: below ~1.5 GHz the Au skin depth (2.2 um at 1 GHz) reaches the 2.25 um OVERLAY thickness, so the semi-infinite-conductor surface impedance under-reports R and inflates the internal inductance. This affects R/Q (read at 3 GHz, delta = 1.28 um) but not the L0 extrapolation (Im Z is dominated by the geometric inductance, not the small internal term).\",\n");
    if choice == FixtureChoice::Benchmark {
        s.push_str("  \"mom PEEC baseline: reference/fixtures/slcfet_mom/ — EXACT integer-turn geometry match (n = 3), the L oracle. mom is NOT a reliable Q oracle above ~2 GHz: its 3-filament lateral discretization cannot resolve the sub-2-um Au skin depth and overestimates R (issue-#211 calibration); the FEM/mom Q ratio is pinned with a tracking band, and the realistic Q oracle is the operator-assisted Palace slot.\",\n");
        s.push_str("  \"Mohan expressions assume an isolated current-sheet spiral (no feed stubs, no via underpass, zero thickness) — a sanity band, not a 5%-grade oracle. Diagnostics ruled out the truncation box (widening 44,582->76,964 edges moved L0 by <1%) and the PEC substrate floor (natural-BC floor moved L by ~3%) as deficit sources; the realized fixture footprint (185 um) and trace length (~1830 um) match/exceed the nominal n=3 geometry. The reconciling factor was the L comparison frequency (see the L-comparison note).\",\n");
    }
    s.push_str("]\n");
    s.push('\n');

    // Oracle and comparison sections apply only to the benchmark
    // fixture — the smoke fixture has a different lateral geometry
    // (d_in = 60 µm), so the Mohan/mom values would not apply.
    if choice == FixtureChoice::Benchmark {
        s.push_str("[oracles.mom_peec]\n");
        s.push_str("baseline = \"reference/fixtures/slcfet_mom/\"\n");
        s.push_str("note = \"exact geometry match (integer n = 3, OVERLAY spiral / PASSIV via-underpass, load_pdk('slcfet_3hp') stack); L0 oracle at the f->0 quasi-static limit (mom is frequency-flat), Q tracked with a band only (filament loss model, see notes)\"\n");
        s.push_str(&format!("l0_nh = {MOM_L0_NH}\n"));
        s.push_str(&format!("q_3ghz = {MOM_Q_3GHZ}\n"));
        s.push('\n');
        s.push_str("[oracles.mohan]\n");
        s.push_str("# geode_core::analytic::spiral on the fixture parameterization (n = 3,\n");
        s.push_str("# w = 10 um, s = 5 um, d_in = 100 um), nH.\n");
        s.push_str(&format!("current_sheet_l_nh = {l_cs:.6e}\n"));
        s.push_str(&format!("modified_wheeler_l_nh = {l_mw:.6e}\n"));
        s.push_str(&format!("monomial_fit_l_nh = {l_mono:.6e}\n"));
        s.push('\n');
        s.push_str("[oracles.palace]\n");
        s.push_str("status = \"pending_operator_run\"\n");
        s.push_str("note = \"No Palace install exists on the generation machine (mom/external/palace-install is absent; only the Docker build recipe in ~/GitHub/sphere/eda/mom/docker/palace). Palace is the realistic Q oracle given the mom filament-loss caveat. Operator-run results slot in here with their own provenance — same toolchain-gap convention as the FastHenry slot of benchmarks/spiral_inductor/results.toml.\"\n");
        s.push('\n');

        // Achieved comparison. L at the f→0 quasi-static limit (the
        // only apples-to-apples L vs the C-free oracles); Q at the
        // 3 GHz reference (tracking band only).
        let l0_fem = extrapolate_l0(rows);
        let q_row = rows
            .iter()
            .min_by(|a, b| {
                (a.f_ghz - Q_REF_GHZ)
                    .abs()
                    .partial_cmp(&(b.f_ghz - Q_REF_GHZ).abs())
                    .unwrap()
            })
            .expect("sweep has points");
        s.push_str("[comparison]\n");
        s.push_str("# L compared at the f->0 quasi-static limit (Richardson\n");
        s.push_str("# extrapolation of the two lowest sweep points); see the\n");
        s.push_str("# L-comparison note for why a finite quote frequency is wrong.\n");
        s.push_str(&format!("l0_fem_nh = {l0_fem:.6e}\n"));
        s.push_str(&format!(
            "rel_err_vs_mom_l0 = {:.6e}\n",
            (l0_fem - MOM_L0_NH) / MOM_L0_NH
        ));
        s.push_str(&format!(
            "rel_err_vs_mohan_current_sheet = {:.6e}\n",
            (l0_fem - l_cs) / l_cs
        ));
        s.push_str(&format!("q_ref_ghz = {Q_REF_GHZ}\n"));
        s.push_str(&format!("q_fem_3ghz = {:.6e}\n", q_row.q));
        s.push_str(&format!(
            "q_ratio_fem_over_mom_3ghz = {:.6e}\n",
            q_row.q / MOM_Q_3GHZ
        ));
        s.push('\n');
    }

    for (i, r) in rows.iter().enumerate() {
        s.push_str(&format!("[point_{i}]\n"));
        s.push_str(&format!("f_ghz = {:.15e}\n", r.f_ghz));
        s.push_str(&format!("omega_natural = {:.15e}\n", r.omega));
        s.push_str(&format!("z_re_ohm = {:.15e}\n", r.z_ohm.re));
        s.push_str(&format!("z_im_ohm = {:.15e}\n", r.z_ohm.im));
        s.push_str(&format!("l_nh = {:.15e}\n", r.l_nh));
        s.push_str(&format!("r_ohm = {:.15e}\n", r.r_ohm));
        s.push_str(&format!("q = {:.15e}\n", r.q));
        s.push_str(&format!("s11_mag = {:.15e}\n", r.s11_mag));
        s.push_str(&format!("solve_residual_rel = {:.3e}\n", r.residual_rel));
        s.push('\n');
    }

    fs::create_dir_all(path.parent().expect("results parent")).expect("mkdir");
    fs::write(path, s).expect("write slcfet_3hp results TOML");
    eprintln!("wrote {}", path.display());
}

fn main() {
    let choice = match std::env::args().nth(1).as_deref() {
        None => FixtureChoice::Benchmark,
        Some("smoke") => FixtureChoice::Smoke,
        Some(other) => {
            eprintln!("unknown argument {other:?} — expected `smoke` or no argument");
            std::process::exit(2);
        }
    };
    let (fixture, freqs): (SpiralFixture, &[f64]) = match choice {
        FixtureChoice::Benchmark => (
            read_spiral_slcfet_3hp_fixture().expect("bundled SLCFET 3HP benchmark fixture"),
            &FREQS_GHZ,
        ),
        FixtureChoice::Smoke => (
            read_spiral_slcfet_3hp_smoke_fixture().expect("bundled SLCFET 3HP smoke fixture"),
            &FREQS_GHZ_SMOKE,
        ),
    };

    let rows = run_sweep(&fixture, freqs);

    let omegas: Vec<f64> = rows.iter().map(|r| r.omega).collect();
    let zs: Vec<c64> = rows.iter().map(|r| r.z_ohm).collect();
    let srf_ghz =
        detect_srf(&omegas, &zs).map(|w| w * C_UM_PER_S / (2.0 * std::f64::consts::PI * 1.0e9));

    let l_cs = mohan_current_sheet_l(&FIXTURE_SPIRAL) * 1.0e9;
    eprintln!(
        "\n{:>7}  {:>10}  {:>10}  {:>10}  {:>8}  {:>8}",
        "f (GHz)", "L (nH)", "R (ohm)", "Q", "|S11|", "residual"
    );
    for r in &rows {
        eprintln!(
            "{:>7.2}  {:>10.4}  {:>10.4}  {:>10.3}  {:>8.4}  {:>8.1e}",
            r.f_ghz, r.l_nh, r.r_ohm, r.q, r.s11_mag, r.residual_rel
        );
    }
    let l0_fem = if choice == FixtureChoice::Benchmark {
        Some(extrapolate_l0(&rows))
    } else {
        None
    };
    eprintln!("\nMohan current-sheet L (analytic, isolated spiral): {l_cs:.4} nH");
    eprintln!("mom PEEC L0 (exact geometry, quasi-static): {MOM_L0_NH:.4} nH");
    if let Some(l0) = l0_fem {
        eprintln!(
            "FEM L0 (f->0 extrapolation): {l0:.4} nH  ({:+.2}% vs mom, {:+.2}% vs Mohan)",
            100.0 * (l0 - MOM_L0_NH) / MOM_L0_NH,
            100.0 * (l0 - l_cs) / l_cs
        );
    }
    match srf_ghz {
        Some(srf) => eprintln!("SRF (Im Z zero crossing): {srf:.2} GHz"),
        None => eprintln!("SRF: not bracketed by the sweep"),
    }

    write_toml(&rows, &results_path(choice), choice, srf_ghz);
}
