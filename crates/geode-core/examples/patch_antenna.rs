//! Patch-antenna S₁₁ / resonance / bandwidth / efficiency benchmark:
//! drives the bundled probe-fed FR-4 rectangular microstrip patch
//! (`geode_core::mesh::patch`, Epic #226) through a frequency sweep and
//! extracts the antenna figures of merit against the in-repo cavity-model
//! analytic oracle (`geode_core::patch_cavity`, issue #228 Phase 2).
//!
//! For each frequency the structure is solved as an **open radiator**:
//! a 50 Ω lumped port across the coax-probe gap, PEC patch + ground +
//! outer walls, and a **matched (box) UPML** absorbing shell
//! (`DrivenMaterials::MatchedUpml` built per-ω from
//! `PatchFixture::matched_upml_materials`, since the box-UPML stretch is
//! ω-dependent). From the port readback we get:
//!
//! ```text
//! Z_in(ω) = V / I,  I = (2 V_inc − V)/R
//! S₁₁(ω)  = (Z − Z₀)/(Z + Z₀)   vs Z₀ = 50 Ω
//! P_in    = ½ Re(V · I*)         (net power delivered into the antenna)
//! P_rad   = ∮ ½ Re(E×H*)·n̂ dS    (`flux_power_box` over a surface just
//!                                  inside the UPML, enclosing the patch)
//! η_rad   = P_rad / P_in         (radiation efficiency)
//! ```
//!
//! and over the sweep: `f_res` (the `Im Z = 0` crossing / S₁₁ dip),
//! the −10 dB |S₁₁| bandwidth, and `Z_in` at resonance.
//!
//! # Oracle
//!
//! `geode_core::patch_cavity` (Balanis cavity model): ε_eff, ΔL, the
//! TM₀₁₀ resonant frequency `f_res = c/(2 L_eff √ε_eff)`, the two-slot
//! edge/inset input resistance, and a loss-limited fractional bandwidth.
//! For FR-4 (ε_r = 4.4, tan δ = 0.02) the fixture cavity model places
//! f_res ≈ 2.435 GHz, so the sweep brackets 2.0–3.0 GHz to *find* the
//! S₁₁ dip (lesson from issue #212: locate the dip, do not extrapolate).
//!
//! Writes `benchmarks/patch_antenna/results.toml`. Run with:
//!
//! ```sh
//! cargo run -p geode-core --release --example patch_antenna
//! ```
//!
//! Passing `smoke` selects the coarse `patch_2g4_smoke.msh` fixture and
//! writes `results_smoke.toml` — a fast end-to-end check of the same
//! pipeline:
//!
//! ```sh
//! cargo run -p geode-core --release --example patch_antenna -- smoke
//! ```
//!
//! Passing `pattern` runs the near-to-far-field transform
//! (`geode_core::ntff`, issue #229 Phase 3) on the driven near field at
//! the Phase-2 resonant frequency and writes
//! `benchmarks/patch_antenna/pattern.toml` — broadside directivity,
//! gain `G = D·η`, and E-/H-plane principal-plane cuts cross-checked
//! against the Balanis cavity-model two-slot directivity oracle.
//! `pattern-smoke` does the same on the coarse fixture (pipeline check).
//! `pattern-matched` (issue #247) runs the NTFF on the impedance-matched
//! `patch_2g4_matched.msh` fixture (issue #237, probe inset 8.0 → 7.0 mm)
//! at the matched S11-dip frequency and writes
//! `benchmarks/patch_antenna/pattern_matched.toml` — `G = D·η` with the
//! matched-port radiation efficiency from the tuned fixture. D is
//! essentially unchanged from the untuned `pattern.toml` (tuning the
//! probe shifts the *match*, not the radiation pattern shape); the
//! reported gain is the physically meaningful number for the tuned
//! antenna.
//!
//! `pattern-3d` (issue #289, Epic #276 Phase 3A) runs the same driven
//! solve + NTFF as `pattern` on the benchmark fixture, samples the
//! directivity `D(θ, φ)` on a 37×72 sphere, and writes a triangulated
//! 3D radiation-*lobe* surface `.vtu` (`geode_core::viz_vtu::write_vtu_surface`,
//! `VTK_TRIANGLE` cells) whose vertex radius is the dB-floored normalised
//! directivity (floor −20 dB) and which carries `D` / `D_dB` as `PointData`
//! for ParaView colouring. Output path defaults to `artifacts/viz/patch_lobe.vtu`
//! (gitignored) or is set with `--out <path>`.
//!
//! ```sh
//! cargo run -p geode-core --release --example patch_antenna -- pattern
//! cargo run -p geode-core --release --example patch_antenna -- pattern-matched
//! cargo run -p geode-core --release --example patch_antenna -- pattern-3d --out /tmp/lobe.vtu
//! ```

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use faer::c64;

use geode_core::mesh::patch::FR4_MATERIALS;
use geode_core::viz_vtu::write_vtu_surface;
use geode_core::{
    broadside_directivity, directivity, driven_solve_with_ports, flux_power_box, gain,
    im_z_zero_crossings, ntff_far_field, pec_interior_mask_from_triangles, port_current,
    port_voltage, principal_plane_cuts, read_patch_fixture, read_patch_matched_fixture,
    read_patch_smoke_fixture, s11, to_db, CurrentSource, DefaultBackend, DrivenBcs,
    DrivenMaterials, PatchCavity, PatchFixture,
};

/// Free-space impedance η₀ (Ω) — the solver's natural impedance unit.
const ETA_0: f64 = 376.730_313_668;

/// Speed of light in mm/s — the fixture length unit is the millimeter,
/// so `ω_natural ≡ k₀ = 2π f / C_MM_PER_S` (rad/mm).
const C_MM_PER_S: f64 = 2.997_924_58e11;

/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;

/// Box-UPML strength (quadratic σ ramp) — same family/value as the
/// matched Mie shell (`tests/mie_driven_scattering.rs`, σ₀ = 25).
const SIGMA_0: f64 = 25.0;

/// UPML shell thickness (mm), from the benchmark fixture provenance
/// (`pml_thick = 25`). The smoke fixture uses a thinner shell; the
/// shell thickness is recovered from the mesh extents via
/// `PatchFixture::air_box`, so this constant only needs to be a sane
/// default per fixture (see [`pml_thick_for`]).
const PML_THICK_BENCH_MM: f64 = 25.0;
/// Smoke-fixture UPML thickness (mm) — must match
/// `reference/gmsh/patch_2g4_smoke.yaml` `pml_thick = 8.0`.
const PML_THICK_SMOKE_MM: f64 = 8.0;

