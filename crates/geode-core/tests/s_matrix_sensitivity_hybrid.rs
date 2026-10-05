//! **Hybrid port-mode terms in the 3-D N-port ∂S** (Epic #841 Phase 3b,
//! issue #872): [`s_matrix_sensitivity_sweep`] / [`s_matrix_vjp`] on designs
//! that **touch a hybrid port face** — a material region on the face, or a
//! shape column that moves face nodes (strip width, substrate height) —
//! against **central finite differences through the public hybrid spec
//! sweep** (`solve_wave_port_spec_sweep_with_mode`, with the port faces
//! rebuilt from the perturbed volume, as a user re-running the forward
//! would), plus the forward-parity guard, the mutation tripwires of the
//! three new term families, reciprocity of ∂S, VJP = JVP, the General
//! transpose path, and the degenerate-cluster error.
//!
//! Fixtures:
//!
//! 1. **Slab-loaded guide** (`2 × 1 × 1.2`, `8 × 4 × 4` cells, `ε = 2.25`
//!    below `y = 0.5`) with hybrid ports on both ends: `∂S/∂ε′`, `∂S/∂ε″`
//!    of the slab (which fills both port faces) and `∂S/∂(slab height)`
//!    (the `y = 0.5` interface moves on the faces and through the volume).
//! 2. **Straight shielded microstrip section** (the #817 face, `ε_r = 4.4`,
//!    `w = h = 1`, `8h × 5h` box, extruded): `∂S/∂ε_r`, `∂S/∂ε″`, the strip
//!    width `w` and the substrate height `h` (lossless and `tan δ = 0.02`).
//! 3. **Width step** (`w₁ = 1 → w₂ = 2`): `∂S/∂ε_r`, `∂S/∂ε″` and `∂S/∂w₁`
//!    (the narrow strip widens on port 1's face and through the narrow
//!    section; the wide strip's edges stay put).
//! 4. **Degenerate pair** (homogeneous two-strip stripline, the #817 even /
//!    odd TEM pair): a design that touches the face is the typed
//!    [`SSensitivityError::DegenerateCluster`] error.
//!
//! Every gate prints an `FD |` row, every tripwire a `MUTATION |` row and
//! every parity check a `PARITY |` row, which the PR's tables collect.
//!
//! Run:
//!
//! ```sh
//! cargo test -p geode-core --release --test s_matrix_sensitivity_hybrid -- --nocapture
//! ```

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::analytic::microstrip::{ShieldedStripFace, StripFaceMesh, StripMeshOpts};
use geode_core::analytic::port_mode_sensitivity::{
    FaceDesign, FaceParamKind, strip_face_groups,
};
use geode_core::analytic::port_modes::HybridPecMasks;
use geode_core::analytic::waveguide::TriMesh;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::ports::{
    HybridPortFace, HybridWavePort, HybridWavePortOpts, WavePortSpec,
    extruded_rect_waveguide_mesh, solve_wave_port_spec_sweep_with_mode, strip_line_section,
};
use geode_core::driven::s_sensitivity::{
    MaterialDesign, OperatorSymmetry, SDesign, SNetwork, SParam, SSensitivityError,
    SSensitivityOptions, SSensitivitySweep, SensitivityFault, ShapeDesign,
    s_matrix_sensitivity_sweep, s_matrix_vjp,
};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, ElementOrder, SolverMode};
use geode_core::mesh::{ExtrudedEdge, TetMesh, extrude_tri_mesh};
use geode_core::shape::apply_node_motion;
use geode_core::testing::TestBackend;

type B = TestBackend;

/// The adjoint-vs-FD bar on these smooth fixtures (Epic #841 validation bar).
const TOL: f64 = 1e-4;
/// A tripwire must miss FD by at least 10× the bar.
const MUTATION_MISS: f64 = 10.0 * TOL;
/// The forward-parity bar (the #853 drift guard).
const PARITY_TOL: f64 = 1e-12;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn quiet() -> HybridWavePortOpts {
    HybridWavePortOpts {
        accuracy: None,
        ..Default::default()
    }
}

fn centroid(mesh: &TetMesh, t: usize) -> [f64; 3] {
    let mut c = [0.0; 3];
    for &v in &mesh.tets[t] {
        for (ci, x) in c.iter_mut().zip(mesh.nodes[v as usize]) {
            *ci += 0.25 * x;
        }
    }
    c
}

/// `max_kp |adj − fd| / max_kp |fd|`.
fn rel_err(adj: &[c64], fd: &[c64]) -> f64 {
    let scale = fd.iter().map(|z| z.norm()).fold(0.0, f64::max);
    let err = adj
        .iter()
        .zip(fd)
        .map(|(a, f)| (a - f).norm())
        .fold(0.0, f64::max);
    err / scale.max(1e-300)
}

