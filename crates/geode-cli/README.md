# geode-cli — the `geode` binary

Headless, JSON-in / JSON-out driver for GEODE-FEM solves (issue #673,
Phase 1). A problem spec (JSON or TOML) plus a Gmsh mesh go in; exactly
one JSON report comes out on stdout (or `-o <path>`).

```sh
cargo install --locked --path crates/geode-cli      # or: cargo build --release -p geode-cli
geode --version                                     # geode 0.3.0 (<git-sha>[-dirty])
geode check  spec.json                              # validate + DOF counts, no solve
geode driven spec.json -o report.json               # frequency sweep → Z / Y / S, L / R / Q
geode driven spec.toml --threads 8 --backend ndarray
```

| Subcommand | Phase 1 status |
|---|---|
| `check`   | live — parse + validate spec, load mesh, resolve every named physical group, report DOF counts; never solves |
| `driven`  | live — lumped-port frequency sweep with PEC + Leontovich BCs, direct LU or COCG |
| `eigen`   | reserved — parses, exits non-zero with `not_implemented` (Phase 2) |
| `extract` | reserved — parses, exits non-zero with `not_implemented` (Phase 2) |

## Host behavior

- **Exit code**: `0` on success; non-zero on any failure — bad arguments
  (clap, exit 2), invalid spec, unresolved physical group, mesh error,
  factorization failure, **iterative non-convergence**, or a non-finite
  result. Every failure after argument parsing also writes a
  `kind = "error"` report (below) to stdout / `-o` and a human-readable
  message to stderr.
- **Backend** is chosen at **build time**, mirroring `geode-core`: the
  default binary uses the `ndarray` f64 CPU backend with `faer-parallel`
  and has zero GPU dependencies. Build with `--features wgpu`, `cuda` or
  `metal` for a GPU backend. `--backend <name>` does **not** switch
  backends at runtime: it confirms the compiled-in backend and fails with
  `backend_mismatch` otherwise. (True runtime multi-backend dispatch is a
  later phase.)
- **Threads**: `--threads N` sets `GEODE_NUM_THREADS=N` (host-side
  assembly pool) and scopes faer's sparse-LU parallelism to `N`. Without
  it the library defaults apply (`GEODE_NUM_THREADS` if set, else all
  cores for assembly; serial LU).
- **No network access**: nothing in the binary's dependency tree makes
  network calls at solve time.
- **Version pinning**: `geode --version` and every report's `git_sha`
  carry the source revision baked in by `build.rs`
  (`git rev-parse --short=12 HEAD`, `-dirty` if tracked files differ;
  falls back to the `GEODE_GIT_SHA` build-time env var outside a git
  checkout, else `unknown`).

## Problem spec, schema v1

Unknown fields are rejected everywhere (a typo never silently falls back
to a default). `.toml` files are parsed as TOML, anything else as JSON —
both through the same schema. Example (the spiral-inductor golden input,
`tests/fixtures/spiral_golden_smoke.json`):

```json
{
  "schema_version": 1,
  "mesh": { "path": "../../../geode-core/tests/fixtures/spiral_3p5_smoke.msh", "length_unit_m": 1e-6 },
  "materials": [
    { "physical_group": "substrate",  "eps_r": [11.9, -0.0595] },
    { "physical_group": "dielectric", "eps_r": [4.0, -0.004] }
  ],
  "boundary_conditions": {
    "pec": ["outer_boundary"],
    "leontovich": [{ "physical_group": "conductor_surface", "conductivity_s_m": 5.8e7 }]
  },
  "ports": [{ "physical_group": "port", "e_hat": [0, 1, 0], "resistance_ohm": 50 }],
  "frequencies": { "unit": "ghz", "values": [1, 5, 10, 20] },
  "solver": { "mode": "direct" }
}
```

| Field | Type / units | Meaning |
|---|---|---|
| `schema_version` | int, must be `1` | spec schema version |
| `mesh.path` | path | Gmsh **MSH 4.1 ASCII**, Tet4 volume mesh with `$PhysicalNames`. Relative paths resolve against the spec file's directory |
| `mesh.length_unit_m` | float > 0, **required** | metres per mesh length unit (`1e-6` = micron mesh). Every SI ↔ natural-unit conversion depends on it |
| `materials[]` | | per-volume-region scalar permittivity; unlisted regions are vacuum (`check` reports which) |
| `materials[].physical_group` | string | name of a **dimension-3** physical group |
| `materials[].eps_r` | `[re, im]`, `im ≤ 0` | complex relative permittivity, `exp(+jωt)` convention: `ε_r = ε'(1 − j·tan δ)`. `im > 0` (gain) is rejected |
| `boundary_conditions.pec[]` | strings | **dimension-2** groups whose edges are eliminated (tangential E = 0). Unnamed surfaces are natural (PMC-like) boundaries |
| `boundary_conditions.leontovich[]` | | good-conductor surface impedance `Z_s = (1+j)·√(ωμ₀/2σ)` |
| `….leontovich[].physical_group` | string | dimension-2 group |
| `….leontovich[].conductivity_s_m` | float > 0, S/m | converted to natural units `σ·η₀·length_unit_m` |
| `ports[]` | ≥ 1 entry | uniform (Palace-style) lumped ports; index order = matrix order |
| `ports[].physical_group` | string | dimension-2 group holding the port faces |
| `ports[].e_hat` | `[x, y, z]`, non-zero | gap direction (normalized on load) |
| `ports[].resistance_ohm` | float > 0, Ω | termination **and** S-parameter reference impedance |
| `ports[].width` | optional, mesh units | extent across `ê`; default `area / length` of the tagged faces |
| `ports[].length` | optional, mesh units | gap extent along `ê`; default: extent of the tagged faces along `ê` |
| `ports[].v_inc` | `[re, im]`, default `[1, 0]` | incident drive voltage (non-zero) |
| `frequencies.unit` | `"hz"` \| `"ghz"` \| `"k0"`, **required** | `k0` = the solver's natural unit `ω/c` in **rad per mesh length unit** |
| `frequencies.values` | floats > 0 | explicit list, **or** … |
| `frequencies.start` / `stop` / `count` | floats > 0, int ≥ 1 | … an inclusive sweep |
| `frequencies.spacing` | `"linear"` (default) \| `"log"` | sweep spacing |
| `solver.mode` | `"direct"` (default) \| `"iterative"` | sparse LU per frequency, or COCG with a Jacobi preconditioner built once per frequency |
| `solver.tol` / `solver.max_iters` | iterative only; default `1e-10` / `5000` | relative-residual tolerance / per-RHS budget; exceeding the budget is a hard error (`solve_failed`, non-zero exit). Jacobi-preconditioned COCG can stall on ill-conditioned low-frequency conductor problems (e.g. the spiral fixture); prefer `direct` there |

