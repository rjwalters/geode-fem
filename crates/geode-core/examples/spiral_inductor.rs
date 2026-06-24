//! Spiral-inductor extraction benchmark: L(f) / R(f) / Q(f) / S₁₁ / SRF
//! over a port-driven frequency sweep of the bundled 3.5-turn generic
//! square spiral fixture (issue #211, Epic #193 Phase 3).
//!
//! Drives the `spiral_3p5.msh` fixture (54,428 edges; see
//! `geode_core::mesh::spiral` for the stack and physical groups)
//! through its lumped port with [`geode_core::driven_frequency_sweep`]:
//! the ω-independent operator assembles **once** (sparse `[nnz]` path,
//! issue #218), then per frequency only the complex recombination +
//! sparse LU + port readback run. Setup mirrors the issue-#218
//! benchmark-fixture regression:
//!
//! - lossy substrate/dielectric permittivities from the recorded stack
//!   materials (`SpiralFixture::epsilon_r_default`);
//! - PEC outer walls (edge-exact mask);
//! - the conductor cavity walls carry the Leontovich good-conductor
//!   surface impedance (copper, issue #207) — the conductor-loss term
//!   behind Q;
//! - 50 Ω lumped port driven with `V_inc = 1`.
//!
//! Per frequency the port impedance `Z = V/I` reduces to the circuit
//! quantities `L = Im Z/ω`, `R = Re Z`, `Q = Im Z/Re Z`, `S₁₁` vs 50 Ω
//! (`geode_core::extraction`), and the sweep is scanned for the
//! self-resonant `Im Z = 0` crossing (`detect_srf`).
//!
//! # Oracles (recorded in the output TOML)
//!
//! - **Mohan analytic** (`geode_core::mohan`, in-repo): the three
//!   closed-form square-spiral expressions on the fixture
//!   parameterization — a ±5–10 % low-frequency sanity band (no ground
//!   plane, no feed stubs).
//! - **mom PEEC sidecar** (`reference/fixtures/spiral_mom/`): the
//!   sister-repo MoM/PEEC extractor on the matching generic stack;
//!   integer-turn brackets n = 3 and n = 4 around the fixture's 3.5
//!   turns (mom's spiral generator takes integer turns — geometry delta
//!   documented in the baseline provenance).
//! - **FastHenry**: not installed on the generation machine — slot
//!   recorded as deferred; see the `[oracles.fasthenry]` table.
//!
//! Writes `benchmarks/spiral_inductor/results.toml` (the spiral sibling
//! of `benchmarks/mie_sphere/driven_results.toml`).
//!
//! Run with:
//!
//! ```sh
//! cargo run -p geode-core --release --example spiral_inductor
//! ```
//!
//! Passing `smoke` selects the coarse `spiral_3p5_smoke.msh` fixture
//! (~15 k edges) and writes `results_smoke.toml` instead — a fast
//! end-to-end check of the same pipeline:
//!
//! ```sh
//! cargo run -p geode-core --release --example spiral_inductor -- smoke
//! ```
//!
//! # Field export (Epic #276 Phase 2B, issue #287)
//!
//! Passing `--export-field <path.vtu>` is an opt-in side channel that
//! does **not** touch the extraction sweep above (the `results.toml` is
//! byte-identical with or without it). When present, the benchmark
//! fixture is solved once at the **low-frequency reference operating
//! point** already used for the oracle comparison
//! (`L_REF_GHZ = 1.0` GHz) and the driven near field `E(r)` is dumped to
//! `<path.vtu>` for ParaView inspection:
//!
//! ```sh
//! cargo run -p geode-core --release --example spiral_inductor -- --export-field artifacts/viz/E_spiral.vtu
//! ```
//!
//! The exported per-node `E` is the crude per-tet-vertex average of the
//! Whitney interpolant (see `examples/viz_export_helper.rs`). No
//! per-node `eps_r` is exported for the spiral (the stack uses a
//! per-region scalar permittivity that is not as cleanly nodal as the
//! sphere/patch case) — `None` is passed.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use faer::c64;

