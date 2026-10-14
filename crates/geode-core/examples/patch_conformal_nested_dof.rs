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
//! `--fd-only` (validate the gradient, skip the descent), `--design TOML`,
//! `--verify-only`, `--p2`, `--wide-step GHZ` (default 0.025).
//!
//! # Carrying a design across meshes (issue #1103, gate G2)
//!
//! The basis is a function of the chart `(s, y)` alone, so a coefficient
//! vector defines the same surface on every level. `--design TOML` reads
//! `[shape].design_vector` from a results TOML of this example (the basis
//! description must match this run's):
//!
//! * without `--verify-only` it is the **warm start** of the descent
//!   (halved until it clears the volume guard on this mesh; the scale is
//!   recorded in `[start]`);
//! * with `--verify-only` the FD gate and the descent are skipped and the
//!   design is **verified** by fresh forward sweeps on this fixture: the
//!   `--verify-n` in-band points and the wide sweep at `--wide-step`
//!   (`0` skips it), at P1 or, with `--p2`, with second-order Nédélec
//!   elements (the #1085 `dip --p2` path). Without `--design` the initial
//!   patch (`x = 0`) is verified. The output goes to
//!   `nested_dof/verify/<design>_on_<level>[_p2].toml`.
//!
//! Every run records where the distortion guard binds (`[guard]`: each
//! rejected line-search trial's worst tet, located in the chart relative to
//! the feed and the patch corners) and the feed-crater / substrate-thickness
//! metrics of the design (`[crater]`).

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

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::constants::ETA_0_OHM as ETA_0;
use geode_core::driven::extraction::{s11_sq_and_dg_dv, s11_sq_objective};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::driven::shape::{
    driven_shape_gradient_matched_upml_moving_port_s11, driven_shape_gradient_matched_upml_ports,
};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource,
    driven_solve_with_ports,
};
use geode_core::elements::ElementOrder;
use geode_core::driven::scattering::{e_norm_sq_integral, flux_power_box};
use geode_core::mesh::patch::{FR4_MATERIALS, ISM_2G4_BAND_GHZ, PHYS_SUBSTRATE, bend_axis_z};
use geode_core::postproc::ntff::{broadside_directivity, directivity, ntff_far_field};
use geode_core::mesh::{PatchFixture, pec_interior_mask_from_triangles, read_patch_fixture_path};
use geode_core::shape::conformal_basis::{
    AirMotion, ChartMorph, ConformalPatchNodes, CornerBasis, CylinderChart, FeedBasis, FeedCutoff,
    OutlineBasis, PatchRect, RadialBasis, filter_columns, helmholtz_lattice_filter,
    outline_columns, radial_columns, substrate_top_triangles,
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
/// Wide resonance-location sweep range (GHz).
const WIDE_GHZ: [f64; 2] = [2.20, 2.70];
/// Copper mode (issue #1120): feed rigid-core radius and support radius, as
/// multiples of the probe width (1.2 mm and 6.4 mm for the 1.6 mm probe; the
/// +y edge is 7 mm from the feed, so the outline never moves). The support is
/// capped at the feed's distance to the patch outline ([`FeedBasis::capped`])
/// unless `--feed-support-legacy`: on the re-based fixtures (probe 2.1–4.5 mm,
/// 4.9–5.7 mm from the +y edge) the uncapped `4 · probe_w` reaches the edge.
const FEED_CORE_FRAC: f64 = 0.75;
const FEED_SUPPORT_FRAC: f64 = 4.0;
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
    /// A results TOML whose `[shape].design_vector` is the warm start (descent
    /// mode) or the design to verify (`--verify-only`).
    design: Option<PathBuf>,
    /// Skip the FD gate and the descent: fresh forward sweeps of `--design`
    /// (or of the initial patch, `x = 0`) on this fixture (issue #1103).
    verify_only: bool,
    /// Verification sweeps with second-order Nédélec elements (`--verify-only` only).
    p2: bool,
    /// Wide-sweep spacing over 2.20–2.70 GHz (GHz); `0` skips the wide sweep.
    wide_step: f64,
    /// Copper-only design on the fixed 1.6 mm board (issue #1120): the exact
    /// on-cylinder [`ChartMorph`], no radial DOFs.
    copper: bool,
    /// Corner-bump support radius (mm; `0` = no corner DOFs). Copper mode only.
    corner_radius: f64,
    /// Feed inset depth + probe width DOFs (moving-feed adjoint). Copper mode only.
    feed: bool,
    /// Objective band shift (GHz): the band points are `f − shift` (a frequency
    /// space-mapping correction for the resonance drift to higher fidelity).
    band_shift: f64,
    /// `--verify-only` dense sweep `(lo, hi, n)` (GHz, inclusive grid).
    dense: Option<(f64, f64, usize)>,
    /// `--verify-only` shard `(k, n)` of the dense grid: points `i % n == k`.
    shard: Option<(usize, usize)>,
    /// Carry `--design` onto a fixture regenerated at another probe inset /
    /// width (copper mode): the feed coefficients are shifted by the change of
    /// the fixture's `probe_inset` / `probe_w`, so the probe position and width
    /// are carried exactly. The outline and corners are carried exactly only
    /// when the feed support stays inside the outline on both fixtures (the
    /// default capped support); a feed support reaching the +y edge drags it,
    /// and that drag is not carried (refused unless `--feed-support-legacy`).
    rebase_feed: bool,
    /// Copper mode: the uncapped `4 · probe_w` feed support that produced the
    /// committed #1120 artifacts (reproduces them on the re-based fixtures,
    /// where it reaches the +y edge).
    feed_support_legacy: bool,
    /// Copper mode: move the air on the cylinders too ([`AirMotion::OnCylinder`]).
    air_on_cylinder: bool,
    /// Copper mode (issue #1126, X4): the 4-parameter rectangle baseline. The
    /// outline DOFs collapse to two, `ΔW` and `ΔL` (mm, total width / length
    /// change): each moves both opposite edges outward by half its value,
    /// the sum of that edge's B-splines (a partition of unity, so the edges
    /// translate uniformly). With `--feed` the design is (W, L, inset, width).
    rect4: bool,
    /// Copper mode with `--feed` (issue #1126, X5): pin the probe width DOF at
    /// 0, so the port keeps the fixture's `probe_w` (a realizable 1.27–1.6 mm
    /// feed on the rbG / rbH fixtures). The width coefficient is zeroed in the
    /// start and in every gradient.
    freeze_feed_width: bool,
    /// `--verify-only` at P1 (issue #1126, X3): power balance and broadside
    /// gain at these frequencies (GHz): port power, box Poynting flux,
    /// substrate dielectric loss, radiation efficiency, NTFF directivity and
    /// realized gain.
    radiation: Vec<f64>,
    /// `--verify-only` (issue #1126, X1): write the moved mesh (with the
    /// fixture's physical tags) as MSH 2.2 ASCII for the Palace reference.
    write_mesh: Option<PathBuf>,
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
        design: None,
        verify_only: false,
        p2: false,
        wide_step: 0.025,
        copper: false,
        corner_radius: 0.0,
        feed: false,
        band_shift: 0.0,
        dense: None,
        shard: None,
        rebase_feed: false,
        feed_support_legacy: false,
        air_on_cylinder: false,
        rect4: false,
        freeze_feed_width: false,
        radiation: Vec::new(),
        write_mesh: None,
    };
    let mut radial_given = false;
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
                radial_given = true;
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
            "--design" => cli.design = Some(PathBuf::from(next(&mut i))),
            "--verify-only" => cli.verify_only = true,
            "--p2" => cli.p2 = true,
            "--wide-step" => cli.wide_step = next(&mut i).parse().expect("--wide-step GHz"),
            "--copper" => cli.copper = true,
            "--rebase-feed" => cli.rebase_feed = true,
            "--feed-support-legacy" => cli.feed_support_legacy = true,
            "--air-on-cylinder" => cli.air_on_cylinder = true,
            "--rect4" => cli.rect4 = true,
            "--freeze-feed-width" => cli.freeze_feed_width = true,
            "--radiation" => {
                cli.radiation = next(&mut i)
                    .split(',')
                    .map(|v| v.trim().parse().expect("--radiation f1,f2,... (GHz)"))
                    .collect();
            }
            "--write-mesh" => cli.write_mesh = Some(PathBuf::from(next(&mut i))),
            "--corners" => cli.corner_radius = next(&mut i).parse().expect("--corners MM"),
            "--feed" => cli.feed = true,
            "--band-shift" => cli.band_shift = next(&mut i).parse().expect("--band-shift GHz"),
            "--dense" => {
                let v = next(&mut i);
                let p: Vec<&str> = v.split(':').collect();
                assert_eq!(p.len(), 3, "--dense LO:HI:N");
                cli.dense = Some((
                    p[0].parse().expect("LO"),
                    p[1].parse().expect("HI"),
                    p[2].parse().expect("N"),
                ));
            }
            "--shard" => {
                let v = next(&mut i);
                let (a, b) = v.split_once('/').expect("--shard K/N");
                cli.shard = Some((a.parse().expect("K"), b.parse().expect("N")));
            }
            other => panic!("unknown flag {other}"),
        }
        i += 1;
    }
    assert!(cli.n_band >= 2, "--n-band must be at least 2");
    assert!(
        !cli.p2 || cli.verify_only,
        "--p2 is a verification order: give it with --verify-only"
    );
    assert!(
        cli.wide_step > 0.0 || (cli.verify_only && cli.wide_step == 0.0),
        "--wide-step must be > 0 (0, no wide sweep, only with --verify-only)"
    );
    if cli.copper {
        // Fixed 1.6 mm board (issue #1120): the radial (gap) DOFs are frozen.
        assert!(
            !radial_given || cli.radial.is_none(),
            "--copper freezes the substrate thickness: --radial must be 0 (or omitted)"
        );
        cli.radial = None;
        assert!(
            cli.outline > 0 || cli.corner_radius > 0.0 || cli.feed,
            "no copper freedom: give --outline N, --corners MM and/or --feed"
        );
        assert!(
            cli.filter_radius == 0.0,
            "--filter-radius acts on radial DOFs only"
        );
    } else {
        assert!(
            cli.corner_radius == 0.0 && !cli.feed,
            "--corners / --feed need --copper"
        );
    }
    assert!(
        cli.dense.is_none() || cli.verify_only,
        "--dense is a verification sweep: give it with --verify-only"
    );
    assert!(
        !cli.rect4 || (cli.copper && cli.outline > 0 && cli.corner_radius == 0.0),
        "--rect4 needs --copper --outline N (the splines it sums) and no --corners"
    );
    assert!(
        !cli.freeze_feed_width || (cli.copper && cli.feed),
        "--freeze-feed-width needs --copper --feed"
    );
    assert!(
        cli.radiation.is_empty() || (cli.verify_only && !cli.p2),
        "--radiation is a P1 verification: give it with --verify-only, without --p2"
    );
    assert!(
        cli.write_mesh.is_none() || cli.verify_only,
        "--write-mesh needs --verify-only"
    );
    assert!(
        !cli.rebase_feed || (cli.copper && cli.feed && cli.design.is_some()),
        "--rebase-feed needs --copper --feed and --design"
    );
    if let Some((k, n)) = cli.shard {
        assert!(
            n >= 1 && k < n && cli.dense.is_some(),
            "--shard K/N needs --dense, K < N"
        );
    }
    assert!(
        cli.radial.is_some() || cli.outline > 0 || cli.copper,
        "no design freedom: give --radial NSxNY and/or --outline N"
    );
    if cli.label.is_empty() && cli.copper {
        cli.label = format!(
            "copper_{}",
            if cli.rect4 { 2 } else { 4 * cli.outline }
                + if cli.corner_radius > 0.0 { 4 } else { 0 }
                + if cli.feed { 2 } else { 0 }
        );
    }
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
    (
        out,
        peak_kib.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
    )
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
            .map(|i| {
                if mask(i) {
                    2.0 * self.next_f64() - 1.0
                } else {
                    0.0
                }
            })
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

/// The design morph: the #1087 linear column morph (radial and / or the
/// straight-line outline columns) or the #1120 exact on-cylinder chart morph.
enum DesignMorph {
    Linear(FreeformBoundaryMorph),
    Chart(ChartMorph),
}

