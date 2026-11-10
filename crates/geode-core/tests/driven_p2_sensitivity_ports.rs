//! Central-finite-difference goldens for the **order-generic** driven
//! sensitivities at p=2 (issue #1264, Epic #836 Phase 4): material `∂g/∂ε`
//! ([`driven_material_adjoint_gradient_on_space`]) and shape `∂g/∂X`
//! ([`driven_shape_gradient_on_space`]) through the [`DrivenOperator`] — with
//! lumped ports, Leontovich impedance walls and matched-UPML tensors — each
//! checked against a central difference of the whole p=2 forward pipeline
//! (`assemble_with_space` + `solve_at`).
//!
//! # Fixture
//!
//! A jittered unit cube (PEC plates at `y = 0, 1`, natural side walls at
//! `x = 0, 1`), a uniform lumped port across `z = 0` with `ê = ŷ`, and an
//! end cap at `z = 1` that is a PEC short, a Leontovich good-conductor wall or
//! a matched `Fixed(1)` Silver-Müller wall. Two permittivity regions split at
//! `x = 0.5`; the observable is `g = Σ_i |x_i|²`.
//!
//! # Bar
//!
//! The same bar as the p=1 driven-adjoint goldens and `driven_p2_adjoint.rs`:
//! central FD (`h = 1e-6`), relative error `< 1e-4` (normalised by
//! `|FD| + 1e-3·max|gradient|` so a near-zero component cannot dominate).

mod common;
use common::boundary_plane_faces;

use common::Lcg;
use faer::c64;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::adjoint::{
    DrivenGradientProblem, WAVE_PORT_GRADIENT_FEATURE, driven_material_adjoint_gradient_on_space,
};
use geode_core::driven::ports::{LumpedPort, WavePortSpec};
use geode_core::driven::shape::driven_shape_gradient_on_space;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::{TestBackend, test_device as device};

type B = TestBackend;

const OMEGA: f64 = 1.3;
const FD_H: f64 = 1e-6;
const TOL: f64 = 1e-4;

/// `cube_tet_mesh(n)` with interior and face-interior nodes jittered.
fn jittered(n: usize, seed: u64) -> TetMesh {
    let mut mesh = cube_tet_mesh(n, 1.0);
    let h = 1.0 / n as f64;
    let mut rng = Lcg(seed);
    for p in mesh.nodes.iter_mut() {
        let free: Vec<usize> = (0..3)
            .filter(|&d| p[d] > 1e-12 && p[d] < 1.0 - 1e-12)
            .collect();
        if free.len() >= 2 {
            for d in free {
                p[d] += 0.12 * h * rng.next();
            }
        }
    }
    mesh
}

fn regions(mesh: &TetMesh) -> Vec<usize> {
    mesh.tets
        .iter()
        .map(|t| {
            let cx = t.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
            usize::from(cx > 0.5)
        })
        .collect()
}

fn objective(x: &[c64]) -> (f64, Vec<c64>) {
    (
        x.iter().map(|z| z.norm_sqr()).sum(),
        x.iter().map(|z| z.conj()).collect(),
    )
}

#[derive(Clone, Copy, Debug)]
enum Cap {
    Pec,
    Leontovich,
    SilverMuller,
}

#[derive(Clone, Copy, Debug)]
struct Case {
    name: &'static str,
    port: bool,
    cap: Cap,
    upml: bool,
    source: bool,
}

const CASES: [Case; 5] = [
    Case {
        name: "ports",
        port: true,
        cap: Cap::Pec,
        upml: false,
        source: false,
    },
    Case {
        name: "leontovich",
        port: false,
        cap: Cap::Leontovich,
        upml: false,
        source: true,
    },
    Case {
        name: "upml+ports",
        port: true,
        cap: Cap::Pec,
        upml: true,
        source: false,
    },
    Case {
        name: "ports+leontovich",
        port: true,
        cap: Cap::Leontovich,
        upml: false,
        source: true,
    },
    Case {
        name: "ports+silver-muller+upml",
        port: true,
        cap: Cap::SilverMuller,
        upml: true,
        source: true,
    },
];

/// Everything that does not change when a node moves or ε is perturbed.
struct Fixture {
    case: Case,
    mesh: TetMesh,
    port_faces: Vec<[u32; 3]>,
    cap_faces: Vec<[u32; 3]>,
    mask: Vec<bool>,
    region_of_tet: Vec<usize>,
    eps: Vec<c64>,
    eps_tensor: Vec<[[c64; 3]; 3]>,
    nu_tensor: Vec<[[c64; 3]; 3]>,
    source: CurrentSource,
}

