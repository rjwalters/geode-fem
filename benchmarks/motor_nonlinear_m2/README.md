# Motor paper M2: sector constraints and the conforming sliding band (issue #1161)

This directory holds the validation record for
`crates/geode-core/src/assembly/magnetostatic_sector.rs` and the signed node
links it adds to `crates/geode-core/src/assembly/magnetostatic_nonlinear.rs`.

- **Sector pairing.** Antiperiodic or periodic pairing of the two radial
  edges of a machine sector. This is constrained elimination with a
  prolongation `P`: `A = P a + A_D`, the reduced residual is `Pᵀ R(P a)`,
  and the reduced tangent is the symmetric `Pᵀ K P`. `P` is exposed for the
  M3 adjoint.
- **Sliding band.** A conforming sliding band rotates the rotor by whole
  coupling steps `Δ = α/N`. Rotor coupling node `i` is identified with
  stator node `(i+k) mod N`, and the wrap past the sector edge picks up the
  antiperiodic sign. The rotor coordinates and the magnetization are rotated
  physically, so B is in the lab frame.

`m2_tables.log` is the full `--nocapture` output of the acceptance tests.
Regenerate it with:

```text
cargo test -p geode-core --release --test magnetostatic_sector_sliding -- --nocapture --test-threads=1
```

The test file is `crates/geode-core/tests/magnetostatic_sector_sliding.rs`,
and CI runs it in `integration-tests.yml` (core-tests-a). It takes about
12 s in release.

## Test machine

The test machine is a 6-pole (p = 3) slotless surface-PM machine on a
structured polar mesh (`geode_core::mesh::polar_sector`). The radii, in mm,
are:

| region | radius (mm) |
|---|---|
| rotor | 0–30 |
| magnet (radial M0 cos 3θ, 1.2 T) | 30–40 |
| inner air gap | 40–42.5 |
| outer air gap | 42.5–45 |
| winding (J0 cos 3θ) | 45–55 |
| outer region | 55 to r_out |

The coupling circle is at 42.5 mm, and A = 0 at r_out. The machine comes in
two variants:

- **"Iron"** has r_out = 57 mm. The rotor and the 2 mm stator yoke use the
  synthetic M1 steel table, which is not measured data. The magnet has
  μ_rec 1.05 and J0 is 1e7 A/m². The yoke saturates (max |B| 1.69–2.1 T).
- **"Air"** has r_out = 200 mm with every region μ_r = 1 and J0 = 4e7 A/m².
  This is the open-domain configuration of the closed-form
  `SlotlessPmDriven` torque oracle.

## Angle grid

Only `θ = kΔ` (k ∈ ℤ, any sign) is reachable, with `Δ = α/N`. The tests
use N = 60 per 60° (Δ = 1°), as well as 120 and 240. For the reference
meshes of #1162, N = 120…960 gives Δ = 0.5°…0.0625°, so the 1° and 2° steps
of the reference are `k = N/60` and `k = 2N/60`. `SlidingBand::index_of_angle`
returns `OffGrid` for any other angle.

## 1. Antiperiodic and periodic sectors against the full machine

The setup is nonlinear iron on an unsplit mesh, solved with rel_tol 1e-13.

| N/60° | case | nodes | reduced unknowns | forced zero | Newton its | max \|B\| iron (T) | max\|ΔA\|/max\|A\| |
|---|---|---|---|---|---|---|---|
| 30 | full 360° | 4861 | 4681 | 0 | 4 | 1.686 | (reference) |
| 30 | antiperiodic 60° | 838 | 780 | 1 | 4 | 1.686 | 4.2e-16 |
| 30 | periodic 120° | 1648 | 1561 | 0 | 4 | 1.686 | 5.2e-16 |
| 60 | full 360° | 9721 | 9361 | 0 | 4 | 1.687 | (reference) |
| 60 | antiperiodic 60° | 1648 | 1560 | 1 | 4 | 1.687 | 4.2e-16 |
| 60 | periodic 120° | 3268 | 3121 | 0 | 4 | 1.687 | 8.4e-16 |