impl DesignMorph {
    fn n_dofs(&self) -> usize {
        match self {
            DesignMorph::Linear(m) => m.n_dofs(),
            DesignMorph::Chart(m) => m.n_dofs(),
        }
    }

    fn apply(&self, mesh: &geode_core::mesh::TetMesh, x: &[f64]) -> geode_core::mesh::TetMesh {
        match self {
            DesignMorph::Linear(m) => m.apply(mesh, x),
            DesignMorph::Chart(m) => m.apply(mesh, x),
        }
    }

    /// Nodal displacement `X(x) − X⁰`.
    fn displacement(&self, mesh: &geode_core::mesh::TetMesh, x: &[f64]) -> Vec<[f64; 3]> {
        match self {
            DesignMorph::Linear(m) => m.combined_velocity(x),
            DesignMorph::Chart(m) => m.displacement(mesh, x),
        }
    }

    fn design_gradient(&self, x: &[f64], grad_node: &[[f64; 3]]) -> Vec<f64> {
        match self {
            DesignMorph::Linear(m) => m.design_gradient(grad_node),
            DesignMorph::Chart(m) => m.design_gradient(x, grad_node),
        }
    }
}

struct Model {
    base: PatchFixture,
    morph: DesignMorph,
    /// The design moves the port faces (copper mode with feed DOFs): the port is
    /// rebuilt on every moved mesh and the moving-feed adjoint is used.
    moving_port: bool,
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
    /// Design DOFs pinned at 0 (`--freeze-feed-width`): their gradient entry
    /// is zeroed, so the descent never moves them.
    frozen: Vec<usize>,
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

    /// The lumped port, its flux covector and `1/w` on a (moved) mesh: the
    /// base port unless the design moves the feed, in which case the port
    /// width / gap length and flux moments are those of `mesh`.
    fn port_on(
        &self,
        mesh: &geode_core::mesh::TetMesh,
    ) -> (geode_core::mesh::patch::PatchPort, Vec<f64>, f64) {
        if self.moving_port {
            let port = self.base.port_on_mesh(mesh);
            let flux = assemble_port_flux(mesh, &port.faces, port.e_hat, &mesh.edges());
            let inv_width = 1.0 / port.width;
            (port, flux, inv_width)
        } else {
            (self.port.clone(), self.flux.clone(), self.inv_width)
        }
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
        let (port, flux, inv_width) = self.port_on(&mesh);
        let lp = port.lumped_port(self.r_nat, self.v_inc);
        let n_nodes = mesh.n_nodes();
        let mut g_f = Vec::with_capacity(self.band.len());
        let mut s11 = Vec::with_capacity(self.band.len());
        let mut grads: Vec<Vec<[f64; 3]>> = Vec::new();
        let mut worst_res = 0.0_f64;
        for (_f, omega, eps, nu) in &self.band {
            self.n_solves.set(self.n_solves.get() + 1);
            if grad {
                let sg = if self.moving_port {
                    driven_shape_gradient_matched_upml_moving_port_s11::<B>(
                        &mesh,
                        eps,
                        nu,
                        &bcs,
                        &lp,
                        *omega,
                        &self.source,
                        self.r_nat,
                        &device(),
                    )
                    .expect("moving-feed composed shape gradient")
                } else {
                    driven_shape_gradient_matched_upml_ports::<B, _>(
                        &mesh,
                        eps,
                        nu,
                        &bcs,
                        std::slice::from_ref(&lp),
                        *omega,
                        &self.source,
                        s11_sq_objective(
                            flux.clone(),
                            inv_width,
                            self.v_inc,
                            self.r_nat,
                            self.r_nat,
                        ),
                        &device(),
                    )
                    .expect("composed shape gradient")
                };
                assert_eq!(sg.n_factorizations, 1, "adjoint must reuse the forward LU");
                worst_res = worst_res.max(sg.residual_rel);
                let v = port_voltage(&flux, inv_width, &sg.e_edges);
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
                let v = port_voltage(&flux, inv_width, &sol.e_edges);
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
            let mut g = self.morph.design_gradient(x, &acc);
            for &k in &self.frozen {
                g[k] = 0.0;
            }
            g
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
        let (port, flux, inv_width) = self.port_on(&mesh);
        let lp = port.lumped_port(self.r_nat, self.v_inc);
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
                let v = port_voltage(&flux, inv_width, &sol.e_edges);
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

    /// [`Self::sweep`] with second-order Nédélec elements on the moved mesh
    /// (the `patch_conformal_convergence dip --p2` path of #1085: the
    /// `HcurlSpace` operator, port voltage from the operator's own port
    /// functional; at `ElementOrder::P1` it is bit-identical to the P1 path).
    fn sweep_p2(&self, x: &[f64], freqs: &[f64]) -> Vec<(f64, c64, f64)> {
        let mesh = self.moved_mesh(x);
        let space = HcurlSpace::build(&mesh, ElementOrder::P2);
        let patch = self.base.patch_triangles();
        let ground = self.base.ground_triangles();
        let outer = self.base.outer_boundary_triangles();
        let mask = space
            .pec_interior_mask(
                &mesh,
                &[patch.as_slice(), ground.as_slice(), outer.as_slice()],
            )
            .expect("P2 PEC mask");
        eprintln!(
            "    P2 space: {} DOFs ({} interior)",
            mask.len(),
            mask.iter().filter(|&&m| m).count()
        );
        let (port, _, _) = self.port_on(&mesh);
        let lp = port.lumped_port(self.r_nat, self.v_inc);
        freqs
            .iter()
            .map(|&f| {
                let omega = ghz_to_omega_mm(f);
                let t0 = Instant::now();
                let (eps, nu) = self.base.matched_upml_materials(
                    &FR4_MATERIALS,
                    self.air_lo,
                    self.air_hi,
                    self.pml_thick,
                    self.sigma0,
                    omega,
                );
                self.n_solves.set(self.n_solves.get() + 1);
                let op = DrivenOperator::assemble_with_space::<B>(
                    &space,
                    &mesh,
                    DrivenMaterials::MatchedUpml {
                        epsilon_tensor: &eps,
                        nu_tensor: &nu,
                    },
                    None,
                    &DrivenBcs {
                        pec_interior_mask: &mask,
                    },
                    std::slice::from_ref(&lp),
                    &[],
                    DrivenSource::Constant(&self.source),
                    &device(),
                )
                .unwrap_or_else(|e| panic!("P2 assemble at {f} GHz: {e}"));
                let sol = op
                    .solve_at(omega)
                    .unwrap_or_else(|e| panic!("P2 solve at {f} GHz: {e}"));
                let v = op.port_voltage(0, &sol.e_edges);
                let s = self.s11_of_v(v);
                assert!(s.norm() <= 1.0 + 1e-6, "passivity at {f} GHz (P2)");
                let i = (self.v_inc * 2.0 - v) * (1.0 / self.r_nat);
                let z = v / i * ETA_0;
                eprintln!(
                    "    sweep(P2) {f:.4} GHz: |S11| = {:.4} ({:6.2} dB), Z = {:.1} {:+.1}i ohm, \
                     res {:.1e}, {:.0} s",
                    s.norm(),
                    20.0 * s.norm().log10(),
                    z.re,
                    z.im,
                    sol.residual_rel,
                    t0.elapsed().as_secs_f64()
                );
                (f, s, sol.residual_rel)
            })
            .collect()
    }
}

/// One `--radiation` point (issue #1126, X3), powers in the solver's natural
/// units (`η₀ = 1`; only ratios are reported).
struct RadRow {
    f_ghz: f64,
    s11: c64,
    z_ohm: c64,
    /// Accepted power `½ Re(V I*)` and available power `|V_inc|² / (2R)`.
    p_in: f64,
    p_avail: f64,
    /// Poynting flux through the air box shrunk by [`FLUX_SHRINK`].
    p_rad: f64,
    /// Substrate dielectric loss `½ ω ε_r tan δ ∫_sub |E|²`.
    p_diel: f64,
    /// NTFF directivity, broadside (`θ = 0`, the patch normal at `s = 0`) and maximum.
    d_broadside: f64,
    d_max: f64,
}

impl RadRow {
    fn eta_rad(&self) -> f64 {
        self.p_rad / self.p_in
    }
    /// `(P_in − P_rad − P_diel) / P_in`: what neither the flux surface nor the
    /// substrate accounts for (discretization of the flux quadrature).
    fn balance_residual(&self) -> f64 {
        (self.p_in - self.p_rad - self.p_diel) / self.p_in
    }
    /// Gain `D η_rad` and realized gain `D P_rad / P_avail` at broadside (dBi).
    fn gain_dbi(&self) -> f64 {
        10.0 * (self.d_broadside * self.eta_rad()).log10()
    }
    fn realized_gain_dbi(&self) -> f64 {
        10.0 * (self.d_broadside * self.p_rad / self.p_avail).log10()
    }
}

/// The flux surface: the air box shrunk by this fraction of its half-extent
/// (the `patch_conformal_2g4_baseline` convention), inside the UPML and
/// outside the board.
const FLUX_SHRINK: f64 = 0.10;
/// NTFF grid (θ from 0 to π, φ over [0, 2π)).
const NTFF_N_THETA: usize = 37;
const NTFF_N_PHI: usize = 72;

impl Model {
    /// P1 power balance and broadside gain at `x` (issue #1126, X3).
    fn radiation(&self, x: &[f64], freqs: &[f64]) -> Vec<RadRow> {
        let mesh = self.moved_mesh(x);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let (port, flux, inv_width) = self.port_on(&mesh);
        let lp = port.lumped_port(self.r_nat, self.v_inc);
        let center: [f64; 3] = std::array::from_fn(|k| 0.5 * (self.air_lo[k] + self.air_hi[k]));
        let half: [f64; 3] = std::array::from_fn(|k| 0.5 * (self.air_hi[k] - self.air_lo[k]));
        let flux_lo: [f64; 3] = std::array::from_fn(|k| center[k] - (1.0 - FLUX_SHRINK) * half[k]);
        let flux_hi: [f64; 3] = std::array::from_fn(|k| center[k] + (1.0 - FLUX_SHRINK) * half[k]);
        let eps_im = FR4_MATERIALS.eps_r_substrate * FR4_MATERIALS.tan_delta_substrate;
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
                .expect("radiation solve");
                assert!(sol.residual_rel < 1e-6, "unhealthy radiation solve");
                let v = port_voltage(&flux, inv_width, &sol.e_edges);
                let i = (self.v_inc * 2.0 - v) * (1.0 / self.r_nat);
                let p_in = 0.5 * (v * i.conj()).re;
                let p_avail = self.v_inc.norm_sqr() / (2.0 * self.r_nat);
                let p_rad = flux_power_box(&mesh, omega, &sol.e_edges, flux_lo, flux_hi);
                let p_diel = 0.5
                    * omega
                    * eps_im
                    * e_norm_sq_integral(
                        &mesh,
                        &self.base.tet_physical_tags,
                        PHYS_SUBSTRATE,
                        &sol.e_edges,
                    );
                let ff = ntff_far_field(
                    &mesh,
                    omega,
                    &sol.e_edges,
                    flux_lo,
                    flux_hi,
                    NTFF_N_THETA,
                    NTFF_N_PHI,
                );
                let (d_max, _) = directivity(&ff);
                let row = RadRow {
                    f_ghz: f,
                    s11: self.s11_of_v(v),
                    z_ohm: v / i * ETA_0,
                    p_in,
                    p_avail,
                    p_rad,
                    p_diel,
                    d_broadside: broadside_directivity(&ff),
                    d_max,
                };
                eprintln!(
                    "    radiation {f:.4} GHz: |S11| {:6.2} dB, P_rad/P_in {:.4}, P_diel/P_in {:.4}, \
                     balance {:+.2e}, D_broadside {:.2} dBi, realized gain {:.2} dBi",
                    20.0 * row.s11.norm().log10(),
                    row.eta_rad(),
                    row.p_diel / row.p_in,
                    row.balance_residual(),
                    10.0 * row.d_broadside.log10(),
                    row.realized_gain_dbi()
                );
                row
            })
            .collect()
    }
}

/// Write a (moved) patch mesh with the fixture's physical tags as MSH 2.2
/// ASCII (issue #1126: the Palace reference reads MSH 2.2; the tag numbers
/// are the attributes its config refers to). Elementary and physical tag are
/// both the physical tag.
fn write_msh22(path: &Path, base: &PatchFixture, mesh: &geode_core::mesh::TetMesh) {
    let mut t = String::with_capacity(64 * (mesh.n_nodes() + mesh.n_tets()));
    t.push_str("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$PhysicalNames\n7\n");
    for (dim, tag, name) in [
        (3, 1, "substrate"),
        (3, 2, "air"),
        (3, 3, "upml"),
        (2, 11, "port"),
        (2, 12, "patch"),
        (2, 13, "ground"),
        (2, 14, "outer_boundary"),
    ] {
        let _ = writeln!(t, "{dim} {tag} \"{name}\"");
    }
    t.push_str("$EndPhysicalNames\n$Nodes\n");
    let _ = writeln!(t, "{}", mesh.n_nodes());
    for (i, p) in mesh.nodes.iter().enumerate() {
        let _ = writeln!(t, "{} {:.17e} {:.17e} {:.17e}", i + 1, p[0], p[1], p[2]);
    }
    t.push_str("$EndNodes\n$Elements\n");
    let n_tri = base.boundary_triangles.len();
    let _ = writeln!(t, "{}", n_tri + mesh.n_tets());
    let mut id = 1usize;
    for (tri, &tag) in base
        .boundary_triangles
        .iter()
        .zip(&base.triangle_physical_tags)
    {
        let _ = writeln!(
            t,
            "{id} 2 2 {tag} {tag} {} {} {}",
            tri[0] + 1,
            tri[1] + 1,
            tri[2] + 1
        );
        id += 1;
    }
    for (tet, &tag) in mesh.tets.iter().zip(&base.tet_physical_tags) {
        let _ = writeln!(
            t,
            "{id} 4 2 {tag} {tag} {} {} {} {}",
            tet[0] + 1,
            tet[1] + 1,
            tet[2] + 1,
            tet[3] + 1
        );
        id += 1;
    }
    t.push_str("$EndElements\n");
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).expect("write-mesh dir");
    }
    fs::write(path, t).expect("write msh 2.2");
}