fn label(sw: &SSensitivitySweep, i: usize, shape_names: &[&str]) -> String {
    match sw.params[i] {
        SParam::EpsPrime { region } => format!("eps'[{region}]"),
        SParam::EpsDoublePrime { region } => format!("eps''[{region}]"),
        SParam::Shape { column } => shape_names
            .get(column)
            .map_or_else(|| format!("shape[{column}]"), |s| (*s).to_string()),
    }
}

/// The forward-parity guard: the sensitivity's own S against the public
/// hybrid spec sweep's, every ω.
fn assert_forward_parity(spec: &str, sw: &SSensitivitySweep, base: &[Vec<c64>]) {
    assert_eq!(sw.points.len(), base.len(), "{spec}: ω count");
    let mut worst = 0.0_f64;
    for (pt, s) in sw.points.iter().zip(base) {
        worst = worst.max(rel_err(&pt.s, s));
    }
    eprintln!("PARITY | {spec} | S vs the public hybrid spec sweep | rel = {worst:.2e}");
    assert!(
        worst <= PARITY_TOL,
        "{spec}: the sensitivity forward drifted from the public sweep (rel {worst:.3e})"
    );
}

/// Central FD (step `h`) of `fwd(param, step)` for every parameter, after
/// the parity check; returns the worst relative error per parameter.
fn fd_check<F>(spec: &str, sw: &SSensitivitySweep, names: &[&str], h: f64, fwd: F) -> Vec<f64>
where
    F: Fn(SParam, f64) -> Vec<Vec<c64>>,
{
    let base = fwd(sw.params[0], 0.0);
    assert_forward_parity(spec, sw, &base);
    (0..sw.params.len())
        .map(|i| {
            let plus = fwd(sw.params[i], h);
            let minus = fwd(sw.params[i], -h);
            let mut worst = 0.0_f64;
            for (w, pt) in sw.points.iter().enumerate() {
                let fd: Vec<c64> = plus[w]
                    .iter()
                    .zip(&minus[w])
                    .map(|(p, m)| (p - m) / (2.0 * h))
                    .collect();
                worst = worst.max(rel_err(&pt.ds[i], &fd));
            }
            eprintln!(
                "FD | {spec} | {} | rel_err = {worst:.3e}",
                label(sw, i, names)
            );
            worst
        })
        .collect()
}

/// `|∂S_qp − ∂S_pq| / max|∂S|`, worst over parameters and ω.
fn gradient_reciprocity(sw: &SSensitivitySweep) -> f64 {
    let mut worst = 0.0_f64;
    for pt in &sw.points {
        let n = pt.n_ports;
        for ds in &pt.ds {
            let scale = ds.iter().map(|z| z.norm()).fold(0.0, f64::max);
            for q in 0..n {
                for p in 0..n {
                    worst = worst.max((ds[q * n + p] - ds[p * n + q]).norm() / scale);
                }
            }
        }
    }
    worst
}

fn faulted(fault: SensitivityFault) -> SSensitivityOptions {
    SSensitivityOptions {
        fault: Some(fault),
        ..SSensitivityOptions::default()
    }
}

/// `eps` with material parameter `prm` shifted by `h` on its region
/// (`ε = ε′ − jε″`).
fn shift_eps(eps: &[c64], regions: &[Option<usize>], prm: SParam, h: f64) -> Vec<c64> {
    let (region, d) = match prm {
        SParam::EpsPrime { region } => (region, c64::new(h, 0.0)),
        SParam::EpsDoublePrime { region } => (region, c64::new(0.0, -h)),
        SParam::Shape { .. } => return eps.to_vec(),
    };
    eps.iter()
        .zip(regions)
        .map(|(&e, r)| if *r == Some(region) { e + d } else { e })
        .collect()
}

/// A design point of a two-hybrid-port fixture: everything the public
/// forward needs to be re-run at a perturbed `(ε, X)`.
struct Fixture {
    name: &'static str,
    mesh: TetMesh,
    eps: Vec<c64>,
    pec: Vec<bool>,
    port_faces: [Vec<[u32; 3]>; 2],
    /// Build the faces with the volume PEC mask (interior strips).
    with_pec: bool,
    regions: Vec<Option<usize>>,
    shape: Option<ShapeDesign>,
    shape_names: Vec<&'static str>,
    omegas: Vec<f64>,
    /// The FD step.
    h: f64,
}

