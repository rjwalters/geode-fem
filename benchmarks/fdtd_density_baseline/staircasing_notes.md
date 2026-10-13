# Physical bent-patch geometry diagnostics (#1098)

The current JSON and PNG use the physical 2.4 GHz fixture: bend radius
100 mm, substrate thickness 1.6 mm, patch width 38 mm and ground/substrate
width 62 mm. Top/bottom radii are 100.8/99.2 mm, about the y-parallel axis at
z = −99.2 mm. Patch and ground half-angles are 0.19 and 0.31 rad.

The historical h=2 mm, R=40 mm smoke outputs are **retired from the current
artifacts**. They remain available in PR #1095, commit `7baa2e16`. The old
surface-only midpoint ladder is also retired. L0 and L1 now mean the actual
volume meshes introduced in PR #1097, each SHA-pinned in the new JSON and
checked against its independently recorded GEODE baseline.

## Reproduce

Python 3.11+, NumPy and Matplotlib are required. Set `PATCH_CONFORMAL_L1` to
an existing regenerated L1 mesh. If it is unavailable, generate it with the
fixture's pinned Gmsh 4.15.2-git environment:

```sh
export PATCH_CONFORMAL_L1=/path/to/mesh-output/patch_conformal_L1.msh
python3 reference/gmsh/generate_patch_fixture.py \
  reference/gmsh/patch_conformal_L1.yaml "$PATCH_CONFORMAL_L1"
python3 benchmarks/fdtd_density_baseline/staircasing_demo.py
python3 benchmarks/fdtd_density_baseline/staircasing_verify.py
python3 -O benchmarks/fdtd_density_baseline/staircasing_verify.py
```

The expected L1 hash is
`b811910151dea09d65e6e056add3203a6df71d90903d24e8ff0ae3401c3b7dab`;
the committed L0 hash is
`6550124b067a06d9e4dec05b86d7394771524fb04c0791d30215bb46c9fc7c56`.
No machine-specific path is stored in the outputs. The already-bent coordinates
are used directly, with radial and arc-width checks to reject an inconsistent
geometry configuration.

The committed JSON, `.sha256` sidecar and PNG were generated together, from
the same script revision, on the project's macOS dev host with Python 3.14,
NumPy 2.4.1 and Matplotlib 3.10.8. The JSON is host-independent: every
measured quantity is an exact floating-point reduction of the mesh and grid
data, and the one fitted quantity that is not
(`resonance_proxy_slope_vs_cell_size`, from `np.polyfit`) is rounded to 12
decimal places so that last-bit differences between LAPACK builds do not
change the file. The PNG is **not** host-independent: Matplotlib embeds its
version in the file and its text/antialiasing output changes between
releases, so byte-identity of `staircasing.png` is only expected with
Matplotlib 3.10.8. A differing PNG from another Matplotlib release is a
render-version difference, not data drift; the JSON and its hash are the
citation artifact.

The same scripts also run in the native arm64 `meep-matched:mpi` image
documented in [the environment manifest](../../reference/meep/docker/MATCHED.md),
whose explicit conda export fixes dependency versions (Matplotlib 3.11.2
there, so its PNG differs from the committed one while the JSON and hash do
not). The diagnostic itself does not import Meep or run a field solve. To
reproduce in that image, mount the repository at `/work` and the directory
containing L1 at `/meshes`:

```sh
docker run --rm --cpus 2 --memory 2g \
  -v "$PWD:/work" -v "$(dirname "$PATCH_CONFORMAL_L1"):/meshes:ro" \
  -e PATCH_CONFORMAL_L1=/meshes/patch_conformal_L1.msh \
  meep-matched:mpi python benchmarks/fdtd_density_baseline/staircasing_demo.py
```

