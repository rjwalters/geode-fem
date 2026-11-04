# The copper design re-centred on the refined forward model (#1134)

#1126 X1 found that the v5 headline design (`../copper/rbD_L1_copper_38_sm45`) misses −10 dB once the patch rim is resolved. At Palace p = 3 its band is 2.4029–2.4975 GHz and its worst in-band value is −9.56 dB. The coarse-to-fine band shift it was centred with (44.94 MHz) stopped at P2/L1, which sits about 9 MHz low. This directory re-centres the fixed-1.6 mm copper design on the refined model, with a realizable 1.6 mm probe, and verifies it on geode's finest mesh and with Palace p = 3 on the X1 setup.

Every number below comes from a committed TOML in this directory. `../scripts/recentre_summary.py --write` collects them into `recentre_summary.toml`, and `crates/geode-core/tests/patch_conformal_recentre.rs` pins them. σ₀ = 1 throughout.

## Result

**The target is met on the most accurate model.** The re-centred design `rbG_L1_copper_38_sm57_w16` (38 copper DOFs, fixed 1.6 mm FR-4, a 1.6 mm probe) clears −10 dB across 2.400–2.4835 GHz on Palace p = 3 and on geode's finest mesh:

| model | worst of the 9 in-band points | −10 dB band | ISM coverage | margin at 2.400 / 2.4835 GHz |
|---|---|---|---|---|
| **Palace p = 3** on the design's L1 mesh (2.50M DOFs) | **−10.46 dB** | **2.3935–2.4864 GHz** (93.0 MHz) | **100 %** | **6.5 / 2.9 MHz** |
| geode P1, 0.1 mm rim + 0.3 mm slab (1.74M DOFs) | −10.73 dB | 2.3934–2.4881 GHz (94.7 MHz) | 100 % | 6.6 / 4.6 MHz |
| v5 headline `rbD_L1_copper_38_sm45`, Palace p = 3 (X1) | −9.56 dB | 2.4029–2.4975 GHz (94.6 MHz) | 96.5 % | −2.9 / 14.0 MHz |

The two independent fine models agree on the lower band edge to 0.02 MHz, as they agreed on the initial patch's dip in X1. At p = 3 the margin is 0.46 dB, with the worst point at 2.4835 GHz; at 2.400 GHz the design is at −11.16 dB.

**The margin is small, and the volume guard is what limits it.** The design has about 9.5 MHz of total slack at p = 3 (a 93.0 MHz band over the 83.5 MHz ISM band). Both descents stopped on the 0.25 tet-volume guard at the patch widening, not at a converged optimum, so the bandwidth, and with it the slack, is what the guard allows. The slack was placed 6.5 MHz at the lower edge and 2.9 MHz at the upper, because refinement moves the band up (see "Margin against the remaining refinement uncertainty"). Against X1's estimate of the remaining drift above p = 3 (0–14 MHz), the lower-edge margin covers the order-1 Richardson value (6.4 MHz) but not the rim fit (13.5 MHz).

## Method

**Start.** The X5 realizable-feed design `../v6/x5/rbG_L1_copper_38_sm45_w16` (38 copper DOFs, probe width pinned at 1.6 mm on `rbG_L1`). It already clears −10 dB at P2/L1 (−10.62 dB), so only the centring has to move.

**Space mapping, one more step.** The descent is the #1120 one: L-BFGS on the soft-max of |S₁₁|² over 5 band points on L1 at P1, with the band shifted down by the coarse-to-fine shift (`--band-shift`). The flags are the X5 ones (`--rebase-feed --feed-support-legacy --freeze-feed-width`; the fixture is the design's own, so the re-base shift is zero and the copper is carried exactly). Two shifts were run in parallel:

| run | band shift | why |
|---|---|---|
| `sm54` | 53.6 MHz | 44.94 (L1 → P2/L1) + 8.7 (the v5 design's P2 → Palace p3 dip shift): centred for p = 3 |
| `sm57` | 57.0 MHz | 3.4 MHz further down: biased low, because every refinement computed so far moves the band up |

Both descents re-centre within three steps and then stop **distortion-limited on the 0.25 volume guard**, at the same site as the X5 run: an air tet 5.5 mm outside the −s edge, where the patch widening (38 → 49.7 mm) compresses the margin. So the guard, not the objective, ends the descent, and it caps the bandwidth. The L1 −10 dB band is 91.0–91.3 MHz, against 91.9 MHz for the X5 start.

| design | steps | solves | worst on its shifted L1 band | as built (L × W, inset, probe) |
|---|---|---|---|---|
| `sm54` | 11 | 130 | −9.23 → −10.65 dB | 28.95 × 49.72 mm, 5.11 mm, 1.6 mm |
| `sm57` | 10 | 125 | −8.73 → −10.64 dB | 29.00 × 49.72 mm, 5.14 mm, 1.6 mm |

**Choosing the headline.** On the 0.2 mm-rim mesh `sm54` lands 2.3884–2.4820 GHz and `sm57` 2.3850–2.4783 GHz. The 0.2 mm rim sits with P2/L1 (X1), and the rim-to-p3 step measured on the initial patch is +9 MHz, so `sm54` would sit about 2.6 MHz from the lower band edge at p = 3 and `sm57` about 6 MHz. `sm57` was taken forward to the expensive verifications. `sm54` has only its L1 and 0.2 mm-rim rows.

## Verification of `sm57`

The models, in order of refinement. "Worst in band" is the worst |S₁₁| over the 9 in-band points (2.400 to 2.4835 GHz in 10.4375 MHz steps): geode solves them, and Palace interpolates its 1 MHz grid at them, so the criterion is the same for both codes. The −10 dB band is the contiguous run around the dip, with crossings interpolated linearly in dB between computed points. The geode grids are 7.5 MHz on L1, 5 MHz on the 0.2 mm rim, and on the combined mesh only the 9 in-band points plus 2.390, 2.395, 2.4885 and 2.4935 GHz. Palace's grid is 1 MHz. Margins are 2.400 GHz minus the lower crossing and the upper crossing minus 2.4835 GHz; positive means covered.

| model | air morph | DOFs | min vol ratio | worst in band | −10 dB band (GHz) | width | ISM coverage | margin lo / hi (MHz) |
|---|---|---|---|---|---|---|---|---|
| P1 on L1 (optimization mesh) | default | 150 541 | 0.257 | −4.94 dB | 2.3393–2.4304 | 91.0 MHz | 36 % | 60.7 / −53.1 (sits 57 MHz low by design) |
| P2 on L1 (geode) | default | 804 336 | 0.257 | −9.08 dB | (9 points only; −13.20 at 2.400) | | | |
| **Palace p = 2** on L1 | default | 884 536 | (same mesh) | **−9.08 dB** | 2.3847–2.4770 | 92.3 MHz | 92.2 % | 15.3 / −6.5 |
| P1, 0.2 mm rim | default | 431 528 | 0.172 | −9.25 dB | 2.3850–2.4783 | 93.3 MHz | 93.8 % | 15.0 / −5.2 |
| P1, 0.2 mm rim | on-cylinder | 431 528 | 0.262 | −9.23 dB | 2.3849–2.4782 | 93.3 MHz | 93.6 % | 15.1 / −5.3 |
| P1, 0.1 mm rim + 0.3 mm slab | on-cylinder | 1 738 279 | 0.218 | **−10.73 dB** | 2.3934–2.4881 | 94.7 MHz | 100 % | 6.6 / 4.6 |
| **Palace p = 3** on L1 | default | 2 503 077 | (same mesh) | **−10.46 dB** | **2.3935–2.4864** | **93.0 MHz** | **100 %** | **6.5 / 2.9** |

- **geode's finest mesh meets the target.** The combined fixture (`rbGcomb_L1`: the X1 `x1comb` refinement at the rbG probe, 1.74M DOFs) gives −10.73 dB worst in band and a 94.7 MHz band, 2.3934–2.4881 GHz, covering ISM with 6.6 MHz to spare at 2.400 GHz and 4.6 MHz at 2.4835 GHz.
- **P2/L1 and the 0.2 mm rim now miss, at the upper edge, by construction.** They sit about 8.5 MHz below the combined mesh (`[refinement].sm57_comb_minus_rim020_lo_edge_mhz`), as X1 found for the initial patch (9.0 MHz, x1rim020 → x1comb). A design centred for the refined model is high on them: −9.08 dB (P2/L1) and −9.23 dB (0.2 mm rim) at 2.4835 GHz, with more than 13 dB at 2.400 GHz. P2/L1 is no longer the model to headline.
- **Palace.** On the design's own L1 mesh, Palace p = 2 reproduces geode P2/L1: −9.076 against −9.079 dB worst in band, the same agreement as for the v5 design in X1. Palace p = 3 moves the band up 8.7 MHz from p = 2 (8.4 MHz for the v5 design) and lands it 2.3935–2.4864 GHz, −10.46 dB worst in band. Its lower edge agrees with geode's combined mesh to 0.02 MHz. The upper edges differ by 1.7 MHz (2.4864 against 2.4881 GHz), and the worst-in-band values by 0.27 dB, both at 2.4835 GHz. The geode upper crossing is interpolated across a 5 MHz gap (2.4835 → 2.4885 GHz) and Palace's across 1 MHz, so part of that difference is the grid. Palace's absorber differs too (first-order ABC on the air box, X1).

### Mesh quality on the refined meshes

The fixtures are rectangular 38 × 29 mm patches with the rim refinement laid around that outline, and the design moves the outline by up to 6 mm. Two checks:

- **The rim refinement moves with the outline.** `../scripts/rim_edge_stats.py` measures the patch-triangle edges within 0.5 mm of the *moved* outline on the moved meshes (`rim_edges.toml`): 0.205 mm mean on the 0.2 mm-rim mesh and 0.103 mm on the combined mesh, the same as the fixtures' undeformed 0.197 and 0.098 mm.
- **The default air morph inverts tets on the combined mesh.** The default (linear) air motion leaves the cylinders on which the slab and patch move. That costs a chord error of about d²/2R ≈ 0.05 mm for a 3 mm move at R = 100 mm, which is comparable to the 0.1 mm rim tets above the patch. The first combined run (`..._on_rbGcomb_L1_default_morph_inverted.toml`) had a minimum volume ratio of −1.13, an air tet 0.1 mm above the patch, 0.3 mm inside the −s edge. Its numbers are recorded and not used. The refined meshes were re-run with `--air-on-cylinder` (the v5 L3 remedy): 0.26 on the 0.2 mm rim and 0.22 on the combined mesh, where the worst tet is back at the widening site in the substrate. The air motion is a mesh choice and not a design change: the copper, slab and ground are identical, and on the 0.2 mm rim the two morphs agree to 0.09 MHz at the lower edge and 0.015 dB worst in band. L1 (P1 and P2) and the Palace mesh keep the default morph the design was optimized with (minimum ratio 0.257), as in X1.

## Margin against the remaining refinement uncertainty

X1 measured, for the initial patch, how far the dip still moves above the finest models. Palace p = 3 on L1 and geode's combined mesh agree (2.3660 and 2.3659 GHz). If the monotone trend holds, those are lower bounds, and the converged dip is estimated at 2.366–2.380 GHz: 0 to 14 MHz higher. The order-1 Richardson step on the rim pair gives +6.4 MHz and the rim-size fit +13.5 MHz (`[refinement]`). The fit is not in its asymptotic range, so these are estimates, not bounds.

An upward shift d moves the whole band (the p = 2 → p = 3 step of this design, 8.7 MHz, is close to the initial patch's 9.0 MHz, so the design shifts like the patch). It only gains at the upper edge, and keeps ISM covered at the lower edge while d ≤ 6.5 MHz:

| remaining shift above Palace p = 3 | lower band edge | ISM covered? |
|---|---|---|
| 0 (p = 3 is converged) | 2.3935 GHz | yes, 6.5 MHz margin, −10.46 dB worst in band |
| +6.4 MHz (order-1 Richardson) | about 2.3999 GHz | just: about 0.1 MHz margin, about −10.0 dB at 2.400 GHz |
| +13.5 MHz (rim fit) | about 2.4070 GHz | no: about 7 MHz short, about −8.9 dB at 2.400 GHz |

The last two rows are the measured p = 3 response shifted by d (Palace's |S₁₁| at 2.400 − d GHz), not computed. So **the design meets the target with margin on the most accurate computed models**. It stays inside the band for the lower half of X1's drift estimate, and it would miss at 2.400 GHz if the drift reaches the rim-fit value. Covering the full 0–14 MHz range needs about 14 MHz of lower-edge slack on top of the upper-edge need, about 97.5 MHz of −10 dB bandwidth. That is 4.5 MHz more than this guard-limited design has.

## Not done

- **A wider band.** The descents stop on the 0.25 volume guard at the widening, so the bandwidth (about 93–95 MHz on the refined models) is what the guard allows, not an optimum. A re-based fixture at the widened outline would relieve it; that was not run.
- **Optimizing on the refined mesh directly.** The design was optimized on L1 with a band shift and verified on the refined meshes, not optimized on them.
- **Palace on the refined meshes.** Palace ran on the design's L1 mesh only, as in X1. Its p-refinement and geode's rim h-refinement are the two independent fine models.
- **The 1.27 mm probe.** X5 showed it behaves like the 1.6 mm one at P2/L1; it was not re-centred here.
- **A coax feed and copper loss.** The probe is still a uniform 1.6 mm lumped port, and conductors are PEC.

## Reproducing

Each TOML's `[meta].command` is the exact command, and the MANIFEST in `crates/geode-core/tests/benchmark_provenance.rs` maps the design copies (`<stem>.design.toml`) to their committed sources. The fixtures are `reference/gmsh/patch_conformal_rbG_L1.yaml` (#1126) and the new `patch_conformal_rbGrim020_L1.yaml` and `patch_conformal_rbGcomb_L1.yaml` (rbG with `lc_rim` 0.2, and with `lc_sub_patch` 0.3 plus `lc_rim` 0.1), generated with gmsh 4.15.2 on the Mac. Their sha256 are in `../provenance/` and in each artifact's `[meta].fixture_sha256`. The meshes are not committed. The Palace runs used the vendored `reference/palace/docker/Dockerfile` (Palace `fba6a5b`, `palace/palace-build-info.txt`), `../scripts/palace_patch_config.py` with the X1 settings (`AdaptiveTol` 1e-3, 1 MHz step, GMRES to 1e-8, -np 16 on an r6i.4xlarge), on the moved mesh written by `--write-mesh` (sha256 in `palace/meshes.txt`). Logs are in `logs/`, with host paths scrubbed.

geode ran on an r8g.8xlarge (Graviton4) and Palace on an r6i.4xlarge (x86), both in us-east-1.
