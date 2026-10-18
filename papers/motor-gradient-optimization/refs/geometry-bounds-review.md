# Independent check: geometry-bounds-audit.md premises

Source: commit `43db1c626bdc2367f896905d7807979789ee22b3`, read with `git show 43db1c62:<path>`; no checkout was modified.
Abbreviations: **P** = `crates/geode-core/src/mesh/icem_parametric.rs`, **CP** = `crates/geode-core/src/mesh/icem_controlpoints.rs`, **E2** = `crates/geode-core/tests/common/icem_experiment2.rs`, **RD** = `benchmarks/motor_reference/reference_data.json`.
A port of the construction to Python (rotor knot insertion, bore control points, slot-top arc) was used as a numerical cross-check: `scratchpad/bvs/chk.py`.

## Premises

### 1. Scalar ranges: VERIFIED
- Spaces A and B free `MAG MH MW SW1 SW2 SW3 SW4 Sr1` with bounds taken from RD `optimization_parameter_bounds` (E2:96, E2:176-186, E2:351-370). The offsets take their bounds from `OFFSET_BOUNDS`.
- RD:157-174 give MAG ∈ [6, 15] mm, MH ∈ [2, 12] mm and MW ∈ [10, 25] mm.
- SD1 = 135 mm, ScaleR = 1, LENGTH = 0.1 m and OPERATING_ANGLE = 0 are frozen (E2:91-93). ScaleR only rescales torque; it never changes the geometry.
- At θ0, MH = 7 mm and MW = 19 mm (P:147, RD:113-114), so the magnet area is 7 × 19 = 133 mm². Verified.

### 2. Magnet shape and position: VERIFIED
- In the rotor frame, the magnet is the axis-aligned rectangle x ∈ [44 − MAG − MH, 44 − MAG], |y| ≤ MW/2 (P:22-25, P:800-806, P:820-823).
- The whole rotor is then turned rigidly by 90° (P:857-861). The magnet is not tilted and not V-shaped.
- **What MAG means:** MAG is the depth of iron *along the pole axis* between the magnet's outer face and the rotor surface (P:24-25). The outer face is a chord at x = 44 − MAG. It is not an arc at radius 44 − MAG. The farthest corner is therefore at sqrt((44 − MAG)² + (MW/2)²) exactly. With MAG_min = 6 and MW_max = 25 this is sqrt(38² + 12.5²) = **40.00312 mm**.
- The magnet has positive area everywhere in the box. The shaft guard x_in > 16 mm (P:807) holds throughout, because 44 − 15 − 12 = 17 > 16.

### 3. Pocket polygon: VERIFIED
- The fixed pocket corners sit at r = 44 − 3 = 41 mm at ±0.37558 rad and r = 44 − 2 = 42 mm at ±0.37 rad (P:112-119, P:815-818). These constants depend on no design variable, and space B does not replace them.
- Every pocket edge is a `line` (P:824-833). Each pocket is the quadrilateral formed by two magnet corners and these two fixed points.

### 4. Rotor: VERIFIED
- **Unperturbed surface is the 44 mm circle.** In space B the surface is five rational quadratic pieces: edge, bridge, pole (refined by inserting 7 knots), bridge, edge. They are built by `arc3` at R_ROTOR (CP:321-369) and do not depend on the scalars.
  - The unit test `rotor_surface_is_the_circle_at_zero_offset` asserts a radial deviation below 1e-16 (CP:852-881).
  - My port gives a minimum weight of 0.966, so all weights are positive. Boehm insertion keeps homogeneous weights as convex combinations of positive weights (CP:269-294).
- **Offsets.** CPR1, CPR2 and CPR4..CPR7 are in [−1.5, 0] mm; CPR3 is in [−1.0, 0] mm (CP:140-151). So [−1.5, 0] is a valid enclosing interval.
  - Each point moves by δ_g·u_i, where u_i is the unit radial direction of the baseline point. That direction is frozen and independent of the scalars (CP:458-463, CP:485-500).
  - So each point moves by at most 1.5 mm.
- **Weights and knots are unchanged.** `pc.knots.clone()` and `T::cst(w)` use the fixed values from `rotor_surface()` (CP:552-559).
- **Guard.** `sector_curves_cp` rejects any rotor control point with r ≥ R_COUPLING = 44.5 mm (CP:530-541, P:109). Every space-B curve evaluation goes through it (CP:802, CP:812; `icem_remesh.rs:340`), so the guard is enforced.
  - The guard never binds in the box. Each offset is ≤ 0 along the point's own radial direction, so |P + δu| = |P| + δ ≤ |P|.
  - The largest baseline rotor control-point radius is 44.1208 mm (the middle control of the edge arc, half-angle 0.0740 rad).

