# geode-cli — the `geode` binary

Headless, JSON-in / JSON-out driver for GEODE-FEM solves (issue #673;
Phase 2 tracked in Epic #680). A problem spec (JSON or TOML) plus a Gmsh mesh go in; exactly
one JSON report comes out on stdout (or `-o <path>`).

```sh
cargo install --locked --path crates/geode-cli      # or: cargo build --release -p geode-cli
geode --version                                     # geode 0.3.0 (<git-sha>[-dirty])
geode check  spec.json                              # validate + DOF counts, no solve
geode driven spec.json -o report.json               # frequency sweep → Z / Y / S, L / R / Q
geode driven spec.toml --threads 8 --backend ndarray
geode eigen  cavity.json -o modes.json              # lossless PEC-cavity modes → f
geode extract inductor.json -o l0.json              # sweep → L / R / Q, f→0 L₀, SRF
```

| Subcommand | Status |
|---|---|
| `check`   | live — parse + validate spec, load mesh, resolve every named physical group, report DOF counts; never solves |
| `driven`  | live — lumped-port frequency sweep with PEC + Leontovich BCs, direct LU or COCG |
| `eigen`   | live (#681) — lossless PEC-cavity eigenmodes near a shift frequency, sparse shift-invert Lanczos; `Q` is `null` (lossless) |
| `extract` | live (#682) — the `driven` sweep post-processed per port into L / R / Q, the quasi-static `L₀` (f → 0 Richardson extrapolation, with a consistency error estimate and an optional convergence gate) and the SRF |

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
both through the same schema.

A spec describes **one** analysis, decided by which optional analysis
section it carries:

- **driven spec** (no `eigen`, no `extract`): needs `ports` (≥ 1) and
  `frequencies`; run with `geode driven`.
- **eigen spec** (has `eigen`): must not have `ports`, `frequencies` or
  Leontovich walls, and every `eps_r` must be real (`im = 0`); run with
  `geode eigen`.
- **extract spec** (has `extract`): exactly a driven spec plus the
  `extract` section (`"extract": {}` takes every default) — needs
  `ports` (≥ 1), `frequencies`, and ≥ 2 distinct `L₀` anchor
  frequencies; run with `geode extract`.

A spec with both `eigen` and `extract` is rejected. Running a spec under
another subcommand fails with `invalid_spec` before the mesh is read;
`geode check` validates any kind and reports which (`analysis`).

Driven example (the spiral-inductor golden input,
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
| `materials[].eps_r` | `[re, im]`, `im ≤ 0` (eigen: `im = 0`, `re > 0`) | complex relative permittivity, `exp(+jωt)` convention: `ε_r = ε'(1 − j·tan δ)`. `im > 0` (gain) is rejected |
| `boundary_conditions.pec[]` | strings | **dimension-2** groups whose edges are eliminated (tangential E = 0). Unnamed surfaces are natural (PMC-like) boundaries |
| `boundary_conditions.leontovich[]` | | good-conductor surface impedance `Z_s = (1+j)·√(ωμ₀/2σ)` |
| `….leontovich[].physical_group` | string | dimension-2 group |
| `….leontovich[].conductivity_s_m` | float > 0, S/m | converted to natural units `σ·η₀·length_unit_m` |
| `ports[]` | driven: ≥ 1 entry; eigen: absent | uniform (Palace-style) lumped ports; index order = matrix order |
| `ports[].physical_group` | string | dimension-2 group holding the port faces |
| `ports[].e_hat` | `[x, y, z]`, non-zero | gap direction (normalized on load) |
| `ports[].resistance_ohm` | float > 0, Ω | termination **and** S-parameter reference impedance |
| `ports[].width` | optional, mesh units | extent across `ê`; default `area / length` of the tagged faces |
| `ports[].length` | optional, mesh units | gap extent along `ê`; default: extent of the tagged faces along `ê` |
| `ports[].v_inc` | `[re, im]`, default `[1, 0]` | incident drive voltage (non-zero) |
| `frequencies` | driven: **required**; eigen: absent | the frequencies to sweep |
| `frequencies.unit` | `"hz"` \| `"ghz"` \| `"k0"`, **required** | `k0` = the solver's natural unit `ω/c` in **rad per mesh length unit** |
| `frequencies.values` | floats > 0 | explicit list, **or** … |
| `frequencies.start` / `stop` / `count` | floats > 0, int ≥ 1 | … an inclusive sweep |
| `frequencies.spacing` | `"linear"` (default) \| `"log"` | sweep spacing |
| `solver.mode` | `"direct"` (default) \| `"iterative"` | sparse LU per frequency, or COCG with a Jacobi preconditioner built once per frequency |
| `solver.tol` / `solver.max_iters` | iterative only; default `1e-10` / `5000` | relative-residual tolerance / per-RHS budget; exceeding the budget is a hard error (`solve_failed`, non-zero exit). Jacobi-preconditioned COCG can stall on ill-conditioned low-frequency conductor problems (e.g. the spiral fixture); prefer `direct` there |
| `solver.mode` (eigen) | `"direct"` only | the eigen path always factors `K − σM` once with sparse LU; `"iterative"` is rejected |
| `extract` | optional section | its presence makes this an extract spec (see below) |
| `extract.anchor_frequencies` | optional frequency block (same shape as `frequencies`) | explicit `L₀` anchor ladder, solved **in addition to** `frequencies`. Omitted: the anchors are `frequencies` itself |
| `extract.l0_rel_tol` | optional float > 0 | convergence gate: fail (`solve_failed`) if any port's relative `L₀` consistency estimate exceeds it. Needs ≥ 3 distinct anchors. Omitted: no gate (the estimate is still reported) |
| `eigen` | optional section | its presence makes this an eigen spec |
| `eigen.n_modes` | int ≥ 1 | physical modes to return |
| `eigen.unit` | `"hz"` \| `"ghz"` \| `"k0"`, **required** | unit of `shift` |
| `eigen.shift` | float > 0 | shift / target frequency: the modes **closest** to it are returned, ascending. Place it just **below** the lowest mode of interest (see below) |
| `eigen.max_iters` | int ≥ 1, default `160` | Lanczos basis size; raise it for many modes or tight near-degenerate multiplets. Too small a basis leaves modes unconverged, which fails the run (see `eigen.residual_tol`) |
| `eigen.tol` | float > 0, default `1e-9` | Lanczos relative convergence tolerance |
| `eigen.residual_tol` | float > 0, default `1e-6` | per-mode acceptance bound on the relative eigen-residual `‖Kx − λMx‖ / (\|λ\| ‖Mx‖)`; if any returned mode exceeds it the run fails with `solve_failed` (non-zero exit) |

Eigen example (the sphere-cavity golden input,
`tests/fixtures/sphere_pec_golden.json` — a dielectric sphere,
`n = 1.5`, inside a PEC spherical wall):

```json
{
  "schema_version": 1,
  "mesh": { "path": "../../../geode-core/tests/fixtures/sphere.msh", "length_unit_m": 0.01 },
  "materials": [{ "physical_group": "sphere_interior", "eps_r": [2.25, 0.0] }],
  "boundary_conditions": { "pec": ["outer_boundary"] },
  "eigen": { "n_modes": 5, "unit": "k0", "shift": 1.0 }
}
```

**What `geode eigen` solves.** The lossless first-order Nédélec pencil
`K x = k₀² M_ε x` (curl-curl stiffness, `ε_r`-weighted mass, `μ_r = 1`)
on the edges left after PEC elimination, assembled sparse and solved by
the pure-Rust sparse shift-invert Lanczos
(`geode_core::eigen::pec_cavity`) around `σ = k₀,shift²`. Surfaces not
named PEC are natural (PMC-like) boundaries, as for `driven`.

**Shift placement.** `K` has a large nullspace (the discrete gradients,
`k₀ = 0`), and Lanczos converges the Ritz values closest to the shift
first. `shift > 0` is required (a zero shift makes `K − σM` singular);
Ritz values with `k₀² ≤ 10⁻³ · σ` are classified as gradient nullspace
and dropped (`solver.n_null_filtered`). Put `shift` just below the
lowest mode you want — with a shift far below it the basis is spent on
the nullspace, with a shift far above it you get the modes around the
shift instead. If fewer than `n_modes` physical modes are resolved the
run fails with `solve_failed` (never a silently short list); raise
`max_iters` or move `shift`.

**Convergence gate.** Lanczos returns whatever Ritz pairs its basis
yields, converged or not, so every returned mode's relative residual
`‖Kx − λMx‖ / (|λ| ‖Mx‖)` is checked against `eigen.residual_tol`
(default `10⁻⁶`). If any mode exceeds it the run fails with
`solve_failed` and a non-zero exit, naming the worst mode, its `λ` and
residual — never a silently wrong frequency list. Fix it by raising
`max_iters` or moving `shift`. A converged run reports the actual
residuals in `modes[].residual_rel` and `solver.residual_rel_max`
(the sphere golden case sits near `10⁻¹³`). The solver backend is fixed (pure-Rust
Lanczos, no ARPACK knob) — a backend choice is a possible future
additive field.

Extract example (the SLCFET 3HP golden input,
`tests/fixtures/slcfet_extract_smoke.json` — anchors given in Hz next to
a GHz sweep):

```json
{
  "schema_version": 1,
  "mesh": { "path": "../../../geode-core/tests/fixtures/spiral_slcfet_3hp_smoke.msh", "length_unit_m": 1e-6 },
  "materials": [
    { "physical_group": "substrate",  "eps_r": [9.7, -0.0388] },
    { "physical_group": "dielectric", "eps_r": [1.0, 0.0] }
  ],
  "boundary_conditions": {
    "pec": ["outer_boundary"],
    "leontovich": [{ "physical_group": "conductor_surface", "conductivity_s_m": 51466803.9114771 }]
  },
  "ports": [{ "physical_group": "port", "e_hat": [0, 1, 0], "resistance_ohm": 50 }],
  "frequencies": { "unit": "ghz", "values": [10] },
  "solver": { "mode": "direct" },
  "extract": { "anchor_frequencies": { "unit": "hz", "values": [1e8, 2e8, 5e8] } }
}
```

**What `geode extract` computes.** The solve is exactly `geode
driven`'s, over the **solved list** = ascending union of `frequencies`
and `extract.anchor_frequencies` (points equal to 1e-12 relative, e.g.
the same frequency in two units, are solved once). Per port `k`, from the
diagonal `Z_kk` at each anchor, `L(f) = Im Z_kk / ω`; then

- **`L₀`** — the quasi-static inductance `lim_{f→0} L(f)` by two-point
  Richardson extrapolation of `L(f) ≈ L₀ − a·f²` on the **two lowest**
  anchors: `L₀ = (L₁f₂² − L₂f₁²) / (f₂² − f₁²)`
  (`geode_core::driven::extraction::extrapolate_l0`). Below self-resonance
  the shunt substrate capacitance siphons current as `f` rises, so `L(f)`
  falls ~quadratically; `L₀` is the capacitance-free inductance that
  quasi-static oracles (PEEC, Mohan) report. Put the two lowest anchors
  well below the SRF (the SLCFET benchmark uses 0.1 / 0.2 GHz).
- **Error estimate** — with a third anchor, the same extrapolation on the
  2nd / 3rd-lowest pair; `|L₀(f₁,f₂) − L₀(f₂,f₃)|` is reported absolute
  and relative. It vanishes when the anchors sit in the asymptotic
  `L₀ − a·f²` regime and grows otherwise. With `extract.l0_rel_tol` set,
  an estimate above it fails the run with `solve_failed` (non-zero exit),
  naming the port, anchors and estimate — never a silently unconverged
  `L₀`.
- **SRF** — every `Im Z_kk` sign change across the whole solved list,
  linearly interpolated; the first one is `srf_hz` (`null` if the sweep
  brackets none). A sign change is a series resonance or a flip through a
  pole (parallel anti-resonance), as in the SLCFET benchmark.

Not in schema v1 (Phase 2+): wave ports, UPML / Silver-Müller absorbing
regions, tensor materials, the matrix-free iterative solver, field /
NTFF export, lossy / open-cavity eigenmodes (complex `ε_r`, Leontovich,
absorbing boundaries → finite `Q`; issue #683).

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
| `kind` | `"check"` \| `"driven"` \| `"eigen"` \| `"extract"` \| `"error"` |
| `status` | `"ok"` \| `"error"` |

Complex numbers are `[re, im]`; matrices are row-major nested arrays
`m[row][col]` indexed by port.

**`mesh`** (every success kind): `path`, `sha256` (hex SHA-256 of the mesh
bytes), `length_unit_m`, `n_nodes`, `n_tets`, `n_edges` (Nédélec DOFs),
`n_interior` (DOFs kept after PEC elimination = linear-system size).

**`ports[]`** (check + driven + extract): `index`, `physical_group`, `tag`,
`n_triangles`, `e_hat` (normalized), `width`, `length` (mesh units),
`geometry_derived`, `resistance_ohm`, `v_inc`.

**`kind = "check"`** adds `regions[]` (`physical_group`, `tag`, `n_tets`,
`eps_r`, `eps_r_source` = `"spec"` \| `"default_vacuum"`), `pec[]`
(`physical_group`, `tag`, `n_triangles`), `leontovich[]` (… plus
`conductivity_s_m`, `conductivity_natural`), `frequencies[]`
(`frequency_hz`, `k0`; empty for an eigen spec), `solver` (`mode`, `tol`,
`max_iters`), `analysis` (`"driven"` \| `"eigen"` \| `"extract"`),
`eigen` (`null` unless an eigen spec, else `n_modes`, `shift_hz`,
`shift_k0`, `sigma` = `shift_k0²`, `max_iters`, `tol`, `residual_tol`)
and `extract` (`null` unless an extract spec, else `anchor_source` =
`"frequencies"` \| `"anchor_frequencies"`, `anchor_frequencies[]`
(`frequency_hz`, `k0`; distinct, ascending) and `l0_rel_tol`).
`ports[]` is empty for an eigen spec; for an extract spec
`frequencies[]` is the ascending solved list.

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

**`kind = "eigen"`** adds `regions[]` and `pec[]` (as for `check`),
`eigen` (the resolved settings, as for `check`), and:

- `solver`: `method` (`"shift_invert_lanczos"`), `inner` (`"direct_lu"`),
  `n_null_filtered` (Ritz values dropped as gradient nullspace),
  `residual_rel_max`, `wall_time_s` (assembly + eigensolve, seconds).
- `modes[]`, ascending in frequency:

| Field | Units | Meaning |
|---|---|---|
| `index` | – | mode index |
| `lambda` | (rad / mesh unit)² | eigenvalue `k₀²` |
| `k0` | rad / mesh unit | resonant `ω/c` |
| `frequency_hz` | Hz | `k₀ c / (2π · length_unit_m)` |
| `omega_rad_s` | rad/s | `2πf` |
| `q` | – | quality factor — **always `null`** in this build: the pencil is lossless, so `Q` is undefined (infinite), not a number. Finite `Q` needs lossy / open-cavity eigenmodes (issue #683) |
| `residual_rel` | – | `‖Kx − λMx‖ / (|λ| ‖Mx‖)` |

**`kind = "extract"`** has the `kind = "driven"` fields (`mesh`,
`ports`, `solver`, `results[]` — but `results[]` is **ascending** in
frequency over the solved list, not spec order), plus `extract` (the
resolved settings, as for `check`) and `extraction[]`, one per port:

| Field | Units | Meaning |
|---|---|---|
| `index` | – | port index |
| `l0_h` | H | quasi-static `L₀`, two-point Richardson on the two lowest anchors |
| `l0_anchor_frequencies_hz` | Hz | `[f₁, f₂]`, the two anchors used |
| `l0_error_estimate_h` | H | `\|L₀(f₁,f₂) − L₀(f₂,f₃)\|`; `null` with two anchors |
| `l0_error_estimate_rel` | – | `l0_error_estimate_h / \|l0_h\|`; `null` with two anchors |
| `l0_check_frequency_hz` | Hz | `f₃`, the third anchor behind the estimate; `null` with two anchors |
| `im_z_zero_crossings_hz` | Hz | every `Im Z_kk` sign change over the solved list, ascending |
| `srf_hz` | Hz | first entry of `im_z_zero_crossings_hz`, or `null` |

**`kind = "error"`** adds `command` (the subcommand) and
`error: { code, message }` with `code` one of `io`, `spec_parse`,
`schema_version`, `invalid_spec`, `mesh`, `unresolved_physical_group`,
`backend_mismatch`, `solve_failed`, `non_finite`, `serialize`
(`not_implemented` was retired once `extract` went live).

## Golden tests

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

`tests/sphere_pec_golden.rs` (issue #681) re-expresses the PEC-walled
dielectric-sphere cavity benchmark (`geode-core/tests/sphere_pec_eigenmode.rs`,
issues #26/#69) as an eigen spec and runs `geode eigen`:

```sh
cargo test -p geode-cli --test sphere_pec_golden                                  # default CI tier
cargo test -p geode-cli --release --test sphere_pec_golden -- --ignored           # dense oracle
```

The default tier holds each of the lowest 5 modes to the benchmark's
existing bound — within 15 % of the closest analytic PEC-cavity Mie root
(`geode_core::analytic::mie::merged_roots`) — plus the report contract
(ascending, `q = null`, `f = k₀c/(2πL)`, residuals). The ignored tier
checks the sparse Lanczos modes against the full dense solve of the same
pencil to 1e-6 relative. This is deliberately not the
`examples/mie_sphere` UPML benchmark, which needs UPML-as-material and
complex quasimode eigensolvers (issue #683).

`tests/slcfet_extract_golden.rs` (issue #682) re-expresses the SLCFET 3HP
spiral benchmark (`geode-core/tests/slcfet_3hp_benchmark.rs`, issue #212
— the only fixture with a validated f → 0 `L₀` oracle chain) as an
extract spec and runs `geode extract`:

```sh
cargo test -p geode-cli --test slcfet_extract_golden                              # smoke mesh, default CI
cargo test -p geode-cli --release --test slcfet_extract_golden -- --ignored       # 77k-edge benchmark mesh
```

The smoke mesh is a different geometry (`d_in` = 60 µm vs 100 µm), so
its tier holds `l0_h` to **library parity** (an in-process
`driven_frequency_sweep` + `extrapolate_l0` on the bundled fixture, 1e-9
relative, plus `Z` at the anchors) and loose physical sanity. The
ignored benchmark tier holds `L₀` to the existing bands — 1 % of the
committed `benchmarks/slcfet_3hp/results.toml` `L₀`, 5 % of the mom-PEEC
oracle (2.155 nH), 10 % of Mohan — and the SRF to the calibrated
28–38 GHz band.
