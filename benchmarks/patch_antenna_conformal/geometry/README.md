# Optimized-geometry dumps, 2.4 GHz bent patch (#1087)

There is one `L{0,1}_<label>.json` per nested-DOF run in `../nested_dof/`. The matching TOML pins each file by `[shape] geometry_json_sha256`. The files are written by `crates/geode-core/examples/patch_conformal_nested_dof.rs` and consumed by:

- the Meep matched-geometry cross-check (#1088);
- `../scripts/plot_patch_displacement.py`, which makes `L1_combined_64.png`.

These geometries hold only on the mesh level they were optimized on, and they move the substrate thickness. See `../nested_dof/README.md` before using them.

## Format

- **Units:** mm. **Frame:** the bent fixture coordinates `(x, y, z)`. The bend axis is parallel to `y` through `(x = 0, z = z_axis_mm)`, and the chart is `s = R·atan2(x, z − z_axis)`.
- **Node arrays.** `node_ids`, `is_patch`, `base` and `moved` are parallel arrays, one entry per dumped node (`n_nodes` of them). The dumped nodes are the substrate-top surface nodes, with the patch nodes among them. `node_ids[i]` is the 0-based node index of local node `i` in the fixture mesh.
- **Index arrays are local, not mesh ids.** `patch_triangles`, `substrate_top_triangles` and `patch_outline_loop` hold **0-based indices into the dump's own node arrays**. They are not mesh node ids. To get a mesh id, map through `node_ids`. The JSON records this as `"index_base": 0` and in its `"format"` string.
- **The ground plane is not dumped.** It does not move radially. Under outline DOFs its nodes slide only tangentially, along the base-point tangent, so it stays on the R = 100 mm cylinder up to the chord error `d²/2R` (0.18 mm at `d = 6` mm). The patch–ground gap is preserved because the whole slab column moves together.

The `index_base` and `format` fields and the ground-plane wording were added to the committed files in the Doctor pass on PR #1104. No coordinate, index or S₁₁ value changed. The writer in the example now emits the same fields, so `generated_at_commit` still names the commit whose code produced the numbers.

## L2 designs (#1103)

`L2_combined_64_cold.json`, `L2_combined_64_warm.json` and `L2_gap_48_warm.json` are the L2 re-optimizations of gate G2 (see `../nested_dof/README.md`). They have the same format as the L0/L1 dumps. **They are an idealized non-uniform-thickness ablation.** Their radial DOFs thicken the FR-4 under the patch to about 3.7 mm on average, which cannot be manufactured (operator ruling, 2026-10-13). The fixed-thickness, copper-only design is #1120. A Meep cross-check of these shapes must model the variable-thickness slab.

## Copper-only designs on the fixed 1.6 mm board (#1120)

`<level>_copper_38*.json` and `rb<X>_<level>_copper_38_*.json` are the manufacturable designs of `../copper/` (see `../copper/README.md`): the board is the undeformed R = 100 mm, uniform 1.6 mm FR-4 shell, and only the etched copper and the probe change. **The headline design for the Meep cross-check (#1088) is `rbD_L1_copper_38_sm45.json`.**

They have the same format as the dumps above, with these differences:

- **Every dumped node lies exactly on its own cylinder.** The exact chart morph moves substrate nodes within their cylinder (`max_slab_radius_change_mm` ≤ 1.4e-14), so the ground plane is still the unchanged substrate bottom at R − 0.8 mm.
- **The probe moves and resizes.** `feed_moved_s_y_mm` is the moved probe centre in the chart, and `probe_w_moved_mm` is the lumped-port width along `y` (the gap stays 1.6 mm). `inset_moved_mm` is the distance from the moved `+y` edge (at the probe's `s`) to the probe centre. The `probe_*_mm` fields give the probe of the fixture the design was optimized on. For a re-based fixture (`rb*`) that is already the moved probe of the design it was carried from; see `../copper/README.md`.
- `corner_basis`, `feed_basis`, `n_corner_dofs`, `n_feed_dofs`, `thickness_fixed = true` and `band_shift_ghz` are added. `band_shift_ghz` is the frequency space-mapping shift the design was optimized with: the design is meant for the band as the p = 2 model sees it, not as its own mesh sees it.