impl Fixture {
    fn ports(&self, mesh: &TetMesh, eps: &[c64]) -> Vec<WavePortSpec> {
        let lossy = eps.iter().any(|e| e.im != 0.0);
        let edges = mesh.edges();
        self.port_faces
            .iter()
            .map(|faces| {
                let mut f = if lossy {
                    HybridPortFace::from_volume_lossy(mesh, faces, eps).unwrap()
                } else {
                    let re: Vec<f64> = eps.iter().map(|e| e.re).collect();
                    HybridPortFace::from_volume(mesh, faces, &re).unwrap()
                };
                if self.with_pec {
                    f = f.with_interior_pec(&edges, &self.pec).unwrap();
                }
                WavePortSpec::from(HybridWavePort::new(f, vec![c64::new(1.0, 0.0)]).with_opts(quiet()))
            })
            .collect()
    }

    fn design(&self) -> SDesign {
        SDesign {
            material: Some(
                MaterialDesign::from_regions(self.regions.clone(), vec!["design".into()]).unwrap(),
            ),
            shape: self.shape.clone(),
            ..SDesign::default()
        }
    }

    fn sens_with(&self, design: &SDesign, o: &SSensitivityOptions) -> SSensitivitySweep {
        let specs = self.ports(&self.mesh, &self.eps);
        let bcs = DrivenBcs {
            pec_interior_mask: &self.pec,
        };
        let space = HcurlSpace::build(&self.mesh, ElementOrder::P1);
        let net = SNetwork {
            space: &space,
            mesh: &self.mesh,
            materials: DrivenMaterials::Scalar(&self.eps),
            sigma_tet: None,
            bcs: &bcs,
            lumped: &[],
            wave: &specs,
            surfaces: &[],
        };
        s_matrix_sensitivity_sweep::<B>(&net, &self.omegas, design, o, &device()).unwrap()
    }

    fn sens(&self, o: &SSensitivityOptions) -> SSensitivitySweep {
        self.sens_with(&self.design(), o)
    }

    /// The public forward at the design point moved by `h` along `prm`.
    fn forward(&self, prm: SParam, h: f64) -> Vec<Vec<c64>> {
        let eps = shift_eps(&self.eps, &self.regions, prm, h);
        let mesh = match (prm, &self.shape) {
            (SParam::Shape { column }, Some(s)) => apply_node_motion(&self.mesh, s.column(column), h),
            _ => self.mesh.clone(),
        };
        let specs = self.ports(&mesh, &eps);
        solve_wave_port_spec_sweep_with_mode::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &self.pec,
            },
            &specs,
            &[],
            &self.omegas,
            SolverMode::Direct,
            &device(),
        )
        .unwrap()
        .points
        .into_iter()
        .map(|p| p.s)
        .collect()
    }

    /// FD-check `sw` against the public forward; returns per-parameter
    /// errors.
    fn fd(&self, spec: &str, sw: &SSensitivitySweep) -> Vec<f64> {
        fd_check(spec, sw, &self.shape_names, self.h, |prm, h| {
            self.forward(prm, h)
        })
    }
}

// ---------------------------------------------------------------------------
// 1. Slab-loaded guide
// ---------------------------------------------------------------------------

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;

/// The slab guide: the slab (`y < 0.5`, `ε_slab`) is the design region, so it
/// fills both hybrid port faces; the shape column moves the `y = 0.5`
/// interface (a hat on that node row, faces included).
fn slab_fixture(eps_slab: c64) -> Fixture {
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, B_DIM, LEN);
    let mesh = g.mesh.clone();
    let regions: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| (centroid(&mesh, t)[1] < 0.5).then_some(0))
        .collect();
    let eps: Vec<c64> = regions
        .iter()
        .map(|r| if r.is_some() { eps_slab } else { c64::new(1.0, 0.0) })
        .collect();
    let col: Vec<[f64; 3]> = mesh
        .nodes
        .iter()
        .map(|p| {
            if (p[1] - 0.5).abs() < 1e-9 {
                [0.0, 1.0, 0.0]
            } else {
                [0.0; 3]
            }
        })
        .collect();
    Fixture {
        name: "slab guide",
        pec: g.pec_interior_mask(),
        port_faces: [g.port1_faces.clone(), g.port2_faces.clone()],
        with_pec: false,
        mesh,
        eps,
        regions,
        shape: Some(ShapeDesign::from_columns(vec![col], vec!["slab height".into()]).unwrap()),
        shape_names: vec!["slab height"],
        omegas: vec![1.8, 2.1],
        h: 1e-5,
    }
}

