# Copper-only design on a fixed 1.6 mm FR-4 board, 2.4 GHz bent patch (#1120)

Operator ruling (2026-10-13): the board stays a standard, uniform 1.6 mm FR-4 shell, and only the etched copper is optimized. This directory holds that design. The non-uniform-thickness designs of #1087 / #1103 (`../nested_dof/`) are an idealized ablation only.

Generator: `crates/geode-core/examples/patch_conformal_nested_dof.rs --copper`. Tests: `crates/geode-core/tests/patch_conformal_copper.rs`. Gate-G2 table: `python3 ../scripts/copper_g2.py`. Geometry dumps: `../geometry/<level>_copper_*.json`. Every number is at UPML σ₀ = 1.

## Design freedom: 38 copper DOFs, no radial DOF

The board is frozen: there are **no radial (gap) DOFs**, and every substrate node moves only **within its own cylinder** about the bend axis. The exact on-cylinder morph `conformal_basis::ChartMorph` moves a node with chart displacement `(Δs, Δy)` to angle `φ₀ + Δs/R` and `y₀ + Δy` **at its base radius**. The whole radial column through the slab rotates together, so the FR-4 is never sheared and stays a uniform 1.6 mm shell. The patch stays on the `R + h/2` offset surface and the ground on `R − h/2`. The #1087 straight-line outline columns did not do this: they left the cylinder by the chord error `d²/2R`. Air nodes follow the harmonic extension of the first-order motion. The design gradient chains the nodal adjoint gradient through the exact Jacobian of this nonlinear map at `x`.

Every run records the check in `[copper]` (`max_slab_radius_change_mm ≤ 1.4e-14`, patch and ground radius ranges) and `[crater]` (thickness 1.600 mm everywhere). The test `copper_morph_keeps_the_board_a_uniform_1p6_mm_shell` checks it for random designs and shows the #1087 linear morph fails the same check.

| group | DOFs | what moves |
|---|---|---|
| outline | 32 | 8 cubic B-splines per patch edge `[+y, −y, +s, −s]`, each moving its edge outward in the surface by 1 mm (mid-plane arc) per unit; ramped into the patch from the pinned feed band and decaying over the 12 mm substrate margin |
| corners | 4 | one bump per corner, moving it outward along the bisector by 1 mm per unit (negative = chamfer), support `1 − smoothstep(d/6 mm)` |
| feed | 2 | the probe's **inset** (its `y` position: 1 mm away from the `+y` edge per unit) and its **width** (the lumped-port rectangle's `y` extent: +1 mm per unit), both along the bend axis |

## The feed: a smooth morph with an exact moving-feed adjoint, re-based by remeshing

The fixture is **probe-fed**: a vertical coax-probe lumped port inset 7.0 mm from the `+y` radiating edge (`patch_antenna.geo`, `probe_inset`, `probe_w`). There is no inset notch cut into the copper. "Inset depth" is the probe position and "width" is the probe footprint. An inset-fed microstrip notch would be a topology change and is not modelled.

**Choice: a smooth morph for the gradient, and an exact parametric remesh only to relieve mesh distortion.**