use geode_core::mesh::spiral::CONDUCTOR_SIGMA_NATURAL;
use geode_core::{
    CurrentSource, DefaultBackend, DrivenBcs, DrivenMaterials, SpiralFixture, SquareSpiral,
    SurfaceImpedanceBc, SurfaceImpedanceModel, detect_srf, driven_frequency_sweep,
    driven_solve_with_ports, modified_wheeler_l, mohan_current_sheet_l, monomial_fit_l,
    pec_interior_mask_from_triangles, read_spiral_fixture, read_spiral_smoke_fixture,
};

#[path = "common/viz_export_helper.rs"]
mod viz_export_helper;

/// Free-space impedance η₀ (Ω) — the solver's natural impedance unit.
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in µm/s — the fixture length unit is the micron, so
/// `ω_natural = 2π f / C_UM_PER_S` (rad/µm).
const C_UM_PER_S: f64 = 2.997_924_58e14;

/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;

/// Fixture spiral parameterization
/// (`tests/fixtures/spiral_3p5.provenance.txt`), meters.
const FIXTURE_SPIRAL: SquareSpiral = SquareSpiral {
    n_turns: 3.5,
    width: 6.0e-6,
    spacing: 4.0e-6,
    d_in: 60.0e-6,
};

/// Benchmark sweep (GHz): log-ish spacing from the L_DC plateau up
/// through the expected self-resonance region (the mom PEEC oracle and
/// the quasi-static parallel-plate estimate both place the SRF in the
/// tens of GHz for this geometry).
const FREQS_GHZ: [f64; 13] = [
    0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 15.0, 20.0, 30.0, 40.0,
];

/// Smoke sweep (GHz): same pipeline, four points on the coarse fixture.
const FREQS_GHZ_SMOKE: [f64; 4] = [1.0, 5.0, 10.0, 20.0];

/// Reference frequency for the low-frequency L comparison: low enough
/// to sit on the L(f) plateau, high enough that the copper skin depth
/// (~2.1 µm at 1 GHz) is below the 3 µm trace thickness, inside the
/// Leontovich good-conductor model's validity domain (below ~0.5 GHz
/// the semi-infinite-conductor surface impedance overestimates the
/// internal inductance and underestimates R — visible as the L upturn
/// and the sub-DC resistance of the 0.1–0.5 GHz points).
const L_REF_GHZ: f64 = 1.0;

/// mom PEEC low-frequency L (nH, at 0.1 GHz) for the integer-turn
/// brackets n = 3 / n = 4 around the fixture's 3.5 turns —
/// `reference/fixtures/spiral_mom/baseline.json` (mom commit
/// ddc02134ce06f7e6ad3583083cce992a5bb2a64c; see the provenance file).
const MOM_L_NH_N3: f64 = 1.2778;
/// See [`MOM_L_NH_N3`].
const MOM_L_NH_N4: f64 = 2.2055;

#[derive(Clone, Copy, PartialEq)]
enum FixtureChoice {
    /// `spiral_3p5.msh` (54,428 edges) → `results.toml`.
    Benchmark,
    /// `spiral_3p5_smoke.msh` (~15 k edges) → `results_smoke.toml`.
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
        .join("spiral_inductor")
        .join(file)
}