/// Fraction of the air-box half-extent to shrink the flux-integration
/// surface inward from the UPML inner wall, so the box-Poynting surface
/// lies strictly in the lossless air gap (not clipping into the
/// quadratic σ ramp).
const FLUX_SHRINK: f64 = 0.10;

/// Fixture geometry for the cavity-model oracle
/// (`tests/fixtures/patch_2g4.provenance.txt`): W = 38, L = 29,
/// h = 1.6 mm FR-4.
const FIXTURE_PATCH: PatchCavity = PatchCavity {
    width: 38.0e-3,
    length: 29.0e-3,
    height: 1.6e-3,
    eps_r: 4.4,
    tan_delta: 0.02,
};

/// Benchmark sweep (GHz): 13 points spanning the cavity-model
/// f_res ≈ 2.435 GHz so the S₁₁ dip is an interior point.
const FREQS_GHZ: [f64; 13] = [
    2.0, 2.1, 2.2, 2.3, 2.35, 2.4, 2.45, 2.5, 2.6, 2.7, 2.8, 2.9, 3.0,
];

/// Smoke sweep (GHz): three points on the coarse fixture — pipeline
/// check, not a benchmark (the smoke geometry differs).
const FREQS_GHZ_SMOKE: [f64; 3] = [2.2, 2.4, 2.6];

/// Matched-fixture sweep (GHz, issue #237): refined 21-point grid
/// (2.15 → 2.35 GHz, 10 MHz step) around the empirically located TM010
/// resonance so the deeper -10 dB S11 dip is well-resolved and the
/// upper / lower -10 dB crossings of the bandwidth are bracketed by
/// interior points (10 MHz ≪ the 2 % loss-limited fractional BW).
const FREQS_GHZ_MATCHED: [f64; 21] = [
    2.15, 2.16, 2.17, 2.18, 2.19, 2.20, 2.21, 2.22, 2.23, 2.24, 2.25, 2.26, 2.27, 2.28, 2.29, 2.30,
    2.31, 2.32, 2.33, 2.34, 2.35,
];

#[derive(Clone, Copy, PartialEq)]
enum FixtureChoice {
    /// `patch_2g4.msh` (~30.6 k edges) → `results.toml`.
    Benchmark,
    /// `patch_2g4_smoke.msh` (~6.2 k edges) → `results_smoke.toml`.
    Smoke,
    /// `patch_2g4_matched.msh` (~31 k edges, issue #237) → `results_matched.toml`.
    Matched,
}

struct Row {
    f_ghz: f64,
    omega: f64,
    z_ohm: c64,
    s11: c64,
    p_in: f64,
    p_rad: f64,
    efficiency: f64,
    residual_rel: f64,
}

fn ghz_to_omega(f_ghz: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / C_MM_PER_S
}

fn pml_thick_for(choice: FixtureChoice) -> f64 {
    match choice {
        FixtureChoice::Benchmark | FixtureChoice::Matched => PML_THICK_BENCH_MM,
        FixtureChoice::Smoke => PML_THICK_SMOKE_MM,
    }
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

fn fixture_sha256(choice: FixtureChoice) -> String {
    let rel = match choice {
        FixtureChoice::Benchmark => "tests/fixtures/patch_2g4.msh",
        FixtureChoice::Smoke => "tests/fixtures/patch_2g4_smoke.msh",
        FixtureChoice::Matched => "tests/fixtures/patch_2g4_matched.msh",
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(_) => return "unknown".to_string(),
    };
    // Lightweight SHA-256 via the `sha2`-free `Command` to `shasum`/
    // `sha256sum`; fall back to "unknown" if neither tool is present.
    for tool in ["sha256sum", "shasum"] {
        let mut cmd = Command::new(tool);
        if tool == "shasum" {
            cmd.args(["-a", "256"]);
        }
        cmd.arg(&path);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                if let Some(hash) = String::from_utf8_lossy(&out.stdout)
                    .split_whitespace()
                    .next()
                {
                    return hash.to_string();
                }
            }
        }
    }
    let _ = bytes;
    "unknown".to_string()
}

fn results_path(choice: FixtureChoice) -> PathBuf {
    let file = match choice {
        FixtureChoice::Benchmark => "results.toml",
        FixtureChoice::Smoke => "results_smoke.toml",
        FixtureChoice::Matched => "results_matched.toml",
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmarks")
        .join("patch_antenna")
        .join(file)
}

fn run_sweep(fixture: &PatchFixture, freqs_ghz: &[f64], pml_thick: f64) -> Vec<Row> {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();

    // PEC: patch + ground conductor faces + outer truncation walls.
    let patch = fixture.patch_triangles();
    let ground = fixture.ground_triangles();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    let port = fixture.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let v_inc = c64::new(1.0, 0.0);
    let lp = port.lumped_port(r_nat, v_inc);

    // Purely port-driven: no volume current source.
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    // Box-UPML geometry + the (shrunk) flux-integration surface.
    let (air_lo, air_hi) = fixture.air_box(pml_thick);
    let center: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_lo[k] + air_hi[k]));
    let half: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_hi[k] - air_lo[k]));
    let flux_lo: [f64; 3] = std::array::from_fn(|k| center[k] - (1.0 - FLUX_SHRINK) * half[k]);
    let flux_hi: [f64; 3] = std::array::from_fn(|k| center[k] + (1.0 - FLUX_SHRINK) * half[k]);

    eprintln!(
        "patch sweep: {} edges, {} tets, {} port faces, {} UPML tets; \
         air box [{:.1},{:.1},{:.1}]–[{:.1},{:.1},{:.1}] mm",
        edges.len(),
        fixture.mesh.n_tets(),
        port.faces.len(),
        fixture.upml_tets().len(),
        air_lo[0],
        air_lo[1],
        air_lo[2],
        air_hi[0],
        air_hi[1],
        air_hi[2],
    );

    let t0 = std::time::Instant::now();
    let rows = freqs_ghz
        .iter()
        .map(|&f_ghz| {
            let omega = ghz_to_omega(f_ghz);
            // Box-UPML tensors are ω-dependent → rebuild per frequency.
            let (eps_t, nu_t) = fixture.matched_upml_materials(
                &FR4_MATERIALS,
                air_lo,
                air_hi,
                pml_thick,
                SIGMA_0,
                omega,
            );
            let sol = driven_solve_with_ports::<B>(
                &fixture.mesh,
                DrivenMaterials::MatchedUpml {
                    epsilon_tensor: &eps_t,
                    nu_tensor: &nu_t,
                },
                None,
                &DrivenBcs {
                    pec_interior_mask: &mask,
                },
                std::slice::from_ref(&lp),
                omega,
                &source,
                &device,
            )
            .unwrap_or_else(|e| panic!("driven solve at {f_ghz} GHz: {e}"));

            let v = port_voltage(&fixture.mesh, &lp, &edges, &sol.e_edges);
            let i = port_current(&lp, v);
            let z = v / i;
            let z_ohm = z * ETA_0;
            // Net input power ½ Re(V·I*) in natural units → watts-like
            // (η₀-normalized; cancels in the efficiency ratio).
            let p_in = 0.5 * (v * i.conj()).re;
            let p_rad = flux_power_box(&fixture.mesh, omega, &sol.e_edges, flux_lo, flux_hi);
            let efficiency = if p_in != 0.0 { p_rad / p_in } else { 0.0 };

            eprintln!(
                "  f = {f_ghz:4.2} GHz: Z = {:8.2} + {:8.2}i ohm, |S11| = {:.4}, \
                 P_in = {:.3e}, P_rad = {:.3e}, eta = {:.3}, res = {:.1e}",
                z_ohm.re,
                z_ohm.im,
                s11(z, r_nat).norm(),
                p_in,
                p_rad,
                efficiency,
                sol.residual_rel,
            );

            Row {
                f_ghz,
                omega,
                z_ohm,
                s11: s11(z, r_nat),
                p_in,
                p_rad,
                efficiency,
                residual_rel: sol.residual_rel,
            }
        })
        .collect::<Vec<_>>();
    eprintln!(
        "sweep of {} points done in {:.1} s",
        rows.len(),
        t0.elapsed().as_secs_f64()
    );
    rows
}

