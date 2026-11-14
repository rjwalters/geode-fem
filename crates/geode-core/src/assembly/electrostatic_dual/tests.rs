use super::*;
use crate::assembly::electrostatic::{
    Electrode, assemble_electrostatic, assemble_electrostatic_p2_sheets,
    assemble_electrostatic_p2_sheets_tensor, assemble_electrostatic_tensor,
    extract_capacitance_amg_pcg_fields, extract_capacitance_p2_amg_pcg_fields,
};
use crate::mesh::cube_tet_mesh;
use crate::mesh::red_refine::red_refine;

fn tight() -> AmgPcgOptions {
    AmgPcgOptions {
        rel_tol: 1e-12,
        ..AmgPcgOptions::default()
    }
}

/// Faces whose three vertices satisfy `on` and whose centroid satisfies
/// `inside`.
fn faces_where(
    mesh: &TetMesh,
    on: impl Fn(&[f64; 3]) -> bool,
    inside: impl Fn(&[f64; 3]) -> bool,
) -> Vec<[u32; 3]> {
    let mut out = std::collections::BTreeSet::new();
    for tet in &mesh.tets {
        for lf in LOCAL_FACES {
            let f = sorted3(lf.map(|l| tet[l]));
            let p = f.map(|v| mesh.nodes[v as usize]);
            let c = std::array::from_fn(|d| (p[0][d] + p[1][d] + p[2][d]) / 3.0);
            if p.iter().all(&on) && inside(&c) {
                out.insert(f);
            }
        }
    }
    out.into_iter().collect()
}