- **Gradient.** The feed DOFs move the port faces, so `S₁₁` depends on the port geometry through the port admittance `(jω/Z_s) S_p` with `Z_s = R·w/l`, the port drive and the port-voltage functional. The new `driven::shape::driven_shape_gradient_matched_upml_moving_port_s11` differentiates all of them on the matched-UPML open radiator, with one LU per frequency. These are the #633 moving-feed terms, now shared with the scalar path through `accumulate_moving_port_terms`. The port is rebuilt on every moved mesh with `PatchFixture::port_on_mesh`. It is FD-validated in `mesh::patch::tests::curved_conformal_moving_feed_gradient_matches_central_finite_difference` to 7.8e-10. For a pure probe resize the port terms are about 0.2 % of the derivative, and the pinned-feed path misses by 1.6e-3. For a rigid probe translation (the inset DOF) the two paths agree exactly. In the full pipeline the inset and width DOFs pass FD on L0 and L1 to ≤ 1.9e-7.
- **Why not a parametric remesh with FD of the inset** (the #1111 hybrid)? #1111 found that remesh noise dominates such differences. The smooth morph gives an exact, noise-free gradient.
- **Re-base.** The morph's limit is mesh distortion. Moving the probe toward the `+y` edge compresses the 5–7 mm gap between them, and on L0, L1 and L2 the 0.25 volume-ratio guard binds there, 3.4–4.0 mm from the feed. A guard-limited feed is **re-based**: the fixture is regenerated (gmsh 4.15.2 on the Mac) with `probe_inset` / `probe_w` set to the moved probe (`reference/gmsh/patch_conformal_rb*_L*.yaml`), and the design is carried over with `--rebase-feed`. That shifts the two feed coefficients by the change of the fixture's probe, so the physical design (probe position and width, outline, corners) is identical, and the optimization continues on an undistorted feed. The outline edge motion does not depend on the probe band, so only the feed coefficients change. Check: the L0 optimum gives −9.56 dB on L0 and −9.70 dB on L0 re-based at its probe (L0 remesh error), with the minimum volume ratio back from 0.25 to 0.56.

## Limitation: no interior slots or deep notches

**Interior slots (U-slot, E-patch) cannot be represented without a topology change.** A slot is a hole in the PEC patch sheet, which is a change of the PEC edge mask. No smooth motion of a fixed-topology mesh opens a hole, and the mask is not differentiable. Edge notches are representable only as smooth indentations of the outline spline, with the knot spacing 38/5 = 7.6 mm along `s` and 29/5 = 5.8 mm along `y`. Narrow, deep notches would need a finer outline basis on a finer mesh, or a remesh. The corner bumps give chamfer-like freedom at the four corners.

## Strategy: optimize on the coarse model, correct with a frequency shift fitted on P2/L1

#1103 found that a design optimized on a coarse mesh resonates higher at higher fidelity. For the copper designs that drift is **a pure frequency shift**. A design's response with p = 2 on L1 (P2/L1, the finest model) equals its coarse-model response shifted up in frequency, to an rms complex misfit of 0.018–0.025 (0.23–0.38 unshifted). The fit is `scripts/copper_shift_fit.py`, pinned in `the_detune_to_p2_on_l1_is_a_pure_frequency_shift`:

| coarse model | design | fitted shift | the initial patch's shift |
|---|---|---|---|
| L1 | `L1_copper_38` | 47.85 MHz | 44.8 MHz |
| L2 | `L2_copper_38_s30` | 29.33 MHz | 27.9 MHz |
| L1 (re-based) | `rbB_L1_copper_38_sm48` | 44.94 MHz | |

The copper morph adds almost no detune of its own: the shift is within 3 MHz of the initial patch's. The #1103 thickness designs drifted 75 MHz. So the correction is **frequency space mapping**. The corrective step re-optimizes on the coarse model for the band shifted down by the fitted shift (`--band-shift`), then checks the result with p = 2 on L1. One refit was needed on each chain.

## Results (σ₀ = 1, guard 0.25)

### Trajectory per level (worst of the band points, on the band the run optimized)

| run | level / fixture | start | band shift | worst initial → final (dB) | steps | terminal | wall |
|---|---|---|---|---|---|---|---|
| `L0_copper_38` | L0 | cold | 0 | −0.90 → −9.56 | 24 | distortion_limited (feed) | 0.2 h |
| `rbA_L0_copper_38_warm` | L0, feed re-based | L0 optimum | 0 | −9.70 → −9.87 | 19 | distortion_limited (widening) | 0.2 h |
| `L1_copper_38` | L1 | cold | 0 | −1.47 → **−10.04** | 24 | distortion_limited (feed) | 0.8 h |
| `rbB_L1_copper_38_warm` | L1, feed re-based | L1 optimum | 0 | −9.97 → −10.03 | 4 | plateau | 0.5 h |
| `L2_copper_38` | L2 | cold | 0 | −1.75 → **−10.04** | 22 | distortion_limited (feed) | 2.8 h |
| `L2_copper_38_s30` | L2 | cold | 30 MHz (hedge) | −2.41 → −9.98 | 16 | distortion_limited (feed) | 2.5 h |
| `rbC_L2_copper_38_sm29` | L2, feed re-based | s30 | 29.33 MHz (fitted) | −9.85 → **−10.46** | 25 | distortion_limited (widening) | 2.4 h |
| `rbB_L1_copper_38_sm48` | L1, feed re-based | rbB optimum | 47.85 MHz (fitted) | −4.75 → −10.25 | 26 | distortion_limited (widening) | 0.8 h |
| `rbD_L1_copper_38_sm45` | L1, feed re-based | sm48 | 44.94 MHz (refitted) | −10.26 → **−10.78** | 9 | distortion_limited (widening) | 0.7 h |

- **Each level, unshifted, lands at about −10.0 dB worst-of-band on its own mesh.** The fixed 1.6 mm board's −10 dB bandwidth is about the width of the ISM band. On their own levels the cold optima's dense −10 dB bandwidths are 84 MHz (L1) and 84 MHz (s30 on L2).
- **The last few tenths of a dB come from re-basing the feed and then widening the patch.** The re-based optimizations widen the patch from 38 to about 49 mm and fatten the probe from 1.6 to about 4.3 mm. Those are the two textbook single-patch bandwidth levers: a lower radiation Q and a lower probe inductance. They raise the own-level −10 dB bandwidth to about 92 MHz (`rbB_L1_copper_38_sm48` on its mesh).
- **Two independent chains converge to the same design family:**

  | chain | design | patch L × W | probe width | inset |
  |---|---|---|---|---|
  | L2 | `rbC_L2_copper_38_sm29` | 28.89 × 48.95 mm | 4.49 mm | 5.72 mm |
  | L1 | `rbD_L1_copper_38_sm45` | 28.80 × 49.75 mm | 4.27 mm | 5.07 mm |
- **FD gate.** It passes at `x0` and `x_final` on every run (worst relative error 2.2e-5, tolerance 5e-3). The per-group gate on L0 and L1 (outline, corners, inset, width) passes to ≤ 1.9e-7.
- **Where the guard binds.** Cold runs bind at the **feed**: a tet between the probe and the `+y` edge, 3.4–4.0 mm from the probe, inside the patch (L0, L1, L2, s30). After the feed re-base they bind at the **widening**: in the substrate margin 5–6 mm outside an `±s` edge. The L1 re-base continuation alone reaches a plateau. All of these are pinned in `guard_binding_sites_are_pinned`.

### Verification (fresh forward sweeps)

Every row is the worst of the 9 in-band points unless marked. "vol" is the minimum tet volume ratio of the verifying mesh against the 0.25 optimization guard. Rows below 0.25 are disclosed and pinned in `every_copper_verification_is_healthy_and_its_design_unchanged`.

| design | model (mesh) | worst-of-band (dB) | vol |
|---|---|---|---|
| `L1_copper_38` (unshifted L1 optimum, −10.04 on L1) | P2/L1 (L1) | −4.76 | 0.353 |
| `L2_copper_38_s30` | P2/L1 (original L1) | −9.92 | **0.244** |
| `L2_copper_38_s30` | P2/L1 (rbC, re-based) | −10.00 | 0.748 |
| `L2_copper_38_s30` | L3 | −7.87 | 0.310 |
| `rbB_L1_copper_38_sm48` | P2/L1 (rbB) | −9.96 | 0.250 |
| **`rbC_L2_copper_38_sm29`** (L2 chain) | P2/L1 (rbE) | **−10.10** | 0.299 |
| `rbC_L2_copper_38_sm29` | L3 (rbE) | −9.25 | **0.228** |
| **`rbD_L1_copper_38_sm45`** (final) | P2/L1 (rbD, its own mesh) | **−10.78** | 0.250 |
| `rbD_L1_copper_38_sm45` | P2/L1 (rbF, independent sibling mesh) | −10.35 | **0.234** |
| `rbD_L1_copper_38_sm45` | L3 (rbD, default air morph) | −8.81 | **0.100** |
| `rbD_L1_copper_38_sm45` | L3 (rbD, `--air-on-cylinder`) | −8.80 | **0.241** |

- **Remesh sensitivity.** The final design on an independent L1 mesh regenerated at its probe sits about 3 MHz higher: −10.35 dB against −10.78 dB, 0.4–0.7 dB at the band edges. It still clears −10 dB.
- **The L3 verification mesh.** The L3 mesh of the widened patch has thin air slivers next to the slab margin, which the default air morph squeezes to a volume ratio of 0.10. The on-cylinder air morph moves the same copper with the air kept on its cylinders, raising the ratio to 0.24. S11 changes by 0.01 dB, so the L3 shortfall is not a mesh artifact.

#### Dense sweeps (41 points, 2.30–2.60 GHz at 7.5 MHz, unless marked; `scripts/copper_g2.py` → `g2_summary.toml`)

| design | model | −10 dB band (GHz) | bandwidth | ISM coverage | centring | dense worst in band |
|---|---|---|---|---|---|---|
| **`rbD_L1_copper_38_sm45`** (final) | **P2/L1** | **2.3938–2.4887** | **94.9 MHz** | **83.5 MHz (100 %)** | +0.5 MHz | −11.37 dB |
| `rbD_L1_copper_38_sm45` | L3 (on-cylinder air) | 2.3801–2.4746 | 94.6 MHz | 74.6 MHz (89 %) | +14.4 MHz | −9.24 dB |
| `rbC_L2_copper_38_sm29` (L2 chain, 21 points, 2.35–2.55 GHz) | P2/L1 | 2.3993–2.4936 | 94.3 MHz | 83.5 MHz (100 %) | −4.7 MHz | −10.10 dB |
| `L1_copper_38` (own level, 61 points) | L1 | 2.3998–2.4841 | 84.3 MHz | 100 % | −0.2 MHz | −10.04 dB |
| `L2_copper_38_s30` (own level, 31 points) | L2 | 2.3697–2.4536 | 83.9 MHz | 64 % (shifted by design) | +30.1 MHz | |
| `rbB_L1_copper_38_sm48` (own level, 61 points) | L1 | 2.3456–2.4373 | 91.7 MHz | (shifted by design) | | |

Centring is the band centre minus the centre of the −10 dB interval (positive: the design sits low). The −10 dB band is the contiguous run around the dip, with crossings interpolated in dB.

### Gate G2 (manufacturable)

**G2 is met with p = 2 on L1, the finest model, on a manufacturable board.** The final design is below −10 dB across the whole ISM band:

- **worst-of-band −10.78 dB** on the 9 in-band points;
- a **94.9 MHz** −10 dB band (2.3938–2.4887 GHz) on the 41-point dense sweep, covering **100 %** of 2.4–2.4835 GHz and centred to 0.5 MHz.

It holds on an independent sibling mesh (−10.35 dB). The L2-chain design meets it independently: −10.10 dB worst-of-band, and a 94.3 MHz band covering 100 % of the ISM band.

**On L3 it is not met:** −8.80 dB worst-of-band and 89 % coverage. L3 resonates about 14 MHz below P2/L1, the same order-versus-mesh offset as the initial patch (2.342 against 2.357 GHz). A bandwidth of 95 MHz leaves only about 11 MHz of slack over the 83.5 MHz band, so a design centred for one model is off-centre on the other. The report treats P2/L1 as the reference because it is the finest model. It is still not the converged antenna; see "Not done here".

## The #1103 ablation, for comparison

The #1103 designs are an idealized ablation: they thicken the FR-4 under the patch to about 3.7 mm and cannot be manufactured. They reached −9.16 dB (cold L2 combined) with p = 2 on L1, missing G2 by 0.84 dB, after a 4.9 dB transfer loss. On the fixed 1.6 mm board, the copper-only design meets G2 at p = 2 on L1, despite having less freedom than #1103. This is not because copper is better than thickness. The space-mapping correction removes the transfer loss that #1103 suffered: the #1103 designs were optimized for the coarse mesh's band, and their non-uniform thickness added a 75 MHz detune of its own.

## Literature baseline

A single 1.6 mm FR-4 probe-fed patch is reported at about 60–95 MHz −10 dB bandwidth (about 2.5–3 %, plus FR-4 loss broadening), against the 83.5 MHz (3.4 %) ISM band. So full-band −10 dB is at or beyond typical. The final design's 94.9 MHz with p = 2 on L1 is at the top of that range, so it reaches full-band −10 dB with 11 MHz to spare. It gets there by widening the patch to 49.8 mm and using a 4.3 mm probe, both standard bandwidth measures, not by any exotic structure. On their own meshes the narrower 38 mm optima reach 84 MHz, in the middle of the range.

## Not done here

- **Interior slots, U-slots and E-patches.** They need a topology change; see above.
- **A shape adjoint at p = 2.** The adjoint is P1-only. The corrective steps use P1 gradients on the coarse model, with the p = 2 response entering through the fitted frequency shift.
- **Converged resonance.** P2/L1 is the finest model here, and it is not converged either (#1085: the initial patch's dip still rises under refinement). A design centred for P2/L1 sits about 15 MHz low on L3, and its position on the converged antenna is unknown.

## Reproducing

Each TOML's `[meta].command` is the exact command; the MANIFEST (`crates/geode-core/tests/benchmark_provenance.rs`) lists them with the design paths mapped to their committed sources.

- **Fixtures.** L1 to L3 are regenerated from `reference/gmsh/patch_conformal_L*.yaml` (sha256 pinned in the convergence tests). The re-based fixtures come from `reference/gmsh/patch_conformal_rb*_L*.yaml`, with their sha256 in each artifact's `[meta].fixture_sha256`, generated with gmsh 4.15.2 on the Mac, not with arm64 gmsh 4.12.1.
- **Design copies.** Verification and warm-start runs read a copy `<stem>.design.toml` of the committed `<stem>.toml`, and `[design].source_sha256` / `[start].design_toml_sha256` pin it.
- **FD-gate commands.** The FD-gate TOMLs' recorded command lacks `--fd-only`, which the generator did not record until a later commit; the MANIFEST gives the full command.
- **Commits.** The runs carry `generated_at_commit` `174836ac`, `cdcd1160`, `5f8198fd` or `714ee1ef`. All are branch commits, so merge with a merge commit, not a squash.