/// Interpolate the two |S₁₁| = −10 dB (|S₁₁| = 1/√10) crossings that
/// bracket the dip, returning `(f_lo, f_hi)` in GHz if the sweep brackets
/// them on both sides of the minimum.
// The threshold walks need both the bracketing sample indices for the
// linear interpolation, so plain range loops are the clearest form.
#[allow(clippy::needless_range_loop)]
fn bandwidth_10db(rows: &[Row]) -> Option<(f64, f64)> {
    let thresh = (0.1_f64).sqrt(); // |S11| at −10 dB return loss
                                   // Index of the |S11| minimum.
    let i_min = rows
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.s11.norm().partial_cmp(&b.s11.norm()).unwrap())
        .map(|(i, _)| i)?;
    if rows[i_min].s11.norm() > thresh {
        return None; // dip never reaches −10 dB
    }
    // Walk left from the minimum to the first crossing above threshold.
    let cross = |i_hi: usize, i_lo: usize| -> f64 {
        let (f0, m0) = (rows[i_lo].f_ghz, rows[i_lo].s11.norm());
        let (f1, m1) = (rows[i_hi].f_ghz, rows[i_hi].s11.norm());
        f0 + (f1 - f0) * (thresh - m0) / (m1 - m0)
    };
    let mut f_lo = None;
    for i in (0..i_min).rev() {
        if rows[i].s11.norm() >= thresh {
            f_lo = Some(cross(i + 1, i));
            break;
        }
    }
    let mut f_hi = None;
    for i in (i_min + 1)..rows.len() {
        if rows[i].s11.norm() >= thresh {
            f_hi = Some(cross(i - 1, i));
            break;
        }
    }
    match (f_lo, f_hi) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