fn nodes_of(tris: &[[u32; 3]]) -> Vec<u32> {
    let mut v: Vec<u32> = tris.iter().flatten().copied().collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A conductor set: driven first, then floating, then ground.
struct Problem {
    mesh: TetMesh,
    eps: Vec<f64>,
    driven: Vec<[u32; 3]>,
    floating: Vec<Vec<[u32; 3]>>,
    ground: Vec<[u32; 3]>,
}

impl Problem {
    fn conductors(&self) -> Vec<DualConductor> {
        let mut c = vec![DualConductor {
            name: "driven".into(),
            role: ConductorRole::Driven,
            triangles: self.driven.clone(),
        }];
        for (i, f) in self.floating.iter().enumerate() {
            c.push(DualConductor {
                name: format!("floating{i}"),
                role: ConductorRole::Floating,
                triangles: f.clone(),
            });
        }
        c.push(DualConductor {
            name: "ground".into(),
            role: ConductorRole::Ground,
            triangles: self.ground.clone(),
        });
        c
    }

    fn topology(&self) -> FaceTopology {
        FaceTopology::new(&self.mesh, &self.conductors()).unwrap()
    }

    /// Conforming primal `C` (F) and its floating-reduced potential.
    fn primal(&self, order: usize) -> (f64, Vec<f64>) {
        self.primal_with(order, None)
    }

    /// [`Self::primal`] with an optional per-tet `ε_r` tensor in place
    /// of the scalar `eps` (issue #1091).
    fn primal_with(&self, order: usize, tensor: Option<&[[[f64; 3]; 3]]>) -> (f64, Vec<f64>) {
        let mut electrodes = vec![Electrode {
            name: "driven".into(),
            nodes: nodes_of(&self.driven),
            voltage: 1.0,
        }];
        for f in &self.floating {
            electrodes.push(Electrode {
                name: "floating".into(),
                nodes: nodes_of(f),
                voltage: 0.0,
            });
        }
        let ground = nodes_of(&self.ground);
        let rho = vec![0.0; self.mesh.n_tets()];
        let (cm, rep, phis) = if order == 1 {
            let sys = match tensor {
                None => assemble_electrostatic(&self.mesh, &self.eps, &rho, &electrodes, &ground),
                Some(t) => assemble_electrostatic_tensor(&self.mesh, t, &rho, &electrodes, &ground),
            }
            .unwrap();
            extract_capacitance_amg_pcg_fields(&sys, &electrodes, &ground, tight()).unwrap()
        } else {
            let mut tris: Vec<&[[u32; 3]]> = vec![&self.driven];
            tris.extend(self.floating.iter().map(Vec::as_slice));
            let (sys, dofs) = match tensor {
                None => assemble_electrostatic_p2_sheets(
                    &self.mesh,
                    &self.eps,
                    &rho,
                    &electrodes,
                    &tris,
                    &ground,
                    &self.ground,
                ),
                Some(t) => assemble_electrostatic_p2_sheets_tensor(
                    &self.mesh,
                    t,
                    &rho,
                    &electrodes,
                    &tris,
                    &ground,
                    &self.ground,
                ),
            }
            .unwrap();
            extract_capacitance_p2_amg_pcg_fields(&sys, &dofs, tight()).unwrap()
        };
        assert!(rep.rel_residual.iter().all(|r| *r < 1e-10));
        let v = floating_potentials(&cm.c).unwrap();
        let mut phi = phis[0].clone();
        for (vf, pf) in v.iter().zip(&phis[1..]) {
            for (a, b) in phi.iter_mut().zip(pf) {
                *a += vf * b;
            }
        }
        let c = cm.c[0][0]
            + v.iter()
                .enumerate()
                .map(|(j, x)| x * cm.c[0][j + 1])
                .sum::<f64>();
        (c, phi)
    }

    fn equilibrated(&self, order: usize, k: usize) -> (f64, DualBound) {
        self.equilibrated_with(order, k, None)
    }

    /// [`Self::equilibrated`] with an optional per-tet `ε_r` tensor.
    fn equilibrated_with(
        &self,
        order: usize,
        k: usize,
        tensor: Option<&[[[f64; 3]; 3]]>,
    ) -> (f64, DualBound) {
        let topo = self.topology();
        let (c, phi) = self.primal_with(order, tensor);
        let primal = if order == 1 {
            PrimalPotential::P1(&phi)
        } else {
            PrimalPotential::P2(&phi)
        };
        let eps = match tensor {
            None => Permittivity::Scalar(&self.eps),
            Some(t) => Permittivity::Tensor(t),
        };
        let flux = equilibrated_flux(&self.mesh, &topo, eps, primal, k).unwrap();
        (
            c,
            certify_lower_bound(&self.mesh, &topo, eps, &flux).unwrap(),
        )
    }

    fn crouzeix_raviart(&self) -> (CrouzeixRaviartReport, DualBound) {
        let topo = self.topology();
        let eps = Permittivity::Scalar(&self.eps);
        let (flux, rep) = crouzeix_raviart_flux(&self.mesh, &topo, eps, tight()).unwrap();
        let bound = certify_lower_bound(&self.mesh, &topo, eps, &flux).unwrap();
        (rep, bound)
    }

    fn red_refined(&self) -> Self {
        let r = red_refine(&self.mesh);
        Self {
            eps: r.lift_per_tet(&self.eps),
            driven: r.lift_triangles(&self.driven),
            floating: self.floating.iter().map(|f| r.lift_triangles(f)).collect(),
            ground: r.lift_triangles(&self.ground),
            mesh: r.mesh,
        }
    }
}

fn factorial(n: u32) -> f64 {
    (1..=n).map(f64::from).product()
}

#[test]
fn duffy_rules_are_exact_to_their_degree() {
    for n in 2..6usize {
        let tet = duffy_tet(n);
        let tri = duffy_tri(n);
        for e in monomials(2 * n - 3, false) {
            // ∫ λ1^a λ2^b λ3^c over the unit tet, normalized by its volume.
            let exact = 6.0 * factorial(e[0]) * factorial(e[1]) * factorial(e[2])
                / factorial(e[0] + e[1] + e[2] + 3);
            let q: f64 = tet
                .iter()
                .map(|(l, w)| {
                    w * l[1].powi(e[0] as i32) * l[2].powi(e[1] as i32) * l[3].powi(e[2] as i32)
                })
                .sum();
            assert!((q - exact).abs() < 1e-14, "tet n={n} {e:?}: {q} vs {exact}");
        }
        for deg in 0..=(2 * n - 2) as u32 {
            for a in 0..=deg {
                let b = deg - a;
                let exact = 2.0 * factorial(a) * factorial(b) / factorial(a + b + 2);
                let q: f64 = tri
                    .iter()
                    .map(|(l, w)| w * l[1].powi(a as i32) * l[2].powi(b as i32))
                    .sum();
                assert!((q - exact).abs() < 1e-14, "tri n={n} ({a},{b})");
            }
        }
    }
}

/// The nodal basis is dual to the degrees of freedom: a single-tet RT_k
/// field with one unit face moment has that moment and no other.
#[test]
fn rt_nodal_basis_is_dual_to_the_face_moments() {
    let mesh = cube_tet_mesh(1, 1.0);
    for k in 0..=MAX_K {
        let sp = RtSpace::new(k);
        let geo = TetGeom::new(&mesh, 2);
        let b = sp.nodal_basis(&mesh, 2, &geo);
        let tet = mesh.tets[2];
        let nfm = sp.face_mono.len();
        let mut s = vec![[0.0; 3]; sp.dim];
        for col in 0..sp.dim {
            let c: Vec<f64> = (0..sp.dim).map(|j| b[j * sp.dim + col]).collect();
            for (f, lf) in LOCAL_FACES.iter().enumerate() {
                let p = sorted3(lf.map(|l| tet[l])).map(|v| mesh.nodes[v as usize]);
                let (n, _) = unit_normal(&p);
                for (i, e) in sp.face_mono.iter().enumerate() {
                    let m: f64 = sp
                        .tri_q
                        .iter()
                        .map(|(lam, w)| {
                            let x = add3(scale3(p[0], lam[0]), p[1], lam[1], p[2], lam[2]);
                            sp.eval(&geo.xi(&x), &mut s);
                            let mu = lam[0].powi(e[0] as i32)
                                * lam[1].powi(e[1] as i32)
                                * lam[2].powi(e[2] as i32);
                            w * mu * dot(combine(&c, &s), n)
                        })
                        .sum();
                    let want = if f * nfm + i == col { 1.0 } else { 0.0 };
                    assert!(
                        (m - want).abs() < 1e-9, // Vandermonde conditioning grows with k
                        "k={k} col={col} face={f} i={i}: {m}"
                    );
                }
            }
        }
    }
}

// ── Certificate terms against closed forms (#1062 review) ────────────
//
// Each test builds a flux by `RT_k` interpolation of a known field (exact
// for a field in `RT_k`) and checks one term of the certificate against
// its analytic value. Together they catch a face `L¹` term taken as
// `|F|·|mean q|` or by one-point quadrature, a dropped divergence term,
// and an energy evaluated with an inexact rule.

/// A mesh from explicit nodes and tets.
fn tet_mesh(nodes: Vec<[f64; 3]>, tets: Vec<[u32; 4]>) -> TetMesh {
    TetMesh {
        nodes,
        tets,
        physical_groups: Default::default(),
    }
}

/// The unit right tetrahedron, `x, y, z ≥ 0`, `x + y + z ≤ 1`.
fn unit_tet() -> TetMesh {
    tet_mesh(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        vec![[0, 1, 2, 3]],
    )
}

/// Topology with one driven face and one ground face.
fn two_face_topology(mesh: &TetMesh, driven: [u32; 3], ground: [u32; 3]) -> FaceTopology {
    FaceTopology::new(
        mesh,
        &[
            DualConductor {
                name: "driven".into(),
                role: ConductorRole::Driven,
                triangles: vec![driven],
            },
            DualConductor {
                name: "ground".into(),
                role: ConductorRole::Ground,
                triangles: vec![ground],
            },
        ],
    )
    .unwrap()
}

/// The `RT_k` interpolant of the per-tet field `f(t, x)`: its face and
/// interior moments, evaluated with rules finer than the space's own,
/// mapped through the nodal basis. Exact when `f(t, ·)` lies in `RT_k`.
fn interpolate(mesh: &TetMesh, k: usize, f: impl Fn(usize, &[f64; 3]) -> [f64; 3]) -> RtFlux {
    let sp = RtSpace::new(k);
    let (dim, nfm) = (sp.dim, sp.face_mono.len());
    let tri_q = duffy_tri(MAX_K + 3);
    let tet_q = duffy_tet(MAX_K + 4);
    let mut coeffs = Vec::with_capacity(dim * mesh.n_tets());
    for t in 0..mesh.n_tets() {
        let geo = TetGeom::new(mesh, t);
        let b = sp.nodal_basis(mesh, t, &geo);
        let tet = mesh.tets[t];
        let mut dofs = vec![0.0; dim];
        for (fi, lf) in LOCAL_FACES.iter().enumerate() {
            let p = sorted3(lf.map(|l| tet[l])).map(|v| mesh.nodes[v as usize]);
            let (n, _) = unit_normal(&p);
            for (lam, w) in &tri_q {
                let x = add3(scale3(p[0], lam[0]), p[1], lam[1], p[2], lam[2]);
                let fn_x = dot(f(t, &x), n);
                for (i, e) in sp.face_mono.iter().enumerate() {
                    let mu = lam[0].powi(e[0] as i32)
                        * lam[1].powi(e[1] as i32)
                        * lam[2].powi(e[2] as i32);
                    dofs[fi * nfm + i] += w * mu * fn_x;
                }
            }
        }
        for (lam, w) in &tet_q {
            let x = geo.point(lam);
            let pw = sp.powers(&geo.xi(&x));
            let v = f(t, &x);
            for (i, e) in sp.int_mono.iter().enumerate() {
                let q = RtSpace::mono(&pw, e);
                for d in 0..3 {
                    dofs[4 * nfm + 3 * i + d] += w * q * v[d];
                }
            }
        }
        for j in 0..dim {
            coeffs.push((0..dim).map(|r| b[j * dim + r] * dofs[r]).sum());
        }
    }
    RtFlux { k, coeffs }
}

/// A closed-form test field.
type Field = fn(&[f64; 3]) -> [f64; 3];

/// `N` is the driven charge minus every penalty (no floating conductor).
fn assert_numerator_identity(b: &DualBound) {
    let want = b.charges[0] - (b.interior_jump_l1 + b.boundary_flux_l1 + b.divergence_l1);
    assert!((b.numerator - want).abs() < 1e-12, "{b:?}");
}

/// `|F|^½ ‖x − ⅓‖_{L²(F)}` on the unit right triangle in `z = 0` (area ½;
/// there `x` has mean ⅓ and variance 1/18), that is `√(½ · ½/18) = 1/√72`.
/// The function has zero mean but a nonzero `L¹` norm.
fn zero_mean_face_penalty() -> f64 {
    1.0 / 72.0_f64.sqrt()
}

/// (a) An interior normal jump with zero mean and nonzero `L¹` norm. Two
/// tets share the face `z = 0`; above it `D = (0, 0, x − ⅓)` (divergence
/// free, no flux through the upper tet's natural faces), below it
/// `D = 0`. The jump penalty must be the Cauchy–Schwarz value
/// `|F|^½ ‖q‖_{L²(F)}` exactly: a signed mean gives 0, a one-point rule a
/// different number.
#[test]
fn zero_mean_interior_jump_is_charged_its_l2_upper_bound() {
    let mesh = tet_mesh(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ],
        vec![[0, 1, 2, 3], [0, 1, 2, 4]],
    );
    // Driven: the slanted face of the upper tet; ground: y = 0 below.
    let topo = two_face_topology(&mesh, [1, 2, 3], [0, 1, 4]);
    for k in 1..=MAX_K {
        let flux = interpolate(&mesh, k, |t, x| {
            if t == 0 {
                [0.0, 0.0, x[0] - 1.0 / 3.0]
            } else {
                [0.0; 3]
            }
        });
        let b =
            certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&[1.0, 1.0]), &flux).unwrap();
        assert!(
            (b.interior_jump_l1 - zero_mean_face_penalty()).abs() < 1e-9,
            "k={k}: jump penalty {} vs {}",
            b.interior_jump_l1,
            zero_mean_face_penalty()
        );
        assert!(
            b.boundary_flux_l1 < 1e-9 && b.divergence_l1 < 1e-9,
            "k={k}: {b:?}"
        );
        assert_numerator_identity(&b);
    }
}

