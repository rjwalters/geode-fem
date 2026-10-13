//! **Mesh-refinement and UPML study of the curved conformal 2.4 GHz patch**
//! (issue #1085, conformal v5 E2).
//!
//! Companion of `patch_conformal_2g4_baseline` (which runs the dense flat /
//! bent S₁₁ sweep of one level). This example answers the questions the
//! dense sweep cannot: *where* the initial-geometry S₁₁ converges under
//! h-refinement (L0 → L3) and p-refinement (p = 2 over a level), how much
//! of it is the absorber, what each level costs, and whether the shape
//! gradient survives a finite-difference check at the study level. Four
//! sub-commands, each writing one TOML artifact under
//! `benchmarks/patch_antenna_conformal/`:
//!
//! ```text
//!   patch_conformal_convergence dip  <bent.msh> [--p2] [--out <toml>]
//!   patch_conformal_convergence fd   <bent.msh> [--out <toml>]
//!   patch_conformal_convergence merge <flat-dip.toml> <bent-dip.toml> [--out <toml>]
//!   patch_conformal_convergence ladder <run-dir> [--out <toml>]
//!   patch_conformal_convergence aggregate [--dir <benchmarks dir>] [--out <toml>]
//! ```
//!
//! * **`dip`** — the |S₁₁| dip of the flat and the bent geometry located
//!   by a coarse pre-scan followed by a golden-section search in frequency
//!   (default: 7 points over 2.20–2.38 GHz, then to 0.2 MHz), plus S₁₁ at a
//!   list of fixed frequencies (default 2.28, 2.31, 2.40, 2.4835 GHz). At
//!   p = 1 the solve is [`DrivenOperator::assemble_with_space`]'s p = 1 arm
//!   (bit-identical to `driven_solve_with_ports`); with `--p2` it is the
//!   second-order Nédélec space over the same mesh. Per-solve wall-clock and
//!   peak RSS are recorded.
//! * **`fd`** — the finite-difference gate of the composed open-radiator
//!   shape gradient at `X = 0`: the radial-normal freeform patch DOFs with
//!   the harmonic mesh-morph regularizer (the `patch_conformal_diffopt`
//!   parametrization, rebuilt here on the 2.4 GHz fixture), the ISM-band
//!   objective `G = Σ_f w_f |S₁₁(f)|²` at the band edges and centre, the
//!   adjoint gradient (one LU per frequency), and a central difference
//!   along `N` seeded random unit directions (default 3). Forward and
//!   gradient wall-clock and RSS are the per-level gradient cost.
//! * **`merge`** — joins a flat-only and a bent-only `dip` artifact of one
//!   level and order (`PATCH_CONFORMAL_GEOMS=flat` / `bent`, run separately
//!   when one geometry at a time is what a host can hold) into the
//!   two-geometry artifact `dip` writes in one go; `[meta]` must agree
//!   apart from `geometries` and `host`, and `[detune]` / `[cost]` are
//!   re-derived.
//! * **`ladder`** — aggregates the σ₀ × shell ladder: the
//!   `patch_conformal_2g4_baseline` runs in `<run-dir>` named
//!   `<shell>_sigma<σ₀>.toml` (shells `patch_conformal_L1`,
//!   `patch_conformal_L1_pml5`, `patch_conformal_L1_pml40`; σ₀ ∈ {0.5, 1,
//!   2, 4, 8, 25}) into one table with the dip, the fixed-frequency S₁₁,
//!   the complex-S₁₁ distance between neighbouring settings, and the
//!   chosen (most stationary) σ₀ with its residual sensitivity.
//! * **`aggregate`** — collects the `dip_*.toml` / `fd_*.toml` level
//!   artifacts, the dense baselines and the ladder into `convergence.toml`:
//!   the S₁₁ convergence table, the observed convergence order of the dip
//!   frequency and depth (least-squares slope of consecutive differences
//!   against the DOF-based `h ∝ N⁻¹ᐟ³`), the Richardson extrapolation from
//!   the finest pair, the per-level cost table and the FD gate.
//!
//! # Units
//!
//! Lengths in mm, frequencies in GHz (`ω = k₀ = 2π f / c` in rad/mm inside
//! the solver), impedances in Ω; the 50 Ω port enters as `50 / η₀`.
//!
//! # Environment
//!
//! * `PATCH_CONFORMAL_SIGMA_0` — UPML σ₀ override (default: the fixture's
//!   provenance value). Recorded in `[meta]`.
//! * `PATCH_CONFORMAL_GEOMS` — `flat,bent` (default) or one of them.
//! * `PATCH_CONFORMAL_DIP_SCAN="lo,hi,n"` — pre-scan grid (default
//!   `2.20,2.38,7`); `PATCH_CONFORMAL_DIP_TOL_MHZ` — golden-section
//!   bracket width to stop at (default 0.2).
//! * `PATCH_CONFORMAL_FIXED_GHZ="f1,f2,..."` — fixed frequencies.
//! * `PATCH_CONFORMAL_FD_DIRS`, `PATCH_CONFORMAL_FD_STEP`,
//!   `PATCH_CONFORMAL_FD_SEED` — FD directions (3), step (1e-4 design
//!   units) and seed (0x1085).
//! * `PATCH_CONFORMAL_HOST` — free-text host description for `[cost]`.
//!
//! The flat companion `<stem>.flat.msh` and `<stem>.provenance.txt` are
//! read next to the bent mesh, exactly as the baseline example does, and
//! both sha256s are checked against the provenance.

use std::collections::BTreeMap;
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

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::{s11, s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::{driven_shape_gradient_matched_upml_ports, pml_shell_nodes};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource,
    driven_solve_with_ports,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::patch::{
    FR4_MATERIALS, FixtureProvenance, ISM_2G4_BAND_GHZ, PHYS_UPML, PatchPort,
    read_patch_fixture_path,
};
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles};
use geode_core::shape::{BoundaryMotionDof, FreeformBoundaryMorph};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;
use geode_util::units::ghz_to_omega_mm;

type B = TestBackend;

/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;

/// RSS sampling period during a solve.
const RSS_SAMPLE: Duration = Duration::from_millis(50);

/// Per-DOF radial-normal motion per unit design value, as a fraction of the
/// substrate thickness (the `patch_conformal_diffopt` convention).
const DIR_SCALE_FRAC: f64 = 0.5;

/// FD gate tolerance on the relative error of the directional derivative.
const FD_TOL: f64 = 5.0e-3;

/// The ladder's shell variants, in file-stem order, with their YAML.
const LADDER_SHELLS: [(&str, &str); 3] = [
    ("patch_conformal_L1", "patch_conformal_L1.yaml"),
    ("patch_conformal_L1_pml5", "patch_conformal_L1_pml5.yaml"),
    ("patch_conformal_L1_pml40", "patch_conformal_L1_pml40.yaml"),
];

/// The ladder's σ₀ values, as written in the run file names.
const LADDER_SIGMAS: [&str; 6] = ["0.5", "1", "2", "4", "8", "25"];

/// A σ₀ is on the plateau when its stationarity score is within this factor
/// of the smallest score in the ladder.
const LADDER_PLATEAU_FACTOR: f64 = 2.0;

/// The ladder's fixed frequencies (GHz) reported in the table (a subset of
/// the 12-point list grid the runs were made on).
const LADDER_REPORT_GHZ: [f64; 4] = [2.28, 2.31, 2.40, 2.4835];

/// The level stems of the h-ladder, finest last.
const LEVELS: [&str; 4] = ["L0", "L1", "L2", "L3"];

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .map(|s| s.parse::<f64>().unwrap_or_else(|e| panic!("{key}: {e}")))
        .unwrap_or(default)
}

fn env_list(key: &str, default: &[f64]) -> Vec<f64> {
    match std::env::var(key) {
        Ok(s) => s
            .split(',')
            .map(|t| {
                t.trim()
                    .parse::<f64>()
                    .unwrap_or_else(|e| panic!("{key}: {t:?}: {e}"))
            })
            .collect(),
        Err(_) => default.to_vec(),
    }
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
    (
        out,
        peak_kib.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
    )
}

// ---------------------------------------------------------------------------
// Fixture loading (bent + flat companion + provenance), shared by `dip` / `fd`.
// ---------------------------------------------------------------------------

struct Loaded {
    stem: String,
    level: String,
    bent: PatchFixture,
    flat: PatchFixture,
    bent_sha: String,
    flat_sha: String,
    prov: FixtureProvenance,
    pml_thick: f64,
    sigma_0: f64,
    sigma_0_fixture: f64,
}

fn load(bent_path: &Path) -> Loaded {
    let stem = bent_path
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("fixture file stem")
        .to_string();
    let dir = bent_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let flat_path = dir.join(format!("{stem}.flat.msh"));
    let prov_path = dir.join(format!("{stem}.provenance.txt"));
    let prov = FixtureProvenance::read(&prov_path)
        .unwrap_or_else(|e| panic!("read provenance {}: {e}", prov_path.display()));
    let bent_bytes =
        fs::read(bent_path).unwrap_or_else(|e| panic!("read {}: {e}", bent_path.display()));
    let flat_bytes =
        fs::read(&flat_path).unwrap_or_else(|e| panic!("read {}: {e}", flat_path.display()));
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
    drop((bent_bytes, flat_bytes));
    let bent = read_patch_fixture_path(bent_path).expect("bent fixture");
    let flat = read_patch_fixture_path(&flat_path).expect("flat fixture");
    assert_eq!(bent.mesh.tets, flat.mesh.tets, "bent/flat topology differ");
    assert_eq!(prov.param("solver.port_resistance_ohm"), R_PORT_OHM);
    assert_eq!(
        prov.param("materials.substrate_eps_r"),
        FR4_MATERIALS.eps_r_substrate
    );
    assert_eq!(
        prov.param("materials.substrate_tan_delta"),
        FR4_MATERIALS.tan_delta_substrate
    );
    let sigma_0_fixture = prov.param("solver.upml_sigma_0");
    let level = stem
        .strip_prefix("patch_conformal_")
        .unwrap_or(&stem)
        .to_string();
    Loaded {
        level,
        stem,
        bent,
        flat,
        bent_sha,
        flat_sha,
        pml_thick: prov.param("domain.pml_thick"),
        sigma_0: env_f64("PATCH_CONFORMAL_SIGMA_0", sigma_0_fixture),
        sigma_0_fixture,
        prov,
    }
}