/// **Slab guide, lossless**: `∂S/∂ε′` of the slab that fills both port faces
/// and `∂S/∂(slab height)` with the faces moving, against FD through the
/// public hybrid spec sweep (faces rebuilt at every perturbed point), plus
/// reciprocity. The `ε″` column is checked on the lossy slab below (central
/// FD straddles the passive-side kink at `ε″ = 0`).
#[test]
fn slab_guide_material_and_height_on_the_port_faces_match_fd() {
    let fx = slab_fixture(c64::new(2.25, 0.0));
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(fx.regions.clone(), vec!["slab".into()])
                .unwrap()
                .with_components(true, false),
        ),
        shape: fx.shape.clone(),
        ..SDesign::default()
    };
    let sw = fx.sens_with(&design, &SSensitivityOptions::default());
    for e in fx.fd(fx.name, &sw) {
        assert!(e <= TOL, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!("RECIPROCITY | {} | max |∂S_qp − ∂S_pq|/max|∂S| = {rec:.2e}", fx.name);
    assert!(rec < 1e-8);
}

/// **Slab guide, lossy** (`ε = 2.25 − 0.05j`, the complex-symmetric face
/// path): `∂S/∂ε′`, `∂S/∂ε″` and the slab height.
#[test]
fn lossy_slab_guide_matches_fd() {
    let fx = slab_fixture(c64::new(2.25, -0.05));
    let sw = fx.sens(&SSensitivityOptions::default());
    for e in fx.fd("lossy slab guide", &sw) {
        assert!(e <= TOL, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!("RECIPROCITY | lossy slab guide | {rec:.2e}");
    assert!(rec < 1e-8);
}

/// The three new term families each carry a tripwire: dropping the
/// mode-shape term, the normalization term or `∂y = ∂β` must miss FD by
/// more than 10× the bar on **every** parameter (material and shape).
#[test]
fn hybrid_term_mutations_are_caught_by_fd() {
    let fx = slab_fixture(c64::new(2.25, -0.05));
    for fault in [
        SensitivityFault::DropModeShape,
        SensitivityFault::DropModeNormalization,
        SensitivityFault::DropModeAdmittance,
    ] {
        let sw = fx.sens(&faulted(fault));
        let errs = fx.fd(&format!("lossy slab guide, mutant {fault:?}"), &sw);
        let worst = errs.iter().copied().fold(0.0, f64::max);
        let per: Vec<String> = errs.iter().map(|e| format!("{e:.2e}")).collect();
        eprintln!("MUTATION | {fault:?} | lossy slab guide | worst rel_err = {worst:.3e} | per param {per:?}");
        let least = errs.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(
            least > MUTATION_MISS,
            "{fault:?} not caught on every parameter: {per:?}"
        );
    }
}

/// VJP of `g = Σ_ω Σ_qp w_qp |S_qp|²` equals the JVP contraction, and the
/// General (transpose-solve) path equals the reciprocity shortcut, on a
/// design that touches both hybrid faces.
#[test]
fn hybrid_vjp_equals_jvp_and_general_equals_shortcut() {
    let fx = slab_fixture(c64::new(2.25, -0.05));
    let sw = fx.sens(&SSensitivityOptions::default());
    let n = sw.points[0].n_ports;
    let weights: Vec<f64> = (0..n * n).map(|k| 1.0 + 0.37 * k as f64).collect();
    let cots: Vec<Vec<c64>> = sw
        .points
        .iter()
        .map(|pt| pt.s.iter().zip(&weights).map(|(s, &w)| s.conj() * w).collect())
        .collect();
    let jvp = sw.contract(&cots);
    let specs = fx.ports(&fx.mesh, &fx.eps);
    let bcs = DrivenBcs {
        pec_interior_mask: &fx.pec,
    };
    let space = HcurlSpace::build(&fx.mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh: &fx.mesh,
        materials: DrivenMaterials::Scalar(&fx.eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &specs,
        surfaces: &[],
    };
    let vjp = s_matrix_vjp::<B, _>(
        &net,
        &fx.omegas,
        &fx.design(),
        &SSensitivityOptions::default(),
        |_, s| {
            let g = s.iter().zip(&weights).map(|(s, &w)| w * s.norm_sqr()).sum();
            (g, s.iter().zip(&weights).map(|(s, &w)| s.conj() * w).collect())
        },
        &device(),
    )
    .unwrap();
    let scale = jvp.iter().map(|v| v.abs()).fold(0.0, f64::max);
    let worst = jvp
        .iter()
        .zip(&vjp.grad)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max)
        / scale;
    eprintln!("VJP | lossy slab guide | max |vjp − jvp|/max|jvp| = {worst:.2e}");
    assert!(worst < 1e-10, "VJP vs JVP {worst:e}");

    let general = fx.sens(&SSensitivityOptions {
        symmetry: OperatorSymmetry::General,
        ..SSensitivityOptions::default()
    });
    let mut worst_g = 0.0_f64;
    for (a, b) in sw.points.iter().zip(&general.points) {
        for (da, db) in a.ds.iter().zip(&b.ds) {
            worst_g = worst_g.max(rel_err(db, da));
        }
        assert_eq!(b.n_factorizations, 1);
    }
    eprintln!("GENERAL | lossy slab guide | max rel |General − shortcut| = {worst_g:.2e}");
    assert!(worst_g < 1e-9);
}