/// (b) A non-solenoidal flux on one tet (driven slanted face, ground
/// `z = 0`, natural faces `x = 0` and `y = 0` with zero normal flux).
/// `D = (x, 0, 0)` has `div D = 1`, so the penalty is
/// `|T|^½ ‖1‖_{L²(T)} = |T| = 1/6`; `D = (x², 0, 0)` has `div D = 2x`,
/// penalty `√(|T| · 4∫x²) = √(1/6 · 1/15) = 1/√90`, above the true
/// `∫|2x| = 1/12`. Dropping the term gives 0.
#[test]
fn divergence_penalty_matches_the_closed_form() {
    let mesh = unit_tet();
    let topo = two_face_topology(&mesh, [1, 2, 3], [0, 1, 2]);
    let eps = [1.0];
    let cases: [(usize, f64, Field); 2] = [
        (1, 1.0 / 6.0, |x| [x[0], 0.0, 0.0]),
        (2, 1.0 / 90.0_f64.sqrt(), |x| [x[0] * x[0], 0.0, 0.0]),
    ];
    for (k_min, want, field) in cases {
        for k in k_min..=MAX_K {
            let flux = interpolate(&mesh, k, |_, x| field(x));
            let b = certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&eps), &flux).unwrap();
            assert!(
                (b.divergence_l1 - want).abs() < 1e-9,
                "k={k}: divergence penalty {} vs {want}",
                b.divergence_l1
            );
            assert!(b.boundary_flux_l1 < 1e-9 && b.interior_jump_l1 == 0.0);
            assert_numerator_identity(&b);
            // The divergence is paid for: N is below the driven charge.
            assert!(b.numerator <= b.charges[0] - want + 1e-9);
        }
    }
}

