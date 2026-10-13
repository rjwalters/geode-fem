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
