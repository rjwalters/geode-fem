//! **Pre-optimization baseline of the curved conformal 2.4 GHz patch**
//! (issue #1084, conformal v5 E1, operator choice (c)).
//!
//! Loads one level of the curved conformal fixture family
//! (`reference/gmsh/patch_conformal_L*.yaml`: the `patch_2g4_matched`
//! geometry, a 38 × 29 mm PEC patch on 1.6 mm FR-4 with a 7.0 mm probe
//! inset, wrapped onto a cylinder of radius R = 100 mm) **by path**
//! through [`read_patch_fixture_from_bytes`], together with the flat mesh
//! it was bent from (same topology, node for node), and runs the same
//! dense port-driven S₁₁ sweep on both:
//!
//! ```text
//! Z_in(f) = V / I,  I = (2 V_inc − V)/R,  S₁₁ = (Z − Z₀)/(Z + Z₀),  Z₀ = R = 50 Ω
//! P_in    = ½ Re(V I*),  P_rad = flux_power_box(...),  η = P_rad / P_in
//! ```
//!
//! Because the two meshes share every tet, the difference between the two
//! sweeps is the **bending detune** alone, not a meshing change.
//!
//! # Units
//!
//! Lengths are in mm. Frequencies are in GHz; the solver works with
//! `ω = k₀ = 2π f / c` in rad/mm (`geode_util::units::ghz_to_omega_mm`).
//! The 50 Ω port resistance enters the solver as `50 / η₀`. Impedances are
//! reported in Ω.
//!
//! # Inputs
//!
//! * `PATCH_CONFORMAL_FIXTURE` (or the first CLI argument): the bent
//!   `.msh`. Default: the committed CI level
//!   `crates/geode-core/tests/fixtures/patch_conformal_L0.msh`.
//! * `PATCH_CONFORMAL_FLAT_FIXTURE`: the flat companion. Default:
//!   `<stem>.flat.msh` next to the bent mesh, as the generator writes it.
//!   The flat L0 companion is not committed; regenerate it with
//!   `python3 reference/gmsh/generate_patch_fixture.py
//!   reference/gmsh/patch_conformal_L0.yaml <dir>/patch_conformal_L0.msh`.
//! * `<stem>.provenance.txt` next to the bent mesh (written by the
//!   generator): the sha256 of both meshes is checked against it, and the
//!   bend radius, UPML thickness and σ₀ are read from it.
//! * `PATCH_CONFORMAL_SWEEP_GHZ="lo,hi,n"` (uniform) or
//!   `"list:f1,f2,..."` (explicit) overrides the default grid (2.10–2.70
//!   GHz, 10 MHz steps over 2.20–2.60 GHz plus the 2.4835 GHz ISM band
//!   edge). A grid with no point in the ISM band leaves the `ism_*_s11_db`
//!   summary keys out instead of writing `inf`.
//! * `PATCH_CONFORMAL_SIGMA_0`: overrides the fixture's UPML σ₀ (for
//!   absorber-sensitivity runs; recorded in `[meta]`).
//! * `PATCH_CONFORMAL_OUT`: output path. Default:
//!   `benchmarks/patch_antenna_conformal/baseline_2g4_<level>.toml`.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --release --example patch_conformal_2g4_baseline -- \
//!       /path/to/patch_conformal_L1.msh
//! ```
//!
//! Per-solve wall-clock (`solve_s`: per-frequency UPML tensors, assembly,
//! sparse LU and solve), post-processing time (`post_s`: port readback and
//! the box Poynting flux) and peak RSS are recorded per sweep point. The RSS
//! is the maximum of `ps -o rss=` samples taken every 50 ms during the
//! solve (the factorization peak), not a kernel high-water mark.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use burn::tensor::backend::BackendTypes;
use faer::c64;
use sha2::{Digest, Sha256};

use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::{im_z_zero_crossings, s11};
use geode_core::driven::ports::{port_current, port_voltage};
use geode_core::driven::scattering::flux_power_box;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::{
    FR4_MATERIALS, FixtureProvenance, ISM_2G4_BAND_GHZ, PHYS_PATCH, bend_axis_z,
    read_patch_fixture_path,
};
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;
use geode_util::units::{ghz_to_omega_mm, omega_to_ghz_mm};