/// Port voltage `V = (1/w) Σ f_i e_i`.
fn port_voltage(flux: &[f64], inv_width: f64, e: &[c64]) -> c64 {
    let mut v = c64::new(0.0, 0.0);
    for (f, x) in flux.iter().zip(e) {
        v += *x * *f;
    }
    v * inv_width
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

// ── Designs carried across meshes, guard sites and the feed crater (#1103) ──

/// A design read back from a results TOML of this example.
struct DesignFile {
    path: PathBuf,
    sha256: String,
    label: String,
    level: String,
    fixture_sha256: String,
    radial: String,
    outline: String,
    /// `[basis].corner` / `[basis].feed` (copper mode, issue #1120; "none" when absent).
    corner: String,
    feed: String,
    /// `[basis]` DOF counts (absent in older files: 0).
    n_outline: usize,
    n_corner: usize,
    n_feed: usize,
    /// `[meta].probe_inset_mm` / `probe_w_mm` of the fixture the design was
    /// optimized on (results TOMLs).
    probe_inset: Option<f64>,
    probe_w: Option<f64>,
    x: Vec<f64>,
}

fn read_design(path: &Path) -> DesignFile {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("read design {}: {e}", path.display()));
    let v: toml::Value = text
        .parse()
        .unwrap_or_else(|e| panic!("parse design {}: {e}", path.display()));
    let s = |t: &str, k: &str| -> String {
        v[t][k]
            .as_str()
            .unwrap_or_else(|| panic!("design {} lacks [{t}].{k}", path.display()))
            .to_string()
    };
    // TOML writes integral floats (`probe_inset_mm = 7`) as integers.
    let num = |c: &toml::Value| c.as_float().or_else(|| c.as_integer().map(|i| i as f64));
    let count =
        |k: &str| -> usize { v["basis"].get(k).and_then(|c| c.as_integer()).unwrap_or(0) as usize };
    let x: Vec<f64> = v["shape"]["design_vector"]
        .as_array()
        .unwrap_or_else(|| panic!("design {} lacks [shape].design_vector", path.display()))
        .iter()
        .map(|e| e.as_float().expect("design_vector entry"))
        .collect();
    DesignFile {
        path: path.to_path_buf(),
        sha256: sha256_hex(text.as_bytes()),
        label: s("meta", "label"),
        level: s("meta", "level"),
        fixture_sha256: s("meta", "fixture_sha256"),
        radial: s("basis", "radial"),
        outline: s("basis", "outline"),
        corner: v["basis"]
            .get("corner")
            .and_then(|c| c.as_str())
            .unwrap_or("none")
            .to_string(),
        feed: v["basis"]
            .get("feed")
            .and_then(|c| c.as_str())
            .unwrap_or("none")
            .to_string(),
        n_outline: count("n_outline_dofs"),
        n_corner: count("n_corner_dofs"),
        n_feed: count("n_feed_dofs"),
        probe_inset: v["meta"].get("probe_inset_mm").and_then(num),
        probe_w: v["meta"].get("probe_w_mm").and_then(num),
        x,
    }
}

/// Repo-relative form of a path for the artifacts (no host paths).
fn repo_rel(path: &Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    match p.find("benchmarks/") {
        Some(i) => p[i..].to_string(),
        None => path
            .file_name()
            .map_or(p.clone(), |f| f.to_string_lossy().into_owned()),
    }
}

/// Where a tet sits relative to the patch, the feed and the corners.
struct TetSite {
    tet: usize,
    ratio: f64,
    region: &'static str,
    s: f64,
    y: f64,
    /// Radial offset of the base centroid from the substrate-top surface (mm; < 0 inside the slab).
    dr_mm: f64,
    feed_dist_mm: f64,
    corner_dist_mm: f64,
    /// Distance outside the patch rectangle in the chart (0 inside).
    outside_patch_mm: f64,
}

struct Geo {
    chart: CylinderChart,
    r_top: f64,
    rect: PatchRect,
    feed: (f64, f64),
}

impl Geo {
    fn site(&self, base: &PatchFixture, moved: &geode_core::mesh::TetMesh, tet: usize) -> TetSite {
        let t = base.mesh.tets[tet];
        let mut c = [0.0_f64; 3];
        for &n in &t {
            let p = base.mesh.nodes[n as usize];
            for k in 0..3 {
                c[k] += 0.25 * p[k];
            }
        }
        let (s, y) = self.chart.s_y(c);
        let corner_dist_mm = [self.rect.s[0], self.rect.s[1]]
            .iter()
            .flat_map(|&cs| {
                [self.rect.y[0], self.rect.y[1]]
                    .into_iter()
                    .map(move |cy| (s - cs).hypot(y - cy))
            })
            .fold(f64::INFINITY, f64::min);
        let ds = (self.rect.s[0] - s).max(s - self.rect.s[1]).max(0.0);
        let dy = (self.rect.y[0] - y).max(y - self.rect.y[1]).max(0.0);
        let region = match base.tet_physical_tags[tet] {
            geode_core::mesh::patch::PHYS_SUBSTRATE => "substrate",
            geode_core::mesh::patch::PHYS_AIR => "air",
            geode_core::mesh::patch::PHYS_UPML => "upml",
            _ => "other",
        };
        // Signed 6·volume of this tet on a mesh.
        let vol6 = |m: &geode_core::mesh::TetMesh| {
            let p = |k: usize| m.nodes[t[k] as usize];
            let (a, b, cc, d) = (p(0), p(1), p(2), p(3));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]];
            let w = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
            u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0])
                + u[2] * (v[0] * w[1] - v[1] * w[0])
        };
        let ratio = vol6(moved) / vol6(&base.mesh);
        TetSite {
            tet,
            ratio,
            region,
            s,
            y,
            dr_mm: self.chart.radius(c) - self.r_top,
            feed_dist_mm: (s - self.feed.0).hypot(y - self.feed.1),
            corner_dist_mm,
            outside_patch_mm: ds.hypot(dy),
        }
    }
}

impl TetSite {
    fn log(&self, what: &str) {
        eprintln!(
            "    {what}: tet {} ratio {:.3} in {} at (s, y) = ({:+.2}, {:+.2}) mm, dr {:+.2} mm \
             from the substrate top, {:.2} mm from the feed, {:.2} mm from a corner, {:.2} mm \
             outside the patch",
            self.tet,
            self.ratio,
            self.region,
            self.s,
            self.y,
            self.dr_mm,
            self.feed_dist_mm,
            self.corner_dist_mm,
            self.outside_patch_mm
        );
    }

    fn toml_inline(&self) -> String {
        format!(
            "{{ tet = {}, ratio = {:.6e}, region = \"{}\", s_mm = {:.4}, y_mm = {:.4}, dr_mm = {:.4}, \
             feed_dist_mm = {:.4}, corner_dist_mm = {:.4}, outside_patch_mm = {:.4} }}",
            self.tet,
            self.ratio,
            self.region,
            self.s,
            self.y,
            self.dr_mm,
            self.feed_dist_mm,
            self.corner_dist_mm,
            self.outside_patch_mm
        )
    }
}

/// Feed-crater and substrate-thickness metrics of a design: the radial lift
/// of the patch surface, its steepest in-surface slope and where it is.
struct Crater {
    feed_node_lift_mm: f64,
    feed_node_dist_mm: f64,
    mean_lift_mm: f64,
    min_lift_mm: f64,
    max_lift_mm: f64,
    max_slope: f64,
    max_slope_s: f64,
    max_slope_y: f64,
    max_slope_feed_dist_mm: f64,
    /// `(r_lo, r_hi, mean lift, n)` over patch nodes by distance from the feed.
    rings: Vec<(f64, f64, f64, usize)>,
}

const CRATER_RINGS_MM: [f64; 7] = [0.0, 1.0, 2.0, 4.0, 6.0, 8.0, f64::INFINITY];

fn crater(
    base: &PatchFixture,
    disp: &[[f64; 3]],
    geo: &Geo,
    patch_nodes: &[u32],
    patch_tris: &[[u32; 3]],
    exact_radius: bool,
) -> Crater {
    // The linear morphs report the radial component of the displacement (the
    // #1087 convention); the exact chart morph (#1120) reports the change of
    // the node's radius about the bend axis, which is the thickness change.
    let lift = |n: u32| {
        let p = base.mesh.nodes[n as usize];
        let d = disp[n as usize];
        if exact_radius {
            geo.chart.radius([p[0] + d[0], p[1] + d[1], p[2] + d[2]]) - geo.chart.radius(p)
        } else {
            let r = geo.chart.radial_dir(p);
            d[0] * r[0] + d[1] * r[1] + d[2] * r[2]
        }
    };
    let sy = |n: u32| geo.chart.s_y(base.mesh.nodes[n as usize]);
    let fd = |(s, y): (f64, f64)| (s - geo.feed.0).hypot(y - geo.feed.1);
    let lifts: Vec<f64> = patch_nodes.iter().map(|&n| lift(n)).collect();
    let (mut feed_node_lift_mm, mut feed_node_dist_mm) = (0.0, f64::INFINITY);
    for (&n, &l) in patch_nodes.iter().zip(&lifts) {
        let d = fd(sy(n));
        if d < feed_node_dist_mm {
            feed_node_dist_mm = d;
            feed_node_lift_mm = l;
        }
    }
    let rings = CRATER_RINGS_MM
        .windows(2)
        .map(|w| {
            let (mut acc, mut k) = (0.0, 0usize);
            for (&n, &l) in patch_nodes.iter().zip(&lifts) {
                let d = fd(sy(n));
                if d >= w[0] && d < w[1] {
                    acc += l;
                    k += 1;
                }
            }
            (w[0], w[1], if k > 0 { acc / k as f64 } else { 0.0 }, k)
        })
        .collect();
    let (mut max_slope, mut at) = (0.0_f64, (0.0, 0.0));
    for t in patch_tris {
        let (p0, p1, p2) = (sy(t[0]), sy(t[1]), sy(t[2]));
        let (l0, l1, l2) = (lift(t[0]), lift(t[1]), lift(t[2]));
        let (a, b, c, d) = (p1.0 - p0.0, p1.1 - p0.1, p2.0 - p0.0, p2.1 - p0.1);
        let det = a * d - b * c;
        if det.abs() < 1e-14 {
            continue;
        }
        let (r1, r2) = (l1 - l0, l2 - l0);
        let gs = (d * r1 - b * r2) / det;
        let gy = (a * r2 - c * r1) / det;
        let g = gs.hypot(gy);
        if g > max_slope {
            max_slope = g;
            at = ((p0.0 + p1.0 + p2.0) / 3.0, (p0.1 + p1.1 + p2.1) / 3.0);
        }
    }
    Crater {
        feed_node_lift_mm,
        feed_node_dist_mm,
        mean_lift_mm: lifts.iter().sum::<f64>() / lifts.len().max(1) as f64,
        min_lift_mm: lifts.iter().cloned().fold(f64::INFINITY, f64::min),
        max_lift_mm: lifts.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        max_slope,
        max_slope_s: at.0,
        max_slope_y: at.1,
        max_slope_feed_dist_mm: fd(at),
        rings,
    }
}