### 5. Stator bore: VERIFIED
- **Half-pitch.** `HALF_SLOT = PI / 36` (P:123), and α = HALF_SLOT in both constructions (P:864, CP:391). Slot k is placed at 65° + 10°k (P:128-129, P:970).
- **Bore control points in a cell** (CP:388-408, slot frame):
  - tooth points: polar(45, ±α);
  - arc middles: polar(45/cos h, ±(α + φo)/2), with h = (α − φo)/2;
  - corners: (45 cos φo, ±s), which lie on the 45 mm circle at angle ±φo;
  - opening middle: (45/cos φo, 0).
  - The guard s < 45 sin α (P:868) makes φo < α. So every angle is in [−α, α] and every radius is ≥ 45 mm, which gives a projection ≥ 45 cos α. Spot-checked over the whole SW2 range [1, 4] mm.
- **Offsets.** CPS1..CPS3 are in [0, 0.5] mm (CP:148-150).
  - The directions are the unit radial directions of the points at the generator default SW2 = 2.3 mm (CP:465, CP:477). In the cell frame their angles are {−α, −0.0564, −0.0256, 0, 0.0256, 0.0564, α}, all within [−α, α].
  - The directions are frozen and do not follow the current SW2 (CP:16-26). Each direction has an axial component ≥ cos α > 0, so a nonnegative offset cannot reduce the projection.
- **Weights.** The weights are 1, w_mid = cos h and w_open = cos φo (CP:407, CP:584-596). Both are positive because |h| < α/2 and |φo| < α.
- **Missing pieces: none.**
  - The gap-facing stator boundary in space B is exactly the three splines bore_lo, bore_open and bore_hi per slot (CP:580-597). The gap band runs from r_c to the bore (CP:103-108).
  - The slot-opening walls (`channel_lo` and `channel_hi`, which in B run from the moved corner to C±), `opening_end` and the tip arcs lie beyond the opening:
    - both channel endpoints have projection ≥ 45 cos α (C± has projection 45 cos φo + SW3);
    - the tip-arc minimum is at least 45 cos α + 0.63 mm over a grid on the SW1 × SW2 × SW3 box.
  - The fillets (Sr1) are at the slot bottom (P:902-906), near SD1/2 − SW4, far from the gap.

### 6. Rotor surface in space A: VERIFIED
- Space A uses `ScalarSpace` → `sector_curves_f64` (`icem_remesh.rs:214`, `icem_remesh.rs:251`).
- There, `rotor.surface` is `circle_arc(R_ROTOR, ±π/6)` (P:840-845), with no dependence on the scalars (P:28-29).
- The space-A bore is three arcs exactly on the 45 mm circle (P:921-923). So the gap in space A is exactly **1 mm** for every design, which is stronger than the stated bound.

## Conclusions
- **Magnet area in [20, 300] mm²: HOLDS.** The area is MH·MW with MH ∈ [2, 12] and MW ∈ [10, 25]. It is 133 mm² at θ0.
- **Gap > 0.32876 mm: HOLDS.** The bore radius is at least 45 cos(π/36) = 44.82876 mm, and the guard keeps the rotor strictly inside 44.5 mm.
  - It is conservative. Without relying on the guard, the rotor control points are ≤ 44.12078 mm, so **gap > 0.70798 mm** in space B.
  - In space A the gap is exactly 1 mm.
- **Outer rotor web ≥ 0.5 mm: HOLDS.**
  - Each surface displacement is a convex combination of control displacements of norm ≤ 1.5 mm, so the surface stays at r ≥ 42.5 mm.
  - The pocket and magnet vertices are at r ≤ 42 mm, the edges are straight, and the disc is convex.
  - In space A the web is ≥ 2 mm. The bound is not attained: the 42 mm pocket corner sits at ±0.37 rad, and the CPR3 bridge controls near it are limited to 1 mm. The bound is valid but loose.

No premise failed, so no correction is needed.

### Minor wording notes for the paper
- Say that MAG is the iron depth *on the pole axis*, to the face chord. The phrase "outer face at radius 44 − MAG" describes the face as an arc, which it is not.
- CPR3's box is [−1, 0], not [−1.5, 0]. The 1.5 mm figure is the maximum over groups.
- The 44.5 mm guard is redundant within the box. Citing the offset sign gives the tighter 0.708 mm bound for the gap.
- Not a bound issue: at SW3 = 0.5 mm and CPS1 = 0.5 mm, the moved opening corner nearly reaches C±, so the channel wall becomes nearly degenerate. This is a meshing and validity concern, not a gap concern.