/// (c) Flux through the natural boundary of one tet (driven slanted face,
/// ground `x = 0`, natural faces `y = 0` and `z = 0`). `D = (0, 0, x − ⅓)`
/// is divergence free with zero-mean normal flux on `z = 0`, penalty
/// `1/√72`; the constant `D = (0, 0, 1)` has `|D·n| = 1` there, penalty
/// `|F| = ½` (Cauchy–Schwarz is an equality for a constant).
#[test]
fn boundary_flux_penalty_matches_the_closed_form() {
    let mesh = unit_tet();
    let topo = two_face_topology(&mesh, [1, 2, 3], [0, 2, 3]);
    let eps = [1.0];
    let cases: [(usize, f64, Field); 2] = [
        (1, zero_mean_face_penalty(), |x| {
            [0.0, 0.0, x[0] - 1.0 / 3.0]
        }),
        (0, 0.5, |_| [0.0, 0.0, 1.0]),
    ];
    for (k_min, want, field) in cases {
        for k in k_min..=MAX_K {
            let flux = interpolate(&mesh, k, |_, x| field(x));
            let b = certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&eps), &flux).unwrap();
            assert!(
                (b.boundary_flux_l1 - want).abs() < 1e-9,
                "k={k}: boundary penalty {} vs {want}",
                b.boundary_flux_l1
            );
            assert!(b.divergence_l1 < 1e-9 && b.interior_jump_l1 == 0.0);
            assert_numerator_identity(&b);
        }
    }
}

/// (d) `W_D = ∫ D·ε⁻¹D` of a degree-`k+1` field of `RT_k`,
/// `D = (x, y, z) x^k`, on the tet `(0,0,0), (2,0,0), (0,1,0), (0,0,½)`,
/// for every `k ≤ MAX_K`, against the closed form from
/// `∫ u^a v^b w^c = a! b! c! / (a+b+c+3)!` on the unit tet (`x = 2u`,
/// `y = v`, `z = w/2`, unit Jacobian). The integrand has degree `2k + 2`,
/// so an inexact rule shows. Scalar `ε = 2.5` and the tensor
/// `diag(2, 4, 8)`.
#[test]
fn single_tet_energy_matches_the_closed_form_for_every_order() {
    let mesh = tet_mesh(
        vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.5],
        ],
        vec![[0, 1, 2, 3]],
    );
    let topo = two_face_topology(&mesh, [1, 2, 3], [0, 1, 2]);
    let tensor = [[[2.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 8.0]]];
    for k in 0..=MAX_K {
        let kk = k as u32;
        let den = factorial(2 * kk + 5);
        let pow4 = 4f64.powi(k as i32);
        // ∫ x^{2k+2}, ∫ x^{2k} y², ∫ x^{2k} z² over the stretched tet.
        let ixx = pow4 * 4.0 * factorial(2 * kk + 2) / den;
        let iyy = pow4 * 2.0 * factorial(2 * kk) / den;
        let izz = pow4 * 0.25 * 2.0 * factorial(2 * kk) / den;
        let flux = interpolate(&mesh, k, |_, x| {
            let m = x[0].powi(k as i32);
            [x[0] * m, x[1] * m, x[2] * m]
        });
        let scalar =
            certify_lower_bound(&mesh, &topo, Permittivity::Scalar(&[2.5]), &flux).unwrap();
        let want = (ixx + iyy + izz) / 2.5;
        assert!(
            (scalar.energy / want - 1.0).abs() < 1e-9,
            "k={k}: scalar W_D {} vs {want}",
            scalar.energy
        );
        let tens = certify_lower_bound(&mesh, &topo, Permittivity::Tensor(&tensor), &flux).unwrap();
        let want = ixx / 2.0 + iyy / 4.0 + izz / 8.0;
        assert!(
            (tens.energy / want - 1.0).abs() < 1e-9,
            "k={k}: tensor W_D {} vs {want}",
            tens.energy
        );
    }
}

/// A stacked two-dielectric parallel plate with a full-width floating
/// sheet at the interface: the field is piecewise constant, so P1, P2,
/// Crouzeix–Raviart and both equilibrations are exact, and the bracket
/// collapses onto `ε₀ A / Σ(d_i/ε_i)`.
#[test]
fn parallel_plate_bracket_collapses_on_the_exact_value() {
    let pb = stacked_plate();
    let exact = EPS_0 / (0.5 / 4.0 + 0.5 / 1.0);
    for order in [1, 2] {
        let (c, b) = pb.equilibrated(order, 1);
        assert!(
            (c / exact - 1.0).abs() < 1e-9,
            "P{order} primal {c} vs {exact}"
        );
        assert!(
            (b.c_lower_f / exact - 1.0).abs() < 1e-8,
            "P{order} dual {}",
            b.c_lower_f
        );
        assert!(b.c_lower_f <= c * (1.0 + 1e-10));
        assert!(
            b.charges[1].abs() < 1e-9 * b.charges[0],
            "floating not neutral"
        );
    }
    let (rep, b) = pb.crouzeix_raviart();
    assert!((rep.c_cr_f / exact - 1.0).abs() < 1e-9);
    assert!((b.c_lower_f / exact - 1.0).abs() < 1e-8);
}

