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
use geode_core::analytic::port_mode_sensitivity::{FaceDesign, FaceParamKind, strip_face_groups};
use geode_core::analytic::port_modes::HybridPecMasks;
use geode_core::analytic::waveguide::TriMesh;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::ports::{
    HybridPortFace, HybridWavePort, HybridWavePortOpts, WavePortSpec, extruded_rect_waveguide_mesh,
    solve_wave_port_spec_sweep_with_mode, strip_line_section,
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
/// The tighter bar of the strip-line fixtures (measured ≤ 1e-7). On their
/// quasi-TEM modes the normalization term of a *material* parameter is
/// `O(E_z²)` (as in #863's `−μ∂B` note): dropping it moves `∂S/∂ε` by only
/// ~9e-4, which a 1e-4 bar would let through by less than 10×, so these
/// fixtures gate at 1e-6 and every tripwire must miss by 10× that.
const STRIP_TOL: f64 = 1e-6;
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
    /// The FD bar of this fixture.
    tol: f64,
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
                WavePortSpec::from(
                    HybridWavePort::new(f, vec![c64::new(1.0, 0.0)]).with_opts(quiet()),
                )
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
            (SParam::Shape { column }, Some(s)) => {
                apply_node_motion(&self.mesh, s.column(column), h)
            }
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
/// fills both hybrid port faces; the shape columns move the `y = 0.5`
/// interface (a hat on that node row, faces included) and shift port 1's
/// plane rigidly along its normal.
fn slab_fixture(eps_slab: c64) -> Fixture {
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, B_DIM, LEN);
    let mesh = g.mesh.clone();
    let regions: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| (centroid(&mesh, t)[1] < 0.5).then_some(0))
        .collect();
    let eps: Vec<c64> = regions
        .iter()
        .map(|r| {
            if r.is_some() {
                eps_slab
            } else {
                c64::new(1.0, 0.0)
            }
        })
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
    // A rigid shift of port 1's plane along its normal (the first cell
    // layer stretches): the 2-D problem is unchanged, the volume term is not.
    let shift: Vec<[f64; 3]> = mesh
        .nodes
        .iter()
        .map(|p| {
            if p[2] < 1e-9 {
                [0.0, 0.0, 1.0]
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
        shape: Some(
            ShapeDesign::from_columns(
                vec![col, shift],
                vec!["slab height".into(), "port-1 plane shift".into()],
            )
            .unwrap(),
        ),
        shape_names: vec!["slab height", "port-1 plane shift"],
        omegas: vec![1.8, 2.1],
        h: 1e-5,
        tol: TOL,
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
        assert!(e <= fx.tol, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!(
        "RECIPROCITY | {} | max |∂S_qp − ∂S_pq|/max|∂S| = {rec:.2e}",
        fx.name
    );
    assert!(rec < 1e-8);
}

/// **Slab guide, lossy** (`ε = 2.25 − 0.05j`, the complex-symmetric face
/// path): `∂S/∂ε′`, `∂S/∂ε″` and the slab height.
#[test]
fn lossy_slab_guide_matches_fd() {
    let fx = slab_fixture(c64::new(2.25, -0.05));
    let sw = fx.sens(&SSensitivityOptions::default());
    for e in fx.fd("lossy slab guide", &sw) {
        assert!(e <= fx.tol, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!("RECIPROCITY | lossy slab guide | {rec:.2e}");
    assert!(rec < 1e-8);
}

/// The three new term families each carry a tripwire: dropping the
/// mode-shape term, the normalization term or `∂y = ∂β` must miss FD by
/// more than 10× the fixture's bar on **every** parameter that touches a
/// port face (material and shape), on the lossy slab guide and the lossy
/// microstrip.
#[test]
fn hybrid_term_mutations_are_caught_by_fd() {
    let slab = slab_fixture(c64::new(2.25, -0.05));
    let strip = microstrip_fixture(0.02);
    for (fx, skip) in [(&slab, Some("port-1 plane shift")), (&strip, None)] {
        for fault in [
            SensitivityFault::DropModeShape,
            SensitivityFault::DropModeNormalization,
            SensitivityFault::DropModeAdmittance,
        ] {
            let sw = fx.sens(&faulted(fault));
            let errs = fx.fd(&format!("{}, mutant {fault:?}", fx.name), &sw);
            let worst = errs.iter().copied().fold(0.0, f64::max);
            let per: Vec<String> = errs.iter().map(|e| format!("{e:.2e}")).collect();
            eprintln!(
                "MUTATION | {fault:?} | {} | worst rel_err = {worst:.3e} | per param {per:?}",
                fx.name
            );
            // The plane shift does not touch the 2-D problem: no port-mode term
            // to drop, so its FD stays green under every mutant.
            let mut caught = Vec::new();
            for (e, p) in errs.iter().zip(&sw.params) {
                match (p, skip) {
                    (SParam::Shape { column }, Some(name)) if fx.shape_names[*column] == name => {
                        assert!(*e <= fx.tol, "{fault:?} broke the untouched {name}: {e:e}");
                    }
                    _ => caught.push(*e),
                }
            }
            let least = caught.iter().copied().fold(f64::INFINITY, f64::min);
            assert!(
                least > 10.0 * fx.tol,
                "{fault:?} not caught by 10× the bar on every parameter: {per:?}"
            );
        }
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
        .map(|pt| {
            pt.s.iter()
                .zip(&weights)
                .map(|(s, &w)| s.conj() * w)
                .collect()
        })
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
            (
                g,
                s.iter().zip(&weights).map(|(s, &w)| s.conj() * w).collect(),
            )
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

// ---------------------------------------------------------------------------
// 2–3. Microstrip sections
// ---------------------------------------------------------------------------

fn strip_mesh_opts() -> StripMeshOpts {
    StripMeshOpts {
        h_min: 0.2,
        h_max: 1.0,
        ratio: 1.5,
        mirror_symmetric: true,
    }
}

/// A 2-D face motion bound to named node groups ([`FaceDesign::push_group_motion`]).
fn face_motion(
    mesh: &TriMesh,
    groups: &geode_core::analytic::port_mode_sensitivity::FaceGroups,
    moves: &[(&str, [f64; 2])],
    pinned: &[&str],
) -> Vec<[f64; 2]> {
    let mut d = FaceDesign::new(mesh);
    d.push_group_motion(mesh, groups, "m", moves, pinned)
        .unwrap();
    match &d.params()[0].kind {
        FaceParamKind::Shape { velocity } => velocity.clone(),
        FaceParamKind::Material { .. } => unreachable!("a shape parameter"),
    }
}

/// The 3-D column of an extruded section that moves every layer by the 2-D
/// face motion `v` (so the port faces move exactly as the volume does).
fn extrude_motion(n_face_nodes: usize, n_nodes: usize, v: &[[f64; 2]]) -> Vec<[f64; 3]> {
    (0..n_nodes)
        .map(|i| {
            let w = v[i % n_face_nodes];
            [w[0], w[1], 0.0]
        })
        .collect()
}

/// A two-hybrid-port strip-line fixture from an extruded face: the
/// substrate (`y < h`) is the design region; `motions` are named 2-D face
/// motions extruded through the section.
#[allow(clippy::too_many_arguments)]
fn strip_fixture(
    name: &'static str,
    face: &StripFaceMesh,
    ex: geode_core::mesh::ExtrudedTriMesh,
    pec: Vec<bool>,
    sub_eps: c64,
    sup_eps: f64,
    h: f64,
    motions: Vec<(&'static str, Vec<[f64; 2]>)>,
    omegas: Vec<f64>,
) -> Fixture {
    let mesh = ex.mesh.clone();
    let sub_tri: Vec<bool> = face
        .mesh
        .tris
        .iter()
        .map(|t| {
            t.iter()
                .map(|&v| face.mesh.nodes[v as usize][1])
                .sum::<f64>()
                / 3.0
                < h
        })
        .collect();
    let regions: Vec<Option<usize>> = ex
        .tet_source_tri
        .iter()
        .map(|&t| sub_tri[t].then_some(0))
        .collect();
    let eps: Vec<c64> = regions
        .iter()
        .map(|r| {
            if r.is_some() {
                sub_eps
            } else {
                c64::new(sup_eps, 0.0)
            }
        })
        .collect();
    let (names, cols): (Vec<&'static str>, Vec<Vec<[f64; 3]>>) = motions
        .into_iter()
        .map(|(n, v)| (n, extrude_motion(ex.n_face_nodes, mesh.n_nodes(), &v)))
        .unzip();
    Fixture {
        name,
        port_faces: [ex.port1_faces.clone(), ex.port2_faces.clone()],
        with_pec: true,
        pec,
        eps,
        regions,
        shape: Some(
            ShapeDesign::from_columns(cols, names.iter().map(|s| (*s).to_string()).collect())
                .unwrap(),
        ),
        shape_names: names,
        mesh,
        omegas,
        h: 1e-5,
        tol: STRIP_TOL,
    }
}

/// The straight shielded microstrip section (`ε_r = 4.4`, `w = h = 1`,
/// `8h × 5h` box, `L = 4h`, 6 slabs) with the strip width and the substrate
/// height as named-group face motions extruded through the section.
fn microstrip_fixture(tan_d: f64) -> Fixture {
    let spec = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.0, 4.4);
    let face = spec.build(&strip_mesh_opts());
    let groups = strip_face_groups(&spec, &face);
    let w = face_motion(
        &face.mesh,
        &groups,
        &[("strip_0_left", [-0.5, 0.0]), ("strip_0_right", [0.5, 0.0])],
        &["shield"],
    );
    let hgt = face_motion(
        &face.mesh,
        &groups,
        &[("interface", [0.0, 1.0])],
        &["ground", "lid"],
    );
    let sec = strip_line_section(&face, 6, 4.0);
    strip_fixture(
        if tan_d == 0.0 {
            "microstrip section"
        } else {
            "lossy microstrip section"
        },
        &face,
        sec.extruded,
        sec.pec_interior_mask,
        c64::new(4.4, -4.4 * tan_d),
        1.0,
        1.0,
        vec![("strip width w", w), ("substrate height h", hgt)],
        vec![0.15, 0.25],
    )
}

/// **Straight microstrip section**: `∂S/∂ε_r` of the substrate (which fills
/// both port faces), the strip width `w` and the substrate height `h` (the
/// port faces move with the volume), against central FD through the public
/// hybrid spec sweep; plus reciprocity of ∂S.
#[test]
fn microstrip_section_eps_width_height_match_fd() {
    let fx = microstrip_fixture(0.0);
    let design = SDesign {
        material: Some(
            MaterialDesign::from_regions(fx.regions.clone(), vec!["substrate".into()])
                .unwrap()
                .with_components(true, false),
        ),
        shape: fx.shape.clone(),
        ..SDesign::default()
    };
    let sw = fx.sens_with(&design, &SSensitivityOptions::default());
    eprintln!(
        "microstrip section: {} tets, β = {:?}, |S21| = {:.6}",
        fx.mesh.n_tets(),
        sw.points[0].beta,
        sw.points[0].s[2].norm()
    );
    for e in fx.fd(fx.name, &sw) {
        assert!(e <= fx.tol, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!("RECIPROCITY | {} | {rec:.2e}", fx.name);
    assert!(rec < 1e-8);
}

/// **Lossy microstrip** (`tan δ = 0.02`, the complex-symmetric face path):
/// `∂S/∂ε′`, `∂S/∂ε″`, `w`, `h`. The tan δ derivative at fixed `ε′` is the
/// chain `∂/∂tanδ = ε′·∂/∂ε″`, checked here against a direct central FD in
/// `tan δ` through the public forward.
#[test]
fn lossy_microstrip_section_matches_fd_including_tan_delta() {
    let tan_d = 0.02;
    let fx = microstrip_fixture(tan_d);
    let sw = fx.sens(&SSensitivityOptions::default());
    for e in fx.fd(fx.name, &sw) {
        assert!(e <= fx.tol, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!("RECIPROCITY | {} | {rec:.2e}", fx.name);
    assert!(rec < 1e-8);
    // ∂/∂tanδ at fixed ε′ = 4.4: ε″ = ε′ tanδ.
    let i_dp = sw
        .params
        .iter()
        .position(|p| matches!(p, SParam::EpsDoublePrime { .. }))
        .unwrap();
    let h = 1e-6;
    let at_tan = |t: f64| -> Vec<Vec<c64>> {
        let eps: Vec<c64> = fx
            .eps
            .iter()
            .zip(&fx.regions)
            .map(|(&e, r)| {
                if r.is_some() {
                    c64::new(4.4, -4.4 * t)
                } else {
                    e
                }
            })
            .collect();
        let specs = fx.ports(&fx.mesh, &eps);
        solve_wave_port_spec_sweep_with_mode::<B>(
            &fx.mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &fx.pec,
            },
            &specs,
            &[],
            &fx.omegas,
            SolverMode::Direct,
            &device(),
        )
        .unwrap()
        .points
        .into_iter()
        .map(|p| p.s)
        .collect()
    };
    let (plus, minus) = (at_tan(tan_d + h), at_tan(tan_d - h));
    let mut worst = 0.0_f64;
    for (w, pt) in sw.points.iter().enumerate() {
        let fd: Vec<c64> = plus[w]
            .iter()
            .zip(&minus[w])
            .map(|(p, m)| (p - m) / (2.0 * h))
            .collect();
        let adj: Vec<c64> = pt.ds[i_dp].iter().map(|d| d * 4.4).collect();
        worst = worst.max(rel_err(&adj, &fd));
    }
    eprintln!(
        "FD | {} | tan_delta (= ε′·∂/∂ε″) | rel_err = {worst:.3e}",
        fx.name
    );
    assert!(worst <= fx.tol);
}

/// The width-step section of #817 (`w₁ = 1` for `z < L/2`, `w₂ = 2` beyond;
/// `L = 8h`, 8 slabs): the PEC set is z-dependent, so the two port faces
/// carry different strips. `w₁` moves the narrow strip's edges on port 1's
/// face and through the narrow section (the wide strip's edges stay put);
/// `h` moves the substrate interface everywhere.
fn width_step_fixture(tan_d: f64) -> Fixture {
    let (w1, w2, hh, len) = (1.0, 2.0, 1.0, 8.0);
    let spec = ShieldedStripFace {
        box_width: 8.0,
        box_height: 5.0,
        h: hh,
        strips: vec![[-0.5 * w2, -0.5 * w1], [0.5 * w1, 0.5 * w2]],
        thickness: 0.0,
        eps_below: 4.4,
        eps_above: 1.0,
    };
    let face = spec.build(&strip_mesh_opts());
    let ex = extrude_tri_mesh(&face.mesh, 8, len);
    let rim = HybridPecMasks::from_mesh(&face.mesh, None).pec_edges;
    let edges2 = face.mesh.edges();
    let mut rim_node = vec![false; face.mesh.n_nodes()];
    for (e, &r) in edges2.iter().zip(&rim) {
        if r {
            rim_node[e[0] as usize] = true;
            rim_node[e[1] as usize] = true;
        }
    }
    let tol = 1e-9;
    let half = |z: f64| {
        if z < 0.5 * len - tol {
            0.5 * w1
        } else {
            0.5 * w2
        }
    };
    let in_strip = |n: u32, z: f64| {
        let p = face.mesh.nodes[n as usize];
        (p[1] - hh).abs() < tol && p[0].abs() <= half(z) + tol
    };
    let pec = ex.pec_interior_mask_by(|k| match k {
        ExtrudedEdge::Horizontal { edge, layer } => {
            let [a, b] = edges2[edge];
            rim[edge] || (in_strip(a, ex.z[layer]) && in_strip(b, ex.z[layer]))
        }
        ExtrudedEdge::Diagonal { edge, slab } => {
            let [a, b] = edges2[edge];
            let zm = 0.5 * (ex.z[slab] + ex.z[slab + 1]);
            rim[edge] || (in_strip(a, zm) && in_strip(b, zm))
        }
        ExtrudedEdge::Vertical { node, slab } => {
            let zm = 0.5 * (ex.z[slab] + ex.z[slab + 1]);
            rim_node[node] || in_strip(node as u32, zm)
        }
    });
    let mut groups = strip_face_groups(&spec, &face);
    let on_line = |f: &dyn Fn(f64) -> bool| -> Vec<u32> {
        (0..face.mesh.n_nodes() as u32)
            .filter(|&k| {
                let p = face.mesh.nodes[k as usize];
                (p[1] - hh).abs() < tol && f(p[0])
            })
            .collect()
    };
    groups.nodes.insert(
        "narrow_left".into(),
        on_line(&|x| x < -tol && x >= -0.5 * w1 - tol),
    );
    groups.nodes.insert(
        "narrow_right".into(),
        on_line(&|x| x > tol && x <= 0.5 * w1 + tol),
    );
    groups
        .nodes
        .insert("wide_edges".into(), on_line(&|x| x.abs() >= 0.5 * w2 - tol));
    let w1_motion = face_motion(
        &face.mesh,
        &groups,
        &[("narrow_left", [-0.5, 0.0]), ("narrow_right", [0.5, 0.0])],
        &["shield", "wide_edges"],
    );
    let hgt = face_motion(
        &face.mesh,
        &groups,
        &[("interface", [0.0, 1.0])],
        &["ground", "lid"],
    );
    strip_fixture(
        if tan_d == 0.0 {
            "width step"
        } else {
            "lossy width step"
        },
        &face,
        ex,
        pec,
        c64::new(4.4, -4.4 * tan_d),
        1.0,
        hh,
        vec![("narrow width w1", w1_motion), ("substrate height h", hgt)],
        vec![0.1, 0.2],
    )
}

/// **Width step**: `∂S/∂ε′`, `∂S/∂ε″` (lossy substrate, `tan δ = 0.02`),
/// `∂S/∂w₁` and `∂S/∂h` against FD, reciprocity included — the two port
/// faces see different strips, so `S11 ≠ S22` and every entry moves.
#[test]
fn width_step_matches_fd() {
    let fx = width_step_fixture(0.02);
    let sw = fx.sens(&SSensitivityOptions::default());
    let pt = &sw.points[0];
    eprintln!(
        "width step: {} tets, |S11| = {:.4e}, |S22| = {:.4e}, |S21| = {:.6}",
        fx.mesh.n_tets(),
        pt.s[0].norm(),
        pt.s[3].norm(),
        pt.s[2].norm()
    );
    for e in fx.fd(fx.name, &sw) {
        assert!(e <= fx.tol, "rel_err {e}");
    }
    let rec = gradient_reciprocity(&sw);
    eprintln!("RECIPROCITY | {} | {rec:.2e}", fx.name);
    assert!(rec < 1e-8);
}

// ---------------------------------------------------------------------------
// 4. Degenerate cluster
// ---------------------------------------------------------------------------

/// **Degenerate coupled-line cluster** (the #817 homogeneous two-strip
/// stripline: an exactly degenerate even / odd TEM pair). With θ off the
/// port faces the ports are untouched and the gradient is admitted; a
/// design that touches a face (a material block over one strip — symmetry
/// breaking — or the strip width) is the typed `DegenerateCluster` error,
/// never a basis-dependent gradient.
#[test]
fn degenerate_pair_on_a_touched_face_is_a_typed_error() {
    let (w, gap, eps) = (1.0, 0.5, 2.2);
    let spec = ShieldedStripFace {
        box_width: 10.0,
        box_height: 4.0,
        h: 2.0,
        strips: vec![[-0.5 * gap - w, -0.5 * gap], [0.5 * gap, 0.5 * gap + w]],
        thickness: 0.0,
        eps_below: eps,
        eps_above: eps,
    };
    let face = spec.build(&strip_mesh_opts());
    let sec = strip_line_section(&face, 4, 3.0);
    let mesh = &sec.extruded.mesh;
    let eps_c: Vec<c64> = sec.eps_tet.iter().map(|&e| c64::new(e, 0.0)).collect();
    let ports = sec.hybrid_ports(2, quiet()).unwrap();
    let bcs = DrivenBcs {
        pec_interior_mask: &sec.pec_interior_mask,
    };
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&eps_c),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &[],
        wave: &ports,
        surfaces: &[],
    };
    let run = |regions: Vec<Option<usize>>| {
        let design = SDesign {
            material: Some(MaterialDesign::from_regions(regions, vec!["block".into()]).unwrap()),
            ..SDesign::default()
        };
        s_matrix_sensitivity_sweep::<B>(
            &net,
            &[0.1],
            &design,
            &SSensitivityOptions::default(),
            &device(),
        )
    };
    // Off the faces: admitted (the cluster's modes are θ-independent).
    let len = sec.length();
    let mid: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| {
            let c = centroid(mesh, t);
            (c[2] > 0.3 * len && c[2] < 0.7 * len && c[0] > 0.0).then_some(0)
        })
        .collect();
    let ok = run(mid).expect("an untouched degenerate pair is admitted");
    assert_eq!(ok.points[0].n_ports, 4);
    // On the faces (x > 0 half: symmetry breaking): the typed error.
    let half: Vec<Option<usize>> = (0..mesh.n_tets())
        .map(|t| (centroid(mesh, t)[0] > 0.0).then_some(0))
        .collect();
    let err = run(half).unwrap_err();
    eprintln!("degenerate pair, touched face: {err}");
    match err {
        SSensitivityError::DegenerateCluster { members, hint, .. } => {
            assert_eq!(members.len(), 2);
            assert!(hint.contains("cluster_sensitivity"));
        }
        other => panic!("expected DegenerateCluster, got {other:?}"),
    }
}