The issue's gate is ≤1e-10. The one forced-zero node is the centre of the
antiperiodic sector: it is its own image, so `A = −A`, which pins it to 0.

## 2. Sliding band: the sector at step k against the full machine at step k

This is the sliding-band antiperiodic sector against the sliding-band full
machine (both split at r_c), with nonlinear iron, N = 60 and rel_tol 1e-13.

| k | angle (°) | wrap | max\|ΔA\|/max\|A\| | T sector (N·m/m) | rel ΔT |
|---|---|---|---|---|---|
| 0 | 0 | 0 | 3.0e-16 | −203.924721 | 9.8e-16 |
| 7 | 7 | 1 | 5.1e-16 | −200.175735 | 1.4e-16 |
| 23 | 23 | 1 | 9.0e-16 | −81.713448 | 0 |
| 59 | 59 | 1 | 6.3e-16 | 206.559443 | 9.6e-16 |
| 60 | 60 | 1 | 1.1e-15 | 205.386918 | 2.8e-16 |
| 67 | 67 | 2 | 9.8e-16 | 183.395619 | 1.6e-16 |
| 131 | 131 | 3 | 8.4e-16 | −184.661654 | 1.5e-16 |
| −5 | −5 | 0 | 5.4e-16 | −190.636309 | 6.0e-16 |