/// Structured annulus (one slab layer, natural top and bottom) between
/// two similar regular `n`-gons with vertex radii `r_in < r_out`;
/// returns the problem with the inner polygon driven and the outer one
/// grounded.
fn coax(n: usize, m: usize, r_in: f64, r_out: f64, thickness: f64) -> Problem {
    let idx = |i: usize, j: usize, l: usize| ((l * (m + 1) + j) * n + i % n) as u32;
    let mut nodes = Vec::new();
    for l in 0..2 {
        for j in 0..=m {
            let r = r_in * (r_out / r_in).powf(j as f64 / m as f64);
            for i in 0..n {
                let th = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                nodes.push([r * th.cos(), r * th.sin(), l as f64 * thickness]);
            }
        }
    }
    let mut tets = Vec::new();
    let perms = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for i in 0..n {
        for j in 0..m {
            for p in perms {
                let mut c = [i, j, 0];
                let mut tet = [idx(c[0], c[1], c[2]); 4];
                for (s, &axis) in p.iter().enumerate() {
                    c[axis] += 1;
                    tet[s + 1] = idx(c[0], c[1], c[2]);
                }
                tets.push(tet);
            }
        }
    }
    let mesh = TetMesh {
        nodes,
        tets,
        physical_groups: Default::default(),
    };
    let on_ring = |mesh: &TetMesh, j: usize| {
        let set: std::collections::BTreeSet<u32> = (0..2)
            .flat_map(|l| (0..n).map(move |i| idx(i, j, l)))
            .collect();
        let mut out = std::collections::BTreeSet::new();
        for tet in &mesh.tets {
            for lf in LOCAL_FACES {
                let f = sorted3(lf.map(|x| tet[x]));
                if f.iter().all(|v| set.contains(v)) {
                    out.insert(f);
                }
            }
        }
        out.into_iter().collect::<Vec<_>>()
    };
    let driven = on_ring(&mesh, 0);
    let ground = on_ring(&mesh, m);
    let eps = vec![1.0; mesh.n_tets()];
    Problem {
        mesh,
        eps,
        driven,
        floating: Vec::new(),
        ground,
    }
}

/// The exact coaxial capacitance `2πε₀t/ln(b/a)` is bracketed with
/// certainty by two polygonal families: an inner polygon around the
/// circle `a` and an outer one inside `b` can only raise `C` (upper
/// family), the reverse can only lower it (lower family). The P2 upper
/// bound of the first and the equilibrated lower bound of the second
/// must bracket the analytic value, and the bracket must shrink.
#[test]
fn coax_analytic_value_is_bracketed_and_the_bracket_converges() {
    let (a, b, t): (f64, f64, f64) = (1.0, 2.0, 0.25);
    let exact = 2.0 * std::f64::consts::PI * EPS_0 * t / (b / a).ln();
    let mut widths = Vec::new();
    for n in [12, 24, 48] {
        let m = n / 4;
        let cos = (std::f64::consts::PI / n as f64).cos();
        let upper = coax(n, m, a / cos, b, t);
        let lower = coax(n, m, a, b / cos, t);
        let (c_up, _) = upper.primal(2);
        let (c_lo_primal, bound) = lower.equilibrated(2, 1);
        let (cr, cr_bound) = lower.crouzeix_raviart();
        assert!(bound.c_lower_f <= c_lo_primal);
        assert!(cr_bound.c_lower_f <= c_lo_primal);
        assert!((cr_bound.c_lower_f / cr.c_cr_f - 1.0).abs() < 1e-8);
        assert!(
            bound.c_lower_f < exact && exact < c_up,
            "n={n}: [{}, {c_up}] misses {exact}",
            bound.c_lower_f
        );
        assert!(cr_bound.c_lower_f < exact);
        widths.push((c_up - bound.c_lower_f) / exact);
    }
    // Geometry and discretization error are both O(h^2) here: halving h
    // should divide the width by 4 (ratio 0.25). Measured: 0.236, 0.245.
    assert!(
        widths[1] < 0.27 * widths[0] && widths[2] < 0.27 * widths[1],
        "{widths:?}"
    );
}

/// A coplanar island and a floating feedline in a ground cutout on the
/// interface of an `ε = 10` substrate (the #1057 judge's synthetic
/// check, with a floating conductor). Sheet edges make the field
/// singular, so nothing is exact; every certified lower bound must lie
/// below every conforming upper bound across two levels and both
/// orders.
fn coplanar() -> Problem {
    let mesh = cube_tet_mesh(8, 1.0);
    let tol = 1e-12;
    let on = |p: &[f64; 3]| (p[2] - 0.5).abs() < tol;
    let inr = |c: &[f64; 3], x0: f64, x1: f64, y0: f64, y1: f64| {
        c[0] > x0 && c[0] < x1 && c[1] > y0 && c[1] < y1
    };
    let h = 0.125;
    let driven = faces_where(&mesh, on, |c| inr(c, 2.0 * h, 4.0 * h, 2.0 * h, 6.0 * h));
    let feed = faces_where(&mesh, on, |c| inr(c, 5.0 * h, 6.0 * h, 2.0 * h, 6.0 * h));
    let ground = faces_where(&mesh, on, |c| !inr(c, h, 7.0 * h, h, 7.0 * h));
    let (a, f, g) = (nodes_of(&driven), nodes_of(&feed), nodes_of(&ground));
    assert!(a.iter().all(|v| !f.contains(v) && !g.contains(v)));
    assert!(f.iter().all(|v| !g.contains(v)));
    let eps = mesh
        .tets
        .iter()
        .map(|t| {
            let z: f64 = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
            if z < 0.5 { 10.0 } else { 1.0 }
        })
        .collect();
    Problem {
        mesh,
        eps,
        driven,
        floating: vec![feed],
        ground,
    }
}

#[test]
fn coplanar_island_lower_bounds_stay_below_every_upper_bound() {
    let coarse = coplanar();
    let fine = coarse.red_refined();
    let mut lowers = Vec::new();
    let mut uppers = Vec::new();
    let mut eq_p2 = Vec::new();
    for pb in [&coarse, &fine] {
        for order in [1, 2] {
            let (c, b) = pb.equilibrated(order, 1);
            // P1 on this coarse mesh is about 50 % off; the bound inherits it.
            assert!(
                b.c_lower_f > 0.4 * c,
                "P{order}: bound {} vs {c}",
                b.c_lower_f
            );
            uppers.push(c);
            lowers.push(b.c_lower_f);
            if order == 2 {
                eq_p2.push((b.c_lower_f, c));
            }
            // Floating neutrality survives the equilibration.
            assert!(b.charges[1].abs() < 1e-8 * b.charges[0]);
        }
        let (rep, b) = pb.crouzeix_raviart();
        assert!((b.c_lower_f / rep.c_cr_f - 1.0).abs() < 1e-8);
        lowers.push(b.c_lower_f);
    }
    let max_lower = lowers.iter().copied().fold(0.0, f64::max);
    let min_upper = uppers.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(max_lower < min_upper, "{lowers:?} vs {uppers:?}");
    // The P2-equilibrated bracket shrinks under refinement.
    let w = |(l, u): (f64, f64)| u - l;
    assert!(w(eq_p2[1]) < 0.75 * w(eq_p2[0]), "{eq_p2:?}");
}