type B = TestBackend;

/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;

/// Fraction of the air-box half-extent the flux-integration box is shrunk
/// inward from the UPML inner wall (same as the flat patch pipeline).
const FLUX_SHRINK: f64 = 0.10;

/// RSS sampling period during a solve.
const RSS_SAMPLE: Duration = Duration::from_millis(50);

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Default sweep grid (GHz): 20 MHz steps over 2.10–2.18 and 2.62–2.70,
/// 10 MHz steps over 2.20–2.60 (41 points), plus the 2.4835 GHz upper ISM
/// band edge. The dense window starts at 2.20 GHz (issue #1085: the L0
/// resonance near 2.27 GHz fell below the old 2.30 GHz start, so its dip
/// was grid-limited).
fn default_grid() -> Vec<f64> {
    let mut f: Vec<f64> = Vec::new();
    for i in 0..5 {
        f.push(2.10 + 0.02 * i as f64);
    }
    for i in 0..=40 {
        f.push(2.20 + 0.01 * i as f64);
    }
    f.push(ISM_2G4_BAND_GHZ[1]);
    for i in 1..=5 {
        f.push(2.60 + 0.02 * i as f64);
    }
    // Round to 0.1 MHz so the grid prints cleanly.
    let mut f: Vec<f64> = f.iter().map(|v| (v * 1.0e4).round() / 1.0e4).collect();
    f.sort_by(|a, b| a.partial_cmp(b).unwrap());
    f.dedup();
    f
}

/// The sweep grid: `PATCH_CONFORMAL_SWEEP_GHZ` as `"lo,hi,n"` (uniform) or
/// `"list:f1,f2,..."` (explicit, sorted and deduplicated), else
/// [`default_grid`].
fn sweep_grid() -> Vec<f64> {
    match std::env::var("PATCH_CONFORMAL_SWEEP_GHZ") {
        Ok(s) => parse_grid(&s),
        Err(_) => default_grid(),
    }
}

fn parse_grid(spec: &str) -> Vec<f64> {
    let parse = |t: &str| {
        t.trim()
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("PATCH_CONFORMAL_SWEEP_GHZ: {t:?}: {e}"))
    };
    if let Some(list) = spec.strip_prefix("list:") {
        let mut f: Vec<f64> = list.split(',').map(parse).collect();
        assert!(!f.is_empty(), "PATCH_CONFORMAL_SWEEP_GHZ list is empty");
        f.sort_by(|a, b| a.partial_cmp(b).unwrap());
        f.dedup();
        return f;
    }
    let v: Vec<f64> = spec.split(',').map(parse).collect();
    assert_eq!(
        v.len(),
        3,
        "PATCH_CONFORMAL_SWEEP_GHZ = \"lo,hi,n\" or \"list:f1,f2,...\""
    );
    let n = v[2] as usize;
    assert!(n >= 2);
    (0..n)
        .map(|i| v[0] + (v[1] - v[0]) * i as f64 / (n - 1) as f64)
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Max of `ps -o rss=` samples (GiB) while `f` runs.
fn with_peak_rss<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let pid = std::process::id().to_string();
    let stop = Arc::new(AtomicBool::new(false));
    let peak_kib = Arc::new(AtomicU64::new(0));
    let sampler = {
        let stop = Arc::clone(&stop);
        let peak_kib = Arc::clone(&peak_kib);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok(out) = Command::new("ps").args(["-o", "rss=", "-p", &pid]).output()
                    && let Ok(kib) = String::from_utf8_lossy(&out.stdout).trim().parse::<u64>()
                {
                    peak_kib.fetch_max(kib, Ordering::Relaxed);
                }
                std::thread::sleep(RSS_SAMPLE);
            }
        })
    };
    let out = f();
    stop.store(true, Ordering::Relaxed);
    sampler.join().expect("rss sampler");
    let gib = peak_kib.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0);
    (out, gib)
}

