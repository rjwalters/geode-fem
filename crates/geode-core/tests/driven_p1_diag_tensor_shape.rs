//! Epic #841 Phase 2a (#1259): the p=1 **shape gradient of a
//! [`DrivenMaterials::DiagTensor`] material** (`ε = diag(ε_x, ε_y, ε_z)`,
//! `μ = 1`) through [`driven_shape_gradient_on_space`], against a central
//! finite difference of the whole forward (`assemble_with_space` +
//! `solve_at`), and its bit-for-bit equivalence with the matched-UPML tensor
//! path it reuses (`ε = diag`, `ν = I`).
//!
//! Fixture: a jittered unit cube, PEC plates at `y = 0, 1`, a uniform lumped
//! port across `z = 0` (`ê = ŷ`), a PEC cap at `z = 1`, and a genuinely
//! anisotropic complex diagonal ε in each of two halves. Observable
//! `g = Σ_i |x_i|²`.

mod common;
use common::boundary_plane_faces;

use burn::tensor::backend::BackendTypes;
use common::Lcg;
use faer::c64;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::adjoint::DrivenGradientProblem;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::shape::driven_shape_gradient_on_space;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

const OMEGA: f64 = 1.3;
const FD_H: f64 = 1e-6;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn objective(x: &[c64]) -> (f64, Vec<c64>) {
    (
        x.iter().map(|z| z.norm_sqr()).sum(),
        x.iter().map(|z| z.conj()).collect(),
    )
}

struct Fixture {
    mesh: TetMesh,
    port_faces: Vec<[u32; 3]>,
    mask: Vec<bool>,
    eps_diag: Vec<[c64; 3]>,
    source: CurrentSource,
}

fn fixture() -> Fixture {
    let mut mesh = cube_tet_mesh(2, 1.0);
    let h = 0.5;
    let mut rng = Lcg(0x1259);
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
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let port_faces = boundary_plane_faces(&mesh, 2, 0.0);
    let cap_faces = boundary_plane_faces(&mesh, 2, 1.0);
    let plates = [
        boundary_plane_faces(&mesh, 1, 0.0),
        boundary_plane_faces(&mesh, 1, 1.0),
    ];
    let walls: Vec<&[[u32; 3]]> = plates
        .iter()
        .map(Vec::as_slice)
        .chain(std::iter::once(cap_faces.as_slice()))
        .collect();
    let mask = space.pec_interior_mask(&mesh, &walls).expect("mask");
    let halves = [
        [
            c64::new(2.0, -0.1),
            c64::new(2.6, -0.05),
            c64::new(1.7, -0.2),
        ],
        [
            c64::new(3.0, -0.05),
            c64::new(2.2, -0.1),
            c64::new(3.4, 0.0),
        ],
    ];
    let eps_diag = mesh
        .tets
        .iter()
        .map(|t| {
            let cx = t.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
            halves[usize::from(cx > 0.5)]
        })
        .collect();
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.3 * c[2]),
            c64::new(0.5 * c[1], 0.2),
            c64::new(c[0], 0.4 * c[2]),
        ]
    });
    Fixture {
        mesh,
        port_faces,
        mask,
        eps_diag,
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

    fn forward_g(&self, mesh: &TetMesh) -> f64 {
        let space = HcurlSpace::build(mesh, ElementOrder::P1);
        let ports = [self.port()];
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            mesh,
            DrivenMaterials::DiagTensor(&self.eps_diag),
            None,
            &DrivenBcs {
                pec_interior_mask: &self.mask,
            },
            &ports,
            &[],
            DrivenSource::Constant(&self.source),
            &device(),
        )
        .expect("forward operator");
        objective(&op.solve_at(OMEGA).expect("forward solve").e_edges).0
    }

    fn gradient(&self, materials: DrivenMaterials<'_>) -> Vec<[f64; 3]> {
        let space = HcurlSpace::build(&self.mesh, ElementOrder::P1);
        let ports = [self.port()];
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
            surfaces: &[],
            wave_ports: &[],
            omega: OMEGA,
            excited: None,
        };
        let res = driven_shape_gradient_on_space::<B, _>(&problem, objective, &device())
            .expect("p1 shape gradient");
        assert_eq!(res.n_factorizations, 1);
        assert!(res.residual_rel < 1e-8);
        res.grad_node
    }
}

#[test]
fn diag_tensor_shape_gradient_matches_central_fd_and_the_tensor_path() {
    let fx = fixture();
    let grad = fx.gradient(DrivenMaterials::DiagTensor(&fx.eps_diag));

    // A random motion of every non-feed node.
    let mut rng = Lcg(0xD1A6);
    // The lumped feed is pinned (its geometry is a parameter of the port, not
    // of the differentiated pencil): port-face nodes do not move.
    let pinned: std::collections::HashSet<u32> = fx.port_faces.iter().flatten().copied().collect();
    let d: Vec<[f64; 3]> = (0..fx.mesh.nodes.len())
        .map(|i| {
            let v = [rng.next(), rng.next(), rng.next()];
            if pinned.contains(&(i as u32)) {
                [0.0; 3]
            } else {
                v
            }
        })
        .collect();
    let ana: f64 = grad
        .iter()
        .zip(&d)
        .map(|(g, dn)| g[0] * dn[0] + g[1] * dn[1] + g[2] * dn[2])
        .sum();
    let moved = |theta: f64| {
        let mut m = fx.mesh.clone();
        for (p, dn) in m.nodes.iter_mut().zip(&d) {
            for k in 0..3 {
                p[k] += theta * dn[k];
            }
        }
        fx.forward_g(&m)
    };
    let fd = (moved(FD_H) - moved(-FD_H)) / (2.0 * FD_H);
    assert!(fd.abs() > 1e-6, "degenerate FD {fd}");
    let rel = (ana - fd).abs() / fd.abs();
    eprintln!("DiagTensor shape: adjoint {ana:.8e} FD {fd:.8e} rel {rel:.2e}");
    assert!(rel < 1e-4, "adjoint {ana} vs FD {fd} (rel {rel:.3e})");

    // The same ε as full tensors with ν = I: the very same Dual path.
    let z = c64::new(0.0, 0.0);
    let one = c64::new(1.0, 0.0);
    let eps_t: Vec<[[c64; 3]; 3]> = fx
        .eps_diag
        .iter()
        .map(|e| [[e[0], z, z], [z, e[1], z], [z, z, e[2]]])
        .collect();
    let nu_t = vec![[[one, z, z], [z, one, z], [z, z, one]]; fx.mesh.n_tets()];
    let via_tensor = fx.gradient(DrivenMaterials::MatchedUpml {
        epsilon_tensor: &eps_t,
        nu_tensor: &nu_t,
    });
    assert_eq!(grad, via_tensor);
}
