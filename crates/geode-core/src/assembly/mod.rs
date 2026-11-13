//! Global system assembly on a tetrahedral mesh.
//!
//! This group gathers the routines that turn element-local matrices and
//! the mesh connectivity into global linear systems ready for the
//! eigen / driven solvers:
//!
//! - [`electrostatic`]: 3-D **scalar** electrostatic Poisson assembly on a
//!   [`TetMesh`](crate::mesh::TetMesh) — host-side `f64` per-element
//!   ε-weighted stiffness, multi-conductor Dirichlet electrode BCs with the
//!   full unconstrained `K` retained, the N×N Maxwell capacitance-matrix
//!   extraction (energy method + surface-flux cross-check), `E = −∇φ`
//!   recovery, and field-energy postprocessing (Epic #475, Palace
//!   `Electrostatic` parity).
//! - [`conduction`]: steady DC current conduction `∇·(σ∇φ) = 0` on a
//!   [`TetMesh`](crate::mesh::TetMesh) — per-tet `σ`, Dirichlet or
//!   lumped-resistance terminals, the terminal conductance matrix, `J`,
//!   Joule density and power balance, Jacobi-scaled LU for 12+ decades of
//!   contrast (issue #1476).
//! - [`thermal`]: steady heat conduction `∇·(k∇T) = -q` on a tet mesh with
//!   fixed-T nodes, per-tet flux and energy balance, plus a relaxed Picard
//!   Joule self-heating loop `σ(T)` ↔ `k(T)` with runaway detection
//!   (issue #1556).
//! - [`eqs_transient`]: electro-quasi-static `∇·(σ∇φ + ∂ₜ(ε∇φ)) = 0` —
//!   θ-method time stepping with driven terminals and floating conductors
//!   (conduction / displacement current split, region fields, charge
//!   residual) and the small-signal admittance `Y = G + jωC` (issue #1478).
//! - [`electrostatic_2d`]: the planar (`TriMesh`, P1) sibling of
//!   [`electrostatic`] with the same contract — per-triangle `ε_r`/`ρ`,
//!   electrode node sets, `Result` errors, `E = −∇φ` recovery and the
//!   per-unit-length Maxwell capacitance matrix (issue #1362).
//! - [`electrostatic_dual`]: guaranteed **lower** bounds on a capacitance
//!   from the complementary (flux, `H(div)`) principle — a certificate for
//!   any piecewise-polynomial flux, Raviart–Thomas patch equilibration of a
//!   P1/P2 potential, and a Crouzeix–Raviart (RT0 mixed) dual solve, with
//!   floating conductors and scalar or tensor ε (issue #1062).
//! - [`magnetostatic`]: 2-D **scalar** magnetostatic Poisson assembly on a
//!   [`TriMesh`](crate::analytic::waveguide::TriMesh) — the node-indexed
//!   ν-weighted stiffness, consistent-mass current RHS, symmetric Dirichlet
//!   elimination, and piecewise-constant `B`-field recovery for the
//!   straight-wire oracle (Epic #448 Phase 1).
//! - [`magnetostatic_nonlinear`]: the **nonlinear** (B-H) sibling of
//!   [`magnetostatic`] in SI units — the [`ReluctivityLaw`](magnetostatic_nonlinear::ReluctivityLaw)
//!   trait with Brauer and monotone-table laws, the consistent-tangent
//!   damped Newton solve, and the converged tangent for the adjoint
//!   (motor-paper milestone M1, issue #1157).
//! - [`magnetostatic_sector`]: periodic / antiperiodic sector pairing and the
//!   conforming sliding band for rotor rotation, as signed node links on the
//!   nonlinear system (motor-paper milestone M2, issue #1161).
//! - [`magnetostatic_shape`]: the discrete shape adjoint of the nonlinear
//!   sector torque (Arkkio and harmonic coupling torque, mean and ripple
//!   objectives) with respect to the node coordinates (motor-paper
//!   milestone M3, issue #1174).
//! - [`magnetostatic3d`]: 3-D **vector** magnetostatic assembly on a
//!   [`TetMesh`](crate::mesh::TetMesh) with lowest-order Nédélec edge
//!   elements — host-side `f64` ν-weighted curl-curl (retained full `K`),
//!   edge-DOF current RHS with a discrete-solenoidality check, tree-cotree
//!   gauged faer LU solve, `B = ∇×A` recovery, and the N×N Maxwell
//!   inductance-matrix extraction (energy method + flux-linkage cross-check;
//!   Epic #475, Palace `Magnetostatic` parity — the dual of
//!   [`electrostatic`]).
//! - [`p1`]: global P1 (nodal) stiffness/mass assembly, the
//!   [`SparsityPattern`](p1::SparsityPattern) side-output, and the
//!   mesh-upload helpers.
//! - [`nedelec`]: global first-order Nédélec (edge) assembly — the
//!   `assemble_global_nedelec*` family, the anisotropic / complex / PML
//!   epsilon builders, current-RHS and σ-damping assembly, and the
//!   edge scatter maps.
//! - [`hcurl_space`]: the order-pluggable H(curl) space
//!   [`HcurlSpace`](hcurl_space::HcurlSpace) (issue #838, Epic #836) — one
//!   object owning the DOF layout, orientation, entity→DOF maps, the
//!   face-exact PEC mask and field evaluation at p=1 (Whitney, the pre-#838
//!   tables bit for bit) and p=2.
//! - [`fe`]: the high-level [`fe_assemble`](fe::fe_assemble) operator
//!   that selects between the P1 and Nédélec pipelines via
//!   [`ElementType`](fe::ElementType) and applies Dirichlet BCs.
//! - [`sparse`]: the dense → CSR/CSC projection of an assembled
//!   [`GlobalSystem`](p1::GlobalSystem) into faer's sparse form, with
//!   optional Dirichlet reduction.
//! - [`surface`]: Silver–Müller and surface-mass boundary assembly on
//!   the mesh's exterior triangle faces.
//! - [`surface_p2`]: the p=2 (second-order Nédélec) tangential-trace
//!   surface kernel — surface mass and port flux over the 8 trace DOFs of a
//!   boundary triangle (issue #857, Epic #836 Phase 1b).
//! - [`torque`]: electromagnetic torque extraction from a per-triangle
//!   air-gap flux density — the Maxwell stress-tensor line integral and
//!   Arkkio's volume-averaged variant (Epic #448 Phase 3), with the shared
//!   contour/triangle-location sampler they build on.

pub mod conduction;
pub mod current_path;
mod dense_scatter;
pub mod electrostatic;
pub mod electrostatic_2d;
pub mod electrostatic_dual;
pub mod eqs_transient;
pub mod fe;
pub mod hcurl_space;
pub mod magnetostatic;
pub mod magnetostatic3d;
pub mod magnetostatic_nonlinear;
pub mod magnetostatic_sector;
pub mod magnetostatic_shape;
pub mod nedelec;
pub mod nedelec_matvec;
pub mod nedelec_p2;
pub mod p1;
pub mod periodic;
pub mod sparse;
pub mod surface;
pub mod surface_p2;
pub mod thermal;
pub mod torque;