/// The stacked parallel plate of
/// [`parallel_plate_bracket_collapses_on_the_exact_value`]: exact for
/// every discretization, so a violated bracket is unambiguous.
fn stacked_plate() -> Problem {
    let mesh = cube_tet_mesh(4, 1.0);
    let tol = 1e-12;
    let plane = |z: f64| move |p: &[f64; 3]| (p[2] - z).abs() < tol;
    let all = |_: &[f64; 3]| true;
    let eps: Vec<f64> = mesh
        .tets
        .iter()
        .map(|t| {
            let z: f64 = t.iter().map(|&v| mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
            if z < 0.5 { 4.0 } else { 1.0 }
        })
        .collect();
    Problem {
        driven: faces_where(&mesh, plane(1.0), all),
        floating: vec![faces_where(&mesh, plane(0.5), all)],
        ground: faces_where(&mesh, plane(0.0), all),
        eps,
        mesh,
    }
}

/// Mutation: a flux that is admissible for the *grounded*-feedline
/// problem carries a charge on the floating sheet. The certificate pays
/// for it (here the bound drops to zero); with the `|Q_f|` term dropped
/// it certifies the grounded `C_ii = 2ε₀` as a lower bound on the
/// floating `C_Σ = 1.6ε₀`, above the exact (and conforming) value.
#[test]
fn mutation_ignoring_the_floating_charge_violates_the_bracket() {
    let pb = stacked_plate();
    let (c_floating, _) = pb.primal(2);
    let exact = EPS_0 / (0.5 / 4.0 + 0.5 / 1.0);
    assert!((c_floating / exact - 1.0).abs() < 1e-9);
    let grounded = Problem {
        mesh: pb.mesh.clone(),
        eps: pb.eps.clone(),
        driven: pb.driven.clone(),
        floating: Vec::new(),
        ground: pb.ground.iter().chain(&pb.floating[0]).copied().collect(),
    };
    let (c_grounded, phi) = grounded.primal(2);
    assert!((c_grounded / (2.0 * EPS_0) - 1.0).abs() < 1e-9);
    let topo = pb.topology();
    let eps = Permittivity::Scalar(&pb.eps);
    let flux = equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P2(&phi), 1).unwrap();
    let honest = certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::None).unwrap();
    let broken = certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::IgnoreFloatingCharge).unwrap();
    assert!(honest.c_lower_f <= c_floating);
    assert!((broken.c_lower_f / c_grounded - 1.0).abs() < 1e-8);
    assert!(
        broken.c_lower_f > 1.2 * c_floating,
        "{} vs {c_floating}",
        broken.c_lower_f
    );
}

/// Mutation: add to an admissible flux a constant field pointing away
/// from the island in every substrate tetrahedron under it. That buys
/// island charge without paying for it in energy at the island face, but
/// the field leaves through the three other faces of each tetrahedron as
/// normal jumps. The honest certificate charges those jumps; with the
/// jump, boundary and divergence terms dropped, the "bound" approaches the
/// thin-slab value `ε A² / Σ(|T|/ε)`, above the conforming upper bound.
#[test]
fn mutation_ignoring_flux_jumps_violates_the_bracket() {
    let pb = coplanar();
    let (c_p2, phi) = pb.primal(2);
    let topo = pb.topology();
    let eps = Permittivity::Scalar(&pb.eps);
    let mut flux = equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P2(&phi), 1).unwrap();
    let dim = RtFlux::dim(1);
    for tri in &pb.driven {
        let f = topo.faces.binary_search(&sorted3(*tri)).unwrap();
        let p = tri.map(|v| pb.mesh.nodes[v as usize]);
        let (n, _) = unit_normal(&p);
        for &t in &topo.face_tets[f] {
            let t = t as usize;
            if pb.eps[t] < 2.0 {
                continue; // substrate side only
            }
            let opp = pb.mesh.tets[t].iter().find(|v| !tri.contains(v)).unwrap();
            let side = dot(n, sub(pb.mesh.nodes[*opp as usize], p[0])).signum();
            // Constant monomial, directions 0..3, come first in RT_k.
            for d in 0..3 {
                flux.coeffs[t * dim + d] += 1e3 * side * n[d];
            }
        }
    }
    let honest = certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::None).unwrap();
    let broken = certify_impl(&pb.mesh, &topo, eps, &flux, Mutation::IgnoreSpurious).unwrap();
    assert!(honest.c_lower_f < c_p2);
    // The bought charge reappears, at least in full, as jump penalty.
    let clean = certify_lower_bound(
        &pb.mesh,
        &topo,
        eps,
        &equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P2(&phi), 1).unwrap(),
    )
    .unwrap();
    let bought = honest.charges[0] - clean.charges[0];
    assert!(bought > 5.0 * clean.charges[0]);
    assert!(honest.interior_jump_l1 >= bought);
    assert!(
        broken.c_lower_f > 1.5 * c_p2,
        "{} vs {c_p2}",
        broken.c_lower_f
    );
}