impl Crater {
    fn write(&self, t: &mut String, h: f64) {
        t.push_str("[crater]\n");
        t.push_str(
            "description = \"Radial lift of the patch surface nodes (moved - base, along the \
             outward radial normal), its steepest in-surface slope |grad lift| over the patch \
             triangles, and the lift by distance from the pinned feed. The ground does not move \
             radially, so the substrate thickness under the patch is h + lift.\"\n",
        );
        let _ = writeln!(t, "substrate_h_mm = {h}");
        let _ = writeln!(t, "feed_node_lift_mm = {:.6e}", self.feed_node_lift_mm);
        let _ = writeln!(t, "feed_node_dist_mm = {:.6e}", self.feed_node_dist_mm);
        let _ = writeln!(t, "mean_lift_mm = {:.6e}", self.mean_lift_mm);
        let _ = writeln!(t, "min_lift_mm = {:.6e}", self.min_lift_mm);
        let _ = writeln!(t, "max_lift_mm = {:.6e}", self.max_lift_mm);
        let _ = writeln!(t, "mean_thickness_mm = {:.6e}", h + self.mean_lift_mm);
        let _ = writeln!(t, "max_thickness_mm = {:.6e}", h + self.max_lift_mm);
        let _ = writeln!(t, "min_thickness_mm = {:.6e}", h + self.min_lift_mm);
        let _ = writeln!(
            t,
            "crater_depth_mm = {:.6e}",
            self.mean_lift_mm - self.feed_node_lift_mm
        );
        let _ = writeln!(t, "max_slope = {:.6e}", self.max_slope);
        let _ = writeln!(t, "max_slope_s_mm = {:.4}", self.max_slope_s);
        let _ = writeln!(t, "max_slope_y_mm = {:.4}", self.max_slope_y);
        let _ = writeln!(
            t,
            "max_slope_feed_dist_mm = {:.4}",
            self.max_slope_feed_dist_mm
        );
        let ring = |v: f64| {
            if v.is_finite() {
                format!("{v}")
            } else {
                "1e9".to_string()
            }
        };
        let _ = writeln!(
            t,
            "ring_mean_lift_mm = [{}]",
            self.rings
                .iter()
                .map(|(lo, hi, m, n)| format!(
                    "{{ r_lo_mm = {}, r_hi_mm = {}, mean_lift_mm = {m:.6e}, n_nodes = {n} }}",
                    ring(*lo),
                    ring(*hi)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        t.push('\n');
    }

    fn log(&self, h: f64) {
        eprintln!(
            "  crater: feed-node lift {:+.3} mm ({:.2} mm from the feed), mean lift {:+.3} mm \
             (thickness {:.3} mm, max {:.3}), steepest |grad lift| {:.3} at {:.2} mm from the feed",
            self.feed_node_lift_mm,
            self.feed_node_dist_mm,
            self.mean_lift_mm,
            h + self.mean_lift_mm,
            h + self.max_lift_mm,
            self.max_slope,
            self.max_slope_feed_dist_mm
        );
    }
}

/// Copper-mode geometry of a design (issue #1120): the board-thickness check
/// and the as-built copper dimensions.
struct Copper {
    /// Largest |radius change| of any substrate node (mm): 0 for the exact
    /// chart morph, so the board stays a uniform `h`.
    max_slab_radius_change_mm: f64,
    /// Patch-node radius range minus `R + h/2` (mm).
    patch_radius_dev_mm: [f64; 2],
    /// Ground-node radius range minus `R − h/2` (mm).
    ground_radius_dev_mm: [f64; 2],
    /// Moved feed centre `(s, y)` (mm), port width and gap length (mm).
    feed_s_mm: f64,
    feed_y_mm: f64,
    probe_w_mm: f64,
    port_length_mm: f64,
    /// Inset: moved `+y` edge at the feed's `s` minus the moved feed `y` (mm).
    inset_mm: f64,
    /// Patch length along `y` at `s ≈ 0` and width along `s` at `y ≈ 0` (mm, chart).
    length_mm: f64,
    width_mm: f64,
    /// Mean outward edge motion `[+y, −y, +s, −s]` (mm, chart).
    edge_outward_mm: [f64; 4],
    /// Corner motion `(Δs, Δy)` of the patch node nearest each corner
    /// `[(s_hi, y_hi), (s_lo, y_hi), (s_hi, y_lo), (s_lo, y_lo)]` (mm, chart).
    corner_disp_mm: [[f64; 2]; 4],
    /// Patch area, base and moved (mm²).
    area_base_mm2: f64,
    area_mm2: f64,
}

fn tri_area(p: [[f64; 3]; 3]) -> f64 {
    let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    0.5 * ((e1[1] * e2[2] - e1[2] * e2[1]).powi(2)
        + (e1[2] * e2[0] - e1[0] * e2[2]).powi(2)
        + (e1[0] * e2[1] - e1[1] * e2[0]).powi(2))
    .sqrt()
}

fn copper(
    model: &Model,
    geo: &Geo,
    nodes: &ConformalPatchNodes,
    patch_tris: &[[u32; 3]],
    h: f64,
    x: &[f64],
) -> Copper {
    let base = &model.base.mesh;
    let moved = model.moved_mesh(x);
    let ch = &geo.chart;
    let r_mid = ch.bend_radius;
    let max_slab_radius_change_mm = nodes
        .slab
        .iter()
        .map(|&n| (ch.radius(moved.nodes[n as usize]) - ch.radius(base.nodes[n as usize])).abs())
        .fold(0.0_f64, f64::max);
    let range = |set: &[u32], r_ref: f64| {
        set.iter()
            .fold([f64::INFINITY, f64::NEG_INFINITY], |a, &n| {
                let d = ch.radius(moved.nodes[n as usize]) - r_ref;
                [a[0].min(d), a[1].max(d)]
            })
    };
    let patch_radius_dev_mm = range(&nodes.patch, r_mid + 0.5 * h);
    let ground_radius_dev_mm = range(&nodes.ground, r_mid - 0.5 * h);
    let port = model.base.port_on_mesh(&moved);
    let port_nodes: Vec<u32> = {
        let mut v: Vec<u32> = port.faces.iter().flatten().copied().collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    // Probe centre: the midpoint of the port's chart extent (the port nodes
    // are not symmetric about it).
    let (mut s_lim, mut y_lim) = (
        [f64::INFINITY, f64::NEG_INFINITY],
        [f64::INFINITY, f64::NEG_INFINITY],
    );
    for &n in &port_nodes {
        let (s, y) = ch.s_y(moved.nodes[n as usize]);
        s_lim = [s_lim[0].min(s), s_lim[1].max(s)];
        y_lim = [y_lim[0].min(y), y_lim[1].max(y)];
    }
    let (fs, fy) = (0.5 * (s_lim[0] + s_lim[1]), 0.5 * (y_lim[0] + y_lim[1]));
    let sy = |n: u32| ch.s_y(base.nodes[n as usize]);
    let msy = |n: u32| ch.s_y(moved.nodes[n as usize]);
    let tol = 1e-6;
    let on = |pick: &dyn Fn(f64, f64) -> bool| -> Vec<u32> {
        nodes
            .patch
            .iter()
            .copied()
            .filter(|&n| {
                let (s, y) = sy(n);
                pick(s, y)
            })
            .collect()
    };
    let yp = on(&|_, y| (y - geo.rect.y[1]).abs() < tol);
    let ym = on(&|_, y| (y - geo.rect.y[0]).abs() < tol);
    let sp = on(&|s, _| (s - geo.rect.s[1]).abs() < tol);
    let sm = on(&|s, _| (s - geo.rect.s[0]).abs() < tol);
    let nearest = |set: &[u32], key: &dyn Fn(f64, f64) -> f64| -> u32 {
        *set.iter()
            .min_by(|&&a, &&b| {
                let (sa, ya) = sy(a);
                let (sb, yb) = sy(b);
                key(sa, ya).partial_cmp(&key(sb, yb)).unwrap()
            })
            .expect("edge nodes")
    };
    let f0 = geo.feed.0;
    let yp_c = nearest(&yp, &|s, _| (s - f0).abs());
    let ym_c = nearest(&ym, &|s, _| (s - f0).abs());
    let sp_c = nearest(&sp, &|_, y| y.abs());
    let sm_c = nearest(&sm, &|_, y| y.abs());
    let mean = |set: &[u32], f: &dyn Fn(u32) -> f64| {
        set.iter().map(|&n| f(n)).sum::<f64>() / set.len().max(1) as f64
    };
    let edge_outward_mm = [
        mean(&yp, &|n| msy(n).1 - sy(n).1),
        mean(&ym, &|n| sy(n).1 - msy(n).1),
        mean(&sp, &|n| msy(n).0 - sy(n).0),
        mean(&sm, &|n| sy(n).0 - msy(n).0),
    ];
    let corners = [
        (geo.rect.s[1], geo.rect.y[1]),
        (geo.rect.s[0], geo.rect.y[1]),
        (geo.rect.s[1], geo.rect.y[0]),
        (geo.rect.s[0], geo.rect.y[0]),
    ];
    let corner_disp_mm = corners.map(|(cs, cy)| {
        let n = nearest(&nodes.patch, &|s, y| (s - cs).hypot(y - cy));
        let (a, b) = (sy(n), msy(n));
        [b.0 - a.0, b.1 - a.1]
    });
    let area = |m: &geode_core::mesh::TetMesh| {
        patch_tris
            .iter()
            .map(|t| tri_area(t.map(|n| m.nodes[n as usize])))
            .sum::<f64>()
    };
    Copper {
        max_slab_radius_change_mm,
        patch_radius_dev_mm,
        ground_radius_dev_mm,
        feed_s_mm: fs,
        feed_y_mm: fy,
        probe_w_mm: port.width,
        port_length_mm: port.length,
        inset_mm: msy(yp_c).1 - fy,
        length_mm: msy(yp_c).1 - msy(ym_c).1,
        width_mm: msy(sp_c).0 - msy(sm_c).0,
        edge_outward_mm,
        corner_disp_mm,
        area_base_mm2: area(base),
        area_mm2: area(&moved),
    }
}

impl Copper {
    fn write(&self, t: &mut String) {
        t.push_str("[copper]\n");
        t.push_str(
            "description = \"Copper-only design on the fixed 1.6 mm board (issue #1120). Board \
             check: the largest radius change of any substrate node about the bend axis (0: every \
             node stays on its own cylinder, so the FR-4 is a uniform shell and the patch stays on \
             the R + h/2 offset surface) and the patch / ground radius ranges. As-built copper: \
             the moved probe centre, width and gap length (the geometric lumped port), the inset \
             (moved +y edge at the feed's s minus the feed y), the patch length at s ~ 0 and width \
             at y ~ 0, the mean outward edge motion, the corner motion and the patch area. Chart \
             coordinates (s = mid-plane arc length, y), mm.\"\n",
        );
        let _ = writeln!(
            t,
            "max_slab_radius_change_mm = {:.6e}",
            self.max_slab_radius_change_mm
        );
        let _ = writeln!(
            t,
            "patch_radius_dev_mm = [{:.6e}, {:.6e}]",
            self.patch_radius_dev_mm[0], self.patch_radius_dev_mm[1]
        );
        let _ = writeln!(
            t,
            "ground_radius_dev_mm = [{:.6e}, {:.6e}]",
            self.ground_radius_dev_mm[0], self.ground_radius_dev_mm[1]
        );
        let _ = writeln!(t, "feed_s_mm = {:.6}", self.feed_s_mm);
        let _ = writeln!(t, "feed_y_mm = {:.6}", self.feed_y_mm);
        let _ = writeln!(t, "probe_w_mm = {:.6}", self.probe_w_mm);
        let _ = writeln!(t, "port_length_mm = {:.6}", self.port_length_mm);
        let _ = writeln!(t, "inset_mm = {:.6}", self.inset_mm);
        let _ = writeln!(t, "patch_length_mm = {:.6}", self.length_mm);
        let _ = writeln!(t, "patch_width_mm = {:.6}", self.width_mm);
        let e = self.edge_outward_mm;
        let _ = writeln!(
            t,
            "edge_outward_mm = {{ y_plus = {:.6e}, y_minus = {:.6e}, s_plus = {:.6e}, s_minus = {:.6e} }}",
            e[0], e[1], e[2], e[3]
        );
        let c = self.corner_disp_mm;
        let _ = writeln!(
            t,
            "corner_disp_mm = [[{:.6e}, {:.6e}], [{:.6e}, {:.6e}], [{:.6e}, {:.6e}], [{:.6e}, {:.6e}]]",
            c[0][0], c[0][1], c[1][0], c[1][1], c[2][0], c[2][1], c[3][0], c[3][1]
        );
        let _ = writeln!(t, "patch_area_base_mm2 = {:.6}", self.area_base_mm2);
        let _ = writeln!(t, "patch_area_mm2 = {:.6}", self.area_mm2);
        t.push('\n');
    }

    fn log(&self) {
        eprintln!(
            "  copper: max slab radius change {:.1e} mm; probe at ({:+.3}, {:+.3}) mm, width {:.3} \
             mm, inset {:.3} mm; patch {:.3} x {:.3} mm (L x W), area {:.1} mm^2 (base {:.1})",
            self.max_slab_radius_change_mm,
            self.feed_s_mm,
            self.feed_y_mm,
            self.probe_w_mm,
            self.inset_mm,
            self.length_mm,
            self.width_mm,
            self.area_mm2,
            self.area_base_mm2
        );
    }
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

/// The wide-sweep grid over 2.20–2.70 GHz at `step` GHz (empty for 0).
fn wide_grid(step: f64) -> Vec<f64> {
    if step <= 0.0 {
        return Vec::new();
    }
    let n = ((WIDE_GHZ[1] - WIDE_GHZ[0]) / step).round() as usize;
    (0..=n).map(|i| WIDE_GHZ[0] + step * i as f64).collect()
}

/// Run-level context of a `--verify-only` run.
struct VerifyCtx<'a> {
    stem: &'a str,
    level: &'a str,
    fixture_sha: &'a str,
    h: f64,
    sigma0: f64,
    sigma0_fixture: f64,
    radial_spec: &'a str,
    outline_spec: &'a str,
    corner_spec: &'a str,
    feed_spec: &'a str,
    n_radial: usize,
    n_outline: usize,
    n_corner: usize,
    n_feed: usize,
    n_edges: usize,
    /// `--rebase-feed` offsets `(Δ inset, Δ width)` (mm) against the design's fixture.
    rebase: (f64, f64),
    t_start: Instant,
}

/// The basis flags of a run, for the `[meta].command` strings: the #1087 /
/// #1103 form, plus the copper flags (issue #1120) when given.
fn basis_flags(cli: &Cli) -> String {
    let mut f = format!(
        "--radial {} --outline {} --filter-radius {}",
        cli.radial
            .map_or("0".to_string(), |(a, b)| format!("{a}x{b}")),
        cli.outline,
        cli.filter_radius
    );
    if cli.copper {
        f.push_str(" --copper");
        if cli.corner_radius > 0.0 {
            let _ = write!(f, " --corners {}", cli.corner_radius);
        }
        if cli.feed {
            f.push_str(" --feed");
        }
        if cli.rebase_feed {
            f.push_str(" --rebase-feed");
        }
        if cli.feed_support_legacy {
            f.push_str(" --feed-support-legacy");
        }
        if cli.air_on_cylinder {
            f.push_str(" --air-on-cylinder");
        }
        if cli.rect4 {
            f.push_str(" --rect4");
        }
        if cli.freeze_feed_width {
            f.push_str(" --freeze-feed-width");
        }
    }
    f
}

/// The `--dense` grid (all points, or this shard's).
fn dense_grid(cli: &Cli) -> Vec<f64> {
    cli.dense.map_or_else(Vec::new, |(lo, hi, n)| {
        (0..n)
            .filter(|i| cli.shard.is_none_or(|(k, m)| i % m == k))
            .map(|i| lo + (hi - lo) * i as f64 / (n - 1).max(1) as f64)
            .collect()
    })
}

/// `--verify-only`: fresh forward sweeps of a carried design (or of the
/// initial patch) on this fixture at P1 or P2, with the guard site and the
/// crater metrics of the design as it lands on this mesh (issue #1103).
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn run_verify(
    cli: &Cli,
    model: &Model,
    geo: &Geo,
    nodes: &ConformalPatchNodes,
    patch_tris: &[[u32; 3]],
    design: Option<&DesignFile>,
    c: &VerifyCtx<'_>,
) {
    let n_dofs = model.morph.n_dofs();
    let mut x: Vec<f64> = design.map_or_else(|| vec![0.0; n_dofs], |d| d.x.clone());
    for &k in &model.frozen {
        x[k] = 0.0;
    }
    let order = if cli.p2 { "p2" } else { "p1" };
    let design_name = design.map_or_else(
        || "initial".to_string(),
        |d| format!("{}_{}", d.level, d.label),
    );
    eprintln!(
        "  verify-only: design {design_name} on {} at {order}",
        c.level
    );
    let moved = model.moved_mesh(&x);
    let (min_ratio, worst_tet) = worst_tet_volume_ratio(&model.base.mesh, &moved);
    let site = geo.site(&model.base, &moved, worst_tet);
    site.log("worst tet");
    drop(moved);
    let disp = model.morph.displacement(&model.base.mesh, &x);
    let cr = crater(
        &model.base,
        &disp,
        geo,
        &nodes.patch,
        patch_tris,
        cli.copper,
    );
    cr.log(c.h);
    let cu = cli
        .copper
        .then(|| copper(model, geo, nodes, patch_tris, c.h, &x));
    if let Some(cu) = &cu {
        cu.log();
    }
    let verify_f: Vec<f64> = (0..cli.verify_n)
        .map(|i| {
            ISM_2G4_BAND_GHZ[0]
                + (ISM_2G4_BAND_GHZ[1] - ISM_2G4_BAND_GHZ[0]) * i as f64
                    / (cli.verify_n - 1).max(1) as f64
        })
        .collect();
    let wide_f = wide_grid(cli.wide_step);
    let dense_f = dense_grid(cli);
    let run = |f: &[f64]| {
        if f.is_empty() {
            Vec::new()
        } else if cli.p2 {
            model.sweep_p2(&x, f)
        } else {
            model.sweep(&x, f)
        }
    };
    let t_sweep = Instant::now();
    let ((verify, wide, dense), peak_rss) = with_peak_rss(|| {
        eprintln!("  in-band verification sweep:");
        let v = run(&verify_f);
        let w = if wide_f.is_empty() {
            Vec::new()
        } else {
            eprintln!("  wide sweep:");
            run(&wide_f)
        };
        if !dense_f.is_empty() {
            eprintln!("  dense sweep ({} points):", dense_f.len());
        }
        let d = run(&dense_f);
        (v, w, d)
    });
    let sweep_s = t_sweep.elapsed().as_secs_f64();
    let rad = if cli.radiation.is_empty() {
        Vec::new()
    } else {
        eprintln!("  radiation (P1) at {:?} GHz:", cli.radiation);
        model.radiation(&x, &cli.radiation)
    };
    let mesh_sha = cli.write_mesh.as_ref().map(|p| {
        write_msh22(p, &model.base, &model.moved_mesh(&x));
        let sha = sha256_hex(&fs::read(p).expect("read written mesh"));
        eprintln!("  wrote the moved mesh (MSH 2.2) to {} (sha256 {sha})", p.display());
        sha
    });
    let db = |s: &c64| 20.0 * s.norm().log10();
    let worst = verify
        .iter()
        .map(|(_, s, _)| db(s))
        .fold(f64::NEG_INFINITY, f64::max);
    let best = verify
        .iter()
        .map(|(_, s, _)| db(s))
        .fold(f64::INFINITY, f64::min);
    let worst_res = verify
        .iter()
        .chain(&wide)
        .chain(&dense)
        .map(|(_, _, r)| *r)
        .fold(0.0_f64, f64::max);
    assert!(
        worst_res < 1e-6,
        "unhealthy forward (residual {worst_res:.2e})"
    );
    let reached = worst <= TARGET_DB;
    let dip = wide
        .iter()
        .map(|(f, s, _)| (*f, db(s)))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let in_band = if verify.is_empty() {
        "no in-band points".to_string()
    } else {
        format!(
            "worst {worst:.2} dB, best {best:.2} dB over {} points; reached -10 dB = {reached}",
            verify.len()
        )
    };
    eprintln!(
        "  verify {design_name} on {} ({order}): {in_band}; {} dense points; min vol ratio \
         {min_ratio:.3}; {}; {sweep_s:.0} s, peak RSS {peak_rss:.1} GiB",
        c.level,
        dense.len(),
        dip.map_or("no wide sweep".to_string(), |(f, d)| format!(
            "wide-sweep dip {f:.3} GHz ({d:.1} dB)"
        ))
    );

    let mut t = String::with_capacity(16 * 1024);
    t.push_str("# Auto-generated by `cargo run -p geode-core --release --example \\\n");
    t.push_str("#   patch_conformal_nested_dof -- --verify-only <flags>` (see [meta].command).\n");
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Consumed by `tests/patch_conformal_nested_dof.rs`.\n\n");
    t.push_str("[meta]\n");
    t.push_str(
        "description = \"Fresh forward verification of a nested-DOF design (or of the initial \
         patch, x = 0) carried onto one fixture of the patch_conformal family at one element \
         order (issue #1103, conformal v5 E4b / gate G2). The basis is a function of the chart \
         (s, y) alone, so the coefficient vector defines the same surface on every level.\"\n",
    );
    t.push_str("mode = \"verify\"\n");
    let _ = writeln!(
        t,
        "command = {}",
        toml_str(&format!(
            "cargo run -p geode-core --release --example patch_conformal_nested_dof -- --fixture \
             <dir>/{}.msh {} --verify-only{} --verify-n {} --wide-step {}{}{}{}",
            c.stem,
            basis_flags(cli),
            if cli.p2 { " --p2" } else { "" },
            cli.verify_n,
            cli.wide_step,
            if cli.radiation.is_empty() {
                String::new()
            } else {
                format!(
                    " --radiation {}",
                    cli.radiation
                        .iter()
                        .map(|f| format!("{f}"))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            },
            cli.dense.map_or(String::new(), |(lo, hi, n)| format!(
                " --dense {lo}:{hi}:{n}{}",
                cli.shard
                    .map_or(String::new(), |(k, m)| format!(" --shard {k}/{m}"))
            )),
            design.map_or(String::new(), |d| format!(
                " --design {}",
                repo_rel(&d.path)
            ))
        ))
    );
    let _ = writeln!(t, "fixture = {}", toml_str(c.stem));
    let _ = writeln!(t, "level = {}", toml_str(c.level));
    let _ = writeln!(t, "fixture_sha256 = {}", toml_str(c.fixture_sha));
    let _ = writeln!(
        t,
        "fixture_yaml = {}",
        toml_str(&format!("reference/gmsh/{}.yaml", c.stem))
    );
    let _ = writeln!(t, "element_order = \"{order}\"");
    let _ = writeln!(
        t,
        "generated_at_commit = \"{}\"",
        geode_util::repo::current_commit()
    );
    BackendInfo::of::<B>(&device()).push_meta(&mut t);
    if let Ok(host) = std::env::var("PATCH_CONFORMAL_HOST") {
        let _ = writeln!(t, "host = {}", toml_str(&host));
    }
    t.push_str("length_unit = \"mm\"\n");
    t.push_str("frequency_unit = \"GHz\"\n");
    let _ = writeln!(
        t,
        "band_f_ghz = [{}, {}]",
        ISM_2G4_BAND_GHZ[0], ISM_2G4_BAND_GHZ[1]
    );
    let _ = writeln!(t, "target_db = {TARGET_DB}");
    let _ = writeln!(t, "port_resistance_ohm = {R_PORT_OHM}");
    let _ = writeln!(t, "substrate_h_mm = {}", c.h);
    let _ = writeln!(t, "upml_sigma_0 = {}", c.sigma0);
    let _ = writeln!(t, "upml_sigma_0_fixture = {}", c.sigma0_fixture);
    let _ = writeln!(t, "n_edges = {}", c.n_edges);
    let _ = writeln!(t, "n_nodes = {}", model.base.mesh.n_nodes());
    let _ = writeln!(t, "n_tets = {}", model.base.mesh.n_tets());
    t.push('\n');

    t.push_str("[design]\n");
    let _ = writeln!(t, "name = {}", toml_str(&design_name));
    match design {
        None => t.push_str("source = \"initial\"\n"),
        Some(d) => {
            let _ = writeln!(t, "source = {}", toml_str(&repo_rel(&d.path)));
            let _ = writeln!(t, "source_sha256 = {}", toml_str(&d.sha256));
            let _ = writeln!(t, "label = {}", toml_str(&d.label));
            let _ = writeln!(t, "optimized_on_level = {}", toml_str(&d.level));
            let _ = writeln!(
                t,
                "optimized_on_fixture_sha256 = {}",
                toml_str(&d.fixture_sha256)
            );
            if cli.rebase_feed {
                let _ = writeln!(
                    t,
                    "rebase_feed_mm = [{:.9e}, {:.9e}]",
                    c.rebase.0, c.rebase.1
                );
            }
        }
    }
    if cli.freeze_feed_width {
        t.push_str(
            "feed_width_frozen = true  # --freeze-feed-width: the width coefficient is 0, the \
             port keeps the fixture's probe_w\n",
        );
    }
    let _ = writeln!(t, "radial = {}", toml_str(c.radial_spec));
    let _ = writeln!(t, "outline = {}", toml_str(c.outline_spec));
    if cli.copper {
        let _ = writeln!(t, "corner = {}", toml_str(c.corner_spec));
        let _ = writeln!(t, "feed = {}", toml_str(c.feed_spec));
    }
    let _ = writeln!(t, "n_radial_dofs = {}", c.n_radial);
    let _ = writeln!(t, "n_outline_dofs = {}", c.n_outline);
    if cli.copper {
        let _ = writeln!(t, "n_corner_dofs = {}", c.n_corner);
        let _ = writeln!(t, "n_feed_dofs = {}", c.n_feed);
        let _ = writeln!(
            t,
            "air_motion = \"{}\"",
            if cli.air_on_cylinder {
                "on_cylinder"
            } else {
                "linear"
            }
        );
    }
    let _ = writeln!(
        t,
        "design_vector = [{}]",
        x.iter()
            .map(|v| format!("{v:.9e}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    t.push('\n');

    t.push_str("[verify]\n");
    let _ = writeln!(t, "verify_n_points = {}", verify.len());
    if !verify.is_empty() {
        let _ = writeln!(t, "verify_worst_s11_db = {worst:.6e}");
        let _ = writeln!(t, "verify_best_s11_db = {best:.6e}");
        let _ = writeln!(t, "reached_target = {reached}");
    }
    let _ = writeln!(t, "wide_step_ghz = {}", cli.wide_step);
    if let Some((lo, hi, n)) = cli.dense {
        let _ = writeln!(t, "dense_grid_ghz = [{lo}, {hi}]");
        let _ = writeln!(t, "dense_grid_n = {n}");
        let (k, m) = cli.shard.unwrap_or((0, 1));
        let _ = writeln!(t, "dense_shard = [{k}, {m}]");
        let _ = writeln!(t, "dense_n_points = {}", dense.len());
    }
    if let Some((f, d)) = dip {
        let _ = writeln!(t, "wide_dip_f_ghz = {f:.4}");
        let _ = writeln!(t, "wide_dip_db = {d:.4}");
    }
    let _ = writeln!(t, "min_vol_ratio = {min_ratio:.6e}");
    let _ = writeln!(t, "worst_tet = {}", site.toml_inline());
    let _ = writeln!(t, "worst_solve_residual_rel = {worst_res:.3e}");
    let _ = writeln!(t, "n_driven_solves = {}", model.n_solves.get());
    let _ = writeln!(t, "sweep_wall_s = {sweep_s:.1}");
    let _ = writeln!(t, "sweep_peak_rss_gib = {peak_rss:.3}");
    let _ = writeln!(t, "total_wall_s = {:.1}", c.t_start.elapsed().as_secs_f64());
    t.push('\n');
    cr.write(&mut t, c.h);
    if let Some(cu) = &cu {
        cu.write(&mut t);
    }
    for (f, s, res) in &verify {
        t.push_str("[[s11_verify.point]]\n");
        let _ = writeln!(t, "f_ghz = {f:.6}");
        let _ = writeln!(t, "s11_re = {:.9e}", s.re);
        let _ = writeln!(t, "s11_im = {:.9e}", s.im);
        let _ = writeln!(t, "s11_mag = {:.9e}", s.norm());
        let _ = writeln!(t, "s11_db = {:.6}", db(s));
        let _ = writeln!(t, "solve_residual_rel = {res:.3e}");
        t.push('\n');
    }
    for (f, s, _) in &wide {
        t.push_str("[[s11_wide.point]]\n");
        let _ = writeln!(t, "f_ghz = {f:.6}");
        let _ = writeln!(t, "s11_mag = {:.9e}", s.norm());
        let _ = writeln!(t, "s11_db = {:.6}", db(s));
        t.push('\n');
    }
    if let Some(sha) = &mesh_sha {
        t.push_str("[moved_mesh]\n");
        t.push_str(
            "description = \"The moved mesh of this design on this fixture, written as MSH 2.2 \
             ASCII with the fixture's physical tags (--write-mesh) for the Palace reference \
             (issue #1126); not committed (regenerate with this command plus --write-mesh).\"\n",
        );
        let _ = writeln!(t, "sha256 = {}", toml_str(sha));
        t.push('\n');
    }
    if !rad.is_empty() {
        t.push_str("[radiation]\n");
        t.push_str(
            "description = \"P1 power balance and broadside gain (issue #1126, X3). p_in = \
             1/2 Re(V I*) (accepted), p_avail = |V_inc|^2 / (2 R); p_rad = box Poynting flux \
             through the air box shrunk by flux_shrink of its half-extent (inside the UPML, \
             outside the board); p_diel = 1/2 omega eps_r tan_delta int_substrate |E|^2 \
             (exact P1 quadrature); eta_rad = p_rad / p_in; balance = (p_in - p_rad - p_diel) \
             / p_in. Directivity from the Love-equivalence NTFF on the same box (theta from 0 to \
             pi, phi over [0, 2 pi)); broadside = theta 0, the patch normal at s = 0. gain = D \
             eta_rad; realized gain = D p_rad / p_avail = gain (1 - |S11|^2). Conductors are PEC \
             (no copper loss).\"\n",
        );
        let _ = writeln!(t, "flux_shrink = {FLUX_SHRINK}");
        let _ = writeln!(t, "ntff_grid = [{NTFF_N_THETA}, {NTFF_N_PHI}]");
        let _ = writeln!(
            t,
            "substrate_eps_r = {}\nsubstrate_tan_delta = {}",
            FR4_MATERIALS.eps_r_substrate, FR4_MATERIALS.tan_delta_substrate
        );
        t.push('\n');
        for r in &rad {
            t.push_str("[[radiation.point]]\n");
            let _ = writeln!(t, "f_ghz = {:.6}", r.f_ghz);
            let _ = writeln!(t, "s11_re = {:.9e}", r.s11.re);
            let _ = writeln!(t, "s11_im = {:.9e}", r.s11.im);
            let _ = writeln!(t, "s11_db = {:.6}", db(&r.s11));
            let _ = writeln!(t, "z_re_ohm = {:.6}", r.z_ohm.re);
            let _ = writeln!(t, "z_im_ohm = {:.6}", r.z_ohm.im);
            let _ = writeln!(t, "p_in = {:.9e}", r.p_in);
            let _ = writeln!(t, "p_avail = {:.9e}", r.p_avail);
            let _ = writeln!(t, "p_rad = {:.9e}", r.p_rad);
            let _ = writeln!(t, "p_diel = {:.9e}", r.p_diel);
            let _ = writeln!(t, "eta_rad = {:.6}", r.eta_rad());
            let _ = writeln!(t, "diel_fraction = {:.6}", r.p_diel / r.p_in);
            let _ = writeln!(t, "balance_residual = {:.4e}", r.balance_residual());
            let _ = writeln!(
                t,
                "mismatch_check = {:.3e}",
                r.p_in / r.p_avail - (1.0 - r.s11.norm_sqr())
            );
            let _ = writeln!(
                t,
                "directivity_broadside_dbi = {:.4}",
                10.0 * r.d_broadside.log10()
            );
            let _ = writeln!(t, "directivity_max_dbi = {:.4}", 10.0 * r.d_max.log10());
            let _ = writeln!(t, "gain_broadside_dbi = {:.4}", r.gain_dbi());
            let _ = writeln!(
                t,
                "realized_gain_broadside_dbi = {:.4}",
                r.realized_gain_dbi()
            );
            t.push('\n');
        }
    }
    for (f, s, res) in &dense {
        t.push_str("[[s11_dense.point]]\n");
        let _ = writeln!(t, "f_ghz = {f:.6}");
        let _ = writeln!(t, "s11_re = {:.9e}", s.re);
        let _ = writeln!(t, "s11_im = {:.9e}", s.im);
        let _ = writeln!(t, "s11_mag = {:.9e}", s.norm());
        let _ = writeln!(t, "s11_db = {:.6}", db(s));
        let _ = writeln!(t, "solve_residual_rel = {res:.3e}");
        t.push('\n');
    }
    let out = cli.out.clone().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../benchmarks/patch_antenna_conformal/nested_dof/verify/{design_name}_on_{}{}.toml",
            c.level,
            if cli.p2 { "_p2" } else { "" }
        ))
    });
    if let Some(p) = out.parent() {
        fs::create_dir_all(p).expect("out dir");
    }
    fs::write(&out, &t).expect("write verify toml");
    println!(
        "patch_conformal_nested_dof [verify {design_name} on {} {order}]: {}",
        c.level,
        out.display()
    );
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
        let mut cols =
            radial_columns(&base.mesh, &chart, rb, &nodes, radial_scale).expect("radial columns");
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
    if let Some(ob) = outline_basis.as_ref().filter(|_| cli.copper && cli.rect4) {
        // Rectangle baseline (issue #1126, X4): two DOFs, each the sum of the
        // splines of two opposite edges at half weight.
        outline_spec = format!(
            "rect4: 2 DOFs [dW, dL] (mm, total patch width / length change): dW moves the +s \
             and -s edges outward by dW/2 each, dL the +y and -y edges by dL/2 each, as the sum \
             of that edge's {} clamped uniform B-splines (degree {}; a partition of unity, so the \
             edge translates uniformly); same ramp from the pinned feed band (half-width {:.2} \
             mm), {sub_pad} mm margin decay and exact on-cylinder chart morph as the per-edge \
             outline basis",
            cli.outline,
            cli.outline.saturating_sub(1).min(SPLINE_DEGREE),
            0.5 * probe_w
        );
        n_outline = 2;
        let _ = ob;
    } else if let Some(ob) = outline_basis.as_ref().filter(|_| cli.copper) {
        // Copper mode (issue #1120): the same per-edge splines, applied by the
        // exact on-cylinder chart morph below (no chord error).
        outline_spec = format!(
            "{} clamped uniform B-splines (degree {}) per patch edge [+y, -y, +s, -s], each DOF \
             moving its edge outward in the surface by {OUTLINE_SCALE_MM} mm (mid-plane arc) per \
             unit; carried into the slab by a smoothstep ramp from the pinned feed band \
             (half-width {:.2} mm) to the edge on every substrate node (top, interior, ground) and \
             decaying over the {sub_pad} mm margin; exact on-cylinder chart morph (each substrate \
             node keeps its radius)",
            cli.outline,
            cli.outline.saturating_sub(1).min(SPLINE_DEGREE),
            0.5 * probe_w
        );
        n_outline = ob.n_dofs();
    } else if let Some(ob) = &outline_basis {
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
    let mut corner_spec = String::from("none");
    let mut feed_spec = String::from("none");
    let mut n_corner = 0;
    let mut n_feed = 0;
    let morph = if cli.copper {
        let corner_basis =
            (cli.corner_radius > 0.0).then(|| CornerBasis::new(rect, cli.corner_radius));
        let feed_basis = cli.feed.then(|| {
            if cli.feed_support_legacy {
                FeedBasis {
                    s: feed.0,
                    y: feed.1,
                    half_width: 0.5 * probe_w,
                    r_inner: FEED_CORE_FRAC * probe_w,
                    r_outer: FEED_SUPPORT_FRAC * probe_w,
                }
            } else {
                FeedBasis::capped(
                    &rect,
                    feed.0,
                    feed.1,
                    0.5 * probe_w,
                    FEED_CORE_FRAC * probe_w,
                    FEED_SUPPORT_FRAC * probe_w,
                )
                .expect("feed basis")
            }
        });
        if let Some(fb) = &feed_basis
            && fb.r_outer > FeedBasis::outline_clearance(&rect, feed.0, feed.1)
        {
            eprintln!(
                "  WARNING: --feed-support-legacy: the feed support ({:.2} mm) reaches the patch \
                 outline ({:.2} mm from the feed): the feed DOFs also move the copper edge",
                fb.r_outer,
                FeedBasis::outline_clearance(&rect, feed.0, feed.1)
            );
        }
        if let Some(cb) = &corner_basis {
            n_corner = cb.n_dofs();
            corner_spec = format!(
                "4 corner bumps [(s_hi, y_hi), (s_lo, y_hi), (s_hi, y_lo), (s_lo, y_lo)], each moving \
                 its corner outward along the bisector by 1 mm per unit (negative = chamfer) with \
                 support 1 - smoothstep(d / {} mm) of the chart distance to the corner, on every \
                 substrate node; exact on-cylinder chart morph",
                cli.corner_radius
            );
        }
        if let Some(fb) = &feed_basis {
            n_feed = fb.n_dofs();
            // The spec string is unchanged where the cap is inactive (the
            // original fixtures), so their committed designs still match.
            let capped = if fb.r_outer < FEED_SUPPORT_FRAC * probe_w {
                " (capped at the patch outline)"
            } else {
                ""
            };
            feed_spec = format!(
                "2 feed DOFs along y (the bend axis): [inset: probe moves 1 mm away from the +y \
                 edge per unit; width: probe (port) width +1 mm per unit, uniform y scaling of the \
                 port faces]; rigid within {:.2} mm of the feed centre, smoothstep to 0 at {:.2} \
                 mm{capped} (the +y edge is {probe_inset} mm away), on every substrate node; \
                 moving-feed adjoint (port width / length / flux re-derived on every moved mesh)",
                fb.r_inner, fb.r_outer
            );
        }
        let n_cols = n_outline + n_corner + n_feed;
        let ob = outline_basis.clone();
        let rect4 = cli.rect4;
        let air = if cli.air_on_cylinder {
            AirMotion::OnCylinder
        } else {
            AirMotion::Linear
        };
        let cm = ChartMorph::build_with(
            &base.mesh,
            &chart,
            &nodes,
            n_cols,
            |s, y| {
                let mut v = Vec::with_capacity(n_cols);
                if let Some(ob) = &ob {
                    let e = ob.eval(s, y);
                    if rect4 {
                        // [+y, -y, +s, -s] blocks of n splines each.
                        let n = e.len() / 4;
                        let sum = |r: std::ops::Range<usize>| {
                            e[r].iter()
                                .fold([0.0_f64; 2], |a, b| [a[0] + b[0], a[1] + b[1]])
                        };
                        let (yp, ym, sp, sm) =
                            (sum(0..n), sum(n..2 * n), sum(2 * n..3 * n), sum(3 * n..4 * n));
                        let half = 0.5 * OUTLINE_SCALE_MM;
                        v.push([half * (sp[0] + sm[0]), half * (sp[1] + sm[1])]);
                        v.push([half * (yp[0] + ym[0]), half * (yp[1] + ym[1])]);
                    } else {
                        v.extend(
                            e.into_iter()
                                .map(|[a, b]| [OUTLINE_SCALE_MM * a, OUTLINE_SCALE_MM * b]),
                        );
                    }
                }
                if let Some(cb) = &corner_basis {
                    v.extend(cb.eval(s, y));
                }
                if let Some(fb) = &feed_basis {
                    v.extend(fb.eval(s, y));
                }
                v
            },
            air,
        )
        .expect("chart morph");
        DesignMorph::Chart(cm)
    } else {
        DesignMorph::Linear(FreeformBoundaryMorph::from_columns(columns).expect("morph"))
    };
    let n_dofs = morph.n_dofs();
    let morph_s = t_morph.elapsed().as_secs_f64();
    // A design carried from another run (possibly another mesh level): the
    // basis is a function of the chart alone, so the same coefficients give
    // the same surface on every level, provided the basis is the same.
    let mut design = cli.design.as_deref().map(read_design);
    // Feed re-base offsets `(Δ inset, Δ width)` (mm) of this fixture against the
    // design's (`--rebase-feed`).
    let mut rebase = (0.0_f64, 0.0_f64);
    if let Some(d) = design.as_mut() {
        assert_eq!(
            d.x.len(),
            n_dofs,
            "design {} has {} DOFs, this basis {n_dofs}",
            d.path.display(),
            d.x.len()
        );
        assert_eq!(
            d.radial, radial_spec,
            "design radial basis differs from this run's"
        );
        assert_eq!(d.corner, corner_spec, "design corner basis differs");
        if cli.rebase_feed {
            // The outline / feed spec strings name the fixture's probe geometry;
            // the bases are otherwise the same (same DOF layout). The edge motion
            // of the outline DOFs does not depend on the feed band, so only the
            // two feed coefficients change: x_inset' = x_inset - (inset' - inset),
            // x_width' = x_width - (w' - w). That carries the probe exactly. It
            // carries the copper outline exactly only if the feed DOFs leave the
            // outline fixed on BOTH fixtures: a feed support reaching the +y edge
            // drags it by g(edge) per unit, and that drag is lost on re-base
            // (#1120 Judge: 0.47 mm for rbC -> rbE, 0.09 mm for rbD -> rbF with
            // the uncapped 4 · probe_w support). So refuse it unless
            // --feed-support-legacy (which reproduces the committed artifacts).
            assert_eq!(
                (d.n_outline, d.n_corner, d.n_feed),
                (n_outline, n_corner, n_feed),
                "design DOF layout differs"
            );
            let (pi, pw) = (
                d.probe_inset.expect("design lacks [meta].probe_inset_mm"),
                d.probe_w.expect("design lacks [meta].probe_w_mm"),
            );
            // The design's support reached its outline if it was built uncapped
            // (no "capped" marker) with 4 · probe_w beyond its inset.
            let src_drags = !d.feed.contains("capped at the patch outline")
                && FEED_SUPPORT_FRAC * pw > pi.min(0.5 * patch_w);
            let here_drags = cli.feed_support_legacy
                && FEED_SUPPORT_FRAC * probe_w
                    > FeedBasis::outline_clearance(&rect, feed.0, feed.1);
            if src_drags || here_drags {
                assert!(
                    cli.feed_support_legacy,
                    "--rebase-feed is not exact here: the design's feed support reaches its \
                     patch outline, so its copper edge carries a feed-DOF drag this re-base \
                     drops (pass --feed-support-legacy to reproduce the committed #1120 \
                     artifacts anyway)"
                );
                eprintln!(
                    "  WARNING: rebase-feed is NOT exact: a feed support reaches the +y edge \
                     (design fixture: {src_drags}, this fixture: {here_drags}), so the copper \
                     outline differs from the design's; compare the [copper] sections"
                );
            }
            rebase = (probe_inset - pi, probe_w - pw);
            let k = n_outline + n_corner;
            d.x[k] -= rebase.0;
            d.x[k + 1] -= rebase.1;
            eprintln!(
                "  rebase-feed: design fixture inset {pi} / width {pw} mm -> this fixture \
                 {probe_inset} / {probe_w} mm; feed coefficients [{:.6}, {:.6}]",
                d.x[k],
                d.x[k + 1]
            );
        } else {
            assert_eq!(
                d.outline, outline_spec,
                "design outline basis differs from this run's"
            );
            assert_eq!(d.feed, feed_spec, "design feed basis differs");
        }
    }
    let geo = Geo {
        chart,
        r_top: nodes.r_top,
        rect,
        feed,
    };

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
    // Objective band points, shifted down by `--band-shift` (frequency space
    // mapping: a design whose resonance rises by `shift` at higher fidelity is
    // optimized for the band `shift` lower on this mesh).
    let band_f: Vec<f64> = (0..cli.n_band)
        .map(|i| {
            ISM_2G4_BAND_GHZ[0]
                + (ISM_2G4_BAND_GHZ[1] - ISM_2G4_BAND_GHZ[0]) * i as f64 / (cli.n_band - 1) as f64
                - cli.band_shift
        })
        .collect();
    let band: Vec<BandPoint> = band_f
        .iter()
        .map(|&f| {
            let omega = ghz_to_omega_mm(f);
            let (eps, nu) = base.matched_upml_materials(
                &FR4_MATERIALS,
                air_lo,
                air_hi,
                pml_thick,
                sigma0,
                omega,
            );
            (f, omega, eps, nu)
        })
        .collect();
    let model = Model {
        source: CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; base.mesh.n_tets()],
        },
        base,
        morph,
        moving_port: cli.feed,
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
        frozen: if cli.freeze_feed_width {
            vec![n_outline + n_corner + 1]
        } else {
            Vec::new()
        },
    };
    let n_nodes = model.base.mesh.n_nodes();
    let n_edges = edges.len();
    let n_tets = model.base.mesh.n_tets();
    eprintln!(
        "patch_conformal_nested_dof [{}]: {stem} ({n_edges} edges, {n_nodes} nodes, {n_tets} \
         tets), R = {bend_radius} mm, sigma_0 = {sigma0} (fixture {sigma0_fixture}); DOFs: \
         {n_radial} radial + {n_outline} outline + {n_corner} corner + {n_feed} feed = {n_dofs}; \
         band {:?} GHz; morph built in {morph_s:.1} s",
        cli.label, band_f
    );

    if cli.verify_only {
        run_verify(
            &cli,
            &model,
            &geo,
            &nodes,
            &patch_tris,
            design.as_ref(),
            &VerifyCtx {
                stem: &stem,
                level: &level,
                fixture_sha: &fixture_sha,
                h,
                sigma0,
                sigma0_fixture,
                radial_spec: &radial_spec,
                outline_spec: &outline_spec,
                corner_spec: &corner_spec,
                feed_spec: &feed_spec,
                n_radial,
                n_outline,
                n_corner,
                n_feed,
                n_edges,
                rebase,
                t_start,
            },
        );
        return;
    }

    // ── Initial evaluation + FD gates ──
    // Cold start at the fixture (x = 0) or warm start at a carried design,
    // scaled back by halving until it clears the volume guard on this mesh.
    let (x0, warm_scale) = match &design {
        None => (vec![0.0_f64; n_dofs], 0.0),
        Some(d) => {
            let mut scale = 1.0_f64;
            loop {
                let xs: Vec<f64> = d
                    .x
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        if model.frozen.contains(&i) {
                            0.0
                        } else {
                            v * scale
                        }
                    })
                    .collect();
                let vr = model.min_vol_ratio(&xs);
                eprintln!(
                    "  warm start from {} (scale {scale}): min vol ratio {vr:.3}",
                    repo_rel(&d.path)
                );
                if vr >= MIN_VOL_RATIO {
                    break (xs, scale);
                }
                scale *= 0.5;
                assert!(scale > 1e-3, "warm start cannot clear the volume guard");
            }
        }
    };
    let t0 = Instant::now();
    let (e0, rss0) = with_peak_rss(|| model.eval(&x0, true));
    let eval_s = t0.elapsed().as_secs_f64();
    assert!(
        e0.worst_residual < 1e-6,
        "unhealthy forward (residual {:.2e})",
        e0.worst_residual
    );
    if design.is_none() {
        assert!((e0.min_vol_ratio - 1.0).abs() < 1e-9);
    }
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
        if !cli.copper {
            if n_radial > 0 {
                let d = rng.unit_direction(n_dofs, |i| i < n_radial);
                fd_checks.push(fd_check(&model, &x0, &grad0, &d, "random_radial", "x0"));
            }
            if n_outline > 0 {
                let d = rng.unit_direction(n_dofs, |i| i >= n_radial);
                fd_checks.push(fd_check(&model, &x0, &grad0, &d, "random_outline", "x0"));
            }
        } else if cli.fd_only {
            // Copper mode, FD-only (issue #1120): every DOF group separately and
            // each feed DOF alone (the inset is a rigid port translation, the width
            // exercises the moving-feed port terms).
            let (o_end, c_end) = (n_outline, n_outline + n_corner);
            if n_outline > 0 {
                let d = rng.unit_direction(n_dofs, |i| i < o_end);
                fd_checks.push(fd_check(&model, &x0, &grad0, &d, "random_outline", "x0"));
            }
            if n_corner > 0 {
                let d = rng.unit_direction(n_dofs, |i| (o_end..c_end).contains(&i));
                fd_checks.push(fd_check(&model, &x0, &grad0, &d, "random_corner", "x0"));
            }
            for (k, name) in ["feed_inset", "feed_width"].iter().enumerate().take(n_feed) {
                if model.frozen.contains(&(c_end + k)) {
                    continue;
                }
                let d: Vec<f64> = (0..n_dofs)
                    .map(|i| if i == c_end + k { 1.0 } else { 0.0 })
                    .collect();
                fd_checks.push(fd_check(&model, &x0, &grad0, &d, name, "x0"));
            }
        } else {
            let d = rng.unit_direction(n_dofs, |i| !model.frozen.contains(&i));
            fd_checks.push(fd_check(&model, &x0, &grad0, &d, "random_all", "x0"));
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
    let mut guard_events: Vec<(usize, f64, TetSite)> = Vec::new();
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
        let mut a = if dmax > MAX_STEP_INF {
            MAX_STEP_INF / dmax
        } else {
            1.0
        };

        let mut backtracks = 0;
        let mut guard_binding = false;
        let accepted = loop {
            let xt: Vec<f64> = x.iter().zip(&d).map(|(xi, di)| xi + a * di).collect();
            let moved_t = model.moved_mesh(&xt);
            let (vr, wt) = worst_tet_volume_ratio(&model.base.mesh, &moved_t);
            if vr < MIN_VOL_RATIO {
                guard_binding = true;
                let site = geo.site(&model.base, &moved_t, wt);
                site.log(&format!("guard binds (step {iter}, a = {a:.3e})"));
                guard_events.push((iter, a, site));
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
        let d = rng.unit_direction(n_dofs, |i| !model.frozen.contains(&i));
        fd_checks.push(fd_check(
            &model,
            &x_final,
            &grad_final,
            &d,
            "random_all",
            "x_final",
        ));
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
    let wide_f = wide_grid(cli.wide_step);
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
    let disp = model.morph.displacement(&model.base.mesh, &x_final);
    let max_disp = disp
        .iter()
        .map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
        .fold(0.0_f64, f64::max);
    let (worst_ratio_final, worst_tet) =
        worst_tet_volume_ratio(&model.base.mesh, &model.moved_mesh(&x_final));
    let final_site = geo.site(&model.base, &model.moved_mesh(&x_final), worst_tet);
    final_site.log("worst tet at x_final");
    let crater_final = crater(
        &model.base,
        &disp,
        &geo,
        &nodes.patch,
        &patch_tris,
        cli.copper,
    );
    crater_final.log(h);
    let copper_final = cli
        .copper
        .then(|| copper(&model, &geo, &nodes, &patch_tris, h, &x_final));
    if let Some(cu) = &copper_final {
        cu.log();
    }
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
        let description_1087 = "Optimized curved conformal 2.4 GHz patch (issue #1087): base and moved \
                coordinates of the substrate-top surface nodes (the patch nodes among them, \
                is_patch) with the patch and substrate-top triangles, bent frame. The ground \
                plane is not dumped: it does not move radially, and its mesh nodes slide only \
                tangentially under outline DOFs, which leaves it on the cylinder up to the chord \
                error d^2/2R. Consumed by the Meep matched-geometry work (#1088) and \
                benchmarks/patch_antenna_conformal/scripts/plot_patch_displacement.py.";
        let description_1120 = "Optimized copper of the curved conformal 2.4 GHz patch on a fixed, \
                uniform 1.6 mm FR-4 board (issue #1120): base and moved coordinates of the \
                substrate-top surface nodes (the patch nodes among them, is_patch) with the patch \
                and substrate-top triangles, bent frame. Every substrate node keeps its radius \
                about the bend axis (exact on-cylinder chart morph), so the board is the \
                undeformed R = 100 mm shell and only the etched copper (outline, corners) and the \
                probe (feed_moved_s_y_mm, probe_w_moved_mm) change. The ground plane is the \
                unchanged substrate bottom and is not dumped. Consumed by the Meep \
                matched-geometry work (#1088) and \
                benchmarks/patch_antenna_conformal/scripts/plot_patch_displacement.py.";
        field(
            "description",
            js(if cli.copper {
                description_1120
            } else {
                description_1087
            }),
        );
        field("index_base", "0".to_string());
        field(
            "format",
            js(
                "node_ids, is_patch, base and moved are parallel arrays, one entry per dumped \
                node; node_ids holds the 0-based node index in the fixture mesh. \
                patch_triangles, substrate_top_triangles and patch_outline_loop hold 0-based \
                indices into these local arrays, not mesh node ids (map through node_ids).",
            ),
        );
        field("units", js("mm"));
        field(
            "frame",
            js(
                "bent fixture coordinates (x, y, z); bend axis parallel to y through (x = 0, z = \
                z_axis_mm); chart s = R*atan2(x, z - z_axis), y",
            ),
        );
        field("fixture", js(&stem));
        field("level", js(&level));
        field("fixture_sha256", js(&fixture_sha));
        field("design_label", js(&cli.label));
        field(
            "generated_at_commit",
            js(&geode_util::repo::current_commit()),
        );
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
        if let Some(cu) = &copper_final {
            field("corner_basis", js(&corner_spec));
            field("feed_basis", js(&feed_spec));
            field("n_corner_dofs", format!("{n_corner}"));
            field("n_feed_dofs", format!("{n_feed}"));
            field("thickness_fixed", "true".to_string());
            field(
                "max_slab_radius_change_mm",
                format!("{:e}", cu.max_slab_radius_change_mm),
            );
            field(
                "feed_moved_s_y_mm",
                format!("[{},{}]", r6(cu.feed_s_mm), r6(cu.feed_y_mm)),
            );
            field("probe_w_moved_mm", format!("{}", r6(cu.probe_w_mm)));
            field("inset_moved_mm", format!("{}", r6(cu.inset_mm)));
            field("band_shift_ghz", format!("{}", cli.band_shift));
        }
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
                ids.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
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
             <dir>/{stem}.msh {}{} --n-band {} --beta {} --max-steps {} --verify-n {} --wide-step \
             {} --label {}{}{}",
            basis_flags(&cli),
            if cli.band_shift != 0.0 {
                format!(" --band-shift {}", cli.band_shift)
            } else {
                String::new()
            },
            cli.n_band,
            cli.beta,
            cli.max_steps,
            cli.verify_n,
            cli.wide_step,
            cli.label,
            if cli.fd_only { " --fd-only" } else { "" },
            design.as_ref().map_or(String::new(), |d| format!(
                " --design {}",
                repo_rel(&d.path)
            ))
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
    if let Ok(host) = std::env::var("PATCH_CONFORMAL_HOST") {
        let _ = writeln!(t, "host = {}", toml_str(&host));
    }
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
    if cli.copper || cli.band_shift != 0.0 {
        let _ = writeln!(t, "band_shift_ghz = {}", cli.band_shift);
    }
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
    if cli.copper {
        let _ = writeln!(t, "corner = {}", toml_str(&corner_spec));
        let _ = writeln!(t, "feed = {}", toml_str(&feed_spec));
        t.push_str(
            "morph = \"chart_exact: every substrate node moves to angle phi0 + ds/R and y0 + dy at \
             its base radius (ChartMorph, issue #1120); air nodes follow the harmonic extension of \
             the first-order motion; UPML shell and outer boundary pinned\"\n",
        );
        t.push_str("thickness = \"fixed: uniform 1.6 mm FR-4, radial (gap) DOFs frozen\"\n");
        let _ = writeln!(
            t,
            "air_motion = \"{}\"",
            if cli.air_on_cylinder {
                "on_cylinder"
            } else {
                "linear"
            }
        );
    }
    let _ = writeln!(t, "n_radial_dofs = {n_radial}");
    let _ = writeln!(t, "n_outline_dofs = {n_outline}");
    if cli.copper {
        let _ = writeln!(t, "n_corner_dofs = {n_corner}");
        let _ = writeln!(t, "n_feed_dofs = {n_feed}");
    }
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
    let _ = writeln!(
        t,
        "edge_mean_outward_mm = {{ y_plus = {yp:.6e}, y_minus = {ym:.6e}, s_plus = {sp:.6e}, s_minus = {sm:.6e} }}"
    );
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

    t.push_str("[start]\n");
    match &design {
        None => t.push_str("kind = \"cold\"\n"),
        Some(d) => {
            t.push_str("kind = \"warm\"\n");
            let _ = writeln!(t, "design_toml = {}", toml_str(&repo_rel(&d.path)));
            let _ = writeln!(t, "design_toml_sha256 = {}", toml_str(&d.sha256));
            let _ = writeln!(t, "design_label = {}", toml_str(&d.label));
            let _ = writeln!(t, "design_level = {}", toml_str(&d.level));
            let _ = writeln!(t, "design_fixture_sha256 = {}", toml_str(&d.fixture_sha256));
            let _ = writeln!(t, "scale = {warm_scale}");
            if cli.rebase_feed {
                let _ = writeln!(t, "rebase_feed_mm = [{:.9e}, {:.9e}]", rebase.0, rebase.1);
            }
        }
    }
    let _ = writeln!(
        t,
        "x_start = [{}]",
        x0.iter()
            .map(|v| format!("{v:.9e}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    t.push('\n');

    t.push_str("[guard]\n");
    t.push_str(
        "description = \"Where the per-tet volume guard (min ratio 0.25) binds: every line-search \
         trial it rejected (step, trial step length a, and the worst tet of the trial), and the \
         worst tet of the reported design. Sites are base-mesh centroids in the chart: (s, y), dr \
         = radial offset from the substrate top (< 0 inside the slab), distances to the pinned \
         feed and to the nearest patch corner, and the distance outside the patch rectangle.\"\n",
    );
    let _ = writeln!(t, "n_events = {}", guard_events.len());
    let _ = writeln!(t, "final_worst = {}", final_site.toml_inline());
    t.push('\n');
    for (it, a, site) in &guard_events {
        t.push_str("[[guard.event]]\n");
        let _ = writeln!(t, "step = {it}");
        let _ = writeln!(t, "a = {a:.6e}");
        let _ = writeln!(t, "site = {}", site.toml_inline());
        t.push('\n');
    }
    crater_final.write(&mut t, h);
    if let Some(cu) = &copper_final {
        cu.write(&mut t);
    }

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
