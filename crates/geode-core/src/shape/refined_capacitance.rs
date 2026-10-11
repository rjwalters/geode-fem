//! Memory-scalable shape derivatives from equilibrated P1 or P2 potentials.
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

/// P2 counterpart of [`p1_energy_shape_derivatives`] on affine tetrahedra.
/// The potential is ordered by vertices then global edge midpoints, exactly
/// as the electrostatic P2 assembler. The geometry velocity remains P1.
///
/// For a mapped element, differentiating `det(J) |J^-T grad(u)|^2` gives
/// `div(v)|grad(u)|^2 - 2 grad(u)^T grad(v) grad(u)`. Four-point quadrature
/// integrates this degree-two expression exactly. As for P1, equilibrium is
/// required to interpret this energy partial derivative as a solved gradient.
///
/// # Errors
/// Returns an error for inconsistent sizes or non-finite values.
#[allow(clippy::needless_range_loop)] // small fixed-size tensor contractions
pub fn p2_energy_shape_derivatives(
    mesh: &TetMesh,
    eps_r: &[f64],
    phi: &[f64],
    directions: &[Vec<[f64; 3]>],
) -> Result<Vec<f64>, ElectrostaticError> {
    use crate::assembly::electrostatic::{tet_bary_grads, tet_p1_local};
    use crate::elements::nedelec::{TET_QUAD4_A, TET_QUAD4_B};
    use crate::elements::p2::tet_p2_grads;
    let invalid = || {
        ElectrostaticError::ShapeMismatch(
            "P2 energy derivative: invalid size or non-finite data".into(),
        )
    };
    if eps_r.len() != mesh.n_tets()
        || phi.len() != mesh.n_nodes() + mesh.edges().len()
        || directions.iter().any(|d| d.len() != mesh.n_nodes())
        || eps_r.iter().chain(phi).any(|v| !v.is_finite())
        || directions
            .iter()
            .flatten()
            .flatten()
            .any(|v| !v.is_finite())
    {
        return Err(invalid());
    }
    let edges = mesh.tet_edges();
    let mut sums = vec![0.0; directions.len()];
    let mut corrections = vec![0.0; directions.len()];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = tet.map(|v| mesh.nodes[v as usize]);
        let bary = tet_bary_grads(&coords);
        let volume = tet_p1_local(&coords).2;
        let mut local = [0.0; 10];
        for a in 0..4 {
            local[a] = phi[tet[a] as usize];
        }
        for e in 0..6 {
            local[e + 4] = phi[mesh.n_nodes() + edges[t][e].0 as usize];
        }
        let mut moment = [[0.0; 3]; 3];
        for q in 0..4 {
            let mut lam = [TET_QUAD4_B; 4];
            lam[q] = TET_QUAD4_A;
            let basis = tet_p2_grads(&lam, &bary);
            let mut g = [0.0; 3];
            for a in 0..10 {
                for d in 0..3 {
                    g[d] += local[a] * basis[a][d];
                }
            }
            for i in 0..3 {
                for j in 0..3 {
                    moment[i][j] += 0.25 * g[i] * g[j];
                }
            }
        }
        let energy = moment[0][0] + moment[1][1] + moment[2][2];
        for (p, field) in directions.iter().enumerate() {
            let mut grad_v = [[0.0; 3]; 3];
            for a in 0..4 {
                for i in 0..3 {
                    for j in 0..3 {
                        grad_v[i][j] += field[tet[a] as usize][i] * bary[a][j];
                    }
                }
            }
            let divergence = grad_v[0][0] + grad_v[1][1] + grad_v[2][2];
            let mut contraction = 0.0;
            for i in 0..3 {
                for j in 0..3 {
                    contraction += grad_v[i][j] * moment[i][j];
                }
            }
            let value = EPS_0 * eps_r[t] * volume * (divergence * energy - 2.0 * contraction);
            let y = value - corrections[p];
            let next = sums[p] + y;
            corrections[p] = (next - sums[p]) - y;
            sums[p] = next;
        }
    }
    if sums.iter().any(|v| !v.is_finite()) {
        return Err(invalid());
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
    fn quadratic_energy_derivative_matches_independent_kernel_fd() {
        use crate::elements::p2::tet_p2_local;
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let te = mesh.tet_edges();
        let value = |x: [f64; 3]| x[0] * x[0] + 0.7 * x[1] * x[2] + 0.2 * x[2];
        let mut phi: Vec<f64> = mesh.nodes.iter().copied().map(value).collect();
        phi.extend(edges.iter().map(|e| {
            value(std::array::from_fn(|d| {
                0.5 * (mesh.nodes[e[0] as usize][d] + mesh.nodes[e[1] as usize][d])
            }))
        }));
        let field: Vec<_> = mesh
            .nodes
            .iter()
            .map(|x| [0.1 * x[1] * x[2], -0.07 * x[0] * x[2], 0.2 * x[2]])
            .collect();
        let eps = vec![2.0; mesh.n_tets()];
        let energy = |m: &TetMesh| {
            m.tets
                .iter()
                .enumerate()
                .map(|(t, tet)| {
                    let k = tet_p2_local(&tet.map(|v| m.nodes[v as usize])).0;
                    let mut local = [0.0; 10];
                    for a in 0..4 {
                        local[a] = phi[tet[a] as usize];
                    }
                    for e in 0..6 {
                        local[e + 4] = phi[mesh.n_nodes() + te[t][e].0 as usize];
                    }
                    let mut sum = 0.0;
                    for i in 0..10 {
                        for j in 0..10 {
                            sum += local[i] * k[i][j] * local[j];
                        }
                    }
                    EPS_0 * eps[t] * sum
                })
                .sum::<f64>()
        };
        let d = p2_energy_shape_derivatives(&mesh, &eps, &phi, std::slice::from_ref(&field))
            .unwrap()[0];
        let h = 1e-4;
        let fd = (energy(&apply_node_motion(&mesh, &field, h))
            - energy(&apply_node_motion(&mesh, &field, -h)))
            / (2.0 * h);
        assert!((d - fd).abs() / d.abs() < 1e-7, "{d} vs {fd}");
        let dilation =
            p2_energy_shape_derivatives(&mesh, &eps, &phi, std::slice::from_ref(&mesh.nodes))
                .unwrap()[0];
        assert!((dilation / energy(&mesh) - 1.0).abs() < 1e-12);
    }

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
        let sys = crate::assembly::electrostatic::assemble_electrostatic(
            &mesh,
            &eps,
            &vec![0.0; mesh.n_tets()],
            &electrodes,
            &ground,
        )
        .unwrap();
        let (matrix, report, potentials) =
            crate::assembly::electrostatic::extract_capacitance_amg_pcg_fields(
                &sys,
                &electrodes,
                &ground,
                Default::default(),
            )
            .unwrap();
        assert!(report.rel_residual.iter().all(|r| *r < 1e-9));
        for (pcg, lu) in potentials.iter().zip(&solved.phis) {
            assert!(pcg.iter().zip(lu).all(|(a, b)| (a - b).abs() < 1e-8));
        }
        let vf = -matrix.c[1][0] / matrix.c[1][1];
        let phi: Vec<f64> = potentials[0]
            .iter()
            .zip(&potentials[1])
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