Not in schema v1 (Phase 2+): wave ports, UPML / Silver-Müller absorbing
regions, tensor materials, the matrix-free iterative solver, field /
NTFF export.

## Report, schema v1

Every report carries these top-level provenance fields:

| Field | Meaning |
|---|---|
| `schema_version` | report schema version (`1`) — bumped only on breaking layout changes |
| `geode_version` | `geode-cli` crate version |
| `git_sha` | source revision (`<sha>[-dirty]` or `unknown`) |
| `backend` | compiled-in backend: `ndarray` \| `wgpu` \| `cuda` \| `metal` |
| `threads` | `--threads` value, or `null` |
| `spec_path` | spec path as given |
| `kind` | `"check"` \| `"driven"` \| `"error"` |
| `status` | `"ok"` \| `"error"` |

Complex numbers are `[re, im]`; matrices are row-major nested arrays
`m[row][col]` indexed by port.

**`mesh`** (check + driven): `path`, `sha256` (hex SHA-256 of the mesh
bytes), `length_unit_m`, `n_nodes`, `n_tets`, `n_edges` (Nédélec DOFs),
`n_interior` (DOFs kept after PEC elimination = linear-system size).

**`ports[]`** (check + driven): `index`, `physical_group`, `tag`,
`n_triangles`, `e_hat` (normalized), `width`, `length` (mesh units),
`geometry_derived`, `resistance_ohm`, `v_inc`.

**`kind = "check"`** adds `regions[]` (`physical_group`, `tag`, `n_tets`,
`eps_r`, `eps_r_source` = `"spec"` \| `"default_vacuum"`), `pec[]`
(`physical_group`, `tag`, `n_triangles`), `leontovich[]` (… plus
`conductivity_s_m`, `conductivity_natural`), `frequencies[]`
(`frequency_hz`, `k0`) and `solver` (`mode`, `tol`, `max_iters`).

**`kind = "driven"`** adds:

- `solver`: `mode`, `tol`, `max_iters`, `iterations_max` (largest per-RHS
  Krylov count; `0` on the direct path), `residual_rel_max` (largest
  `‖Ax − b‖/‖b‖`), `wall_time_s` (assembly + all solves, seconds).
- `results[]`, one per frequency in spec order:

| Field | Units | Meaning |
|---|---|---|
| `frequency_hz` | Hz | |
| `k0` | rad / mesh unit | solver ω |
| `omega_rad_s` | rad/s | `2πf` |
| `residual_rel` | – | worst per-RHS relative residual at this frequency |
| `iterations` | – | Krylov iterations per RHS (one per port; `0` direct) |
| `z_ohm` | Ω | impedance matrix `Z` (every port excited in turn, others terminated in their own `R`) |
| `y_s` | S | `Y = Z⁻¹`, `null` if `Z` is singular |
| `s` | – | scattering matrix vs the per-port `resistance_ohm` |
| `ports[].z_ohm` / `s` | Ω / – | diagonal entries `Z_kk`, `S_kk` |
| `ports[].s_db` | dB | `20·log10|S_kk|` |
| `ports[].r_ohm` | Ω | `Re Z_kk` |
| `ports[].l_h` | H | `Im Z_kk / ω` (negative above self-resonance) |
| `ports[].q` | – | `Im Z_kk / Re Z_kk` |

**`kind = "error"`** adds `command` (the subcommand) and
`error: { code, message }` with `code` one of `io`, `spec_parse`,
`schema_version`, `invalid_spec`, `mesh`, `unresolved_physical_group`,
`backend_mismatch`, `solve_failed`, `non_finite`, `not_implemented`,
`serialize`.

## Golden test

`tests/spiral_golden.rs` re-expresses the spiral-inductor benchmark
(issue #211) as a spec and runs it through the real binary:

```sh
cargo test -p geode-cli                                                   # smoke mesh, default CI
cargo test -p geode-cli --release --test spiral_golden -- --ignored      # 54k-edge benchmark mesh
```

The smoke tier pins the committed `benchmarks/spiral_inductor/results_smoke.toml`
sweep (L 1 %, R / Q 2 %, |S11| 0.01) and checks that the CLI's generic
named-group path reproduces an in-process library solve on the bundled
`SpiralFixture` to 1e-9 relative in `Z`. The ignored benchmark tier pins
the 1 GHz inductance to the committed `results.toml` (1 %), holds it to
the issue-#211 oracle bands (Mohan current-sheet 10 %, projected mom-PEEC
mean 12 %), and checks library parity. R and Q on the benchmark mesh are
covered by parity only, because the committed values are stale (#674).