/// A tensor `ε` that is the scalar one times the identity reproduces the
/// scalar bound, and a genuinely anisotropic tensor still certifies a
/// bound below the tensor-`ε` P1 conforming value.
#[test]
fn tensor_permittivity_matches_scalar_and_stays_below_the_tensor_primal() {
    use crate::assembly::electrostatic::assemble_electrostatic_tensor;
    let pb = coplanar();
    let topo = pb.topology();
    let tens: Vec<[[f64; 3]; 3]> = pb.eps.iter().map(|&e| diag(e)).collect();
    let (fs, _) =
        crouzeix_raviart_flux(&pb.mesh, &topo, Permittivity::Scalar(&pb.eps), tight()).unwrap();
    let (ft, _) =
        crouzeix_raviart_flux(&pb.mesh, &topo, Permittivity::Tensor(&tens), tight()).unwrap();
    let bs = certify_lower_bound(&pb.mesh, &topo, Permittivity::Scalar(&pb.eps), &fs).unwrap();
    let bt = certify_lower_bound(&pb.mesh, &topo, Permittivity::Tensor(&tens), &ft).unwrap();
    assert!((bs.c_lower_f / bt.c_lower_f - 1.0).abs() < 1e-9);
    // Anisotropic substrate (xy-coupled, SPD).
    let aniso: Vec<[[f64; 3]; 3]> = pb
        .eps
        .iter()
        .map(|&e| {
            if e > 1.0 {
                [[9.3, 0.8, 0.0], [0.8, 9.7, 0.0], [0.0, 0.0, 11.5]]
            } else {
                diag(1.0)
            }
        })
        .collect();
    let electrodes = vec![
        Electrode {
            name: "driven".into(),
            nodes: nodes_of(&pb.driven),
            voltage: 1.0,
        },
        Electrode {
            name: "feed".into(),
            nodes: nodes_of(&pb.floating[0]),
            voltage: 0.0,
        },
    ];
    let ground = nodes_of(&pb.ground);
    let rho = vec![0.0; pb.mesh.n_tets()];
    let sys = assemble_electrostatic_tensor(&pb.mesh, &aniso, &rho, &electrodes, &ground).unwrap();
    let (cm, _, phis) =
        extract_capacitance_amg_pcg_fields(&sys, &electrodes, &ground, tight()).unwrap();
    let v = floating_potentials(&cm.c).unwrap();
    let c = cm.c[0][0] + v[0] * cm.c[0][1];
    let phi: Vec<f64> = phis[0]
        .iter()
        .zip(&phis[1])
        .map(|(a, b)| a + v[0] * b)
        .collect();
    let eps = Permittivity::Tensor(&aniso);
    let flux = equilibrated_flux(&pb.mesh, &topo, eps, PrimalPotential::P1(&phi), 1).unwrap();
    let b = certify_lower_bound(&pb.mesh, &topo, eps, &flux).unwrap();
    let (fc, _) = crouzeix_raviart_flux(&pb.mesh, &topo, eps, tight()).unwrap();
    let bc = certify_lower_bound(&pb.mesh, &topo, eps, &fc).unwrap();
    assert!(b.c_lower_f < c && bc.c_lower_f < c);
    assert!(b.c_lower_f > 0.4 * c);
    assert!(b.charges[1].abs() < 1e-8 * b.charges[0]);
}

/// Issue #1091: the tensor P2 bracket on the stacked parallel plate
/// collapses onto the closed form. The field is normal to the plates, so
/// only `ε_zz` enters, `C = ε₀ / (½/ε_zz,lower + ½/ε_zz,upper)`; in-plane
/// anisotropy and `xy` coupling must drop out exactly, and the floating
/// middle plate must stay neutral.
#[test]
fn tensor_p2_stacked_plate_bracket_collapses_on_the_closed_form_1091() {
    let pb = stacked_plate();
    for (lower_zz, upper_zz) in [(4.0, 1.0), (11.5, 1.0), (11.5, 2.5)] {
        let tensor: Vec<[[f64; 3]; 3]> = pb
            .eps
            .iter()
            .map(|&e| {
                if e > 1.0 {
                    [[9.3, 0.8, 0.0], [0.8, 9.7, 0.0], [0.0, 0.0, lower_zz]]
                } else {
                    [[1.0, 0.2, 0.0], [0.2, 1.3, 0.0], [0.0, 0.0, upper_zz]]
                }
            })
            .collect();
        let exact = EPS_0 / (0.5 / lower_zz + 0.5 / upper_zz);
        for k in [1, 2] {
            let (c, b) = pb.equilibrated_with(2, k, Some(&tensor));
            assert!((c / exact - 1.0).abs() < 1e-9, "P2 {c} vs {exact}");
            assert!(
                (b.c_lower_f / exact - 1.0).abs() < 1e-8,
                "RT{k}: {} vs {exact}",
                b.c_lower_f
            );
            assert!(b.c_lower_f <= c * (1.0 + 1e-12));
            assert!(b.charges[1].abs() < 1e-8 * b.charges[0]);
        }
    }
}