fn write_toml(rows: &[Row], path: &PathBuf, choice: FixtureChoice, pml_thick: f64) {
    let commit = current_commit();
    let cavity = FIXTURE_PATCH;
    let f_res_cavity_ghz = cavity.resonant_frequency() / 1e9;

    // FEM resonance: the Im Z = 0 crossing (fall back to the |S11| dip
    // frequency if the sweep doesn't bracket a crossing).
    let omegas: Vec<f64> = rows.iter().map(|r| r.omega).collect();
    let zs: Vec<c64> = rows.iter().map(|r| r.z_ohm).collect();
    let f_res_fem_ghz = im_z_zero_crossings(&omegas, &zs)
        .first()
        .map(|&w| w * C_MM_PER_S / (2.0 * std::f64::consts::PI * 1.0e9))
        .or_else(|| {
            rows.iter()
                .min_by(|a, b| a.s11.norm().partial_cmp(&b.s11.norm()).unwrap())
                .map(|r| r.f_ghz)
        });

    let dip = rows
        .iter()
        .min_by(|a, b| a.s11.norm().partial_cmp(&b.s11.norm()).unwrap())
        .expect("non-empty sweep");
    let s11_dip_db = 20.0 * dip.s11.norm().log10();
    let bw = bandwidth_10db(rows);

    let mut s = String::new();
    s.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    match choice {
        FixtureChoice::Benchmark => s.push_str("#   --example patch_antenna`.\n"),
        FixtureChoice::Smoke => s.push_str("#   --example patch_antenna -- smoke`.\n"),
        FixtureChoice::Matched => s.push_str("#   --example patch_antenna -- matched`.\n"),
    }
    s.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    s.push_str("# Consumed by `tests/patch_antenna_benchmark.rs` and compared\n");
    s.push_str("# against the in-repo cavity-model oracle (geode_core::patch_cavity).\n");
    s.push('\n');

    s.push_str("[meta]\n");
    match choice {
        FixtureChoice::Benchmark => {
            s.push_str("description = \"Patch-antenna S11/resonance/bandwidth/efficiency benchmark (issue #228, Epic #226 Phase 2): port-driven frequency sweep of the FR-4 patch fixture (patch_2g4.msh) with a matched box-UPML, S11(f) / f_res / -10 dB BW / Z_in / radiation efficiency vs the Balanis cavity-model oracle.\"\n");
            s.push_str("fixture = \"tests/fixtures/patch_2g4.msh\"\n");
            s.push_str("fixture_provenance = \"tests/fixtures/patch_2g4.provenance.txt\"\n");
        }
        FixtureChoice::Smoke => {
            s.push_str("description = \"Patch-antenna smoke run (issue #228): same pipeline as results.toml on the coarse patch_2g4_smoke.msh fixture — pipeline check, not a benchmark.\"\n");
            s.push_str("fixture = \"tests/fixtures/patch_2g4_smoke.msh\"\n");
            s.push_str("fixture_provenance = \"tests/fixtures/patch_2g4_smoke.provenance.txt\"\n");
        }
        FixtureChoice::Matched => {
            s.push_str("description = \"Patch-antenna impedance-matched return-loss benchmark (issue #237, Epic #226 follow-up): same pipeline as results.toml on the tuned patch_2g4_matched.msh fixture (coax-probe inset 8.0 -> 7.0 mm), with a refined 21-point sweep so the -10 dB return-loss bandwidth is bracketed by interior points.\"\n");
            s.push_str("fixture = \"tests/fixtures/patch_2g4_matched.msh\"\n");
            s.push_str(
                "fixture_provenance = \"tests/fixtures/patch_2g4_matched.provenance.txt\"\n",
            );
        }
    }
    s.push_str(&format!(
        "fixture_sha256 = \"{}\"\n",
        fixture_sha256(choice)
    ));
    s.push_str(&format!("generated_at_commit = \"{commit}\"\n"));
    s.push_str(&format!("port_resistance_ohm = {R_PORT_OHM}\n"));
    s.push_str("conductors = \"pec\"\n");
    s.push_str("outer_boundary = \"pec\"\n");
    s.push_str("absorber = \"matched_box_upml\"\n");
    s.push_str(&format!("upml_sigma_0 = {SIGMA_0}\n"));
    s.push_str(&format!("upml_thick_mm = {pml_thick}\n"));
    s.push_str("substrate = \"fr4\"\n");
    s.push_str(&format!("eps_r = {}\n", cavity.eps_r));
    s.push_str(&format!("tan_delta = {}\n", cavity.tan_delta));
    if let Some(f) = f_res_fem_ghz {
        s.push_str(&format!("f_res_fem_ghz = {f:.6e}\n"));
    } else {
        s.push_str("# f_res_fem_ghz: sweep does not bracket an Im Z = 0 crossing.\n");
    }
    s.push_str(&format!("s11_dip_db = {s11_dip_db:.6e}\n"));
    s.push_str(&format!("s11_dip_f_ghz = {:.6e}\n", dip.f_ghz));
    if let Some((lo, hi)) = bw {
        s.push_str(&format!("bw_10db_lo_ghz = {lo:.6e}\n"));
        s.push_str(&format!("bw_10db_hi_ghz = {hi:.6e}\n"));
        s.push_str(&format!("bw_10db_ghz = {:.6e}\n", hi - lo));
        if let Some(f) = f_res_fem_ghz {
            s.push_str(&format!("bw_10db_fractional = {:.6e}\n", (hi - lo) / f));
        }
    } else {
        s.push_str("# bw_10db_*: sweep does not bracket both -10 dB crossings around the dip.\n");
    }
    s.push_str("notes = [\n");
    s.push_str("  \"Z_in = V/I at the lumped probe port (Palace-style uniform port, V_inc = 1, R = 50 ohm); S11 vs 50 ohm; f_res = first Im Z = 0 crossing (im_z_zero_crossings).\",\n");
    s.push_str("  \"Radiation efficiency eta = P_rad / P_in, P_in = 0.5 Re(V I*) at the port, P_rad = box-surface Poynting flux (scattering::flux_power_box) over a surface shrunk 10% inside the matched box-UPML inner wall, enclosing the whole radiator.\",\n");
    s.push_str("  \"Conductors (patch, ground, outer wall) are PEC; substrate loss enters via FR-4 tan delta in the permittivity. With PEC metal the only loss channels are dielectric + radiation, so eta is loss-limited by the FR-4 tan delta (0.02).\",\n");
    s.push_str("  \"Cavity-model oracle (Balanis Antenna Theory 4e, geode_core::patch_cavity): a ~3-5% sanity band on f_res, not a tight reference — FR-4 eps_r tolerance (+-0.2) and the fringing curve-fit dominate the residual.\",\n");
    s.push_str("]\n");
    s.push('\n');

    s.push_str("[oracles.cavity_model]\n");
    s.push_str("# geode_core::patch_cavity (Balanis 4e) on the fixture geometry\n");
    s.push_str("# (W = 38, L = 29, h = 1.6 mm, FR-4 eps_r = 4.4).\n");
    s.push_str(&format!("epsilon_eff = {:.6e}\n", cavity.epsilon_eff()));
    s.push_str(&format!("delta_l_mm = {:.6e}\n", cavity.delta_l() * 1000.0));
    s.push_str(&format!("f_res_ghz = {f_res_cavity_ghz:.6e}\n"));
    s.push_str(&format!(
        "edge_resistance_ohm = {:.6e}\n",
        cavity.edge_resistance()
    ));
    {
        let gamma = (0.1_f64).sqrt();
        let vswr = (1.0 + gamma) / (1.0 - gamma);
        s.push_str(&format!(
            "loss_limited_q = {:.6e}\n",
            cavity.loss_limited_q()
        ));
        s.push_str(&format!(
            "fractional_bw_10db = {:.6e}\n",
            cavity.fractional_bandwidth(vswr)
        ));
    }
    s.push('\n');

    s.push_str("[oracles.palace]\n");
    s.push_str("status = \"pending_operator_run\"\n");
    s.push_str("note = \"Palace is not installed on the generation machine (only a Docker build recipe under ~/GitHub/sphere/eda/mom/docker/palace). The geode-fem-side config generator + result ingester (issue #239) live in reference/palace/geode_patch_baseline/ (emits palace_config.json) and crates/geode-core/src/palace.rs (parses Palace's s-parameters.csv into a populated [oracles.palace] block). Operator workflow: (1) emit the config, (2) run Palace, (3) populate this slot via geode_core::palace::PalaceResults with full provenance (palace_version, config_sha256). Same toolchain-gap convention as the FastHenry slot of benchmarks/spiral_inductor/results.toml.\"\n");
    s.push('\n');

    // Achieved comparison at resonance.
    if let Some(f) = f_res_fem_ghz {
        let rel = (f - f_res_cavity_ghz) / f_res_cavity_ghz;
        let z_at_res = rows
            .iter()
            .min_by(|a, b| {
                (a.f_ghz - f)
                    .abs()
                    .partial_cmp(&(b.f_ghz - f).abs())
                    .unwrap()
            })
            .expect("non-empty sweep");
        s.push_str("[comparison]\n");
        s.push_str(&format!("f_res_fem_ghz = {f:.6e}\n"));
        s.push_str(&format!("f_res_cavity_ghz = {f_res_cavity_ghz:.6e}\n"));
        s.push_str(&format!("f_res_rel_err = {rel:.6e}\n"));
        s.push_str(&format!("z_in_at_res_re_ohm = {:.6e}\n", z_at_res.z_ohm.re));
        s.push_str(&format!("z_in_at_res_im_ohm = {:.6e}\n", z_at_res.z_ohm.im));
        s.push_str(&format!(
            "efficiency_at_res = {:.6e}\n",
            z_at_res.efficiency
        ));
        s.push('\n');
    }

    for (i, r) in rows.iter().enumerate() {
        s.push_str(&format!("[point_{i}]\n"));
        s.push_str(&format!("f_ghz = {:.15e}\n", r.f_ghz));
        s.push_str(&format!("omega_natural = {:.15e}\n", r.omega));
        s.push_str(&format!("z_re_ohm = {:.15e}\n", r.z_ohm.re));
        s.push_str(&format!("z_im_ohm = {:.15e}\n", r.z_ohm.im));
        s.push_str(&format!("s11_re = {:.15e}\n", r.s11.re));
        s.push_str(&format!("s11_im = {:.15e}\n", r.s11.im));
        s.push_str(&format!("s11_mag = {:.15e}\n", r.s11.norm()));
        s.push_str(&format!("s11_db = {:.15e}\n", 20.0 * r.s11.norm().log10()));
        s.push_str(&format!("p_in = {:.15e}\n", r.p_in));
        s.push_str(&format!("p_rad = {:.15e}\n", r.p_rad));
        s.push_str(&format!("efficiency = {:.15e}\n", r.efficiency));
        s.push_str(&format!("solve_residual_rel = {:.3e}\n", r.residual_rel));
        s.push('\n');
    }

    fs::create_dir_all(path.parent().expect("results parent")).expect("mkdir");
    fs::write(path, s).expect("write patch_antenna results TOML");
    eprintln!("wrote {}", path.display());
}