fn run_sweep(fixture: &SpiralFixture, freqs_ghz: &[f64]) -> Vec<Row> {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_default();

    // PEC outer walls; the conductor surface carries the Leontovich BC.
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);

    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: CONDUCTOR_SIGMA_NATURAL,
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
    .expect("port-driven frequency sweep on the spiral fixture");
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
        FixtureChoice::Benchmark => s.push_str("#   --example spiral_inductor`.\n"),
        FixtureChoice::Smoke => s.push_str("#   --example spiral_inductor -- smoke`.\n"),
    }
    s.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    s.push_str("# Consumed by `tests/spiral_inductor_benchmark.rs` (calibrated bands)\n");
    s.push_str("# and compared against the mom PEEC baseline\n");
    s.push_str("# (`reference/fixtures/spiral_mom/`).\n");
    s.push('\n');
    s.push_str("[meta]\n");
    match choice {
        FixtureChoice::Benchmark => {
            s.push_str("description = \"Spiral-inductor extraction benchmark (issue #211, Epic #193 Phase 3): port-driven frequency sweep of the bundled 3.5-turn generic square spiral (spiral_3p5.msh, 54,428 edges), L/R/Q/S11/SRF vs the Mohan analytic and mom PEEC oracles.\"\n");
            s.push_str("fixture = \"tests/fixtures/spiral_3p5.msh\"\n");
            s.push_str("fixture_provenance = \"tests/fixtures/spiral_3p5.provenance.txt\"\n");
            s.push_str("fixture_sha256 = \"c9707fb9bd5f3f484b845e96b90cf53f0b196794c5793f7e4f682bf97e101589\"\n");
        }
        FixtureChoice::Smoke => {
            s.push_str("description = \"Spiral-inductor extraction smoke run (issue #211): same pipeline as results.toml on the coarse spiral_3p5_smoke.msh fixture — pipeline check, not a benchmark.\"\n");
            s.push_str("fixture = \"tests/fixtures/spiral_3p5_smoke.msh\"\n");
            s.push_str("fixture_provenance = \"tests/fixtures/spiral_3p5_smoke.provenance.txt\"\n");
        }
    }
    s.push_str(&format!("generated_at_commit = \"{commit}\"\n"));
    s.push_str(&format!("port_resistance_ohm = {R_PORT_OHM}\n"));
    s.push_str("conductor_model = \"leontovich_good_conductor\"\n");
    s.push_str("conductor_sigma_s_m = 5.8e7\n");
    s.push_str("outer_boundary = \"pec\"\n");
    if let Some(srf) = srf_ghz {
        s.push_str(&format!("srf_ghz = {srf:.6e}\n"));
    } else {
        s.push_str("# srf_ghz: the sweep does not bracket an Im Z = 0 crossing.\n");
    }
    s.push_str("notes = [\n");
    s.push_str("  \"Z = V/I at the lumped port (Palace-style uniform port, V_inc = 1, R = 50 ohm); L = Im Z / omega, Q = Im Z / Re Z, S11 vs 50 ohm.\",\n");
    s.push_str("  \"Conductor loss via the Leontovich good-conductor surface impedance on the cavity walls (copper, sigma = 5.8e7 S/m); substrate/oxide loss via tan-delta in the recorded permittivities.\",\n");
    s.push_str("  \"Leontovich validity caveat: below ~0.5 GHz the copper skin depth (6.6 um at 0.1 GHz) exceeds the 3 um trace thickness, so the semi-infinite-conductor surface impedance underestimates R (below the ~1.2 ohm DC resistance) and overestimates the internal inductance (the L upturn of the lowest points). The low-frequency L reference for the oracle comparison is the 1 GHz point.\",\n");
    if choice == FixtureChoice::Benchmark {
        s.push_str("  \"Mohan expressions assume an isolated spiral: no PEC box (the fixture's PEC walls reduce L via image currents and add shunt C) and no feed stubs/underpass — a sanity band, not a 5%-grade oracle.\",\n");
        s.push_str("  \"mom PEEC baseline: reference/fixtures/spiral_mom/ (integer-turn brackets n = 3 / n = 4 around the 3.5-turn fixture; matching generic stack). The FEM SRF (~21 GHz, a parallel anti-resonance: Im Z crosses zero through the |Z| peak visible at the 30 GHz point) has no mom counterpart below 40 GHz — the PEC side walls (45 um margin) and full 3D capacitance of the FEM domain are absent from mom's laterally open model.\",\n");
        s.push_str("  \"Q model spread: mid-band FEM Q exceeds the mom-bracket Q by ~3-5x. mom's 3-filament lateral discretization (2 um filaments) cannot resolve the sub-um skin depth above ~2 GHz and overestimates R there, while the FEM Leontovich surface underestimates R below ~1 GHz; the epic's factor-2 Q target is not met by this oracle pair — documented, see the PR for issue #211.\",\n");
    }
    s.push_str("]\n");
    s.push('\n');

    // Oracle and comparison sections apply only to the benchmark
    // fixture — the smoke fixture has a different lateral geometry
    // (d_in = 40 µm), so the Mohan/mom values would not apply.
    if choice == FixtureChoice::Benchmark {
        s.push_str("[oracles.mohan]\n");
        s.push_str("# geode_core::mohan on the fixture parameterization (n = 3.5,\n");
        s.push_str("# w = 6 um, s = 4 um, d_in = 60 um), nH.\n");
        s.push_str(&format!("current_sheet_l_nh = {l_cs:.6e}\n"));
        s.push_str(&format!("modified_wheeler_l_nh = {l_mw:.6e}\n"));
        s.push_str(&format!("monomial_fit_l_nh = {l_mono:.6e}\n"));
        s.push('\n');
        s.push_str("[oracles.mom_peec]\n");
        s.push_str("baseline = \"reference/fixtures/spiral_mom/\"\n");
        s.push_str("note = \"integer-turn brackets n = 3 and n = 4 (mom-geom SpiralParams::n_turns is u32); see the baseline provenance for the geometry delta\"\n");
        s.push_str(&format!("l_nh_n3 = {MOM_L_NH_N3}\n"));
        s.push_str(&format!("l_nh_n4 = {MOM_L_NH_N4}\n"));
        s.push('\n');

        s.push_str("[oracles.fasthenry]\n");
        s.push_str("status = \"deferred\"\n");
        s.push_str("note = \"FastHenry is not installed on the generation machine (toolchain gap, same convention as the Julia sidecars). Operator-supplied values can be recorded here with their own provenance.\"\n");
        s.push('\n');
        // Achieved low-frequency L comparison at the reference point.
        let ref_row = rows
            .iter()
            .min_by(|a, b| {
                (a.f_ghz - L_REF_GHZ)
                    .abs()
                    .partial_cmp(&(b.f_ghz - L_REF_GHZ).abs())
                    .unwrap()
            })
            .expect("sweep has points");
        let l_fem = ref_row.l_nh;
        // Project the integer-turn mom brackets onto n = 3.5 with the
        // Mohan current-sheet turn-count ratio.
        let mohan_n = |n: f64| {
            let mut sp = FIXTURE_SPIRAL;
            sp.n_turns = n;
            mohan_current_sheet_l(&sp) * 1.0e9
        };
        let proj_n3 = MOM_L_NH_N3 * l_cs / mohan_n(3.0);
        let proj_n4 = MOM_L_NH_N4 * l_cs / mohan_n(4.0);
        let proj_mean = 0.5 * (proj_n3 + proj_n4);
        s.push_str("[comparison]\n");
        s.push_str("# Achieved low-frequency L figures at the reference point.\n");
        s.push_str(&format!("l_ref_ghz = {L_REF_GHZ}\n"));
        s.push_str(&format!("l_fem_nh = {l_fem:.6e}\n"));
        s.push_str(&format!(
            "rel_err_vs_mohan_current_sheet = {:.6e}\n",
            (l_fem - l_cs) / l_cs
        ));
        s.push_str(&format!(
            "inside_mom_bracket = {}\n",
            l_fem > MOM_L_NH_N3 && l_fem < MOM_L_NH_N4
        ));
        s.push_str("# mom brackets projected to n = 3.5 via the Mohan turn-count ratio:\n");
        s.push_str(&format!("mom_projected_n3_nh = {proj_n3:.6e}\n"));
        s.push_str(&format!("mom_projected_n4_nh = {proj_n4:.6e}\n"));
        s.push_str(&format!(
            "rel_err_vs_mom_projected_mean = {:.6e}\n",
            (l_fem - proj_mean) / proj_mean
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
    fs::write(path, s).expect("write spiral_inductor results TOML");
    eprintln!("wrote {}", path.display());
}

/// Opt-in `--export-field` (Epic #276 Phase 2B, issue #287): solve the
/// benchmark fixture once at the low-frequency reference operating point
/// (`L_REF_GHZ`) and dump the driven near field to `<path>` as `.vtu`.
///
/// Independent of the extraction sweep — does not write `results.toml`.
fn export_field(path: &str) {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let fixture =
        read_spiral_fixture().expect("bundled benchmark spiral fixture for --export-field");
    let omega = ghz_to_omega(L_REF_GHZ);
    eprintln!(
        "=== --export-field: driven solve at {L_REF_GHZ} GHz (omega = {omega:.6e} rad/um) ==="
    );

    let edges = fixture.mesh.edges();
    let eps = fixture.epsilon_r_default();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);
    let port = fixture.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let lp = port.lumped_port(r_nat, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    // The extraction sweep adds a Leontovich conductor-surface-impedance
    // loss term; the public single-frequency `driven_solve_with_ports`
    // does not take surface BCs, so the visualisation field is the
    // PEC-walls + scalar-eps near field (a debugging visual, not the
    // loss-loaded extraction operator — see issue #287 scope).
    let sol = driven_solve_with_ports::<B>(
        &fixture.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        omega,
        &source,
        &device,
    )
    .expect("port-driven solve for --export-field");
    eprintln!(
        "  solve residual_rel = {:.3e}; reconstructing per-node E (Whitney average)",
        sol.residual_rel
    );

    let (e_re, e_im) = viz_export_helper::edge_field_to_nodes(&fixture.mesh, &sol.e_edges);

    let out = std::path::Path::new(path);
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).expect("create --export-field parent dir");
    }
    geode_core::viz_vtu::write_vtu(out, &fixture.mesh, &e_re, Some(&e_im), None)
        .expect("write --export-field .vtu");
    eprintln!(
        "  wrote {} ({} nodes, {} tets)",
        out.display(),
        fixture.mesh.n_nodes(),
        fixture.mesh.n_tets()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Opt-in field export (issue #287). Short-circuits the extraction
    // sweep so a normal run is byte-identical.
    if let Some(path) = viz_export_helper::parse_export_field(&args) {
        export_field(&path);
        return;
    }

    let choice = match args.get(1).map(String::as_str) {
        None => FixtureChoice::Benchmark,
        Some("smoke") => FixtureChoice::Smoke,
        Some(other) => {
            eprintln!(
                "unknown argument {other:?} — expected `smoke`, `--export-field <path>`, or no argument"
            );
            std::process::exit(2);
        }
    };
    let (fixture, freqs): (SpiralFixture, &[f64]) = match choice {
        FixtureChoice::Benchmark => (
            read_spiral_fixture().expect("bundled benchmark spiral fixture"),
            &FREQS_GHZ,
        ),
        FixtureChoice::Smoke => (
            read_spiral_smoke_fixture().expect("bundled smoke spiral fixture"),
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
    eprintln!("\nMohan current-sheet L (analytic, isolated spiral): {l_cs:.4} nH");
    match srf_ghz {
        Some(srf) => eprintln!("SRF (Im Z zero crossing): {srf:.2} GHz"),
        None => eprintln!("SRF: not bracketed by the sweep"),
    }

    write_toml(&rows, &results_path(choice), choice, srf_ghz);
}
