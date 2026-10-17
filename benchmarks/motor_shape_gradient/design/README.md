# ICEM design gradients dJ/dθ: FD and Taylor validation (M3c)

Motor paper M3, part C, issue #1177. This joins the fixed-topology node
motion `X(θ)` of #1173 (`mesh/icem_parametric.rs`) with the discrete shape
adjoint of #1174 (`assembly/magnetostatic_shape.rs`) into design gradients
`dJ/dθ_k` for the reference optimization's parameters, and validates them.
There is no optimization here, and no paper text.

**Verdict (measured; all statements are bounded by the runs below).**

- **The discrete design gradient is validated at both designs, both
  couplings and L0 to L2.** Under load (currents 0.5 and 1), for all 12
  parameters under the prescribed rule and the 10 that the Jinit rule
  changes:
  - the best central difference that re-moves the mesh agrees with the
    adjoint to between 2.7e-16 and 1.1e-5 relative over 800 objective and
    parameter cases. It is 2.8e-7 or better in 782 of them. The other 18,
    up to 1.1e-5, are all entries with |dJ/dθ| ≤ 0.054 (the Sr1, SW3, SD1,
    SW4 and MAG std or mean at a near-stationary point). There, the step at
    which truncation meets round-off is reached before the difference
    converges further (diagnosed below);
  - the Taylor remainder is O(h²). The window anchored at h_ref spans
    2.0 to 6.0 decades for the geometric parameters, the angle and the
    Jinit ScaleR, with one exception at two levels. SW1 std under Jinit,
    conforming, full current reads 0.0 at L0 and at L1, because the remainder changes
    sign next to h_ref; its widest window is 2.0 decades. SD1, Sr1 and SW4
    have the narrowest windows (2.0 to 3.5 decades). Their objectives vary
    least, and the remainder meets the f64 noise (about 1e-14 N m) near
    1e-6 m.
- **Zero current.** The cogging mean, about 1e-6 N m, is a 1e-4 residue of
  per-angle torques of ±1e-2 N m. It is the weak case, as in #1174:
  - its gradients are 1e-5 to 1e-3 N m/m, 1e5 to 1e7 times smaller than
    under load;
  - its best FD errors are 2.1e-8 to 7.0e-4;
  - its widest windows are 1.0 to 5.0 decades, limited by higher-order
    terms above about 1e-5 m and by round-off below about 1e-7 m.

  The zero-current std behaves like the loaded cases (best FD ≤ 8.8e-7).
- **Tolerance.** Under load, at rel_tol 1e-12 no gradient entry changes by
  more than 6.3e-8 relative from its 1e-14 value, except one Sr1 std entry
  (|dJ| = 8e-5, 4.1e-6). At 1e-8 the largest relative change is 3.4e-3, on
  the same kind of near-zero Sr1 std entry. Scaled by the box width and the
  objective, the change is at most 2.9e-6 at 1e-8 and 1.8e-10 at 1e-12.
- **Moved mesh against remeshing.** For MW, SW1 and SW2 (h45, θ0), the
  least-squares remeshed-minus-moved derivative gap falls with refinement.
  At full current it goes from L0 to L1 to L2 as follows:
  - MW: mean 5.4e-3, 1.4e-3, 2.6e-6; std 8.0e-2, 1.6e-2, 2.2e-4;
  - SW1: mean 4.4e-2, 1.2e-2, 3.7e-3; std 0.20, 0.11, 0.038;
  - SW2: mean 1.8e-2, 1.4e-2, 3.5e-3; std 9.7e-3, 7.7e-3, 2.1e-3.

  For h ≤ 1e-5 m a remeshed central difference is dominated by mesh jitter,
  1e-7 to 4e-5 N m in the objective, which is up to 10 times the derivative
  at h = 1e-6. Fresh meshes of θ ± h never had the same connectivity, even
  at h = 1e-6 m.