Replace the final script with `staircasing_verify.py` to verify; add `-O` after
`python` to confirm that checks remain enabled. The verifier uses explicit
exceptions, not removable `assert` statements. It tests analytic chord
sagitta, doubled-order quadrature on both meshes, configurable physical tags,
invalid inputs, Cartesian displacement bounds, and byte-identical JSON
regeneration. PNG regeneration is deterministic for a fixed Matplotlib
release (3.10.8 for the committed file) and is not checked by the verifier.

All geometry inputs and physical tags can be overridden. For example, this
explicit command selects the same physical geometry and mesh levels:

```sh
python3 benchmarks/fdtd_density_baseline/staircasing_demo.py \
  --mesh L0=crates/geode-core/tests/fixtures/patch_conformal_L0.msh \
  --mesh "L1=$PATCH_CONFORMAL_L1" \
  --bend-radius 100 --thickness 1.6 --patch-width 38 --substrate-width 62 \
  --patch-tag 12 --ground-tag 13
```

Only ASCII MSH 4.1 linear surface triangles are supported. Other levels can
be supplied with additional `--mesh LEVEL=PATH` arguments. Missing L1 does
not silently substitute a synthetic refinement or overwrite the full result
with an L0-only default; pass an explicit mesh list for a smaller study.

## Current measurements

The patch profile follows the upper cylinder surface; the ground profile
follows the lower surface across the full ground width. Their arc lengths
are **38.304 mm** and **61.504 mm**. Arc-length metadata and the fitted
proxy slopes are rounded to 12 decimal places to avoid floating-point noise
in citations. The field
`top_arc_length_mm` gives the top of each slab sector (38.304/62.496 mm);
`profile_arc_length_mm` is the actual denominator of that surface's proxy.

At 1–16 cells/mm the displacement/arc-length proxy falls from 0.00868445 to
0.000493513 for the patch and from 0.00452351 to 0.000294225 for the ground.
Fitted powers of cell size are 1.0064 and 0.9850. The legacy field name
`resonance_shift_proxy_abs` is retained, but its coefficient is uncalibrated:
these are not solved frequency shifts or S11 errors.

| Actual mesh | Surface | Triangles | Maximum radial error (µm) | Area-weighted RMS (µm) |
|---|---|---:|---:|---:|
| L0 | Patch | 1042 | 4.35810 | 1.75231 |
| L0 | Ground | 3016 | 4.77409 | 1.72225 |
| L1 | Patch | 4074 | 1.12760 | 0.44069 |
| L1 | Ground | 12032 | 1.25129 | 0.43356 |

Vertices lie on their circles to about 1.4e−14 mm, but straight facets have
finite error between vertices. Maximum error is computed from the nearest
point on each projected triangle and independently checked with the angular
chord formula. RMS uses area-weighted triangle quadrature, checked again at
twice the quadrature order. These measurements describe geometric error;
they do not certify the meshes' electromagnetic convergence.

## Interpretation and paper citation

The grid origin is fixed. Profiles sample column centres, clip end-column
centres to the arc, and round to grid faces. Vertical RMS uses uniform
projected-x weighting. Grid-phase sensitivity is not studied. The occupied
area and perimeter refer to the whole slab sector under each footprint;
they are separate from the metal boundary profile.

The perimeter limit in panel (d) is the taxicab staircase paradox, recomputed
for this geometry. It cannot establish non-converging electromagnetic error
or the impossibility of FDTD inverse design. This occupancy model does not
represent Meep subpixel averaging or conformal-FDTD methods. Cartesian vertical
RMS and FEM radial RMS have different measures, so no equal-error cost ratio
is inferred from the plots.

`staircasing_results.sha256` pins the exact current JSON for the v5 paper's
citation; the current value is
`6cfc90b6065d7593a1d981cd2b7955a87191097d8b35cc5577a82237bc1a4bf2`.
Regeneration writes both together; verification checks the hash.
The generator itself and both source meshes are hashed inside the JSON.
The v5 author should use this pin and the current physical-geometry values,
not the retired smoke results. This change does not edit the paper version.
The electromagnetic convergence and cost comparison remain #1085/#1088.