struct Row {
    f_ghz: f64,
    omega: f64,
    z_ohm: c64,
    s11: c64,
    p_in: f64,
    p_rad: f64,
    residual_rel: f64,
    solve_s: f64,
    post_s: f64,
    peak_rss_gib: f64,
}

impl Row {
    fn s11_db(&self) -> f64 {
        20.0 * self.s11.norm().log10()
    }
}

fn run_sweep(fixture: &PatchFixture, freqs: &[f64], pml_thick: f64, sigma_0: f64) -> Vec<Row> {
    let edges = fixture.mesh.edges();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[
            fixture.patch_triangles().as_slice(),
            fixture.ground_triangles().as_slice(),
            fixture.outer_boundary_triangles().as_slice(),
        ],
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

    freqs
        .iter()
        .map(|&f_ghz| {
            let omega = ghz_to_omega_mm(f_ghz);
            let t0 = Instant::now();
            let (sol, peak_rss_gib) = with_peak_rss(|| {
                let (eps_t, nu_t) = fixture.matched_upml_materials(
                    &FR4_MATERIALS,
                    air_lo,
                    air_hi,
                    pml_thick,
                    sigma_0,
                    omega,
                );
                driven_solve_with_ports::<B>(
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
                    &device(),
                )
                .unwrap_or_else(|e| panic!("driven solve at {f_ghz} GHz: {e}"))
            });
            let solve_s = t0.elapsed().as_secs_f64();
            let t1 = Instant::now();
            let v = port_voltage(&fixture.mesh, &lp, &edges, &sol.e_edges);
            let i = port_current(&lp, v);
            let z = v / i;
            let refl = s11(z, r_nat);
            assert!(
                refl.norm() <= 1.0 + 1e-9,
                "passivity: |S11| = {} > 1 at {f_ghz} GHz",
                refl.norm()
            );
            let p_in = 0.5 * (v * i.conj()).re;
            let p_rad = flux_power_box(&fixture.mesh, omega, &sol.e_edges, flux_lo, flux_hi);
            let row = Row {
                f_ghz,
                omega,
                z_ohm: z * ETA_0,
                s11: refl,
                p_in,
                p_rad,
                residual_rel: sol.residual_rel,
                solve_s,
                post_s: t1.elapsed().as_secs_f64(),
                peak_rss_gib,
            };
            eprintln!(
                "  f = {f_ghz:.4} GHz: Z = {:8.2} {:+8.2}i ohm, |S11| = {:.4} ({:6.2} dB), \
                 eta = {:.3}, res = {:.1e}, {solve_s:.1} s, {peak_rss_gib:.2} GiB",
                row.z_ohm.re,
                row.z_ohm.im,
                refl.norm(),
                row.s11_db(),
                if p_in != 0.0 { p_rad / p_in } else { 0.0 },
                sol.residual_rel,
            );
            row
        })
        .collect()
}

/// Sweep summary: |S11| dip (3-point parabolic refinement in dB), first
/// Im Z = 0 crossing, −10 dB band, and the ISM-band figures.
struct Summary {
    dip_f_ghz: f64,
    dip_db: f64,
    f_res_imz0_ghz: Option<f64>,
    bw_10db: Option<(f64, f64)>,
    ism_worst_db: f64,
    ism_best_db: f64,
    ism_n_points: usize,
}

