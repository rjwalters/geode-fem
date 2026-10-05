# Changelog

All notable changes to GEODE-FEM are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

**Breaking (`geode-core` library):**
- `DrivenSolution` gains an `order: ElementOrder` field, and `DrivenError` gains `UnsupportedAtOrder` and `SpaceMeshMismatch` (#838). Code that builds `DrivenSolution` literals or matches `DrivenError` exhaustively must be updated.
- `PeriodicPairReport` gains `translation_fit_pairs` and `translation_residual` (#856). Code that builds it as a struct literal must be updated.

### Added

#### `geode-core`

- **Conforming tet refinement by newest-vertex bisection** (#860, Epic #835 Phase 2). The new `adapt::refine` module refines a `TaggedTetMesh` with Arnold–Mukherjee–Pouly bisection. Marked tets are bisected once, and a recursive closure restores conformity. The algorithm works from any conforming mesh (longest-edge initial marking, strict global tie-break). `BisectionMesh` keeps the marking state between cycles, so the similarity-class bound holds over repeated adaptation; `refine()` is the one-shot form.
  - **Tags are preserved.** Children inherit the 3-D tag. Tagged triangles (ports, PEC, impedance, interfaces) split exactly as their faces do, keeping their tag and winding. `physical_groups` is copied unchanged. Old nodes keep their indices, and new nodes are appended. Planar port faces stay bit-exactly planar, so `project_port_face` and the wave/hybrid port face meshes keep working.
  - **Exact nested prolongations:**
    - `Refined::edge_prolongation` (Whitney p=1), computed from exact dyadic barycentrics with no geometry;
    - `node_prolongation` (P1);
    - `hcurl_prolongation(coarse, P2)` (p=2, by exact local L² projection over `HcurlSpace`).

    Every shared row is computed from each adjacent fine tet and checked for agreement, so a broken DOF map is a typed error.
  - **Periodic meshes.** `BisectionMesh::new_periodic` mirrors refinement across `PeriodicMap::paired_face` partners, including chained corners. After every step it re-matches the refined pairs with #839's `PeriodicMap::build`, which is the post-refinement gate. A failed or snapped match is reported as a refinement bug (`RefineError::PeriodicGate` / `Internal`), never as a user error.
  - `mesh_quality` reports the min/max dihedral angle and the max aspect ratio. `Refined::stats` reports the closure ratio (bisections per marked tet). A safety cap turns a runaway closure into `RefineError::ClosureCap`.
  - New test target: `adapt_refine`. Measured results:
    - Three uniform levels of `cube_tet_mesh(n)` reproduce the node, tet, edge and face counts of `cube_tet_mesh(2n)` exactly (n = 1, 2, 3; six levels give `4n`), so the p=1 / p=2 DOF counts are known in closed form.
    - `Pᵀ K_h P = K_H` and `Pᵀ M_h P = M_H` hold to ≤ 2e-14 relative Frobenius error, at p=1 and p=2. This was checked on the Kuhn cube, on the non-Kuhn 5-tet cube and on the gmsh spiral smoke fixture (10.7k → 44.5k tets).
    - Interior faces have exactly two tets, and the Euler characteristic is unchanged. Volume per 3-D group and area per 2-D group are conserved to 1e-12 on the spiral fixture.
    - A generic tet's descendants fall into 36 similarity classes, all reached by level 6, with none new up to level 15.
    - A renumbered, triply periodic cube stays matched through five random cycles: #839's matcher accepts every mesh with zero snaps. A non-mirrored refinement trips the matcher.
    - Refining the middle of a straight wave-port guide leaves β bit-identical and moves S by 3.8e-3. Refining the port face itself (64 → 256 triangles) keeps it planar and moves β toward the analytic value.
  - **Honest limits:**
    - The first bisections of a regular tet cost dihedral angle. On the 5-tet cube the minimum drops from 54.7° to 25.2° (0.46×) and then stays constant over eight cycles. The epic's "≥ 0.5× the initial minimum" bar holds on the Kuhn cube (unchanged at 45°) but not on the 5-tet split.
    - The closure cascades on gmsh meshes: 5% random marks on the spiral fixture give 11–25 bisections per marked tet.
    - Curved boundaries are not followed, because midpoints stay on the chord.
- **Port-mode sensitivities on the 2-D hybrid port pencil** (#859, Epic #841 Phase 3a). The new `analytic::port_mode_sensitivity` module differentiates a hybrid port mode with respect to face design parameters. Every gradient is FD-validated through the shipped 2-D forward. The mode can come from the lossless solver (`solve_hybrid_port_modes`) or the lossy one (`solve_lossy_hybrid_port_modes`).
  - **Quantities.** `∂β²`, `∂β` (and so `∂α`), `∂ε_eff`, and the line impedances `∂Z_PI` / `∂Z_PV` / `∂Z_VI`. The impedances use the same definitions as the hybrid wave port's line reports. You also get the derivative of the normalized mode itself (`HybridModeDerivative::tangent`), and a reverse-mode `vjp` for any linear functional of the mode. Phase 3b will compose these into the 3-D S gradient.
  - **Parameters.** Per-region `ε′`, `ε″`, `tan δ` and `ε_r` at fixed `tan δ` (`FaceDesign`, bound to named triangle groups). Face geometry is node motion bound to named node groups: `push_group_motion` translates the groups rigidly with pinned groups and fills in the rest of the face by harmonic extension. `strip_face_groups` names the groups of a `ShieldedStripFace`, so strip width `w` and substrate height `h` are one call each.
  - **Method.** `∂β²` is the unconjugated Hellmann–Feynman quotient, `zᵀ(∂A − μ∂B)z / zᵀBz` with `zᵀBz = β²`. The eigenvector terms come from one bordered solve per observable, `[[A − μB, Bz], [(Bz)ᵀ, 0]]`. Shape derivatives use exact dual-number element kernels.
  - **Measured.** On a shielded microstrip (ε_r = 4.4, w = h = 1, k₀h = 0.1, 8h box), central FD agrees with the adjoint to 1e-8 or better on every parameter × quantity, on both the lossless and the lossy (`tan δ = 0.02`) faces.
    - `∂α/∂tan δ` is within 1.1 % of Pozar's quasi-TEM `α_d/tan δ`.
    - Against the differentiated Hammerstad–Jensen closed forms (k₀h = 0.01, 20h box, ε_r ∈ {4.4, 9.8}, w/h ∈ {0.5, 1, 2}), `∂Z₀/∂w` is within 3.4 % and `∂ε_eff/∂w` within 5.5 %.
  - **Degenerate clusters.** A mode in an exactly degenerate cluster, such as the TEM pair of a homogeneous two-strip line, is a typed `DegenerateCluster` error. It never gets a per-mode derivative. `cluster_sensitivity` returns the cluster invariants instead: `∂(mean β²)` matches FD to 1e-9, and the restricted derivative matrix's eigenvalues match the one-sided split rates. Every result reports the relative gap to the nearest mode, including complex-pair members of a lossless set (`NearestMode::Mode` / `NearestMode::ComplexPair`), with a `NearDegenerate` warning below 1e-3.
  - **Mutation tripwires**, each caught by FD:
    - dropping the eigenvector term makes `∂Z_PI` fail;
    - dropping `−μ∂B` makes `∂β²` fail, for shape parameters and on a hybrid mode for ε. On the quasi-TEM mode the ε part of that term is only about 1e-4 relative, because it is O(E_z²), so the main golden holds `∂β²/∂ε` to 1e-6 (measured 3.9e-9) and catches that drop too;
    - dropping the geometric kernel makes `∂β²/∂w,h` fail.
  - **Lossy forward parity.** The module's lossy line evaluator (power as `zᵀBz`) is pinned to the shipped `driven::ports::line_complex` readout to ≤ 1e-9 at tan δ = 0.05 (`Z_PI` / `Z_PV` / `Z_VI`), and the lossy FD golden runs through the shipped readout.
  - New test target: `port_mode_sensitivity`.
- **A-posteriori error estimator for H(curl)** (#840, Epic #835 Phase 1). The new `adapt::estimator::estimate_hcurl` estimates the error of a driven or eigen solution with an explicit residual estimator. It takes the solution's `HcurlSpace`, the DOF vector, `k²` and per-tet `ε` / `ν`. It returns:
  - a per-tet indicator `η_T²` and a per-tet breakdown (volume, divergence, tangential jump, normal jump, natural boundary, plus a reserved `p_surplus` that stays 0 until #836 Phase 5a);
  - the global `η` and the unit-invariant `eta_rel = η / ‖E_h‖_E`, with an empirical effectivity range (`VALIDATED_EFFECTIVITY`, θ ∈ [3.5, 7.0]) and `error_bracket()`;
  - points per wavelength, with a `pre_asymptotic` flag below 6;
  - a coverage report. It counts Leontovich, Silver-Müller, port and UPML regions by kind instead of silently ignoring them.

  Material interfaces need no special case, because jumps are taken between per-tet coefficients. Faces paired by the mesh's `PeriodicMap` (#839, `EstimatorInput::with_periodic_map`) are treated as interior on the torus. A lower-level `paired_face` closure takes Floquet phases. `write_estimate_vtu` exports the indicators as VTU cell data, so you can see in ParaView where to refine.
  - The estimator is order-generic: a p=1 field injected into a p=2 space gives the same η to round-off. Only p=1 is validated.
  - On the 133k-tet `transmon_smoke` fixture the estimator takes 0.49 s, against 1.39 s to assemble the pencil.
  - New test target: `hcurl_error_estimator`. Measured effectivity θ = η / ‖E − E_h‖_E:
    - driven manufactured cube: 6.50–6.85 over n = 4, 8, 16, and η converges at rate 0.95;
    - PEC box cavity: 6.22–6.36;
    - a gradient error at an ε = 4 : 1 interface: 3.72–3.92.
  - Every term and weight has a test that fails if it is removed or mis-weighted. The checks cover each of the five terms, the ½ on interior faces, `1/ν`, and the smaller coefficient taken at interfaces (ε and ν). They include a closed-form golden with ν ≠ 1, complex and anisotropic ε, complex `k²` and `∇·f ≠ 0`.
  - `summary()` prints the error bracket only on an asymptotic mesh. On a pre-asymptotic mesh it says the bracket does not apply and gives the resolution to refine to. When coverage is incomplete, it says the estimate omits the uncovered boundary terms.
- **`adapt::goal::GoalFunctional`**, the goal-functional trait shared with the differentiable-EDA epic (#841). It provides `value`, `rhs` (the adjoint load, with no assumed symmetry) and `is_linear`. `LinearGoal` is a reference implementation. Goal-oriented (DWR) estimation itself is #835 Phase 4.
- **Order-pluggable H(curl) space** (#838, Epic #836 Phase 1a). `assembly::hcurl_space::HcurlSpace::build(mesh, order)` owns the DOF layout. It provides:
  - per-entity DOF counts;
  - the per-tet gather and its orientation;
  - `edge_dofs` / `face_dofs`;
  - a face-exact PEC mask that never eliminates chords. It returns `Result`: a wall triangle that is not a mesh face is `DrivenError::SurfaceNotOnMesh` (named `"PEC wall {i}"`), and a space used with another mesh is `SpaceMeshMismatch`;
  - `field_at` / `curl_at`;
  - the hierarchical `prolong_p1`;
  - `face_dof_transform`, the 2×2 face-DOF relabelling map that periodic pairing needs.

  There is now one crate-level `elements::ElementOrder`, re-exported at `driven::solve::ElementOrder` and `eigen::cavity::ElementOrder`.
- **p=2 driven operator.** `DrivenOperator::assemble_with_space` assembles the production driven operator on a p=2 space. It supports complex scalar, diagonal-tensor and matched-UPML ε (with ν), σ damping, volume J (constant, degree-2 quadrature samples, or a closure sampled at the degree-4 rule), and tagged face-exact PEC. Direct LU, assembled-matrix COCG (Jacobi / ILU(0) / Chebyshev) and the adaptive PROM all work on it.
  - On flat-sided unstructured boxes (n = 2..8), the fitted field L² slope is 1.92 at p=2 (gated at ≥ 1.85; the finest-step local slope is 2.00) against 0.99–1.16 at p=1, and p=2 is 4.4–5.9× more accurate on the coarsest mesh.
  - With a p=1 space, the call routes to the existing assembly verbatim, and the outputs are bit-identical.
- **p=2 surface terms: lumped ports, Leontovich / rough / London walls and Silver-Müller** (#857, Epic #836 Phase 1b). `DrivenOperator::assemble_with_space` now accepts lumped ports and every `SurfaceImpedanceModel` on a p=2 space, through the new tangential-trace kernel `assembly::surface_p2`. The kernel covers the 8 trace DOFs of a boundary triangle (3 edges × `(W, Q)` + the face pair `(φ0, φ1)`) in the ascending-vertex convention, so they are the space's `edge_dofs` / `face_dofs` with unit sign. It provides the surface mass, the port flux functional (drive and voltage readout) and `p2_trace_dofs`. The dense sweep, `s_parameter_point` and the adaptive PROM run on these operators unchanged. Wave and hybrid ports stay p=1 (Phase 3).
  - Trace identity (new target `surface_p2_trace`): the tet p=2 basis restricted to a boundary face equals the kernel to 1.8e-15, and the off-face functions have no trace (6e-16). The kernel mass equals `tri_nedelec2_local` laid flat to 6e-16, `xᵀSx = ∫|E_t|²` from `field_at` to 3e-15, and a prolonged p=1 field keeps its p=1 surface energy and flux.
  - Goldens on a jittered TEM parallel-plate line, every termination an exact transmission-line load (new target `driven_p2_surface`). For the shorted line (two ω), the Silver-Müller cap at ε = 4, and the Leontovich, Hammerstad, Huray and London caps, the `Z_in` error slope is 3.93–4.20 at p=2 against 2.00–2.12 at p=1. The field L² slope is 1.96–2.01 against 0.97–1.03. p=2 on the coarsest mesh (436 DOFs) is 10–520× more accurate in `Z_in` than p=1 there.
  - The Silver-Müller cap reflection is Γ = 0.333337 at p=2 n=6, against the closed form (√ε − 1)/(√ε + 1) = 1/3 (p=1: 0.3287).
  - Leontovich wall loss (lossy plates, matched two-port, power balance) is within 0.03–0.21 % of the transmission-line-model Re γ (series impedance jω + 2Z_s/d) at p=2, and 0.6–0.9 % below Pozar's first-order α_c = R_s/(ηd). That gap is the perturbation error of α_c, not discretization: p=2 is converged by n = 3. Roughness scales the loss by K(f) to ≤ 0.53 % at three frequencies.
  - Reciprocity `∫J₂·E₁ = ∫J₁·E₂` holds to 2.7e-14, and `A(ω)ᵀ = A(ω)` to 1.4e-19 with a port plus four surface models present. The two-port S at p=2 has `|S12 − S21|` ≤ 6e-15, and the PROM matches the dense sweep to 3.9e-12.
  - p=1 is untouched: the `driven_p1_bit_identity` hashes are unchanged.
- `DrivenOperator::{order, n_dofs, interior_index, matrix_at}`, a public `interior_to_full`, `DrivenSolution::dofs` and `DrivenRom::evaluate_field`.
- **Zero-phase periodic boundary conditions on p=1 Nédélec** (#839, Epic #837 Phase 1).
  - `mesh::periodic::PeriodicMap::build(&mut mesh, pairs, opts)` matches master/slave boundary faces. It hashes the translated master nodes onto the slave nodes in O(n), infers the translation from the face bounding boxes when none is given, and rejects faces that are not translates. Edge orientation `σ = ±1` follows the node numbering, and corner entities (four copies of a z-edge on an x–y corner, eight of a triply periodic corner node) are chained to one canonical master with the lattice vector `Σd`.
  - Design rule (#804): a node within `1e-6·|d|` matches silently. A node within `0.25·h_min` is snapped onto the exact image, with a warning in `PeriodicMatchReport` (`n_snapped`, `max_snap_displacement`), but only when every master triangle maps onto a slave triangle. Anything else is `PeriodicMatchError::NonConforming`, which names the pair and the unmatched node and triangle counts and gives the fix: Gmsh `Periodic Surface … Translate {…}` or `geode mesh --periodic`.
  - `assembly::periodic::PeriodicConstraint::build(space, map, pec_mask)` builds the prolongation `P` on an `HcurlSpace` (`c64` coefficients, ready for the Phase 2 Bloch phase), together with the P1 node constraint and the reduced gradient `G_r`. `G P_node = P_edge G_r` is checked exactly. `Pᴴ A P`, `Pᴴ b` and `P x` are index remaps over the existing pattern. A one-sided PEC class eliminates every copy and warns. The row storage allows the p=2 blocks; a p=2 space returns `PeriodicError::Unsupported`.
  - `eigen::periodic_cavity::solve_periodic_cavity_modes` runs the unchanged shift-invert Lanczos on the reduced lossless pencil. `driven::periodic::PeriodicDrivenOperator` wraps a p=1 `DrivenOperator` for direct LU solves; lumped ports may not touch a periodic face. Every non-periodic path is untouched, and an empty periodic map reproduces `solve_pec_cavity_modes` and `DrivenOperator::solve_at` bit for bit.
  - Hooks for the sibling epics: `PeriodicMap::paired_face` (AMR, #835), `PeriodicMap::alias_node_motion` (shape sensitivities, #841) and `DofAliasMap::dphase_dk` (group velocity, Phase 2).
  - Goldens, recorded in `benchmarks/periodic/results.toml`:
    - the de Rham kernel is exact: `n_nodes_red − 1 + 3` on the 3-torus and `n_interior_nodes_red + 1` with PEC lids;
    - a box periodic in x and y with PEC lids matches the closed form, 12 modes with multiplicity: 0.78 % at the finest mesh, finest-step rate 1.96;
    - the two-layer Bragg stack at Γ matches the Kronig–Penney band edges to 0.27 %;
    - the periodic manufactured driven solution converges at energy-norm rate 1.08.
  - Inverse tripwires: PEC instead of periodic misses the closed form by 64 %; forcing `σ = +1` on a renumbered mesh, or dropping corner chaining, breaks the kernel count.
- **Bloch-phase periodic BCs and band-structure eigen solves** (#858, Epic #837 Phase 2).
  - `PeriodicConstraint::with_bloch_phase(k)` puts `σ·e^{−j k·Σd}` on every alias row of both the H(curl) DOFs and the P1 nodes. Re-phasing replaces `k` and does not compose. Phases in `(π/2)ℤ` are snapped exact, so `k·Σd ∈ πℤ` gives an exactly real `±1` `P`, and `k = 0` is bit-identical to the #839 constraint. `reduced_gradient_complex` carries the node phases, and `check_gradient_commutes` checks `G P_node = P_edge G_r` at any `k`. `DofAliasMap::reduce_real_matrix` reduces a real matrix with a complex `P`.
  - **Hermitian, not complex-symmetric.** With a complex phase `K_r = Pᴴ K P` is Hermitian and `K_rᵀ = K_r(−k)`. The complex-symmetric Lanczos, COCG and the port/adjoint shortcuts are never used for it. `eigen::bloch::BlochCell` assembles the real cell pencil once and solves any `k` with a native Hermitian shift-invert **block** Krylov method: conjugated `M`-inner product, Rayleigh–Ritz, and a residual gate on every returned pair. The block start resolves multiplicities up to `block_size` exactly. A single-vector Lanczos on the realified pencil sees every eigenvalue doubled and finds the extra copies of a true degeneracy only through round-off. `realify_hermitian` is kept as the cross-check: its real `2n` spectrum is the Hermitian one exactly doubled (1.2e-14).
  - **Null space at `k ≠ 0`.** The Bloch-phased gradients are deflated with an exact `M_r`-orthogonal projector. A node is pinned when constants are in the kernel (every phase 1, no PEC). The lowest bands are then targeted with a negative shift, including `ω → 0` near Γ. Harmonic fields at Γ come back as `ω = 0` modes flagged `is_static`.
  - **Group velocity** `∂ω/∂k` by Hellmann–Feynman through `dphase_dk()`. `align_clusters_along(t)` diagonalizes a degenerate cluster's directional derivative matrix (degenerate perturbation theory).
  - **k-paths:** `KPath` (`square_lattice` gives Γ–X–M–Γ) and `BlochCell::band_structure`. It reports sorted bands plus `n_bands` **overlap-tracked** bands, using phase-stripped periodic parts and `n_guard` extra modes. Degenerate clusters are tracked as subspaces: rotated along the segment, or group-summed where symmetry forces equal slopes (Γ, M). Low-confidence steps and kinked vertex degeneracies are warned about.
  - **Refusals (#804):** a driven solve with a complex Bloch phase (`PeriodicDrivenOperator::new` returns `PeriodicError::Unsupported`, Phase 3), p=2 periodic (`Unsupported`), and the zero-phase `solve_periodic_cavity_modes` with a complex constraint (`InvalidInput`, pointing at `eigen::bloch`).
  - Goldens (new targets `bloch_kernel`, `bloch_bands`):
    - **kernel tripwire:** `dim ker K_r(k) = n_nodes_red` exactly off the reciprocal lattice (3-torus and PEC lids, lexicographic and renumbered); `+2` at Γ and at `k = G`; `+1` at Γ between PEC lids. Phasing the edges but not the nodes breaks `G P = P G_r` (1.58) and `K_r G_r = 0` (0.52);
    - **empty lattice** vs the folded light lines with degeneracy clusters, including symmetry-forced 4-fold ones on the zone face: 0.80 % / 0.57 % at n = 12, observed rates 1.95 / 1.97. `v_g` along `k̂` is 1.0007 / 1.0013, and across `k̂` it is below 1.4e-3;
    - **Bragg stack** vs Kronig–Penney, normal and in-plane `k_y = 1` (TE and TM forms), `k_x a/π ∈ {0.05, 0.3, 0.6, ≈1}`, including the first gap edges: worst 0.074 % (gate 0.5 %);
    - **square rod crystal** (ε = 8.9, r = 0.2a, TM_z) along Γ–X–M–Γ. FEM is within 0.29 % of an in-tree plane-wave-expansion oracle at every point. The TM gap is 0.3226–0.4419 (2πc/a), gap–midgap 31.2 %. The PWE gives 0.3224–0.4425 and 31.41 %, converged in the plane-wave count. Joannopoulos et al., *Photonic Crystals* (2nd ed., 2008), ch. 5 fig. 2, gives 31.4 % (≈ 0.32–0.44);
    - **FD group velocity:** Hellmann–Feynman vs central differences agree to 2–6e-9 on rod-crystal and oblique-Bragg bands;
    - **band tracking through a crossing:** two folded light-line families cross mid-segment; the tracked bands stay monotone and swap sorted index.

- **N-port S-matrix sensitivities** (#842, Epic #841 Phase 1). `driven::s_sensitivity::s_matrix_sensitivity_sweep` returns `∂S_qp/∂θ` for every entry of the power-wave S matrix of a lumped, geometric wave, filled wave, mixed lumped + wave, walled (Leontovich / rough / Silver-Müller) or untouched-hybrid spec. `s_matrix_vjp` returns `dg/dθ` for a real `g(S)` from the same contraction.
  - Parameters are per-region `ε′` / `ε″` (`MaterialDesign`, bindable to named volume groups), node-motion shape columns with the port faces and walls pinned (`ShapeDesign`: a `FreeformBoundaryMorph`, or `from_group_translations` of named surface groups with named pinned groups), and filled ports whose medium follows a design region (`SDesign::port_fill`, through `∂y/∂ε_t = k₀²/(2β)`).
  - On the complex-symmetric operator the readout adjoint is the forward field already solved (`λ_q = x_q/d_q`), so all N² gradients cost no extra solve and one factorization per ω. `OperatorSymmetry::General` does explicit transpose solves on the same LU instead, for the Bloch-periodic case (#837).
  - The forward, dual fields and drive / readout scales are returned (`SAdjointFields`) for goal-oriented error estimation (#835 Phase 4).
  - Unsupported combinations are typed errors that name the phase that lifts them: moving port faces or walls, touched hybrid ports, non-scalar materials and magnetic fills, iterative solvers, p=2 spaces, and a region on an unbound port face.
  - Validated against central FD through the public forwards (worst relative error 4e-9 against a 1e-4 bar), with five mutation tripwires that each miss FD by 0.3 or more.
- `DrivenLinearSolver::back_solve_transpose`: a transpose back-solve on the cached LU (direct path only).

#### `geode-optimize` (new crate)

- **Gradient-based optimizer core** (#873, Epic #841 Phase 6a). The new `geode-optimize` crate is the library under the coming `geode optimize` (#841 P6b). It is pure Rust and physics-independent: no burn, faer or geode-core, and only the existing serde / serde_json / thiserror workspace dependencies.
  - **`Objective` trait.** One `evaluate(x)` returns `f`, `∇f` and, for constrained problems, the constraint values and Jacobian rows, which is what one adjoint solve gives. It matches `s_matrix_vjp` (#842) and the port-mode `vjp` (#859). `FnObjective` wraps a VJP closure, `Scaled` maps O(1) design units to physical parameters with an exact gradient chain (`weight = −1` maximizes), and `check_gradient` is a central-FD self-check. The `s_matrix_vjp_adapter` example wires a mock with `s_matrix_vjp`'s exact calling convention end to end. It runs as a test.
  - **`Lbfgsb`** (Byrd–Lu–Nocedal–Zhu 1995):
    - the compact limited-memory Hessian;
    - the generalized Cauchy point along the projected-gradient path;
    - direct-primal subspace minimization with the Morales–Nocedal (2011) projection;
    - a strong-Wolfe line search (Nocedal–Wright bracketing and zoom with safeguarded cubic interpolation).

    Unit tests check the compact `B` against the explicit BFGS recursion, the Cauchy point against a brute-force walk of the piecewise path, and the subspace step against `x − B⁻¹g`.
  - **`Mma`** (Svanberg 1987, with the 2007 `mmasub` parameters):
    - moving asymptotes and separable convex approximations;
    - elastic variables, so the subproblem is always feasible;
    - the subproblem solved through its **dual**, by projected Newton on the concave `W(λ)` with the exact `m × m` dual Hessian.

    `globalize = true` adds the GCMMA conservative inner loop.
  - **Stopping.** The run stops on the projected-gradient (or KKT) residual, the relative change in `f`, the step size, the maximum iterations or the maximum evaluations. It keeps a per-iteration history (x, f, residual, constraints, step, cumulative and per-iteration evaluation and failure counts, notes).
  - **Checkpoint/resume.** The whole state goes to JSON (`checkpoint_json` / `resume_json`), with every float stored as a shortest-round-trip string. ±inf bounds survive, and a resumed run is **byte-identical** to the uninterrupted one. A 1e-12 perturbation of one state value is detected, which shows the comparison is sensitive.
  - **Failed evaluations back off rather than abort**, and are recorded:
    - L-BFGS-B pulls the line-search trial back toward the last good step;
    - plain MMA halves the step;
    - GCMMA multiplies `ρ` by 10 and re-solves.

    A run ends with `Status::Stalled` (and a message) only when even the steepest-descent search, or every backoff, fails. A failure at `x0` is an error.
  - **Round-off floor.** Sufficient decrease is strict. A step whose `f` is within a few ulps of `f(0)` is accepted by the Hager–Zhang approximate Wolfe test, which reads the decrease from the slope. A search whose bracket predicts sub-ulp decrease stops at once, so near the optimum the projected gradient keeps falling instead of the search burning 40 evaluations per iteration.
  - **New test targets** `lbfgsb_goldens`, `mma_goldens` and `optimize_robustness`, run by the new `optimize-tests` job in `integration-tests.yml`. Results (the SciPy 1.17 L-BFGS-B counts are reference values, checked within 3×):

    | Problem | Method | Iterations / evaluations | SciPy | Result |
    |---|---|---|---|---|
    | Rosenbrock 2-D from (−1.2, 1) | L-BFGS-B | 38 / 47 | 38 / 46 | (1, 1), f = 2e-28 |
    | Rosenbrock 2-D, x₁ ≤ 0.5 | L-BFGS-B | 24 / 34 | 20 / 30 | (0.5, 0.25) exactly, f = 0.25 |
    | Rosenbrock 10-D | L-BFGS-B | 83 / 99 | 76 / 94 | all ones, f = 1e-21 |
    | Rosenbrock 10-D in [1.2, 3]¹⁰ (8 active bounds) | L-BFGS-B | 16 / 18 | 18 / 22 | SciPy's KKT point to 1e-9; bounds hit exactly |
    | Box QP, n = 30, 12 active bounds | L-BFGS-B | exact active set, ‖x − x*‖∞ ≤ 1e-9 | — | — |
    | HS71 (≤ form) | MMA / GCMMA | 21 / 22, 26 / 32 | — | 17.0140172892 |
    | HS76 | MMA / GCMMA | 17 / 18, 15 / 30 | — | −4.6818181818 |
    | HS21 | GCMMA | 12 / 63 | — | −99.96 at (2, 0) |
    | Svanberg cantilever | MMA / GCMMA | 10 / 11, 15 / 29 | — | 1.3399563606 |
    | Svanberg two-bar truss | MMA / GCMMA | 15 / 16, 15 / 21 | — | 1.5086524175 |
    | Min-compliance, n = 40 (closed form) | MMA / GCMMA | 17 / 18, 28 / 42 | — | f* to 1e-7 relative, x* to 1e-6 |

  - **Honest negative.** **Plain MMA does not converge on HS21.** With `x₂ ∈ [−50, 50]`, Svanberg's asymptote floor `0.01·(xmax − xmin)` leaves the approximation of `x₂²` too flat, and `x₂` falls into a stable 2-cycle between −0.207 and 0.693. GCMMA converges. A test pins both behaviours, and it is why GCMMA exists. Plain MMA is the cheaper default; switch to `globalize = true` when a run oscillates.

### Changed

- `cube_pec_interior_p2_dofs` is now a wrapper over the face-exact `HcurlSpace` mask. It gives the same mask as before on boxes (#838).
- The p=1 post-processors `driven::ports::port_voltage` (and so `port_input_impedance` and `extraction::extract_port_circuit`) and `geode_util::viz::edge_field_to_nodes` (the CLI VTU export path) now panic when the DOF vector length is not the mesh edge count. Before, a p=2 vector was silently truncated or indexed into, giving a plausible but wrong voltage or field plot (#838, #804). p=1 results are unchanged.
- p=2 requests that are not supported yet fail with `DrivenError::UnsupportedAtOrder` instead of solving at another order. This covers the matrix-free solver, the AMS preconditioner, wave ports in the PROM and the transient solver (#838; Phase 3 of Epic #836). Lumped ports and impedance surfaces were in this list until #857 added them at p=2.

### Fixed

#### `geode-core`

- **The PEC-cavity eigensolve no longer fails at µm and nm mesh units** (#852). `solve_pec_cavity_modes` was unit-invariant down to `length_unit = 3e-6` but returned `NotConverged` (a garbage Ritz pair with residual ≈ 1.0) at `1e-6`, `1e-7` and `1e-9`. The cause was faer 0.24's divide-and-conquer tridiagonal eigensolver, which the eigenpair Lanczos paths use once the Krylov basis has 128 or more vectors (the default `max_iters` is 160). It deflates with the tolerance `8ε · max(max|d|, max|z|)`, where `z` is a unit vector, so the tolerance is absolute when the matrix is small, and the shift-inverted tridiagonal scales as `L²` in the mesh unit (`~1e-13` for a µm cavity). The deflation merged distinct Ritz values: the gradient-null cluster drifted from round-off to `0.05 · σ` and passed the null filter as a "physical" mode. `T` is now scaled to unit magnitude by an exact power of two before the EVD, in both the plain and the projected shift-invert Lanczos. Unit-scale results are unchanged.
  - New test target `pec_cavity_unit_invariance`: the `1 × 0.8 × 0.6` PEC box gives the same `λ · s²` for `s` from `1e-9` to `1e3` (worst relative spread 6e-15). A unit test checks that `tridiag_eigenpairs` is homogeneous under scalings of `1e-19` to `1e8`.
  - The estimator's eigen unit-invariance golden (`hcurl_error_estimator`, #840) now reruns the full solve at `s = 1e-6` instead of `3e-6`, and its bars are tightened from `1e-8` / `1e-6` to `1e-12` / `1e-9` (measured `2.1e-15` / `2.8e-12`).

- **Periodic matching refits an inferred translation by least squares** (#856). With `PeriodicPair::transform = None`, `PeriodicMap::build` took the translation from the faces' bounding-box centroids. Those are set by a few extreme nodes, so jitter on a face's edges and corners went straight into the translation, off-axis components included. The snap then made the mesh exactly periodic on a slightly sheared, mis-sized lattice: on the #855 Judge's probe (a renumbered `6³` 3-torus with every hi-face node jittered by up to `0.04 h`), the lowest 14 eigenvalues moved by up to 0.37 % and the 6-fold degeneracies split. The bounding box is now only the first guess. After the node match, the translation is refitted by least squares over the matched node pairs (the mean offset `x_slave − x_master`) and the nodes are rematched until the match is stable. When part of the face is exactly periodic, the fit uses that subset, so interior-only jitter does not bias it; an exactly periodic mesh keeps its translation bit for bit. A subset counts as exactly periodic only if its offsets agree to round-off (`1024 ε · max |x|`) and it holds at least 3 pairs and 10 % of the matched pairs. Otherwise the fit uses every pair. An agreement threshold of `snap_tol` would let a few jittered pairs that agree by coincidence pass for an exact subset on large faces (the #862 Judge measured 2 to 5 such pairs on `31²` to `101²` faces, giving translation errors 25 to 275 times the least-squares one). Refitted components within `snap_tol` of zero are set to zero.
  - `PeriodicPairReport` gains `translation_fit_pairs` and `translation_residual` (the RMS master-image-to-slave distance before the snap). When an inferred translation needed a snap, the warning now gives the translation, the fit size and the residual. For an all-pairs fit it also estimates the translation error relative to the period (residual / √n) and the resulting relative spectrum shift (twice that). It says to pass the exact translation if it is known.
  - New goldens in `periodic_mesh_matching`: the refitted translation equals the least-squares one to round-off; with jitter whose mean offset over each pair is zero, the snapped torus and its spectrum match the exact-translation case to round-off with every degeneracy kept, while the old bounding-box translation shifts the spectrum by 3.6e-3 and splits them; On the literal Judge probe (generic jitter, whose mean offset is not zero) the translation cannot be identified from the mesh, so no estimator is exact; least squares cuts the spectrum error from 5.4e-3 to 8.8e-4. On the #862 Judge's probe (`101²` and `61²` faces, period 1 and 10, every slave node jittered in-plane or in 3D by up to `0.04 h`), the fit uses every matched pair and the translation equals the all-pairs least-squares one. Existing periodic goldens are unchanged.

## [0.8.0] - 2026-10-04

This release completes Epic #756 (geode CLI Phase 4: physics breadth for EDA flows) and Epic #778 (hybrid wave ports). It covers:

- **Materials:** wideband dielectric and conductor models and anisotropic materials.
- **Wave ports:** the full story, including filled ports, mixed with lumped ports, lossy and rough walls, Touchstone export, adaptive sweeps, and microstrip/stripline/coax/inhomogeneous ports in the CLI with Z_PI line impedances and accuracy warnings.
- **Correctness fixes:** several silently-wrong results are fixed:
  - PEC edge masks;
  - weak-guidance fiber modes;
  - the faer complex QZ hang and the inaccurate real QZ;
  - TE-only ports above the TM cutoff;
  - mesh-unit-dependent thresholds;
  - high-Q `Im k`;
  - unconverged Ritz pairs.
- **CI** now runs 157 of 160 test targets, up from 41, guarded by a fail-closed coverage check.

**Breaking (`geode-core` library):**
- `solve_wave_port_sweep_with_mode` gains a `surfaces` parameter (#776).
- `EigenError` gains `DenseTooLarge` and is not `#[non_exhaustive]` (#796, #800).
- `LossyCavityModes` gains fields (#834).
- `pec_interior_edge_mask` is deprecated (#771).

The `geode` spec and report schemas change additively (schema v1).

### Added

#### `geode` CLI

- **Dispersive dielectrics.** Wideband Djordjevic–Sarkar laminates, specified by `eps_r` / `tan_delta` at a reference frequency, with ε(f) evaluated per frequency in `driven` / `extract` (#757). Multi-pole Debye and Drude models are also available (#761). Adaptive (PROM) sweeps reject dispersive specs with `invalid_spec`, because ε(ω) is not affine.
- **Conductor surface roughness** on Leontovich walls: Hammerstad–Jensen and Huray (cannonball) loss multipliers K(f) (#758).
- **Diagonal anisotropic materials:** `eps_r_diag` / `mu_r_diag` in every analysis. The AMS preconditioner refuses non-positive Re ε (#760).
- **Mixed lumped and wave ports** in one driven spec, giving a single S-matrix over all ports (#759).
- **Filled wave ports.** The port mode solve uses the material filling the face (`PortMedium`), with β² = k₀²ε_tμ_t − (μ_t/μ_n)k_c² and admittance y = β/μ_t. Lossy fills take a complex branch, and vacuum results are bit-identical to before. The report gains `wave_ports[].medium`. These port faces are rejected with `invalid_spec`, including in `geode check`:
  - inhomogeneous faces (later supported by #807);
  - unequal in-plane tensor components;
  - UPML faces;
  - Re ε_t·μ_n ≤ 0 (#781). (#777)
- Wave ports compose with Leontovich walls, including rough walls: a lossy guide's S-parameters carry the conductor attenuation, validated against Pozar Eq. 3.96 (TE10 α_c) and the roughness factor K(f). A Silver-Müller wall that shares an edge with a wave-port rim is rejected with `invalid_spec` (#776).
- **Hybrid wave ports: microstrip, stripline and inhomogeneous port faces** (#807, Epic #778 Phase 5).
  - **Routing.** `check` and `driven` route each wave port by its cross-section (`wave_ports[].route`). A face touching more than one material, or carrying a floating conductor (a `pec` strip sheet crossing it, or a carved-out strip), goes to the hybrid path: the full-vector E_t–E_z port modes, re-solved and tracked at every frequency. Before, an inhomogeneous face was `invalid_spec` naming #778, and a coax/strip face silently lost its TEM mode. Homogeneous faces without a conductor keep the geometric TE path bit for bit. The TE-only TM-cutoff guard (#808) applies to geometric ports only; hybrid ports use core's completeness check, and their rim must still lie on a conductor wall. Lossy faces use the complex-symmetric pencil; dispersive specs use the dispersive spec sweeps, so the faces and the volume read ε(ω) from the same per-tet vector.
  - **Defaults** (operator decision on #804): complex evanescent pairs are carried; `wave_ports[].hybrid.n_termination_evanescent` defaults to 4 (clamped on a small face, with a warning); the accuracy estimate is on, with `hybrid.accuracy_threshold` (default 0.5 %) applied to β and, on a lossy face, to α. Every warning goes to the report's new `warnings[]` and to stderr.
  - **Line impedance.** `wave_ports[].impedance_definition` defaults to `power_current` (Z_PI); `power_voltage` and `voltage_current` are opt-ins documented as path- and frequency-dependent. Z_TE is never offered for hybrid ports. `--touchstone` renormalizes hybrid channels to `reference_ohm` with the per-row `z_line_ohm`, which is complex on a lossy face. A hybrid port without a floating conductor is rejected for `--touchstone`.
  - **Line-impedance accuracy estimate** (#807 review: Z₀ is never silently wrong). The β estimate is the wrong proxy for Z: ε_eff = C/C₀ is a ratio, so the strip-edge singular error cancels in it, while Z = 1/(c√(C·C₀)) keeps all of it. Every channel with a line impedance now reports `hybrid.z_line_accuracy`: `z_line_ohm` re-evaluated on the h/2 face the β estimate already solves, Richardson-extrapolated with the rate observed over h, h/2, h/4 (about 1–1.5 at a strip edge, not the O(h²) of β; the conservative singular rate 1 when it cannot be observed). Above `wave_ports[].hybrid.impedance_accuracy_threshold` (default 1 %: a 1 % Z error renormalizes a matched line to |S11| ≈ 0.005) it raises `impedance_accuracy_above_threshold`, in `warnings[]` and on stderr, on `check` and on `driven` (including `--touchstone`) runs. The warning gives the refine factor and the strip-edge cell size from the observed rate, and says to grade toward the conductor edges. A failed refined solve, a missing match or a degenerate cluster gives `impedance_accuracy_unavailable`. The estimate covers discretization only: a shielded line's Z₀ also depends on the shield box (physics; about −3.3 % for an 8h × 5h box, −1 % for 16h × 10h, −0.3 % for 40h × 40h), which it does not flag. Measured on the 8h × 5h microstrip: estimates within 1.05× of the true Z error at three face levels (10.8 % vs 10.4 %, 6.4 % vs 6.3 %, 3.6 % vs 3.6 %), where the β estimate is 0.2–0.5 %.
  - **Report consumers: branch on `wave_ports[].route` before reading `medium`** — a hybrid port's `medium` (and `modes`, the TM-guard fields) is `null`.
  - **Report** (additive, schema v1). Every wave channel gets `eps_eff` and `normalization` (`power`, `pseudo_power` or `evanescent`; evanescent means not power-normalized). Hybrid channels add `hybrid`: E_z energy fraction, β and α accuracy estimates, residual and floor, tracking overlap, cluster size, `coupled_mode` even/odd, line impedances, and `z_line_ohm`. Rows of a lossy spec get `sigma_max` (measured passivity, with a `passivity` warning above 1 + 1e-6). Wave ports get `route` and `hybrid` (reason, conductors, free face unknowns, h, options, observed rates, worst tracking overlap); a hybrid port's `medium` is `null`. `geode check` previews every hybrid port's face solve at every frequency (`wave_ports[].hybrid.frequencies[]`) without the 3-D solve.
  - **Still `invalid_spec`.** μ_r ≠ 1 or anisotropic ε on a hybrid face; `sweep.adaptive` with a hybrid port (non-affine in ω, #774); `absorbing_regions` with a hybrid port; a dispersive geometric fill alongside hybrid ports; `power_voltage` / `voltage_current` on a conductor with no voltage path; internal port planes that differ across the plane; and the hybrid-only fields on geometric ports.
  - **Cookbook.** `examples/driven/microstrip_line.json` with its Gmsh geometry `microstrip_line.geo`: a 50 Ω shielded microstrip with hybrid ports, written to Touchstone. A 20h × 12h shield graded toward the strip edges gives Z_PI = 49.32 Ω (1.4 % below 50 Ω: 0.6 % shield offset, 0.9 % mesh error, estimated 0.88 %) with no impedance warning.
  - **Measured** (`tests/hybrid_wave_port.rs`):
    - slab-loaded guide β within 0.031 % of the LSE/LSM oracle (the accuracy estimate tracks it: 3.13e-4 vs 3.13e-4);
    - a Hammerstad–Jensen 50 Ω microstrip gives Z_PI = 48.5 Ω and Touchstone |S11| ≤ 0.014, while the Z_TE tripwire would give |Γ| ≈ 0.61;
    - microstrip eps_eff is −0.09 % from Hammerstad–Jensen (20h shield);
    - coupled microstrip: even first, eps_eff 3.27 vs odd 2.88, even→odd conversion ≤ 3e-5;
    - a tan δ = 0.02 line has σ_max = 0.997 / 0.995 and |S21| = e^{−αL} to 1e-5.

#### `geode-core`

- `solve_wave_port_sweep_with_mode` takes a `surfaces` argument (impedance walls folded into the base operator); an empty slice is bit-identical to before. New `analytic::waveguide::te10_conductor_attenuation` oracle (#776).
- New `analytic::port_modes`: a p=1 Whitney + P1 mixed E_t–E_z port-mode solver (`solve_hybrid_port_modes`) for PEC-shielded cross-sections with real per-triangle ε. It returns every propagating mode plus the first K evanescent modes, including fast modes. The exact β² = 0 null space is deflated out of the Krylov space, every pair carries an explicit residual, and modes are normalized by the B-form with `xᵀBx = β²` for propagating modes. A short or uncertified window returns an explicit `Shortfall` error. A complex-conjugate pair inside the window returns a `ComplexPair` error. New `analytic::loaded_guide` closed-form LSE/LSM oracle for the slab-loaded rectangular guide. Measured on the slab guide (ε_r 2.25 and 4): β error ≤ 0.43 % at h = b/16, rate ≈ 2 in β², and the mode set matches the oracle one-to-one with no ceiling pileup. Honest limits: spurious complex LSE/LSM evanescent pairs, with Im β² = O(h); and P1-carried TM modes (uniform TM₂₁ β error 1.8 % at h = b/16). The fiber `mixed_pencil` solver is unchanged; its Arnoldi gains an optional projector (#803, Epic #778 Phase 1).
- **Hybrid wave ports** (#804, Epic #778 Phase 2): a `HybridWavePort` drives the 3-D solve with modes of an inhomogeneous PEC cross-section, re-solved per frequency with the p=1 mixed solver and tracked by continuity (B-pairing overlap, one-to-one; lost identity is an explicit error, never a guess). The modal flux is `f̂ = S_p(E_t − (j/β)∇_tE_z)` normalized by the unconjugated B-form, so the existing rank-N SMW, drive and `√β` power weights carry over unchanged. Additive API: `WavePortSpec` (geometric or hybrid), `solve_wave_port_spec_sweep_with_mode` and `solve_mixed_port_spec_sweep_with_mode` return the S points plus per-port reports (β, E_z energy fraction, tracking overlap, accuracy estimate) and report-level warnings. Every propagating face mode must be a reported channel, else `InvalidPort`; hybrid ports carry TM content, so the TE-only TM-cutoff guard of #808 does not apply to them. Mesh-induced complex evanescent pairs are terminated as a reciprocal 2×2 block with a warning that names the port and modes and gives a predicted split resolution. A per-mode accuracy estimate (an h/2 face re-solve with nested prolongation, `analytic::port_mode_accuracy`) warns above 0.5 % with a refine-to-h hint. `DrivenRom::build_with_wave_port_specs` rejects hybrid ports, because their port operator is not affine in ω. `HybridPortOpts::carry_complex_pairs` (default off, so P1 behaviour is unchanged) returns complex pairs as `HybridComplexPair`. Measured: the homogeneous limit matches the `PortMedium` path to `|ΔS| = 4e-15`; on the slab-loaded section, β is within 0.22 % of the oracle, `|S11|` ≤ 1.0e-2, `|S21 − e^{−jβL}|` ≤ 1.2e-2, and reciprocity and energy hold to round-off; the accuracy estimate is within a factor of 1.11 of the true error over 46 modes.
- **Interior PEC strips on port faces and the quasi-TEM mode, 2-D part** (#805, Epic #778 Phase 3).
  - `analytic::port_modes::HybridPecMasks` treats the rim, carved-out thick strips (holes) and zero-thickness PEC sheets with one rule: an edge is PEC if it is on the boundary or in an extra mask; a node is PEC if it touches a PEC edge.
  - The p=1 hybrid solver returns the quasi-TEM mode, which lies far from the deflated β² = 0 null space.
  - New `mode_line_quantities` gives the line impedances Z_PI, Z_PV and Z_VI from the discrete Ampère current and a path voltage. These are distinct from the #775 wave impedance.
  - New `analytic::microstrip` adds:
    - the Hammerstad–Jensen ε_eff/Z₀, Kirschning–Jansen dispersion and Pozar α_d closed forms;
    - a graded, optionally mirror-symmetric shielded-strip face builder (sheet or thick strips);
    - a C4v square-coax face;
    - a 2-D P1 electrostatic capacitance reference on the same mesh.
  - Measured:
    - the homogeneous TEM has β² = k₀²ε to 2e-13;
    - the low-frequency ε_eff tends to C(ε)/C(1) with an O(k₀²) gap (ratio 4.00 per halving of k₀);
    - against Hammerstad–Jensen, ε_eff is within 0.53 % and Z_PI within 1.33 % over w/h ∈ {0.5, 1, 2}, ε_r ∈ {4.4, 9.8}, with box doubling under 0.41 %;
    - against Kirschning–Jansen, within 0.47 % up to f·h = 5 GHz·mm.
  - Solver changes, which leave the P1/P2 tests unchanged:
    - a default multiplicity verification pass (`HybridPortOpts::verify_multiplicity`). It re-runs Arnoldi from an independent start with the found modes deflated, and recovers missed copies of exactly repeated eigenvalues. A single start returned only one of the two degenerate TEM modes of a homogeneous two-strip face.
    - B-orthogonalization within degenerate clusters. Copies found on a C4v mesh paired at 4e-2 before it.
    - a residual round-off floor (`HybridPortMode::residual_floor`, reported per mode and counted in `floor_accepted`). Low-frequency faces (k₀W ≤ 0.1) previously failed with `Shortfall` even though the eigenpair was accurate.
  - The 3-D part (driving with the quasi-TEM mode) is a follow-up phase.
- **Lossy and dispersive substrates on hybrid port faces** (#806, Epic #778 Phase 4).
  - New `analytic::lossy_port_modes`: the complex-symmetric p=1 mixed pencil for complex per-triangle ε (tan δ, Djordjevic–Sarkar, Debye, Drude), solved by a complex shift-invert Arnoldi. It keeps the null-space deflation, coverage certificate, explicit `Shortfall`, multiplicity pass and residual floor of the real solver. β is complex on the outgoing branch (Im β ≤ 0). "Propagating" means Re β² > 0, i.e. Re β > |Im β|. Modes are normalized unconjugated, zᵀBz = β², so Sᵀ = S.
  - `HybridPortFace::from_volume_lossy` / `new_lossy` build lossy faces. `solve_{wave,mixed}_port_spec_sweep_dispersive_with_mode` re-assemble the volume and re-solve the port faces from the same per-tet ε(ω) at every frequency, with mode tracking across frequency.
  - A face built from the volume must match the volume's ε bit for bit; a lossless face on a lossy volume is now an `InvalidPort` error. Gain (Im ε > 0) is refused.
  - `HybridChannelReport` gains `beta_sq_im` and an α accuracy estimate. A failed refined solve on this path reports the estimate as unavailable instead of aborting the sweep.
  - `FaerComplexEigensolver::smallest_complex_pencil_pairs` adds a general complex (A, B) eigenpair entry point, used as the dense oracle. `SlabLoadedGuide::continued_root` gives complex LSE/LSM roots by continuation in ε.
  - Measured:
    - Re β is within 0.44 % and α within 0.41 % of the continued root at h = b/32 (28 modes, O(h²) rates);
    - first-order α matches the exact lossy solve to 3.4e-6;
    - the microstrip α_d is −0.86 % from Pozar;
    - the dense oracle agrees to 3.3e-13;
    - on the 3-D lossy straight section, |S21| matches e^{−αL} to 8.5e-4 and reciprocity holds to 1e-14; σ_max(S) ≤ 0.981 is measured, not guaranteed (pseudo-power normalization);
    - a Djordjevic–Sarkar slab over 8 frequencies tracks continuously;
    - the tan δ = 0 regression reproduces Phase 1/2 to 7.5e-15 (S) and 6.7e-13 (β²);
    - the accuracy estimate is within 1.08× (β) and 1.30× (α) of the true error.
- **Hybrid-port follow-ups for the CLI** (#807, Epic #778 Phase 5):
  - `solve_hybrid_port_face_sweep` runs the port-face half of a hybrid spec sweep without the 3-D solve: modes, tracking, completeness, termination window, accuracy and warnings. Its report and warnings equal the 3-D sweep's bit for bit, on both the real and lossy paths.
  - A failed refined (h/2) accuracy solve on the real path now degrades to `AccuracyUnavailable`; it no longer aborts the sweep (#815 review note 1). The lossy path already did this.
  - New opt-in `PortAccuracyOpts::alpha_threshold` raises the new `PortWarningKind::AttenuationAccuracyAboveThreshold` for a lossy channel whose α estimate exceeds it (#819 review). It is off by default, so the Phase 4 goldens are unchanged; the CLI sets it to the β threshold.
  - Lossy faces report complex line impedances, `HybridChannelReport::line_lossy` (`HybridComplexLineReport`, unconjugated `Z_PI = 2P/Σ I_c²`). These equal the real Z_PI in the lossless limit (3e-15), and on an exact lossy TEM line they scale as √(ε′/ε) (3e-13).
  - With `sigma_tet`, a hybrid face built from the volume now sees the ε − jσ/ω of the conducting tets it bounds; before, it ignored σ. The port β equals the dispersive sweep with that ε(ω) bit for bit, and S matches to 4e-15.
  - The internal `HybridState` report assembly moved into `into_report`; the arithmetic is unchanged.
  - **Line-impedance accuracy estimate** (#807 review): `HybridChannelReport::impedance_accuracy` (`ImpedanceAccuracy`: `Z_PI`, and `Z_PV` / `Z_VI` with a voltage path) re-evaluates the line quantities on the h/2 face (refined conductor = coarse conductor nodes plus the midpoints of its PEC edges; the voltage path through the edge midpoints) and extrapolates with the per-channel rate observed over h, h/2, h/4, clamped to [0.5, 2] (`SINGULAR_IMPEDANCE_RATE` = 1 when unobserved). New `PortAccuracyOpts::impedance_threshold` (default `DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD` = 1 %) and `::impedance` (`LineImpedance`, the definition checked) raise `PortWarningKind::ImpedanceAccuracyAboveThreshold` (refine factor, h and strip-edge cell targets); `ImpedanceAccuracyUnavailable` says why an estimate is missing. Real and lossy paths. A lossy face's complex line currents now apply the full complex T_ε of the face ε, so a dispersive face's Re ε(ω) is honoured (before, the displacement term used the build-time Re ε; identical for a non-dispersive face).
- **3-D microstrip / stripline sections with hybrid wave ports** (#817, Epic #778 Phase 3b).
  - `HybridPortFace::from_volume_with_pec` / `with_interior_pec`: a face edge is PEC if it is on the rim or its 3-D edge is eliminated by the volume PEC mask, so zero-thickness strips (PEC sheets inside the volume) reach the port solve. Floating conductors are found automatically (`HybridPortFace::conductors`, ordered by 3-D centroid), each with a shortest shield-to-conductor voltage path. Without a mask the face is rim-only, as before.
  - New `mesh::extrude_tri_mesh` (and `_layers`): a conforming sorted-vertex prism split of any `TriMesh` into right-handed tets, with per-tet source triangle and slab and an `ExtrudedEdge` classification for z-dependent PEC masks. New `driven::ports::strip_line_section` builds a straight shielded strip-line section and its two hybrid ports.
  - Every propagating channel on a strip face reports `HybridLineReport` (Z_PI, Z_PV, Z_VI, signed conductor currents and voltages) per port and per frequency, for #807. With several conductors these are the per-line modal impedances (`Z_PI = 2P/Σ|I_c|²`), which are Z_e / Z_o for a symmetric pair.
  - Exactly degenerate clusters (e.g. the TEM pair of a homogeneous two-strip line) are tracked as subspaces by principal angles and rotated by orthogonal Procrustes, and get a canonical basis at the first frequency (orthogonal conductor currents: odd / even). A split cluster or a non-canonical basis is a warning; a reported-channel count that splits a cluster is `InvalidPort`.
  - The sweep consumes `multiplicity_certified`: an uncertified solve is retried with twice the Krylov cap, then continues with a `MultiplicityUncertified` warning naming the port and ω (new `HybridWavePortOpts::verify_multiplicity`, default on). Each channel reports `residual`, `residual_floor` and `floor_accepted` at every frequency; each point reports the certificate, retries, repeated copies, degenerate clusters and the floor-accepted count.
  - The Phase 4 lossy path gets the same treatment: lossy faces take the volume PEC mask (`from_volume_lossy(..)?.with_interior_pec(..)`); exactly degenerate lossy clusters are tracked as subspaces with complex-orthogonal Procrustes rotations (`RᵀR = I`, keeping `zᵀBz = β²`; Denman–Beavers inverse square root) and get the canonical odd/even basis from the complex-symmetric current matrix. The certificate, floor and cluster reports and warnings apply as well. Lossy faces do not report line impedances yet (`line: None`): a lossy line's Z is complex, and choosing its definition is #807's job.
  - The E_t-only `PortFaceProjection::solve_modes` count subtracts one harmonic field per hole (`n_holes`); hybrid ports reject more reported + termination channels than the face's free transverse DOF count (`HybridPortFace::max_modes`). New `PortFaceError::InvalidPecMask`.
  - Measured (`tests/hybrid_strip_line.rs`): on a straight shielded microstrip (ε_r 4.4, w/h 1), `|S11|` ≤ 5.1e-3, `|S21 − e^{−jβL}|` ≤ 8.3e-3 and reciprocity holds to 2e-14, while the port β and Z_PI equal the 2-D P3a solve to 2e-13. A homogeneous stripline gives exact TEM (β to 3e-13, Z_PI = Z_qs to 2e-13). A carved thick strip and a masked one agree to 1e-13 in 3-D S. Across a width step w/h 1 → 2, `|S11|` follows the Z_PI ratio within 0.30 %, where Z_TE would predict an 18× smaller reflection. Coupled microstrip even/odd ε_eff and Z match the 2-D capacitance matrix within 3.4e-4 at k₀h = 0.01. The degenerate stripline pair is canonical to 1e-12 and tracked with overlaps of 1 − 2e-12; a C4v coax TE11-like pair (one conductor) keeps the solver basis with a warning and is tracked to 1 − 2e-15. On the P3a 20h shield, Z_PI is −1.43 % from Hammerstad–Jensen. On a lossy microstrip (tan δ 0.02), the port β equals the 2-D lossy solve to 1.4e-13; a lossy degenerate stripline pair keeps β² = k₀²ε to 3e-13, with tracking overlaps of 1 − 2e-12.
- `eigen::lanczos::SparseShiftInvertLanczos::smallest_eigenpairs_checked` (with `ConvergenceCheck` and `CheckedEigenpairs`) returns only Ritz pairs whose true residual `‖K x − λ M x‖ / (max(|λ|, |σ|)‖M x‖)` meets a tolerance. When the first pass leaves a pair unconverged, it extends the same Krylov run (no restart, bounded by a step cap) until a converged pair confirms it. It reports what it withheld (`rejected`, `shortfall()`). After an extension, `rejected` holds only the final run's unconverged pairs among the requested in-window pairs nearest σ, plus targets still unconfirmed at the cap. It does not hold stale first-pass tail pairs that the longer run resolved, so the SMF-28 p=2 log now reads 0 withheld with 4/4 modes (it used to read 7), and the metallic retry no longer fires on them. When the first pass already converges, the result is bit-identical to `smallest_eigenpairs`. The plain `smallest_eigenpairs` is unchanged, so the cavity, transmon, projection and CLI `eigen` solves keep their results and cost exactly (#798). (Its convergence probe was later made relative to the operator scale, which changes the iteration count on pencils with `|λ − σ| > 1`; see #828 under Fixed.)
- `eigen::complex::SparseComplexShiftInvertLanczos::smallest_eigenpairs_checked` (returning `CheckedComplexEigenpairs`, with the same `ConvergenceCheck`; its `window` is an interval on `Re λ`) is the complex-symmetric counterpart of the real checked solve (#834). It returns only Ritz pairs whose true residual `‖K x − λ M x‖₂ / (max(|λ|, |σ|)‖M x‖₂)` meets the tolerance. When the first pass leaves a pair unconverged among the requested pairs nearest σ, it extends the same Krylov run (no restart, no refactorization, bounded by a step cap) until a converged pair confirms every first-pass pair that locates an eigenvalue, and reports the rest (`rejected`, `shortfall()`, `lanczos_steps`, `extended`). A Ritz value that locates no eigenvalue (`ρ·max(|λ|, |σ|) > |λ − σ|`) is withheld and never drives the extension. When the first pass converges, the result is bit-identical to `smallest_eigenpairs`. `smallest_eigenpairs_checked_filtered` takes an eligibility predicate applied before the closest-to-σ selection (first pass: every Ritz pair of the `max_iters` basis, as `lossy_cavity` needs); the dropped values are in `screened`. The bilinear Lanczos needs this more than the real one: it has no interlacing guarantee, so an unconverged Ritz value can sit nearer σ than converged modes. A seeded, platform-independent regression pencil (`M = I`, 40 passive eigenvalues, `σ = 1`, 16 steps) reproduces one: the plain solve returns `λ ≈ 1.0900 + 0.1486j` (relative residual 2.2, the unphysical sign of `Im λ`) among its 3 values nearest σ, and the checked solve extends to 24 steps and returns the exact `0.82 − 0.02j`, `1.15 − 0.01j` and `1.3 − 0.005j`.

### Fixed

#### `geode` CLI

- A wave-port sweep that reaches a port's TM cutoff is now rejected with `invalid_spec` by `check` and `driven` (pure wave, mixed and adaptive specs). Before, it gave silently wrong S-parameters. The port modal solve carries TE modes only, so above the lowest TM cutoff (TM₁₁ in a rectangular guide) a propagating TM channel had no termination. A height step driven above TM₁₁ changed `|S|` by 0.44 when its feed section was lengthened, against 3.6e-3 below TM₁₁, and its power balance still summed to 1. The limit is 5 % below an estimate of the TM cutoff: the port face's P1 Dirichlet `E_z` eigenvalue, Richardson-extrapolated over two uniform face refinements with the measured order. The face value alone is a Rayleigh-Ritz upper bound and sits above the 3-D Nédélec model's own TM cutoff, which falls below the continuum on a coarse axial mesh (8 × 4 face of a 2 × 1 guide: face 3.661, analytic 3.512, 3-D 3.349 with one tet layer of 0.5; the limit is 3.337). Filled ports scale the limit by `1/√(Re ε_n·μ_t)`, using the axial `ε_n`. Fills with `Re ε_n·μ_t ≤ 0` have no TM cutoff and are always rejected. The message names the port, the limit and the first offending frequency in Hz, and points to #778 / #804. `check` / `driven` reports gain `wave_ports[].tm_k_c`, `tm_k_c_face` and `tm_limit_hz` (additive in schema v1). Two existing CLI tests swept an ε_r ≈ 1.5 guide to k₀ = 3.0, past its filled TM₁₁ (2.87); they now stop at k₀ = 2.7, below the filled limit 2.72. Their straight, uniformly filled guides do not couple TE₁₀ to TM₁₁, so the old rows were not numerically damaged (#808).
- A wave port whose rim is not entirely on `pec` or `leontovich` walls is now rejected with `invalid_spec` by `check` and `driven`, naming the port, the number of open rim edges and the surface groups they lie on. The port modes are solved with a PEC rim, so on any other rim (for example a PMC symmetry plane) they were silently the modes of the wrong cross-section (#808).
- `geode eigen` on a lossy or open pencil reports `solver.n_withheld` (additive, schema v1): the unconverged Ritz pairs the residual-checked Lanczos withheld from the nearest-σ set (#834). Each is a spurious Ritz value or lies beyond the farthest returned mode. A genuine mode still unconverged inside the returned range now fails the solve ("eigensolve did not converge", naming that pair) instead of being replaced by a farther mode.
- `geode eigen` on a lossy or open pencil now reports high-Q modes with a finite, accurate `q` and `k0_im`. Before, a mode with `Q ≳ 6.7e7` came out as `k0_im = 0`, `q: null` ("lossless"), and from `Q ≈ 1e7` the Ritz values themselves degraded: on a uniformly filled cavity with `tan δ = 1e-10` (`Q = 1e10`) the solver's own `Q(λ)` was 88 % low with residuals `≈ 1e-7`. Both now agree with the exact `½·cot(δ/2)` to `≤ 8.2e-14` relative at `Q = 2e8` and `1e10`, with residuals `≤ 1.2e-13` (#830).
- The wave-port TM guard's margin now follows the axial mesh at the port. It was a fixed 5 %, which a fine port face over a coarse axial mesh could slip under: 12 × 6, 16 × 8 and 24 × 12 faces over one tet layer of `h_z = 0.5` have a 3-D TM₁₁ of 3.327, 3.318 and 3.312, all below the old limit of 3.337. The controlling variable is `k_c·h_n`; the axial/in-face spacing ratio the docs gave does not control it. `h_n` is the largest extent along the port normal of a tet in the guide feeding the port: a tet on the port face, or one whose centroid lies over the face and within one wavelength (`2π/min(k_c, k)`, `k` the top sweep wavenumber in the fill) of the port plane. It is read over the guide because the 3-D cutoff is set by the guide's coarsest cells. Read off the face-adjacent tets alone, a fine layer at the port hid the coarse layers behind it, and the guard sat above the 3-D TM cutoff: 3.337 against 3.302 on a 2 × 1 guide with layers of 0.15 and 0.6, and 3.146 against 3.077 on a 3 × 1 guide with layers of 0.2 and 0.7. Read over the guide, those limits are 3.122 and 2.867. Measured over `k_c·h_n` from 0.29 to 3.42, with faces from 4 × 2 to 48 × 24 on structured meshes uniform along the guide, the 3-D undershoot is at most `0.0205·(k_c·h_n)²`. Stepped-layer and graded structured meshes give at most `0.0140·(k_c·h_n)²`, uniform Gmsh meshes `0.0167`, and graded or stepped Gmsh meshes `0.0089`. The margin is now `max(5 %, 0.025·(k_c·h_n)²)`. That is still 5 % up to `k_c·h_n ≈ 1.41`, about 4.4 axial cells per TM-cutoff wavelength, so existing specs, fixtures and goldens are unchanged. On those three meshes it is 7.7 %, a limit of 3.24. When the widened margin rejects a frequency, the message says so and gives the axial spacing that would admit the sweep, to be applied along the guide feeding the port within one wavelength of it, not only at the face. When the top sweep frequency has `k·h_n > 1.41` in the fill, `check` and `driven` still run but print a `warning:` line on stderr with the spacing to refine the guide to (`h ≤ 1.41/k` along its axis). The same text appears in `wave_ports[].tm_warning`. Reports gain `wave_ports[].tm_axial_spacing` and `tm_margin` (additive in schema v1) (#824).
- The wave-port TM guard now reads the guide's axial mesh over three TM-cutoff wavelengths of the port (`max(3·2π/k_c, 2π/k)`), not one, and counts a tet by its nearest vertex, not its centroid (#845). A coarse section just beyond the one-wavelength window slipped under the guard: a 2 × 1 guide with layers of 0.15 out to 2.25 or 3.0 mesh units (1.26 and 1.68 λ_c), then layers of 0.6, read `h_n = 0.15` and had a limit of 3.337, 2 % above its lowest TM-like 3-D mode (3.272 and 3.269), whose field reaches the port through the evanescent fine section at 9–14 % amplitude. The centroid test also dropped a coarse layer starting inside the window (at 0.92 λ_c). Both now read 0.6 and get a limit of 3.12. The window follows the evanescent leak `exp(−α·d)`, `α = √(k_c² − k²)`: at `k = 0.95·k_c` it is 14 % one TM-cutoff wavelength out, 2 % at two and 0.3 % at three. A coarser section beyond the window still does not reject the sweep (it may be a device region, and its leak is bounded). When it can carry a TM mode below the top sweep frequency and the estimated leak at the port is above 1 %, `check` and `driven` warn on stderr and in `wave_ports[].tm_warning` (now one line per warning) with where it starts, the spacing to refine it to, and the frequency below which the leak is under 1 %. Rejections and warnings now also say how far from the port the cells that need refining start. Existing specs, fixtures and goldens are unchanged.
- The TM guard's beyond-window warning now says the S-parameters can be off "by up to" the quoted leak, which is an upper bound (on the fine-window probe it quotes about 28 %, against a realistic 0.2 %). The CLI README notes the ~8 % worst case three TM-cutoff wavelengths out, from the coarse section's mesh-cutoff undershoot, which the warning covers. The guide's tets are scanned once per port, not once per message (#848).

#### `geode-core`

- **PEC edge masks are now face-exact.** `cube_pec_interior_edges`, `sphere_pec_interior_edges` and `cylinder_pec_interior_mask` used a node rule that also eliminated interior edges joining two boundary faces: 12(n−1) wrong edges on an n-cube, and 2·n_theta on coax/loop meshes. That over-constrained cavities and biased results.
  - The sphere mask is unchanged, bit for bit.
  - Accuracy improves sharply, e.g. coax L′ error 6.1e-3 → 9.1e-4, skin depth 3.0% → 0.45%, unit-cube PEC cavity 19.1% → 6.6%. Test tolerances are tightened to match, and `benchmarks/magnetostatic_inductance` is regenerated.
  - New `TetMesh::boundary_faces()` and `boundary_pec_interior_edges`; `pec_interior_edge_mask` is deprecated (#771).
- New `PortFaceProjection::lowest_tm_cutoff` (the port face's P1 TM cutoff, with optional open rim edges; a Rayleigh-Ritz upper bound), `PortFaceProjection::tm_cutoff_estimate` / `TmCutoffEstimate` (the face value extrapolated over two uniform refinements, and `guard_k_c`, `TM_GUARD_MARGIN` = 5 % below it) and `PortMedium::tm_cutoff_k0` (the filled TM cutoff from the axial `ε_n`). Wave ports remain TE-only (#808).
- The TE-only TM guard's margin is mesh-aware (#824). It is `tm_guard_margin(k_c, h_n) = max(TM_GUARD_MARGIN, TM_GUARD_AXIAL_COEFF·(k_c·h_n)²)` with `TM_GUARD_AXIAL_COEFF` = 0.025; the measured basis is in the `TM_GUARD_MARGIN` docs. `PortFaceProjection::guide_axial_spacing(mesh, reach)` gives `h_n`: the largest axial extent of a tet on the face or with its centroid over the face within `reach` of the port plane. `tm_guard_axial_reach(k_c, k)` gives the window, `2π/min(k_c, k)`. `TmCutoffEstimate` gains an `axial_spacing` field (0 from `from_levels`, so the old 5 % guard is unchanged) and new methods `with_axial_spacing`, `k_c`, `margin`, `axial_kh`, `axial_spacing_admitting` and `base_margin_axial_spacing`. The constant `TM_GUARD_MEASURED_KH` is the top of the measured range. The `tm_cutoff_estimate` docs now say which order clamp only lowers the estimate: the cap at p = 2. The floor at 0.5 raises it.
- `PortFaceProjection::guide_axial_spacing` counts a tet when its nearest point to the port plane is within `reach` (its centroid was), and `tm_guard_axial_reach` is now `max(TM_GUARD_REACH_CUTOFF_WAVELENGTHS·2π/k_c, 2π/k)` with the new constant at 3, which contains the old one-wavelength window (#845). New `PortFaceProjection::guide_axial_mesh` returns a `GuideAxialMesh`: `h_n` in the window, plus the coarsest axial extent over the whole face footprint and how near the port the first coarser tet starts. New `PortFaceProjection::guide_coarser_than_distance` gives where a refinement to `h` has to start. New `tm_evanescent_leak(k_c, k, d)` = `exp(−√(k_c² − k²)·d)`. New ignored test `a_coarse_section_beyond_one_wavelength_does_not_slip_under_the_tm_guard` (`tests/wave_port.rs`) reproduces the Judge's three probes with a TM-like mode classifier: the old readings put the guard above the 3-D mode and the new ones put it below.
- `analytic::waveguide::solve_dielectric_modes2` now returns the guided modes of weakly-guiding fibers (for example SMF-28). Every in-window eigenpair of its pencil obeys the exact Rayleigh identity `r = 1 − n_eff²/⟨ε⟩_x`, so its curl ratio is below `(ε_max − ε_min)/ε_max` (`7.8e-3` for SMF-28). The old fixed curl-energy floor (`3e-2`) sat above that bound, so it rejected every guided mode. What it returned as the "fundamental" was an unconverged Lanczos Ritz vector, or nothing, depending on platform rounding and mesh. The floor now scales with the contrast and is capped at the old value, so the Si/SiO₂ floor is unchanged. The solver also rejects in-window pairs that break the Rayleigh identity (unconverged Ritz vectors). The #449 grad–div audit is re-measured on the genuine fundamentals, and its verdict is corrected in `docs/formulation_audit_reduced_vs_full_vector.md` (#791).
- `analytic::waveguide::solve_dielectric_modes2` and `solve_dielectric_modes` (p=1) no longer pass unconverged Lanczos Ritz pairs to their mode filters. The Lanczos solve returned its requested window after `n_request + 8` steps without a convergence check, and the tail of that window reached the filters. The #791 Rayleigh-identity check caught only grossly unconverged pairs: its violation is second order in the residual, so a pair with residual `1e-2` could pass it. Both solvers now use the residual-checked solve, with tolerance `1e-8` and a cap of 6× the old step budget. Unconverged in-window pairs extend the run until they converge; the rest are withheld, and the solver log counts them. The Rayleigh check stays as a second line and now also applies at p=1. Converged results are unchanged (SMF-28 timing mesh 1.448239, Si slab 2.910844, the SOI, mixed-pencil and audit benchmarks), and the SMF-28 timing solve takes 0.35 s instead of 0.25 s (#798).
- `analytic::waveguide::solve_waveguide_modes`, `solve_waveguide_modes_with_opts` and `solve_rect_waveguide_modes` (the metallic port-mode solver behind wave ports), and `solve_rect_waveguide_modes2_cutoffs`, use the residual-checked Lanczos solve as well. They return only converged modes. An undercount, or an unconverged pair below the last returned mode, triggers the existing budget-doubling retry, and the error names the shortfall. On the `b/16` guide, `n_modes = 12` returned TE₃₁ off by `1.06e-8` and merged the near-degenerate pairs at `k_c² ≈ 39.31` / `49.34` (found in PR #809). It now matches the `n_modes = 20` solve to `1e-12`. The same tail also dropped degenerate partners: the circular guide's second TE₁₁ polarization, and the 2:1 rectangle's TE₂₀/TE₀₁ pair in the p=2 cutoffs. Both are now returned. Wave-port S-parameters change only at round-off (≤ 1e-14), except where a returned port mode was itself unconverged. The CLI vacuum golden `PURE_WAVE_BITS` is re-recorded because this change deliberately converges the port mode, so the old bits cannot be reproduced. They recorded an unconverged second mode of the 8×4 port face (the near-degenerate TE₂₀/TE₀₁ pair) from one unchecked pass: relative residual `3.595e-4`, eigenvalue `6.8e-10` (relative) off. That tail was deterministic on every platform. Converging it moves S by up to `6e-9` normwise everywhere. `MIXED_BITS` is untouched (within `1.8e-15`), and the core test `vacuum_medium_is_bit_identical_to_the_pre_fill_formulas` does not touch the mode solve (#798).
- `analytic::waveguide::solve_dielectric_modes` (p=1) now uses the contrast-scaled curl-energy floor from #791 (`clamp(1e-2 · (ε_max − ε_min)/ε_min, 1e-6, 3e-2)`). The fixed `3e-2` rejected every guided mode at weak index contrast (Δn/n ≲ 1.5 %); for example, the p=1 SMF-28 fundamental (`r = 9.2e-4`) is now returned. Si/SiO₂ cross-sections keep exactly `3e-2` (#794).
- `FaerComplexEigensolver` (and so `self_consistent_k` / `self_consistent_k_vector_tracked`) no longer hangs on complex pencils above about 500 DOF. It called faer 0.24's generalized complex QZ, which turns the matrices into NaN when many shifts come from a degenerate eigenvalue cluster (the Nédélec gradient null space) and then runs its `30·n` sweep cap without reporting an error; a 3300-DOF Mie pencil ran for 14.8 h. The solver now computes the full spectrum by dense shift-invert (LU of `A − σB`, then faer's standard complex Schur QR of `(A − σB)⁻¹B`). The eigenvalues match the old solver's to about `1e-12` relative, and the 3300-DOF pencil takes about 11 s. Pencils larger than the new public `eigen::complex::MAX_DENSE_COMPLEX_DIM` (6000) are refused with the new `EigenError::DenseTooLarge { dim, max }`. Non-finite input or output returns `EigenError::FaerGevd` instead of NaN eigenvalues. `EigenError` is not `#[non_exhaustive]`, so a downstream exhaustive `match` on it needs a new arm (#796).
- `FaerDenseEigensolver` (`smallest_eigenvalues` / `smallest_eigenpairs`) is no longer many times slower from about 590 DOF, and no longer returns inaccurate or spuriously complex eigenvalues on real pencils of a few hundred DOF and up. It called faer 0.24's generalized real QZ, which from 590 DOF spins its aggressive-early-deflation window QZ to an iteration cap (5.7 s at 600 DOF against 0.36 s at 560), and whose blocked path lost accuracy on symmetric-definite FEM pencils: on sub-blocks of the bundled Mie pencil `(Re K, Re M)` its physical eigenvalues were off the symmetric reference by `9e-5` at 250 DOF, `8e-4` at 600 and up to `0.57` at 1200, with spurious complex-conjugate pairs. The solver now uses dense shift-invert (LU of `K + τM`, then faer's standard real Schur QR of `(K + τM)⁻¹M`, with complex-shift fallbacks), which matches the symmetric reference to about `1e-12` relative and takes 0.15 s at 600 DOF and 8 s at 3300 with eigenvectors (46 s before). Pencils larger than the new public `eigen::dense::MAX_DENSE_REAL_DIM` (8000) are refused with `EigenError::DenseTooLarge`, whose message now names the real and the complex sparse solvers; non-finite input returns `EigenError::FaerGevd`. Ordering, the `ComplexEigenvalue` / `SingularPencil` errors and the M-normalized eigenvectors are unchanged (#800).
- The release test `transmon_eigen_sensitivity::real_eigen_sensitivity_release` now FD-validates a physical mode and runs in CI (the `core-ignored` job). On the μm-unit 133k-tet transmon mesh it checked a gradient null-space mode (λ ≈ 1e-17, 0.0002 GHz) and failed on Linux and macOS. The test's mode picker skipped null-space modes with an absolute `λ ≤ 1e-3` cutoff, but every physical λ on a μm mesh is about 1e-8, so it skipped them all and fell back to index 0. The picker now uses the σ-relative criterion `λ ≤ PecCavitySettings::DEFAULT_NULL_TOL_REL · σ`, which the production cavity solvers and the `geode` CLI already used, and panics if no physical mode was returned. The test's interior-node finder also assumed the unit cube; it now chooses a node by topology (all incident edges unconstrained). The picked mode is now λ = 6.25e-9 (3.77 GHz). The material gradient matches the FD and the closed form `−λ/ε` to the printed digits, and the geometry gradient matches the FD to a relative 1.9e-5. The tolerances are unchanged. A new regression test checks that the same cavity meshed in metres and in micrometres gives the same picked mode. No library code changed (#814).
- Two eigen thresholds no longer depend on the mesh length unit (#826). `eigen::self_consistent::q_factor` (now public, with `Q_LOSSLESS_REL_TOL`) reported `Q = ∞` when `|Im k| < 1e-12`, an absolute bound in rad per mesh unit: on a μm mesh at 5 GHz every mode with `Q ≳ 5e7` was infinite, against `Q ≈ 5e13` in metres. The mode is now lossless only when `|Im k| ≤ 16ε·|Re k|`. `FaerDenseEigensolver` rejected a kept complex pair only when `|Im λ| > 1e-9·max(|Re λ|, 1)`, so below `|λ| = 1` the test was absolute and at `λ ≈ 1e-8` a 10 % imaginary part passed as real. The floor is now the pencil scale `τ` (the shift scale, `∝ 1/L²` like `λ`), which still keeps round-off null eigenvalues real. The fiber `analytic::mixed_pencil::solve_mixed_modes` had the same `max(|β²|, 1)` floor in its real-eigenvalue and duplicate tests: on an nm mesh (`β² ≈ 3.5e-5`) the duplicate test merged all 14 in-window modes of the SMF-28 decoupled ladder into one. Both tests are now relative (every kept `β²` is in the window, so positive). Each fix has a two-unit regression test. Eigenvalues, modes and `Q` values are bit-identical; only the classification moves, and only where an old floor was active (`|Im k| < 1e-12`, `|Re λ| < 1`, `β² < 1`). For a pencil with `τ > 1`, an eigenvalue below `τ` may now carry up to `1e-9·τ` of imaginary round-off (before, `1e-9`).
- `Im k` no longer cancels at high `Q` (#830). Every `λ → k` conversion (`lossy_cavity::principal_k0`, the self-consistent driver's `k = √λ`, and `geode_util::eigen::k_from_lambda`) and the complex Lanczos's M-bilinear norm `β = √(wᵀMw)` computed `Im √z = √(½(|z| − Re z))`. That form loses `≈ log₁₀(2Q²)` digits (relative error `≈ ε·Q²`: `2e-5` at `Q = 1e6`) and is exactly 0 from `Q ≈ 6.7e7`, in any mesh unit. All four now call one shared helper, the new public `eigen::wavenumber::principal_sqrt`, which evaluates only the non-cancelling half (`Re k = √(½(|z| + Re z))`, `Im k = Im z / (2 Re k)`; mirrored for `Re z < 0`). Against `mpmath` at 50 digits it is correctly rounded on every test point from `Q = 1e2` to `1e14`, in mm-like and m-like units and in all four quadrants. Branch and sign conventions are unchanged (`Re k ≥ 0`, `sign Im k = sign Im λ`, `Im k = +0` exactly for real `λ ≥ 0`, so a lossless mode still has `Q = ∞`). In the complex Lanczos the lost `Im β` left the basis M-normalized only to `≈ √ε` on weakly lossy pencils, so its Ritz values degraded from `Q ≈ 1e7` up (see the `geode` CLI entry). At moderate `Q` every `Im k` changes in its last bits only: over the committed sphere Mie/PML fixtures (`Q ≤ 1.2e3`) the old `Q` was off its `mpmath` value by up to `2.2e-10` relative and the new one by `≤ 4.5e-16`. No golden or tolerance changed. `analytic::waveguide`'s private `principal_sqrt_c64` (the leaky `n_eff` of the complex PML port pencil) has the same form and is left for #831, because PR #812 is editing that file.
- `eigen::lossy_cavity::solve_lossy_cavity_modes` (the lossy/open `geode eigen` path) no longer fails on a spurious Ritz value near `σ` (#830). The complex-symmetric Lanczos has no interlacing guarantee, and its full Ritz set can contain values that approximate no eigenvalue. On the uniformly filled `tan δ = 0.1` sphere (x86-64 Linux, 4 threads) one such value, `λ = 0.4756 − 0.0767j` with relative residual 11, sat between the converged modes, displaced one from the closest-to-`σ` selection and failed the `residual_tol` gate. Whether it appears depends on the last bits of the iteration. At 2 threads, 1 of 12 loss levels failed both before the `β` fix above (at `tan δ = 0.08`) and after it (at `0.12`). At 4 threads, the committed `0.1` case failed after it. The selection now passes over candidates with relative residual `≥` the new `SPURIOUS_RESIDUAL_REL` (1.0, i.e. no correct digit), but only when that yields `n_modes` modes that all pass `residual_tol`. Otherwise it falls back to the old selection, so every failing run fails as before. Residuals are now computed for every candidate (two SpMVs each). The same sweep plus `tan δ = 0.01`, `5e-9` and `1e-10` (30 runs) now passes, with `Q` within `2.1e-13` of `½·cot(δ/2)`. (Before release, the residual-checked complex Lanczos (#834, below) replaced this downstream filter: `SPURIOUS_RESIDUAL_REL` and the `n_spurious_filtered` field it added are gone.)
- `eigen::lossy_cavity::solve_lossy_cavity_modes` (the lossy/open `geode eigen` path) now uses the residual-checked complex Lanczos (`smallest_eigenpairs_checked_filtered`, #834). It never returns an unconverged Ritz pair: when one ranks among the `n_modes` physical pairs nearest σ, it is withheld and the same Krylov run is extended (to at most `2 · max_iters` steps) and the result is the `n_modes` converged modes nearest σ, or an error. A withheld pair that is not *localized* (`ρ · max(|λ|, σ) > |λ − σ|`, such as the #833 value with residual ≈ 11) locates no eigenvalue and is skipped. A localized one still unconverged at the cap, nearer σ than the farthest returned mode, fails with `NotConverged` rather than let a farther mode fill its slot. This replaces the #830 spurious-residual skip, which only handled values with relative residual `≥ 1` and still failed with `NotConverged` on one between `residual_tol` and 1. When the first `max_iters` pass converges (every existing test and golden), the modes are bit-identical. If the cap is reached first, the error is the same `NotConverged` (the worst unconverged pair among the `n_modes` nearest σ) or `TooFewModes` as before. `LossyCavityModes` loses the unreleased `n_spurious_filtered` and gains `n_withheld` and `lanczos_steps` (it is not `#[non_exhaustive]`, so a downstream struct literal needs them).
- The PML dielectric mode solvers (`analytic::waveguide::solve_dielectric_modes2_pml` and its profile-selected and scored variants) and the analytic-cladding (DtN) self-consistent loop (`solve_dielectric_modes2_analytic_cladding_bc`) use the residual-checked complex Lanczos with the guided `Re β²` window, tolerance `1e-8` and a 6× step cap, the same settings as the PEC dielectric solves since #798 (#834). Their converged selections are unchanged: SMF-28 `Re n_eff = 1.44903016`, the high-contrast fiber `1.47668266`, LP₀₁ profile selection `b_fem = 0.7593`, and every analytic-cladding iterate print the same digits as before. The analytic-cladding loop fails (`EigenError::FaerGevd`) when a localized withheld pair, a genuine eigenvalue still unconverged at the cap, lies nearer σ than the fundamental it selected. The PML classifier solves keep reporting withheld in-window pairs instead: their window holds leaky and continuum pairs that legitimately stay unconverged, so a withheld pair nearer σ than some returned continuum mode is no hole. On SMF-28 the PML solve extends to the 288-step cap and withholds 2 in-window pairs that never converge, which takes it from 0.10 s to 1.4 s; the log line reports it.
- Krylov thresholds no longer depend on the mesh length unit (#828). The β-bound convergence probe of the real, projected and complex shift-invert Lanczos (`SparseShiftInvertLanczos`, `ProjectedShiftInvertLanczos`, `SparseComplexShiftInvertLanczos`) was `β ≤ tol · max(μ_max, 1)`, where `μ = 1/(λ − σ)` scales as `L²`. It was absolute, and up to `1/μ_max` times too loose, whenever `|λ − σ| > 1`: about `1e4×` on a metre-unit mesh at GHz, so `pec_cavity` / `lossy_cavity` reported `NotConverged` and the PML / waveguide solves stopped early. It is now `β ≤ tol · μ_max`. The Lanczos breakdown test (`β < 1e-14`) is now `β < 1e-14 · max |α_j|`; on an SI-scaled pencil (`|μ| ~ 1e-13`) the old cut ended the run after one step. The complex start-vector isotropy test (`|vᵀMv| < 1e-30`) is now relative to `Σ |v_i| |(M v)_i|`. The Arnoldi breakdown (`h_{j+1,j} < 1e-12`) and null-`ν` (`|ν|² < 1e-30`) tests of `analytic::mixed_pencil` and `analytic::lossy_port_modes` (behind the hybrid wave ports) are now relative to the running `max |h_ik|`; an optical guide meshed in metres (`|T| ~ 1e-11`) sat at the old cut. Results are unchanged where the old floors were inactive (`|λ − σ| ≤ 1`, every µm- and mm-unit CLI solve). On unit-scale test pencils, where `|μ| < 1`, the probe now runs longer and the converged values move only at round-off: at most `4e-15` in S and β across the wave-port, hybrid-port and cavity suites. The Whitney cross-check in `hybrid_port_modes` tightens from `≤ 2.7e-12` to `≤ 2.6e-14`. Regression tests solve the same problem at mesh scales `1`, `1e-6` and `1e3` (cube cavity: real plain and eigenvalue-only, projected, complex plain, eigenvalue-only and checked; `pair_hint`), the real and complex Arnoldi on a pencil scaled by `1e13` and `1e-6` (with the old cut the `1e13` run stopped after one step), and SMF-28 in µm, nm and metres (`mixed_pencil_fiber_benchmark`; on that mesh the old Arnoldi cut does not yet fire in metres). Each fails with its old threshold restored.
- `analytic::waveguide` unit-dependent floors (#828): the bound-mode classifier of the PML solvers (`|Im β²| ≤ 1e-8 · max(|Re β²|, 1)`) and the analytic-cladding loop's convergence test (`|Δβ²| / max(β², 1)`) are now relative to `β²`. The general metallic solver's probe shift was `1e-10 · max(tr M / n, 1)` (the mean mass diagonal is not an eigenvalue scale) and its gradient-cluster floor the absolute `1e-12`. They are now `1e-10 · τ` and `1e-12 · τ`, with `τ = tr K / tr M`.
- `driven::ports::hybrid` `pair_hint` (the "refine the port face below h ≈ …" estimate for a mesh-induced complex pair) matched the refined face's nearest pair within `½ · max(|Re β²|, 1)`. On a µm-unit face at GHz (`β² ~ 1e-8 µm⁻²`) that is an absolute `½`, which matched any pair. It is now `½ · |β²|` (#828).
- `analytic::waveguide::principal_sqrt_c64` (`β` and `n_eff` of the complex PML port pencil) now delegates to the cancellation-free `eigen::wavenumber::principal_sqrt` (#831), with the same branch (`Re ≥ 0`, `sign Im = sign Im β²`, `0 → 0`). The old `Im √z = √(½(|z| − Re z))` gave a weakly leaky mode's propagation loss `Im n_eff` with relative error `≈ ε·(Re β²/Im β²)²`: `1.3e-5` at `Im/Re = 1e-6` (`1.1e-4` in metre units), and exactly 0 (lossless) below `≈ 1.5e-8`. It is now within `4ε` of `mpmath` at `1e-6` and `1e-10`, in µm and metre units. Golden review: no committed artifact or test asserts a leaky `Im n_eff`. The PML fiber benchmarks print `relIm` (`2.96e-17 → 2.95e-17`, `1.08e-15 → 1.07e-15`), and every `Re n_eff` is unchanged.
- New `PortFaceProjection::guide_scan` returns a `GuideScan`: the guide footprint's tets, scanned once, with `axial_mesh(reach)` and `coarser_than_distance(h)`. `guide_axial_mesh` and `guide_coarser_than_distance` delegate to it (#848).

#### `geode-util`

- `eigen::dense_lowest_eigenvalues` / `dense_lowest_eigenpairs` no longer call faer 0.24's generalized real QZ (`generalized_eigen` → `qz_real`). That QZ is inaccurate in the middle and upper spectrum of these pencils, and it returns spurious complex pairs, which the helpers silently dropped, shifting every later index. On the 3300-DOF sphere-PEC pencil, against a symmetric Cholesky reference, the old QZ was 1.4e-3 off at λ[379], 0.45 at worst over the physical spectrum, and dropped 32 complex eigenvalues; the compared λ[0..375] were already accurate to about 2e-12. The helpers now delegate to `FaerDenseEigensolver` (dense shift-invert, #800), which is within 1.2e-12 over the whole physical spectrum. They return `Result<_, EigenError>`, so a complex eigenvalue among the requested lowest modes is an `EigenError::ComplexEigenvalue` error, not a silent drop. `dense_lowest_eigenpairs` now does the per-cluster M-Gram–Schmidt its docs always promised (the old code only M-normalized). `dense_lowest_eigenvalues` no longer computes eigenvectors, so the sphere-PEC reference solve drops from about 42 s to about 5 s in release (#813).
- `eigen::dense_lowest_eigenpairs` groups eigenvalues into degenerate clusters (for its per-cluster Gram–Schmidt) with the gap `1e-8 · max(|λ|, τ)`, `τ = tr K / tr M`, instead of `1e-8 · max(|λ|, 1)` (#828). On a µm-unit mesh the old absolute floor split an exact null cluster into separate clusters, and on a nanometre one it merged the whole low spectrum into one. `τ` scales with the eigenvalues, so the clustering no longer depends on the mesh length unit.
- `eigen::q_factor` (and so the CLI eigen report's `q`, and `q_factor_from_lambda`) now calls `geode_core::eigen::self_consistent::q_factor`: its lossless cutoff is relative to `Re k` rather than the absolute `|Im k| > 1e-12`, so `Q` does not depend on the mesh length unit (#826).

#### Tests and CI

- **CI test coverage** goes from 41 to 157 of 160 integration-test targets.
  - A new `ci-test-coverage` workflow runs a fail-closed guard (`scripts/ci-test-coverage.py`, with a 90-case self-test). It counts a `cargo test` only if it provably runs and its failure fails the job, and it tracks both test targets and `#[ignore]`d tests per test.
  - New `integration-tests` jobs: `core-tests`, `validation-tests`, `core-dense-eigen` and `core-ignored`.
  - `cube-cavity-tolerance` now runs under `shell: bash`, so a failure piped through `tee` fails the step.
  - The remaining 3 allowlisted targets are slow benchmarks, tracked in #786. (#785, #786, #788, #790, #793)
- The sparse-vs-dense Mie complex eigensolver test is split around a committed dense-oracle fixture (shift-invert, cross-checked by a capped QZ to 1.4e-11); its sparse half runs in CI in about 3 s (#710).
- The geode-cli release `--ignored` tiers run in a parallel job, fixing main CI timeouts (#772). Tests use panic-safe tempfile scratch dirs (#766).
- `arpack.yml` now runs the `eigen::complex::`, `eigen::lossy_cavity::` and `eigen::projection::` lib tests (the #834 checked-solve and seeded spurious-Ritz regressions, and the #828 mesh-unit invariance tests) next to the #798 `eigen::lanczos::` / `analytic::waveguide::` step, and the `geode-util` lib tests (`dense_lowest_eigenpairs` clustering, `q_factor`, `k_from_lambda`), which no job ran before.
- The 22 `#[ignore = "faer 0.24 qz_real panics under debug-assertions"]` reasons are gone. No in-tree solver reaches `qz_real` any more; only `faer_qz_debug_overflow_guard` calls it, on purpose. The `derham_gauge_invariance` full-spectrum GEVD also moved off a direct `generalized_eigen` call. Tests that pass in debug in under about 30 s are un-ignored. The rest are ignored in debug builds only (`#[cfg_attr(debug_assertions, ignore = "slow in debug …")]`) and run in the default tier in release. As a result, the eigensolve tests of `eigensolver`, `nedelec_cavity`, `cube_cavity_{jax,julia,onnx}_reference` and `sphere_pec_{jax,julia,numpy,onnx,tfjava}_reference` now run in CI. `arpack.yml` runs `sparse_eigensolver`'s default tier, and `cube-cavity-tolerance.yml` runs `cube_cavity_numpy_reference`'s default tier, instead of their `--ignored` tiers, which would now select nothing (#813).
- `--ignored` benchmark runs no longer rewrite committed artifacts (#823). `fiber_dispersion_benchmark`, `transient_sparams::transient_self_oracle_broadband` and `sparse_complex_eigensolver::regenerate_dense_oracle_fixture` wrote `benchmarks/fiber_dispersion/results.toml`, `benchmarks/transient/results.toml` and `tests/fixtures/mie_complex_pml_dense_eigenvalues.toml` into the checkout on every run, including in CI's `core-ignored` job. The fiber benchmark wrote before its asserts, so a failing local run overwrote the committed numbers with failing ones. Each test now writes under `CARGO_TARGET_TMPDIR` by default and writes the committed path only with `GEODE_BLESS_FIBER_DISPERSION=1`, `GEODE_BLESS_TRANSIENT=1` or `GEODE_BLESS_DENSE_ORACLE=1` (following `GEODE_BLESS_WAVEGUIDE_MSH`). The fiber headline also checks its fresh numbers against the committed `results.toml`: every D within the 1 ps/(nm·km) bar, every ZDW within the 10 nm bar and each FEM `n_eff` within 1e-6. A drift fails with the bless command. The regeneration commands in the artifacts' headers, the provenance manifest and the CI comment name the bless variables.

## [0.7.0] - 2026-10-01

This release completes the `geode` CLI's EDA-flow epic (#702). It adds:
- lossy and open-cavity eigenmodes;
- design sensitivities for every analysis;
- a scalable iterative driven path (an AMS preconditioner) with an adaptive frequency sweep and parallel frequency points;
- published JSON Schemas, a cookbook and prebuilt binaries.

It also fixes several precision and convergence bugs that could silently degrade results.

### Added

#### `geode` CLI

- `geode eigen` solves lossy and open-cavity problems (lossy dielectrics, UPML), reporting complex frequency, f and Q per mode (#706, #737).
- **Design sensitivities:** a new `sensitivity` spec section. Every gradient is computed by an adjoint, and an optional `fd_check` verifies each one against finite differences (#707, #741, #739, #750). Available observables:
  - capacitance: two-terminal C at P2, as `c_farad_p2`;
  - inductance: L_ij;
  - lossless eigen frequencies;
  - driven: |S11|², for one lumped port.
- **Scalable driven solves** (#708):
  - `solver.preconditioner` selects `jacobi` (default), `ilu0` or `ams` for iterative solves (#742, #744, #752).
  - `ams` is a Hiptmair–Xu auxiliary-space preconditioner with an exact nodal coarse solve. On the spiral inductor it converges where Jacobi and ILU(0) stall. At 228k edges it takes 33 s and 1.7 GB, against direct LU's 73 s and 17.8 GB. A spec with floating PEC conductors is rejected up front with `invalid_spec`.
  - `sweep.adaptive {tolerance, max_snapshots}` runs a Galerkin reduced-order sweep. Interpolated rows carry an error indicator. A frequency that misses tolerance, or whose reduced solve is singular, falls back to an exact solve (#745, #747, #753).
  - `--jobs N` solves frequency points in parallel. Each job gets `max(1, threads/N)` threads, so jobs don't oversubscribe the CPU (#745, #753, #755, #762).
  - `--progress` writes JSONL progress events to stderr (#745).
- **Mesh:** `geode mesh --analysis capacitance|inductance` emits starter specs with nets, ground and contacts (#720, #727).
- **SPICE:** `--spice-positive-k` and `--spice-ret-pin` export variants (#723, #733).
- **`geode check`:** reports `anchor_nnz_ratio` and `above_anchor` resource-estimate fields, plus an AMS memory estimate (#713, #735, #752).
- **Touchstone:** the output path is checked before the solve, and the export is round-trip-checked with scikit-rf in CI (#713, #735).

#### Distribution (#709)

- Published JSON Schemas for the spec and the report (`geode schema`, `crates/geode-cli/schemas/`, with a drift test), plus an examples cookbook in `crates/geode-cli/examples/` (#728).
- Prebuilt `geode` binaries for linux x86_64/aarch64 (glibc ≥ 2.35) and macOS arm64, attached to each GitHub Release, plus a Gmsh-equipped container (`docker/geode-cli/`) (#726).

### Changed

- **Iterative driven solves fail honestly:** a solve whose recomputed explicit residual misses `solver.tol` now fails with `solve_failed`, even if the recursive residual converged. The error suggests loosening `tol` (#744, #749).
- **`--threads 1` and `GEODE_NUM_THREADS=1` now mean a serial LU.** Previously faer's LU still used every core. Results at one thread can change at roundoff (#753, #755, #762).
- **`--jobs` bit-identity:** `--jobs N` matches `--jobs 1` bit for bit only at the same threads per factorization. Otherwise the results agree to roundoff, about 1e-14 relative (#753).

### Fixed

- **Nédélec precision:** material and source weights are now uploaded at backend precision (f64). Before this, ε was rounded to f32 (#740, #746).
- **Surface triangles:** a surface triangle that is not a tet face returns `SurfaceNotOnMesh` instead of panicking, with a per-tet-face check and an integration fixture (#725, #729, #732, #751).
- **SPICE `ret` pin docs:** the floating `ret` caveat (it needs a DC path to `0`) is now documented, and the README usage lines are fixed (#736, #738).

### API notes (geode-core, pre-1.0)

- `waveguide_mode_reduce` now returns `Result`.
- `RomError::UnsupportedOperator` is removed.
- The capacitance sensitivity observable is renamed `c_farad` → `c_farad_p2` (#748).
- New: `driven::solve_ams` and `IterativePreconditioner::Ams`, `eigen::parallel::ParallelismGuard::cap` and `with_thread_budget`.

The spec and report schemas stay at v1; every CLI change is additive.

## [0.6.0] - 2026-09-29

This release turns the `geode` CLI into a static parasitic-extraction tool
for EDA flows (Epic #702): layout geometry goes in, and tagged meshes,
Maxwell capacitance and inductance matrices, and SPICE subcircuits come
out, alongside the Touchstone S-parameters added in 0.5.0.

### Added

#### `geode` CLI — EDA flows (Epic #702)

- `geode mesh`: turns a versioned, tool-neutral layout description
  (rectilinear conductor polygons, a dielectric layer stack, lumped gap
  ports, PEC or UPML outer boundary) into an MSH 4.1 mesh with
  automatically named physical groups, plus a starter problem spec, by
  driving an external `gmsh` binary (`--gmsh` / `GEODE_GMSH` / `PATH`,
  `--gmsh-timeout`). The report records the Gmsh version and sha256 of the
  layout, script and mesh. Golden test: the spiral inductor rebuilt from a
  layout, L −4.3 % vs Mohan / −6.3 % vs MoM (#704, #712).
- `geode capacitance`: Maxwell capacitance matrix between named conductor
  surfaces, with one or more grounded surfaces. Golden test: coaxial and
  triaxial lines, ≤ 0.2 % vs analytic on the benchmark mesh (#705, #716).
- `geode capacitance --spice <PATH>`: exports the matrix as a mutual-C
  SPICE `.subckt` (ground branches from row sums, noise-level branches
  dropped and listed, significant sign violations rejected), verified by
  an ngspice AC admittance check (#715, #717).
- `geode inductance`: static inductance matrix for open current paths
  (conductor volume from a source face to a sink face), with a new
  `materials[].mu_r`. Source and sink must sit on the same connected
  piece of the required PEC wall, checked before solving; a non-physical
  L (non-SPD or L_ii ≤ 0) fails with `solve_failed`. Golden test: coax
  and triax, ≤ 0.35 % vs analytic on the benchmark mesh, including a
  μ_r = 4 core (#714, #718).
- `geode inductance --spice <PATH>`: exports the L matrix as an
  `LEXTRACT` `.subckt` (one inductor per path to the PEC return,
  `K` couplings with sign preserved), verified by an ngspice AC check
  (#719, #722).
- Spec and report stay schema v1; all changes are additive.

#### Library

- `geode_core::assembly::current_path`: general open-path current
  excitation. A P1 conduction solve gives J = −σ∇φ normalised to 1 A,
  discretely divergence-free and verified against the analytic
  wire/loop builders. Grounded-component analysis with a per-path
  current-balance check (`CurrentPathError::UnbalancedGround`) (#718).
- Electrostatic face-flux helpers made public (#718).

### Changed

- `materials[].eps_r` now defaults to vacuum (1) for every analysis, so a
  spec that omits it now parses where it used to fail. A non-unit `mu_r`
  is rejected outside `geode inductance` (#718).
- CI installs Gmsh (4.12.1 on Ubuntu) for the `geode-cli` job and fails,
  rather than skips, the Gmsh tests if it is missing. It also runs the
  inductance golden benchmark tier in release mode (#712, #718).

### Fixed

- `geode mesh` now hollows thick conductors (cut out of the dielectric
  and any sheets they pierce), so Leontovich R/Q match the benchmark:
  spiral R −5.9 % → +0.16 %, Q +6.7 % → +0.76 %. Coplanar-wall merging
  is disabled so no cavity face is left untagged, and new mesh-time and
  solve-time guards reject untagged or dangling surface triangles
  (#721, #724).

## [0.5.0] - 2026-09-28

This release completes the `geode` CLI's second phase: alongside `check`
and `driven`, it now computes cavity eigenmodes (`eigen`), extracts L/R/Q
with a quasi-static f→0 L₀ extrapolation (`extract`), handles open
radiating problems with wave ports and UPML / Silver-Müller boundaries,
exports fields and far-field patterns, and writes Touchstone files for
EDA tools. Every committed benchmark artifact produced through a Burn
backend was audited for float precision. Three had been generated in
f32, and CI now enforces f64 provenance. The audit also uncovered a
singular σ = 0 shift that had silently broken the sparse Mie eigensolve;
it is fixed and now guarded.

### Added

#### `geode` CLI — Phase 2 (Epic #680)

- `geode eigen`: lossless PEC-cavity eigenmodes via sparse shift-invert
  Lanczos, with a residual acceptance gate (`eigen.residual_tol`) so a
  non-converged solve fails with `solve_failed` instead of reporting wrong
  modes. Golden test: dielectric sphere in a PEC cavity vs analytic Mie
  roots (#681, #685).
- `geode extract`: per-frequency L/R/Q, the quasi-static L₀ via
  Richardson extrapolation (optional `l0_rel_tol` gate) and SRF. Golden
  test: SLCFET 3HP, L₀ −2.6 % vs MoM-PEEC (#682, #686).
- Open boundaries and wave ports in the problem spec: box UPML
  (`absorbing_regions`), Silver-Müller walls, and wave ports built from a
  tagged mesh face. Golden test: patch antenna, f_res −6.5 % vs the
  Balanis cavity model (#683, #688).
- `--outdir`: opt-in VTU field export and NTFF far-field results
  (directivity, gain, efficiency, principal-plane cuts). The NTFF box is
  validated before the sweep (#684, #694, #697, #699).
- `--touchstone <PATH>` on `driven` / `extract`: Touchstone 2.0 `.sNp`
  output (S-parameters, RI, per-port `[Reference]` impedances), referenced
  from the report with its sha256; wave-port specs and `eigen` are
  rejected up front (#703, #711).
- `geode check` reports `resources`: nnz(A) and order-of-magnitude
  memory/time estimates for the chosen solver, calibrated on a 1.16M-DOF
  run and documented with measured error bars (#703, #711).
- Spec and report stay schema v1; all changes are additive.

#### Library

- `geode_core::eigen::pec_cavity`: tagged mesh + named PEC group +
  materials → eigenmodes (#685).
- `geode_core::driven::extraction::extrapolate_l0`: public, panic-free
  replacement for two duplicated private helpers (#686).
- Wave-port construction from a tagged planar mesh face
  (`driven::ports::wave_face`) (#688).
- `EigenError::DegenerateShift`: the sparse complex and projected
  shift-invert Lanczos solvers now reject a degenerate σ ≈ 0 solve that
  collapses onto the gradient null space instead of returning spurious
  modes (#696, #701).
- Shared benchmark-provenance helpers in `geode-util` (`BackendInfo`,
  oracle-block carry-through on regeneration) (#689).

### Changed

- CI: a `benchmark_provenance` guard classifies every committed benchmark
  artifact and requires f64 provenance on Burn-backed ones. CI also now
  runs the default tiers of the spiral, SLCFET and patch benchmark tests,
  which never ran there before (#692, #698, #700).

### Fixed

- Benchmark artifacts that were generated on the f32 `wgpu` backend,
  now regenerated in f64:
  - SLCFET 3HP: Q and R had been ~2× off (#687, #689).
  - patch antenna: negligible drift (#690, #695).
- `examples/mie_sphere` used a singular σ = 0 Lanczos shift and on main
  returned only null-space modes. It now uses σ = 1.0, and the committed
  results were regenerated (TM₁,₁ ~5.7 %, TE₁,₁ ~1 %, TM₂,₁ 0.5–2.3 %)
  (#691, #693).
- The same σ = 0 bug in the ignored sparse Mie test and the
  `mie_end_to_end` benchmark; earlier sparse timings from that
  benchmark are invalid (#696, #701).
- A stray tracked `.loom-managed` worktree marker (#679).

## [0.4.0] - 2026-09-27

This release gives GEODE-FEM a headless command-line driver: a `geode`
binary that takes a mesh plus a JSON/TOML problem spec and emits a
versioned JSON report, so design flows and agents can run validated solves
without writing Rust. It also closes two latent hazards found along the
way — a spiral-inductor reference artifact that had silently been generated
in f32, and lint debt outside `geode-core` hidden by a narrowly scoped CI
gate.

### Added

#### `geode` CLI — Phase 1 (#673)

- New `crates/geode-cli` crate producing the `geode` binary, installable
  with `cargo install --locked --git … --rev <sha>` (#676).
- `geode check` validates a spec and mesh without solving (physical-group
  resolution, DOF counts); `geode driven` runs a lumped-port frequency
  sweep with PEC / Leontovich boundaries, direct-LU or iterative COCG, and
  reports Z/Y/S per frequency plus per-port L/R/Q/S11. `eigen` and
  `extract` are reserved and return `not_implemented`.
- Versioned problem-spec v1 (JSON or TOML) and report v1 schemas with
  explicit units, unknown-field rejection, stable error codes, and
  provenance (crate version, git sha, backend, mesh SHA-256); documented in
  `crates/geode-cli/README.md`.
- `geode --version` prints the git sha; the process exits non-zero on
  solver failure or non-convergence; `--backend` confirms the compiled-in
  backend and `--threads N` caps threading.
- Golden tests re-express the spiral-inductor benchmark as CLI input (smoke
  tier in default CI; full-mesh tier `#[ignore]`d) and assert agreement
  with both the committed results and the library (~1e-12).
- `geode_core::mesh::read_tagged_tet_mesh` / `TaggedTetMesh`: a public
  tagged-mesh reader with physical-group name lookup; the sphere, spiral,
  patch, and transmon fixture loaders now share it.

### Changed

- CI clippy gate widened from `geode-core` to the whole workspace
  (`--workspace --tests --features geode-core/arpack`), and a new CI job
  runs `cargo test -p geode-cli`; pre-existing lints fixed across
  `geode-util`, `geode-validation`, and the example crates (#675, #677).
- Vendored `.anvil/` tool install untracked and gitignored — it is a
  copy-installed tool tree, not project source.
- Loom orchestration tooling upgraded to 0.18.0.

### Fixed

- `benchmarks/spiral_inductor/results.toml` regenerated in f64. The
  committed artifact had been produced on the f32 `wgpu` backend, leaving
  R/Q at 1 GHz ~16–18 % off (L unaffected); no physics changed. Regeneration
  now carries the `[oracles.palace]` block through, a default-CI check
  requires `float_dtype = "F64"`, and the CLI golden test checks R/Q against
  the artifact (#674, #678).
- New Rust 1.98 clippy lint (`chunks_exact_to_as_chunks`) fixed in
  `geode-util` and `geode-validation` (#676, #677).
- Stale references to the retired `geode-cli` crate in `geode-app` docs.

## [0.3.0] - 2026-07-29

This release repositions GEODE-FEM as a differentiable-by-construction
complement to Palace: the full 2×2 sensitivity matrix (material + geometry ×
scalar + H(curl) EM) is FD-validated and mutation-tested, and it is exercised
end-to-end by a driven-Maxwell shape-adjoint chain that culminates in
many-DOF freeform inverse design of a curved conformal radiator. Alongside
the adjoint work, a matrix-free eigensolve path with a full Hiptmair–Xu AMS
preconditioner opens the road past the direct-LU memory wall, and a suite of
validated benchmarks lands: slotless-PM Arkkio torque T(θ) at 0.71 %, SMF-28
LP01 modal b at 0.88 %, and transmon capacitance/EPR extraction. The
workspace itself goes standalone (vendored Palace docker, external refs
scrubbed) and gains the `geode-app` harness, standalone example crates, and
the `geode-util` pre-core staging crate.

### Added

#### Differentiable sensitivities — the full 2×2 matrix (Epic #569)

- Discrete-adjoint material sensitivity ∂(observable)/∂ε on the scalar path
  (#570, #573) and geometry/shape gradient ∂(observable)/∂X (#571, #575),
  both FD-validated.
- H(curl) driven-EM material adjoint ∂/∂ε (#579) and geometry/shape adjoint
  ∂(EM observable)/∂(node coords) (#581), completing the matrix; complex-ε
  (loss-tangent) sensitivities on the driven adjoint (#598).
- Hellmann–Feynman eigenvalue sensitivities ∂λ/∂p (#600) and
  inductance-matrix reluctivity sensitivity ∂L/∂ν via the self-adjoint
  energy form (#615).

#### Driven shape-adjoint chain → freeform inverse design

- |S11|² shape-adjoint objective closure and the first driven-Maxwell
  optimization loop (#627); complex-ε lossy shape adjoint (#630);
  pinned-feed (#632) and moving-feed (#644) lumped-port terminations in the
  shape adjoint; box-UPML tensor-material adjoint at fixed Λ (#643).
- Composed open-radiator shape adjoint + full inverse-design capstone
  (#645); high-DOF freeform boundary parametrization with mesh-morph
  regularizer (#654); curved conformal radiator fixture (#653) and many-DOF
  freeform inverse design on it (#655).
- FDTD-density head-to-head baseline plus measured Meep runtime-scaling
  evidence for the intractability axis (#656, #657).

#### Matrix-free scale path and AMS preconditioning (Epic #547)

- Matrix-free Nédélec curl-curl + mass matvecs on Burn (#483), GPU-resident
  COCG over the matrix-free operator (#487), and
  `SolverMode::IterativeMatrixFree` on the driven pencil (#495).
- Matrix-free shift-invert Lanczos to scale past the direct-LU memory wall
  (#525), AMS-lite H(curl) preconditioner (#528), and a matrix-free MINRES
  inner solver for interior/indefinite shifts (#537).
- Full Hiptmair–Xu AMS cycle: three-space AMS as the SPD preconditioner for
  indefinite MINRES (#560), the vector-nodal ΠᵀAΠ block (#553), an
  O(node_dim) few-sweep coarse solve (#554) with a smoothed-aggregation AMG
  alternative (#566), and the `transmon_bench` scale harness (#552).
- Distributed groundwork: geometric k-way edge-DOF partitioner + halo map
  (#642) and a distributed matrix-free Krylov abstraction with a
  single-process mock collective (#646).
- Opt-in `InnerSolver::DirectCustomOrder` — custom fill-reducing LU ordering
  via faer's public deeper API (#543, #544).

#### Second-order elements

- 20-DOF second-order Nédélec tet element (#617) with an opt-in p=2 driven
  forward path + material adjoint (#621), p=2 PEC-cube eigensolve with a
  frequency-convergence gate (#622), and a p=2 shape adjoint via a dual
  element twin (#623).
- P2 Lagrange tets on the scalar magnetostatic path, retiring the 2.44 %
  B-field miss to 0.211 % at O(h²) (#472, #474), and on the electrostatic
  path with the adjoint retained (#608).

#### New solver physics

- 3-D electrostatic solver + Maxwell capacitance-matrix extraction (#481)
  and 3-D vector magnetostatics + inductance-matrix extraction (#512), with
  a 2-D scalar magnetostatic Poisson solver as the oracle rung (#460).
- Transient EM solver — generalized-α time integration with broadband
  S-parameters via DFT (#489); adaptive fast frequency sweep via greedy
  Galerkin PROM (#610).
- London superconductor surface BC (per-tag λ_L) on the driven and eigenmode
  paths, with ∂λ/∂λ_L via Hellmann–Feynman (#609).
- Divergence-free M-orthogonal projection for the eigen path, port-aware so
  the junction LC mode survives (#513, #515), plus a tree-cotree gauge
  module with its spectrum limitation pinned (#508).

#### Validated benchmarks

- Motor torque (Epic #448): PM magnetization sources and air-gap oracle
  (#463), Maxwell-stress + Arkkio torque extractors vs the loop T = m × B
  oracle (#464), and the driven slotless-PM locked-rotor T(θ) benchmark at
  0.71 % (#465), on multi-band annular meshes with ν heterogeneity oracles
  (#462).
- Optical fiber (Epic #339): full-vector mixed E_t–E_z Nédélec–Lagrange
  dielectric modal pencil hitting SMF-28 LP01 b within 0.88 % (≤ 1 % gate),
  with a regression tripwire for the old reduced-pencil artifact (#473,
  #477) and an audit of the ε-coupling term that pencil dropped (#461);
  analytic-cladding DtN fiber solver (#447); chromatic dispersion D(λ) + ZDW
  vs the analytic oracle (#482).
- Transmon (Epics #475/#476 groundwork): fixture ingestion + shared MSH tag
  scanners (#488, #494), Josephson junction as a lumped reactive shunt in
  the eigensolve (#496), EPR quantization + qubit parameters (#511), a
  differentiable capacitance→E_C chain (#586) driving gradient-based
  transmon-parameter optimization (#588) and island-pad shape optimization
  on the real 133k-tet mesh (#590), with harmonic mesh-morphing island
  deformation (#599).

#### New `geode-util` pre-core staging crate (Epic #414)

- Introduced `geode-util`, a pre-core staging layer above `geode-core` that
  collects shared helpers previously scattered across `geode-validation` and
  the example crates (module map `repo` / `convert` / `interop` / `fixture`
  / `viz`): fixture-repository helpers (#417), interop decoders (#418),
  edge-DOF → nodal reconstruction (#419), and the shared fixture
  TOML/pvd/sweep harness (#423). `geode-validation` now consumes these and
  retains only genuine validation-harness code, with JSON fixture
  loader/schema and serde glue migrated in Epic #429 (#434, #435, #437).

### Changed

- The workspace is fully standalone: Palace docker tooling vendored in-repo
  and all external repository references scrubbed (#542).
- Examples restructured into standalone top-level crates (mie, patch
  antenna, spiral inductor, SLCFET, waveguides, fibers; #402–#412) driven by
  the new `geode-app` clap harness with arg groups and a lifecycle seam
  (#400); example crates now depend on `geode-util` instead of
  `geode-validation`.
- Bunsen integration closed out (Epic #355): named shape contracts wired
  across the P1 gather cluster, assembly, and basis/eigensolver paths (#466,
  #467, #469) and the git pin swapped to crates.io bunsen 0.28.0 (#507).
- Assembly and factorization hot paths parallelized: rayon on the Nédélec
  host-side pattern/slot build (#538) and in faer's sparse-LU factorization
  (#521); M·v_j caching drops O(k²) reorthogonalization SpMVs (#510).
- Orchestration/authoring tooling (Loom, Anvil, Repo Skills) kept current
  across the cycle (#540, #597, #625, #662, #667, #670, #671).

### Fixed

- Transmon fixtures resolve mesh groups by name, with the real DeviceLayout
  fixture swapped in (#494).
- `geode-validation` rustdoc lints repaired and the `cargo doc -D warnings`
  gate widened to the whole workspace (#541).
- Dead macos-13 CI leg removed from cube-cavity-tolerance, ending 24 h hangs
  (#443).

### Removed

- Deleted the orphaned `examples/_support` (`geode-examples-support`) crate
  after its last consumer moved to `geode_util::viz` (Epic #414 Phase 3,
  #426, #427).
- Deleted the external-reference MoM baseline crates as part of the
  standalone move (#542).

## [0.2.0] - 2026-06-25

### Changed

#### geode-core public API reorganized into a hierarchical module tree (Epic #377 — BREAKING)

- The crate's public surface, previously a flat set of root re-exports
  (`geode_core::<item>`), is now organized into directory-backed module
  groups: `backend`, `traits`, `mesh`, `elements`, `derham`, `assembly`,
  `solver`, `eigen`, `driven`, `analytic`, `postproc`, `interop`, and
  `prelude`. Every public item now lives at its canonical path
  `geode_core::<module>::<item>` (children #378–#386).
- **All deprecated flat-root re-export shims have been removed.** Code that
  imported items via `geode_core::<item>` must migrate to the canonical
  module path or `use geode_core::prelude::*;`. The only re-exports that
  remain at the crate root are the core traits
  `geode_core::{Element, Mesh, Operator}` (also available via
  `geode_core::traits::*` and the prelude).
- `silvermuller_self_consistent` has moved to `eigen::self_consistent`
  (canonical path `geode_core::eigen::self_consistent::*`), with no compat
  shim — it is a quasimode-`k` eigenpencil finder and now lives alongside
  the other eigensolvers.
- `geode_core::prelude` is finalized as the recommended ergonomic surface:
  glob-import it (`use geode_core::prelude::*;`) to pull in the high-traffic
  entry points (mesh constructors/readers, assembly/eigen/driven/analytic
  types, core traits) from their canonical paths.

This is a breaking change for downstream callers; the workspace minor
version is bumped accordingly. See epic #377 and children #378–#387.

## [0.1.0] - 2026-06-15

### Summary

Initial public release. GEODE-FEM is a Burn-based Rust FEM/DG electromagnetic
solver. The 0.1 milestone closes four foundational epics (#88, #193, #226,
#234) and lands a Krylov + iterative-solver sweep on top, bringing the project
to the point where the driven solver hits Palace 3D parity on a spiral
inductor benchmark and the wave-port path validates against analytic
mode-matching cross-checks.

### Added

#### Solver core (Epic #88 — Burn bring-up)

- Workspace skeleton with three crates: `geode-core` (solver primitives),
  `geode-cli` (`geode` binary), and `geode-validation` (cross-backend
  comparison harness).
- Burn-tensor assembly layer with `wgpu` default backend and opt-in `ndarray`
  / `cuda` / `autodiff` backends; `unsafe_code = "deny"` at the crate
  boundary.
- Whitney / Nédélec / P1 element kernels including the shared
  `whitney_face` surface-mass module (#221).
- Sparse `[nnz]` pattern-slot Nédélec assembly for the driven path, lifting
  the 46k-edge dense-scatter cap (#220).
- De Rham `d⁰` rank classifier replacing the older spurious-mode heuristic
  (#124).
- ARPACK FFI eigensolver (vendored `dsaupd_c` / `dseupd_c` bindings, no
  bindgen required) behind the `arpack` Cargo feature.
- Pure-Rust sparse shift-invert Lanczos path (faer sparse LU) as the
  default eigensolver.
- Phase I/J cross-backend Mie reference suite: NumPy (#179), Julia (#181),
  JAX (#180), TF-Java (#183), and ONNX expressibility audits (#178, #182).

#### Driven solver (Epic #193)

- Deterministic driven solve `A(ω)x = b` with volumetric current source
  (#194).
- Conductivity term σ via ω-independent damping matrix C (#196).
- Matched (full Sacks) UPML lifted into the Burn assembly layer and into
  `driven_solve` (#205).
- Palace-style uniform lumped port for `driven_solve` with R termination
  and V/I bookkeeping (#206).
- Leontovich surface-impedance BC for thick conductors (#207).
- Driven Mie scattering benchmark Q_ext / Q_sca vs ka with matched UPML
  against the analytic series (#195).
- Z(ω) → L/R/Q/S₁₁ extraction and assembly-reusing frequency sweep over
  port-driven solves (#209).
- Layered-stack spiral inductor mesh generation (gmsh) with tag adapter to
  port / Leontovich / UPML inputs (#217).
- N-port S-matrix extraction over factor-once / multi-RHS port-driven
  solves (#219).
- Spiral inductor L/Q benchmark — FEM sweep vs Mohan analytic and MoM PEEC
  baselines (#211).
- SLCFET 3HP spiral capstone hitting the 5 % bar on quasi-static L₀
  comparison (#230).

#### Patch antenna (Epic #226)

- Probe-fed FR-4 patch-antenna gmsh fixture with box-UPML open-radiator
  adapter (#231).
- Patch-antenna S11 / resonance / bandwidth / efficiency benchmark vs the
  cavity-model oracle (#232).
- Love-equivalence near-to-far-field transform → patch radiation pattern,
  directivity, gain (#229).
- Impedance-matched patch feed delivering a real −10 dB return loss and
  bandwidth (#237).
- NTFF pattern artifact for the impedance-matched patch fixture
  (G = D·η_matched) (#252).

#### Wave-port BC (Epic #234)

- 2D transverse modal eigensolver for waveguide port cross-sections
  (#240).
- Wave-port boundary condition and wave-port S-parameters (#234 Phase 2)
  (#245).
- 2D waveguide modal pencil moved onto the sparse Lanczos path; drops the
  faer-QZ debug-overflow workaround (#253).
- True mesh height-step waveguide fixture and single-mode S-parameter
  validation (#248).
- Multi-mode waveguide modal eigensolve with outgoing-β branch and
  wrapper unification (#254).
- Rank-N SMW wave-port BC, multi-mode `waveguide_mode_reduce`, and block
  S-matrix (#255).
- Bi-modal straight-section wave-port validation (#256).
- Bi-modal height-step with analytic mode-matching cross-check (#257).
- Deterministic eigenvector sign pin in `solve_rect_waveguide_modes`
  (#262).
- General-cross-section 2D modal eigensolver (#265).

#### Iterative solvers and oracle parity (post-epic sweep)

- Krylov iterative solver path (COCG + Jacobi) for the driven
  complex-symmetric system (#243).
- Krylov iterative solver wired through sweep pipelines (#264).
- ILU(0) preconditioner for the COCG Krylov path (#267).
- Palace 3D oracle scaffolding: config generator, result ingester, and
  patch-benchmark wiring (#239).
- Palace 3D oracle parity for the spiral inductor benchmark (#266).
- Matched (full Sacks) UPML on the eigenmode path with quasi-mode Q vs
  `mie_open` complex roots (#223).
- Fine Mie sphere fixture — on-resonance driven Q_ext / Q_sca below 5 %
  (#224).

### Changed

- Build profile keeps dense linear algebra (faer) and tensor backends
  (Burn / wgpu) optimized in debug and test builds; project crates remain
  no-opt for fast iteration.
- README refreshed for the driven + multi-mode wave-port era (#274).

### Fixed

- ARPACK iterations are now deterministic (fixed-seed v₀ + rng) for all
  reference eigensolves (#191).
- `upload_mesh` honors `B::FloatElem` instead of forcing f32 (#99).
- Backend cfg robust to feature unification via precedence selection
  (#76).

### Removed

- A1's deprecated wave-port shims dropped following the multi-mode
  migration (#268).
