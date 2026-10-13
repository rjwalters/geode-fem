//! **Nested-DOF shape optimization of the curved conformal 2.4 GHz patch
//! on a mesh-independent smooth basis** (issue #1087, conformal v5 E4).
//!
//! Runs ONE configuration of the design freedom on ONE level of the
//! `patch_conformal_L*` fixture family (issue #1084: the 38 × 29 mm
//! `patch_2g4_matched` patch on 1.6 mm FR-4, bent at R = 100 mm) and drives
//! a soft-max of `|S₁₁|²` over the 2.4–2.4835 GHz ISM band down with the
//! composed open-radiator shape adjoint
//! [`driven_shape_gradient_matched_upml_ports`] (one forward + one adjoint
//! solve per band frequency, one complex LU each) and an L-BFGS descent
//! with a backtracking line search and a per-tet non-inversion guard.
//!
//! # Design freedom
//!
//! The design vector `x` acts through two mesh-independent bases written
//! in the chart `(s, y)` of the bent patch
//! ([`geode_core::shape::conformal_basis`]):
//!
//! * **Radial (gap) DOFs**, `--radial NSxNY`: a tensor-product B-spline
//!   over the patch (times a smooth cutoff at the pinned feed and a decay
//!   over the substrate margin) moving every substrate-top node along the
//!   outward radial normal by `0.5 h` mm per unit coefficient. `1x1` is
//!   the uniform gap change; an optional Helmholtz filter
//!   (`--filter-radius r`) smooths the coefficient lattice.
//! * **Outline (tangential) DOFs**, `--outline N`: one 1-D B-spline of `N`
//!   functions per patch edge, moving the edge outward within the surface
//!   by 1 mm per unit coefficient, carried into the slab by a ramp from the
//!   pinned feed so the thin substrate translates instead of shearing.
//!
//! The initial design `x = 0` is the fixture itself. The gradient is
//! finite-difference validated at `x = 0` (steepest-descent direction and
//! one random direction per DOF group) and at the final design (random
//! direction). The final design is verified by a fresh forward sweep over
//! the band (`--verify-n` points) and a wider 2.20–2.70 GHz sweep for the
//! resonance location (the initial design's wide sweep is the #1084
//! baseline artifact).
//!
//! # Units
//!
//! Lengths in mm, frequencies in GHz (`ω = 2πf/c` in rad/mm inside the
//! solver), port resistance 50 Ω (`50/η₀` in the solver). The UPML σ₀ is
//! the fixture's unless `--sigma0` overrides it; #1085 found σ₀ dominates
//! S₁₁, so the value used is recorded in `[meta]`.
//!
//! # Usage
//!
//! ```text
//!   cargo run -p geode-core --release --example patch_conformal_nested_dof -- \
//!       --fixture <dir>/patch_conformal_L1.msh --radial 8x6 --outline 4 \
//!       --label combined --out benchmarks/patch_antenna_conformal/nested_dof/L1_combined.toml \
//!       --geometry benchmarks/patch_antenna_conformal/geometry/L1_combined.json
//! ```
//!
//! The geometry dump (`--geometry PATH`, JSON) carries the base and moved
//! coordinates of the substrate-top nodes (patch nodes flagged) with the
//! patch and substrate-top triangles and the patch outline loop, in mm, in
//! the bent frame; the figure script
//! `benchmarks/patch_antenna_conformal/scripts/plot_patch_displacement.py`
//! draws it.
//!
//! Flags: `--fixture PATH` (default: the committed L0), `--radial NSxNY`
//! (`0` = none), `--outline N` (`0` = none), `--filter-radius MM`,
//! `--n-band N` (band points, default 5), `--beta B` (soft-max sharpness,
//! default 25), `--max-steps N` (default 60), `--sigma0 S`, `--verify-n N`
//! (default 9), `--label NAME`, `--out PATH`, `--geometry PATH`,
//! `--fd-only` (validate the gradient, skip the descent).

use std::collections::{BTreeMap, VecDeque};
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
use geode_core::driven::extraction::{s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::driven_shape_gradient_matched_upml_ports;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, driven_solve_with_ports,
};
use geode_core::mesh::patch::{FR4_MATERIALS, ISM_2G4_BAND_GHZ, bend_axis_z};
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles, read_patch_fixture_path};
use geode_core::shape::conformal_basis::{
    ConformalPatchNodes, CylinderChart, FeedCutoff, OutlineBasis, PatchRect, RadialBasis,
    filter_columns, helmholtz_lattice_filter, outline_columns, radial_columns,
    substrate_top_triangles,
};
use geode_core::shape::{FreeformBoundaryMorph, min_tet_volume_ratio, worst_tet_volume_ratio};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;
use geode_util::units::ghz_to_omega_mm;

type B = TestBackend;

/// Port reference resistance (Ω).
const R_PORT_OHM: f64 = 50.0;
/// Target: `|S₁₁| ≤ −10 dB` across the band.
const TARGET_DB: f64 = -10.0;
/// Hard per-tet signed-volume-ratio guard.
const MIN_VOL_RATIO: f64 = 0.25;
/// Radial motion per unit coefficient, as a fraction of the substrate thickness.
const RADIAL_SCALE_FRAC: f64 = 0.5;
/// Tangential (outline) motion per unit coefficient (mm).
const OUTLINE_SCALE_MM: f64 = 1.0;
/// B-spline degree of both bases (reduced automatically for tiny lattices).
const SPLINE_DEGREE: usize = 3;
/// Feed cutoff: zero within `FEED_R_INNER × probe_w`, one beyond `FEED_R_OUTER × probe_w`.
const FEED_R_INNER: f64 = 0.5;
const FEED_R_OUTER: f64 = 2.5;
/// L-BFGS memory.
const LBFGS_M: usize = 8;
/// Largest per-coefficient change in one step (design units).
const MAX_STEP_INF: f64 = 1.0;
/// Armijo constant and backtracking cap.
const ARMIJO_C: f64 = 1e-4;
const MAX_BACKTRACKS: usize = 25;
/// Convergence: relative gradient norm, plateau tolerance and window.
const GRAD_TOL_REL: f64 = 1e-3;
const PLATEAU_TOL: f64 = 5e-4;
const PLATEAU_WINDOW: usize = 4;
/// FD step (design units) and tolerance.
const FD_H: f64 = 1e-5;
const FD_TOL: f64 = 5e-3;
/// RSS sampling period.
const RSS_SAMPLE: Duration = Duration::from_millis(100);

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

// ── CLI ──────────────────────────────────────────────────────────────────

struct Cli {
    fixture: PathBuf,
    radial: Option<(usize, usize)>,
    outline: usize,
    filter_radius: f64,
    n_band: usize,
    beta: f64,
    max_steps: usize,
    sigma0: Option<f64>,
    verify_n: usize,
    label: String,
    out: Option<PathBuf>,
    geometry: Option<PathBuf>,
    fd_only: bool,
}