fn push_meta_common(t: &mut String, l: &Loaded, geoms: &[&str]) {
    let backend = BackendInfo::of::<B>(&device());
    let _ = writeln!(t, "fixture = \"{}\"", l.stem);
    let _ = writeln!(t, "level = \"{}\"", l.level);
    let _ = writeln!(
        t,
        "fixture_yaml = \"reference/gmsh/{}\"",
        l.prov
            .top("parameters")
            .map_or_else(|| format!("{}.yaml", l.stem), str::to_string)
    );
    let _ = writeln!(t, "fixture_sha256 = \"{}\"", l.bent_sha);
    let _ = writeln!(t, "flat_fixture_sha256 = \"{}\"", l.flat_sha);
    let _ = writeln!(
        t,
        "gmsh_version = \"{}\"",
        l.prov.top("gmsh version").unwrap_or_default()
    );
    let _ = writeln!(
        t,
        "generated_at_commit = \"{}\"",
        geode_util::repo::current_commit()
    );
    backend.push_meta(t);
    t.push_str("length_unit = \"mm\"\nfrequency_unit = \"GHz\"\nimpedance_unit = \"ohm\"\n");
    t.push_str(
        "omega_convention = \"omega = k0 = 2 pi f / c in rad/mm, c = 299792458e3 mm/s \
         (geode_util::units::ghz_to_omega_mm)\"\n",
    );
    let _ = writeln!(
        t,
        "band_f_ghz = [{}, {}]",
        ISM_2G4_BAND_GHZ[0], ISM_2G4_BAND_GHZ[1]
    );
    let _ = writeln!(t, "port_resistance_ohm = {R_PORT_OHM}");
    let _ = writeln!(t, "eps_r = {}", FR4_MATERIALS.eps_r_substrate);
    let _ = writeln!(t, "tan_delta = {}", FR4_MATERIALS.tan_delta_substrate);
    let _ = writeln!(t, "bend_radius_mm = {}", l.prov.param("bend.bend_radius"));
    let _ = writeln!(t, "air_margin_mm = {}", l.prov.param("domain.air_margin"));
    let _ = writeln!(t, "upml_thick_mm = {}", l.pml_thick);
    let _ = writeln!(t, "upml_sigma_0 = {}", l.sigma_0);
    let _ = writeln!(t, "upml_sigma_0_fixture = {}", l.sigma_0_fixture);
    t.push_str(
        "upml_profile = \"s = 1 - j sigma_0 (d/w)^2 / omega per axis (box UPML), omega in rad/mm\"\n",
    );
    let _ = writeln!(
        t,
        "geometries = [{}]",
        geoms
            .iter()
            .map(|g| format!("\"{g}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "n_edges = {}", l.bent.mesh.edges().len());
    let _ = writeln!(t, "n_nodes = {}", l.bent.mesh.n_nodes());
    let _ = writeln!(t, "n_tets = {}", l.bent.mesh.n_tets());
    let _ = writeln!(
        t,
        "host = \"{}\"",
        std::env::var("PATCH_CONFORMAL_HOST").unwrap_or_else(|_| "unrecorded".into())
    );
    t.push('\n');
    t.push_str("[meta.mesh_sizing_mm]\n");
    for key in [
        "lc_patch", "lc_sub", "lc_port", "lc_far", "dist_far", "lc_air", "lc_pml",
    ] {
        if let Some(v) = l.prov.param_str(&format!("mesh.{key}")) {
            let _ = writeln!(t, "{key} = {v}");
        }
    }
    t.push('\n');
    t.push_str("[meta.resolution]\n");
    t.push_str("# Copied from the generator provenance (lengths in mm).\n");
    for (k, v) in l.prov.resolution() {
        let _ = writeln!(t, "\"{k}\" = \"{v}\"");
    }
    t.push('\n');
}

fn geoms() -> Vec<String> {
    let s = std::env::var("PATCH_CONFORMAL_GEOMS").unwrap_or_else(|_| "flat,bent".into());
    let v: Vec<String> = s.split(',').map(|g| g.trim().to_string()).collect();
    for g in &v {
        assert!(g == "flat" || g == "bent", "PATCH_CONFORMAL_GEOMS: {g:?}");
    }
    v
}

// ---------------------------------------------------------------------------
// One port-driven solve at either order.
// ---------------------------------------------------------------------------

struct Point {
    f_ghz: f64,
    z_ohm: c64,
    s11: c64,
    residual_rel: f64,
    solve_s: f64,
    peak_rss_gib: f64,
}

impl Point {
    fn db(&self) -> f64 {
        20.0 * self.s11.norm().log10()
    }
}

/// The ω-independent part of a port-driven solve on one geometry.
struct Solver<'a> {
    fixture: &'a PatchFixture,
    space: HcurlSpace,
    mask: Vec<bool>,
    port: PatchPort,
    source: CurrentSource,
    air_lo: [f64; 3],
    air_hi: [f64; 3],
    pml_thick: f64,
    sigma_0: f64,
    n_interior: usize,
}

impl<'a> Solver<'a> {
    fn new(fixture: &'a PatchFixture, order: ElementOrder, pml_thick: f64, sigma_0: f64) -> Self {
        let space = HcurlSpace::build(&fixture.mesh, order);
        let patch = fixture.patch_triangles();
        let ground = fixture.ground_triangles();
        let outer = fixture.outer_boundary_triangles();
        let mask = space
            .pec_interior_mask(
                &fixture.mesh,
                &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
            )
            .expect("PEC mask");
        let n_interior = mask.iter().filter(|&&m| m).count();
        let (air_lo, air_hi) = fixture.air_box(pml_thick);
        Solver {
            fixture,
            space,
            mask,
            port: fixture.port(),
            source: CurrentSource {
                j_tet: vec![[c64::new(0.0, 0.0); 3]; fixture.mesh.n_tets()],
            },
            air_lo,
            air_hi,
            pml_thick,
            sigma_0,
            n_interior,
        }
    }

    fn solve(&self, f_ghz: f64) -> Point {
        let omega = ghz_to_omega_mm(f_ghz);
        let r_nat = R_PORT_OHM / ETA_0;
        let lp = self.port.lumped_port(r_nat, c64::new(1.0, 0.0));
        let t0 = Instant::now();
        let ((v, residual_rel), peak_rss_gib) = with_peak_rss(|| {
            let (eps_t, nu_t) = self.fixture.matched_upml_materials(
                &FR4_MATERIALS,
                self.air_lo,
                self.air_hi,
                self.pml_thick,
                self.sigma_0,
                omega,
            );
            let op = DrivenOperator::assemble_with_space::<B>(
                &self.space,
                &self.fixture.mesh,
                DrivenMaterials::MatchedUpml {
                    epsilon_tensor: &eps_t,
                    nu_tensor: &nu_t,
                },
                None,
                &DrivenBcs {
                    pec_interior_mask: &self.mask,
                },
                std::slice::from_ref(&lp),
                &[],
                DrivenSource::Constant(&self.source),
                &device(),
            )
            .unwrap_or_else(|e| panic!("assemble at {f_ghz} GHz: {e}"));
            let sol = op
                .solve_at(omega)
                .unwrap_or_else(|e| panic!("solve at {f_ghz} GHz: {e}"));
            (op.port_voltage(0, &sol.e_edges), sol.residual_rel)
        });
        let solve_s = t0.elapsed().as_secs_f64();
        let i = (c64::new(2.0, 0.0) - v) / r_nat;
        let z = v / i;
        let refl = s11(z, r_nat);
        assert!(
            refl.norm() <= 1.0 + 1e-9,
            "passivity: |S11| = {} > 1 at {f_ghz} GHz",
            refl.norm()
        );
        let p = Point {
            f_ghz,
            z_ohm: z * ETA_0,
            s11: refl,
            residual_rel,
            solve_s,
            peak_rss_gib,
        };
        eprintln!(
            "  f = {f_ghz:.5} GHz: Z = {:8.2} {:+8.2}i ohm, |S11| = {:.5} ({:7.3} dB), res = {:.1e}, \
             {solve_s:.1} s, {peak_rss_gib:.2} GiB",
            p.z_ohm.re,
            p.z_ohm.im,
            refl.norm(),
            p.db(),
            residual_rel
        );
        p
    }
}

// ---------------------------------------------------------------------------
// `dip`
// ---------------------------------------------------------------------------

struct Dip {
    f_ghz: f64,
    db: f64,
    /// The sampled minimum (before the parabolic refinement).
    sampled_f_ghz: f64,
    sampled_db: f64,
    sampled_z_ohm: c64,
    bracket: (f64, f64),
    n_solves: usize,
}

/// Pre-scan + golden-section search of the |S₁₁| minimum; returns the dip
/// and every evaluated point (sorted by frequency).
fn find_dip(solver: &Solver<'_>, scan: &[f64], tol_ghz: f64) -> (Dip, Vec<Point>) {
    let mut pts: Vec<Point> = scan.iter().map(|&f| solver.solve(f)).collect();
    let i = (0..pts.len())
        .min_by(|&a, &b| pts[a].s11.norm().partial_cmp(&pts[b].s11.norm()).unwrap())
        .unwrap();
    assert!(
        i > 0 && i + 1 < pts.len(),
        "the |S11| minimum of the pre-scan is at its edge ({} GHz): widen PATCH_CONFORMAL_DIP_SCAN",
        pts[i].f_ghz
    );
    let (mut a, mut b) = (pts[i - 1].f_ghz, pts[i + 1].f_ghz);
    let bracket = (a, b);
    let phi = 0.5 * (5.0_f64.sqrt() - 1.0);
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let mut fc = solver.solve(c);
    let mut fd = solver.solve(d);
    let mut n_solves = pts.len() + 2;
    while (b - a) > tol_ghz {
        if fc.s11.norm() < fd.s11.norm() {
            b = d;
            pts.push(fd);
            fd = fc;
            d = c;
            c = b - phi * (b - a);
            fc = solver.solve(c);
        } else {
            a = c;
            pts.push(fc);
            fc = fd;
            c = d;
            d = a + phi * (b - a);
            fd = solver.solve(d);
        }
        n_solves += 1;
    }
    pts.push(fc);
    pts.push(fd);
    pts.sort_by(|p, q| p.f_ghz.partial_cmp(&q.f_ghz).unwrap());
    pts.dedup_by(|p, q| (p.f_ghz - q.f_ghz).abs() < 1e-12);
    let i = (0..pts.len())
        .min_by(|&a, &b| pts[a].s11.norm().partial_cmp(&pts[b].s11.norm()).unwrap())
        .unwrap();
    let (sampled_f_ghz, sampled_db, sampled_z_ohm) = (pts[i].f_ghz, pts[i].db(), pts[i].z_ohm);
    // Parabolic refinement in dB through the sampled minimum and its two
    // neighbours (same formula as the baseline example's summary).
    let (f_ghz, db) = if i > 0 && i + 1 < pts.len() {
        let (x0, x1, x2) = (pts[i - 1].f_ghz, pts[i].f_ghz, pts[i + 1].f_ghz);
        let (y0, y1, y2) = (pts[i - 1].db(), pts[i].db(), pts[i + 1].db());
        let d01 = (y1 - y0) / (x1 - x0);
        let d12 = (y2 - y1) / (x2 - x1);
        let a2 = (d12 - d01) / (x2 - x0);
        if a2 > 0.0 {
            let xv = 0.5 * (x0 + x1) - d01 / (2.0 * a2);
            (xv, y1 + d01 * (xv - x1) + a2 * (xv - x0) * (xv - x1))
        } else {
            (x1, y1)
        }
    } else {
        (sampled_f_ghz, sampled_db)
    };
    (
        Dip {
            f_ghz,
            db,
            sampled_f_ghz,
            sampled_db,
            sampled_z_ohm,
            bracket,
            n_solves,
        },
        pts,
    )
}

/// One geometry's `dip` result: `(name, dip, search points, fixed points,
/// n_dofs, n_interior)`.
type GeomResult = (String, Dip, Vec<Point>, Vec<Point>, usize, usize);

fn push_point(t: &mut String, table: &str, p: &Point) {
    let _ = writeln!(t, "[[{table}]]");
    let _ = writeln!(t, "f_ghz = {:.6}", p.f_ghz);
    let _ = writeln!(t, "z_re_ohm = {:.6e}", p.z_ohm.re);
    let _ = writeln!(t, "z_im_ohm = {:.6e}", p.z_ohm.im);
    let _ = writeln!(t, "s11_re = {:.9e}", p.s11.re);
    let _ = writeln!(t, "s11_im = {:.9e}", p.s11.im);
    let _ = writeln!(t, "s11_mag = {:.9e}", p.s11.norm());
    let _ = writeln!(t, "s11_db = {:.6}", p.db());
    let _ = writeln!(t, "solve_residual_rel = {:.3e}", p.residual_rel);
    let _ = writeln!(t, "solve_s = {:.3}", p.solve_s);
    let _ = writeln!(t, "peak_rss_gib = {:.3}", p.peak_rss_gib);
    t.push('\n');
}

#[allow(clippy::too_many_lines)]
fn run_dip(bent_path: &Path, p2: bool, out: Option<PathBuf>) {
    let l = load(bent_path);
    let order = if p2 {
        ElementOrder::P2
    } else {
        ElementOrder::P1
    };
    let scan_spec = env_list("PATCH_CONFORMAL_DIP_SCAN", &[2.20, 2.38, 7.0]);
    assert_eq!(scan_spec.len(), 3, "PATCH_CONFORMAL_DIP_SCAN = \"lo,hi,n\"");
    let n_scan = scan_spec[2] as usize;
    assert!(n_scan >= 3);
    let scan: Vec<f64> = (0..n_scan)
        .map(|i| scan_spec[0] + (scan_spec[1] - scan_spec[0]) * i as f64 / (n_scan - 1) as f64)
        .collect();
    let tol_ghz = env_f64("PATCH_CONFORMAL_DIP_TOL_MHZ", 0.2) * 1e-3;
    let fixed = env_list("PATCH_CONFORMAL_FIXED_GHZ", &[2.28, 2.31, 2.40, 2.4835]);
    let geoms = geoms();
    let order_name = if p2 { "p2" } else { "p1" };
    eprintln!(
        "patch_conformal_convergence dip: {} ({order_name}): {} edges, {} nodes, {} tets; \
         UPML {} mm / sigma_0 = {}; scan {:?}, tol {} MHz, fixed {:?}, geometries {:?}",
        l.stem,
        l.bent.mesh.edges().len(),
        l.bent.mesh.n_nodes(),
        l.bent.mesh.n_tets(),
        l.pml_thick,
        l.sigma_0,
        scan,
        tol_ghz * 1e3,
        fixed,
        geoms
    );

    let t_all = Instant::now();
    let mut results: Vec<GeomResult> = Vec::new();
    for g in &geoms {
        let fixture = if g == "bent" { &l.bent } else { &l.flat };
        let solver = Solver::new(fixture, order, l.pml_thick, l.sigma_0);
        eprintln!(
            "{g} ({order_name}): {} DOFs, {} interior",
            solver.space.n_dofs(),
            solver.n_interior
        );
        eprintln!("{g}: dip search");
        let (dip, pts) = find_dip(&solver, &scan, tol_ghz);
        eprintln!(
            "{g}: dip {:.3} dB at {:.5} GHz (sampled {:.3} dB at {:.5} GHz, {} solves)",
            dip.db, dip.f_ghz, dip.sampled_db, dip.sampled_f_ghz, dip.n_solves
        );
        eprintln!("{g}: fixed frequencies");
        let fixed_pts: Vec<Point> = fixed.iter().map(|&f| solver.solve(f)).collect();
        results.push((
            g.clone(),
            dip,
            pts,
            fixed_pts,
            solver.space.n_dofs(),
            solver.n_interior,
        ));
    }
    let total_s = t_all.elapsed().as_secs_f64();

    let mut t = String::with_capacity(32 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example \\\n");
    let _ = writeln!(
        t,
        "#   patch_conformal_convergence -- dip <fixture.msh>{}`.",
        if p2 { " --p2" } else { "" }
    );
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Consumed by `patch_conformal_convergence aggregate` and `tests/patch_conformal_convergence.rs`.\n\n");
    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Curved conformal 2.4 GHz patch (issue #1085, conformal v5 E2): |S11| dip of \
         the initial geometry located by pre-scan + golden-section search in frequency, and S11 at \
         fixed frequencies, on one level of the mesh ladder at one element order.\"\n",
    );
    let _ = writeln!(t, "element_order = \"{order_name}\"");
    let _ = writeln!(t, "n_dofs = {}", results[0].4);
    let _ = writeln!(t, "n_interior_dofs = {}", results[0].5);
    let _ = writeln!(
        t,
        "dip_scan_ghz = [{}]",
        scan.iter()
            .map(|f| format!("{f:.4}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "dip_tol_mhz = {:.3}", tol_ghz * 1e3);
    let _ = writeln!(
        t,
        "fixed_ghz = [{}]",
        fixed
            .iter()
            .map(|f| format!("{f:.4}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let geom_refs: Vec<&str> = geoms.iter().map(String::as_str).collect();
    push_meta_common(&mut t, &l, &geom_refs);

    for (g, dip, _, _, _, _) in &results {
        let _ = writeln!(t, "[dip.{g}]");
        t.push_str("# f_ghz / s11_db: parabolic refinement (in dB) through the sampled minimum and its neighbours.\n");
        let _ = writeln!(t, "f_ghz = {:.6}", dip.f_ghz);
        let _ = writeln!(t, "s11_db = {:.4}", dip.db);
        let _ = writeln!(t, "sampled_f_ghz = {:.6}", dip.sampled_f_ghz);
        let _ = writeln!(t, "sampled_s11_db = {:.4}", dip.sampled_db);
        let _ = writeln!(t, "sampled_z_re_ohm = {:.4}", dip.sampled_z_ohm.re);
        let _ = writeln!(t, "sampled_z_im_ohm = {:.4}", dip.sampled_z_ohm.im);
        let _ = writeln!(
            t,
            "bracket_ghz = [{:.4}, {:.4}]",
            dip.bracket.0, dip.bracket.1
        );
        let _ = writeln!(t, "n_solves = {}", dip.n_solves);
        t.push('\n');
    }
    if results.len() == 2 {
        t.push_str("[detune]\n# bent minus flat, same mesh topology.\n");
        let (f, b) = (&results[0].1, &results[1].1);
        let _ = writeln!(t, "dip_shift_mhz = {:.3}", (b.f_ghz - f.f_ghz) * 1e3);
        let _ = writeln!(t, "dip_depth_change_db = {:.4}", b.db - f.db);
        t.push('\n');
    }

    let all: Vec<&Point> = results
        .iter()
        .flat_map(|r| r.2.iter().chain(r.3.iter()))
        .collect();
    let n = all.len() as f64;
    t.push_str("[cost]\n");
    t.push_str(
        "# Per solve = per-frequency UPML tensor build + assembly (host-side faer at p=2) + sparse LU \
         + solve + port readback. RSS = max of ps -o rss= samples every 50 ms during the solve.\n",
    );
    let _ = writeln!(t, "n_solves = {}", all.len());
    let _ = writeln!(
        t,
        "solve_s_mean = {:.3}",
        all.iter().map(|p| p.solve_s).sum::<f64>() / n
    );
    let _ = writeln!(
        t,
        "solve_s_max = {:.3}",
        all.iter().map(|p| p.solve_s).fold(0.0, f64::max)
    );
    let _ = writeln!(
        t,
        "first_solve_peak_rss_gib = {:.3}",
        results[0].2.first().map_or(0.0, |p| p.peak_rss_gib)
    );
    let _ = writeln!(
        t,
        "peak_rss_gib = {:.3}",
        all.iter().map(|p| p.peak_rss_gib).fold(0.0, f64::max)
    );
    let _ = writeln!(t, "total_s = {total_s:.1}");
    t.push('\n');

    for (g, _, pts, fixed_pts, _, _) in &results {
        for p in fixed_pts {
            push_point(&mut t, &format!("fixed.{g}"), p);
        }
        for p in pts {
            push_point(&mut t, &format!("points.{g}"), p);
        }
    }

    let out = out.unwrap_or_else(|| {
        repo_root()
            .join("benchmarks/patch_antenna_conformal/convergence")
            .join(format!(
                "dip_{}{}.toml",
                l.level,
                if p2 { "_p2" } else { "" }
            ))
    });
    if let Some(p) = out.parent() {
        fs::create_dir_all(p).expect("create output dir");
    }
    fs::write(&out, &t).expect("write dip toml");
    println!("wrote {}", out.display());
}

// ---------------------------------------------------------------------------
// `merge`
// ---------------------------------------------------------------------------

/// Splits a generated artifact into its leading comment block and its
/// top-level TOML tables, each as `(header line, full text)`.
fn split_tables(text: &str) -> (String, Vec<(String, String)>) {
    let mut preamble = String::new();
    let mut tables: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if line.starts_with('[') {
            tables.push((line.to_string(), format!("{line}\n")));
        } else if let Some(last) = tables.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        } else {
            preamble.push_str(line);
            preamble.push('\n');
        }
    }
    (preamble, tables)
}

/// `(f, solve_s, peak_rss_gib)` of every solve recorded in a one-geometry
/// dip artifact (dip-search points and fixed frequencies).
fn dip_solves(doc: &toml::Value, g: &str) -> Vec<(f64, f64, f64)> {
    doc["points"][g]
        .as_array()
        .unwrap()
        .iter()
        .chain(doc["fixed"][g].as_array().unwrap())
        .map(|r| {
            (
                num(&r["f_ghz"]),
                num(&r["solve_s"]),
                num(&r["peak_rss_gib"]),
            )
        })
        .collect()
}

/// Merges a flat-only and a bent-only `dip` artifact of the same level and
/// order (run separately, possibly on different hosts) into the
/// two-geometry artifact `dip` writes in one go: identical `[meta]` apart
/// from `geometries` and `host`, the two `[dip.*]` tables, the re-derived
/// `[detune]` and `[cost]`, then the flat and the bent point tables. The
/// text of every table is carried over verbatim.
fn run_merge(flat_path: &Path, bent_path: &Path, out: Option<PathBuf>) {
    let flat_text = fs::read_to_string(flat_path).expect("read flat dip toml");
    let bent_text = fs::read_to_string(bent_path).expect("read bent dip toml");
    let flat = toml::from_str::<toml::Value>(&flat_text).expect("flat dip toml");
    let bent = toml::from_str::<toml::Value>(&bent_text).expect("bent dip toml");
    let geoms_of = |d: &toml::Value| -> Vec<String> {
        d["meta"]["geometries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        geoms_of(&flat),
        ["flat"],
        "{}: not flat-only",
        flat_path.display()
    );
    assert_eq!(
        geoms_of(&bent),
        ["bent"],
        "{}: not bent-only",
        bent_path.display()
    );
    // Everything in [meta] but the geometry list and the host must agree:
    // fixture sha256s, order, DOF counts, scan, tolerance, fixed list, σ₀.
    let strip = |d: &toml::Value| -> toml::Value {
        let mut m = d["meta"].clone();
        let t = m.as_table_mut().unwrap();
        t.remove("geometries");
        t.remove("host");
        toml::Value::Table(t.clone())
    };
    assert_eq!(
        strip(&flat),
        strip(&bent),
        "{} and {}: [meta] differ beyond geometries / host",
        flat_path.display(),
        bent_path.display()
    );
    let host_of = |d: &toml::Value| {
        d["meta"]["host"]
            .as_str()
            .unwrap_or("unrecorded")
            .to_string()
    };
    let (host_f, host_b) = (host_of(&flat), host_of(&bent));
    for h in [&host_f, &host_b] {
        assert!(
            !h.contains('"') && !h.contains('\\'),
            "host string needs escaping: {h}"
        );
    }

    let (preamble_f, tables_f) = split_tables(&flat_text);
    let (_, tables_b) = split_tables(&bent_text);
    let table = |tables: &[(String, String)], header: &str| -> String {
        tables
            .iter()
            .find(|(h, _)| h == header)
            .unwrap_or_else(|| panic!("no {header} table"))
            .1
            .clone()
    };
    let name = |p: &Path| p.file_name().unwrap().to_string_lossy().into_owned();

    let mut t = String::with_capacity(flat_text.len() + bent_text.len());
    for line in preamble_f.lines() {
        if line.starts_with("# Do NOT edit") {
            let _ = writeln!(
                t,
                "# Merged by `patch_conformal_convergence merge` from {} (flat) and {} (bent), \
                 run separately (hosts in [meta]); every table is carried over verbatim, \
                 [detune] and [cost] are re-derived.",
                name(flat_path),
                name(bent_path)
            );
        }
        t.push_str(line);
        t.push('\n');
    }
    for line in table(&tables_f, "[meta]").lines() {
        if line.starts_with("geometries = ") {
            t.push_str("geometries = [\"flat\", \"bent\"]\n");
        } else if line.starts_with("host = ") && host_f != host_b {
            let _ = writeln!(t, "host = \"flat: {host_f}; bent: {host_b}\"");
        } else {
            t.push_str(line);
            t.push('\n');
        }
    }
    t.push_str(&table(&tables_f, "[meta.mesh_sizing_mm]"));
    t.push_str(&table(&tables_f, "[meta.resolution]"));
    t.push_str(&table(&tables_f, "[dip.flat]"));
    t.push_str(&table(&tables_b, "[dip.bent]"));

    let (df, db) = (&flat["dip"]["flat"], &bent["dip"]["bent"]);
    t.push_str("[detune]\n# bent minus flat, same mesh topology.\n");
    let _ = writeln!(
        t,
        "dip_shift_mhz = {:.3}",
        (num(&db["f_ghz"]) - num(&df["f_ghz"])) * 1e3
    );
    let _ = writeln!(
        t,
        "dip_depth_change_db = {:.4}",
        num(&db["s11_db"]) - num(&df["s11_db"])
    );
    t.push('\n');

    let mut all = dip_solves(&flat, "flat");
    all.extend(dip_solves(&bent, "bent"));
    let n = all.len() as f64;
    t.push_str("[cost]\n");
    t.push_str(
        "# Per solve = per-frequency UPML tensor build + assembly (host-side faer at p=2) + sparse LU \
         + solve + port readback. RSS = max of ps -o rss= samples every 50 ms during the solve.\n",
    );
    let _ = writeln!(t, "n_solves = {}", all.len());
    let _ = writeln!(
        t,
        "solve_s_mean = {:.3}",
        all.iter().map(|p| p.1).sum::<f64>() / n
    );
    let _ = writeln!(
        t,
        "solve_s_max = {:.3}",
        all.iter().map(|p| p.1).fold(0.0, f64::max)
    );
    let _ = writeln!(
        t,
        "first_solve_peak_rss_gib = {:.3}",
        num(&flat["cost"]["first_solve_peak_rss_gib"])
    );
    let _ = writeln!(
        t,
        "peak_rss_gib = {:.3}",
        all.iter().map(|p| p.2).fold(0.0, f64::max)
    );
    let _ = writeln!(
        t,
        "total_s = {:.1}",
        num(&flat["cost"]["total_s"]) + num(&bent["cost"]["total_s"])
    );
    t.push('\n');

    for (tables, g) in [(&tables_f, "flat"), (&tables_b, "bent")] {
        for (h, text) in tables {
            if h == &format!("[[fixed.{g}]]") || h == &format!("[[points.{g}]]") {
                t.push_str(text);
            }
        }
    }

    let out = out.unwrap_or_else(|| {
        let meta = &flat["meta"];
        flat_path.parent().unwrap().join(format!(
            "dip_{}{}.toml",
            meta["level"].as_str().unwrap(),
            if meta["element_order"].as_str() == Some("p2") {
                "_p2"
            } else {
                ""
            }
        ))
    });
    fs::write(&out, &t).expect("write merged dip toml");
    // The merged artifact must parse and carry both geometries.
    let merged = toml::from_str::<toml::Value>(&t).expect("merged dip toml parses");
    assert_eq!(geoms_of(&merged), ["flat", "bent"]);
    assert_eq!(
        merged["cost"]["n_solves"].as_integer(),
        Some(all.len() as i64)
    );
    println!("wrote {}", out.display());
}

// ---------------------------------------------------------------------------
// `fd`
// ---------------------------------------------------------------------------

type Band = (f64, Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>, f64);

/// The freeform radial-morph model of `patch_conformal_diffopt`, rebuilt on
/// a 2.4 GHz level: every free patch node moves along its outward radial
/// normal, the interior follows harmonically, and the UPML shell, port
/// faces, ground and outer boundary are pinned. UPML tensors are built once
/// at `X = 0` and held fixed (`∂Λ/∂X = 0`), the convention the composed
/// adjoint differentiates.
struct Model {
    mesh: geode_core::mesh::TetMesh,
    morph: FreeformBoundaryMorph,
    bands: Vec<Band>,
    mask: Vec<bool>,
    source: CurrentSource,
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    r_nat: f64,
    port: PatchPort,
    dir_scale_mm: f64,
    base_det6: Vec<f64>,
}

fn signed_det6(v: &[[f64; 3]; 4]) -> f64 {
    let e = |a: usize| [v[a][0] - v[0][0], v[a][1] - v[0][1], v[a][2] - v[0][2]];
    let (e1, e2, e3) = (e(1), e(2), e(3));
    e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0])
}

impl Model {
    fn build(l: &Loaded, band_ghz: &[f64]) -> Self {
        let base = &l.bent;
        let edges = base.mesh.edges();
        let patch = base.patch_triangles();
        let ground = base.ground_triangles();
        let outer = base.outer_boundary_triangles();
        let mask = pec_interior_mask_from_triangles(
            &edges,
            &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
        );
        let pml_tet_mask: Vec<bool> = base
            .tet_physical_tags
            .iter()
            .map(|&t| t == PHYS_UPML)
            .collect();
        let pml_shell = pml_shell_nodes(&base.mesh, &pml_tet_mask);
        let n_nodes = base.mesh.n_nodes();
        let mut pinned = vec![false; n_nodes];
        for (i, &p) in pml_shell.iter().enumerate() {
            if p {
                pinned[i] = true;
            }
        }
        let port = base.port();
        for tri in &port.faces {
            for &nn in tri {
                pinned[nn as usize] = true;
            }
        }
        for tri in ground.iter().chain(outer.iter()) {
            for &nn in tri {
                pinned[nn as usize] = true;
            }
        }
        let (sub_lo, sub_hi) = l.flat.substrate_box();
        let thickness = sub_hi[2] - sub_lo[2];
        let dir_scale_mm = DIR_SCALE_FRAC * thickness;
        let z_axis = 0.5 * (sub_lo[2] + sub_hi[2]) - l.prov.param("bend.bend_radius");
        let mut patch_nodes: Vec<u32> = patch.iter().flatten().copied().collect();
        patch_nodes.sort_unstable();
        patch_nodes.dedup();
        let mut dofs: Vec<BoundaryMotionDof> = Vec::new();
        for &nn in &patch_nodes {
            if pinned[nn as usize] {
                continue;
            }
            let p = base.mesh.nodes[nn as usize];
            let (rx, rz) = (p[0], p[2] - z_axis);
            let norm = (rx * rx + rz * rz).sqrt().max(1e-12);
            let mut dir = [rx / norm * dir_scale_mm, 0.0, rz / norm * dir_scale_mm];
            if dir[2] < 0.0 {
                dir = [-dir[0], -dir[1], -dir[2]];
            }
            dofs.push(BoundaryMotionDof { node: nn, dir });
        }
        let fixed_zero: Vec<u32> = (0..n_nodes as u32)
            .filter(|&i| pinned[i as usize])
            .collect();
        let t0 = Instant::now();
        let morph = FreeformBoundaryMorph::harmonic_boundary(&base.mesh, &dofs, &fixed_zero)
            .expect("harmonic freeform boundary morph");
        eprintln!(
            "  morph: {} radial DOFs over {} nodes ({} pinned), {:.1} s",
            morph.n_dofs(),
            n_nodes,
            fixed_zero.len(),
            t0.elapsed().as_secs_f64()
        );
        let (air_lo, air_hi) = base.air_box(l.pml_thick);
        let w = 1.0 / band_ghz.len() as f64;
        let bands: Vec<Band> = band_ghz
            .iter()
            .map(|&f| {
                let omega = ghz_to_omega_mm(f);
                let (eps, nu) = base.matched_upml_materials(
                    &FR4_MATERIALS,
                    air_lo,
                    air_hi,
                    l.pml_thick,
                    l.sigma_0,
                    omega,
                );
                (omega, eps, nu, w)
            })
            .collect();
        let r_nat = R_PORT_OHM / ETA_0;
        let flux = assemble_port_flux(&base.mesh, &port.faces, port.e_hat, &edges);
        let base_det6 = base
            .mesh
            .tets
            .iter()
            .map(|tet| {
                let v: [[f64; 3]; 4] = std::array::from_fn(|k| base.mesh.nodes[tet[k] as usize]);
                signed_det6(&v)
            })
            .collect();
        Model {
            mesh: base.mesh.clone(),
            morph,
            bands,
            mask,
            source: CurrentSource {
                j_tet: vec![[c64::new(0.0, 0.0); 3]; base.mesh.n_tets()],
            },
            flux,
            inv_width: 1.0 / port.width,
            v_inc: c64::new(1.0, 0.0),
            r_nat,
            port,
            dir_scale_mm,
            base_det6,
        }
    }

    fn min_vol_ratio(&self, x: &[f64]) -> f64 {
        let mesh = self.morph.apply(&self.mesh, x);
        let mut worst = f64::INFINITY;
        for (t, tet) in mesh.tets.iter().enumerate() {
            let v: [[f64; 3]; 4] = std::array::from_fn(|k| mesh.nodes[tet[k] as usize]);
            let d0 = self.base_det6[t];
            worst = worst.min(if d0 != 0.0 { signed_det6(&v) / d0 } else { 1.0 });
        }
        worst
    }

    /// Fresh forward-only band objective `G(x) = Σ w |S₁₁|²` and per-frequency
    /// `|S₁₁|`, through the public `driven_solve_with_ports` path.
    fn band_forward(&self, x: &[f64]) -> (f64, Vec<f64>, f64) {
        let mesh = self.morph.apply(&self.mesh, x);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let mut g_band = 0.0;
        let mut mags = Vec::new();
        let mut worst_res = 0.0_f64;
        for (omega, eps, nu, w) in &self.bands {
            let sol = driven_solve_with_ports::<B>(
                &mesh,
                DrivenMaterials::MatchedUpml {
                    epsilon_tensor: eps,
                    nu_tensor: nu,
                },
                None,
                &bcs,
                std::slice::from_ref(&lp),
                *omega,
                &self.source,
                &device(),
            )
            .expect("forward solve");
            let mut v = c64::new(0.0, 0.0);
            for (f, e) in self.flux.iter().zip(sol.e_edges.iter()) {
                v += *e * *f;
            }
            v *= self.inv_width;
            let (g, _) = s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.r_nat);
            assert!(g.sqrt() <= 1.0 + 1e-9, "passivity at omega {omega}");
            g_band += *w * g;
            mags.push(g.sqrt());
            worst_res = worst_res.max(sol.residual_rel);
        }
        (g_band, mags, worst_res)
    }

    /// Composed forward + adjoint (one LU per frequency) → `(G, ∇_X G, residual)`.
    fn band_grad(&self, x: &[f64]) -> (f64, Vec<f64>, f64) {
        let mesh = self.morph.apply(&self.mesh, x);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let mut grad_node_acc = vec![[0.0_f64; 3]; mesh.n_nodes()];
        let mut g_band = 0.0;
        let mut worst_res = 0.0_f64;
        for (omega, eps, nu, w) in &self.bands {
            let sg = driven_shape_gradient_matched_upml_ports::<B, _>(
                &mesh,
                eps,
                nu,
                &bcs,
                std::slice::from_ref(&lp),
                *omega,
                &self.source,
                s11_sq_objective(
                    self.flux.clone(),
                    self.inv_width,
                    self.v_inc,
                    self.r_nat,
                    self.r_nat,
                ),
                &device(),
            )
            .expect("composed open-radiator shape gradient");
            assert_eq!(sg.n_factorizations, 1, "adjoint must reuse the forward LU");
            g_band += *w * sg.objective;
            worst_res = worst_res.max(sg.residual_rel);
            for (acc, gn) in grad_node_acc.iter_mut().zip(sg.grad_node.iter()) {
                for k in 0..3 {
                    acc[k] += *w * gn[k];
                }
            }
        }
        (
            g_band,
            self.morph.design_gradient(&grad_node_acc),
            worst_res,
        )
    }
}

/// A small LCG (deterministic, dependency-free) for the random directions.
struct Lcg(u64);

impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

#[allow(clippy::too_many_lines)]
fn run_fd(bent_path: &Path, out: Option<PathBuf>) {
    let l = load(bent_path);
    let band_ghz = env_list(
        "PATCH_CONFORMAL_FD_BAND_GHZ",
        &[
            ISM_2G4_BAND_GHZ[0],
            0.5 * (ISM_2G4_BAND_GHZ[0] + ISM_2G4_BAND_GHZ[1]),
            ISM_2G4_BAND_GHZ[1],
        ],
    );
    let n_dirs = env_f64("PATCH_CONFORMAL_FD_DIRS", 3.0) as usize;
    let step = env_f64("PATCH_CONFORMAL_FD_STEP", 1.0e-4);
    let seed = env_f64("PATCH_CONFORMAL_FD_SEED", 0x1085 as f64) as u64;
    eprintln!(
        "patch_conformal_convergence fd: {}: {} edges, {} nodes; band {:?} GHz; {n_dirs} directions, \
         step {step:e}, seed {seed}",
        l.stem,
        l.bent.mesh.edges().len(),
        l.bent.mesh.n_nodes(),
        band_ghz
    );
    let model = Model::build(&l, &band_ghz);
    let n_dofs = model.morph.n_dofs();
    let x0 = vec![0.0_f64; n_dofs];

    eprintln!("forward at X = 0");
    let t0 = Instant::now();
    let ((g0, mags0, res_fwd), rss_fwd) = with_peak_rss(|| model.band_forward(&x0));
    let forward_s = t0.elapsed().as_secs_f64();
    eprintln!(
        "  G = {g0:.6e}, |S11| = {:?}, {forward_s:.1} s ({:.1} s / solve), {rss_fwd:.2} GiB",
        mags0,
        forward_s / band_ghz.len() as f64
    );
    eprintln!("gradient at X = 0");
    let t0 = Instant::now();
    let ((g0_adj, grad0, res_adj), rss_grad) = with_peak_rss(|| model.band_grad(&x0));
    let gradient_s = t0.elapsed().as_secs_f64();
    let gnorm = grad0.iter().map(|g| g * g).sum::<f64>().sqrt();
    eprintln!(
        "  G = {g0_adj:.6e}, |grad| = {gnorm:.6e}, {gradient_s:.1} s ({:.1} s / frequency), \
         {rss_grad:.2} GiB",
        gradient_s / band_ghz.len() as f64
    );
    assert!(
        (g0 - g0_adj).abs() <= 1e-6 * g0.abs().max(1.0),
        "adjoint objective {g0_adj} disagrees with forward {g0}"
    );

    let mut rng = Lcg(seed);
    struct Dir {
        analytic: f64,
        fd: f64,
        rel_err: f64,
        g_plus: f64,
        g_minus: f64,
        vol_ratio: f64,
        wall_s: f64,
    }
    let mut dirs = Vec::new();
    for k in 0..n_dirs {
        let mut d: Vec<f64> = (0..n_dofs).map(|_| 2.0 * rng.next_f64() - 1.0).collect();
        let nrm = d.iter().map(|v| v * v).sum::<f64>().sqrt();
        for v in &mut d {
            *v /= nrm;
        }
        let analytic = grad0.iter().zip(&d).map(|(g, v)| g * v).sum::<f64>();
        let xp: Vec<f64> = d.iter().map(|v| step * v).collect();
        let xm: Vec<f64> = d.iter().map(|v| -step * v).collect();
        let vol_ratio = model.min_vol_ratio(&xp).min(model.min_vol_ratio(&xm));
        let t0 = Instant::now();
        let (g_plus, _, _) = model.band_forward(&xp);
        let (g_minus, _, _) = model.band_forward(&xm);
        let wall_s = t0.elapsed().as_secs_f64();
        let fd = (g_plus - g_minus) / (2.0 * step);
        let rel_err = (analytic - fd).abs() / fd.abs().max(f64::MIN_POSITIVE);
        eprintln!(
            "  direction {k}: analytic {analytic:+.6e}, central FD {fd:+.6e}, rel {rel_err:.3e}, \
             min vol ratio {vol_ratio:.6}, {wall_s:.1} s"
        );
        dirs.push(Dir {
            analytic,
            fd,
            rel_err,
            g_plus,
            g_minus,
            vol_ratio,
            wall_s,
        });
    }
    let max_rel = dirs.iter().map(|d| d.rel_err).fold(0.0, f64::max);

    let mut t = String::with_capacity(8 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example \\\n");
    t.push_str("#   patch_conformal_convergence -- fd <fixture.msh>`.\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Consumed by `patch_conformal_convergence aggregate` and `tests/patch_conformal_convergence.rs`.\n\n");
    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Curved conformal 2.4 GHz patch (issue #1085, conformal v5 E2): finite-difference \
         gate of the composed open-radiator shape gradient at X = 0 on one level, along seeded random \
         unit directions in the radial-normal freeform DOF space, plus the forward and gradient cost.\"\n",
    );
    t.push_str("element_order = \"p1\"\n");
    push_meta_common(&mut t, &l, &["bent"]);

    t.push_str("[model]\n");
    t.push_str(
        "parametrization = \"FreeformBoundaryMorph::harmonic_boundary: each free patch-conductor node \
         is a radial-normal design DOF (outward from the bend axis); interior extended by the P1-Laplace \
         mesh morph; UPML shell, port faces, ground and outer boundary pinned (fixed_zero)\"\n",
    );
    t.push_str(
        "objective = \"G(X) = sum_f w_f |S11(f; X)|^2, uniform weights, S11 = (Z - Z0)/(Z + Z0), Z0 = 50 ohm\"\n",
    );
    t.push_str(
        "gradient = \"driven_shape_gradient_matched_upml_ports per band frequency (one forward + one \
         adjoint solve on a single LU), nodal gradients contracted through the morph columns; UPML \
         tensors built at X = 0 and held fixed\"\n",
    );
    let _ = writeln!(t, "n_design_dofs = {n_dofs}");
    let _ = writeln!(t, "dir_scale_mm = {:.4}", model.dir_scale_mm);
    let _ = writeln!(
        t,
        "band_f_ghz = [{}]",
        band_ghz
            .iter()
            .map(|f| format!("{f:.5}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    t.push('\n');

    t.push_str("[forward]\n");
    let _ = writeln!(t, "band_objective = {g0:.9e}");
    let _ = writeln!(
        t,
        "s11_mag = [{}]",
        mags0
            .iter()
            .map(|m| format!("{m:.6e}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        t,
        "s11_db = [{}]",
        mags0
            .iter()
            .map(|m| format!("{:.4}", 20.0 * m.log10()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "worst_residual_rel = {res_fwd:.3e}");
    let _ = writeln!(t, "wall_s = {forward_s:.3}");
    let _ = writeln!(t, "per_solve_s = {:.3}", forward_s / band_ghz.len() as f64);
    let _ = writeln!(t, "peak_rss_gib = {rss_fwd:.3}");
    t.push('\n');

    t.push_str("[gradient]\n");
    let _ = writeln!(t, "band_objective = {g0_adj:.9e}");
    let _ = writeln!(t, "grad_norm = {gnorm:.9e}");
    let _ = writeln!(t, "worst_residual_rel = {res_adj:.3e}");
    t.push_str("n_factorizations_per_freq = 1\n");
    let _ = writeln!(t, "wall_s = {gradient_s:.3}");
    let _ = writeln!(
        t,
        "per_frequency_s = {:.3}",
        gradient_s / band_ghz.len() as f64
    );
    let _ = writeln!(t, "peak_rss_gib = {rss_grad:.3}");
    t.push('\n');

    for (k, d) in dirs.iter().enumerate() {
        t.push_str("[[directions]]\n");
        let _ = writeln!(t, "index = {k}");
        let _ = writeln!(t, "seed = {seed}");
        let _ = writeln!(t, "step = {step:e}");
        let _ = writeln!(t, "analytic_dir_deriv = {:.9e}", d.analytic);
        let _ = writeln!(t, "central_fd_dir_deriv = {:.9e}", d.fd);
        let _ = writeln!(t, "rel_err = {:.6e}", d.rel_err);
        let _ = writeln!(t, "g_plus = {:.12e}", d.g_plus);
        let _ = writeln!(t, "g_minus = {:.12e}", d.g_minus);
        let _ = writeln!(t, "min_vol_ratio = {:.6}", d.vol_ratio);
        let _ = writeln!(t, "wall_s = {:.3}", d.wall_s);
        t.push('\n');
    }
    t.push_str("[fd_gate]\n");
    let _ = writeln!(t, "n_directions = {n_dirs}");
    let _ = writeln!(t, "max_rel_err = {max_rel:.6e}");
    let _ = writeln!(t, "tolerance = {FD_TOL:e}");
    let _ = writeln!(t, "pass = {}", max_rel < FD_TOL);
    t.push('\n');

    let out = out.unwrap_or_else(|| {
        repo_root()
            .join("benchmarks/patch_antenna_conformal/convergence")
            .join(format!("fd_{}.toml", l.level))
    });
    if let Some(p) = out.parent() {
        fs::create_dir_all(p).expect("create output dir");
    }
    fs::write(&out, &t).expect("write fd toml");
    println!("wrote {}", out.display());
    assert!(max_rel < FD_TOL, "FD gate failed: max rel {max_rel:.3e}");
}

// ---------------------------------------------------------------------------
// TOML helpers for the aggregators.
// ---------------------------------------------------------------------------

fn num(v: &toml::Value) -> f64 {
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("expected a number, got {v:?}"))
}

fn read_toml(path: &Path) -> toml::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn get<'a>(v: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur.get(k)?;
    }
    Some(cur)
}

fn fmt_list(v: &[f64], prec: usize) -> String {
    v.iter()
        .map(|x| format!("{x:.prec$}"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// `ladder`
// ---------------------------------------------------------------------------

/// `(dip f, dip dB, ISM worst dB, S11 at every grid point)`.
type LadderGeom = (f64, f64, f64, Vec<(f64, c64)>);

struct LadderRun {
    shell: String,
    sigma: f64,
    sha: String,
    n_edges: usize,
    pml_thick: f64,
    lc_pml: f64,
    /// Per geometry: (dip f, dip dB, ISM worst dB, S11 at every grid point).
    geoms: BTreeMap<String, LadderGeom>,
    solve_s_mean: f64,
    peak_rss: f64,
}

fn load_ladder(dir: &Path) -> Vec<LadderRun> {
    let mut runs = Vec::new();
    for (shell, _) in LADDER_SHELLS {
        for s in LADDER_SIGMAS {
            let path = dir.join(format!("{shell}_sigma{s}.toml"));
            if !path.exists() {
                // The 40 mm shell is a thickness check beyond the two
                // required shell resolutions; it was run on the plateau
                // candidates only (issue #1085, compute-bound host).
                assert_eq!(
                    shell,
                    LADDER_SHELLS[2].0,
                    "ladder run {} is missing",
                    path.display()
                );
                eprintln!("ladder: no {} (not run)", path.display());
                continue;
            }
            let doc = read_toml(&path);
            let meta = &doc["meta"];
            assert_eq!(meta["fixture"].as_str(), Some(shell));
            let sigma: f64 = s.parse().unwrap();
            assert_eq!(num(&meta["upml_sigma_0"]), sigma, "{}", path.display());
            let mut geoms = BTreeMap::new();
            let mut solve_s = Vec::new();
            let mut rss = 0.0_f64;
            for g in ["flat", "bent"] {
                let rows = doc["sweep"][g].as_array().unwrap();
                let pts: Vec<(f64, c64)> = rows
                    .iter()
                    .map(|r| {
                        solve_s.push(num(&r["solve_s"]));
                        rss = rss.max(num(&r["peak_rss_gib"]));
                        (
                            num(&r["f_ghz"]),
                            c64::new(num(&r["s11_re"]), num(&r["s11_im"])),
                        )
                    })
                    .collect();
                let sum = &doc["summary"][g];
                geoms.insert(
                    g.to_string(),
                    (
                        num(&sum["s11_dip_f_ghz"]),
                        num(&sum["s11_dip_db"]),
                        num(&sum["ism_worst_s11_db"]),
                        pts,
                    ),
                );
            }
            runs.push(LadderRun {
                shell: shell.to_string(),
                sigma,
                sha: meta["fixture_sha256"].as_str().unwrap().to_string(),
                n_edges: meta["n_edges"].as_integer().unwrap() as usize,
                pml_thick: num(&meta["upml_thick_mm"]),
                lc_pml: num(&meta["mesh_sizing_mm"]["lc_pml"]),
                geoms,
                solve_s_mean: solve_s.iter().sum::<f64>() / solve_s.len() as f64,
                peak_rss: rss,
            });
        }
    }
    runs
}

/// Max over both geometries and every grid point of `|S₁₁(a) − S₁₁(b)|`.
fn s11_distance(a: &LadderRun, b: &LadderRun) -> f64 {
    let mut worst = 0.0_f64;
    for (g, (_, _, _, pa)) in &a.geoms {
        let pb = &b.geoms[g].3;
        assert_eq!(pa.len(), pb.len(), "ladder grids differ");
        for ((fa, sa), (fb, sb)) in pa.iter().zip(pb) {
            assert!((fa - fb).abs() < 1e-9, "ladder grids differ");
            worst = worst.max((sa - sb).norm());
        }
    }
    worst
}

#[allow(clippy::too_many_lines)]
fn run_ladder(dir: &Path, out: Option<PathBuf>) {
    let runs = load_ladder(dir);
    let find = |shell: &str, sigma: f64| -> &LadderRun {
        runs.iter()
            .find(|r| r.shell == shell && r.sigma == sigma)
            .unwrap_or_else(|| panic!("ladder: no run for {shell} at sigma_0 = {sigma}"))
    };
    let try_find = |shell: &str, sigma: f64| -> Option<&LadderRun> {
        runs.iter().find(|r| r.shell == shell && r.sigma == sigma)
    };
    let base = LADDER_SHELLS[0].0;
    let sigmas: Vec<f64> = LADDER_SIGMAS.iter().map(|s| s.parse().unwrap()).collect();
    let sigma_fixture = {
        let yaml = repo_root().join("reference/gmsh").join(LADDER_SHELLS[0].1);
        let text = fs::read_to_string(&yaml).expect("L1 yaml");
        text.lines()
            .find_map(|l| {
                let l = l.split('#').next().unwrap().trim();
                l.strip_prefix("upml_sigma_0:")
                    .map(|v| v.trim().parse::<f64>().expect("upml_sigma_0"))
            })
            .expect("L1 yaml carries solver.upml_sigma_0")
    };

    // Stationarity score of each σ₀ on the base shell: the largest complex-S11
    // move to its σ₀ neighbours and to the shell variants at the same σ₀.
    let mut scores: Vec<(f64, f64, f64, f64, f64, f64)> = Vec::new();
    for (i, &s) in sigmas.iter().enumerate() {
        let r = find(base, s);
        let lo = if i > 0 {
            s11_distance(r, find(base, sigmas[i - 1]))
        } else {
            f64::NAN
        };
        let hi = if i + 1 < sigmas.len() {
            s11_distance(r, find(base, sigmas[i + 1]))
        } else {
            f64::NAN
        };
        let d5 = s11_distance(r, find(LADDER_SHELLS[1].0, s));
        let d40 = try_find(LADDER_SHELLS[2].0, s).map_or(f64::NAN, |v| s11_distance(r, v));
        // The score uses the comparisons every σ₀ has (its σ₀ neighbours and
        // the lc_pml 5 shell); the 40 mm thickness check is reported
        // alongside, so a σ₀ without a 40 mm run is not favoured.
        let score = [lo, hi, d5]
            .iter()
            .filter(|v| !v.is_nan())
            .fold(0.0_f64, |a, &b| a.max(b));
        scores.push((s, lo, hi, d5, d40, score));
    }
    // Choice rule: the plateau is every σ₀ whose score is within a factor
    // PLATEAU_FACTOR of the smallest; keep the fixture's σ₀ if it is on the
    // plateau (no churn of the committed default for a hair's-breadth
    // argmin), otherwise take the argmin.
    let min_score = scores.iter().map(|s| s.5).fold(f64::INFINITY, f64::min);
    let fixture_row = scores.iter().find(|s| s.0 == sigma_fixture);
    let chosen = match fixture_row {
        Some(row) if row.5 <= LADDER_PLATEAU_FACTOR * min_score => row,
        _ => scores
            .iter()
            .min_by(|a, b| a.5.partial_cmp(&b.5).unwrap())
            .unwrap(),
    };
    let chosen_sigma = chosen.0;
    let plateau: Vec<f64> = scores
        .iter()
        .filter(|s| s.5 <= LADDER_PLATEAU_FACTOR * min_score)
        .map(|s| s.0)
        .collect();
    eprintln!(
        "sigma_0 stationarity (max |dS11| to lower / upper sigma_0 neighbour, lc_pml 5 shell; 40 mm shell \
         reported, not scored):"
    );
    for (s, lo, hi, d5, d40, score) in &scores {
        eprintln!("  sigma_0 = {s:>4}: {lo:.4} / {hi:.4} / {d5:.4} / {d40:.4} -> score {score:.4}");
    }
    eprintln!(
        "plateau (score <= {LADDER_PLATEAU_FACTOR} x min {min_score:.4}): {plateau:?}; fixture sigma_0 = \
         {sigma_fixture}; chosen sigma_0 = {chosen_sigma} (score {:.4})",
        chosen.5
    );

    let mut t = String::with_capacity(32 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example \\\n");
    t.push_str("#   patch_conformal_convergence -- ladder <run-dir>` from the per-run\n");
    t.push_str("# `patch_conformal_2g4_baseline` sweeps in benchmarks/patch_antenna_conformal/upml_ladder/.\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_conformal_convergence.rs`.\n\n");
    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Curved conformal 2.4 GHz patch (issue #1085, conformal v5 E2): UPML sigma_0 x \
         shell ladder on level L1. Each run is the dense flat / bent S11 sweep of \
         patch_conformal_2g4_baseline on a 12-point grid, with sigma_0 overridden; the three shells \
         are the L1 mesh (25 mm shell, lc_pml 8.3), the same with lc_pml 5.0, and a 40 mm shell at \
         lc_pml 8.0. The chosen sigma_0 is the most stationary one: the smallest worst-case complex-S11 \
         move to its sigma_0 neighbours and to both shell variants.\"\n",
    );
    let _ = writeln!(
        t,
        "generated_at_commit = \"{}\"",
        geode_util::repo::current_commit()
    );
    t.push_str("length_unit = \"mm\"\nfrequency_unit = \"GHz\"\nimpedance_unit = \"ohm\"\n");
    t.push_str(
        "upml_profile = \"s = 1 - j sigma_0 (d/w)^2 / omega per axis (box UPML), omega in rad/mm; \
         one-way attenuation sigma_0 w / 3 Np\"\n",
    );
    let _ = writeln!(t, "sigma_0 = [{}]", fmt_list(&sigmas, 1));
    let grid: Vec<f64> = runs[0].geoms["flat"].3.iter().map(|p| p.0).collect();
    let _ = writeln!(t, "grid_ghz = [{}]", fmt_list(&grid, 4));
    let _ = writeln!(t, "report_ghz = [{}]", fmt_list(&LADDER_REPORT_GHZ, 4));
    t.push_str("geometries = [\"flat\", \"bent\"]\n\n");

    for (shell, yaml) in LADDER_SHELLS {
        let r = find(shell, sigma_fixture);
        let _ = writeln!(t, "[shells.{shell}]");
        let _ = writeln!(t, "fixture_yaml = \"reference/gmsh/{yaml}\"");
        let _ = writeln!(t, "fixture_sha256 = \"{}\"", r.sha);
        let _ = writeln!(t, "n_edges = {}", r.n_edges);
        let _ = writeln!(t, "upml_thick_mm = {}", r.pml_thick);
        let _ = writeln!(t, "lc_pml_mm = {}", r.lc_pml);
        let _ = writeln!(
            t,
            "nominal_elements_through_shell = {:.2}",
            r.pml_thick / r.lc_pml
        );
        let _ = writeln!(
            t,
            "one_way_attenuation_np_per_sigma_0 = {:.3}",
            r.pml_thick / 3.0
        );
        t.push('\n');
    }

    let _ = writeln!(
        t,
        "[meta.runs]\nn_runs = {}\n# Each shell lists the sigma_0 values it was run at.",
        runs.len()
    );
    for (shell, _) in LADDER_SHELLS {
        let ran: Vec<f64> = runs
            .iter()
            .filter(|r| r.shell == shell)
            .map(|r| r.sigma)
            .collect();
        let _ = writeln!(t, "{shell} = [{}]", fmt_list(&ran, 1));
    }
    t.push('\n');
    for r in &runs {
        for (g, (dip_f, dip_db, ism_worst, pts)) in &r.geoms {
            t.push_str("[[rows]]\n");
            let _ = writeln!(t, "shell = \"{}\"", r.shell);
            let _ = writeln!(t, "sigma_0 = {}", r.sigma);
            let _ = writeln!(t, "geometry = \"{g}\"");
            let _ = writeln!(t, "s11_dip_f_ghz = {dip_f:.6}");
            let _ = writeln!(t, "s11_dip_db = {dip_db:.4}");
            let _ = writeln!(t, "ism_worst_s11_db = {ism_worst:.4}");
            for &f in &LADDER_REPORT_GHZ {
                let s = pts
                    .iter()
                    .find(|p| (p.0 - f).abs() < 1e-6)
                    .unwrap_or_else(|| panic!("ladder run lacks {f} GHz"))
                    .1;
                let key = format!("{f:.4}").replace('.', "p");
                let _ = writeln!(
                    t,
                    "s11_{key}_ghz = {{ re = {:.6e}, im = {:.6e}, db = {:.4} }}",
                    s.re,
                    s.im,
                    20.0 * s.norm().log10()
                );
            }
            let _ = writeln!(t, "solve_s_mean = {:.3}", r.solve_s_mean);
            let _ = writeln!(t, "peak_rss_gib = {:.3}", r.peak_rss);
            t.push('\n');
        }
    }

    for (s, lo, hi, d5, d40, score) in &scores {
        t.push_str("[[sensitivity]]\n");
        t.push_str(
            "# max |S11(a) - S11(b)| over both geometries and all 12 grid points, base shell.\n",
        );
        let _ = writeln!(t, "sigma_0 = {s}");
        if lo.is_nan() {
            t.push_str("# d_s11_lower_sigma: no lower neighbour\n");
        } else {
            let _ = writeln!(t, "d_s11_lower_sigma = {lo:.6e}");
        }
        if hi.is_nan() {
            t.push_str("# d_s11_upper_sigma: no upper neighbour\n");
        } else {
            let _ = writeln!(t, "d_s11_upper_sigma = {hi:.6e}");
        }
        let _ = writeln!(t, "d_s11_shell_lc_pml_5 = {d5:.6e}");
        if d40.is_nan() {
            t.push_str("# d_s11_shell_40mm: the 40 mm shell was not run at this sigma_0\n");
        } else {
            let _ = writeln!(t, "d_s11_shell_40mm = {d40:.6e}");
        }
        let _ = writeln!(t, "score = {score:.6e}");
        let rb = find(base, *s);
        let r5 = find(LADDER_SHELLS[1].0, *s);
        let r40 = try_find(LADDER_SHELLS[2].0, *s);
        for g in ["flat", "bent"] {
            let mut dbs = vec![rb.geoms[g].1, r5.geoms[g].1];
            let mut fs = vec![rb.geoms[g].0, r5.geoms[g].0];
            if let Some(r40) = r40 {
                dbs.push(r40.geoms[g].1);
                fs.push(r40.geoms[g].0);
            }
            let _ = writeln!(t, "{g}_dip_db_base_lcpml5_40mm = [{}]", fmt_list(&dbs, 3));
            let _ = writeln!(t, "{g}_dip_f_ghz_base_lcpml5_40mm = [{}]", fmt_list(&fs, 5));
        }
        t.push('\n');
    }

    t.push_str("[choice]\n");
    let _ = writeln!(t, "sigma_0 = {chosen_sigma}");
    let _ = writeln!(t, "sigma_0_fixture = {sigma_fixture}");
    t.push_str(
        "rule = \"score(sigma_0) = max(d_s11_lower_sigma, d_s11_upper_sigma, d_s11_shell_lc_pml_5) on the \
         base shell (the 40 mm thickness check is reported, not scored); plateau = { sigma_0 : score <= \
         plateau_factor x min score }; chosen = sigma_0_fixture if it is on the plateau, else the argmin\"\n",
    );
    let _ = writeln!(t, "plateau_factor = {LADDER_PLATEAU_FACTOR}");
    let _ = writeln!(t, "min_score = {min_score:.6e}");
    let _ = writeln!(t, "plateau_sigma_0 = [{}]", fmt_list(&plateau, 1));
    let _ = writeln!(t, "residual_sensitivity_s11 = {:.6e}", chosen.5);
    if chosen.4.is_nan() {
        t.push_str("# residual_sensitivity_incl_40mm_s11: the 40 mm shell was not run at the chosen sigma_0\n");
    } else {
        let _ = writeln!(
            t,
            "residual_sensitivity_incl_40mm_s11 = {:.6e}",
            chosen.5.max(chosen.4)
        );
    }
    let _ = writeln!(
        t,
        "residual_sensitivity_db_at_minus_20db = {:.3}",
        20.0 * (0.1_f64 + chosen.5).log10() + 20.0
    );
    t.push_str(
        "# residual_sensitivity_db_at_minus_20db: the dB change a |S11| = 0.1 (-20 dB) point would \
         show under the residual complex-S11 move (an upper bound on the dip-depth uncertainty there).\n",
    );
    let rb = find(base, chosen_sigma);
    for g in ["flat", "bent"] {
        let _ = writeln!(t, "{g}_dip_f_ghz = {:.6}", rb.geoms[g].0);
        let _ = writeln!(t, "{g}_dip_db = {:.4}", rb.geoms[g].1);
        let _ = writeln!(t, "{g}_ism_worst_s11_db = {:.4}", rb.geoms[g].2);
    }
    // The span across the whole ladder, for the record.
    let mut span = 0.0_f64;
    for a in &runs {
        for b in &runs {
            span = span.max(s11_distance(a, b));
        }
    }
    let _ = writeln!(t, "ladder_span_s11 = {span:.6e}");
    let r1 = find(base, 1.0);
    let r25 = find(base, 25.0);
    let _ = writeln!(
        t,
        "flat_dip_db_sigma_1_vs_25 = [{:.3}, {:.3}]",
        r1.geoms["flat"].1, r25.geoms["flat"].1
    );
    t.push('\n');

    let out = out.unwrap_or_else(|| {
        repo_root().join("benchmarks/patch_antenna_conformal/upml_ladder_L1.toml")
    });
    if let Some(p) = out.parent() {
        fs::create_dir_all(p).expect("create output dir");
    }
    fs::write(&out, &t).expect("write ladder toml");
    println!("wrote {}", out.display());
}

// ---------------------------------------------------------------------------
// `aggregate`
// ---------------------------------------------------------------------------

/// `(dip f, dip dB, fixed S11 by frequency, Z_in at the sampled dip)`.
type LevelGeom = (f64, f64, Vec<(f64, c64)>, c64);

struct LevelRow {
    name: String,
    order: String,
    n_edges: usize,
    n_dofs: usize,
    h: f64,
    sha: String,
    layers: Option<f64>,
    sigma_0: f64,
    /// Per geometry: (dip f, dip dB, fixed S11 by frequency, Z_in at the sampled dip).
    geoms: BTreeMap<String, LevelGeom>,
    solve_s_mean: f64,
    first_rss: f64,
    peak_rss: f64,
}

fn load_level(path: &Path) -> LevelRow {
    let doc = read_toml(path);
    let meta = &doc["meta"];
    let n_dofs = meta["n_dofs"].as_integer().unwrap() as usize;
    let mut geoms = BTreeMap::new();
    for g in meta["geometries"].as_array().unwrap() {
        let g = g.as_str().unwrap();
        let dip = &doc["dip"][g];
        let fixed: Vec<(f64, c64)> = doc["fixed"][g]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    num(&r["f_ghz"]),
                    c64::new(num(&r["s11_re"]), num(&r["s11_im"])),
                )
            })
            .collect();
        geoms.insert(
            g.to_string(),
            (
                num(&dip["f_ghz"]),
                num(&dip["s11_db"]),
                fixed,
                c64::new(num(&dip["sampled_z_re_ohm"]), num(&dip["sampled_z_im_ohm"])),
            ),
        );
    }
    let layers = get(&doc, "meta.resolution").and_then(|r| {
        r.as_table()?
            .iter()
            .find(|(k, _)| k.starts_with("substrate layers"))
            .and_then(|(_, v)| v.as_str()?.split_whitespace().next()?.parse().ok())
    });
    LevelRow {
        name: meta["level"].as_str().unwrap().to_string(),
        order: meta["element_order"].as_str().unwrap().to_string(),
        n_edges: meta["n_edges"].as_integer().unwrap() as usize,
        n_dofs,
        h: (n_dofs as f64).powf(-1.0 / 3.0),
        sha: meta["fixture_sha256"].as_str().unwrap().to_string(),
        layers,
        sigma_0: num(&meta["upml_sigma_0"]),
        geoms,
        solve_s_mean: num(&doc["cost"]["solve_s_mean"]),
        first_rss: num(&doc["cost"]["first_solve_peak_rss_gib"]),
        peak_rss: num(&doc["cost"]["peak_rss_gib"]),
    }
}

/// Least-squares slope of `log|q_i − q_{i+1}|` against `log h_i` over the
/// consecutive pairs of an h-sequence (finest last); `None` with fewer than
/// two pairs or a vanishing difference.
fn observed_order(h: &[f64], q: &[f64]) -> Option<f64> {
    if h.len() < 3 {
        return None;
    }
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..h.len() - 1 {
        let e = (q[i] - q[i + 1]).abs();
        if e <= 0.0 {
            return None;
        }
        xs.push(h[i].ln());
        ys.push(e.ln());
    }
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    Some(sxy / sxx)
}

/// Richardson extrapolation from the finest pair at order `p`:
/// `q* = q_f + (q_f − q_c) / ((h_c/h_f)^p − 1)`.
fn extrapolate(h_c: f64, h_f: f64, q_c: f64, q_f: f64, p: f64) -> f64 {
    q_f + (q_f - q_c) / ((h_c / h_f).powf(p) - 1.0)
}

#[allow(clippy::too_many_lines)]
fn run_aggregate(dir: &Path, out: Option<PathBuf>) {
    let conv = dir.join("convergence");
    // h-ladder at p=1, then the p=2 rows.
    let mut levels: Vec<LevelRow> = Vec::new();
    for lv in LEVELS {
        let p = conv.join(format!("dip_{lv}.toml"));
        if p.exists() {
            levels.push(load_level(&p));
        } else {
            eprintln!("aggregate: no {} (level not run)", p.display());
        }
    }
    let mut p2_rows: Vec<LevelRow> = Vec::new();
    for lv in LEVELS {
        let p = conv.join(format!("dip_{lv}_p2.toml"));
        if p.exists() {
            p2_rows.push(load_level(&p));
        }
    }
    assert!(levels.len() >= 3, "need at least three p=1 levels");
    let sigma_0 = levels[0].sigma_0;
    for r in levels.iter().chain(p2_rows.iter()) {
        assert_eq!(
            r.sigma_0, sigma_0,
            "{}: mixed sigma_0 in the ladder",
            r.name
        );
    }
    let fixed_ghz: Vec<f64> = levels[0].geoms["bent"].2.iter().map(|p| p.0).collect();

    // FD / gradient-cost artifacts.
    let mut fd_docs: Vec<(String, toml::Value)> = Vec::new();
    for lv in LEVELS {
        let p = conv.join(format!("fd_{lv}.toml"));
        if p.exists() {
            fd_docs.push((lv.to_string(), read_toml(&p)));
        }
    }
    // Dense baselines (for the ISM-band figures and the grid-based dips).
    let mut dense: Vec<(String, toml::Value)> = Vec::new();
    for lv in LEVELS {
        let p = dir.join(format!("baseline_2g4_{lv}.toml"));
        if p.exists() {
            dense.push((lv.to_string(), read_toml(&p)));
        }
    }
    let ladder_path = dir.join("upml_ladder_L1.toml");
    let ladder = ladder_path.exists().then(|| read_toml(&ladder_path));

    let mut t = String::with_capacity(32 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example \\\n");
    t.push_str(
        "#   patch_conformal_convergence -- aggregate` from benchmarks/patch_antenna_conformal/\n",
    );
    t.push_str("# {convergence/dip_*.toml, convergence/fd_*.toml, baseline_2g4_*.toml, upml_ladder_L1.toml}.\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_conformal_convergence.rs`.\n\n");
    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Curved conformal 2.4 GHz patch (issue #1085, conformal v5 E2): mesh-refinement \
         study of the initial (pre-optimization) S11, flat and bent, over the h-ladder L0-L3 at p=1 and \
         p=2 over the coarse levels, at the UPML setting chosen by the sigma_0 x shell ladder; observed \
         convergence order of the |S11| dip frequency and depth, Richardson extrapolation from the \
         finest pair, per-level forward / gradient cost and peak RSS, and the finite-difference gate \
         of the shape gradient.\"\n",
    );
    let _ = writeln!(
        t,
        "generated_at_commit = \"{}\"",
        geode_util::repo::current_commit()
    );
    t.push_str("length_unit = \"mm\"\nfrequency_unit = \"GHz\"\nimpedance_unit = \"ohm\"\n");
    let _ = writeln!(
        t,
        "band_f_ghz = [{}, {}]",
        ISM_2G4_BAND_GHZ[0], ISM_2G4_BAND_GHZ[1]
    );
    let _ = writeln!(t, "upml_sigma_0 = {sigma_0}");
    t.push_str("h_proxy = \"h = n_dofs^(-1/3) (DOF-based mesh size; the levels are graded, not uniform)\"\n");
    let _ = writeln!(
        t,
        "levels_p1 = [{}]",
        levels
            .iter()
            .map(|l| format!("\"{}\"", l.name))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        t,
        "levels_p2 = [{}]",
        p2_rows
            .iter()
            .map(|l| format!("\"{}\"", l.name))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "fixed_ghz = [{}]", fmt_list(&fixed_ghz, 4));
    t.push('\n');

    for r in levels.iter().chain(p2_rows.iter()) {
        let key = format!("{}_{}", r.name, r.order);
        let _ = writeln!(t, "[levels.{key}]");
        let _ = writeln!(t, "level = \"{}\"", r.name);
        let _ = writeln!(t, "element_order = \"{}\"", r.order);
        let _ = writeln!(t, "fixture_sha256 = \"{}\"", r.sha);
        let _ = writeln!(t, "n_edges = {}", r.n_edges);
        let _ = writeln!(t, "n_dofs = {}", r.n_dofs);
        let _ = writeln!(t, "h_proxy = {:.6e}", r.h);
        if let Some(l) = r.layers {
            let _ = writeln!(t, "substrate_layers_proxy = {l:.2}");
        }
        t.push('\n');
    }

    // S11 convergence table.
    for g in ["flat", "bent"] {
        for r in levels.iter().chain(p2_rows.iter()) {
            let Some((dip_f, dip_db, fixed, dip_z)) = r.geoms.get(g) else {
                continue;
            };
            let _ = writeln!(t, "[[s11.{g}]]");
            let _ = writeln!(t, "level = \"{}\"", r.name);
            let _ = writeln!(t, "element_order = \"{}\"", r.order);
            let _ = writeln!(t, "n_dofs = {}", r.n_dofs);
            let _ = writeln!(t, "dip_f_ghz = {dip_f:.6}");
            let _ = writeln!(t, "dip_db = {dip_db:.4}");
            let _ = writeln!(t, "dip_z_re_ohm = {:.4}", dip_z.re);
            let _ = writeln!(t, "dip_z_im_ohm = {:.4}", dip_z.im);
            for (f, s) in fixed {
                let key = format!("{f:.4}").replace('.', "p");
                let _ = writeln!(
                    t,
                    "s11_{key}_ghz = {{ re = {:.6e}, im = {:.6e}, db = {:.4} }}",
                    s.re,
                    s.im,
                    20.0 * s.norm().log10()
                );
            }
            t.push('\n');
        }
    }

    // Observed orders and extrapolation over the p=1 h-ladder.
    let hs: Vec<f64> = levels.iter().map(|l| l.h).collect();
    for g in ["flat", "bent"] {
        if !levels.iter().all(|l| l.geoms.contains_key(g)) {
            continue;
        }
        let _ = writeln!(t, "[order.{g}]");
        t.push_str(
            "# Observed order p: least-squares slope of log|q_L - q_(L+1)| against log h_L over the \
             consecutive level pairs (q = dip frequency, dip depth, complex S11 at a fixed frequency). \
             Extrapolated values use the finest pair at the fitted order.\n",
        );
        let fs: Vec<f64> = levels.iter().map(|l| l.geoms[g].0).collect();
        let dbs: Vec<f64> = levels.iter().map(|l| l.geoms[g].1).collect();
        let zr: Vec<f64> = levels.iter().map(|l| l.geoms[g].3.re).collect();
        let _ = writeln!(t, "dip_f_ghz = [{}]", fmt_list(&fs, 6));
        let _ = writeln!(t, "dip_db = [{}]", fmt_list(&dbs, 4));
        t.push_str(
            "# dip_z_re_ohm: Re Z_in at the sampled dip. The dip depth in dB is set by how closely \
             Z_in passes 50 ohm, so it is an ill-conditioned convergence metric; Re Z_in at the dip \
             and the dip frequency are the robust ones.\n",
        );
        let _ = writeln!(t, "dip_z_re_ohm = [{}]", fmt_list(&zr, 4));
        if let Some(p) = observed_order(&hs, &zr) {
            let _ = writeln!(t, "dip_z_re_order = {p:.3}");
        }
        let n = levels.len();
        let diffs_f: Vec<f64> = (0..n - 1).map(|i| (fs[i + 1] - fs[i]) * 1e3).collect();
        let diffs_db: Vec<f64> = (0..n - 1).map(|i| dbs[i + 1] - dbs[i]).collect();
        let _ = writeln!(
            t,
            "dip_f_consecutive_change_mhz = [{}]",
            fmt_list(&diffs_f, 3)
        );
        let _ = writeln!(
            t,
            "dip_db_consecutive_change = [{}]",
            fmt_list(&diffs_db, 3)
        );
        match observed_order(&hs, &fs) {
            Some(p) => {
                let _ = writeln!(t, "dip_f_order = {p:.3}");
                let _ = writeln!(
                    t,
                    "dip_f_extrapolated_ghz = {:.6}",
                    extrapolate(hs[n - 2], hs[n - 1], fs[n - 2], fs[n - 1], p.max(0.5))
                );
            }
            None => t.push_str("# dip_f_order: not fittable\n"),
        }
        match observed_order(&hs, &dbs) {
            Some(p) => {
                let _ = writeln!(t, "dip_db_order = {p:.3}");
                let _ = writeln!(
                    t,
                    "dip_db_extrapolated = {:.4}",
                    extrapolate(hs[n - 2], hs[n - 1], dbs[n - 2], dbs[n - 1], p.max(0.5))
                );
            }
            None => t.push_str("# dip_db_order: not fittable\n"),
        }
        for &f in &fixed_ghz {
            // Only the levels that sampled this frequency (the finest level
            // and the p=2 rows may carry a shorter fixed list).
            let (hs_f, ss): (Vec<f64>, Vec<c64>) = levels
                .iter()
                .filter_map(|l| {
                    l.geoms[g]
                        .2
                        .iter()
                        .find(|p| (p.0 - f).abs() < 1e-6)
                        .map(|p| (l.h, p.1))
                })
                .unzip();
            let key = format!("{f:.4}").replace('.', "p");
            if ss.len() < 2 {
                let _ = writeln!(t, "# s11_{key}_ghz: sampled on fewer than two levels");
                continue;
            }
            let m_lv = ss.len();
            let sampled: Vec<String> = levels
                .iter()
                .filter(|l| l.geoms[g].2.iter().any(|p| (p.0 - f).abs() < 1e-6))
                .map(|l| format!("\"{}\"", l.name))
                .collect();
            let _ = writeln!(t, "s11_{key}_ghz_levels = [{}]", sampled.join(", "));
            // Order of the complex S11 from the consecutive complex differences.
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            for i in 0..m_lv - 1 {
                let e = (ss[i] - ss[i + 1]).norm();
                if e > 0.0 {
                    xs.push(hs_f[i].ln());
                    ys.push(e.ln());
                }
            }
            let diffs: Vec<f64> = (0..m_lv - 1).map(|i| (ss[i + 1] - ss[i]).norm()).collect();
            let _ = writeln!(
                t,
                "s11_{key}_ghz_consecutive_change = [{}]",
                fmt_list(&diffs, 6)
            );
            if xs.len() >= 2 {
                let m = xs.len() as f64;
                let mx = xs.iter().sum::<f64>() / m;
                let my = ys.iter().sum::<f64>() / m;
                let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
                let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
                let _ = writeln!(t, "s11_{key}_ghz_order = {:.3}", sxy / sxx);
            }
        }
        // Finest-pair statement for gate G1.
        let _ = writeln!(
            t,
            "finest_pair = [\"{}\", \"{}\"]",
            levels[n - 2].name,
            levels[n - 1].name
        );
        let _ = writeln!(
            t,
            "finest_pair_dip_f_change_mhz = {:.3}",
            (fs[n - 1] - fs[n - 2]) * 1e3
        );
        let _ = writeln!(
            t,
            "finest_pair_dip_db_change = {:.3}",
            dbs[n - 1] - dbs[n - 2]
        );
        t.push('\n');
    }

    // p=2 against the p=1 ladder, where available.
    for r in &p2_rows {
        let p1 = levels.iter().find(|l| l.name == r.name).unwrap();
        let _ = writeln!(t, "[p2_vs_p1.{}]", r.name);
        for g in ["flat", "bent"] {
            if let (Some(a), Some(b)) = (r.geoms.get(g), p1.geoms.get(g)) {
                let _ = writeln!(t, "{g}_dip_f_shift_mhz = {:.3}", (a.0 - b.0) * 1e3);
                let _ = writeln!(t, "{g}_dip_db_change = {:.3}", a.1 - b.1);
                // Matched by frequency: the p=2 row may carry a shorter
                // fixed list than its p=1 level.
                let mut worst = 0.0_f64;
                let mut shared: Vec<f64> = Vec::new();
                for (fa, sa) in &a.2 {
                    if let Some((_, sb)) = b.2.iter().find(|(fb, _)| (fa - fb).abs() < 1e-9) {
                        worst = worst.max((sa - sb).norm());
                        shared.push(*fa);
                    }
                }
                assert!(
                    !shared.is_empty(),
                    "{}: p=2 and p=1 share no fixed frequency",
                    r.name
                );
                let _ = writeln!(t, "{g}_fixed_ghz_compared = [{}]", fmt_list(&shared, 4));
                let _ = writeln!(t, "{g}_max_fixed_s11_change = {worst:.6e}");
            }
        }
        t.push('\n');
    }

    // Detune per level.
    t.push_str("[detune]\n# bent minus flat dip, per level (MHz) and depth change (dB).\n");
    for r in levels.iter().chain(p2_rows.iter()) {
        if let (Some(f), Some(b)) = (r.geoms.get("flat"), r.geoms.get("bent")) {
            let _ = writeln!(
                t,
                "{}_{} = {{ dip_shift_mhz = {:.3}, dip_depth_change_db = {:.3} }}",
                r.name,
                r.order,
                (b.0 - f.0) * 1e3,
                b.1 - f.1
            );
        }
    }
    t.push('\n');

    // ISM-band figures from the dense baselines.
    for (lv, doc) in &dense {
        let _ = writeln!(t, "[dense.{lv}]");
        let _ = writeln!(
            t,
            "fixture_sha256 = \"{}\"",
            doc["meta"]["fixture_sha256"].as_str().unwrap()
        );
        let _ = writeln!(t, "upml_sigma_0 = {}", num(&doc["meta"]["upml_sigma_0"]));
        let _ = writeln!(
            t,
            "n_points = {}",
            doc["sweep"]["bent"].as_array().unwrap().len()
        );
        for g in ["flat", "bent"] {
            let s = &doc["summary"][g];
            let _ = writeln!(
                t,
                "{g} = {{ dip_f_ghz = {:.6}, dip_db = {:.4}, ism_worst_s11_db = {:.4}, ism_best_s11_db = {:.4} }}",
                num(&s["s11_dip_f_ghz"]),
                num(&s["s11_dip_db"]),
                num(&s["ism_worst_s11_db"]),
                num(&s["ism_best_s11_db"])
            );
        }
        t.push('\n');
    }

    // Cost table.
    t.push_str("[cost]\n");
    t.push_str(
        "# forward_solve_s: mean per port-driven solve (UPML tensors + assembly + LU + solve). \
         gradient_s: one band gradient = per band frequency one forward + one adjoint on a single LU \
         (3 frequencies). total_s = forward_solve_s + gradient_s. RSS: max of ps -o rss= samples.\n",
    );
    let _ = writeln!(
        t,
        "host = \"{}\"",
        std::env::var("PATCH_CONFORMAL_HOST").unwrap_or_else(|_| "unrecorded".into())
    );
    t.push('\n');
    for r in levels.iter().chain(p2_rows.iter()) {
        let key = format!("{}_{}", r.name, r.order);
        let _ = writeln!(t, "[cost.{key}]");
        let _ = writeln!(t, "n_dofs = {}", r.n_dofs);
        let _ = writeln!(t, "forward_solve_s = {:.3}", r.solve_s_mean);
        let _ = writeln!(t, "first_solve_peak_rss_gib = {:.3}", r.first_rss);
        let _ = writeln!(t, "forward_peak_rss_gib = {:.3}", r.peak_rss);
        if let Some((_, fd)) = fd_docs
            .iter()
            .find(|(lv, _)| *lv == r.name && r.order == "p1")
        {
            let gs = num(&fd["gradient"]["wall_s"]);
            let _ = writeln!(t, "gradient_s = {gs:.3}");
            let _ = writeln!(
                t,
                "gradient_per_frequency_s = {:.3}",
                num(&fd["gradient"]["per_frequency_s"])
            );
            let _ = writeln!(
                t,
                "gradient_peak_rss_gib = {:.3}",
                num(&fd["gradient"]["peak_rss_gib"])
            );
            let _ = writeln!(t, "forward_band_s = {:.3}", num(&fd["forward"]["wall_s"]));
            let _ = writeln!(t, "total_s = {:.3}", r.solve_s_mean + gs);
            let _ = writeln!(
                t,
                "n_design_dofs = {}",
                fd["model"]["n_design_dofs"].as_integer().unwrap()
            );
        }
        t.push('\n');
    }

    // FD gate.
    for (lv, fd) in &fd_docs {
        let _ = writeln!(t, "[fd_gate.{lv}]");
        let gate = &fd["fd_gate"];
        let _ = writeln!(
            t,
            "n_directions = {}",
            gate["n_directions"].as_integer().unwrap()
        );
        let _ = writeln!(t, "max_rel_err = {:.6e}", num(&gate["max_rel_err"]));
        let _ = writeln!(t, "tolerance = {:e}", num(&gate["tolerance"]));
        let _ = writeln!(t, "pass = {}", gate["pass"].as_bool().unwrap());
        let _ = writeln!(
            t,
            "band_objective = {:.9e}",
            num(&fd["gradient"]["band_objective"])
        );
        let _ = writeln!(t, "grad_norm = {:.9e}", num(&fd["gradient"]["grad_norm"]));
        let rels: Vec<f64> = fd["directions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| num(&d["rel_err"]))
            .collect();
        let _ = writeln!(t, "rel_err = [{}]", fmt_list(&rels, 9));
        t.push('\n');
    }

    if let Some(l) = &ladder {
        t.push_str("[upml]\n");
        let c = &l["choice"];
        let _ = writeln!(t, "chosen_sigma_0 = {}", num(&c["sigma_0"]));
        let _ = writeln!(
            t,
            "residual_sensitivity_s11 = {:.6e}",
            num(&c["residual_sensitivity_s11"])
        );
        let _ = writeln!(t, "ladder_span_s11 = {:.6e}", num(&c["ladder_span_s11"]));
        let _ = writeln!(
            t,
            "flat_dip_db_sigma_1_vs_25 = [{}]",
            fmt_list(
                &c["flat_dip_db_sigma_1_vs_25"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(num)
                    .collect::<Vec<_>>(),
                3
            )
        );
        t.push_str("source = \"upml_ladder_L1.toml\"\n\n");
    }

    let out = out.unwrap_or_else(|| dir.join("convergence.toml"));
    fs::write(&out, &t).expect("write convergence toml");
    println!("wrote {}", out.display());
}

// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: patch_conformal_convergence (dip <bent.msh> [--p2] | fd <bent.msh> | \
                 merge <flat-dip.toml> <bent-dip.toml> | ladder <run-dir> | \
                 aggregate [--dir <dir>]) [--out <toml>]";
    let mode = args.first().unwrap_or_else(|| panic!("{usage}")).clone();
    let mut out: Option<PathBuf> = None;
    let mut dir: Option<PathBuf> = None;
    let mut p2 = false;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                out = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--dir" => {
                dir = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--p2" => p2 = true,
            a => positional.push(a.to_string()),
        }
        i += 1;
    }
    match mode.as_str() {
        "dip" => run_dip(
            Path::new(positional.first().unwrap_or_else(|| panic!("{usage}"))),
            p2,
            out,
        ),
        "fd" => run_fd(
            Path::new(positional.first().unwrap_or_else(|| panic!("{usage}"))),
            out,
        ),
        "merge" => {
            assert_eq!(positional.len(), 2, "{usage}");
            run_merge(Path::new(&positional[0]), Path::new(&positional[1]), out);
        }
        "ladder" => run_ladder(
            Path::new(positional.first().unwrap_or_else(|| panic!("{usage}"))),
            out,
        ),
        "aggregate" => run_aggregate(
            &dir.unwrap_or_else(|| repo_root().join("benchmarks/patch_antenna_conformal")),
            out,
        ),
        _ => panic!("{usage}"),
    }
}