fn fixture(case: Case) -> Fixture {
    let mesh = jittered(2, 0x1264);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let port_faces = boundary_plane_faces(&mesh, 2, 0.0);
    let cap_faces = boundary_plane_faces(&mesh, 2, 1.0);
    let plates = [
        boundary_plane_faces(&mesh, 1, 0.0),
        boundary_plane_faces(&mesh, 1, 1.0),
    ];
    let mut walls: Vec<&[[u32; 3]]> = plates.iter().map(Vec::as_slice).collect();
    if !case.port {
        walls.push(port_faces.as_slice());
    }
    if matches!(case.cap, Cap::Pec) {
        walls.push(cap_faces.as_slice());
    }
    let mask = space.pec_interior_mask(&mesh, &walls).expect("mask");
    let region_of_tet = regions(&mesh);
    let eps_region = [c64::new(2.0, -0.1), c64::new(3.0, -0.05)];
    let eps: Vec<c64> = region_of_tet.iter().map(|&r| eps_region[r]).collect();
    // A diagonal complex Λ (UPML-like) in region 1, identity in region 0;
    // ε = ε_r Λ, ν = Λ⁻¹.
    let a = c64::new(1.2, -0.5);
    let one = c64::new(1.0, 0.0);
    let z = c64::new(0.0, 0.0);
    let lam: Vec<[c64; 3]> = region_of_tet
        .iter()
        .map(|&r| if r == 1 { [one, a, a] } else { [one; 3] })
        .collect();
    let eps_tensor = lam
        .iter()
        .zip(&region_of_tet)
        .map(|(l, &r)| {
            let er = [c64::new(2.0, 0.0), c64::new(3.0, 0.0)][r];
            [[er * l[0], z, z], [z, er * l[1], z], [z, z, er * l[2]]]
        })
        .collect();
    let nu_tensor = lam
        .iter()
        .map(|l| [[one / l[0], z, z], [z, one / l[1], z], [z, z, one / l[2]]])
        .collect();
    let source = if case.source {
        CurrentSource::from_centroids(&mesh, |c| {
            [
                c64::new(0.0, 0.3 * c[2]),
                c64::new(0.5 * c[1], 0.2),
                c64::new(c[0], 0.4 * c[2]),
            ]
        })
    } else {
        CurrentSource {
            j_tet: vec![[z; 3]; mesh.n_tets()],
        }
    };
    Fixture {
        case,
        mesh,
        port_faces,
        cap_faces,
        mask,
        region_of_tet,
        eps,
        eps_tensor,
        nu_tensor,
        source,
    }
}

impl Fixture {
    fn port(&self) -> LumpedPort<'_> {
        LumpedPort {
            faces: &self.port_faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: 1.3,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(1.0, 0.2),
        }
    }

    fn cap_bc(&self) -> Option<SurfaceImpedanceBc<'_>> {
        match self.case.cap {
            Cap::Pec => None,
            Cap::Leontovich => Some(SurfaceImpedanceBc {
                triangles: &self.cap_faces,
                model: SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
            }),
            Cap::SilverMuller => Some(SurfaceImpedanceBc {
                triangles: &self.cap_faces,
                model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
            }),
        }
    }

    /// `g` of the full p=2 forward pipeline on `mesh` with the given
    /// per-tet scalar ε / tensor ε scale.
    fn forward_g(&self, mesh: &TetMesh, eps: &[c64], eps_tensor: &[[[c64; 3]; 3]]) -> f64 {
        let space = HcurlSpace::build(mesh, ElementOrder::P2);
        let port = self.port();
        let ports: Vec<LumpedPort<'_>> = if self.case.port { vec![port] } else { vec![] };
        let surfaces: Vec<SurfaceImpedanceBc<'_>> = self.cap_bc().into_iter().collect();
        let materials = if self.case.upml {
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: eps_tensor,
                nu_tensor: &self.nu_tensor,
            }
        } else {
            DrivenMaterials::Scalar(eps)
        };
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            mesh,
            materials,
            None,
            &DrivenBcs {
                pec_interior_mask: &self.mask,
            },
            &ports,
            &surfaces,
            DrivenSource::Constant(&self.source),
            &device(),
        )
        .expect("forward operator");
        let sol = op.solve_at(OMEGA).expect("forward solve");
        objective(&sol.e_edges).0
    }

    /// Run `f` on the gradient problem of this fixture.
    fn with_problem<R>(&self, f: impl FnOnce(&DrivenGradientProblem<'_>) -> R) -> R {
        let space = HcurlSpace::build(&self.mesh, ElementOrder::P2);
        let port = self.port();
        let ports: Vec<LumpedPort<'_>> = if self.case.port { vec![port] } else { vec![] };
        let surfaces: Vec<SurfaceImpedanceBc<'_>> = self.cap_bc().into_iter().collect();
        let materials = if self.case.upml {
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &self.eps_tensor,
                nu_tensor: &self.nu_tensor,
            }
        } else {
            DrivenMaterials::Scalar(&self.eps)
        };
        let bcs = DrivenBcs {
            pec_interior_mask: &self.mask,
        };
        let problem = DrivenGradientProblem {
            space: &space,
            mesh: &self.mesh,
            materials,
            bcs: &bcs,
            source: &self.source,
            ports: &ports,
            surfaces: &surfaces,
            wave_ports: &[],
            omega: OMEGA,
            excited: None,
        };
        f(&problem)
    }
}

