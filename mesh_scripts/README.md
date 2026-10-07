# mesh_scripts/

Sources for the Mie-sphere test meshes and the open-space Mie reference roots.
The generated outputs are committed elsewhere (meshes under
`crates/geode-core/tests/fixtures/` and `reference/fixtures/sphere_*/`, the
root table in `geode-core`), so these
files are needed only to regenerate them. The other Gmsh geometries and
fixture generators are in [`reference/gmsh/`](../reference/gmsh/).

| File | Purpose |
|---|---|
| [`sphere.geo`](sphere.geo) | Dielectric sphere (R = 1) in a vacuum gap and an absorbing PML shell, with physical groups `sphere_interior`, `vacuum_gap`, `pml_shell`, `outer_boundary`, `sphere_surface` (issues #25, #28, #38). Source of the sphere fixtures; see `crates/geode-core/src/mesh/sphere.rs` |
| [`sphere_small.geo`](sphere_small.geo) | The same topology at about 200 tets, small enough for the default-CI Burn vs NumPy cross-check (issue #158) |
| [`mie_open_space_roots.py`](mie_open_space_roots.py) | Root-finder for the complex open-space Mie whispering-gallery resonances. Prints a Rust `const` table for `geode-core`'s open-space Mie oracle (`src/analytic/mie/open.rs`). Needs numpy and scipy |

```sh
gmsh -3 -format msh4 -o sphere.msh mesh_scripts/sphere.geo
python3 mesh_scripts/mie_open_space_roots.py
```