fn parse_cli() -> Cli {
    let mut cli = Cli {
        fixture: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/patch_conformal_L0.msh"),
        radial: Some((1, 1)),
        outline: 0,
        filter_radius: 0.0,
        n_band: 5,
        beta: 25.0,
        max_steps: 60,
        sigma0: None,
        verify_n: 9,
        label: String::new(),
        out: None,
        geometry: None,
        fd_only: false,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let next = |i: &mut usize| -> String {
        *i += 1;
        args.get(*i)
            .unwrap_or_else(|| panic!("missing value after {}", args[*i - 1]))
            .clone()
    };
    while i < args.len() {
        match args[i].as_str() {
            "--fixture" => cli.fixture = PathBuf::from(next(&mut i)),
            "--radial" => {
                let v = next(&mut i);
                cli.radial = if v == "0" {
                    None
                } else {
                    let (a, b) = v.split_once('x').expect("--radial NSxNY");
                    Some((a.parse().expect("NS"), b.parse().expect("NY")))
                };
            }
            "--outline" => cli.outline = next(&mut i).parse().expect("--outline N"),
            "--filter-radius" => cli.filter_radius = next(&mut i).parse().expect("mm"),
            "--n-band" => cli.n_band = next(&mut i).parse().expect("--n-band N"),
            "--beta" => cli.beta = next(&mut i).parse().expect("--beta B"),
            "--max-steps" => cli.max_steps = next(&mut i).parse().expect("--max-steps N"),
            "--sigma0" => cli.sigma0 = Some(next(&mut i).parse().expect("--sigma0 S")),
            "--verify-n" => cli.verify_n = next(&mut i).parse().expect("--verify-n N"),
            "--label" => cli.label = next(&mut i),
            "--out" => cli.out = Some(PathBuf::from(next(&mut i))),
            "--geometry" => cli.geometry = Some(PathBuf::from(next(&mut i))),
            "--fd-only" => cli.fd_only = true,
            other => panic!("unknown flag {other}"),
        }
        i += 1;
    }
    assert!(cli.n_band >= 2, "--n-band must be at least 2");
    assert!(
        cli.radial.is_some() || cli.outline > 0,
        "no design freedom: give --radial NSxNY and/or --outline N"
    );
    if cli.label.is_empty() {
        cli.label = match (cli.radial, cli.outline) {
            (Some((a, b)), 0) => format!("gap_{}", a * b),
            (None, n) => format!("outline_{}", 4 * n),
            (Some((a, b)), n) => format!("combined_{}_{}", a * b, 4 * n),
        };
    }
    cli
}

// ── Provenance ───────────────────────────────────────────────────────────

/// The generator's `<stem>.provenance.txt`: top-level `key: value` lines
/// and `section.key` parameter values.
struct Provenance {
    top: BTreeMap<String, String>,
    params: BTreeMap<String, String>,
}

impl Provenance {
    fn read(path: &Path) -> Self {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read provenance {}: {e}", path.display()));
        let mut top = BTreeMap::new();
        let mut params = BTreeMap::new();
        let mut block = String::new();
        let mut section = String::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            let Some((k, v)) = line.trim().split_once(':') else {
                continue;
            };
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            match indent {
                0 => {
                    if v.is_empty() {
                        block = k;
                    } else {
                        block.clear();
                        top.insert(k, v);
                    }
                }
                2 if block == "parameter values" => section = k,
                4 if block == "parameter values" => {
                    params.insert(format!("{section}.{k}"), v);
                }
                _ => {}
            }
        }
        Provenance { top, params }
    }

    fn param(&self, key: &str) -> f64 {
        self.params
            .get(key)
            .unwrap_or_else(|| panic!("provenance lacks parameter {key}"))
            .parse()
            .unwrap_or_else(|e| panic!("provenance parameter {key}: {e}"))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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
    (out, peak_kib.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0))
}

/// xorshift64* for the random FD directions (reproducible, no dependency).
struct Rng(u64);
impl Rng {
    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }
    fn unit_direction(&mut self, n: usize, mask: impl Fn(usize) -> bool) -> Vec<f64> {
        let mut d: Vec<f64> = (0..n)
            .map(|i| if mask(i) { 2.0 * self.next_f64() - 1.0 } else { 0.0 })
            .collect();
        let norm = d.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-300);
        for v in d.iter_mut() {
            *v /= norm;
        }
        d
    }
}

// ── Model ────────────────────────────────────────────────────────────────

/// Per-frequency matched-UPML tensors `(f_ghz, ω, ε, ν)`.
type BandPoint = (f64, f64, Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>);

struct Model {
    base: PatchFixture,
    morph: FreeformBoundaryMorph,
    band: Vec<BandPoint>,
    beta: f64,
    mask: Vec<bool>,
    source: CurrentSource,
    flux: Vec<f64>,
    inv_width: f64,
    v_inc: c64,
    r_nat: f64,
    port: geode_core::mesh::patch::PatchPort,
    air_lo: [f64; 3],
    air_hi: [f64; 3],
    pml_thick: f64,
    sigma0: f64,
    /// Total driven solves (forward or forward+adjoint) so far.
    n_solves: std::cell::Cell<usize>,
}

/// One band evaluation: per-point `|S₁₁|²`, `S₁₁`, the soft-max objective
/// and (when requested) its design gradient.
struct Eval {
    g_f: Vec<f64>,
    s11: Vec<c64>,
    objective: f64,
    grad: Option<Vec<f64>>,
    worst_residual: f64,
    min_vol_ratio: f64,
}

impl Eval {
    fn worst_db(&self) -> f64 {
        self.g_f
            .iter()
            .map(|g| 10.0 * g.log10())
            .fold(f64::NEG_INFINITY, f64::max)
    }
    fn best_db(&self) -> f64 {
        self.g_f
            .iter()
            .map(|g| 10.0 * g.log10())
            .fold(f64::INFINITY, f64::min)
    }
}

/// Soft-max `G = β⁻¹ ln Σ_f exp(β g_f)` and its weights `∂G/∂g_f`.
fn softmax(beta: f64, g: &[f64]) -> (f64, Vec<f64>) {
    let m = g.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let e: Vec<f64> = g.iter().map(|v| (beta * (v - m)).exp()).collect();
    let z: f64 = e.iter().sum();
    (m + z.ln() / beta, e.iter().map(|v| v / z).collect())
}

impl Model {
    fn moved_mesh(&self, x: &[f64]) -> geode_core::mesh::TetMesh {
        self.morph.apply(&self.base.mesh, x)
    }

    fn min_vol_ratio(&self, x: &[f64]) -> f64 {
        min_tet_volume_ratio(&self.base.mesh, &self.moved_mesh(x))
    }