fn check(label: &str, adj: f64, fd: f64, scale: f64) {
    let rel = (adj - fd).abs() / (fd.abs() + 1e-3 * scale);
    eprintln!("{label}: adjoint = {adj:.6e}, FD = {fd:.6e}, rel-err = {rel:.3e}");
    assert!(
        rel < TOL,
        "{label}: adjoint {adj:.6e} vs FD {fd:.6e} (rel-err {rel:.3e} > {TOL:e})"
    );
}

#[test]
fn p2_material_gradient_matches_central_fd_for_every_feature_combination() {
    for case in CASES {
        let fx = fixture(case);
        let n_regions = 2;
        let res = fx.with_problem(|p| {
            driven_material_adjoint_gradient_on_space::<B, _>(
                p,
                &fx.region_of_tet,
                n_regions,
                objective,
                &device(),
            )
            .expect("p2 material gradient")
        });
        assert_eq!(res.n_factorizations, 1, "{}", case.name);
        assert!(res.residual_rel < 1e-8, "{} residual", case.name);
        let scale = res
            .grad_eps_prime
            .iter()
            .chain(&res.grad_eps_dprime)
            .fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!(scale > 0.0, "{}: gradient identically zero", case.name);

        for k in 0..n_regions {
            // ε′_k (resp. the tensor's real scale) and ε″_k perturbations.
            for (part, adj) in [
                ("eps'", res.grad_eps_prime[k]),
                ("eps''", res.grad_eps_dprime[k]),
            ] {
                let g_at = |d: f64| -> f64 {
                    let s = if part == "eps'" {
                        c64::new(d, 0.0)
                    } else {
                        c64::new(0.0, -d)
                    };
                    if case.upml {
                        let t: Vec<[[c64; 3]; 3]> = fx
                            .eps_tensor
                            .iter()
                            .zip(&fx.region_of_tet)
                            .map(|(m, &r)| {
                                // ε_t(s) = (1 + s)·T_t on region k.
                                let f = if r == k {
                                    c64::new(1.0, 0.0) + s
                                } else {
                                    c64::new(1.0, 0.0)
                                };
                                m.map(|row| row.map(|v| v * f))
                            })
                            .collect();
                        fx.forward_g(&fx.mesh, &fx.eps, &t)
                    } else {
                        let e: Vec<c64> = fx
                            .eps
                            .iter()
                            .zip(&fx.region_of_tet)
                            .map(|(&v, &r)| if r == k { v + s } else { v })
                            .collect();
                        fx.forward_g(&fx.mesh, &e, &fx.eps_tensor)
                    }
                };
                let fd = (g_at(FD_H) - g_at(-FD_H)) / (2.0 * FD_H);
                check(&format!("{} region {k} {part}", case.name), adj, fd, scale);
            }
        }
    }
}