/// `(θ, φ)` grid for the patch far-field extraction. A 1° polar step
/// resolves the main lobe and nulls; 5° in azimuth is ample for the
/// principal-plane cuts.
const PATTERN_N_THETA: usize = 91; // 0..π in 2° steps
const PATTERN_N_PHI: usize = 72; // 0..2π in 5° steps

/// Re-solve the patch at its resonant frequency and run the near-to-
/// far-field transform (issue #229, Epic #226 Phase 3): far-field
/// `E(θ,φ)` → broadside directivity, gain `G = D·η`, and E-/H-plane
/// principal-plane cuts. Writes `benchmarks/patch_antenna/pattern.toml`.
fn extract_pattern(fixture: &PatchFixture, f_res_ghz: f64, pml_thick: f64, choice: FixtureChoice) {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let patch = fixture.patch_triangles();
    let ground = fixture.ground_triangles();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    let port = fixture.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let lp = port.lumped_port(r_nat, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    let (air_lo, air_hi) = fixture.air_box(pml_thick);
    let center: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_lo[k] + air_hi[k]));
    let half: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_hi[k] - air_lo[k]));
    let flux_lo: [f64; 3] = std::array::from_fn(|k| center[k] - (1.0 - FLUX_SHRINK) * half[k]);
    let flux_hi: [f64; 3] = std::array::from_fn(|k| center[k] + (1.0 - FLUX_SHRINK) * half[k]);

    let omega = ghz_to_omega(f_res_ghz);
    let (eps_t, nu_t) =
        fixture.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, pml_thick, SIGMA_0, omega);
    eprintln!("NTFF extraction at f_res = {f_res_ghz:.4} GHz (omega = {omega:.5e} rad/mm)");
    let sol = driven_solve_with_ports::<B>(
        &fixture.mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        omega,
        &source,
        &device,
    )
    .expect("patch driven solve for NTFF");

    let v = port_voltage(&fixture.mesh, &lp, &edges, &sol.e_edges);
    let i = port_current(&lp, v);
    let p_in = 0.5 * (v * i.conj()).re;
    let p_rad = flux_power_box(&fixture.mesh, omega, &sol.e_edges, flux_lo, flux_hi);
    let eta = if p_in != 0.0 { p_rad / p_in } else { 0.0 };

    let ff = ntff_far_field(
        &fixture.mesh,
        omega,
        &sol.e_edges,
        flux_lo,
        flux_hi,
        PATTERN_N_THETA,
        PATTERN_N_PHI,
    );
    let (d_max, _d_grid) = directivity(&ff);
    let d_broadside = broadside_directivity(&ff);
    let g_broadside = gain(d_broadside, eta);
    let (e_plane, h_plane) = principal_plane_cuts(&ff);

    let cavity = FIXTURE_PATCH;
    let d_cavity = cavity.broadside_directivity(cavity.resonant_wavelength());

    eprintln!(
        "  eta = {eta:.4}, D_max = {d_max:.3} ({:.2} dBi), D_broadside = {d_broadside:.3} ({:.2} dBi)",
        to_db(d_max),
        to_db(d_broadside),
    );
    eprintln!(
        "  G_broadside = {g_broadside:.3} ({:.2} dBi); cavity-model D = {d_cavity:.3} ({:.2} dBi), delta = {:.2} dB",
        to_db(g_broadside),
        to_db(d_cavity),
        to_db(d_broadside) - to_db(d_cavity),
    );

    write_pattern_toml(
        choice,
        f_res_ghz,
        omega,
        eta,
        d_max,
        d_broadside,
        g_broadside,
        d_cavity,
        &e_plane,
        &h_plane,
    );
}

/// 3D radiation-lobe `(θ, φ)` sampling resolution (issue #289).
///
/// `LOBE_N_THETA = 37` polar samples over `[0, π]` (5° steps, inclusive of
/// both poles) and `LOBE_N_PHI = 72` azimuth samples over `[0, 2π)` (5°
/// steps). This is the spec's suggested 37×72 grid: dense enough that the
/// triangulated lobe reads smoothly in ParaView (5° facets) while keeping
/// the driven NTFF cost modest (2664 observation directions). The 2D-cut
/// `pattern` path uses a finer 91×72 grid because it only reports 1D cuts;
/// the 3D surface trades a little polar resolution for a balanced sphere.
const LOBE_N_THETA: usize = 37;
const LOBE_N_PHI: usize = 72;

