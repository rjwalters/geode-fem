# The copper design re-centred on the refined forward model (#1134)

#1126 X1 found that the v5 headline design (`../copper/rbD_L1_copper_38_sm45`) misses −10 dB once the patch rim is resolved. At Palace p = 3 its band is 2.4029–2.4975 GHz and its worst in-band value is −9.56 dB. The coarse-to-fine band shift it was centred with (44.94 MHz) stopped at P2/L1, which sits about 9 MHz low. This directory re-centres the fixed-1.6 mm copper design on the refined model, with a realizable 1.6 mm probe, and verifies it on geode's finest mesh and with Palace p = 3 on the X1 setup.

Every number below comes from a committed TOML in this directory. `../scripts/recentre_summary.py --write` collects them into `recentre_summary.toml`, and `crates/geode-core/tests/patch_conformal_recentre.rs` pins them. σ₀ = 1 throughout.

## Result

{RESULT}

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

{TABLE}

- **geode's finest mesh meets the target.** The combined fixture (`rbGcomb_L1`: the X1 `x1comb` refinement at the rbG probe, 1.74M DOFs) gives −10.73 dB worst in band and a 94.7 MHz band, 2.3934–2.4881 GHz, covering ISM with 6.6 MHz to spare at 2.400 GHz and 4.6 MHz at 2.4835 GHz.
- **P2/L1 and the 0.2 mm rim now miss, at the upper edge, by construction.** They sit about 8.5 MHz below the combined mesh (`[refinement].sm57_comb_minus_rim020_lo_edge_mhz`), as X1 found for the initial patch (9.0 MHz, x1rim020 → x1comb). A design centred for the refined model is high on them: −9.08 dB (P2/L1) and −9.23 dB (0.2 mm rim) at 2.4835 GHz, with more than 13 dB at 2.400 GHz. P2/L1 is no longer the model to headline.
- **Palace.** {PALACE}

### Mesh quality on the refined meshes

The fixtures are rectangular 38 × 29 mm patches with the rim refinement laid around that outline, and the design moves the outline by up to 6 mm. Two checks:

- **The rim refinement moves with the outline.** `../scripts/rim_edge_stats.py` measures the patch-triangle edges within 0.5 mm of the *moved* outline on the moved meshes (`rim_edges.toml`): 0.205 mm mean on the 0.2 mm-rim mesh and 0.103 mm on the combined mesh, the same as the fixtures' undeformed 0.197 and 0.098 mm.
- **The default air morph inverts tets on the combined mesh.** The default (linear) air motion leaves the cylinders on which the slab and patch move. That costs a chord error of about d²/2R ≈ 0.05 mm for a 3 mm move at R = 100 mm, which is comparable to the 0.1 mm rim tets above the patch. The first combined run (`..._on_rbGcomb_L1_default_morph_inverted.toml`) had a minimum volume ratio of −1.13, an air tet 0.1 mm above the patch, 0.3 mm inside the −s edge. Its numbers are recorded and not used. The refined meshes were re-run with `--air-on-cylinder` (the v5 L3 remedy): 0.26 on the 0.2 mm rim and 0.22 on the combined mesh, where the worst tet is back at the widening site in the substrate. The air motion is a mesh choice and not a design change: the copper, slab and ground are identical, and on the 0.2 mm rim the two morphs agree to 0.09 MHz at the lower edge and 0.015 dB worst in band. L1 (P1 and P2) and the Palace mesh keep the default morph the design was optimized with (minimum ratio 0.257), as in X1.

## Margin against the remaining refinement uncertainty

{MARGIN}

## Not done

- **A wider band.** The descents stop on the 0.25 volume guard at the widening, so the bandwidth (about 93–95 MHz on the refined models) is what the guard allows, not an optimum. A re-based fixture at the widened outline would relieve it; that was not run.
- **Optimizing on the refined mesh directly.** The design was optimized on L1 with a band shift and verified on the refined meshes, not optimized on them.
- **Palace on the refined meshes.** Palace ran on the design's L1 mesh only, as in X1. Its p-refinement and geode's rim h-refinement are the two independent fine models.
- **The 1.27 mm probe.** X5 showed it behaves like the 1.6 mm one at P2/L1; it was not re-centred here.
- **A coax feed and copper loss.** The probe is still a uniform 1.6 mm lumped port, and conductors are PEC.

## Reproducing

Each TOML's `[meta].command` is the exact command, and the MANIFEST in `crates/geode-core/tests/benchmark_provenance.rs` maps the design copies (`<stem>.design.toml`) to their committed sources. The fixtures are `reference/gmsh/patch_conformal_rbG_L1.yaml` (#1126) and the new `patch_conformal_rbGrim020_L1.yaml` and `patch_conformal_rbGcomb_L1.yaml` (rbG with `lc_rim` 0.2, and with `lc_sub_patch` 0.3 plus `lc_rim` 0.1), generated with gmsh 4.15.2 on the Mac. Their sha256 are in `../provenance/` and in each artifact's `[meta].fixture_sha256`. The meshes are not committed. The Palace runs used the vendored `reference/palace/docker/Dockerfile` (Palace `fba6a5b`, `palace/palace-build-info.txt`), `../scripts/palace_patch_config.py` with the X1 settings (`AdaptiveTol` 1e-3, 1 MHz step, GMRES to 1e-8, -np 16 on an r6i.4xlarge), on the moved mesh written by `--write-mesh` (sha256 in `palace/meshes.txt`). Logs are in `logs/`, with host paths scrubbed.

geode ran on an r8g.8xlarge (Graviton4) and Palace on an r6i.4xlarge (x86), both in us-east-1.