/// Issue #1091: a full SPD tensor `ε = s(x)·L Lᵀ` (`s` the scalar
/// substrate/vacuum map, `L` lower triangular with every off-diagonal
/// set) on a mesh `M` is the scalar problem `s(x)` on the mapped mesh
/// `L⁻¹M`, scaled by `det L`: with `x' = L⁻¹x`,
/// `∇φ·ε∇φ dx = s |∇'φ|² det L dx'`. The map preserves the P2 space,
/// the sheets and the natural boundary, so the tensor P2 capacitance
/// matches to solver precision, and so does the certified lower bound
/// (the Raviart–Thomas patch problems are equivariant under the Piola
/// map; only the `L¹` penalty estimates, about `1e-9` of the charge, are
/// not). Every tensor entry of the P2 kernel, of the patch weights and
/// divergence data, and of the certificate's `ε⁻¹` is exercised.
#[test]
fn tensor_bracket_is_the_scalar_bracket_of_the_mapped_mesh_1091() {
    let pb = coplanar();
    let l = [[1.1, 0.0, 0.0], [0.3, 0.9, 0.0], [-0.2, 0.25, 1.2]];
    let llt: [[f64; 3]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| l[i][k] * l[j][k]).sum()));
    let det_l = l[0][0] * l[1][1] * l[2][2];
    let tensor: Vec<[[f64; 3]; 3]> = pb
        .eps
        .iter()
        .map(|&s| llt.map(|row| row.map(|x| s * x)))
        .collect();
    let map = |x: &[f64; 3]| {
        let y0 = x[0] / l[0][0];
        let y1 = (x[1] - l[1][0] * y0) / l[1][1];
        let y2 = (x[2] - l[2][0] * y0 - l[2][1] * y1) / l[2][2];
        [y0, y1, y2]
    };
    let mut mesh = pb.mesh.clone();
    mesh.nodes = pb.mesh.nodes.iter().map(map).collect();
    let mapped = Problem {
        mesh,
        eps: pb.eps.clone(),
        driven: pb.driven.clone(),
        floating: pb.floating.clone(),
        ground: pb.ground.clone(),
    };
    for k in [1, 2] {
        let (ct, bt) = pb.equilibrated_with(2, k, Some(&tensor));
        let (cs, bs) = mapped.equilibrated(2, k);
        assert!(
            (ct / (det_l * cs) - 1.0).abs() < 1e-9,
            "P2: tensor {ct} vs mapped scalar {}",
            det_l * cs
        );
        assert!(
            (bt.c_lower_f / (det_l * bs.c_lower_f) - 1.0).abs() < 1e-6,
            "RT{k}: tensor {} vs mapped scalar {}",
            bt.c_lower_f,
            det_l * bs.c_lower_f
        );
        assert!(
            bt.c_lower_f < ct && bt.c_lower_f > 0.4 * ct,
            "{} vs {ct}",
            bt.c_lower_f
        );
    }
    // The P1 tensor path and the CR dual map the same way.
    let (c1t, _) = pb.primal_with(1, Some(&tensor));
    let (c1s, _) = mapped.primal(1);
    assert!((c1t / (det_l * c1s) - 1.0).abs() < 1e-9);
    let topo = pb.topology();
    let (fc, _) =
        crouzeix_raviart_flux(&pb.mesh, &topo, Permittivity::Tensor(&tensor), tight()).unwrap();
    let bc = certify_lower_bound(&pb.mesh, &topo, Permittivity::Tensor(&tensor), &fc).unwrap();
    let (_, bcs) = mapped.crouzeix_raviart();
    assert!((bc.c_lower_f / (det_l * bcs.c_lower_f) - 1.0).abs() < 1e-6);
}

/// Issue #1091: the uniaxial sapphire-like substrate `diag(9.3, 9.3,
/// 11.5)` lies between `9.3·I` and `11.5·I` in the Loewner order, and the
/// capacitance is monotone in `ε`, so its certified tensor bracket must
/// be consistent with the scalar brackets at 9.3 and 11.5; it must also
/// stay a bracket under refinement and shrink. An isotropic tensor
/// reproduces the scalar P2 bracket.
#[test]
fn tensor_p2_bracket_is_ordered_between_the_scalar_extremes_1091() {
    let coarse = coplanar();
    let iso: Vec<[[f64; 3]; 3]> = coarse.eps.iter().map(|&e| diag(e)).collect();
    let (cs, bs) = coarse.equilibrated(2, 1);
    let (ci, bi) = coarse.equilibrated_with(2, 1, Some(&iso));
    assert!((ci / cs - 1.0).abs() < 1e-12, "{ci} vs {cs}");
    assert!((bi.c_lower_f / bs.c_lower_f - 1.0).abs() < 1e-10);
    let with_sub = |pb: &Problem, sub: f64| Problem {
        mesh: pb.mesh.clone(),
        eps: pb
            .eps
            .iter()
            .map(|&e| if e > 1.0 { sub } else { 1.0 })
            .collect(),
        driven: pb.driven.clone(),
        floating: pb.floating.clone(),
        ground: pb.ground.clone(),
    };
    let mut widths = Vec::new();
    for pb in [coarse.red_refined(), coarse] {
        let sapphire: Vec<[[f64; 3]; 3]> = pb
            .eps
            .iter()
            .map(|&e| {
                if e > 1.0 {
                    [[9.3, 0.0, 0.0], [0.0, 9.3, 0.0], [0.0, 0.0, 11.5]]
                } else {
                    diag(1.0)
                }
            })
            .collect();
        let (ct, b1) = pb.equilibrated_with(2, 1, Some(&sapphire));
        let (_, b2) = pb.equilibrated_with(2, 2, Some(&sapphire));
        let lt = b1.c_lower_f.max(b2.c_lower_f);
        assert!(lt < ct, "{lt} vs {ct}");
        let (c_lo, b_lo) = with_sub(&pb, 9.3).equilibrated(2, 2);
        let (c_hi, b_hi) = with_sub(&pb, 11.5).equilibrated(2, 2);
        // C(9.3) <= C_tensor <= C(11.5), checked on certified sides,
        // and on the conforming values of one space (the discrete
        // minimum is monotone in ε as well).
        assert!(b_lo.c_lower_f < ct && lt < c_hi, "ordering");
        assert!(c_lo < ct && ct < c_hi, "{c_lo} {ct} {c_hi}");
        assert!(b_hi.c_lower_f < c_hi);
        widths.push(ct - lt);
    }
    assert!(widths[0] < 0.75 * widths[1], "{widths:?}");
}

#[test]
fn topology_rejects_bad_conductor_sets() {
    let mesh = cube_tet_mesh(2, 1.0);
    let tri = sorted3(LOCAL_FACES[0].map(|l| mesh.tets[0][l]));
    let c = |role, triangles: Vec<[u32; 3]>| DualConductor {
        name: "c".into(),
        role,
        triangles,
    };
    let only_driven = [c(ConductorRole::Driven, vec![tri])];
    assert!(FaceTopology::new(&mesh, &only_driven).is_err());
    let two_driven = [
        c(ConductorRole::Driven, vec![tri]),
        c(ConductorRole::Driven, vec![tri]),
        c(ConductorRole::Ground, vec![tri]),
    ];
    assert!(FaceTopology::new(&mesh, &two_driven).is_err());
    let shared = [
        c(ConductorRole::Driven, vec![tri]),
        c(ConductorRole::Ground, vec![tri]),
    ];
    assert!(FaceTopology::new(&mesh, &shared).is_err());
    let not_a_face = [
        c(ConductorRole::Driven, vec![[0, 1, 26]]),
        c(ConductorRole::Ground, vec![tri]),
    ];
    assert!(FaceTopology::new(&mesh, &not_a_face).is_err());
}