/// dB floor (dBi-below-peak) for the radiation-lobe vertex radius (#289).
///
/// The lobe vertex radius encodes *normalised* directivity. We use a
/// dB-floored normalisation: `r = clamp((D_dB - D_dB_max - FLOOR) / -FLOOR,
/// 0, 1)` so the peak sits at radius 1 and everything `FLOOR` dB or more
/// below the peak collapses to the origin. A dB floor (vs raw linear)
/// reads better for antenna lobes — it opens up the sidelobe/null structure
/// that a linear radius would crush against zero. −20 dB is the textbook
/// default (Balanis): it shows the first sidelobes without drowning the
/// plot in noise-floor fuzz.
const LOBE_DB_FLOOR: f64 = -20.0;

/// Parse an `--out <path>` flag from the CLI args (after the directive).
/// Returns `None` if absent, so the caller can fall back to a default.
fn parse_out_flag() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--out" {
            return args.next().map(PathBuf::from);
        }
        if let Some(rest) = a.strip_prefix("--out=") {
            return Some(PathBuf::from(rest));
        }
    }
    None
}

/// Re-solve the patch at resonance, run the NTFF (reusing the exact same
/// `ntff_far_field` / `directivity` machinery as [`extract_pattern`]),
/// sample `D(θ, φ)` on the `LOBE_N_THETA × LOBE_N_PHI` sphere, build a
/// triangulated lobe whose vertex radius is the dB-floored normalised
/// directivity, and write it to `out` as a surface `.vtu` via
/// [`write_vtu_surface`], carrying `D` and `D_dB` as `PointData` for
/// ParaView colouring (issue #289, Epic #276 Phase 3A).
fn extract_pattern_3d(
    fixture: &PatchFixture,
    f_res_ghz: f64,
    pml_thick: f64,
    out: &std::path::Path,
) {
    use burn::tensor::backend::BackendTypes;
    type B = DefaultBackend;
    let device = <B as BackendTypes>::Device::default();

    let edges = fixture.mesh.edges();
    let patch = fixture.patch_triangles();
    let ground = fixture.ground_triangles();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
    );

    let port = fixture.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let lp = port.lumped_port(r_nat, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
    };

    let (air_lo, air_hi) = fixture.air_box(pml_thick);
    let center: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_lo[k] + air_hi[k]));
    let half: [f64; 3] = std::array::from_fn(|k| 0.5 * (air_hi[k] - air_lo[k]));
    let flux_lo: [f64; 3] = std::array::from_fn(|k| center[k] - (1.0 - FLUX_SHRINK) * half[k]);
    let flux_hi: [f64; 3] = std::array::from_fn(|k| center[k] + (1.0 - FLUX_SHRINK) * half[k]);

    let omega = ghz_to_omega(f_res_ghz);
    let (eps_t, nu_t) =
        fixture.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, pml_thick, SIGMA_0, omega);
    eprintln!("3D-lobe NTFF at f_res = {f_res_ghz:.4} GHz (omega = {omega:.5e} rad/mm)");
    let sol = driven_solve_with_ports::<B>(
        &fixture.mesh,
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &eps_t,
            nu_tensor: &nu_t,
        },
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        omega,
        &source,
        &device,
    )
    .expect("patch driven solve for 3D lobe NTFF");

    // Reuse the NTFF transform + directivity exactly (do not reimplement).
    let ff = ntff_far_field(
        &fixture.mesh,
        omega,
        &sol.e_edges,
        flux_lo,
        flux_hi,
        LOBE_N_THETA,
        LOBE_N_PHI,
    );
    let (d_max, d_grid) = directivity(&ff);
    eprintln!(
        "  D_max = {d_max:.3} ({:.2} dBi) on the {LOBE_N_THETA}x{LOBE_N_PHI} sphere",
        to_db(d_max)
    );

    let lobe = build_lobe_surface(&ff, &d_grid, d_max);

    fs::create_dir_all(out.parent().expect("out path parent")).expect("mkdir artifacts/viz");
    write_vtu_surface(
        out,
        &lobe.points,
        &lobe.tris,
        &[("D", &lobe.d), ("D_dB", &lobe.d_db)],
    )
    .expect("write 3D radiation-lobe surface .vtu");
    eprintln!(
        "  wrote {} ({} verts, {} tris)",
        out.display(),
        lobe.points.len(),
        lobe.tris.len()
    );
}

/// Triangulated 3D radiation-lobe surface produced by [`build_lobe_surface`].
struct LobeSurface {
    /// Vertex coordinates (radius = dB-floored normalised directivity).
    points: Vec<[f64; 3]>,
    /// Triangle connectivity (0-based indices into `points`).
    tris: Vec<[usize; 3]>,
    /// Per-vertex un-scaled linear directivity `D` (colour scalar).
    d: Vec<f64>,
    /// Per-vertex directivity in dBi (`D_dB`) (colour scalar).
    d_db: Vec<f64>,
}

/// Turn a [`geode_core::FarField`] directivity grid into a triangulated
/// radiation-lobe surface mesh.
///
/// * vertex `(θ_i, φ_j)` sits at radius `r = dB-floored normalised D` along
///   the observation direction `r̂(θ, φ)` (same convention as the NTFF
///   `sph_basis`: `x = sinθ cosφ`, `y = sinθ sinφ`, `z = cosθ`),
/// * triangles wrap the `LOBE_N_THETA × (LOBE_N_PHI + 1)` lat/long grid
///   (azimuth seam closed by reusing column 0 at φ = 2π), with degenerate
///   pole triangles dropped,
/// * `d` carries the *un-scaled* linear `D` and `d_db` the `D_dB` (dBi) at
///   each vertex for ParaView colouring.
fn build_lobe_surface(ff: &geode_core::FarField, d_grid: &[f64], d_max: f64) -> LobeSurface {
    let n_theta = ff.n_theta();
    let n_phi = ff.n_phi();
    // Close the azimuth seam: append a duplicate column at φ = 2π so the
    // surface wraps. Grid width is n_phi + 1.
    let w = n_phi + 1;
    let d_db_max = to_db(d_max);

    let mut points = Vec::with_capacity(n_theta * w);
    let mut d_pd = Vec::with_capacity(n_theta * w);
    let mut d_db_pd = Vec::with_capacity(n_theta * w);

    for it in 0..n_theta {
        let th = ff.theta[it];
        let (st, ct) = th.sin_cos();
        for jp in 0..w {
            let ip = jp % n_phi; // seam column reuses ip = 0
            let phi = 2.0 * std::f64::consts::PI * jp as f64 / n_phi as f64;
            let (sp, cp) = phi.sin_cos();
            let d = d_grid[it * n_phi + ip];
            let d_db = to_db(d);
            // dB-floored normalised radius in [0, 1].
            let r = ((d_db - d_db_max - LOBE_DB_FLOOR) / -LOBE_DB_FLOOR).clamp(0.0, 1.0);
            let r_hat = [st * cp, st * sp, ct];
            points.push([r * r_hat[0], r * r_hat[1], r * r_hat[2]]);
            d_pd.push(d);
            d_db_pd.push(d_db);
        }
    }

    let mut tris = Vec::with_capacity(2 * (n_theta - 1) * n_phi);
    let vid = |it: usize, jp: usize| it * w + jp;
    for it in 0..n_theta - 1 {
        for jp in 0..n_phi {
            // Quad (it,jp)-(it,jp+1)-(it+1,jp+1)-(it+1,jp) → two triangles.
            let a = vid(it, jp);
            let b = vid(it, jp + 1);
            let c = vid(it + 1, jp + 1);
            let d = vid(it + 1, jp);
            // Drop degenerate triangles at the poles (θ = 0 or θ = π rows
            // collapse to a single point, so one tri of each quad is null).
            if it != 0 {
                tris.push([a, b, d]);
            }
            if it != n_theta - 2 {
                tris.push([b, c, d]);
            }
        }
    }

    LobeSurface {
        points,
        tris,
        d: d_pd,
        d_db: d_db_pd,
    }
}