The re-connection and the wrap sign are exact. A sector placed at [60°,
120°] (the #1162 layout, with θ0 detected from the low radial edge) gives
the same torque to 2.8e-16 and 7.8e-16 at k = 7 and 67.

## 3. Rotation against a fresh mesh

This compares the sliding band at kΔ with a fresh mesh that has the rotor
at kΔ. The fresh mesh is unsplit, uses 3+1 gap layers instead of 2+2, and
its interior nodes are jittered by 0.3 of a cell. It has nonlinear iron and
the antiperiodic sector. B is compared at 360 lab-frame points in all six
regions; rotor points that have left the meshed span are mapped back through
the antiperiodic image.

| level | N/60° | nodes (band / fresh) | angle (°) | T band | T fresh | rel ΔT | rel L2 ΔB |
|---|---|---|---|---|---|---|---|
| 1 | 60 | 1709 / 1648 | 7 | −200.176 | −200.216 | 2.0e-4 | 1.13e-1 |
| 1 | 60 | 1709 / 1648 | 23 | −81.713 | −81.677 | 4.5e-4 | 1.32e-1 |
| 1 | 60 | 1709 / 1648 | 41 | 125.749 | 126.028 | 2.2e-3 | 1.12e-1 |
| 2 | 120 | 6656 / 6535 | 7 | −200.716 | −200.814 | 4.9e-4 | 5.68e-2 |
| 2 | 120 | 6656 / 6535 | 23 | −82.369 | −82.351 | 2.2e-4 | 6.72e-2 |
| 2 | 120 | 6656 / 6535 | 41 | 125.005 | 125.196 | 1.5e-3 | 6.35e-2 |
| 4 | 240 | 26270 / 26029 | 7 | −200.851 | −200.883 | 1.6e-4 | 2.85e-2 |
| 4 | 240 | 26270 / 26029 | 23 | −82.533 | −82.527 | 7.4e-5 | 3.43e-2 |
| 4 | 240 | 26270 / 26029 | 41 | 124.819 | 124.889 | 5.6e-4 | 3.15e-2 |

Taking the worst case over the three angles at each level:

- The torque difference is 2.2e-3, then 1.5e-3, then 5.6e-4.
- The B difference is 13%, then 6.7%, then 3.4%.

The B difference halves with each halving of h. That is O(h), as expected
for comparing the piecewise-constant P1 B of two different triangulations
point by point.

## 4. Linear slotless oracle through the sliding band

The setup is all air with the antiperiodic sector. The Arkkio torque (×6)
is compared with the closed-form `T(θ_r)`, whose amplitude is 399.839 N·m/m.
There are 16 angles: 0°…60° in 5° steps, plus 75°, 105° and −10°, which
cover wrap-around and negative k.

| level | N/60° | nodes | Newton its | Arkkio L2 / amp | max \|ΔT\|/amp | gap B rel L2 (centroids) |
|---|---|---|---|---|---|---|
| 1 | 60 | 2685 | 1 | 0.92% | 1.01% | 3.3% |
| 2 | 120 | 10528 | 1 | 0.25% | 0.28% | 1.7% |

The full per-angle table is in the log.

## 5. Reduced tangent PᵀKP

The setup is the sliding-band sector at N = 24 and k = 29, which wraps past
the sector edge and includes the forced-zero centre. The iron uses Brauer.

- **FD check.** It is evaluated at 1.1× the converged solution plus 1e-4
  noise. That state has a max \|B\| of 2.3 T and ν up to 897·k3. The
  central-difference error against `(PᵀKP)v` falls as O(h²):

  | h | 1e-2 | 1e-3 | 1e-4 | 1e-5 | 1e-6 | 1e-7 | 1e-8 |
  |---|---|---|---|---|---|---|---|
  | rel err | 1.8 | 1.1e-2 | 1.1e-4 | 1.1e-6 | 1.1e-8 | **2.3e-10** | 2.9e-9 |

- **Symmetry.** `max|K_r − K_rᵀ|/max|K_r|` = 9.0e-17.
- **Consistency with the exposed P.** `uᵀ K_r w` agrees with
  `(Pu)ᵀ K_full (Pw)` to 1.5e-15 over 4 random pairs, using the exposed `P`
  and the unconstrained system. `‖R_r − Pᵀ R_full‖/‖Pᵀ R_full‖` is 6.4e-17.
- **Adjoint solve.** The residual of `K_rᵀ λ = v` is 3.6e-15.

## 6. Warm starts and offset invariance (the #1159 pitfalls)

**1° sweep.** This is a 1° sweep over k = 0…12 with the default config. A
cold solve takes 4–5 Newton iterations at every angle. A warm start from the
previous angle takes 3. In total that is 59 cold iterations against 41 warm.
The warm and cold results differ by at most 4.9e-9. A restart from the
converged k = 12 solution takes 0 iterations.

**Constant Dirichlet offset C, periodic 120° sector.** Here a constant is
admissible, because every link has sign +1.

| C (Wb/m) | reference change | status | its | ‖(A − C) − A₀‖/‖A₀‖ |
|---|---|---|---|---|
| 0.4 | 2.2e-16 | Tolerance | 5 | 1.6e-14 |
| 1e2 | 0 | Tolerance | 5 | 3.5e-12 |

**Antiperiodic sector.** The two ends of the outer arc are linked with
sign −1, so a constant C ≠ 0 there contradicts the links. That is rejected
loudly, with `InvalidConstraint`, and is never solved silently.

## 7. Loud failures

Each of these gives a typed error:

- a radial node moved by 1 µm (`UnmatchedNode`, which names the node, its
  image, the distance and the tolerance);
- a node missing from one edge;
- a coupling node off the uniform grid (`NonUniformCoupling`);
- the wrong sector angle;
- a half-step angle (`OffGrid`);
- a triangle tag in neither tag set, or in both;
- a rotor/stator interface off the coupling circle;
- a link out of range, or with a sign other than ±1.

An unsplit mesh, with shared coupling nodes, is accepted: the band splits the
13 shared nodes itself.

## Mutation checks (run by hand, not committed)

Each mutation was applied alone, the test target was run in full, and the
change was reverted:

- **Wrap sign ignored** (`s^w → 1`): 4 tests fail (sector vs full, oracle,
  fresh mesh, warm sweep).
- **Signs dropped from the reduced tangent:** all 7 fail.
- **Class averaging in `restrict` broken:** 2 fail (tangent consistency,
  warm sweep).