- **Mesh convergence of the gradient itself** (adjoint on L0 to L3, full
  current). In the reference model (h45), every mean-torque gradient
  except SW3 changes by at most 5.6e-3 relative from L2 to L3. The ratio of
  the L1-to-L2 change to the L2-to-L3 change is 1.8 to 420, and 0.29 for
  MAG at θ1, where both changes are below 1e-3. The std gradients converge more slowly,
  like the std itself, whose L1-to-L2 change is larger than its L0-to-L1
  change. SW3 converges roughly at O(h) and changes sign between L0 and L2.
  In the conforming band the SW2 mean gradient is non-monotone (−46.5,
  −41.3, −44.4, −47.2 N m/m from L0 to L3, against −47.1 for h45 at L3).
  SW2 is the one parameter that moves outer-band nodes, and the Arkkio band
  integral is the estimator most exposed to that motion (#1173). Details and
  likely causes are below.
- **Cost** (one process, sequential): one full gradient (12 parameters, both
  objectives) takes 1.13 to 1.20 forward evaluations at L0 to L2. The 2 × 12
  warm-started FD forwards take 7.2 to 9.5. The ratio gradient / FD is 0.12
  to 0.16. This is not a speedup claim; M5 owns that.


## What is differentiated

- **Objectives.** The mean torque and the population-std torque over the
  reference's optimization grid, 0, 2, ..., 18 mechanical degrees
  (`conventions.json` `torque.optimization_grid_mechanical_degrees`,
  `ripple_definition`), at the three current levels 0, 0.5 and 1 of
  `I_peak = 9.677657 A`. N m per modelled sector, `LENGTH` included.
- **Couplings.** The reference model, harmonic coupling n = 3:6:45 with the
  coupling torque `−L λᵀ ∂(Gᵀ A)/∂β` (`h45`), and the conforming band with
  the Arkkio torque over both gap bands (`conforming`).
- **Parameters.** All 12 of `reference_data.json`
  `optimization_parameter_bounds`:
  - the 9 geometric ones (MAG, MH, MW, SW1, SW2, SW3, SW4, SD1, Sr1):
    `dJ/dθ_k = ⟨dJ/dX, ∂X/∂θ_k⟩`, with `∂X/∂θ` from the dual numbers of
    #1173 at the current θ and `dJ/dX` from the adjoint of #1174;
  - `LENGTH`: `J` is linear in it, `dJ/dL = J/L`;
  - `OPERATING_ANGLE` (degrees): the adjoint of the current right-hand
    side. The same adjoint solve gives `dT/dI_g = −ψᵀ ∂F/∂I_g = −Q_g/A_g`
    for each winding group (new `TorqueShapeGradient::group_current_grad`),
    and `dI_g/dφ0 = I_peak cos(φ0 + 3β + offset_g) · 12 · sign_g` is
    analytic;
  - `ScaleR`: the reference evaluates the unscaled geometry and multiplies
    the torque by `ScaleR²` (`../../motor_reference/parametric/README.md`,
    with its scaled-mesh check to 5.8e-14). With the current prescribed in
    the unscaled geometry, `dJ/ds = 2J/s`.
- **Designs.** The initial design θ0, and a perturbed interior design θ1
  midway between the initial and the optimized parameters, all 12 at once
  (MAG 6.55, MH 4.87, MW 19.87, SW1 4.03, SW2 1.65, SW3 0.75, SW4 6.84,
  SD1 132.65, Sr1 1.50 mm; LENGTH 0.090 m, OPERATING_ANGLE −5.82°,
  ScaleR 1.024). Every parameter is inside its box. θ1 is valid for the
  fixed topology: the full optimized design moves the L0 to L3 meshes
  without an inverted triangle (#1173), and the moved L0 mesh at θ1 has a
  smallest angle of 13.18 degrees, none inverted. Both designs
  use the same θ0 mesh moved by `X(θ)`, as an optimizer would.
- **Current rules.**
  - *Prescribed* (the main tables): the peak current is fixed and
    `J_g = I_g/A_g(X)`, the #1174 rule. A larger slot carries the same
    current at a lower density.
  - *Jinit* (the reference optimizer's rule, `conventions.json`
    `excitation.setup_vs_optimization`): the current density of winding
    group 1 is fixed, so `I_peak ∝ A_1(X) · ScaleR`. The derivative adds
    `(dJ/dI_peak) · I_peak · (dA_1/dθ_k)/A_1`, with the new
    `SectorShapeProblem::group_area_gradients` for `dA_1/dX`. It changes the
    sign of the slot gradients. At θ0, full current, h45, `dJ_mean/dSW1` is
    +1.73 N m/m with the current prescribed and −221.7 N m/m under Jinit. A
    narrower slot then carries less current. So the two rules are different
    design problems, and M4/M5 must pick the rule the comparison needs.
    Both are validated here.

## Method

- `tests/common/icem_design.rs`: the design point, both current rules, the
  gradient assembly, and the step sweep. A geometric step re-moves the mesh:
  `SectorShapeProblem::with_node_positions(X(θ ± h e_k))` on the same
  topology, links and coupling grid.
- `examples/icem_design_gradient.rs`:
  - `study`: the gradient, a central-difference step sweep per parameter,
    the Taylor remainder, and the tolerance study;
  - `remesh`: the remeshed cross-check;
  - `cost`: the cost table;
  - `plan`: the remeshed designs.
- `build_design_mesh.py` builds fresh gmsh meshes of the construction's
  curves at arbitrary θ, for the remeshed cross-check. It uses the tags,
  coupling grid, antiperiodic pairing and size rules of
  `../../motor_reference/geometry/build_sector_mesh.py`, imported unchanged.
- **Steps.** `h = 1e-3, 3e-4, ..., 1e-9` m for the geometric parameters and
  `LENGTH`. The same list ×1e3 in degrees for `OPERATING_ANGLE`, and ×10
  for `ScaleR`. L2 uses decade steps only. A step is skipped when `X(θ ± h)`
  has no geometry or an inverted triangle. Only `h = 1e-3` m was ever
  skipped: for SW3 at both designs (`θ − 1e-3` m collapses the opening
  channel) and for Sr1 at θ0 (zero fillet radius).
- **Newton.** rel_tol 1e-14 (the #1174 setting) for the sweeps. The
  tolerance study re-runs the gradient at 1e-8, 1e-12 and 1e-14.
- **Taylor window.** This is the #1174 definition: the run of steps over
  which `remainder/h²` stays within a factor 2 of its value at the
  reference step (1e-5 m; 1e-2 deg; 1e-4 for ScaleR). `tables.md` also
  gives the widest such window over any anchor step. That one is not
  fooled by a sign change of the remainder at the reference step (one case
  below).

## Results

`analyze.py` generates every table into [`tables.md`](tables.md). That file
also has the per-run sweep tables for every level, coupling, design,
current and rule. The raw data are in `runs/`.

### Gradient tables (reference model h45, full current, prescribed current)

**θ0, mean torque** (h45, full current; adjoint dJ/dθ in N m/m unless noted; best FD relative error and widest O(h²) window in decades, per level):

| parameter | adjoint L0 | adjoint L1 | adjoint L2 | best FD L0 / L1 / L2 | window L0 / L1 / L2 |
|---|---|---|---|---|---|
| MAG | -6.82071e+01 | -6.94255e+01 | -6.92461e+01 | 5.1e-11 / 2.1e-09 / 1.9e-09 | 5.5 / 5.5 / 6.0 |
| MH | +2.14027e+01 | +2.10854e+01 | +2.10145e+01 | 5.9e-10 / 6.5e-10 / 3.6e-09 | 5.5 / 5.0 / 5.0 |
| MW | +9.44493e+01 | +9.47081e+01 | +9.49179e+01 | 5.4e-11 / 3.8e-10 / 8.7e-10 | 6.0 / 5.5 / 5.0 |
| SW1 | +1.73036e+00 | +1.68373e+00 | +1.64742e+00 | 1.2e-08 / 1.4e-08 / 5.1e-08 | 5.5 / 5.0 / 5.0 |
| SW2 | -4.97811e+01 | -4.80568e+01 | -4.72971e+01 | 4.4e-10 / 4.7e-12 / 4.1e-09 | 6.0 / 5.5 / 5.0 |
| SW3 | -7.83974e-01 | -9.88873e-02 | +4.06420e-01 | 4.8e-08 / 3.2e-08 / 4.4e-08 | 5.0 / 4.5 / 4.0 |
| SW4 | +1.06807e+00 | +1.07045e+00 | +1.06828e+00 | 7.1e-09 / 2.4e-08 / 6.8e-08 | 5.0 / 5.0 / 5.0 |
| SD1 | -1.75722e-01 | -1.74859e-01 | -1.74233e-01 | 2.2e-09 / 2.0e-09 / 4.1e-09 | 3.0 / 3.0 / 3.0 |
| Sr1 | +3.23975e-01 | +2.90607e-01 | +3.03172e-01 | 2.0e-09 / 1.5e-08 / 7.9e-08 | 3.5 / 3.5 / 3.0 |
| LENGTH | +1.44751e+01 | +1.44379e+01 | +1.44325e+01 | 1.2e-13 / 3.5e-13 / 5.1e-13 | linear |
| OPERATING_ANGLE (N m/deg) | +6.15954e-03 | +6.23771e-03 | +6.28306e-03 | 1.4e-10 / 1.0e-09 / 1.8e-09 | 5.0 / 5.0 / 5.0 |
| ScaleR (N m) | +2.89501e+00 | +2.88759e+00 | +2.88650e+00 | 3.0e-13 / 1.6e-13 / 7.8e-13 | exact s² |

**θ0, std torque** (h45, full current; adjoint dJ/dθ in N m/m unless noted; best FD relative error and widest O(h²) window in decades, per level):

| parameter | adjoint L0 | adjoint L1 | adjoint L2 | best FD L0 / L1 / L2 | window L0 / L1 / L2 |
|---|---|---|---|---|---|
| MAG | +2.09664e+00 | +1.76508e+00 | +1.28319e+00 | 1.2e-08 / 2.2e-08 / 2.4e-08 | 6.0 / 5.5 / 5.0 |
| MH | +1.35222e+00 | +1.33149e+00 | +1.30008e+00 | 4.8e-09 / 1.5e-08 / 7.2e-09 | 5.0 / 4.5 / 4.0 |
| MW | +5.80610e+00 | +5.49080e+00 | +5.29918e+00 | 1.4e-09 / 1.4e-09 / 4.4e-09 | 5.0 / 4.5 / 4.0 |
| SW1 | -2.54191e-01 | -2.26528e-01 | -2.05357e-01 | 9.7e-08 / 2.1e-07 / 2.8e-07 | 4.5 / 4.0 / 4.0 |
| SW2 | +6.41031e+01 | +6.39167e+01 | +6.34589e+01 | 3.3e-10 / 2.6e-10 / 2.7e-10 | 6.0 / 5.5 / 5.0 |
| SW3 | +6.79673e-01 | -2.60167e-01 | -8.91189e-01 | 2.6e-07 / 8.4e-08 / 9.1e-08 | 5.5 / 5.0 / 4.0 |
| SW4 | +7.54651e-02 | +7.83209e-02 | +7.85007e-02 | 1.3e-07 / 1.6e-08 / 2.2e-07 | 4.5 / 4.5 / 4.0 |
| SD1 | -1.08404e-02 | -1.08382e-02 | -1.08893e-02 | 6.6e-08 / 1.8e-08 / 2.2e-09 | 3.0 / 3.0 / 2.0 |
| Sr1 | +1.86810e-02 | +2.09622e-02 | +1.95812e-02 | 1.8e-08 / 1.3e-06 / 9.4e-08 | 3.5 / 3.0 / 2.0 |
| LENGTH | +8.26704e-01 | +8.20475e-01 | +8.08794e-01 | 5.2e-12 / 1.8e-11 / 3.2e-11 | linear |
| OPERATING_ANGLE (N m/deg) | +2.54409e-04 | +2.36878e-04 | +2.24361e-04 | 1.3e-09 / 3.8e-09 / 1.7e-08 | 4.5 / 4.0 / 4.0 |
| ScaleR (N m) | +1.65341e-01 | +1.64095e-01 | +1.61759e-01 | 8.8e-12 / 1.5e-12 / 3.0e-11 | exact s² |

**θ1, mean torque** (h45, full current; adjoint dJ/dθ in N m/m unless noted; best FD relative error and widest O(h²) window in decades, per level):

| parameter | adjoint L0 | adjoint L1 | adjoint L2 | best FD L0 / L1 / L2 | window L0 / L1 / L2 |
|---|---|---|---|---|---|
| MAG | -7.10330e+01 | -7.25087e+01 | -7.24909e+01 | 6.7e-10 / 8.9e-10 / 7.9e-10 | 5.5 / 6.0 / 6.0 |
| MH | +4.02049e+01 | +3.99353e+01 | +3.98688e+01 | 1.1e-10 / 1.1e-09 / 2.9e-09 | 6.0 / 5.5 / 6.0 |
| MW | +8.97258e+01 | +8.98422e+01 | +8.99886e+01 | 2.5e-11 / 1.1e-10 / 3.7e-10 | 5.5 / 5.0 / 5.0 |
| SW1 | +2.49035e+00 | +2.48988e+00 | +2.40445e+00 | 1.1e-08 / 4.2e-08 / 6.1e-08 | 5.5 / 5.0 / 5.0 |
| SW2 | -3.06672e+01 | -2.92816e+01 | -2.87506e+01 | 1.2e-09 / 5.4e-10 / 1.7e-09 | 5.5 / 5.5 / 6.0 |
| SW3 | -8.99407e-01 | -4.96077e-01 | -1.76896e-01 | 3.0e-08 / 9.6e-08 / 1.1e-07 | 5.0 / 4.5 / 4.0 |
| SW4 | +6.23936e+00 | +6.31368e+00 | +6.25039e+00 | 3.3e-09 / 3.1e-09 / 7.6e-09 | 6.0 / 6.0 / 5.0 |
| SD1 | -2.54698e-01 | -2.54142e-01 | -2.51522e-01 | 5.0e-10 / 3.2e-09 / 5.7e-09 | 3.0 / 3.0 / 3.0 |
| Sr1 | +1.18299e+00 | +1.05277e+00 | +1.07556e+00 | 4.7e-09 / 3.1e-09 / 6.1e-09 | 4.5 / 4.0 / 4.0 |
| LENGTH | +1.54731e+01 | +1.54321e+01 | +1.54214e+01 | 4.5e-13 / 9.7e-13 / 3.5e-14 | linear |
| OPERATING_ANGLE (N m/deg) | +9.21990e-03 | +9.26389e-03 | +9.28895e-03 | 1.8e-10 / 8.7e-10 / 4.6e-10 | 5.0 / 5.0 / 4.0 |
| ScaleR (N m) | +2.72027e+00 | +2.71306e+00 | +2.71117e+00 | 2.0e-13 / 2.7e-13 / 1.3e-13 | exact s² |

**θ1, std torque** (h45, full current; adjoint dJ/dθ in N m/m unless noted; best FD relative error and widest O(h²) window in decades, per level):

| parameter | adjoint L0 | adjoint L1 | adjoint L2 | best FD L0 / L1 / L2 | window L0 / L1 / L2 |
|---|---|---|---|---|---|
| MAG | +1.36846e+00 | +1.23377e+00 | +7.53446e-01 | 3.7e-08 / 8.7e-10 / 6.9e-08 | 6.0 / 5.5 / 6.0 |
| MH | +1.54213e+00 | +1.54399e+00 | +1.51464e+00 | 9.9e-09 / 7.9e-10 / 2.4e-08 | 5.5 / 5.5 / 5.0 |
| MW | +3.39342e+00 | +3.34169e+00 | +3.20735e+00 | 1.0e-09 / 4.1e-09 / 9.8e-09 | 5.0 / 4.5 / 4.0 |
| SW1 | -3.63077e-01 | -4.09834e-01 | -3.76196e-01 | 1.9e-07 / 4.2e-09 / 4.3e-08 | 5.5 / 5.5 / 4.0 |
| SW2 | +4.75752e+01 | +4.69704e+01 | +4.65404e+01 | 1.0e-09 / 1.0e-09 / 1.2e-09 | 6.0 / 5.5 / 6.0 |
| SW3 | +4.25305e-01 | -2.20619e-01 | -6.95528e-01 | 5.8e-08 / 4.5e-08 / 5.6e-08 | 5.0 / 4.5 / 4.0 |
| SW4 | +8.37642e-01 | +8.14462e-01 | +8.10701e-01 | 1.2e-09 / 1.5e-08 / 2.2e-07 | 5.5 / 5.5 / 5.0 |
| SD1 | -1.43392e-02 | -1.24170e-02 | -1.24678e-02 | 2.0e-08 / 5.7e-08 / 1.6e-07 | 3.0 / 3.0 / 3.0 |
| Sr1 | +1.18172e-01 | +1.17114e-01 | +1.07462e-01 | 7.0e-08 / 1.5e-07 / 7.7e-08 | 4.5 / 4.0 / 4.0 |
| LENGTH | +4.53243e-01 | +4.51786e-01 | +4.46677e-01 | 8.4e-13 / 1.1e-12 / 9.8e-11 | linear |
| OPERATING_ANGLE (N m/deg) | +1.76852e-04 | +1.95506e-04 | +1.87815e-04 | 1.2e-08 / 5.1e-09 / 1.5e-08 | 4.5 / 4.5 / 4.0 |
| ScaleR (N m) | +7.96829e-02 | +7.94267e-02 | +7.85285e-02 | 3.3e-12 / 8.7e-13 / 6.8e-11 | exact s² |

L2 uses decade steps, so its windows are quantized to whole decades. For
LENGTH the objective is linear and the remainder is round-off. For ScaleR
(prescribed rule) the objective is exactly s², so the remainder is the exact
second-order term. Neither is an O(h²) test.

`tables.md` ("Condensed: worst case per parameter") has the worst cases
over both couplings, both designs and all currents. The conforming/Arkkio
runs give the same picture as h45. At L1 their largest loaded best-FD
errors are 3.4e-6 (SW3 mean, |dJ| = 8.8e-3) and 1.3e-6 (Sr1 std).

### Current rules (θ0, h45, full current, L1)

`dJ/dI_peak` is +0.1744 N m/A (mean) and +0.00877 N m/A (std). The rotor
parameters do not change the slot area, so the two rules agree there.

| parameter | d mean, prescribed | d mean, Jinit | Jinit best FD (window) | d std, prescribed | d std, Jinit | Jinit best FD (window) |
|---|---|---|---|---|---|---|
| MAG | -6.9425e+01 | -6.9425e+01 | 2.1e-09 (5.5) | +1.7651e+00 | +1.7651e+00 | 2.2e-08 (5.5) |
| MH | +2.1085e+01 | +2.1085e+01 | 6.5e-10 (5.0) | +1.3315e+00 | +1.3315e+00 | 1.5e-08 (4.5) |
| MW | +9.4708e+01 | +9.4708e+01 | 3.8e-10 (5.5) | +5.4908e+00 | +5.4908e+00 | 1.4e-09 (4.5) |
| SW1 | +1.6837e+00 | -2.2085e+02 | 6.1e-11 (5.0) | -2.2653e-01 | -1.1421e+01 | 8.2e-11 (4.0) |
| SW2 | -4.8057e+01 | -4.1267e+00 | 1.2e-08 (5.0) | +6.3917e+01 | +6.6127e+01 | 8.6e-11 (6.0) |
| SW3 | -9.8887e-02 | -1.3501e+02 | 1.1e-09 (5.0) | -2.6017e-01 | -7.0467e+00 | 1.7e-08 (4.5) |
| SW4 | +1.0705e+00 | -1.7263e+02 | 9.1e-12 (5.5) | +7.8321e-02 | -8.6595e+00 | 3.0e-09 (5.0) |
| SD1 | -1.7486e-01 | +8.6674e+01 | 3.0e-10 (5.0) | -1.0838e-02 | +4.3581e+00 | 1.4e-09 (4.5) |
| Sr1 | +2.9061e-01 | +8.0866e+01 | 2.7e-11 (5.0) | +2.0962e-02 | +4.0743e+00 | 1.4e-09 (4.5) |
| ScaleR | +2.8876e+00 | +4.5753e+00 | 8.7e-11 (5.0) | +1.6410e-01 | +2.4899e-01 | 1.2e-09 (4.0) |

The FD errors and widest windows are under the Jinit rule. The current is
recomputed from the moved mesh's group-1 area at θ ± h.

### Diagnosis of the narrow cases

- **Small gradient against curvature.** The 18 loaded cases with a best FD
  error above 2.8e-7 all have |dJ/dθ| ≤ 0.054 N m/m.
  - Example: the std's MAG derivative at θ1, half current, L1. It is
    −0.0155 N m/m. `remainder/h²` is 1.05e3 N m/m², flat to 2% from 3e-5
    to 3e-8 m. The FD error falls as h², from 2.3e-2 at 3e-5 m to 8.7e-5
    at 1e-6 m. Below 3e-7 m it stalls at 8e-6 to 1e-5. Round-off,
    1e-14 N m / (h |dJ|), is 6.5e-6 at h = 1e-7.
  - The conforming mean's SW3 derivative at L1 behaves the same way
    (−8.8e-3, `remainder/h²` = 1.9e3).

  In these cases the truncation and round-off limits meet before the FD
  error gets small. The Taylor remainder is O(h²) over 3 to 4 decades, so
  the evidence points to the step range, not to the gradient.
- **Anchored window 0.0** (SW1 std, Jinit, conforming, full current, L0
  and L1). From h = 1e-3 m down to 1e-7 m, `remainder/h²` runs 280, 168,
  69, 16, **0.38**, 4.5, 5.9, 6.4, 7.1 (L0). The remainder crosses zero
  between 3e-5 and 1e-5 m, next to the reference step, where the O(h²)
  term (about 6) and the higher-order terms cancel. The widest window,
  3e-6 to 3e-8 m, spans 2.0 decades. The FD error falls by about 100 per
  decade, from 6.8e-6 at 1e-5 m to 4.2e-11 at 1e-7 m.
- **SD1, Sr1, SW4: windows of 2.0 to 3.5 decades.** These parameters move
  the outer boundary and the slot bottom, far from the gap.
  `remainder/h²` is 0.04 to 21 N m/m²: SD1 std 0.04, SD1 mean 0.4, Sr1 std
  2.6, Sr1 mean 21, SW4 std 20. The magnet and opening parameters have
  1e2 to 1e4. The remainder therefore meets the objective noise, about
  1e-14 N m, near h = 1e-6 to 3e-7 m, and the window ends there.
- **Zero current.** The per-angle cogging torques are ±1.1e-2 N m, and
  their mean is −1.4e-6 N m (h45, θ0, L0).
  - The mean's gradient, 1e-5 to 1e-3 N m/m, is a small residue of the
    per-angle gradients.
  - Above about 1e-5 m, `remainder/h²` is not flat. The higher-order terms
    act on a length scale of order 0.1 mm, estimated from the MAG
    remainder.
  - The remainder reaches its f64 floor, about 1e-15 N m, near 1e-7 m. The
    best FD error, about 1e-15/(h |dJ|), matches that floor.

  As in #1174, this does not prove the gradient free of error there. A
  curvature this small cannot be resolved more cleanly in f64.

### Tolerance stability

`tables.md` ("Gradient vs Newton tolerance") has every run. Changes are
relative to the rel_tol = 1e-14 gradient.

| level | worst relative change, loaded, 1e-12 | worst relative change, loaded, 1e-8 | worst scaled change, loaded, 1e-12 / 1e-8 | zero current, 1e-8: relative / scaled |
|---|---|---|---|---|
| L0 | 4.1e-6 (Sr1 std, abs(dJ) = 8e-5); otherwise ≤ 6.3e-8 | 3.2e-3 (Sr1 std) | 1.8e-10 / 2.6e-6 | 1.1e-2 / 1.1e-2 |
| L1 | 3.0e-8 | 3.4e-3 (Sr1 std) | 7.8e-11 / 2.9e-6 | 2.9e-2 / 6.6e-3 |
| L2 | 5.4e-9 | 5.2e-5 | 3.4e-11 / 6.1e-7 | not run |

"Scaled" is `|Δ dJ/dθ_k| · w_k / |J|`: the change of the first-order
objective change across parameter k's box width, relative to the
objective. Under load, then, the default rel_tol 1e-8 moves no predicted
box-wide objective change by more than 3e-6 of the objective. The
zero-current scaled values are inflated by the 1e-6 N m objective.

### Moved mesh against remeshed central differences

- **Meshes.** The adjoint is on a fresh mesh of θ0, built by
  `build_design_mesh.py` from the construction's curves: 2867, 9479 and
  35564 nodes at L0 to L2. At L0 its θ0 torque differs from the NURBS
  mesh's by 2.7e-4 relative. Its adjoint differs by 4e-5 to 2e-2 relative
  (`tables.md`). Each θ0 ± h e_k is meshed afresh, 126 meshes in all
  (2 × 7 steps × 3 parameters × 3 levels).
- **Columns.** The table is at full current, h45. rm-mv is
  (remeshed FD − moved FD) / |dJ|. Both differences carry the same
  truncation, so rm-mv isolates the remeshing. Jitter is the median of
  `|J+ − J− − 2h dJ|/2` over h ≤ 1e-5 m. The slope is the least-squares
  slope of the per-step difference over all 7 steps.

| parameter | objective | level | jitter (N m) | rm-mv@1e-3 | rm-mv@3e-4 | rm-mv@1e-4 | rm-mv slope (all steps) |
|---|---|---|---|---|---|---|---|
| MW | mean | L0 | 3.0e-05 | +5.4e-03 | +6.4e-03 | -6.7e-03 | +5.4e-03 |
| MW | mean | L1 | 1.6e-05 | +1.4e-03 | +1.1e-03 | +1.1e-03 | +1.4e-03 |
| MW | mean | L2 | 1.0e-06 | -3.6e-05 | +3.6e-04 | +6.3e-04 | +2.6e-06 |
| MW | std | L0 | 3.5e-05 | -8.5e-02 | -5.5e-02 | +1.6e-01 | -8.0e-02 |
| MW | std | L1 | 3.6e-05 | -1.5e-02 | -3.0e-02 | -2.4e-03 | -1.6e-02 |
| MW | std | L2 | 4.0e-07 | +1.2e-04 | +2.4e-03 | -1.0e-02 | +2.2e-04 |
| SW1 | mean | L0 | 2.0e-06 | -4.7e-02 | -1.6e-02 | -7.3e-02 | -4.4e-02 |
| SW1 | mean | L1 | 1.7e-06 | -1.3e-02 | -2.2e-03 | -8.7e-03 | -1.2e-02 |
| SW1 | mean | L2 | 1.0e-06 | -3.7e-03 | -3.6e-03 | -6.7e-03 | -3.7e-03 |
| SW1 | std | L0 | 1.9e-06 | +2.0e-01 | +2.0e-01 | +4.2e-01 | +2.0e-01 |
| SW1 | std | L1 | 6.9e-07 | +1.1e-01 | +9.7e-02 | +1.7e-01 | +1.1e-01 |
| SW1 | std | L2 | 1.5e-07 | +3.8e-02 | +3.4e-02 | +3.3e-02 | +3.8e-02 |
| SW2 | mean | L0 | 2.6e-06 | +1.9e-02 | +5.0e-03 | +2.5e-02 | +1.8e-02 |
| SW2 | mean | L1 | 1.3e-06 | +1.4e-02 | +3.7e-03 | +2.8e-02 | +1.4e-02 |
| SW2 | mean | L2 | 4.7e-06 | +3.7e-03 | +1.6e-03 | +5.4e-03 | +3.5e-03 |
| SW2 | std | L0 | 6.9e-07 | -1.0e-02 | +4.4e-05 | -1.6e-02 | -9.7e-03 |
| SW2 | std | L1 | 6.4e-07 | -7.9e-03 | -2.8e-03 | -3.4e-02 | -7.7e-03 |
| SW2 | std | L2 | 4.2e-06 | -2.2e-03 | -1.4e-03 | -3.1e-03 | -2.1e-03 |

- **Systematic gap.** The rm-mv slope falls with refinement for all three
  parameters and both objectives:
  - MW: mean 5.4e-3 to 2.6e-6, std 8.0e-2 to 2.2e-4;
  - SW1: mean 4.4e-2 to 3.7e-3, std 0.20 to 0.038 (the slowest);
  - SW2: mean 1.8e-2 to 3.5e-3, std 9.7e-3 to 2.1e-3.

  Within the moved family, the SW1 FD at 1e-3 m is three times the
  adjoint: SW1 is strongly nonlinear at that scale.
- **Jitter.** At small steps the remeshed difference measures mesh jitter,
  not the derivative.
  - The jitter is 1.5e-7 to 3.6e-5 N m, and it is not monotone in level
    (SW2: 2.6e-6, 1.3e-6, 4.7e-6 N m).
  - Fresh meshes of θ ± h never had the same connectivity, even at
    h = 1e-6 m.
  - So a remeshed central difference needs h of order 1e-4 m or more here.
    At that step the truncation for SW1 is already 1e-2 (and 0.1 at
    3e-4 m).

  For M4/M5, then, at fixed topology the usable gradient is that of the
  moved-mesh discrete objective. By the trend above, it approaches the
  remeshed derivative as the mesh is refined.

### Mesh convergence of the gradient

`tables.md` ("Adjoint gradient against mesh level") has every entry at L0
to L3. L3 comes from gradient-only runs, 139k nodes. Selected rows, full
current, θ0:

| mode | parameter | objective | L0 | L1 | L2 | L3 | (L3 - L2) / abs(L3) |
|---|---|---|---|---|---|---|---|
| h45 | MAG | mean | -6.8207e+01 | -6.9425e+01 | -6.9246e+01 | -6.9246e+01 | +6.2e-06 |
| h45 | MAG | std | +2.0966e+00 | +1.7651e+00 | +1.2832e+00 | +1.2173e+00 | -5.4e-02 |
| h45 | MW | mean | +9.4449e+01 | +9.4708e+01 | +9.4918e+01 | +9.4946e+01 | +3.0e-04 |
| h45 | MW | std | +5.8061e+00 | +5.4908e+00 | +5.2992e+00 | +5.2554e+00 | -8.3e-03 |
| h45 | SW1 | mean | +1.7304e+00 | +1.6837e+00 | +1.6474e+00 | +1.6429e+00 | -2.7e-03 |
| h45 | SW1 | std | -2.5419e-01 | -2.2653e-01 | -2.0536e-01 | -2.0099e-01 | +2.2e-02 |
| h45 | SW2 | mean | -4.9781e+01 | -4.8057e+01 | -4.7297e+01 | -4.7110e+01 | +4.0e-03 |
| h45 | SW2 | std | +6.4103e+01 | +6.3917e+01 | +6.3459e+01 | +6.3394e+01 | -1.0e-03 |
| h45 | SW3 | mean | -7.8397e-01 | -9.8887e-02 | +4.0642e-01 | +6.4175e-01 | +3.7e-01 |
| h45 | SW3 | std | +6.7967e-01 | -2.6017e-01 | -8.9119e-01 | -1.1948e+00 | -2.5e-01 |
| h45 | (value) | mean | 1.447506 | 1.443793 | 1.443251 | 1.443264 | +9.1e-06 |
| h45 | (value) | std | 0.082670 | 0.082048 | 0.080879 | 0.080337 | -6.8e-03 |
| conforming | SW2 | mean | -4.6499e+01 | -4.1279e+01 | -4.4426e+01 | -4.7246e+01 | -6.0e-02 |
| conforming | SW2 | std | +6.6440e+01 | +6.7944e+01 | +6.7557e+01 | +6.7404e+01 | -2.3e-03 |
| conforming | (value) | mean | 1.443882 | 1.441196 | 1.439975 | 1.440314 | +2.4e-04 |
| conforming | (value) | std | 0.091085 | 0.089688 | 0.088400 | 0.087828 | -6.5e-03 |

- **Mean-torque gradients (h45).** At both designs, every parameter except
  SW3 changes by at most 5.6e-3 relative from L2 to L3. The mean torque
  itself changes by 9e-6 (θ0) and 6e-5 (θ1).
- **Std gradients converge like the std.**
  - The std itself changes more from L1 to L2 than from L0 to L1 (ratio
    0.53 at θ0, 0.29 at θ1). The 20-angle ripple behaved the same way in
    `../../motor_reference/forward_harmonic/`.
  - From L2 to L3, the std gradients change by 0.02% to 5.4%.
  - Two exceptions: SW3, and MAG at θ1 (−32%). There the gradient is
    0.57 N m/m, against 1.37 at L0, and near-stationary.
- **SW3.** The gradient is small. It converges at about O(h) (ratios 1.1 to
  2.5) and passes through a sign change.
  - SW3 moves the inner end of the slot-opening channel, two re-entrant
    iron corners per slot. A reduced rate there is consistent with the
    corner field singularity. That is a likely cause, not a proven one.
  - Its box-scaled effect is small: at most 1.6e-4 of the mean and 6.5e-3
    of the std, from L2 to L3.
- **Conforming/Arkkio SW2 mean.** It is non-monotone at θ0: −46.5, −41.3,
  −44.4 and −47.2 N m/m from L0 to L3. The h45 coupling-torque value
  converges smoothly (−49.8, −48.1, −47.3, −47.1).
  - SW2 is the only parameter whose velocity moves outer-band (tag 5)
    nodes, through the explicit Arkkio terms. #1173 found the Arkkio band
    integral the estimator most exposed to mesh motion. The coupling
    torque has no such term.
  - The conforming mean torque itself is non-monotone at θ1 (1.3865,
    1.3833, 1.3834, 1.3856).

### Cost

One process, sequential, on the same host (Apple M3 Ultra, 28 cores; the
solvers run with their default threading). Other users' work kept the
load average at 14 to 24 during the cost stage (`logs/cost.log`). The table
gives the median of 3 repeats at L0 and L1, and 1 run at L2. Full current,
θ0.

- **forward**: one evaluation of the 10-angle sweep. It starts from zero,
  and each angle is warm-started from the previous one.
- **gradient**: the forward, plus 10 adjoints, ∂X/∂θ and the contractions,
  for all 12 parameters and both objectives.
- **2×12 FD**: 24 forwards at h = 1e-6, each warm-started from the base
  states.

| level | nodes | mode | rel_tol | forward (s) | gradient (s) | gradient / forward | 2×12 FD (s) | FD / forward | gradient / FD |
|---|---|---|---|---|---|---|---|---|---|
| L0 | 3153 | conforming | 1e-08 | 0.531 | 0.614 | 1.15 | 4.280 | 8.06 | 0.143 |
| L0 | 3153 | conforming | 1e-14 | 0.648 | 0.731 | 1.13 | 6.123 | 9.44 | 0.119 |
| L0 | 3153 | h45 | 1e-08 | 0.487 | 0.577 | 1.19 | 4.304 | 8.84 | 0.134 |
| L0 | 3153 | h45 | 1e-14 | 0.664 | 0.748 | 1.13 | 6.333 | 9.53 | 0.118 |
| L1 | 9937 | conforming | 1e-08 | 2.588 | 2.981 | 1.15 | 21.104 | 8.15 | 0.141 |
| L1 | 9937 | conforming | 1e-14 | 3.450 | 3.915 | 1.13 | 30.366 | 8.80 | 0.129 |
| L1 | 9937 | h45 | 1e-08 | 2.402 | 2.866 | 1.19 | 20.591 | 8.57 | 0.139 |
| L1 | 9937 | h45 | 1e-14 | 3.174 | 3.658 | 1.15 | 29.637 | 9.34 | 0.123 |
| L2 | 36083 | conforming | 1e-08 | 13.787 | 16.204 | 1.18 | 99.854 | 7.24 | 0.162 |
| L2 | 36083 | conforming | 1e-14 | 16.717 | 18.874 | 1.13 | 145.051 | 8.68 | 0.130 |
| L2 | 36083 | h45 | 1e-08 | 9.648 | 11.568 | 1.20 | 76.138 | 7.89 | 0.152 |
| L2 | 36083 | h45 | 1e-14 | 12.702 | 14.644 | 1.15 | 116.687 | 9.19 | 0.125 |

**Split of the gradient.** The 10 adjoints take 0.13 to 0.17 of a forward.
∂X/∂θ for the 9 geometric parameters is one multi-column solve with the
pre-factored extension operator: 0.5 ms (L0) to 9 ms (L2). The 24
contractions take 0.1 to 1.3 ms, and the shape map's one-off set-up
0.01 to 0.23 s. The 3 non-geometric parameters cost no solve, and the Jinit
correction costs one contraction with `dA_1/dX`. So beyond the forward, the
gradient is set by the number of operating points (10 adjoints), not by the
number of parameters.

The 24 FD forwards are warm-started, so each costs 0.30 to 0.40 of the
cold-started forward. These are wall-time ratios on one host, not a speedup
claim.


## Reproduce

```text
benchmarks/motor_shape_gradient/design/run_all.sh            # all stages
benchmarks/motor_shape_gradient/design/run_all.sh meshes     # L1/L2 initial meshes, remesh plan + meshes
benchmarks/motor_shape_gradient/design/run_all.sh study      # step sweeps, Taylor, tolerance (L0-L2); L3 gradients
benchmarks/motor_shape_gradient/design/run_all.sh remesh     # remeshed cross-check
benchmarks/motor_shape_gradient/design/run_all.sh cost       # sequential cost runs
benchmarks/motor_shape_gradient/design/run_all.sh test       # CI test with --nocapture
benchmarks/motor_shape_gradient/design/run_all.sh analyze    # tables.md, summary.json
```

Needs the upstream cache (`../../motor_reference/extract_reference.py
--fetch`) and gmsh 4.15.2 through uv. The `.msh` files and the remesh plan
in `meshes/` are gitignored. The `meshes` stage regenerates them
deterministically, and `runs/remesh_meshes_L*.json` lists their node
counts.

| file | content |
|---|---|
| `runs/study_L{0,1,2}_{h45,conforming}_{initial,perturbed}.json` | gradients, step sweeps, Taylor remainders, tolerance study |
| `runs/gradient_L3_{h45,conforming}_{initial,perturbed}.json` | L3 gradients (full current), for the mesh-convergence table |
| `runs/remesh_L{0,1,2}_h45.json`, `runs/remesh_meshes_L*.json` | remeshed cross-check and its meshes |
| `runs/cost_L*_*.json` | cost runs |
| `tables.md`, `summary.json` | every table (`analyze.py`) |
| `logs/*.log` | console output of every run; `test_motor_design_gradient.log` (CI test, `--nocapture`), `test_existing_motor.log` (the motor forward/shape tests, unchanged and passing), `test_magnetostatic_shape_unit.log`, `mutation_checks.log` |

## Tests in CI

`crates/geode-core/tests/motor_design_gradient.rs` is in both m-z lists of
`integration-tests.yml`. At L0 and rel_tol 1e-14 it checks the following:

- **h45, full current, θ0.** All 12 parameters. A central difference
  (re-moved mesh) must agree with the adjoint to 1e-6 relative, for both
  objectives. For each geometric parameter, `remainder/h²` at 1e-6 m must
  be within 25% of its value at 1e-5 m (1e-5 and 1e-4 for SD1).
- **h45, full current, θ0, Jinit rule.** SW1, SD1 and ScaleR.
- **Conforming, Arkkio, full current, θ0.** SW2, MAG and LENGTH. SW2 moves
  outer-band nodes, so this exercises the explicit Arkkio terms (asserted).
- **h45, half current, θ1.** MW, SD1 and OPERATING_ANGLE.

The steps (3e-7 m, SD1 3e-6 m, Sr1 1e-6 m, 1e-3 deg) sit in the region of
the L0 sweeps where neither truncation nor round-off dominates. The largest
measured error at those steps is 4.4e-7 (SW3, std; truncation).

**Mutation checks** (`logs/mutation_checks.log`). Each mutation fails the
test that should catch it:

- Flipping the sign of `dT/dI_g` fails the three tests that use it: the
  OPERATING_ANGLE checks at θ0 and θ1, and the Jinit test (whose slot and
  ScaleR terms carry `dJ/dI_peak`). The conforming test does not use it
  and passes.
- Dropping the Jinit area term fails the Jinit test.
- Dropping the explicit Arkkio term (as in #1174) fails the conforming
  test, through SW2.