fn summarize(rows: &[Row]) -> Summary {
    let i_min = rows
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.s11.norm().partial_cmp(&b.1.s11.norm()).unwrap())
        .map(|(i, _)| i)
        .expect("non-empty sweep");
    let (dip_f_ghz, dip_db) = if i_min > 0 && i_min + 1 < rows.len() {
        let (x0, x1, x2) = (
            rows[i_min - 1].f_ghz,
            rows[i_min].f_ghz,
            rows[i_min + 1].f_ghz,
        );
        let (y0, y1, y2) = (
            rows[i_min - 1].s11_db(),
            rows[i_min].s11_db(),
            rows[i_min + 1].s11_db(),
        );
        let d01 = (y1 - y0) / (x1 - x0);
        let d12 = (y2 - y1) / (x2 - x1);
        let a = (d12 - d01) / (x2 - x0);
        if a > 0.0 {
            let xv = 0.5 * (x0 + x1) - d01 / (2.0 * a);
            let yv = y1 + d01 * (xv - x1) + a * (xv - x0) * (xv - x1);
            (xv, yv)
        } else {
            (x1, y1)
        }
    } else {
        (rows[i_min].f_ghz, rows[i_min].s11_db())
    };

    let omegas: Vec<f64> = rows.iter().map(|r| r.omega).collect();
    let zs: Vec<c64> = rows.iter().map(|r| r.z_ohm).collect();
    // The Im Z = 0 crossing nearest the |S11| dip (an inductive-to-capacitive
    // crossing of the TM010 resonance).
    let f_res_imz0_ghz = im_z_zero_crossings(&omegas, &zs)
        .into_iter()
        .map(omega_to_ghz_mm)
        .min_by(|a, b| {
            (a - dip_f_ghz)
                .abs()
                .partial_cmp(&(b - dip_f_ghz).abs())
                .unwrap()
        });

    let thresh = 0.1_f64.sqrt();
    let bw_10db = if rows[i_min].s11.norm() <= thresh {
        let cross = |i_in: usize, i_out: usize| {
            let (f0, m0) = (rows[i_in].f_ghz, rows[i_in].s11.norm());
            let (f1, m1) = (rows[i_out].f_ghz, rows[i_out].s11.norm());
            f0 + (f1 - f0) * (thresh - m0) / (m1 - m0)
        };
        let lo = (0..i_min)
            .rev()
            .find(|&i| rows[i].s11.norm() >= thresh)
            .map(|i| cross(i + 1, i));
        let hi = ((i_min + 1)..rows.len())
            .find(|&i| rows[i].s11.norm() >= thresh)
            .map(|i| cross(i - 1, i));
        lo.zip(hi)
    } else {
        None
    };

    let ism: Vec<&Row> = rows
        .iter()
        .filter(|r| r.f_ghz >= ISM_2G4_BAND_GHZ[0] - 1e-9 && r.f_ghz <= ISM_2G4_BAND_GHZ[1] + 1e-9)
        .collect();
    let ism_worst_db = ism
        .iter()
        .map(|r| r.s11_db())
        .fold(f64::NEG_INFINITY, f64::max);
    let ism_best_db = ism.iter().map(|r| r.s11_db()).fold(f64::INFINITY, f64::min);
    Summary {
        dip_f_ghz,
        dip_db,
        f_res_imz0_ghz,
        bw_10db,
        ism_worst_db,
        ism_best_db,
        ism_n_points: ism.len(),
    }
}

fn push_summary(t: &mut String, name: &str, s: &Summary) {
    let _ = writeln!(t, "[summary.{name}]");
    let _ = writeln!(t, "s11_dip_f_ghz = {:.6}", s.dip_f_ghz);
    let _ = writeln!(t, "s11_dip_db = {:.4}", s.dip_db);
    match s.f_res_imz0_ghz {
        Some(f) => {
            let _ = writeln!(t, "f_res_imz0_ghz = {f:.6}");
        }
        None => t.push_str("# f_res_imz0_ghz: no Im Z = 0 crossing in the sweep\n"),
    }
    match s.bw_10db {
        Some((lo, hi)) => {
            let _ = writeln!(t, "bw_10db_lo_ghz = {lo:.6}");
            let _ = writeln!(t, "bw_10db_hi_ghz = {hi:.6}");
            let _ = writeln!(t, "bw_10db_mhz = {:.3}", (hi - lo) * 1e3);
        }
        None => t.push_str("# bw_10db_*: the dip does not reach -10 dB or is not bracketed\n"),
    }
    let _ = writeln!(t, "ism_band_points = {}", s.ism_n_points);
    if s.ism_n_points == 0 {
        // A custom grid with no ISM point: the fold identities are ±inf,
        // which would print as `inf` / `-inf` (invalid TOML).
        t.push_str("# ism_worst_s11_db / ism_best_s11_db: no sweep point in the ISM band\n");
    } else {
        let _ = writeln!(t, "ism_worst_s11_db = {:.4}", s.ism_worst_db);
        let _ = writeln!(t, "ism_best_s11_db = {:.4}", s.ism_best_db);
    }
    t.push('\n');
}