#[allow(clippy::too_many_arguments)]
fn write_pattern_toml(
    choice: FixtureChoice,
    f_res_ghz: f64,
    omega: f64,
    eta: f64,
    d_max: f64,
    d_broadside: f64,
    g_broadside: f64,
    d_cavity: f64,
    e_plane: &geode_core::PatternCut,
    h_plane: &geode_core::PatternCut,
) {
    let file = match choice {
        FixtureChoice::Benchmark => "pattern.toml",
        FixtureChoice::Smoke => "pattern_smoke.toml",
        FixtureChoice::Matched => "pattern_matched.toml",
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmarks")
        .join("patch_antenna")
        .join(file);

    let commit = current_commit();
    let mut s = String::new();
    s.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    match choice {
        FixtureChoice::Benchmark => s.push_str("#   --example patch_antenna -- pattern`.\n"),
        FixtureChoice::Smoke => s.push_str("#   --example patch_antenna -- pattern-smoke`.\n"),
        FixtureChoice::Matched => s.push_str("#   --example patch_antenna -- pattern-matched`.\n"),
    }
    s.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    s.push_str("# Patch radiation pattern / directivity / gain (issue #229,\n");
    s.push_str("# Epic #226 Phase 3) from the near-to-far-field transform\n");
    s.push_str("# (geode_core::ntff) of the driven near field at resonance.\n");
    s.push('\n');

    s.push_str("[meta]\n");
    match choice {
        FixtureChoice::Benchmark | FixtureChoice::Smoke => {
            s.push_str("description = \"Patch-antenna far-field radiation pattern, broadside directivity, and gain (issue #229, Epic #226 Phase 3): Love surface-equivalence NTFF (geode_core::ntff) of the driven near field on the Huygens box just inside the matched box-UPML, at the Phase-2 resonant frequency. Cross-checked against the Balanis cavity-model two-slot directivity (geode_core::patch_cavity).\"\n");
        }
        FixtureChoice::Matched => {
            s.push_str("description = \"Patch-antenna far-field radiation pattern, broadside directivity, and gain on the impedance-matched fixture (issue #247, Epic #226 Phase 3 follow-up): Love surface-equivalence NTFF (geode_core::ntff) of the driven near field on the Huygens box just inside the matched box-UPML, at the matched-fixture S11-dip frequency (issue #237, probe inset 8.0 -> 7.0 mm). D is essentially unchanged from the untuned pattern.toml (tuning the feed shifts the *match*, not the radiation pattern shape); G = D . eta_matched uses the tuned radiation efficiency from results_matched.toml. Cross-checked against the Balanis cavity-model two-slot directivity (geode_core::patch_cavity).\"\n");
        }
    }
    match choice {
        FixtureChoice::Benchmark => {
            s.push_str("fixture = \"tests/fixtures/patch_2g4.msh\"\n");
        }
        FixtureChoice::Smoke => {
            s.push_str("fixture = \"tests/fixtures/patch_2g4_smoke.msh\"\n");
        }
        FixtureChoice::Matched => {
            s.push_str("fixture = \"tests/fixtures/patch_2g4_matched.msh\"\n");
            s.push_str(
                "fixture_provenance = \"tests/fixtures/patch_2g4_matched.provenance.txt\"\n",
            );
            s.push_str(&format!(
                "fixture_sha256 = \"{}\"\n",
                fixture_sha256(choice)
            ));
        }
    }
    s.push_str(&format!("generated_at_commit = \"{commit}\"\n"));
    s.push_str(&format!("f_res_ghz = {f_res_ghz:.6e}\n"));
    s.push_str(&format!("omega_natural = {omega:.6e}\n"));
    s.push_str(&format!("n_theta = {PATTERN_N_THETA}\n"));
    s.push_str(&format!("n_phi = {PATTERN_N_PHI}\n"));
    s.push_str("notes = [\n");
    s.push_str("  \"E(theta,phi) from Love surface equivalence J_s = n x H, M_s = -n x E on the closed Huygens box (same surface as flux_power_box), radiation vectors N/L with e^{+jk r-hat . r'} (exp(+jwt) outgoing e^{-jkr} convention), E_theta = -(L_phi + N_theta), E_phi = (L_theta - N_phi), eta_0 = 1 natural units.\",\n");
    s.push_str("  \"directivity D = 4 pi |E|^2_max / closed-surface integral of |E|^2 dOmega (trapezoid in theta with sin-theta weight, rectangle in phi). broadside = +z (theta = 0). gain G = D_broadside . eta_rad with eta from Phase 2.\",\n");
    s.push_str("  \"NTFF transform validated independently on an analytic short dipole (geode_core::ntff unit tests: recovered D = 1.50, sin-theta pattern, translation/phase-sign invariance) before application to the patch.\",\n");
    s.push_str("]\n");
    s.push('\n');

    s.push_str("[results]\n");
    s.push_str(&format!("efficiency = {eta:.6e}\n"));
    s.push_str(&format!("directivity_max = {d_max:.6e}\n"));
    s.push_str(&format!("directivity_max_dbi = {:.6e}\n", to_db(d_max)));
    s.push_str(&format!("directivity_broadside = {d_broadside:.6e}\n"));
    s.push_str(&format!(
        "directivity_broadside_dbi = {:.6e}\n",
        to_db(d_broadside)
    ));
    s.push_str(&format!("gain_broadside = {g_broadside:.6e}\n"));
    s.push_str(&format!(
        "gain_broadside_dbi = {:.6e}\n",
        to_db(g_broadside)
    ));
    s.push('\n');

    s.push_str("[oracles.cavity_model]\n");
    s.push_str("# geode_core::PatchCavity::broadside_directivity (Balanis 4e two-slot model).\n");
    s.push_str(&format!("directivity_broadside = {d_cavity:.6e}\n"));
    s.push_str(&format!(
        "directivity_broadside_dbi = {:.6e}\n",
        to_db(d_cavity)
    ));
    s.push_str(&format!(
        "directivity_delta_db = {:.6e}\n",
        to_db(d_broadside) - to_db(d_cavity)
    ));
    s.push('\n');

    // Principal-plane cuts: theta (deg) vs normalized |E|.
    let push_cut = |s: &mut String, name: &str, cut: &geode_core::PatternCut| {
        s.push_str(&format!("[cut.{name}]\n"));
        s.push_str("# theta in degrees (0 = broadside +z); e_norm = |E| normalized to lobe max.\n");
        let theta_deg: Vec<String> = cut
            .theta
            .iter()
            .map(|t| format!("{:.4e}", t.to_degrees()))
            .collect();
        let e_norm: Vec<String> = cut.e_norm.iter().map(|e| format!("{e:.4e}")).collect();
        s.push_str(&format!("theta_deg = [{}]\n", theta_deg.join(", ")));
        s.push_str(&format!("e_norm = [{}]\n", e_norm.join(", ")));
        s.push('\n');
    };
    push_cut(&mut s, "e_plane", e_plane);
    push_cut(&mut s, "h_plane", h_plane);

    fs::create_dir_all(path.parent().expect("pattern parent")).expect("mkdir");
    fs::write(&path, s).expect("write patch_antenna pattern TOML");
    eprintln!("wrote {}", path.display());
}

fn main() {
    let arg = std::env::args().nth(1);
    // `pattern` / `pattern-smoke` run the NTFF radiation-pattern
    // extraction instead of the S11 sweep.
    match arg.as_deref() {
        Some("pattern") => {
            let fixture = read_patch_fixture().expect("bundled benchmark patch fixture");
            // The Phase-2 committed FEM resonance.
            let f_res_ghz = 2.274530;
            extract_pattern(
                &fixture,
                f_res_ghz,
                pml_thick_for(FixtureChoice::Benchmark),
                FixtureChoice::Benchmark,
            );
            return;
        }
        Some("pattern-smoke") => {
            let fixture = read_patch_smoke_fixture().expect("bundled smoke patch fixture");
            extract_pattern(
                &fixture,
                2.4,
                pml_thick_for(FixtureChoice::Smoke),
                FixtureChoice::Smoke,
            );
            return;
        }
        Some("pattern-matched") => {
            let fixture = read_patch_matched_fixture().expect("bundled matched patch fixture");
            // Matched-fixture S11 dip (`results_matched.toml::meta.s11_dip_f_ghz`,
            // issue #237) — the physically meaningful operating point of the
            // tuned antenna. Im(Z)=0 sits 2.5 MHz lower (2.2675 GHz) but the
            // matched-port radiation efficiency is reported at the dip.
            let f_res_ghz = 2.270;
            extract_pattern(
                &fixture,
                f_res_ghz,
                pml_thick_for(FixtureChoice::Matched),
                FixtureChoice::Matched,
            );
            return;
        }
        Some("pattern-3d") => {
            // 3D radiation-lobe surface `.vtu` export (issue #289, Epic #276
            // Phase 3A). Same driven solve + NTFF as the `pattern` path on the
            // benchmark fixture at the Phase-2 committed FEM resonance.
            let out = parse_out_flag().unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("..")
                    .join("artifacts")
                    .join("viz")
                    .join("patch_lobe.vtu")
            });
            let fixture = read_patch_fixture().expect("bundled benchmark patch fixture");
            let f_res_ghz = 2.274530;
            extract_pattern_3d(
                &fixture,
                f_res_ghz,
                pml_thick_for(FixtureChoice::Benchmark),
                &out,
            );
            return;
        }
        _ => {}
    }

    let choice = match arg.as_deref() {
        None => FixtureChoice::Benchmark,
        Some("smoke") => FixtureChoice::Smoke,
        Some("matched") => FixtureChoice::Matched,
        Some(other) => {
            eprintln!(
                "unknown argument {other:?} — expected `smoke`, `matched`, `pattern`, `pattern-smoke`, `pattern-matched`, `pattern-3d`, or no argument"
            );
            std::process::exit(2);
        }
    };
    let (fixture, freqs): (PatchFixture, &[f64]) = match choice {
        FixtureChoice::Benchmark => (
            read_patch_fixture().expect("bundled benchmark patch fixture"),
            &FREQS_GHZ,
        ),
        FixtureChoice::Smoke => (
            read_patch_smoke_fixture().expect("bundled smoke patch fixture"),
            &FREQS_GHZ_SMOKE,
        ),
        FixtureChoice::Matched => (
            read_patch_matched_fixture().expect("bundled matched patch fixture"),
            &FREQS_GHZ_MATCHED,
        ),
    };
    let pml_thick = pml_thick_for(choice);

    let rows = run_sweep(&fixture, freqs, pml_thick);

    let f_res_cavity = FIXTURE_PATCH.resonant_frequency() / 1e9;
    let dip = rows
        .iter()
        .min_by(|a, b| a.s11.norm().partial_cmp(&b.s11.norm()).unwrap())
        .expect("non-empty sweep");
    eprintln!("\ncavity-model f_res (Balanis): {f_res_cavity:.4} GHz");
    eprintln!(
        "S11 dip: {:.2} dB at {:.3} GHz",
        20.0 * dip.s11.norm().log10(),
        dip.f_ghz
    );
    if let Some((lo, hi)) = bandwidth_10db(&rows) {
        eprintln!(
            "-10 dB bandwidth: {:.3}-{:.3} GHz ({:.3} GHz)",
            lo,
            hi,
            hi - lo
        );
    } else {
        eprintln!("-10 dB bandwidth: not bracketed by the sweep");
    }

    write_toml(&rows, &results_path(choice), choice, pml_thick);
}
