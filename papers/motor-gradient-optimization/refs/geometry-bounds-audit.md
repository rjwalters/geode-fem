# What the frozen geometry bounds already guarantee

Independent scope audit, 2026-10-18. These are analytical bounds on the generated continuous geometry, not new FEM results, manufacturing qualifications or a certification of every discretized mesh. Source: commit `43db1c626bdc2367f896905d7807979789ee22b3`, specifically `crates/geode-core/src/mesh/icem_parametric.rs`, `icem_controlpoints.rs`, `tests/common/icem_experiment2.rs` and `benchmarks/motor_reference/reference_data.json`.

The main study fixes `ScaleR=1`, axial length 100 mm, outer stator diameter and the electrical control policy. The published scalar bounds and one-sided control-point bounds have useful consequences. They should be checked before describing all physical-feature requirements as missing.

## Magnet area is bounded, but is not held fixed

The rectangular magnet has height MH in [2,12] mm and width MW in [10,25] mm. Hence its area is in **[20,300] mm²**, compared with **133 mm²** at theta0. At fixed 100 mm length its volume is in [2,30] cm³ per magnet, compared with 13.3 cm³ initially. This supplies an explicit area bound implied by the box. It does not impose a fixed magnet-resource budget or prevent a design from using more magnetic material than theta0.

## Positive air-gap separation follows from the construction

All rotor surface pieces are rational splines with positive weights and nonnegative basis functions. Their points are convex combinations of their control points. The existing control-point construction guard requires every rotor control point to have radius strictly below the coupling radius **44.5 mm**; consequently the entire generated rotor surface lies inside that circle. Space A uses the 44 mm rotor circle directly.

For the stator, consider one slot cell in its local frame. Its half-pitch is alpha = pi/36, or 5 degrees. Every baseline bore control point has a projection onto the cell axis of at least `45 cos(alpha)` mm:

- the tooth endpoints attain that projection;
- the opening corners lie on the 45 mm circle with angles between −alpha and alpha;
- the arc middle controls have radius at least 45 mm and angles inside the same interval.

Each stator offset is nonnegative, [0,0.5] mm, in a fixed outward direction whose angle lies within that cell. It therefore cannot reduce that projection. Positive rational weights preserve the projection bound along each bore piece. Every generated bore point thus has radius at least `45 cos(pi/36) = 44.8287614141` mm.

The reverse triangle inequality, together with the rotor's 44.5 mm containment, gives a conservative separation between the rotor surface and stator bore:

`gap > 45 cos(pi/36) − 44.5 = 0.3287614141 mm`.

This bound is rotation-invariant and needs no newly chosen manufacturing margin. It is conservative, does not claim to be the achieved minimum gap, and depends on the existing construction guard and offset signs. The prospective normalized-margin constraint may strengthen it. A control-point radial lower bound alone would not suffice for the stator; the cell-projection argument is essential.

## A bound on the outer rotor web

The unperturbed rational rotor surface is the 44 mm circle, including its knot-refined pole arc. Every rotor control point moves by at most 1.5 mm in norm, with its weights and knots unchanged. The displacement of a curve point is a convex combination of those control-point displacements, hence has norm at most 1.5 mm. Every perturbed outer rotor point therefore has radius at least **42.5 mm**.

The pocket polygon's fixed corners have radii 41 and 42 mm. The furthest possible magnet corner under the scalar box has radius at most

`sqrt((44 − MAG_min)^2 + (MW_max/2)^2) = sqrt(38^2 + 12.5^2) = 40.0031248780 mm`.

The magnet/pocket boundary consists of straight segments joining those vertices, so it lies inside the 42 mm circle. Its distance from the generated outer rotor surface is therefore at least **0.5 mm**, again by the reverse triangle inequality.

This is specifically an **outer surface-to-pocket/magnet web bound**. It is not a lower bound on every iron feature, a stress or demagnetization check, or a claim of manufacturability. Other slot and fillet features still depend on the scalar construction guards. A straight-sided mesh can introduce geometric approximation error, so these continuous-geometry arguments do not replace candidate mesh/refinement checks.

## Consequences for the paper

The reference box already supplies bounded magnet area, positive air-gap separation and a positive outer rotor web under the stated construction assumptions. The paper can report these scoped bounds after independent verification. It must not substitute “fixed magnet volume” or “manufacturable motor” for them.

Mean-torque floors at other loaded currents, a population-method comparator, measured CPU time/memory, and the effective-dimension diagnostic remain separate scope questions. None follows from these geometric bounds.
