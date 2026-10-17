# `geode mesh` — layout → mesh → solve

`geode mesh` turns a **layout** (a layer stack + rectilinear conductor
polygons, JSON or TOML; schema: `geode schema layout`) into a tagged Gmsh
mesh plus a **starter problem spec** wired to the generated physical-group
names, for the analysis chosen with `--analysis` (`driven` by default,
`capacitance` or `inductance`). It needs Gmsh ≥ 4.11 with OpenCASCADE on
`PATH` (or `--gmsh` / `$GEODE_GMSH`).

Mesh sizes and node counts below were measured with Gmsh 4.15.2; other
versions produce slightly different meshes (and numbers).

## Driven: `spiral_inductor.layout.json`

A copy of `tests/fixtures/spiral_layout_smoke.json`: the smoke spiral
geometry as a layout — `m1` underpass, `m2` spiral legs and `via` layer
(thick PEC shells), a lumped gap port `between` the `feed` and `return`
shapes, PEC outer walls.

```sh
geode mesh crates/geode-cli/examples/mesh/spiral_inductor.layout.json \
    --mesh-out spiral.msh --spec-out spiral.spec.json -o mesh.json
geode driven spiral.spec.json
```

The mesh report (`kind = "mesh"`) lists the generated groups — dielectrics
`substrate` / `dielectric` / `air` / `air_buffer`, PEC shells `m1` / `m2` /
`via`, the `port` and `outer_boundary` — and embeds the starter spec
(~2 700 nodes, ~12 000 tets). The starter spec models the conductors as
**PEC** and sweeps 1 GHz: `geode driven` gives `L ≈ 0.72 nH` (11 259
interior DOFs). For lossy copper, edit the spec to move the conductor
groups from `boundary_conditions.pec` to `leontovich`, as
`tests/mesh_golden.rs` does.

## Capacitance: `guard_ring.layout.json`

A Kelvin guard-ring capacitor (mm units): a 1 mm square `plate` inside a
coplanar `guard` ring (0.1 mm gap) — both **nets** of the zero-thickness
`top` sheet layer — 1 mm above a grounded `bottom` plate (`ε_r = 4`
between, `ground = ["bottom"]`) and 1 mm below the grounded outer wall.

```sh
geode mesh crates/geode-cli/examples/mesh/guard_ring.layout.json --analysis capacitance \
    --mesh-out guard.msh --spec-out guard.spec.json
geode capacitance guard.spec.json --spice guard.sp
```

The starter spec has `terminals = ["plate", "guard"]`,
`ground = ["outer_boundary", "bottom"]` (7 835 nodes). The plate's row sum
`c_sigma_farad[0] ≈ 53.8 fF` is its guarded capacitance: **+1.15 %** above
Maxwell's gap-corrected parallel-plate value 53.19 fF, converging from
above on finer meshes (`tests/mesh_static_golden.rs`). Full matrix:
`c_farad ≈ [[285.3, −231.5], [−231.5, 1497.8]] fF`.

## Inductance: `shielded_microstrip.layout.json`

A thick 2 mm trace (`sig`) inside a rectangular PEC shield (`gnd`, `lid`,
`walls`) whose end caps `cap_in` / `cap_out` (nets of the wall layer) touch
the trace's end faces: the two **contacts** `sig_in` / `sig_out` become the
current path's source and sink.

```sh
geode mesh crates/geode-cli/examples/mesh/shielded_microstrip.layout.json --analysis inductance \
    --mesh-out ms.msh --spec-out ms.spec.json
geode inductance ms.spec.json
```

The starter spec has one path `sig` (`source = "sig_in"`,
`sink = "sig_out"`) and the shield + caps + `outer_boundary` as the PEC
return (3 624 nodes). `l_henry ≈ [[0.4836 nH]]`: **−2.6 %** below the
independent 2-D finite-difference reference 0.4965 nH, converging from
below on finer meshes (`tests/mesh_static_golden.rs`).

## Starting from your own layout

```sh
geode schema layout > layout.schema.json   # validate the layout's shape first
geode mesh my.layout.json --analysis capacitance --spec-out my.spec.json
geode check my.spec.json                   # DOF counts + resource estimate before solving
```
