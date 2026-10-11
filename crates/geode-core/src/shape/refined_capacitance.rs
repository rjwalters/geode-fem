//! Memory-scalable shape derivatives from an equilibrated P1 potential.
//!
//! This contraction does not assemble a nodal gradient or factor a matrix.
//! It can consume a potential obtained with an iterative solver. The caller
//! must verify equilibrium, including zero net charge on floating conductors.

use super::{Dual, stiffness_bilinear_dual};
use crate::assembly::electrostatic::{EPS_0, ElectrostaticError};
use crate::mesh::TetMesh;

/// Differentiate `phiᵀ K(X) phi` along each supplied nodal velocity field.
///
/// For an equilibrated unit-voltage excitation with neutral floating
/// conductors, this is the derivative of effective capacitance: the implicit
/// potential terms vanish by stationarity. Fixed-voltage values, conductor
/// membership and material values must stay constant during the motion.
/// `eps_r` is scalar, per tetrahedron. Coordinates/velocities must use metres
/// to obtain F per unit parameter. No derivative of remeshing is implied.
///
/// The result is exact for the discrete P1 energy up to floating-point and
/// field-solve error. It says nothing about spatial convergence. For a
/// non-equilibrated field it is only the partial derivative at fixed `phi`.
///
/// # Errors
/// Returns an error on inconsistent array sizes or non-finite input/output.
pub fn p1_energy_shape_derivatives(
    mesh: &TetMesh,
    eps_r: &[f64],
    phi: &[f64],
    directions: &[Vec<[f64; 3]>],
) -> Result<Vec<f64>, ElectrostaticError> {
    let invalid = |s: &str| ElectrostaticError::ShapeMismatch(s.to_string());
    if eps_r.len() != mesh.n_tets()
        || phi.len() != mesh.n_nodes()
        || directions.iter().any(|d| d.len() != mesh.n_nodes())
    {
        return Err(invalid("P1 energy derivative: inconsistent array sizes"));
    }
    if eps_r.iter().chain(phi).any(|x| !x.is_finite())
        || directions
            .iter()
            .flatten()
            .flatten()
            .any(|x| !x.is_finite())
    {
        return Err(invalid("P1 energy derivative: non-finite input"));
    }
    let mut sums = vec![0.0; directions.len()];
    // Compensated summation matters when a shape derivative cancels over
    // millions of elements (e.g. an internal mesh motion).
    let mut corrections = vec![0.0; directions.len()];
    for (tet, eps) in mesh.tets.iter().zip(eps_r) {
        let local_phi = tet.map(|v| phi[v as usize]);
        for ((field, sum), correction) in directions.iter().zip(&mut sums).zip(&mut corrections) {
            let coords = tet.map(|v| {
                std::array::from_fn(|d| Dual {
                    re: mesh.nodes[v as usize][d],
                    du: field[v as usize][d],
                })
            });
            let value = EPS_0 * eps * stiffness_bilinear_dual(&coords, &local_phi, &local_phi).du;
            let y = value - *correction;
            let next = *sum + y;
            *correction = (next - *sum) - y;
            *sum = next;
        }
    }
    if sums.iter().any(|x| !x.is_finite()) {
        return Err(invalid(
            "P1 energy derivative: non-finite result (degenerate mesh?)",
        ));
    }
    Ok(sums)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::electrostatic::Electrode;
    use crate::mesh::cube_tet_mesh;
    use crate::shape::{apply_node_motion, capacitance_matrix_shape_gradient};

    #[test]
    fn floating_energy_derivative_matches_solved_fd_and_scaling() {
        let mesh = cube_tet_mesh(4, 1.0);
        let on = |z: f64| -> Vec<u32> {
            mesh.nodes
                .iter()
                .enumerate()
                .filter_map(|(i, x)| ((x[2] - z).abs() < 1e-12).then_some(i as u32))
                .collect()
        };
        let electrodes = vec![
            Electrode {
                name: "top".into(),
                nodes: on(1.0),
                voltage: 1.0,
            },
            Electrode {
                name: "floating".into(),
                nodes: on(0.5),
                voltage: 0.0,
            },
        ];
        let ground = on(0.0);
        let eps = vec![3.0; mesh.n_tets()];
        let solved = capacitance_matrix_shape_gradient(&mesh, &eps, &electrodes, &ground).unwrap();
        let reduced = solved.floating_reduction(0, &[1]).unwrap();
        let vf = -solved.c[1][0] / solved.c[1][1];
        let phi: Vec<f64> = solved.phis[0]
            .iter()
            .zip(&solved.phis[1])
            .map(|(a, b)| a + vf * b)
            .collect();
        let dilation = mesh.nodes.clone();
        let translation = vec![[1.0, -2.0, 0.5]; mesh.n_nodes()];
        let thickness: Vec<_> = mesh.nodes.iter().map(|x| [0.0, 0.0, x[2]]).collect();
        let fields = vec![dilation, translation, thickness];
        let derivatives = p1_energy_shape_derivatives(&mesh, &eps, &phi, &fields).unwrap();
        let c = reduced.c_sigma;
        assert!((derivatives[0] / c - 1.0).abs() < 1e-10);
        assert!(derivatives[1].abs() / c < 1e-10);
        assert!((derivatives[2] / c + 1.0).abs() < 1e-10);
        for (field, derivative) in fields.iter().zip(derivatives) {
            assert!((derivative - reduced.dc_dtheta(field)).abs() / c < 1e-10);
            let evaluate = |h| {
                capacitance_matrix_shape_gradient(
                    &apply_node_motion(&mesh, field, h),
                    &eps,
                    &electrodes,
                    &ground,
                )
                .unwrap()
                .floating_reduction(0, &[1])
                .unwrap()
                .c_sigma
            };
            let h = 1e-4;
            let fd = (evaluate(h) - evaluate(-h)) / (2.0 * h);
            assert!((fd - derivative).abs() / c < 2e-8);
        }
    }
}