#[test]
fn p2_shape_gradient_matches_central_fd_for_every_feature_combination() {
    for case in CASES {
        let fx = fixture(case);
        let res = fx.with_problem(|p| {
            driven_shape_gradient_on_space::<B, _>(p, objective, &device())
                .expect("p2 shape gradient")
        });
        assert_eq!(res.n_factorizations, 1, "{}", case.name);
        assert!(res.residual_rel < 1e-8, "{} residual", case.name);
        let scale = res
            .grad_node
            .iter()
            .flatten()
            .fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!(scale > 0.0, "{}: gradient identically zero", case.name);

        // The interior node, a port-face node, a cap-face node and a plate
        // node: volume, port and wall Dual twins are each exercised.
        let on = |axis: usize, v: f64| {
            (0..fx.mesh.n_nodes())
                .find(|&n| (fx.mesh.nodes[n][axis] - v).abs() < 1e-12)
                .expect("node on plane")
        };
        let interior = (0..fx.mesh.n_nodes())
            .find(|&n| fx.mesh.nodes[n].iter().all(|&c| c > 0.1 && c < 0.9))
            .expect("interior node");
        let nodes = [interior, on(2, 0.0), on(2, 1.0), on(1, 0.0)];
        for &n in &nodes {
            for axis in 0..3 {
                let g_at = |d: f64| -> f64 {
                    let mut m = fx.mesh.clone();
                    m.nodes[n][axis] += d;
                    fx.forward_g(&m, &fx.eps, &fx.eps_tensor)
                };
                let fd = (g_at(FD_H) - g_at(-FD_H)) / (2.0 * FD_H);
                check(
                    &format!("{} node {n} axis {axis}", case.name),
                    res.grad_node[n][axis],
                    fd,
                    scale,
                );
            }
        }
    }
}

/// The wave / hybrid-port fence: a typed error naming `#836 P3b`, never a
/// silent fall-back to another order.
#[test]
fn wave_port_gradient_request_is_fenced_with_a_typed_error() {
    let fx = fixture(CASES[0]);
    let spec: Vec<WavePortSpec> = vec![];
    // An empty slice is accepted …
    assert!(
        fx.with_problem(|p| driven_shape_gradient_on_space::<B, _>(p, objective, &device()))
            .is_ok()
    );
    // … a non-empty one is not. Build a (geometry-irrelevant) geometric port.
    let _ = spec;
    let wave = geode_core::driven::ports::WavePort::single_mode(
        fx.port_faces.clone(),
        vec![0.0; 3],
        1.0,
        c64::new(1.0, 0.0),
    );
    let specs = vec![WavePortSpec::from(wave)];
    let err = fx.with_problem(|p| {
        let mut q = *p;
        q.wave_ports = &specs;
        driven_shape_gradient_on_space::<B, _>(&q, objective, &device()).unwrap_err()
    });
    let msg = err.to_string();
    assert!(matches!(
        err,
        DrivenError::UnsupportedAtOrder {
            order: ElementOrder::P2,
            ..
        }
    ));
    assert!(msg.contains("#836 P3b"), "{msg}");
    assert!(WAVE_PORT_GRADIENT_FEATURE.contains("#836 P3b"));

    let err = fx.with_problem(|p| {
        let mut q = *p;
        q.wave_ports = &specs;
        driven_material_adjoint_gradient_on_space::<B, _>(
            &q,
            &fx.region_of_tet,
            2,
            objective,
            &device(),
        )
        .unwrap_err()
    });
    assert!(err.to_string().contains("#836 P3b"));
}

/// At p=1 the order-generic driver is the existing ports driver, bit for bit.
#[test]
fn p1_material_gradient_delegates_bit_identically() {
    use geode_core::driven::adjoint::driven_material_adjoint_gradient_ports;
    let mesh = cube_tet_mesh(2, 1.0);
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let (_, mask) = geode_core::assembly::nedelec::cube_pec_interior_edges(&mesh, 1.0);
    let eps = vec![c64::new(2.0, -0.1); mesh.n_tets()];
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [c64::new(0.0, c[2]), c64::new(0.3, 0.2), c64::new(c[0], 0.1)]
    });
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let regions = regions(&mesh);
    let problem = DrivenGradientProblem {
        space: &space,
        mesh: &mesh,
        materials: DrivenMaterials::Scalar(&eps),
        bcs: &bcs,
        source: &source,
        ports: &[],
        surfaces: &[],
        wave_ports: &[],
        omega: 1.3,
        excited: None,
    };
    let a = driven_material_adjoint_gradient_on_space::<B, _>(
        &problem,
        &regions,
        2,
        objective,
        &device(),
    )
    .unwrap();
    let b = driven_material_adjoint_gradient_ports::<B, _>(
        &mesh,
        &eps,
        &bcs,
        1.3,
        &source,
        &[],
        &[],
        &regions,
        2,
        objective,
        &device(),
    )
    .unwrap();
    assert_eq!(a.grad_eps_prime, b.grad_eps_prime);
    assert_eq!(a.grad_eps_dprime, b.grad_eps_dprime);
}