    /// Forward (and, with `grad`, adjoint) solves at every band point.
    fn eval(&self, x: &[f64], grad: bool) -> Eval {
        let mesh = self.moved_mesh(x);
        let min_vol_ratio = min_tet_volume_ratio(&self.base.mesh, &mesh);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        let n_nodes = mesh.n_nodes();
        let mut g_f = Vec::with_capacity(self.band.len());
        let mut s11 = Vec::with_capacity(self.band.len());
        let mut grads: Vec<Vec<[f64; 3]>> = Vec::new();
        let mut worst_res = 0.0_f64;
        for (_f, omega, eps, nu) in &self.band {
            self.n_solves.set(self.n_solves.get() + 1);
            if grad {
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
                .expect("composed shape gradient");
                assert_eq!(sg.n_factorizations, 1, "adjoint must reuse the forward LU");
                worst_res = worst_res.max(sg.residual_rel);
                let v = self.port_voltage(&sg.e_edges);
                let (g, _) = s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.r_nat);
                assert!(
                    (g - sg.objective).abs() <= 1e-9 * g.max(1.0),
                    "objective readback mismatch"
                );
                g_f.push(g);
                s11.push(self.s11_of_v(v));
                grads.push(sg.grad_node);
            } else {
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
                worst_res = worst_res.max(sol.residual_rel);
                let v = self.port_voltage(&sol.e_edges);
                let (g, _) = s11_sq_and_dg_dv(v, self.v_inc, self.r_nat, self.r_nat);
                g_f.push(g);
                s11.push(self.s11_of_v(v));
            }
        }
        for (k, s) in s11.iter().enumerate() {
            assert!(
                s.norm() <= 1.0 + 1e-6,
                "passivity tripwire: |S11| = {} > 1 at {} GHz",
                s.norm(),
                self.band[k].0
            );
        }
        let (objective, w) = softmax(self.beta, &g_f);
        let design_grad = grad.then(|| {
            let mut acc = vec![[0.0_f64; 3]; n_nodes];
            for (wf, gn) in w.iter().zip(&grads) {
                for (a, g) in acc.iter_mut().zip(gn) {
                    a[0] += wf * g[0];
                    a[1] += wf * g[1];
                    a[2] += wf * g[2];
                }
            }
            self.morph.design_gradient(&acc)
        });
        Eval {
            g_f,
            s11,
            objective,
            grad: design_grad,
            worst_residual: worst_res,
            min_vol_ratio,
        }
    }

    fn port_voltage(&self, e: &[c64]) -> c64 {
        let mut v = c64::new(0.0, 0.0);
        for (f, x) in self.flux.iter().zip(e) {
            v += *x * *f;
        }
        v * self.inv_width
    }

    fn s11_of_v(&self, v: c64) -> c64 {
        let i = (self.v_inc * 2.0 - v) * (1.0 / self.r_nat);
        let z = v / i;
        (z - self.r_nat) / (z + self.r_nat)
    }

    /// Fresh forward `S₁₁` sweep at `x` over arbitrary frequencies (GHz),
    /// building the matched-UPML tensors per point.
    fn sweep(&self, x: &[f64], freqs: &[f64]) -> Vec<(f64, c64, f64)> {
        let mesh = self.moved_mesh(x);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let lp = self.port.lumped_port(self.r_nat, self.v_inc);
        freqs
            .iter()
            .map(|&f| {
                let omega = ghz_to_omega_mm(f);
                let (eps, nu) = self.base.matched_upml_materials(
                    &FR4_MATERIALS,
                    self.air_lo,
                    self.air_hi,
                    self.pml_thick,
                    self.sigma0,
                    omega,
                );
                self.n_solves.set(self.n_solves.get() + 1);
                let sol = driven_solve_with_ports::<B>(
                    &mesh,
                    DrivenMaterials::MatchedUpml {
                        epsilon_tensor: &eps,
                        nu_tensor: &nu,
                    },
                    None,
                    &bcs,
                    std::slice::from_ref(&lp),
                    omega,
                    &self.source,
                    &device(),
                )
                .expect("sweep solve");
                let v = self.port_voltage(&sol.e_edges);
                let s = self.s11_of_v(v);
                assert!(s.norm() <= 1.0 + 1e-6, "passivity at {f} GHz");
                let i = (self.v_inc * 2.0 - v) * (1.0 / self.r_nat);
                let z = v / i * ETA_0;
                eprintln!(
                    "    sweep {f:.4} GHz: |S11| = {:.4} ({:6.2} dB), Z = {:.1} {:+.1}i ohm",
                    s.norm(),
                    20.0 * s.norm().log10(),
                    z.re,
                    z.im
                );
                (f, s, sol.residual_rel)
            })
            .collect()
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

struct FdCheck {
    label: String,
    at: String,
    analytic: f64,
    fd: f64,
    rel: f64,
}

/// Central finite difference of the band objective along `d` at `x`.
fn fd_check(model: &Model, x: &[f64], grad: &[f64], d: &[f64], label: &str, at: &str) -> FdCheck {
    let xp: Vec<f64> = x.iter().zip(d).map(|(a, b)| a + FD_H * b).collect();
    let xm: Vec<f64> = x.iter().zip(d).map(|(a, b)| a - FD_H * b).collect();
    let gp = model.eval(&xp, false).objective;
    let gm = model.eval(&xm, false).objective;
    let fd = (gp - gm) / (2.0 * FD_H);
    let analytic = dot(grad, d);
    let rel = (analytic - fd).abs() / fd.abs().max(1e-300);
    eprintln!("  FD [{label} @ {at}]: analytic {analytic:.6e}, central FD {fd:.6e}, rel {rel:.3e}");
    assert!(fd.abs() > 1e-12, "FD derivative ~0 along {label}");
    assert!(
        rel < FD_TOL,
        "FD gate failed along {label} at {at}: rel {rel:.3e} (tol {FD_TOL:.1e})"
    );
    FdCheck {
        label: label.to_string(),
        at: at.to_string(),
        analytic,
        fd,
        rel,
    }
}

struct Step {
    iter: usize,
    objective: f64,
    worst_db: f64,
    grad_norm: f64,
    min_vol_ratio: f64,
    step_len: f64,
    backtracks: usize,
    wall_s: f64,
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[allow(clippy::too_many_lines)]
fn main() {
    let cli = parse_cli();
    let t_start = Instant::now();

    // ── Fixture + provenance ──
    let bent_path = cli.fixture.clone();
    let stem = bent_path
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("fixture stem")
        .to_string();
    let level = stem.rsplit('_').next().unwrap_or("L?").to_string();
    let dir = bent_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let prov = Provenance::read(&dir.join(format!("{stem}.provenance.txt")));
    let bytes = fs::read(&bent_path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", bent_path.display()));
    let fixture_sha = sha256_hex(&bytes);
    assert_eq!(
        Some(&fixture_sha),
        prov.top.get("sha256"),
        "fixture sha256 does not match its provenance"
    );
    drop(bytes);
    let base = read_patch_fixture_path(&bent_path).expect("bent fixture");

    let bend_radius = prov.param("bend.bend_radius");
    let h = prov.param("patch.h");
    let patch_w = prov.param("patch.patch_w");
    let patch_l = prov.param("patch.patch_l");
    let sub_pad = prov.param("patch.sub_pad");
    let probe_w = prov.param("patch.probe_w");
    let probe_inset = prov.param("patch.probe_inset");
    let pml_thick = prov.param("domain.pml_thick");
    let sigma0_fixture = prov.param("solver.upml_sigma_0");
    let sigma0 = cli.sigma0.unwrap_or(sigma0_fixture);
    assert_eq!(prov.param("solver.port_resistance_ohm"), R_PORT_OHM);

    // ── Chart, node roles, bases, morph ──
    let chart = CylinderChart {
        bend_radius,
        z_axis: bend_axis_z(h, bend_radius),
    };
    let nodes = ConformalPatchNodes::identify(&base, &chart, 1e-6);
    assert!(
        (nodes.r_top - (bend_radius + 0.5 * h)).abs() < 1e-6,
        "patch radius {} != R + h/2",
        nodes.r_top
    );
    let rect = PatchRect {
        s: [-0.5 * patch_w, 0.5 * patch_w],
        y: [-0.5 * patch_l, 0.5 * patch_l],
    };
    let feed = (0.0, 0.5 * patch_l - probe_inset);
    let radial_scale = RADIAL_SCALE_FRAC * h;

    let t_morph = Instant::now();
    let mut columns: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut radial_spec = String::from("none");
    let mut n_radial = 0;
    let radial_basis = cli.radial.map(|(ns, ny)| {
        RadialBasis::new(
            ns,
            ny,
            SPLINE_DEGREE,
            rect,
            Some(FeedCutoff {
                s: feed.0,
                y: feed.1,
                r_inner: FEED_R_INNER * probe_w,
                r_outer: FEED_R_OUTER * probe_w,
            }),
            sub_pad,
        )
    });
    if let Some(rb) = &radial_basis {
        let mut cols = radial_columns(&base.mesh, &chart, rb, &nodes, radial_scale)
            .expect("radial columns");
        let (ns, ny) = rb.shape();
        radial_spec = format!(
            "{ns}x{ny} clamped uniform B-splines (degree {}/{}) in (s, y) over the patch, \
             times feed cutoff (0 within {:.2} mm, 1 beyond {:.2} mm) and a {sub_pad} mm \
             smoothstep margin decay; outward radial motion {radial_scale} mm per unit",
            ns.saturating_sub(1).min(SPLINE_DEGREE),
            ny.saturating_sub(1).min(SPLINE_DEGREE),
            FEED_R_INNER * probe_w,
            FEED_R_OUTER * probe_w
        );
        if cli.filter_radius > 0.0 {
            let (ds, dy) = rb.lattice_spacing();
            let f = helmholtz_lattice_filter(ns, ny, ds, dy, cli.filter_radius);
            cols = filter_columns(&cols, &f);
            let _ = write!(
                radial_spec,
                "; Helmholtz lattice filter (I + r^2 L)^-1 with r = {} mm",
                cli.filter_radius
            );
        }
        n_radial = cols.len();
        columns.extend(cols);
    }
    let mut outline_spec = String::from("none");
    let mut n_outline = 0;
    let outline_basis = (cli.outline > 0).then(|| {
        OutlineBasis::new(
            cli.outline,
            SPLINE_DEGREE,
            rect,
            feed.0,
            feed.1,
            0.5 * probe_w,
            sub_pad,
        )
    });
    if let Some(ob) = &outline_basis {
        let cols = outline_columns(&base.mesh, &chart, ob, &nodes, OUTLINE_SCALE_MM)
            .expect("outline columns");
        outline_spec = format!(
            "{} clamped uniform B-splines (degree {}) per patch edge [+y, -y, +s, -s], each DOF \
             moving its edge outward in the surface by {OUTLINE_SCALE_MM} mm per unit; carried \
             into the slab by a smoothstep ramp from the pinned feed band (half-width {:.2} mm) \
             to the edge on every substrate node (top, interior, ground) and decaying over the \
             {sub_pad} mm margin",
            cli.outline,
            cli.outline.saturating_sub(1).min(SPLINE_DEGREE),
            0.5 * probe_w
        );
        n_outline = cols.len();
        columns.extend(cols);
    }
    let morph = FreeformBoundaryMorph::from_columns(columns).expect("morph");
    let n_dofs = morph.n_dofs();
    let morph_s = t_morph.elapsed().as_secs_f64();

    // ── Driven model ──
    let edges = base.mesh.edges();
    let patch_tris = base.patch_triangles();
    let ground = base.ground_triangles();
    let outer = base.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(
        &edges,
        &[patch_tris.as_slice(), ground.as_slice(), outer.as_slice()],
    );
    let port = base.port();
    let r_nat = R_PORT_OHM / ETA_0;
    let flux = assemble_port_flux(&base.mesh, &port.faces, port.e_hat, &edges);
    let (air_lo, air_hi) = base.air_box(pml_thick);
    let band_f: Vec<f64> = (0..cli.n_band)
        .map(|i| {
            ISM_2G4_BAND_GHZ[0]
                + (ISM_2G4_BAND_GHZ[1] - ISM_2G4_BAND_GHZ[0]) * i as f64 / (cli.n_band - 1) as f64
        })
        .collect();
    let band: Vec<BandPoint> = band_f
        .iter()
        .map(|&f| {
            let omega = ghz_to_omega_mm(f);
            let (eps, nu) =
                base.matched_upml_materials(&FR4_MATERIALS, air_lo, air_hi, pml_thick, sigma0, omega);
            (f, omega, eps, nu)
        })
        .collect();
    let model = Model {
        source: CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; base.mesh.n_tets()],
        },
        base,
        morph,
        band,
        beta: cli.beta,
        mask,
        flux,
        inv_width: 1.0 / port.width,
        v_inc: c64::new(1.0, 0.0),
        r_nat,
        port,
        air_lo,
        air_hi,
        pml_thick,
        sigma0,
        n_solves: std::cell::Cell::new(0),
    };
    let n_nodes = model.base.mesh.n_nodes();
    let n_edges = edges.len();
    let n_tets = model.base.mesh.n_tets();
    eprintln!(
        "patch_conformal_nested_dof [{}]: {stem} ({n_edges} edges, {n_nodes} nodes, {n_tets} \
         tets), R = {bend_radius} mm, sigma_0 = {sigma0} (fixture {sigma0_fixture}); DOFs: \
         {n_radial} radial + {n_outline} outline = {n_dofs}; band {:?} GHz; morph built in \
         {morph_s:.1} s",
        cli.label, band_f
    );

    // ── Initial evaluation + FD gates ──
    let x0 = vec![0.0_f64; n_dofs];
    let t0 = Instant::now();
    let (e0, rss0) = with_peak_rss(|| model.eval(&x0, true));
    let eval_s = t0.elapsed().as_secs_f64();
    assert!(
        e0.worst_residual < 1e-6,
        "unhealthy forward (residual {:.2e})",
        e0.worst_residual
    );
    assert!((e0.min_vol_ratio - 1.0).abs() < 1e-9);
    let grad0 = e0.grad.clone().unwrap();
    let gnorm0 = norm(&grad0);
    eprintln!(
        "  x=0: G = {:.6e}, worst |S11| {:.2} dB, best {:.2} dB, |grad| = {gnorm0:.3e}, \
         residual {:.1e}; gradient eval {eval_s:.1} s, peak RSS {rss0:.2} GiB",
        e0.objective,
        e0.worst_db(),
        e0.best_db(),
        e0.worst_residual
    );
    for (k, s) in e0.s11.iter().enumerate() {
        eprintln!(
            "        {:.4} GHz: |S11| = {:.4} ({:6.2} dB)",
            band_f[k],
            s.norm(),
            20.0 * s.norm().log10()
        );
    }
    assert!(gnorm0 > 1e-12, "zero gradient at x = 0");
    let mut rng = Rng(0x1087_2026_u64);
    let mut fd_checks = Vec::new();
    {
        let d: Vec<f64> = grad0.iter().map(|g| -g / gnorm0).collect();
        fd_checks.push(fd_check(&model, &x0, &grad0, &d, "steepest_descent", "x0"));
        if n_radial > 0 {
            let d = rng.unit_direction(n_dofs, |i| i < n_radial);
            fd_checks.push(fd_check(&model, &x0, &grad0, &d, "random_radial", "x0"));
        }
        if n_outline > 0 {
            let d = rng.unit_direction(n_dofs, |i| i >= n_radial);
            fd_checks.push(fd_check(&model, &x0, &grad0, &d, "random_outline", "x0"));
        }
    }

    // ── L-BFGS descent with Armijo backtracking and the volume guard ──
    let mut x = x0.clone();
    let mut g_cur = e0.objective;
    let mut grad = grad0.clone();
    let mut steps = vec![Step {
        iter: 0,
        objective: e0.objective,
        worst_db: e0.worst_db(),
        grad_norm: gnorm0,
        min_vol_ratio: 1.0,
        step_len: 0.0,
        backtracks: 0,
        wall_s: t_start.elapsed().as_secs_f64(),
    }];
    let mut s_hist: VecDeque<Vec<f64>> = VecDeque::new();
    let mut y_hist: VecDeque<Vec<f64>> = VecDeque::new();
    let mut worst_vol = 1.0_f64;
    let mut total_backtracks = 0usize;
    let mut plateau = 0usize;
    let mut terminal = if cli.fd_only { "fd_only" } else { "max_steps" };
    let mut last_eval: Option<Eval> = None;
    let mut best_worst_db = e0.worst_db();
    let mut best_x = x0.clone();
    let max_steps = if cli.fd_only { 0 } else { cli.max_steps };

    for iter in 1..=max_steps {
        let gnorm = norm(&grad);
        if gnorm < GRAD_TOL_REL * gnorm0 {
            terminal = "converged_grad_norm";
            break;
        }
        // Two-loop recursion.
        let mut q = grad.clone();
        let m = s_hist.len();
        let mut alphas = vec![0.0_f64; m];
        for k in (0..m).rev() {
            let rho = 1.0 / dot(&y_hist[k], &s_hist[k]);
            alphas[k] = rho * dot(&s_hist[k], &q);
            for (qi, yi) in q.iter_mut().zip(&y_hist[k]) {
                *qi -= alphas[k] * yi;
            }
        }
        if m > 0 {
            let gamma = dot(&s_hist[m - 1], &y_hist[m - 1]) / dot(&y_hist[m - 1], &y_hist[m - 1]);
            for qi in q.iter_mut() {
                *qi *= gamma;
            }
        }
        for k in 0..m {
            let rho = 1.0 / dot(&y_hist[k], &s_hist[k]);
            let beta_k = rho * dot(&y_hist[k], &q);
            for (qi, si) in q.iter_mut().zip(&s_hist[k]) {
                *qi += (alphas[k] - beta_k) * si;
            }
        }
        let mut d: Vec<f64> = q.iter().map(|v| -v).collect();
        let mut slope = dot(&d, &grad);
        if slope >= 0.0 || m == 0 {
            // Not a descent direction (or no curvature yet): steepest descent
            // scaled so the largest coefficient moves by 0.25 design units.
            let gmax = grad.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
            d = grad.iter().map(|v| -0.25 * v / gmax).collect();
            slope = dot(&d, &grad);
            s_hist.clear();
            y_hist.clear();
        }
        // Cap the step in the max norm.
        let dmax = d.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
        let mut a = if dmax > MAX_STEP_INF { MAX_STEP_INF / dmax } else { 1.0 };

        let mut backtracks = 0;
        let mut guard_binding = false;
        let accepted = loop {
            let xt: Vec<f64> = x.iter().zip(&d).map(|(xi, di)| xi + a * di).collect();
            let vr = model.min_vol_ratio(&xt);
            if vr < MIN_VOL_RATIO {
                guard_binding = true;
            } else {
                let t_eval = Instant::now();
                let et = model.eval(&xt, true);
                let armijo = et.objective <= g_cur + ARMIJO_C * a * slope;
                eprintln!(
                    "    trial a = {a:.3e}: G = {:.6e} (worst {:.2} dB), vol {vr:.3}, {} ({:.0} s)",
                    et.objective,
                    et.worst_db(),
                    if armijo { "accept" } else { "reject" },
                    t_eval.elapsed().as_secs_f64()
                );
                if armijo {
                    break Some((xt, et, vr));
                }
            }
            if backtracks >= MAX_BACKTRACKS {
                break None;
            }
            a *= 0.5;
            backtracks += 1;
        };
        total_backtracks += backtracks;
        let Some((xn, en, vr)) = accepted else {
            terminal = if guard_binding {
                "distortion_limited"
            } else {
                "line_search_stall"
            };
            break;
        };
        let gn = en.grad.clone().unwrap();
        let s: Vec<f64> = xn.iter().zip(&x).map(|(a, b)| a - b).collect();
        let yv: Vec<f64> = gn.iter().zip(&grad).map(|(a, b)| a - b).collect();
        if dot(&s, &yv) > 1e-12 * norm(&s) * norm(&yv) {
            s_hist.push_back(s.clone());
            y_hist.push_back(yv);
            if s_hist.len() > LBFGS_M {
                s_hist.pop_front();
                y_hist.pop_front();
            }
        }
        let rel_drop = (g_cur - en.objective) / g_cur.max(1e-300);
        let wdb = en.worst_db();
        eprintln!(
            "  step {iter}: G = {:.6e} (worst |S11| {wdb:.2} dB, best {:.2} dB, drop {rel_drop:+.2e}), \
             |grad| = {:.3e}, vol_ratio = {vr:.3}, |dx| = {:.3e}, backtracks = {backtracks}, \
             {:.0} s elapsed",
            en.objective,
            en.best_db(),
            norm(&gn),
            norm(&s),
            t_start.elapsed().as_secs_f64()
        );
        x = xn;
        g_cur = en.objective;
        grad = gn;
        worst_vol = worst_vol.min(vr);
        let guard_binding_last = guard_binding;
        if wdb < best_worst_db {
            best_worst_db = wdb;
            best_x = x.clone();
        }
        steps.push(Step {
            iter,
            objective: g_cur,
            worst_db: wdb,
            grad_norm: norm(&grad),
            min_vol_ratio: vr,
            step_len: norm(&s),
            backtracks,
            wall_s: t_start.elapsed().as_secs_f64(),
        });
        last_eval = Some(en);
        if rel_drop.abs() < PLATEAU_TOL {
            plateau += 1;
        } else {
            plateau = 0;
        }
        if plateau >= PLATEAU_WINDOW {
            terminal = if guard_binding_last && vr <= MIN_VOL_RATIO * 1.05 {
                "distortion_limited"
            } else {
                "plateau"
            };
            break;
        }
    }
    // Report the best worst-of-band iterate (the soft-max can trade a hair of
    // worst-point for the aggregate on the last steps).
    let x_final = best_x;
    let e_final = if x_final == x {
        last_eval.unwrap_or_else(|| model.eval(&x_final, true))
    } else {
        model.eval(&x_final, true)
    };
    let grad_final = e_final.grad.clone().unwrap();
    let gnorm_final = norm(&grad_final);
    if n_dofs > 0 && !cli.fd_only && steps.len() > 1 {
        let d = rng.unit_direction(n_dofs, |_| true);
        fd_checks.push(fd_check(&model, &x_final, &grad_final, &d, "random_all", "x_final"));
    }

    // ── Verification sweeps at the final design (fresh forwards) ──
    let verify_f: Vec<f64> = (0..cli.verify_n)
        .map(|i| {
            ISM_2G4_BAND_GHZ[0]
                + (ISM_2G4_BAND_GHZ[1] - ISM_2G4_BAND_GHZ[0]) * i as f64
                    / (cli.verify_n - 1).max(1) as f64
        })
        .collect();
    eprintln!("  in-band verification sweep at x_final:");
    let verify = model.sweep(&x_final, &verify_f);
    // The initial design's wide sweep is the #1084 baseline artifact
    // (benchmarks/patch_antenna_conformal/baseline_2g4_<level>.toml); only
    // the final design is swept here.
    let wide_f: Vec<f64> = (0..=20).map(|i| 2.20 + 0.025 * i as f64).collect();
    eprintln!("  wide sweep at x_final:");
    let wide = model.sweep(&x_final, &wide_f);
    let (dip_f, dip_db) = wide
        .iter()
        .map(|(f, s, _)| (*f, 20.0 * s.norm().log10()))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap();
    let verify_worst = verify
        .iter()
        .map(|(_, s, _)| 20.0 * s.norm().log10())
        .fold(f64::NEG_INFINITY, f64::max);
    let verify_best = verify
        .iter()
        .map(|(_, s, _)| 20.0 * s.norm().log10())
        .fold(f64::INFINITY, f64::min);
    let reached = verify_worst <= TARGET_DB;
    let disp = model.morph.combined_velocity(&x_final);
    let max_disp = disp
        .iter()
        .map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
        .fold(0.0_f64, f64::max);
    let (worst_ratio_final, worst_tet) =
        worst_tet_volume_ratio(&model.base.mesh, &model.moved_mesh(&x_final));
    // Mean radial lift of the patch and mean outward edge motion.
    let mean_patch_radial = {
        let mut acc = 0.0;
        for &n in &nodes.patch {
            let p = model.base.mesh.nodes[n as usize];
            let r = chart.radial_dir(p);
            let d = disp[n as usize];
            acc += d[0] * r[0] + d[1] * r[1] + d[2] * r[2];
        }
        acc / nodes.patch.len() as f64
    };
    let edge_mean = |pick: &dyn Fn(f64, f64) -> bool, comp: &dyn Fn([f64; 3], [f64; 3]) -> f64| {
        let mut acc = 0.0;
        let mut n = 0usize;
        for &nn in &nodes.patch {
            let p = model.base.mesh.nodes[nn as usize];
            let (s, y) = chart.s_y(p);
            if pick(s, y) {
                acc += comp(p, disp[nn as usize]);
                n += 1;
            }
        }
        if n > 0 { acc / n as f64 } else { 0.0 }
    };
    let tol = 1e-6;
    let yp = edge_mean(&|_, y| (y - rect.y[1]).abs() < tol, &|_, d| d[1]);
    let ym = edge_mean(&|_, y| (y - rect.y[0]).abs() < tol, &|_, d| -d[1]);
    let sp = edge_mean(&|s, _| (s - rect.s[1]).abs() < tol, &|p, d| {
        let t = chart.tangent_s(p);
        d[0] * t[0] + d[2] * t[2]
    });
    let sm = edge_mean(&|s, _| (s - rect.s[0]).abs() < tol, &|p, d| {
        let t = chart.tangent_s(p);
        -(d[0] * t[0] + d[2] * t[2])
    });
    let total_s = t_start.elapsed().as_secs_f64();
    eprintln!(
        "  terminal = {terminal}; worst-of-band {:.2} -> {:.2} dB (band points), verification \
         worst {verify_worst:.2} dB / best {verify_best:.2} dB over {} points, final dip {dip_f:.3} \
         GHz ({dip_db:.1} dB); reached -10 dB = {reached}; max \
         disp {max_disp:.3} mm, mean patch lift {mean_patch_radial:+.3} mm, edges +y {yp:+.3} -y \
         {ym:+.3} +s {sp:+.3} -s {sm:+.3} mm; {} solves, {total_s:.0} s",
        e0.worst_db(),
        e_final.worst_db(),
        verify.len(),
        model.n_solves.get()
    );

    // ── Geometry JSON (consumed by the Meep work, #1088, and the figure script) ──
    let geometry_path = cli.geometry.clone().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../benchmarks/patch_antenna_conformal/geometry/{level}_{}.json",
            cli.label
        ))
    });
    let geometry_sha = {
        let moved = model.moved_mesh(&x_final);
        let top_tris = substrate_top_triangles(&model.base, &nodes.substrate_top);
        let mut local: BTreeMap<u32, usize> = BTreeMap::new();
        let mut ids: Vec<u32> = Vec::new();
        for tri in &top_tris {
            for &n in tri {
                local.entry(n).or_insert_with(|| {
                    ids.push(n);
                    ids.len() - 1
                });
            }
        }
        let is_patch: Vec<bool> = {
            let mut m = vec![false; n_nodes];
            for &n in &nodes.patch {
                m[n as usize] = true;
            }
            ids.iter().map(|&n| m[n as usize]).collect()
        };
        let patch_local: Vec<[usize; 3]> = patch_tris
            .iter()
            .map(|t| [local[&t[0]], local[&t[1]], local[&t[2]]])
            .collect();
        let top_local: Vec<[usize; 3]> = top_tris
            .iter()
            .map(|t| [local[&t[0]], local[&t[1]], local[&t[2]]])
            .collect();
        // Patch outline: boundary edges of the patch triangulation, chained
        // into a closed loop of local node ids.
        let outline: Vec<usize> = {
            let mut count: BTreeMap<(usize, usize), usize> = BTreeMap::new();
            for t in &patch_local {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    *count.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            let mut adj: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for (&(a, b), &c) in &count {
                if c == 1 {
                    adj.entry(a).or_default().push(b);
                    adj.entry(b).or_default().push(a);
                }
            }
            let mut loop_ids = Vec::new();
            if let Some((&start, _)) = adj.iter().next() {
                let mut prev = usize::MAX;
                let mut cur = start;
                loop {
                    loop_ids.push(cur);
                    let nb = &adj[&cur];
                    let nxt = if nb[0] != prev { nb[0] } else { nb[1] };
                    prev = cur;
                    cur = nxt;
                    if cur == start || loop_ids.len() > adj.len() {
                        break;
                    }
                }
            }
            loop_ids
        };
        // Compact JSON: scalars and strings pretty, one array element per
        // line for node/triangle arrays would multiply the size, so every
        // array is written compact; coordinates are rounded to 1e-6 mm.
        let r6 = |v: f64| (v * 1e6).round() / 1e6;
        let coords = |p: [f64; 3]| format!("[{},{},{}]", r6(p[0]), r6(p[1]), r6(p[2]));
        let mut j = String::with_capacity(1 << 20);
        j.push_str("{\n");
        let mut field = |key: &str, value: String| {
            let _ = writeln!(j, "  \"{key}\": {value},");
        };
        let js = |s: &str| serde_json::Value::String(s.to_string()).to_string();
        field(
            "description",
            js("Optimized curved conformal 2.4 GHz patch (issue #1087): base and moved \
                coordinates of the substrate-top surface nodes (the patch nodes among them, \
                is_patch) with the patch and substrate-top triangles, bent frame. The ground \
                plane is physically unchanged (its mesh nodes slide tangentially under outline \
                DOFs only) and is not dumped. Consumed by the Meep matched-geometry work \
                (#1088) and benchmarks/patch_antenna_conformal/scripts/plot_patch_displacement.py."),
        );
        field("units", js("mm"));
        field(
            "frame",
            js("bent fixture coordinates (x, y, z); bend axis parallel to y through (x = 0, z = \
                z_axis_mm); chart s = R*atan2(x, z - z_axis), y"),
        );
        field("fixture", js(&stem));
        field("level", js(&level));
        field("fixture_sha256", js(&fixture_sha));
        field("design_label", js(&cli.label));
        field("generated_at_commit", js(&geode_util::repo::current_commit()));
        field("bend_radius_mm", format!("{bend_radius}"));
        field("z_axis_mm", format!("{}", chart.z_axis));
        field("substrate_h_mm", format!("{h}"));
        field("patch_w_mm", format!("{patch_w}"));
        field("patch_l_mm", format!("{patch_l}"));
        field("sub_pad_mm", format!("{sub_pad}"));
        field("probe_w_mm", format!("{probe_w}"));
        field("probe_inset_mm", format!("{probe_inset}"));
        field("feed_s_y_mm", format!("[{},{}]", feed.0, feed.1));
        field("upml_sigma_0", format!("{sigma0}"));
        field("radial_basis", js(&radial_spec));
        field("outline_basis", js(&outline_spec));
        field("n_radial_dofs", format!("{n_radial}"));
        field("n_outline_dofs", format!("{n_outline}"));
        field(
            "design_vector",
            format!(
                "[{}]",
                x_final
                    .iter()
                    .map(|v| format!("{v:.9e}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
        field("n_nodes", format!("{}", ids.len()));
        field(
            "node_ids",
            format!(
                "[{}]",
                ids.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",")
            ),
        );
        field(
            "is_patch",
            format!(
                "[{}]",
                is_patch
                    .iter()
                    .map(|b| if *b { "1" } else { "0" })
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
        field(
            "base",
            format!(
                "[{}]",
                ids.iter()
                    .map(|&n| coords(model.base.mesh.nodes[n as usize]))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
        field(
            "moved",
            format!(
                "[{}]",
                ids.iter()
                    .map(|&n| coords(moved.nodes[n as usize]))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
        let tris = |v: &[[usize; 3]]| {
            format!(
                "[{}]",
                v.iter()
                    .map(|t| format!("[{},{},{}]", t[0], t[1], t[2]))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        field("patch_triangles", tris(&patch_local));
        field("substrate_top_triangles", tris(&top_local));
        field(
            "patch_outline_loop",
            format!(
                "[{}]",
                outline
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
        let _ = write!(
            j,
            "  \"s11_verify\": [{}]\n}}\n",
            verify
                .iter()
                .map(|(f, s, _)| format!(
                    "{{\"f_ghz\":{f},\"s11_re\":{:.9e},\"s11_im\":{:.9e},\"s11_db\":{:.6}}}",
                    s.re,
                    s.im,
                    20.0 * s.norm().log10()
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        // Must parse as JSON.
        let _: serde_json::Value = serde_json::from_str(&j).expect("geometry JSON is valid");
        if let Some(p) = geometry_path.parent() {
            fs::create_dir_all(p).expect("geometry dir");
        }
        fs::write(&geometry_path, &j).expect("write geometry json");
        sha256_hex(j.as_bytes())
    };

    // ── Results TOML ──
    let mut t = String::with_capacity(64 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example \\\n");
    t.push_str("#   patch_conformal_nested_dof -- <flags>` (see [meta].command).\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_conformal_nested_dof.rs`.\n\n");
    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Nested-DOF shape optimization of the curved conformal 2.4 GHz patch on \
         a mesh-independent smooth basis (issue #1087, conformal v5 E4): soft-max of |S11|^2 over \
         the ISM band driven by the composed open-radiator shape adjoint and an L-BFGS descent \
         with a per-tet non-inversion guard; gradient FD-validated at x = 0 and at the final \
         design; final design verified by fresh forward sweeps.\"\n",
    );
    let _ = writeln!(t, "label = {}", toml_str(&cli.label));
    let _ = writeln!(
        t,
        "command = {}",
        toml_str(&format!(
            "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
             <dir>/{stem}.msh --radial {} --outline {} --filter-radius {} --n-band {} --beta {} \
             --max-steps {} --verify-n {} --label {}",
            cli.radial.map_or("0".to_string(), |(a, b)| format!("{a}x{b}")),
            cli.outline,
            cli.filter_radius,
            cli.n_band,
            cli.beta,
            cli.max_steps,
            cli.verify_n,
            cli.label
        ))
    );
    let _ = writeln!(t, "fixture = {}", toml_str(&stem));
    let _ = writeln!(t, "level = {}", toml_str(&level));
    let _ = writeln!(t, "fixture_sha256 = {}", toml_str(&fixture_sha));
    let _ = writeln!(
        t,
        "fixture_yaml = {}",
        toml_str(&format!("reference/gmsh/{stem}.yaml"))
    );
    let _ = writeln!(
        t,
        "generated_at_commit = \"{}\"",
        geode_util::repo::current_commit()
    );
    BackendInfo::of::<B>(&device()).push_meta(&mut t);
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
    {
        let pts: Vec<String> = band_f.iter().map(|f| format!("{f:.6}")).collect();
        let _ = writeln!(t, "band_points_ghz = [{}]", pts.join(", "));
    }
    let _ = writeln!(t, "n_band_points = {}", band_f.len());
    let _ = writeln!(t, "softmax_beta = {}", cli.beta);
    t.push_str(
        "objective = \"G(x) = (1/beta) ln sum_f exp(beta |S11(f; x)|^2) over the band points \
         (soft-max of |S11|^2); S11 = (Z - Z0)/(Z + Z0), Z = V/I, I = (2 V_inc - V)/R, Z0 = R\"\n",
    );
    let _ = writeln!(t, "target_db = {TARGET_DB}");
    let _ = writeln!(t, "port_resistance_ohm = {R_PORT_OHM}");
    let _ = writeln!(t, "port_resistance_nat = {:.9e}", R_PORT_OHM / ETA_0);
    let _ = writeln!(t, "eta_0_ohm = {ETA_0:.9e}");
    let _ = writeln!(t, "eps_r = {}", FR4_MATERIALS.eps_r_substrate);
    let _ = writeln!(t, "tan_delta = {}", FR4_MATERIALS.tan_delta_substrate);
    let _ = writeln!(t, "bend_radius_mm = {bend_radius}");
    let _ = writeln!(t, "bend_axis_z_mm = {}", chart.z_axis);
    let _ = writeln!(t, "patch_w_mm = {patch_w}");
    let _ = writeln!(t, "patch_l_mm = {patch_l}");
    let _ = writeln!(t, "substrate_h_mm = {h}");
    let _ = writeln!(t, "sub_pad_mm = {sub_pad}");
    let _ = writeln!(t, "probe_w_mm = {probe_w}");
    let _ = writeln!(t, "probe_inset_mm = {probe_inset}");
    let _ = writeln!(t, "feed_s_mm = {}", feed.0);
    let _ = writeln!(t, "feed_y_mm = {}", feed.1);
    let _ = writeln!(t, "upml_thick_mm = {pml_thick}");
    let _ = writeln!(t, "upml_sigma_0 = {sigma0}");
    let _ = writeln!(t, "upml_sigma_0_fixture = {sigma0_fixture}");
    t.push_str(
        "upml_profile = \"s = 1 - j sigma_0 (d/w)^2 / omega per axis (box UPML), omega in rad/mm; \
         sigma_0 dominates S11 (#1085), so the value above is the one this run used\"\n",
    );
    let _ = writeln!(t, "n_edges = {n_edges}");
    let _ = writeln!(t, "n_nodes = {n_nodes}");
    let _ = writeln!(t, "n_tets = {n_tets}");
    let _ = writeln!(t, "n_patch_nodes = {}", nodes.patch.len());
    let _ = writeln!(t, "n_substrate_top_nodes = {}", nodes.substrate_top.len());
    let _ = writeln!(t, "n_slab_nodes = {}", nodes.slab.len());
    t.push('\n');

    t.push_str("[basis]\n");
    t.push_str(
        "chart = \"s = R atan2(x, z - z_axis) (mid-plane arc length = flat x), y; r_hat outward \
         radial, s_hat in-surface tangent (geode_core::shape::conformal_basis::CylinderChart)\"\n",
    );
    let _ = writeln!(t, "radial = {}", toml_str(&radial_spec));
    let _ = writeln!(t, "outline = {}", toml_str(&outline_spec));
    let _ = writeln!(t, "n_radial_dofs = {n_radial}");
    let _ = writeln!(t, "n_outline_dofs = {n_outline}");
    let _ = writeln!(t, "n_dofs = {n_dofs}");
    let _ = writeln!(t, "radial_scale_mm_per_unit = {radial_scale}");
    let _ = writeln!(t, "outline_scale_mm_per_unit = {OUTLINE_SCALE_MM}");
    let _ = writeln!(t, "filter_radius_mm = {}", cli.filter_radius);
    let _ = writeln!(t, "spline_degree = {SPLINE_DEGREE}");
    t.push_str(
        "construction = \"FreeformBoundaryMorph::from_columns; each column is the harmonic \
         (P1-Laplace) extension of the basis function sampled on the prescribed surface nodes \
         (radial: substrate-top nodes; outline: every substrate node), equal by linearity to \
         sum_p B_q(s_p, y_p) D_p over node-local hat columns D_p; UPML shell, port faces and \
         outer boundary pinned; ground pinned for radial columns\"\n",
    );
    let _ = writeln!(t, "morph_build_s = {morph_s:.3}");
    t.push('\n');

    t.push_str("[fd_validation]\n");
    let _ = writeln!(t, "fd_step_h = {FD_H:.1e}");
    let _ = writeln!(t, "tolerance = {FD_TOL:.1e}");
    let _ = writeln!(t, "n_checks = {}", fd_checks.len());
    let _ = writeln!(
        t,
        "max_rel_err = {:.6e}",
        fd_checks.iter().map(|c| c.rel).fold(0.0_f64, f64::max)
    );
    t.push('\n');
    for c in &fd_checks {
        t.push_str("[[fd_validation.check]]\n");
        let _ = writeln!(t, "direction = {}", toml_str(&c.label));
        let _ = writeln!(t, "at = {}", toml_str(&c.at));
        let _ = writeln!(t, "analytic_dir_deriv = {:.9e}", c.analytic);
        let _ = writeln!(t, "central_fd_dir_deriv = {:.9e}", c.fd);
        let _ = writeln!(t, "rel_err = {:.6e}", c.rel);
        t.push('\n');
    }

    t.push_str("[optimization]\n");
    t.push_str(
        "method = \"L-BFGS (memory 8) with Armijo backtracking, per-step max-norm cap 1.0 design \
         units, per-tet signed-volume-ratio guard; gradient = frequency-softmax-weighted composed \
         adjoint nodal gradients contracted through the morph columns\"\n",
    );
    let _ = writeln!(t, "max_steps = {}", cli.max_steps);
    let _ = writeln!(t, "n_steps = {}", steps.len() - 1);
    let _ = writeln!(t, "terminal_condition = {}", toml_str(terminal));
    let _ = writeln!(t, "total_backtracks = {total_backtracks}");
    let _ = writeln!(t, "min_vol_ratio_budget = {MIN_VOL_RATIO}");
    let _ = writeln!(t, "worst_vol_ratio = {worst_vol:.6e}");
    let _ = writeln!(t, "worst_vol_ratio_final = {worst_ratio_final:.6e}");
    let _ = writeln!(t, "worst_tet_final = {worst_tet}");
    let _ = writeln!(t, "grad_norm_initial = {gnorm0:.9e}");
    let _ = writeln!(t, "grad_norm_final = {gnorm_final:.9e}");
    let _ = writeln!(t, "objective_initial = {:.9e}", e0.objective);
    let _ = writeln!(t, "objective_final = {:.9e}", e_final.objective);
    let _ = writeln!(t, "worst_s11_db_initial = {:.6e}", e0.worst_db());
    let _ = writeln!(t, "worst_s11_db_final = {:.6e}", e_final.worst_db());
    let _ = writeln!(t, "best_s11_db_initial = {:.6e}", e0.best_db());
    let _ = writeln!(t, "best_s11_db_final = {:.6e}", e_final.best_db());
    let _ = writeln!(t, "verify_n_points = {}", verify.len());
    let _ = writeln!(t, "verify_worst_s11_db = {verify_worst:.6e}");
    let _ = writeln!(t, "verify_best_s11_db = {verify_best:.6e}");
    let _ = writeln!(t, "reached_target = {reached}");
    let _ = writeln!(t, "dip_f_ghz_final = {dip_f:.4}");
    let _ = writeln!(t, "dip_db_final = {dip_db:.4}");
    let _ = writeln!(t, "max_nodal_disp_mm = {max_disp:.6e}");
    let _ = writeln!(t, "mean_patch_radial_lift_mm = {mean_patch_radial:.6e}");
    let _ = writeln!(t, "edge_mean_outward_mm = {{ y_plus = {yp:.6e}, y_minus = {ym:.6e}, s_plus = {sp:.6e}, s_minus = {sm:.6e} }}");
    let _ = writeln!(t, "n_driven_solves = {}", model.n_solves.get());
    let _ = writeln!(t, "first_gradient_eval_s = {eval_s:.3}");
    let _ = writeln!(t, "first_gradient_eval_peak_rss_gib = {rss0:.3}");
    let _ = writeln!(t, "total_wall_s = {total_s:.1}");
    let outcome = if reached {
        "band_match_-10dB"
    } else {
        "honest_negative"
    };
    let _ = writeln!(t, "outcome = {}", toml_str(outcome));
    t.push('\n');

    for (k, s) in e0.s11.iter().enumerate() {
        t.push_str("[[s11_initial.point]]\n");
        let _ = writeln!(t, "f_ghz = {:.6}", band_f[k]);
        let _ = writeln!(t, "s11_re = {:.9e}", s.re);
        let _ = writeln!(t, "s11_im = {:.9e}", s.im);
        let _ = writeln!(t, "s11_mag = {:.9e}", s.norm());
        let _ = writeln!(t, "s11_db = {:.6}", 20.0 * s.norm().log10());
        t.push('\n');
    }
    for (k, s) in e_final.s11.iter().enumerate() {
        t.push_str("[[s11_band.point]]\n");
        let _ = writeln!(t, "f_ghz = {:.6}", band_f[k]);
        let _ = writeln!(t, "s11_re = {:.9e}", s.re);
        let _ = writeln!(t, "s11_im = {:.9e}", s.im);
        let _ = writeln!(t, "s11_mag = {:.9e}", s.norm());
        let _ = writeln!(t, "s11_db = {:.6}", 20.0 * s.norm().log10());
        let _ = writeln!(t, "objective = {:.9e}", e_final.g_f[k]);
        t.push('\n');
    }
    for (f, s, res) in &verify {
        t.push_str("[[s11_verify.point]]\n");
        let _ = writeln!(t, "f_ghz = {f:.6}");
        let _ = writeln!(t, "s11_re = {:.9e}", s.re);
        let _ = writeln!(t, "s11_im = {:.9e}", s.im);
        let _ = writeln!(t, "s11_mag = {:.9e}", s.norm());
        let _ = writeln!(t, "s11_db = {:.6}", 20.0 * s.norm().log10());
        let _ = writeln!(t, "solve_residual_rel = {res:.3e}");
        t.push('\n');
    }
    for (f, s, _) in &wide {
        t.push_str("[[s11_wide_final.point]]\n");
        let _ = writeln!(t, "f_ghz = {f:.6}");
        let _ = writeln!(t, "s11_mag = {:.9e}", s.norm());
        let _ = writeln!(t, "s11_db = {:.6}", 20.0 * s.norm().log10());
        t.push('\n');
    }
    for s in &steps {
        t.push_str("[[trajectory.step]]\n");
        let _ = writeln!(t, "iter = {}", s.iter);
        let _ = writeln!(t, "objective = {:.9e}", s.objective);
        let _ = writeln!(t, "worst_s11_db = {:.6e}", s.worst_db);
        let _ = writeln!(t, "grad_norm = {:.9e}", s.grad_norm);
        let _ = writeln!(t, "min_vol_ratio = {:.6e}", s.min_vol_ratio);
        let _ = writeln!(t, "step_len = {:.6e}", s.step_len);
        let _ = writeln!(t, "backtracks = {}", s.backtracks);
        let _ = writeln!(t, "wall_s = {:.1}", s.wall_s);
        t.push('\n');
    }
    t.push_str("[shape]\n");
    let _ = writeln!(t, "n_dofs = {n_dofs}");
    let _ = writeln!(t, "n_radial_dofs = {n_radial}");
    let _ = writeln!(t, "n_outline_dofs = {n_outline}");
    {
        let xs: Vec<String> = x_final.iter().map(|v| format!("{v:.9e}")).collect();
        let _ = writeln!(t, "design_vector = [{}]", xs.join(", "));
    }
    let _ = writeln!(
        t,
        "geometry_json = {}",
        toml_str(&format!(
            "benchmarks/patch_antenna_conformal/geometry/{level}_{}.json",
            cli.label
        ))
    );
    let _ = writeln!(t, "geometry_json_sha256 = {}", toml_str(&geometry_sha));
    t.push('\n');

    let out = cli.out.clone().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../benchmarks/patch_antenna_conformal/nested_dof/{level}_{}.toml",
            cli.label
        ))
    });
    if let Some(p) = out.parent() {
        fs::create_dir_all(p).expect("out dir");
    }
    fs::write(&out, &t).expect("write results toml");
    println!(
        "patch_conformal_nested_dof [{}]: results {} ; geometry {}",
        cli.label,
        out.display(),
        geometry_path.display()
    );
}