fn push_rows(t: &mut String, name: &str, rows: &[Row]) {
    for r in rows {
        let _ = writeln!(t, "[[sweep.{name}]]");
        let _ = writeln!(t, "f_ghz = {:.4}", r.f_ghz);
        let _ = writeln!(t, "omega_rad_per_mm = {:.9e}", r.omega);
        let _ = writeln!(t, "z_re_ohm = {:.6e}", r.z_ohm.re);
        let _ = writeln!(t, "z_im_ohm = {:.6e}", r.z_ohm.im);
        let _ = writeln!(t, "s11_re = {:.9e}", r.s11.re);
        let _ = writeln!(t, "s11_im = {:.9e}", r.s11.im);
        let _ = writeln!(t, "s11_mag = {:.9e}", r.s11.norm());
        let _ = writeln!(t, "s11_db = {:.6}", r.s11_db());
        let _ = writeln!(t, "p_in = {:.6e}", r.p_in);
        let _ = writeln!(t, "p_rad = {:.6e}", r.p_rad);
        let eta = if r.p_in != 0.0 { r.p_rad / r.p_in } else { 0.0 };
        let _ = writeln!(t, "efficiency = {eta:.6e}");
        let _ = writeln!(t, "solve_residual_rel = {:.3e}", r.residual_rel);
        let _ = writeln!(t, "solve_s = {:.3}", r.solve_s);
        let _ = writeln!(t, "post_s = {:.3}", r.post_s);
        let _ = writeln!(t, "peak_rss_gib = {:.3}", r.peak_rss_gib);
        t.push('\n');
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[allow(clippy::too_many_lines)]
fn main() {
    let bent_path: PathBuf = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("PATCH_CONFORMAL_FIXTURE").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/patch_conformal_L0.msh")
        });
    let stem = bent_path
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("fixture file stem")
        .to_string();
    let dir = bent_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let flat_path = std::env::var("PATCH_CONFORMAL_FLAT_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dir.join(format!("{stem}.flat.msh")));
    let prov_path = dir.join(format!("{stem}.provenance.txt"));

    let prov = FixtureProvenance::read(&prov_path)
        .unwrap_or_else(|e| panic!("read provenance {}: {e}", prov_path.display()));
    let bent_bytes = fs::read(&bent_path)
        .unwrap_or_else(|e| panic!("read bent fixture {}: {e}", bent_path.display()));
    let flat_bytes = fs::read(&flat_path).unwrap_or_else(|e| {
        panic!(
            "read flat companion {}: {e}. Regenerate it with \
             reference/gmsh/generate_patch_fixture.py (it writes <stem>.flat.msh)",
            flat_path.display()
        )
    });
    let bent_sha = sha256_hex(&bent_bytes);
    let flat_sha = sha256_hex(&flat_bytes);
    assert_eq!(
        Some(bent_sha.as_str()),
        prov.top("sha256"),
        "bent mesh sha256 does not match {}",
        prov_path.display()
    );
    assert_eq!(
        Some(flat_sha.as_str()),
        prov.top("flat sha256"),
        "flat mesh sha256 does not match {}",
        prov_path.display()
    );
    drop(bent_bytes);
    drop(flat_bytes);
    let bent = read_patch_fixture_path(&bent_path).expect("bent fixture");
    let flat = read_patch_fixture_path(&flat_path).expect("flat fixture");
    assert_eq!(bent.mesh.tets, flat.mesh.tets, "bent/flat topology differ");
    assert_eq!(bent.tet_physical_tags, flat.tet_physical_tags);
    assert_eq!(bent.boundary_triangles, flat.boundary_triangles);

    let bend_radius = prov.param("bend.bend_radius");
    let h = prov.param("patch.h");
    let pml_thick = prov.param("domain.pml_thick");
    // `PATCH_CONFORMAL_SIGMA_0` overrides the fixture's UPML strength for
    // absorber-sensitivity runs; the value used is recorded in [meta].
    let sigma_0_fixture = prov.param("solver.upml_sigma_0");
    let sigma_0 = std::env::var("PATCH_CONFORMAL_SIGMA_0")
        .ok()
        .map(|s| s.parse::<f64>().expect("PATCH_CONFORMAL_SIGMA_0"))
        .unwrap_or(sigma_0_fixture);
    let r_port = prov.param("solver.port_resistance_ohm");
    assert_eq!(r_port, R_PORT_OHM, "port resistance");
    assert_eq!(
        prov.param("materials.substrate_eps_r"),
        FR4_MATERIALS.eps_r_substrate
    );
    assert_eq!(
        prov.param("materials.substrate_tan_delta"),
        FR4_MATERIALS.tan_delta_substrate
    );

    // The curved patch lies on its arc (radius R + h/2 about the bend axis).
    let z_axis = bend_axis_z(h, bend_radius);
    let mut arc_resid = 0.0_f64;
    for tri in bent.triangles_with_tag(PHYS_PATCH) {
        for n in tri {
            let p = bent.mesh.nodes[n as usize];
            let r = (p[0] * p[0] + (p[2] - z_axis) * (p[2] - z_axis)).sqrt();
            arc_resid = arc_resid.max((r - (bend_radius + 0.5 * h)).abs());
        }
    }
    assert!(arc_resid < 1e-9, "patch off its arc by {arc_resid:.3e} mm");

    let n_edges = bent.mesh.edges().len();
    let n_nodes = bent.mesh.n_nodes();
    let n_tets = bent.mesh.n_tets();
    let freqs = sweep_grid();
    eprintln!(
        "patch_conformal_2g4_baseline: {stem}: {n_edges} edges, {n_nodes} nodes, {n_tets} tets; \
         R = {bend_radius} mm, UPML {pml_thick} mm / sigma_0 = {sigma_0}; {} frequencies",
        freqs.len()
    );

    let t_all = Instant::now();
    eprintln!("flat sweep:");
    let flat_rows = run_sweep(&flat, &freqs, pml_thick, sigma_0);
    eprintln!("bent sweep:");
    let bent_rows = run_sweep(&bent, &freqs, pml_thick, sigma_0);
    let total_s = t_all.elapsed().as_secs_f64();
    let flat_sum = summarize(&flat_rows);
    let bent_sum = summarize(&bent_rows);

    let backend = BackendInfo::of::<B>(&device());
    let commit = geode_util::repo::current_commit();
    let mut t = String::with_capacity(32 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example \\\n");
    t.push_str("#   patch_conformal_2g4_baseline -- <fixture.msh>`.\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_conformal_2g4_fixture.rs`.\n\n");

    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Curved conformal 2.4 GHz patch fixture (issue #1084, conformal v5 E1, \
         operator choice (c)): pre-optimization S11 sweep of the bent patch and of the flat mesh \
         it was bent from (same topology), so the difference is the bending detune alone.\"\n",
    );
    let _ = writeln!(t, "fixture = \"{stem}\"");
    let _ = writeln!(t, "fixture_yaml = \"reference/gmsh/{}\"", {
        prov.top("parameters")
            .map_or_else(|| format!("{stem}.yaml"), str::to_string)
    });
    let _ = writeln!(t, "fixture_sha256 = \"{bent_sha}\"");
    let _ = writeln!(t, "flat_fixture_sha256 = \"{flat_sha}\"");
    let _ = writeln!(
        t,
        "gmsh_version = \"{}\"",
        prov.top("gmsh version").unwrap_or_default()
    );
    let _ = writeln!(t, "generated_at_commit = \"{commit}\"");
    backend.push_meta(&mut t);
    t.push_str("length_unit = \"mm\"\n");
    t.push_str("frequency_unit = \"GHz\"\n");
    t.push_str(
        "omega_convention = \"omega = k0 = 2 pi f / c in rad/mm, c = 299792458e3 mm/s \
         (geode_util::units::ghz_to_omega_mm)\"\n",
    );
    let _ = writeln!(
        t,
        "band_f_ghz = [{}, {}]",
        ISM_2G4_BAND_GHZ[0], ISM_2G4_BAND_GHZ[1]
    );
    let _ = writeln!(
        t,
        "band_omega_rad_per_mm = [{:.9e}, {:.9e}]",
        ghz_to_omega_mm(ISM_2G4_BAND_GHZ[0]),
        ghz_to_omega_mm(ISM_2G4_BAND_GHZ[1])
    );
    let _ = writeln!(t, "port_resistance_ohm = {R_PORT_OHM}");
    let _ = writeln!(t, "port_resistance_nat = {:.9e}", R_PORT_OHM / ETA_0);
    let _ = writeln!(t, "eta_0_ohm = {ETA_0:.9e}");
    let _ = writeln!(t, "eps_r = {}", FR4_MATERIALS.eps_r_substrate);
    let _ = writeln!(t, "tan_delta = {}", FR4_MATERIALS.tan_delta_substrate);
    let _ = writeln!(t, "bend_radius_mm = {bend_radius}");
    let _ = writeln!(
        t,
        "bend_axis = \"parallel to y through (x = 0, z = {z_axis}) mm\""
    );
    let _ = writeln!(t, "patch_arc_residual_mm = {arc_resid:.3e}");
    for (k, key) in [
        ("patch_w_mm", "patch.patch_w"),
        ("patch_l_mm", "patch.patch_l"),
        ("substrate_h_mm", "patch.h"),
        ("sub_pad_mm", "patch.sub_pad"),
        ("probe_w_mm", "patch.probe_w"),
        ("probe_inset_mm", "patch.probe_inset"),
        ("air_margin_mm", "domain.air_margin"),
        ("upml_thick_mm", "domain.pml_thick"),
    ] {
        let _ = writeln!(t, "{k} = {}", prov.param(key));
    }
    let _ = writeln!(t, "upml_sigma_0 = {sigma_0}");
    let _ = writeln!(t, "upml_sigma_0_fixture = {sigma_0_fixture}");
    t.push_str(
        "upml_profile = \"s = 1 - j sigma_0 (d/w)^2 / omega per axis (box UPML), omega in rad/mm\"\n",
    );
    let _ = writeln!(t, "conductors = \"pec\"");
    let _ = writeln!(t, "n_edges = {n_edges}");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    t.push('\n');

    t.push_str("[meta.mesh_sizing_mm]\n");
    for key in [
        "lc_patch", "lc_sub", "lc_port", "lc_far", "dist_far", "lc_air", "lc_pml",
    ] {
        if let Some(v) = prov.param_str(&format!("mesh.{key}")) {
            let _ = writeln!(t, "{key} = {v}");
        }
    }
    t.push('\n');
    t.push_str("[meta.resolution]\n");
    t.push_str("# Copied from the generator provenance (lengths in mm).\n");
    for (k, v) in prov.resolution() {
        let _ = writeln!(t, "\"{k}\" = \"{v}\"");
    }
    t.push('\n');

    push_summary(&mut t, "flat", &flat_sum);
    push_summary(&mut t, "bent", &bent_sum);

    t.push_str("[detune]\n");
    t.push_str("# bent minus flat, same mesh topology.\n");
    let dip_shift = bent_sum.dip_f_ghz - flat_sum.dip_f_ghz;
    let _ = writeln!(t, "s11_dip_shift_mhz = {:.3}", dip_shift * 1e3);
    let _ = writeln!(
        t,
        "s11_dip_shift_rel = {:.6e}",
        dip_shift / flat_sum.dip_f_ghz
    );
    let _ = writeln!(
        t,
        "s11_dip_depth_change_db = {:.4}",
        bent_sum.dip_db - flat_sum.dip_db
    );
    if let (Some(a), Some(b)) = (flat_sum.f_res_imz0_ghz, bent_sum.f_res_imz0_ghz) {
        let _ = writeln!(t, "f_res_imz0_shift_mhz = {:.3}", (b - a) * 1e3);
    }
    if flat_sum.ism_n_points > 0 && bent_sum.ism_n_points > 0 {
        let _ = writeln!(
            t,
            "ism_worst_s11_change_db = {:.4}",
            bent_sum.ism_worst_db - flat_sum.ism_worst_db
        );
    } else {
        // ±inf − ±inf would print `NaN` here (issue #1085 nit).
        t.push_str("# ism_worst_s11_change_db: no sweep point in the ISM band\n");
    }
    t.push('\n');

    let cost = |rows: &[Row]| {
        let n = rows.len() as f64;
        let mean = rows.iter().map(|r| r.solve_s).sum::<f64>() / n;
        let max = rows.iter().map(|r| r.solve_s).fold(0.0, f64::max);
        let rss = rows.iter().map(|r| r.peak_rss_gib).fold(0.0, f64::max);
        (mean, max, rss)
    };
    let (fm, fx, fr) = cost(&flat_rows);
    let (bm, bx, br) = cost(&bent_rows);
    let first_rss = flat_rows.first().map_or(0.0, |r| r.peak_rss_gib);
    t.push_str("[cost]\n");
    t.push_str(
        "# Per solve = per-frequency UPML tensor build + assembly + sparse LU + solve. RSS = max of \
         ps -o rss= samples every 50 ms during the solve.\n",
    );
    let _ = writeln!(
        t,
        "host = \"{}\"",
        std::env::var("PATCH_CONFORMAL_HOST").unwrap_or_else(|_| "unrecorded".into())
    );
    let _ = writeln!(t, "n_solves = {}", flat_rows.len() + bent_rows.len());
    let _ = writeln!(t, "first_solve_peak_rss_gib = {first_rss:.3}");
    t.push_str(
        "# first_solve_peak_rss_gib is the single-solve footprint. Later per-point peaks include \
         heap the allocator retains between solves (it plateaus and falls back; not a leak), so \
         the *_peak_rss_gib maxima below bound the whole sweep, not one solve.\n",
    );
    let _ = writeln!(t, "flat_solve_s_mean = {fm:.3}");
    let _ = writeln!(t, "flat_solve_s_max = {fx:.3}");
    let _ = writeln!(t, "flat_peak_rss_gib = {fr:.3}");
    let _ = writeln!(t, "bent_solve_s_mean = {bm:.3}");
    let _ = writeln!(t, "bent_solve_s_max = {bx:.3}");
    let _ = writeln!(t, "bent_peak_rss_gib = {br:.3}");
    let post_mean = flat_rows
        .iter()
        .chain(bent_rows.iter())
        .map(|r| r.post_s)
        .sum::<f64>()
        / (flat_rows.len() + bent_rows.len()) as f64;
    let _ = writeln!(
        t,
        "post_s_mean = {post_mean:.3}  # port readback + flux_power_box, outside solve_s"
    );
    let _ = writeln!(t, "total_s = {total_s:.1}");
    t.push('\n');

    push_rows(&mut t, "flat", &flat_rows);
    push_rows(&mut t, "bent", &bent_rows);

    let out = std::env::var("PATCH_CONFORMAL_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let level = stem.strip_prefix("patch_conformal_").unwrap_or(&stem);
            repo_root()
                .join("benchmarks/patch_antenna_conformal")
                .join(format!("baseline_2g4_{level}.toml"))
        });
    if let Some(p) = out.parent() {
        fs::create_dir_all(p).expect("create output dir");
    }
    fs::write(&out, &t).expect("write baseline toml");
    eprintln!(
        "flat: dip {:.2} dB at {:.4} GHz, ISM worst {:.2} dB | bent: dip {:.2} dB at {:.4} GHz, \
         ISM worst {:.2} dB | detune {:+.2} MHz",
        flat_sum.dip_db,
        flat_sum.dip_f_ghz,
        flat_sum.ism_worst_db,
        bent_sum.dip_db,
        bent_sum.dip_f_ghz,
        bent_sum.ism_worst_db,
        dip_shift * 1e3
    );
    println!("wrote {}", out.display());
}
