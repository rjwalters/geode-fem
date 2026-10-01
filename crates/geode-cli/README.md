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
geode eigen  cavity.json -o modes.json              # PEC-cavity modes → f (and Q if lossy / open)
geode extract inductor.json -o l0.json              # sweep → L / R / Q, f→0 L₀, SRF
geode capacitance caps.json -o c.json               # static Maxwell capacitance matrix (F)
geode inductance coax.json -o l.json                # static Maxwell inductance matrix (H)
geode driven patch.json --outdir fields/            # + per-frequency E-field .vtu and NTFF
geode driven filter.json --touchstone filter.s2p    # + Touchstone 2.0 S-parameters
geode capacitance caps.json --spice caps.sp         # + SPICE .subckt of the mutual C network
geode inductance triax.toml --spice l.sp            # + SPICE .subckt of self L + K couplings
geode mesh layout.json --mesh-out m.msh --spec-out s.json && geode driven s.json   # layout → mesh → solve
geode mesh layout.json --analysis capacitance --spec-out c.json && geode capacitance c.json   # layout → C matrix
geode schema spec                                   # JSON Schema (draft 2020-12) of the spec; also `report`, `layout`
```

New to `geode`? Start with the runnable [examples cookbook](examples/README.md):
one worked example per analysis, with the output to expect.

| Subcommand | Status |
|---|---|
| `check`   | live — parse + validate spec, load mesh, resolve every named physical group, report DOF counts and an order-of-magnitude memory / cost estimate (#703); never solves |
| `driven`  | live — lumped-port (or wave-port, #683) frequency sweep with PEC / Leontovich / Silver-Müller BCs and matched box-UPML absorbing regions (#683), direct LU or COCG; optional Touchstone 2.0 `.sNp` (#703) |
| `eigen`   | live (#681) — PEC-cavity eigenmodes near a shift frequency, sparse shift-invert Lanczos; lossless (`Q` is `null`), or lossy / open (#706: complex `eps_r` and / or box-UPML `absorbing_regions` → complex `k₀`, finite `Q`) |
| `extract` | live (#682) — the `driven` sweep post-processed per port into L / R / Q, the quasi-static `L₀` (f → 0 Richardson extrapolation, with a consistency error estimate and an optional convergence gate) and the SRF |
| `capacitance` | live (#705) — static **Maxwell capacitance matrix** (farads) between named conductor surfaces, from one electrostatic solve per terminal (P1 scalar, direct LU); non-driven conductors are **grounded**, never floating (see [Capacitance](#static-capacitance-geode-capacitance-issue-705)); optional SPICE `.subckt` (#715) |
| `inductance` | live (#714) — static **Maxwell inductance matrix** (henries) between named open current paths (conductor volume + source / sink faces returning through one connected PEC conductor), from a P1 conduction solve per path for the current density and one tree-cotree-gauged magnetostatic solve per path (Nédélec edges, direct LU); **not** `extract`'s RF `l0_h` (see [Inductance](#static-inductance-geode-inductance-issue-714)); optional SPICE `.subckt` (#719) |
| `mesh`    | live (#704) — layout (2-D rectilinear polygons + layer stack, JSON/TOML) → tagged Gmsh MSH 4.1 mesh with automatically named physical groups + a starter problem spec (`--analysis driven`, `capacitance` or `inductance`, #720), via the external `gmsh` binary (see [Layout → mesh](#layout--mesh-geode-mesh-issue-704)) |
| `schema`  | live (#709) — print the JSON Schema (draft 2020-12) of the problem spec, the report or the layout (`geode schema spec\|report\|layout [-o PATH]`), derived from the same serde types the binary parses and emits; committed under [`schemas/`](schemas/) (see [JSON Schema](#json-schema-geode-schema-issue-709)) |

## Install

All three routes produce the same default build: the `ndarray` f64 CPU
backend with `faer-parallel`, no GPU and no `arpack`, so the binary has no
native runtime dependencies beyond the platform C library.

**Prebuilt binaries** (issue #709). Each published
[GitHub Release](https://github.com/rjwalters/geode-fem/releases) carries
`geode-<version>-<target>.tar.gz` (the `geode` binary + `LICENSE` + this
README) and a matching `.sha256`, built by `.github/workflows/release.yml` (releases that predate the
workflow have assets only if backfilled via its `workflow_dispatch`
trigger with `upload: true`):

| Target | Built on | Runs on |
|---|---|---|
| `x86_64-unknown-linux-gnu`  | `ubuntu-22.04`     | Linux x86-64, glibc ≥ 2.35 (Ubuntu 22.04+, Debian 12+, RHEL 10+) |
| `aarch64-unknown-linux-gnu` | `ubuntu-22.04-arm` | Linux arm64, glibc ≥ 2.35 |
| `aarch64-apple-darwin`      | `macos-14`         | macOS 11+ on Apple silicon |

```sh
V=0.6.0; T=x86_64-unknown-linux-gnu   # pick your target
curl -LO https://github.com/rjwalters/geode-fem/releases/download/v$V/geode-$V-$T.tar.gz
curl -LO https://github.com/rjwalters/geode-fem/releases/download/v$V/geode-$V-$T.tar.gz.sha256
sha256sum -c geode-$V-$T.tar.gz.sha256          # macOS: shasum -a 256 -c …
tar -xzf geode-$V-$T.tar.gz && install geode-$V-$T/geode ~/.local/bin/
geode --version                                 # geode <version> (<tag's commit sha>)
```

Older glibc (e.g. RHEL/Rocky 9, Ubuntu 20.04) is not supported by the
prebuilt Linux binaries; build from source or use the container. macOS
may quarantine a browser-downloaded binary (`xattr -d com.apple.quarantine geode`);
`curl` downloads are not quarantined.

**From source** (any platform with a Rust toolchain ≥ the workspace
`rust-version`), pinned to a release tag:

```sh
cargo install --locked --git https://github.com/rjwalters/geode-fem --rev v0.6.0 geode-cli
```

**Container** with Gmsh preinstalled: build
[`docker/geode-cli/Dockerfile`](../../docker/geode-cli/README.md) from the
repository root and mount your working directory at `/work`
(`docker run --rm -v "$PWD:/work" geode-cli check spec.json`). The image is
not published to a registry yet; build it locally.

**Gmsh is an external runtime dependency of `geode mesh` only** and is not
bundled in the binaries or the `cargo install` build (the container is the
exception). Every other subcommand needs nothing but `geode`. `geode mesh`
needs Gmsh ≥ 4.11 with OpenCASCADE on `PATH` (or `--gmsh` /
`$GEODE_GMSH`): `brew install gmsh` on macOS, `apt-get install gmsh` on
Ubuntu 24.04+ / Debian 13+, or the [upstream builds](https://gmsh.info/#Download)
elsewhere — Ubuntu 22.04 and Debian 12 package Gmsh 4.8, which is too old.

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
- **Files written**: only the report (stdout or `-o`) — unless
  `--outdir` (field export) or `--touchstone` (`.sNp`) is given (next
  sections), or for `geode mesh`, which writes the mesh, its `.geo`
  script and (with `--spec-out`) the starter spec.
- **Version pinning**: `geode --version` and every report's `git_sha`
  carry the source revision baked in by `build.rs`
  (`git rev-parse --short=12 HEAD`, `-dirty` if tracked files differ;
  falls back to the `GEODE_GIT_SHA` build-time env var outside a git
  checkout, else `unknown`).

## Problem spec, schema v1

Unknown fields are rejected everywhere (a typo never silently falls back
to a default). `.toml` files are parsed as TOML, anything else as JSON —
both through the same schema. The machine-readable form is
[`schemas/spec.schema.json`](schemas/spec.schema.json) (`geode schema spec`;
see [JSON Schema](#json-schema-geode-schema-issue-709)).

A spec describes **one** analysis, decided by which optional analysis
section it carries:

- **driven spec** (no `eigen`, no `extract`): needs `ports` (≥ 1) **or**
  `wave_ports` (≥ 1), and `frequencies`; run with `geode driven`.
- **eigen spec** (has `eigen`): must not have `ports`, `wave_ports`,
  `frequencies`, or Leontovich / Silver-Müller walls (their
  frequency-dependent terms would make the eigenproblem nonlinear, see
  [lossy / open eigen](#lossy--open-cavity-eigenmodes-issue-706)); every
  `eps_r` needs `re > 0`, and lossy `im < 0` plus `absorbing_regions` are
  allowed (issue #706); run with `geode eigen`.
- **extract spec** (has `extract`): exactly a driven spec plus the
  `extract` section (`"extract": {}` takes every default) — needs lumped
  `ports` (≥ 1; wave ports define no `Z_kk`, so they are rejected),
  `frequencies`, and ≥ 2 distinct `L₀` anchor frequencies; run with
  `geode extract`.
- **capacitance spec** (has `capacitance`, issue #705): conductor
  terminals + ground surfaces; must not have `ports`, `wave_ports`,
  `frequencies`, `absorbing_regions`, any `boundary_conditions` (PEC,
  Leontovich, Silver-Müller) or `solver.mode = "iterative"`, and every
  `eps_r` must be real (`im = 0`, `re > 0`); run with `geode capacitance`.
- **inductance spec** (has `inductance`, issue #714): open current paths
  (conductor volume + source / sink faces) and a **required**
  `boundary_conditions.pec` wall; must not have `ports`, `wave_ports`,
  `frequencies`, `absorbing_regions`, Leontovich / Silver-Müller walls,
  `solver.mode = "iterative"` or a non-vacuum `eps_r` (irrelevant to
  magnetostatics); materials carry `mu_r`; run with `geode inductance`.

Each **dimension-2** physical group carries at most one role — lumped
port, wave port, PEC, Leontovich, Silver-Müller, capacitance terminal,
capacitance ground, inductance source or inductance sink; ports, wave
ports, impedance walls, terminals and ground surfaces may not be listed
twice (repeating a PEC name is harmless).

A spec with more than one of `eigen` / `extract` / `capacitance` /
`inductance` is rejected. Running a spec under
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
| `materials[].eps_r` | `[re, im]`, `im ≤ 0` (eigen: `re > 0`; capacitance: `im = 0`, `re > 0`; inductance: omit or `[1, 0]`); default `[1, 0]` (vacuum, for every analysis: a `materials` entry may omit `eps_r`) | complex relative permittivity, `exp(+jωt)` convention: `ε_r = ε'(1 − j·tan δ)`. `im > 0` (gain) is rejected |
| `materials[].mu_r` | float > 0, default `1` (additive in v1, issue #714) | real relative permeability. Honoured **only by `geode inductance`**; every other analysis rejects `mu_r ≠ 1` (its solver has no permeability term) |
| `boundary_conditions.pec[]` | strings | **dimension-2** groups whose edges are eliminated (tangential E = 0). Unnamed surfaces are natural (PMC-like) boundaries |
| `boundary_conditions.leontovich[]` | | good-conductor surface impedance `Z_s = (1+j)·√(ωμ₀/2σ)` |
| `….leontovich[].physical_group` | string | dimension-2 group |
| `….leontovich[].conductivity_s_m` | float > 0, S/m | converted to natural units `σ·η₀·length_unit_m` |
| `boundary_conditions.silver_muller[]` | strings, driven / extract only | **dimension-2** groups carrying the first-order Silver-Müller absorbing condition — the impedance wall with `Z_s = η₀` (no parameters). Composes with Leontovich walls, PEC and `absorbing_regions` |
| `absorbing_regions[]` | driven / extract / eigen | matched (full Sacks) **box UPML** shells, see below (in an eigen spec the stretch is frozen at `eigen.shift`) |
| `absorbing_regions[].physical_group` | string | **dimension-3** group of the shell tets; its `materials` `eps_r` (vacuum if unlisted) is the base permittivity the stretch multiplies |
| `absorbing_regions[].thickness` | float > 0, mesh units | shell depth; the inner wall is the mesh node bounding box shrunk by it on every face |
| `absorbing_regions[].sigma_0` | float > 0, **natural units** (rad / mesh unit, like `k0`) | strength of the quadratic profile `s_i = 1 − j·σ₀·(d_i/thickness)²/k₀`; `25` is the validated value of the patch-antenna / Mie shells |
| `ports[]` | driven: ≥ 1 entry; eigen: absent | uniform (Palace-style) lumped ports; index order = matrix order |
| `ports[].physical_group` | string | dimension-2 group holding the port faces |
| `ports[].e_hat` | `[x, y, z]`, non-zero | gap direction (normalized on load) |
| `ports[].resistance_ohm` | float > 0, Ω | termination **and** S-parameter reference impedance |
| `ports[].width` | optional, mesh units | extent across `ê`; default `area / length` of the tagged faces |
| `ports[].length` | optional, mesh units | gap extent along `ê`; default: extent of the tagged faces along `ê` |
| `ports[].v_inc` | `[re, im]`, default `[1, 0]` | incident drive voltage (non-zero) |
| `wave_ports[]` | driven only; not with `ports`, Leontovich or Silver-Müller (v1) | wave (modal) ports, see below; index order = S-matrix block order |
| `wave_ports[].physical_group` | string | dimension-2 group holding the **planar** port faces |
| `wave_ports[].n_modes` | int ≥ 1, default `1` | lowest-cutoff cross-section modes carried by the port (one S-matrix channel each) |
| `wave_ports[].a_inc` | `[[re, im], …]`, length `n_modes`, default all `[1, 0]` | per-mode incident amplitude (finite, non-zero) |
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
| `capacitance` | optional section | its presence makes this a capacitance spec (see [Capacitance](#static-capacitance-geode-capacitance-issue-705)) |
| `capacitance.terminals` | strings, ≥ 1, distinct | **dimension-2** groups, one per conductor terminal, in matrix row/column order |
| `capacitance.ground` | strings, ≥ 1 (v1) | **dimension-2** groups held at 0 V in every excitation (ground plane, shield, enclosure) |
| `inductance` | optional section | its presence makes this an inductance spec (see [Inductance](#static-inductance-geode-inductance-issue-714)) |
| `inductance.paths[]` | ≥ 1 | open current paths, in matrix row/column order |
| `inductance.paths[].name` | string, distinct | the matrix row/column label |
| `inductance.paths[].conductor` | string | **dimension-3** group the current flows through (one path per volume) |
| `inductance.paths[].source` / `sink` | strings, distinct | **dimension-2** groups on the conductor's boundary where the current enters / leaves; node-disjoint; source and sink must both touch the **same connected component** of the PEC wall (`boundary_conditions.pec` plus all terminal contacts) |
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

**What `geode eigen` solves.** The first-order Nédélec pencil
`K x = k₀² M_ε x` (curl-curl stiffness, `ε_r`-weighted mass, `μ_r = 1`)
on the edges left after PEC elimination, assembled sparse and solved by
the pure-Rust sparse shift-invert Lanczos around `σ = k₀,shift²`. With
real `ε_r` and no `absorbing_regions` the pencil is real symmetric and
lossless (`geode_core::eigen::pec_cavity`, `Q = null`); with any lossy
`ε_r` or `absorbing_regions` it is complex symmetric
(`geode_core::eigen::lossy_cavity`, complex `k₀`, finite `Q` — see
[below](#lossy--open-cavity-eigenmodes-issue-706)). Surfaces not named
PEC are natural (PMC-like) boundaries, as for `driven`.

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

### Lossy / open-cavity eigenmodes (issue #706)

A lossy `materials[].eps_r` (`im < 0`) and / or `absorbing_regions` make
the eigen pencil **complex symmetric**; `geode eigen` then solves it with
the sparse complex shift-invert Lanczos
(`geode_core::eigen::lossy_cavity`) and reports complex modes
(`solver.pencil = "complex_symmetric"`):

- `k₀ = √λ` on the principal branch (`Re k₀ ≥ 0`), time convention
  `exp(+jωt)`: `Im k₀ > 0` is a mode that **decays** (lossy or
  radiating). `lambda` / `k0` carry `Re(λ)` / `Re(k₀)`, the additive
  `lambda_im` / `k0_im` carry the imaginary parts, `frequency_hz` is from
  `Re(k₀)`, and `q = Re(k₀) / (2|Im k₀|)`.
- **Exact check for a uniform fill.** A PEC cavity filled entirely with
  one `ε_r = ε′(1 − j tan δ)` has `λ = λ₀/ε_r` exactly, where `λ₀` is the
  lossless eigenvalue of the same uniform-fill cavity (not the vacuum
  permeability `μ₀`), so
  `Im(λ)/Re(λ) = tan δ` and `Q = ½·cot(δ/2)` (`δ = atan tan δ`; the
  familiar `1/tan δ` is its small-loss limit). `tests/sphere_lossy_pec_golden.rs`
  holds the CLI to that to `10⁻⁶` at `tan δ = 0.01` and `0.1`. A
  partial fill obeys the passive bound `Q ≥ ½·cot(δ_max/2)`.
- **`absorbing_regions` are frozen at the shift.** The UPML stretch
  `s = 1 − jσ₀(d/w)²/k₀` depends on frequency, so an exact open-cavity
  solve is a nonlinear eigenproblem. `geode eigen` evaluates the
  tensors once at `k₀ = eigen.shift` (`solver.upml_reference_k0`) and
  solves the resulting linear pencil — place `shift` **at / near** the
  resonance you want (not below it, as for a closed cavity): modes far
  from it see a mistuned absorber. There is no self-consistent
  iteration of the reference frequency (a possible follow-on).
- **Filtering.** Besides the gradient nullspace, Ritz values with
  `Re(λ) ≤ 0` (`Q ≤ ½`, not resonances — the absorber-trapped quasi-modes
  an open pencil has around the origin) are dropped and counted in
  `solver.n_overdamped_filtered`. Low-`Q` UPML-coupled modes with
  `Re(λ) > 0` are genuine eigenpairs of the discrete pencil and are
  **not** filtered — read `q` to tell them apart from the resonance of
  interest.
- **Validation tier.** The lossy closed cavity is checked against the
  exact relation above. `absorbing_regions`-in-eigen is validated at a
  **smoke** tier only (`tests/eigen_upml_smoke.rs`, the patch smoke mesh):
  decaying modes, finite `Q`, the residual gate, `Re(k₀)` of the patch
  mode stable to ~1 % under a `σ₀` change, and a lower `Q` than the same
  cavity closed. There is no analytic open-cavity oracle through the CLI:
  `examples/mie_sphere`'s open-resonator benchmark uses a **spherical**
  PML, which the box `absorbing_regions` cannot express.
- **Not supported:** Leontovich walls (`Z_s ∝ √(jωμ₀/σ)` depends on the
  unknown frequency) and Silver-Müller walls (a term linear in `k₀`, not
  `k₀²`) — both would make the eigenproblem nonlinear, so eigen specs
  reject them with `invalid_spec`. Model conductor loss with a lossy
  `materials` region, or use `geode driven` at known frequencies.
- `--outdir` writes `E_real` **and** `E_imag` per mode (a complex
  eigenvector has an arbitrary complex phase).

Example (`examples/eigen/lossy_sphere_cavity.json`, the golden fixture):

```json
{
  "schema_version": 1,
  "mesh": { "path": "../../../geode-core/tests/fixtures/sphere.msh", "length_unit_m": 0.01 },
  "materials": [
    { "physical_group": "sphere_interior", "eps_r": [2.25, -0.0225] },
    { "physical_group": "vacuum_gap",      "eps_r": [2.25, -0.0225] },
    { "physical_group": "pml_shell",       "eps_r": [2.25, -0.0225] }
  ],
  "boundary_conditions": { "pec": ["outer_boundary"] },
  "eigen": { "n_modes": 5, "unit": "k0", "shift": 1.0 }
}
```

(`pml_shell` is only a region name here: it is the `1.5 < r < 2` shell
inside the PEC wall, filled like the rest.) Every mode reports
`Im(λ)/Re(λ) = 0.0100000004` (the per-tet weights are uploaded in f32)
and `q = 100.0025` (`½·cot(δ/2) = 100.0025`, vs `1/tan δ = 100`).

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

**Open boundaries** (issue #683). `absorbing_regions` terminates an
**axis-aligned box** mesh with a matched (full Sacks) UPML shell: the
inner wall is the mesh node bounding box shrunk by `thickness` on every
face, and each shell tet whose centroid is beyond it gets `ε = ε_r·Λ`,
`ν = Λ⁻¹` with the per-axis box stretch
(`geode_core::mesh::patch::box_upml_tensors`). Back the shell with a PEC
outer wall (as the patch antenna does) or a Silver-Müller wall. A region
thicker than half the mesh extent, or one with no tet beyond the inner
wall (a no-op), is `invalid_spec`. The UPML tensors carry `1/k₀`, so a
UPML spec reassembles the operator **once per frequency** (a spec without
UPML keeps the batched assemble-once sweep, unchanged). Spherical /
cylindrical shells and the ε-only `DiagTensor` stretch are not in the
schema.

**Wave ports** (issue #683). Each wave port's tagged faces must be
coplanar (to `1e-6` of the face diameter); they are projected into a
local 2-D cross-section whose rim (edges on a single face triangle) is
PEC, and its `n_modes` lowest-cutoff transverse modes
(`geode_core::driven::ports::project_port_face` +
`solve_waveguide_modes`) become the port's channels. Name the waveguide
walls in `pec`. The driven result is the power-normalized channel
S-matrix (port-major, mode-minor), with `β` per channel; wave ports
define no port impedance. **v1 boundary:** a spec has lumped `ports`
**or** `wave_ports`, never both, and `wave_ports` cannot be combined with
Leontovich or Silver-Müller walls (PEC and `absorbing_regions` compose)
— these combinations are rejected with `invalid_spec`, not ignored.
Cross-sections with a TEM mode (multiply connected, e.g. coax) are not
supported: the TEM mode lives in the gradient nullspace and is filtered
out. Asking for more modes than the cross-section can hold fails with
`solve_failed`.

## Static capacitance (`geode capacitance`, issue #705)

```sh
geode capacitance <spec.json|spec.toml> [-o report.json] [--threads N] [--spice out.sp [--spice-ret-pin]]
```

Capacitance example (the triaxial golden input,
`tests/fixtures/capacitance_triax_smoke.toml` — TOML works like JSON):

```toml
schema_version = 1

[mesh]
path = "../../../geode-core/tests/fixtures/coax_capacitance_smoke.msh"
length_unit_m = 0.001

[[materials]]
physical_group = "dielectric_inner"
eps_r = [2.0, 0.0]

[capacitance]
terminals = ["inner", "shield"]
ground = ["outer"]
```

**What it solves.** The scalar electrostatic problem
`−∇·(ε₀ ε_r ∇φ) = 0` on the tets (linear P1 Lagrange, per-region real
`ε_r` from `materials`, vacuum where unlisted), wrapping the validated
library extractor `geode_core::assembly::electrostatic`
(`assemble_electrostatic` + `extract_capacitance`, the
`benchmarks/electrostatic` path). Each terminal's and ground group's
tagged triangles become a Dirichlet node set. For every terminal *i* one
sparse-LU solve runs with **terminal *i* at 1 V and every other terminal
and every ground surface at 0 V**, and the Maxwell matrix follows from
the energy method `C_ij = φ⁽ⁱ⁾ᵀ K φ⁽ʲ⁾` with the full stiffness `K`.

- **Conductor model — grounded, not floating.** A non-excited terminal is
  held at 0 V (the standard multi-conductor Maxwell-matrix convention,
  as in Palace's `terminal-C.csv`). There is **no** floating
  (charge-neutral, unknown-potential) conductor formulation. A metal
  island with no connection must still be listed as a terminal; its
  floating behaviour can then be derived from the Maxwell matrix by
  circuit reduction. The report says so in
  `capacitance.conductor_model = "non_driven_grounded"`.
- **Every conductor is at a known potential**, so `boundary_conditions`
  is rejected in a capacitance spec: list a shield / enclosure under
  `capacitance.ground` (not `pec`). Surfaces listed nowhere are natural
  boundaries (zero normal `D`: a symmetry / open-circuit wall, not a
  conductor), and an unlisted interior surface is just mesh faces.
- **Touching conductors are rejected.** If two terminals, or a terminal
  and a ground surface, share a mesh node they are electrically shorted
  and the run fails with `invalid_spec` — never a silently wrong matrix.
  Merge them into one terminal or separate them in the mesh.
- **Units.** The mesh is integrated in mesh units; `length_unit_m` scales
  the result to **farads**. The matrix is real (plain numbers, not
  `[re, im]` pairs).
- **Conductor interiors** may be meshed or not. Not meshed (the terminal
  is a mesh-boundary surface, as in the coax fixture) is cheaper and
  enables the surface-flux cross-check; a meshed interior is simply
  field-free.
- **Accuracy.** The energy method is exact with respect to the
  discretization; P1 converges as `O(h²)` in `C` and, on the exact
  geometry, from above. The golden coax sits at 0.32 % on a 1 074-node
  mesh and 0.14 % on 5 613 nodes (`benchmarks/electrostatic` bar: 1 %).

**Not the same "inductance / capacitance" as `geode extract`.** `geode
extract`'s `l0_h` is the **RF quasi-static `L₀`** — `Im Z_kk / ω` of a
driven full-wave sweep extrapolated to `f → 0`, per lumped port.
`geode capacitance` reports the **static Maxwell `C`-matrix** of an
electrostatic solve between conductor terminals, with no ports and no
frequency. The static Maxwell `L`-matrix is `geode inductance` (below).

From a layout, `geode mesh --analysis capacitance` writes a ready
capacitance spec (see [Static-extraction starter
specs](#static-extraction-starter-specs---analysis-issue-720)).

Not in v1: floating conductors, `--outdir` potential / field export, a
P2 element option (the library has `extract_capacitance_p2`), and a
calibrated resource estimate (`geode check` reports the scalar system
size in `capacitance` and `resources = null` for a capacitance spec).

Not in schema v1: the matrix-free iterative solver and field / NTFF
export. Lossy / open-cavity eigenmodes (complex `ε_r` and
`absorbing_regions` in an eigen spec → finite `Q`) landed in issue #706
(see [lossy / open eigen](#lossy--open-cavity-eigenmodes-issue-706));
Leontovich / Silver-Müller walls stay driven-only (nonlinear in the
eigenfrequency).

## Static inductance (`geode inductance`, issue #714)

```sh
geode inductance <spec.json|spec.toml> [-o report.json] [--threads N]
                  [--spice out.sp [--spice-ret-pin] [--spice-positive-k]]
```

Inductance example (the triaxial golden input,
`tests/fixtures/inductance_triax_smoke.toml`):

```toml
schema_version = 1

[mesh]
path = "../../../geode-core/tests/fixtures/coax_inductance_smoke.msh"
length_unit_m = 1e-3

[boundary_conditions]
pec = ["shield", "end_caps"]

[[inductance.paths]]
name = "core"
conductor = "core"
source = "core_in"
sink = "core_out"

[[inductance.paths]]
name = "tube"
conductor = "tube"
source = "tube_in"
sink = "tube_out"
```

**What it solves.** For each current path, first the **current
density**: a P1 conduction solve on the path's conductor tets only,
`∇·(σ∇φ) = 0` with `φ = 1` on `source`, `φ = 0` on `sink` and an
insulated (zero normal current) conductor surface elsewhere; then
`J = −σ∇φ` per tet, normalised to a **1 A** net current
(`geode_core::assembly::current_path::open_path_current`). Then the
**field**: the vector magnetostatic problem `∇×(ν₀ν_r∇×A) = J` on
lowest-order Nédélec edges over the whole mesh (per-region
`ν_r = 1/μ_r`), with the PEC wall as `n×A = 0` and a tree-cotree gauge,
one sparse-LU solve per path; the Maxwell matrix follows from the energy
method `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)` with the full curl-curl `K`
(`geode_core::assembly::magnetostatic3d::extract_inductance`, the
`benchmarks/magnetostatic_inductance` path).

- **Open paths returning through a PEC wall.** The current enters the
  conductor through `source` and leaves through `sink`; both faces are
  treated as **PEC contacts** (eliminated like `pec`, do not list them
  there). The PEC wall is the return conductor (the coax shield + end
  caps) and truncates the domain, so it is **required** — unlike
  `geode capacitance`, which rejects `pec`. A terminal floating in the
  dielectric has no return path and is rejected (`invalid_spec`).
- **Source and sink on the same connected PEC component.** Touching
  *some* PEC surface is not enough. Define the grounded set as the
  triangles of every `boundary_conditions.pec` group plus every path's
  source / sink contact faces, with two nodes connected when a chain of
  those triangles' edges joins them. Each connected component must
  receive zero net current from each path, so a path's source and sink
  must touch the **same** component. Coax end caps without the `shield`
  joining them are two separate components: that spec is rejected at
  load time (`invalid_spec`, also by `geode check`), and the message
  names the path and the PEC groups on each side and asks for the
  connecting PEC surface. Without this rule the magnetostatic problem is
  inconsistent, and the gauged solve would return a meaningless matrix
  rather than an error.
- **Closed loops are not supported.** A ring with no terminals has no
  source / sink pair; driving it needs a cut surface carrying a potential
  jump (an EMF / cohomology-cut condition), which v1 does not implement.
  Model a loop as an open path whose two ends land on the PEC wall.
- **Discrete compatibility.** The curl-curl source problem needs a
  discretely divergence-free `J`. The conduction construction satisfies
  it by Galerkin orthogonality (the discrete divergence at a node *is*
  the P1 conduction residual there); every path is checked before its
  solve and the report carries `max_solenoidal_residual` (round-off,
  ~1e-15, on the golden meshes).
- **Conductivity.** Each path is one homogeneous conductor volume; the
  current distribution does not depend on its conductivity value, so the
  spec has no `sigma`. Current sharing between sub-regions of different
  conductivity (laminated conductors) is not in v1.
- **Permeability.** `materials[].mu_r` (real, `> 0`, default `1`) weights
  the curl-curl per region, conductors included.
- **Units.** Integrated in mesh units; `length_unit_m` scales the result
  to **henries**. The matrix is real and symmetric.
- **Physical gate.** A static inductance matrix is symmetric positive
  definite. If any self term `L_ii ≤ 0`, or the matrix is not SPD, the
  run fails with `solve_failed` and a non-zero exit. It never reports
  `status: "ok"` in that case, so a successful report always has
  `is_spd = true`.
- **Accuracy.** The golden coax sits at 0.46 % on a 1 202-node mesh and
  0.16 % on 5 564 nodes (`benchmarks/magnetostatic_inductance` bar: 1 %);
  the triax 2×2 entries at ≤ 0.83 % / ≤ 0.27 %.

**Static Maxwell `L` vs `geode extract`'s `l0_h`.** `geode extract`'s
`l0_h` is the **RF quasi-static `L₀`** of one lumped port: `Im Z_kk / ω`
of a driven full-wave sweep, extrapolated to `f → 0`, including whatever
the port's excitation and the full-wave solution imply. `geode
inductance` reports the **static Maxwell `L`-matrix** of prescribed
current paths from a magnetostatic solve — no ports, no frequency, a
matrix with mutual terms. They answer different questions and agree
only when the port geometry realises the same current path in the
low-frequency limit.

Not in v1: closed loops (cut / EMF boundary condition), lumped gap
sources (terminals off the PEC wall, or a source and sink on different
PEC components), heterogeneous-conductivity paths, and field export.
From a layout, `geode mesh --analysis inductance` writes a ready
inductance spec (see [Static-extraction starter
specs](#static-extraction-starter-specs---analysis-issue-720)). `--spice` writes the matrix as
self inductors plus `K` couplings (see [SPICE subcircuit
export](#spice-subcircuit-export---spice-issues-715--719)).
`geode check` reports the path list and edge-DOF counts in its
`inductance` block and a `resources` estimate with `scalar = "real"` and
one factorization per path.

## Field / far-field export (`--outdir`, issue #684)

`geode driven | extract | eigen … --outdir <DIR>` additionally writes
field files. **Off by default**: without the flag no export runs, nothing
else touches the filesystem, and the report is byte-for-byte what it was
before the flag existed (the new fields are omitted, not `null`).

- **Directory**: `<DIR>` (and missing parents) is created up front,
  before any solve, so an unwritable path fails fast with code `io`. It
  is created even when nothing ends up exported (wave-port specs).
- **Overwrite**: files are written under fixed, index-derived names;
  an existing file of the same name is **overwritten**, and nothing else
  in `<DIR>` is touched or removed (the repo's `--export-field`
  precedent). Use a fresh directory per run to keep runs apart.
- **References**: each file appears in the report as
  `{ "path", "sha256" }` — `path` **relative to `<DIR>`** (a bare file
  name, e.g. `"E_0000.vtu"`), `sha256` the hex SHA-256 of the bytes
  written.

| Spec | Files per report row / mode | Report field |
|---|---|---|
| `driven` / `extract`, lumped ports | `E_<row>.vtu` — `E` of **one extra solve** with every port driven at its spec `v_inc` (the sweep itself only keeps `Z` / `S`) | `results[].field_file` |
| … plus exactly one `absorbing_regions` shell | `pattern_<row>.json` — principal-plane cuts | `results[].far_field` |
| `eigen` | `E_mode_<mode>.vtu` — the eigenvector (PEC edges 0): lossless, real `E_real`, `M_ε`-normalized, arbitrary sign; lossy / open (#706), `E_real` + `E_imag`, bilinear `xᵀMx = 1`-normalized, arbitrary complex phase | `modes[].field_file` |
| wave ports | **nothing** (see below) | — |

`<row>` / `<mode>` are the zero-padded 4-digit `results[]` / `modes[]`
indices (for `extract`, rows are ascending in frequency). The extra
export solves are not counted in `solver` statistics.

**`.vtu` files** are ASCII VTK `UnstructuredGrid` (ParaView) with
`PointData` `E_real`, `E_imag` (driven only), `|E|` and `eps_r` (mean
`Re ε_r` of the incident tets). The nodal `E` is a **Whitney average**
(`geode_util::viz::edge_field_to_nodes`): each incident tet's edge-element
interpolant evaluated at the vertex, averaged onto the shared node — a
visualization aid, not a quadrature-accurate field sample (it smears the
normal-`E` jump at material interfaces).

**`far_field`** (Love surface-equivalence NTFF,
`geode_core::postproc::ntff`, over the shell's inner wall shrunk 10 %
toward its centre — `examples/patch_antenna`'s Huygens box — on a 91 × 72
`(θ, φ)` grid, 2° × 5°):

| Field | Units | Meaning |
|---|---|---|
| `box_lo` / `box_hi` | mesh units | the NTFF / flux box |
| `directivity_max` | – | peak `D` (linear) |
| `directivity_broadside` | – | `D` at +z (`θ = 0`, φ-averaged pole row) |
| `gain_broadside` | – | `D_broadside · η` (`η` clamped to `[0, 1]`) |
| `gain_broadside_db` | dBi | `10·log10(gain_broadside)` |
| `efficiency` | – | `η = P_rad / P_in`: box Poynting flux over the net port input `Σ_k ½ Re(V_k I_k*)`; not clamped |
| `pattern_file` | | `{path, sha256}` of `pattern_<row>.json`: `frequency_hz`, `theta_rad[]`, `e_plane_e_norm[]` (`φ = 0`), `h_plane_e_norm[]` (`φ = π/2`), each cut normalized to its own max |

No NTFF without an `absorbing_regions` shell (a Silver-Müller-only or
closed spec gets `field_file` only), or with more than one shell (no
single Huygens box).

**Placing the NTFF box is your job.** The box is the shell's inner wall
(`air_box_lo` / `air_box_hi`, reported by `check` / `driven` / `extract`)
shrunk another 10 % toward its centre. For `efficiency`, `directivity_*`
and `gain_*` to mean anything, it must:

- **enclose the radiator and every port** — the box's Poynting flux and
  equivalent currents only account for sources inside it, so a radiating
  conductor or feed outside the box breaks both `P_rad` and the pattern;
  and
- **lie entirely in air** — no dielectric or PEC tet may cross the box
  surface, since the Love equivalence assumes free space on it.

The CLI checks only that the box is not empty (at least one tet centroid
inside it; with `--outdir`, before the sweep runs, so a bad box fails
fast). It cannot tell a well-placed box from a badly placed one: a box
that cuts through the substrate or leaves a port outside still runs to
completion and reports a silently wrong far field. Size the shell (mesh
extent and `thickness`) so the radiating structure sits well inside the
shrunk box, as `examples/patch_antenna` does.

**Wave ports are out of scope**: a wave-port driven report never carries
`field_file` / `far_field`, even with `--outdir` (stderr notes the skip).
The physical field there is a linear combination of the per-channel
Sherman–Morrison–Woodbury solves, which the library does not return, and
reconstructing it would need a `geode-core` API extension.

## Touchstone output (`--touchstone`, issue #703)

`geode driven | extract … --touchstone <PATH>` additionally writes the
sweep's S-parameters as a **Touchstone 2.0** file at `<PATH>` (an
existing file is overwritten). The parent directory must exist, be a
directory and not be read-only: this is checked **before the sweep**
(an `io` error, like an unwritable `--outdir`), since the file itself is
only written after it. Nothing is created at `<PATH>` by the check.
Independent of `--outdir`. The report gains
`touchstone_file: { "path", "sha256" }` — `path` is `<PATH>` **as given
on the command line** (like `spec_path`; not relative to `--outdir`),
`sha256` the hex SHA-256 of the bytes written. Without the flag the
field is omitted.

```text
! Touchstone 2.0 written by geode 0.4.0 (<git-sha>)
! spec: filter.json
! S-parameters vs per-port lumped resistance_ohm ([Reference]); RI format; Hz
[Version] 2.0
# HZ S RI R 5e1
[Number of Ports] 2
[Two-Port Data Order] 21_12
[Number of Frequencies] 3
[Reference] 5e1 7.5e1
[Network Data]
1e9 <S11 re im> <S21 re im> <S12 re im> <S22 re im>
…
[End]
```

- **Always 2.0**, whatever the port count: each lumped port's
  `resistance_ohm` is its own reference impedance and may differ between
  ports, which only 2.0's `[Reference]` line can express. The option
  line's `R` repeats port 1's value for 1.0-era readers.
- **`S` only, `RI`, `HZ`**: `results[].s` copied verbatim (`[re, im]`),
  frequencies in Hz. Numbers use shortest round-trip formatting (`5e1`,
  `3.333333333333333e-1`), so a parser gets the report's values back
  bit for bit. Integral mantissas carry **no decimal point** and the
  exponent no `+` (`5e1`, `1e9`, `0e0`) — valid C-locale float syntax,
  but a naive regex reader expecting `5.0e+01` would misparse it.
  `Z` / `Y` / `MA` / `DB` are not offered.
- **Ascending frequency**: rows are sorted ascending even though a
  `driven` report keeps spec order (`extract` rows already are).
  Duplicate frequencies in the spec are rejected up front
  (`invalid_spec`) — a Touchstone file cannot hold two rows at one
  frequency.
- **Layout**: `[Matrix Format]` is the default `Full`. Two-ports use the
  classic `S11 S21 S12 S22` order, declared as
  `[Two-Port Data Order] 21_12`; every other port count is row-major
  (`S11 S12 … S1N`, then row 2, …), each matrix row on its own line(s)
  of at most four pairs.
- **Rejected before any solve** with `invalid_spec`: wave-port specs
  (their power-normalized channel S-matrix has no real reference
  impedance, and writing a placeholder `[Reference]` would silently
  mislabel it) and `geode eigen` (no network parameters).

Loading the file in [scikit-rf](https://scikit-rf.org) — **illustrative,
not run in CI** (scikit-rf is not a dependency of this repo):

```python
import skrf
net = skrf.Network("filter.s2p")   # Touchstone 2.0, per-port [Reference]
print(net.z0[0], net.f[:3], net.s[0])
```

An **optional scikit-rf check** (issue #713) loads a real
`geode driven --touchstone` `.s1p` (`tests/touchstone_skrf.rs`) and
`render`'s 2-port (`21_12` order, unequal references) and 5-port
(row-major, wrapped) output (`src/touchstone.rs` unit tests) and compares
`f`, `z0` and `s` value for value. It runs when `GEODE_SKRF` (else
`python3`) can `import skrf`, and is otherwise skipped with a loud
`SKIPPED` line on stderr — like the ngspice checks, not a CI dependency.
Last run: scikit-rf 2.1.0, 2026-09-30, all three files matched.

```sh
python3 -m venv /tmp/skrf && /tmp/skrf/bin/pip install scikit-rf
GEODE_SKRF=/tmp/skrf/bin/python cargo test -p geode-cli scikit_rf -- --nocapture
```

## SPICE subcircuit export (`--spice`, issues #715 / #719)

`geode capacitance … --spice <PATH>` and `geode inductance … --spice
<PATH>` additionally write the extracted matrix as a SPICE subcircuit at
`<PATH>` (the parent directory must exist; an existing file is
overwritten), for circuit simulators such as ngspice. The report gains
`spice_file: { "path", "sha256" }` — `path` is `<PATH>` **as given on
the command line**, `sha256` the hex SHA-256 of the bytes written;
without the flag the field is omitted. The RF subcommands (`driven`,
`eigen`, `extract`) do not have the flag. Both exports share the
provenance header, node naming and value formatting below; the
inductance specifics follow in [Inductance](#inductance-l--k-issue-719).

### Capacitance (`CEXTRACT`, issue #715)

```text
* SPICE subcircuit written by geode 0.5.0 (<git-sha>)
* spec: tests/fixtures/capacitance_triax_smoke.toml
* mesh: …/coax_capacitance_smoke.msh sha256=582f4250…
* Mutual (circuit) capacitance from the Maxwell matrix, farads: C_ground(i) = sum_j C_ij; C_mutual(i,j) = -C_ij (i != j)
* noise threshold: |C| < 1e-9 * max(diag) = 1e-9 * 3.6264319203392903e-13 F = 3.6264319203392905e-22 F is dropped
* dropped: C(inner, ground) = -8.271806125530277e-28 F, |C| < 3.6264319203392905e-22 F -- below noise threshold
.subckt CEXTRACT inner shield
C1_2 inner shield 2.3767726604103816e-13
C2_0 shield 0 1.2496592599289087e-13
.ends CEXTRACT
```

(the triax golden spec: `inner` is fully enclosed by `shield`, so its
ground branch is exactly zero up to solver round-off).

- **Conversion** — the report's Maxwell matrix becomes one capacitor per
  branch: `C_ground(i) = Σ_j C_ij` (`c_sigma_farad[i]`) from terminal
  *i* to ground node `0`, and `C_mutual(i, j) = −C_ij` (symmetrized,
  `i < j`) between terminals *i* and *j*. The network reproduces the
  Maxwell matrix exactly. `capacitance.ground` groups are the reference
  node `0` itself — never a `.subckt` port.
- **Noise threshold** — a branch with `|C| < 1e-9 × max_i C_ii` is
  numerical noise (a shielded terminal's row sum sits at ~1e-15
  relative) and is **dropped**, with a `* dropped:` comment naming the
  branch and its value. A branch above the threshold with a **negative**
  value means a genuinely non-Maxwell matrix (see
  `maxwell_sign_structure`): the run fails with `invalid_spec` and no
  file is written — negative capacitors are never emitted or clamped.
  The threshold is fixed in v1.
- **Layout** — `.subckt CEXTRACT <terminal nodes…>` in terminal order
  (fixed name, so testbenches do not depend on the spec file name);
  instance names from 1-based terminal indices (`C1_2`, `C2_0`), in the
  order: per terminal, its ground branch then its mutual branches to
  higher-numbered terminals. Values are plain farads (no `p` / `f`
  suffix) in shortest round-trip exponent form, so a parser gets the
  exported numbers back bit for bit. Only whole-line `*` comments.
- **Node names** — terminal names are sanitized: characters outside
  `[A-Za-z0-9_]` become `_`; an empty name or one starting with a digit
  is prefixed `n_`; a (case-insensitive) collision with an earlier node
  or the ground names `0` / `gnd` gets the first free `_2`, `_3`, …
  suffix. Each renamed terminal gets a `* node: "<terminal>" -> <node>`
  comment.

Driving it in ngspice — **illustrative, not run in CI** (ngspice is not
a dependency of this repo; `tests/spice_golden.rs` runs an equivalent
check only when an `ngspice` binary is found): port 1 at 1 V AC reads
back the Maxwell column `C_j1 = −Im I(V_j) / ω`.

```spice
triax capacitance check
.include caps.sp
X1 p1 p2 CEXTRACT
V1 p1 0 DC 0 AC 1
V2 p2 0 DC 0 AC 0
.control
ac lin 1 1meg 1meg
let c11 = -imag(i(v1))/(2*pi*1e6)
let c21 = -imag(i(v2))/(2*pi*1e6)
print c11 c21
.endc
.end
```

### Inductance (`L` / `K`, issue #719)

```text
* SPICE subcircuit written by geode 0.5.0 (<git-sha>)
* spec: tests/fixtures/inductance_triax_smoke.toml
* mesh: …/coax_inductance_smoke.msh sha256=53e6e270…
* Maxwell inductance matrix as self inductors plus K couplings, henries: L<i>_0 = L_ii from path node i to 0; K<i>_<j> = L_ij / sqrt(L_ii * L_jj)
* port: a path's source and sink faces both contact the PEC return wall, which is ground 0, so each path is one node; every inductor is declared node -> 0, so k carries the sign of L_ij
* noise threshold: |k| < 1e-9 is dropped
.subckt LEXTRACT core tube
L1_0 core 0 2.6848490936334466e-10
L2_0 tube 0 8.227359649685979e-11
K1_2 L1_0 L2_0 6.457431227858365e-1
.ends LEXTRACT
```

(the triax golden spec: `core` and `tube` both return through the PEC
shield, so they are strongly coupled, `k ≈ 0.65`).

- **Ports** — a path's source and sink faces both contact the *same*
  connected PEC return wall, so the wall is the common return of every
  path: each path is **one** SPICE node (named from the path name by the
  node-name rules above) and the wall is ground node `0`.
  `.subckt LEXTRACT <path nodes…>` in path order (fixed name, distinct
  from `CEXTRACT`). Physically, a path's port is the **break in that
  path's current loop at its source face**: the node is the conductor
  side of the break, `0` is the PEC wall side, and a current driven
  *into* the node is the path's reference current (source face →
  conductor → sink face → wall → back to the source face). So
  `I1 0 core DC 0 AC 1` pushes 1 A of reference current around the
  `core` loop, and `V(core)` is that loop's voltage `jω Σ_j L_core,j
  I_j`.
- **Conversion** — one self inductor `L<i>_0 <node_i> 0 <L_ii>` per path
  (henries), then one coupling statement `K<i>_<j> L<i>_0 L<j>_0 <k_ij>`
  per pair `i < j` with `k_ij = L_ij / √(L_ii L_jj)`. SPICE has no
  mutual-inductor branch; the coupled pair gives `V_i = jω Σ_j L_ij
  I_j`, i.e. the network reproduces `l_henry` exactly.
- **Sign** — every inductor is declared path node → `0`, so every path's
  reference current (source → sink through the conductor) enters the
  dotted end, and `k_ij` carries the sign of `L_ij` directly. Swapping a
  path's `source` / `sink` flips its mutuals, which appear as a
  **negative `k`** (legal ngspice syntax; verified against ngspice-46)
  with the node order unchanged. See the portability note below, and
  `--spice-positive-k` for simulators that reject negative `k`.
- **Noise threshold** — `k` is dimensionless, so the threshold is
  absolute: a coupling with `|k_ij| < 1e-9` is **dropped** with a
  `* dropped: K(<path_i>, <path_j>) = …` comment. `|k_ij| ≥ 1`, or a
  non-positive self inductance, is not a physical network: the export
  fails with `invalid_spec` and no file is written (the report's SPD gate
  already rules this out; the export re-checks independently).

In ngspice, drive a path node with a 1 A AC current source and leave the
others **undriven** (a grounded node would short the induced mutual
voltage): `L_jk = Im V(p_j) / ω`. `tests/inductance_spice_golden.rs`
runs this check when an `ngspice` binary is found.

```spice
triax inductance check
.include l.sp
X1 p1 p2 LEXTRACT
I1 0 p1 DC 0 AC 1
.control
ac lin 1 1meg 1meg
let l11 = imag(v(p1))/(2*pi*1e6)
let l21 = imag(v(p2))/(2*pi*1e6)
print l11 l21
.endc
.end
```

**Negative-`k` portability** (snapshot as of 2026-09; simulator
behavior can change between versions): ngspice accepts `−1 < k < 1`
(verified against ngspice-46, including the reversed-triax golden
netlist). LTspice, HSPICE and Spectre (SPICE mode) are believed to
accept negative `k` as well, but this is **untested** here. PSpice /
OrCAD has historically required `0 < k ≤ 1` and would reject a
negative-`k` file; Xyce is unconfirmed. For those, use
`--spice-positive-k` (below).

### Variants: `--spice-ret-pin`, `--spice-positive-k` (issue #723)

Both are off by default, and without them the file is byte-identical to
the layouts above. Both require `--spice`.

- **`--spice-ret-pin`** (`capacitance` and `inductance`) — instead of
  tying the ground / return reference to the simulator's global node `0`
  inside the subcircuit, expose it as an extra **last** port named
  `ret`: `.subckt CEXTRACT <terminal nodes…> ret` /
  `.subckt LEXTRACT <path nodes…> ret`, and every branch that would go to
  `0` goes to `ret`. The instantiating deck wires `ret` wherever it
  likes (or floats it, given some DC reference). For that run `ret` is a
  reserved node name (case-insensitively), so a terminal / path literally
  named `ret` / `Ret` gets the usual `_2` suffix, with a `* node:`
  comment.

  ```spice
  X1 p1 p2 r LEXTRACT        ; ret -> r, not ground
  RDC r 0 1                  ; DC reference only; carries no AC current
  I1 r p1 DC 0 AC 1          ; L_j1 = Im(V(p_j) - V(r)) / ω
  ```

- **`--spice-positive-k`** (`inductance` only) — emit every `K` with a
  positive `k`, for simulators that reject negative ones. Some paths are
  emitted **flipped**: their inductor is declared `L<i>_0 0 <node_i>`
  (return → node, dot on the return side) instead of `<node_i> 0`, and
  each emitted `k_ij` becomes `s_i s_j k_ij` (`s = −1` on flipped
  paths). This is an equivalent network, **not** a change of reference:
  the port-level matrix is still exactly `l_henry`, signs included, and
  every path's port reference current is still *into* its node (source
  → sink). What is reversed is only the flipped inductor's own SPICE
  branch convention — its dot is on the return side, and ngspice's
  `i(L<i>_0)` for it reads the **negative** of the path's reference
  current. Each flipped path gets a `* flipped: "<path>" (…)` comment
  (or `* flipped: none`). Verified in ngspice-46: the reversed triax
  exported as `L2_0 0 tube …` with `K1_2 … +0.6457…` measures
  `L_12 = −9.597e-11 H` at the ports, the report's value.

  Choosing the flips is signed-graph switching: a positive coupling
  needs both paths oriented alike, a negative one oppositely. It is
  **always possible for two paths**, and for any set of paths whose
  couplings form no cycle; with three or more mutually coupled paths it
  is possible exactly when every cycle of emitted couplings has an even
  number of negative `k`. Otherwise (e.g. three paths all pairwise
  negatively coupled) the run fails with `invalid_spec` naming the
  offending cycle, and no file is written — it never falls back to
  mixed signs. Couplings dropped by the noise threshold do not count.
  Within each group of coupled paths the lowest-numbered path is never
  flipped, so the choice is deterministic.

## Resource estimate (`geode check`, issue #703)

`geode check` reports a `resources` block so an agent can pick mesh and
solver before a long run. **It is an order-of-magnitude estimate from a
single measured anchor, not a prediction** — read the caveats.

| Field | Units | Meaning |
|---|---|---|
| `solver_mode` | – | `"direct"` \| `"iterative"` (eigen / inductance specs: always `"direct"`) |
| `scalar` | – | `"complex"` (driven / extract pencil, lossy / open eigen pencil) \| `"real"` (lossless eigen pencil, inductance curl-curl) |
| `nnz_a` | – | non-zeros of the full Nédélec system pattern (before PEC elimination — the anchor's convention); computed from the mesh, no assembly |
| `n_factorizations` | – | LU factorizations: one per frequency (driven / extract direct), one (eigen), one per current path (inductance), `0` (iterative) |
| `n_rhs_per_frequency` | – | ports, or `2 × channels` for wave ports; `0` for eigen; `1` for inductance |
| `anchor_nnz_ratio` | – | `nnz_a / 20 467 522` — the mesh's scale relative to the direct-LU calibration anchor; always against the **direct** anchor, even for `"iterative"` (a scale signal, not a confidence claim) |
| `above_anchor` | – | `anchor_nnz_ratio > 1`: the mesh is larger than the anchor, where the direct figures become **under**-estimates (see *Bias*) |
| `peak_memory_gb` | GB (10⁹ B) | estimated peak resident memory |
| `wall_time_s` / `wall_time_per_factorization_s` | s | direct only (`null` iterative). `wall_time_per_factorization_s` is a per-factorization **scaling unit**, not the time of one isolated LU: the anchor run's *total* wall time (assembly + LU + Lanczos back-solves) scaled by `nnz(A)` and the complex-pencil factor; `wall_time_s` = it × `n_factorizations` |
| `flops_per_iteration` / `flops_max` | flop | iterative only: one complex SpMV + vector updates; worst case at `max_iters` for every RHS and frequency |
| `peak_memory_confidence` | – | `"order_of_magnitude"` |
| `wall_time_confidence` | – | `"conservative_below_anchor"` (direct), `null` (iterative) |
| `calibration_basis` | – | the anchor, its date and the scaling assumptions (machine-readable, updated on re-calibration) |

**Direct model.** Peak memory and wall time scale **linearly in
`nnz(A)`** from one measurement (2026-07-15, the 1 157 564-DOF transmon
eigen run: `nnz(A)` = 20 467 522, COLAMD + faer supernodal LU, 565.5 s
total wall — assembly, one real LU and the shift-invert Lanczos
back-solves together, so each "per-factorization" unit carries its share
of assembly and back-solve time — 92 166 884 KiB ≈ 94.4 GB peak RSS on a 128 GB cloud box;
`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`,
`docs/research/geode-vs-palace-comparison.md` §2b). The complex driven
pencil is scaled ×2 memory / ×4 time over the real anchor
(uncalibrated: no complex run at that scale exists).

- **Bias**: LU fill grows super-linearly in `nnz(A)`, so the linear
  model **over-estimates below the anchor scale and under-estimates
  above it**. Measured locally (2026-09-28, Apple M3 Ultra, release
  build) on every in-repo fixture — all 17–300× smaller than the anchor:

  | Run | `nnz_a` | est. / measured peak memory | est. / measured wall |
  |---|---:|---|---|
  | sphere `eigen` | 66 966 | 0.31 / 0.062 GB (5×) | 1.9 / 0.49 s (4×) |
  | spiral smoke `driven`, 4 freq | 217 544 | 2.0 / 0.85 GB (2.4×) | 96 / 3.3 s (29×) |
  | patch `extract` benchmark | 491 375 | 4.5 / 3.2 GB (1.4×) | 706 / 18 s (39×) |
  | spiral benchmark `driven` | 837 812 | 7.7 / 2.3 GB (3.3×) | 93 / 5.4 s (17×) |
  | SLCFET `extract` benchmark | 1 207 670 | 11.1 / 7.1 GB (1.6×) | 667 / 50 s (13×) |

  Memory landed within ~5× (always high); wall time is machine- and
  thread-dependent and was 4–40× high. Treat `wall_time_s` as a
  conservative ceiling below ~1M DOF, and treat either figure above the
  anchor as a floor, not a ceiling. `above_anchor` / `anchor_nnz_ratio`
  say which side of the anchor a mesh is on
  (`wall_time_confidence = "conservative_below_anchor"` holds only while
  `above_anchor` is `false`).
- **Why not a symbolic-fill model**: a fill-reducing ordering that won
  on *symbolic* fill (`nnz(L)`) was **OOM-killed at 128.5 GB** by the
  real supernodal LU at the anchor scale (same log), so symbolic fill is
  not used as the primary predictor. A symbolic-fill refinement is a
  possible follow-up, not this model.

**Capacitance specs** get `resources = null`: the model above is
calibrated on the Nédélec H(curl) pencil and has no measured basis for
the real scalar electrostatic system. `check` reports that system's size
in its `capacitance` block instead (`n_dof`, `n_free_dof`, `nnz_k`,
`n_solves`).
**Inductance specs** do get an estimate (`scalar = "real"`, one
factorization per current path): the magnetostatic system is a real
Nédélec curl-curl like the anchor's pencil (smaller after the
tree-cotree gauge, so the estimate errs high).

**Iterative model.** Memory is a vector count (operator storage, 16
Krylov vectors, per-tet assembly buffers), not an extrapolation — on the
spiral smoke mesh it came out ~1.6× **low** (0.044 vs 0.072 GB; process
and mesh overhead are not modelled). Cost is reported in flops only:
there is **no measured iterative wall-time anchor**, and iteration
counts depend on the problem and preconditioner (they cannot be
predicted from the mesh).

## Layout → mesh (`geode mesh`, issue #704)

```sh
geode mesh <layout.json|layout.toml> [--analysis driven|capacitance|inductance] [--mesh-out mesh.msh] [--spec-out spec.json] [--gmsh PATH] [--gmsh-timeout SECONDS] [-o report.json]
```

`geode mesh` turns a **layout** — 2-D rectilinear polygons per conductor
layer plus a layer stack — into a solver-ready tagged mesh, so an EDA
flow (e.g. klayout-tools reading GDS, which geode deliberately does not
parse) does not need a hand-built `.msh`:

1. validate the layout (schema v1 below; errors are `spec_parse` /
   `invalid_spec`, reported before Gmsh is looked for);
2. generate an OpenCASCADE `.geo` script — dielectric slabs and
   conductor solids as `Box`es, sheets and ports as `Rectangle`s, one
   `BooleanFragments` for conformal interfaces, bounding-box
   physical-group selection, a distance/threshold size field — and write
   it next to the mesh (same stem, `.geo`) as a reproducibility artifact;
3. run the **external `gmsh` binary** (`--gmsh PATH`, else `$GEODE_GMSH`,
   else `gmsh` on `PATH`; Gmsh ≥ 4.11 with the OpenCASCADE kernel — the
   `apt-get install gmsh` / `brew install gmsh` builds). No FFI and no new
   Rust dependency; a missing binary is a `gmsh_not_found` error naming
   the binary and the install commands. Output: **MSH 4.1 ASCII, linear
   Tet4 / Tri3** only. Meshing is pinned single-threaded
   (`General.NumThreads = 1`, Delaunay 3-D) so the same layout and Gmsh
   version reproduce the same mesh bytes; the report records the Gmsh
   version and the SHA-256 of the layout, script and mesh. The run is
   killed after `--gmsh-timeout` seconds (default 600, `0` = no limit)
   and reported as `gmsh_failed`, with the script kept;
4. read the mesh back through `geode_core::mesh::read_tagged_tet_mesh`
   and fail with `gmsh_failed` unless every generated group has elements,
   every tet is tagged, every exterior face carries a surface group and
   every tagged triangle is a face of some tet;
5. emit a **starter problem spec** (spec schema v1) for the
   `--analysis` selected (default `driven`, below; `capacitance` /
   `inductance`: see [Static-extraction starter
   specs](#static-extraction-starter-specs---analysis-issue-720)) wired
   to the generated names. The driven spec: every slab's `eps_r`,
   `pec = ["outer_boundary", <conductor groups>…]`, one lumped port per layout port (explicit
   `e_hat` / `width` / `length`), `absorbing_regions` for a UPML
   boundary, direct solver, and a **placeholder 1 GHz** frequency list to
   edit. It is always in the report (`starter_spec`); `--spec-out` also
   writes it, with `mesh.path` relative to the spec when both share a
   directory. `-o` is the report, as for every subcommand; the mesh
   defaults to the layout path with a `.msh` extension.

   The unedited starter spec models every conductor as **PEC**, so it
   gives **L only**: `R ≈ 0` (lossless conductors), and `Q` is limited
   only by dielectric loss and the UPML. That L is meaningful — on the
   spiral benchmark layout it lands within ~1 % of the same PEC model on
   the committed hand-built mesh. For R / Q, move the conductor groups
   from `pec` to `leontovich` (keep `outer_boundary` PEC), subject to the
   thick-conductor caveat below:

   ```json
   "boundary_conditions": {
     "pec": ["outer_boundary"],
     "leontovich": [{ "physical_group": "m1", "conductivity_s_m": 5.8e7 },
                    { "physical_group": "m2", "conductivity_s_m": 5.8e7 }]
   }
   ```

**Thick conductors and Leontovich.** A conductor layer with
`thickness > 0` is **hollowed** (issue #721): the generated `.geo`
unions the layer's extruded solids with every other thick conductor and
`BooleanDifference`s the union out of the whole dielectric stack (every
slab, plus the UPML inner-wall box) before the conformal
`BooleanFragments` that embeds sheets and ports — the pattern of the
hand-built `reference/gmsh/spiral_inductor.geo`. The conductor interior
is not meshed, and the layer's group holds exactly its exterior cavity
walls: a trace or via crossing a slab interface leaves no internal face
at the interface, and touching conductors (a via over a trace) share no
wall. So a Leontovich model swapped onto the generated groups is the
same conductor model as the committed benchmark meshes: on the spiral
layouts at 1 GHz, R lands at +0.37 % (smoke) / +0.16 % (benchmark) and Q
at +0.56 % / +0.76 % of the committed `results_smoke.toml` /
`results.toml`, inside the benchmark's 2 % R / Q band (before hollowing,
R was about −5 %); L stays inside its bands (+0.92 % / +0.92 %). Every
exterior face of the generated mesh must carry a surface group (outer
walls or cavity walls), and every tagged surface triangle must be a face
of some tet, else `geode mesh` fails with `gmsh_failed`.
Zero-thickness sheets (`thickness = 0`) are embedded in the dielectric
and meshed two-sided, as before — except where a sheet passes through a
thick conductor (e.g. a trace sheet pierced by a thick via): the sheets
are cut by the same conductor union, so the piece inside the conductor
is removed (it has merged into the conductor, whose cavity wall carries
the current) rather than left as a dangling surface with no tet on
either side. A layout with no thick conductor produces the same script
as before issue #721. Independently, `geode driven` rejects
(`invalid_spec`) a Leontovich, Silver-Müller or port surface with a
triangle that is not a face of any tet (e.g. from a hand-built mesh).

Automatically named physical groups:

| dim | name | tag | from |
|---|---|---|---|
| 3 | `<dielectric name>` | 1, 2, … (stack order) | each dielectric slab (thick-conductor interiors and current-path volumes excluded) |
| 3 | `<path conductor group>` | next (inductance only) | a current-path conductor kept as a meshed **solid** (role `conductor_volume`) |
| 2 | `<conductor layer name>` | 101, … | the layer's un-netted polygons: sheets, or the exterior (cavity-wall) faces of hollowed shells (PEC) |
| 2 | `<net name>` | next | the polygons of that net, on any layer, the same way |
| 2 | `<port name>` | next (driven only) | the port rectangle |
| 2 | `<contact name>` | next (inductance only) | the faces a path conductor shares with its contact's `to` group |
| 2 | `outer_boundary` | last | the six outer walls (PEC) |

A layout without nets has one conductor group per layer, as before
issue #720, and `--analysis driven` (the default) produces the same
script, mesh and starter spec as before.

### Static-extraction starter specs (`--analysis`, issue #720)

`--analysis capacitance` and `--analysis inductance` build the mesh and
the starter spec for [`geode capacitance`](#static-capacitance-geode-capacitance-issue-705)
and [`geode inductance`](#static-inductance-geode-inductance-issue-714),
so a layout goes straight to a C / L matrix (and SPICE):

```sh
geode mesh guard.json --analysis capacitance --spec-out c.json && geode capacitance c.json --spice c.sp
geode mesh line.json  --analysis inductance  --spec-out l.json && geode inductance  l.json --spice l.sp
```

Three additive layout fields (schema v1; layouts without them are
unchanged):

- **Nets** — `polygons[].net`: polygons sharing a net name, on any
  layer, form **one conductor group** named after the net instead of
  joining their layer's group (a layer whose polygons are all netted
  produces no group of its own). Use nets to make two pads of one layer
  two capacitance terminals, or a trace plus its via one terminal.
- **Ground** — top-level `ground: [<group>…]` (layer or net names): the
  capacitance reference conductors.
- **Contacts** — top-level `contacts: [{ "name", "conductor", "to" }]`:
  an inductance current-path endpoint is every face the thick conductor
  group `conductor` shares with the conductor group `to` (its PEC
  return, e.g. a via or an end wall). Each path conductor has exactly
  two contacts, to two different groups: the first listed is the
  path's `source`, the second its `sink`.

What each analysis emits (ports are meshed for `driven` only; contacts
are validated always but meshed for `inductance` only; a UPML boundary
keeps its slab split but is not listed — a static solve has no
absorber, and the grounded / PEC outer walls close the domain):

| `--analysis` | conductor bodies | starter spec |
|---|---|---|
| `driven` (default) | sheets / hollow shells | as above: `pec` = `outer_boundary` + every conductor group, lumped ports, 1 GHz placeholder |
| `capacitance` | sheets / hollow shells | `capacitance.terminals` = every conductor group not in `ground` (group order); `capacitance.ground` = `["outer_boundary", <ground>…]`; every slab's `Re eps_r` (loss dropped); no `boundary_conditions`, ports or frequencies |
| `inductance` | path conductors **solid** (a dimension-3 group, left out of the hollowing), others sheets / hollow shells | one `inductance.paths[]` entry per path conductor (`name` = `conductor` = the group, `source` / `sink` = its contacts); `pec` = `outer_boundary` + every other conductor group (the return); vacuum (`materials` empty, `mu_r = 1`); no ports or frequencies |

The static-analysis rules hold **by construction**, checked twice:

1. **At layout validation** (`invalid_spec`, before Gmsh runs):
   capacitance rejects two conductor groups that touch (they would share
   mesh nodes — a short) unless both are in `ground`, and needs at least
   one non-ground group; inductance needs contacts, rejects a path
   conductor with a zero-thickness part, a contact whose groups share no
   face or overlap in volume, a path conductor with other than two
   contacts or with both on one `to` group, and a path conductor that
   touches any group other than its contacts' `to` groups. "Touch"
   here is conservative: shared faces, but also a shared edge or a
   single shared point, count.
2. **On the generated mesh**: the starter spec is loaded against the
   mesh exactly as `geode check` would, so a terminal / ground node
   overlap, a contact face that is not on the conductor volume, or
   contacts on two disconnected PEC components fail `geode mesh` with
   `invalid_spec` instead of producing a spec that `geode capacitance` /
   `geode inductance` would reject.

A layout meant for all three analyses therefore puts touching metal
that is one conductor in one net, and gives each end of a current path
its own return group (e.g. `via_in` / `via_out` nets).
`tests/mesh_static_golden.rs` runs both end to end against independent
references (a Kelvin guard-ring capacitor within `−0.5 % … +2.5 %`, measured
+1.15 % against Maxwell's gap-corrected value;
a shielded microstrip shorted by end caps against a 2-D reference,
measured −2.6 % from below on the default mesh; see [Golden
tests](#golden-tests)).

### Layout, schema v1

A layout (JSON or TOML, same schema; unknown fields rejected) — the
spiral-inductor golden input `tests/fixtures/spiral_layout_smoke.json`,
abridged (the fixture also has an `air_buffer` slab, the spiral legs,
the m1 underpass and the vias):

```json
{
  "schema_version": 1,
  "length_unit_m": 1e-6,
  "dielectrics": [
    { "name": "substrate",  "z_bottom": -18, "thickness": 18, "eps_r": [11.9, -0.0595] },
    { "name": "dielectric", "z_bottom": 0,   "thickness": 12, "eps_r": [4.0, -0.004] },
    { "name": "air",        "z_bottom": 12,  "thickness": 12 }
  ],
  "conductors": [
    { "name": "m2", "z_bottom": 6.5, "thickness": 3, "polygons": [
      { "name": "feed",   "outer": [[-53, -62], [-47, -62], [-47, -47], [-53, -47]] },
      { "name": "return", "outer": [[-53, -77], [-47, -77], [-47, -66], [-53, -66]] }
    ] }
  ],
  "ports": [ { "name": "port", "layer": "m2", "between": ["feed", "return"], "resistance_ohm": 50 } ],
  "margin": 18,
  "boundary": { "kind": "pec" },
  "mesh": { "size_max": 26, "size_conductor": 7, "size_port": 2.5, "near_distance": 6, "far_distance": 24 }
}
```

| Field | Type / units | Meaning |
|---|---|---|
| `schema_version` | int, must be `1` | layout schema version |
| `description` | string, optional | free text, ignored |
| `length_unit_m` | float > 0 | metres per layout unit; copied into the starter spec |
| `dielectrics[]` | ≥ 1 | the layer stack, **bottom to top, contiguous**; each slab spans the whole lateral domain |
| `dielectrics[].name` | `[A-Za-z0-9_.-]+` | volume group name (unique across every group; `outer_boundary` is reserved) |
| `dielectrics[].z_bottom`, `.thickness` | length, thickness > 0 | slab extent |
| `dielectrics[].eps_r` | `[re, im]`, `im ≤ 0`, default `[1, 0]` | complex permittivity (spec convention) |
| `conductors[]` | | conductor layers, all modelled as **PEC** |
| `conductors[].name` | name | surface group name |
| `conductors[].z_bottom` | length | strictly inside the stack |
| `conductors[].thickness` | length ≥ 0, default `0` | `0`: zero-thickness PEC **sheets** at `z_bottom`; `> 0`: closed PEC **shells** (the extruded solid is subtracted from the dielectric stack; the group is its exterior cavity walls; see "Thick conductors and Leontovich" above). Vias are conductor layers spanning the metal layers they join |
| `conductors[].polygons[]` | ≥ 1 | shapes of the layer |
| `….polygons[].name` | name, optional | unique within the layer; referenced by `ports[].between` |
| `….polygons[].net` | name, optional | puts the polygon in the conductor group named after the net (any layer) instead of the layer's group; net names share the group namespace |
| `….polygons[].outer` | `[[x, y], …]` | simple **rectilinear** ring (every edge parallel to x or y), ≥ 4 vertices, either orientation, closing vertex optional |
| `….polygons[].holes` | must be empty | accepted by the schema, rejected in v1 |
| `conductors[].mesh_size` | length > 0, optional | per-layer target size (default `mesh.size_conductor`) |
| `ports[]` | ≥ 1 for `--analysis driven`, else optional (not meshed) | lumped **gap** ports, horizontal, at the layer's mid-height |
| `ports[].name` | name | surface group name |
| `ports[].kind` | `"gap"` (default) | the only v1 kind |
| `ports[].layer` | conductor layer name | the port plane |
| `ports[].between` | `[shape, shape]` | the gap between the two shapes' bounding boxes (separated along exactly one axis, overlapping along the other), across their common extent; `e_hat` points from the first to the second |
| `ports[].rect` + `.direction` | `[x0, y0, x1, y1]` + `"x"`/`"y"` | explicit port rectangle and gap axis instead of `between` |
| `ports[].resistance_ohm` | Ω > 0, default `50` | port resistance / S-parameter reference |
| `ground` | `[group, …]`, default `[]` | conductor groups (layer or net names) that are the capacitance reference (`capacitance.ground`, with `outer_boundary`) |
| `contacts[]` | default `[]` | inductance current-path endpoints (meshed for `--analysis inductance` only) |
| `contacts[].name` | name | surface group name |
| `contacts[].conductor` | group name | the thick path conductor (kept solid for inductance) |
| `contacts[].to` | group name | the return conductor it touches; the contact is every face the two share. Two contacts per path conductor, to two different groups: source, then sink |
| `margin` | length > 0 | lateral gap between the conductor / port footprint and the outer walls |
| `boundary` | `{ "kind": "pec" }` (default) | PEC outer walls |
| | `{ "kind": "upml", "thickness": t, "sigma_0": 25 }` | matched box UPML of depth `t` (< `margin`) inside PEC walls: every slab is split conformally at the inner wall and listed in the starter spec's `absorbing_regions` (`sigma_0` default `25`) |
| `mesh.size_max` | length > 0 | global maximum (far) element size |
| `mesh.size_conductor` | length > 0 | target size on conductor surfaces |
| `mesh.size_port` | length > 0, default `size_conductor / 2` | target size on ports |
| `mesh.near_distance` | length > 0, default: the target size | distance from a conductor over which its target size holds (ports: their gap length) |
| `mesh.far_distance` | length > 0, default `margin` | distance at which the size reaches `size_max` |

**Not in v1** (natural follow-ups): polygons with holes and
non-rectilinear polygons; Leontovich / finite-conductivity conductor
models in the starter spec; wave ports and non-horizontal
ports; per-layer curvature / sloped sidewalls; GDS ingestion (owned by
klayout-tools, by design).

The report (`kind = "mesh"`) carries the provenance fields plus `layout`
/ `geo` (`path`, `sha256`), `gmsh` (`path`, `version`), `mesh` (`path`,
`sha256`, `length_unit_m`, `n_nodes`, `n_tets`, `n_triangles`),
`physical_groups[]` (`dim`, `tag`, `name`, `role` = `dielectric` \|
`pec_sheet` \| `pec_shell` \| `conductor_volume` \| `port` \| `contact`
\| `outer_boundary`, `n_elements`), `analysis` (`driven` \|
`capacitance` \| `inductance`), `starter_spec_path` and `starter_spec`.

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
| `kind` | `"check"` \| `"driven"` \| `"eigen"` \| `"extract"` \| `"capacitance"` \| `"inductance"` \| `"mesh"` \| `"error"` |
| `status` | `"ok"` \| `"error"` |

Complex numbers are `[re, im]`; matrices are row-major nested arrays
`m[row][col]` indexed by port (by terminal for the real capacitance
matrix, by current path for the real inductance matrix).

**`mesh`** (every success kind): `path`, `sha256` (hex SHA-256 of the mesh
bytes), `length_unit_m`, `n_nodes`, `n_tets`, `n_edges` (Nédélec DOFs),
`n_interior` (DOFs kept after PEC elimination = linear-system size).

**`ports[]`** (check + driven + extract): `index`, `physical_group`, `tag`,
`n_triangles`, `e_hat` (normalized), `width`, `length` (mesh units),
`geometry_derived`, `resistance_ohm`, `v_inc`.

Additive in v1 (issue #683) — always present in `check`, present in
`driven` / `extract` only when non-empty:

- **`silver_muller[]`**: `physical_group`, `tag`, `n_triangles`.
- **`absorbing_regions[]`**: `physical_group`, `tag`, `n_tets`,
  `n_tets_stretched` (tets beyond the inner wall), `thickness`, `sigma_0`,
  `air_box_lo` / `air_box_hi` (the derived inner wall, mesh units).
- **`wave_ports[]`**: `index`, `physical_group`, `tag`, `n_triangles`,
  `n_port_edges`, `n_interior_port_edges` (the modal problem size),
  `area`, `normal`, `n_modes`, `a_inc`, and `modes` — `null` in `check`
  (no modal solve), else one entry per mode: `mode`, `channel` (flat
  S-matrix index), `k_c` (rad / mesh unit), `cutoff_hz`.

**`kind = "check"`** adds `regions[]` (`physical_group`, `tag`, `n_tets`,
`eps_r`, `eps_r_source` = `"spec"` \| `"default_vacuum"`, and — additive
in v1, issue #714 — `mu_r`), `pec[]`
(`physical_group`, `tag`, `n_triangles`), `leontovich[]` (… plus
`conductivity_s_m`, `conductivity_natural`), `frequencies[]`
(`frequency_hz`, `k0`; empty for an eigen spec), `solver` (`mode`, `tol`,
`max_iters`), `analysis` (`"driven"` \| `"eigen"` \| `"extract"` \|
`"capacitance"` \| `"inductance"`),
`eigen` (`null` unless an eigen spec, else `n_modes`, `shift_hz`,
`shift_k0`, `sigma` = `shift_k0²`, `max_iters`, `tol`, `residual_tol`),
`extract` (`null` unless an extract spec, else `anchor_source` =
`"frequencies"` \| `"anchor_frequencies"`, `anchor_frequencies[]`
(`frequency_hz`, `k0`; distinct, ascending) and `l0_rel_tol`),
`capacitance` (additive in v1; `null` unless a capacitance spec, else
the block described under `kind = "capacitance"` below), `inductance`
(additive in v1; `null` unless an inductance spec, else the block
described under `kind = "inductance"` below) and `resources`
(additive in v1: the up-front resource estimate — see "Resource
estimate" above; `null` for a capacitance spec).
`ports[]` is empty for an eigen spec; for an extract spec
`frequencies[]` is the ascending solved list.

**`kind = "driven"`** adds:

- `solver`: `mode`, `tol`, `max_iters`, `iterations_max` (largest per-RHS
  Krylov count; `0` on the direct path), `residual_rel_max` (largest
  `‖Ax − b‖/‖b‖`), `wall_time_s` (assembly + all solves, seconds).
- `touchstone_file` (additive in v1; `--touchstone` only):
  `{path, sha256}` of the `.sNp` written (see "Touchstone output").
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
| `wave_channels[]` | | wave-port specs only (see below) |
| `field_file` | | `--outdir` only: `{path, sha256}` of `E_<row>.vtu` (see above) |
| `far_field` | | `--outdir` + one UPML shell only: NTFF quantities (see above) |

For a **wave-port** spec `z_ohm` and `ports` are empty and `y_s` is
`null` (no port impedance); `s` is the power-normalized channel S-matrix
(`S[k][j] = √(β_k/β_j)·(a_k − a_inc δ_kj)/a_inc,j`, reciprocal), and
`iterations` has `2·n_channels` entries (the SMW column solves, then the
excitations). `wave_channels[]`, one per channel, carries `channel`,
`port`, `mode`, `beta` (`[re, im]`: real positive when propagating,
`−j|β|` when evanescent), `propagating`, `s` (`S_kk`) and `s_db`.

**`kind = "eigen"`** adds `regions[]` and `pec[]` (as for `check`),
`eigen` (the resolved settings, as for `check`), `absorbing_regions[]`
(as for `check`; omitted when empty — additive, #706), and:

- `solver`: `method` (`"shift_invert_lanczos"`), `inner` (`"direct_lu"`),
  `pencil` (`"real_symmetric"` \| `"complex_symmetric"`, #706),
  `n_null_filtered` (Ritz values dropped as gradient nullspace),
  `n_overdamped_filtered` (complex pencil only: Ritz values with
  `Re(λ) ≤ 0`, #706), `upml_reference_k0` (with `absorbing_regions`
  only: the `k₀` the UPML was frozen at, = the shift, #706),
  `residual_rel_max`, `wall_time_s` (assembly + eigensolve, seconds).
- `modes[]`, ascending in frequency (`Re(k₀)`):

| Field | Units | Meaning |
|---|---|---|
| `index` | – | mode index |
| `lambda` | (rad / mesh unit)² | eigenvalue `k₀²` (`Re(λ)` for a complex pencil) |
| `lambda_im` | (rad / mesh unit)² | complex pencil only (#706): `Im(λ)` (`≥ 0` for passive modes) |
| `k0` | rad / mesh unit | resonant `ω/c` (`Re(k₀)` for a complex pencil) |
| `k0_im` | rad / mesh unit | complex pencil only (#706): `Im(k₀)`, `> 0` = decaying (`exp(+jωt)`) |
| `frequency_hz` | Hz | `Re(k₀) c / (2π · length_unit_m)` |
| `omega_rad_s` | rad/s | `2πf` |
| `q` | – | quality factor `Re(k₀) / (2|Im k₀|)`: `null` for a lossless pencil (`Q` undefined — infinite — not a number); finite for a lossy / open one (#706; `null` only if `|Im k₀| ≤ 1e-12`, an absolute bound in `k₀`'s rad / mesh length unit, not relative to `Re(k₀)`); uses `|Im k₀|`, so a growing mode (`k0_im < 0`) is not distinguished from decay by `Q` alone — check `k0_im`'s sign |
| `residual_rel` | – | `‖Kx − λMx‖ / (|λ| ‖Mx‖)` |
| `field_file` | | `--outdir` only: `{path, sha256}` of `E_mode_<mode>.vtu` |

**`kind = "extract"`** has the `kind = "driven"` fields (`mesh`,
`ports`, `solver`, `touchstone_file`, `results[]` — but `results[]` is **ascending** in
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

**`kind = "capacitance"`** (additive in v1, issue #705) adds `regions[]`
(as for `check`; every `eps_r[1]` is `0`) and:

- `capacitance`: `terminals[]` and `ground[]` (each `physical_group`,
  `tag`, `n_triangles`, `n_nodes` = distinct pinned nodes),
  `conductor_model` (`"non_driven_grounded"`), `element` (`"p1_tet"`),
  `n_dof` (= mesh nodes), `n_free_dof` (unknowns after pinning every
  terminal and ground node), `nnz_k` (`n_nodes + 2·n_edges`, the full
  scalar stiffness pattern), `n_solves` (one per terminal). `mesh.n_edges`
  / `n_interior` are the H(curl) counts of the other analyses and unused
  here.
- `solver`: `method` (`"energy"`), `inner` (`"direct_lu"`), `wall_time_s`
  (assembly + every solve, seconds).

| Field | Units | Meaning |
|---|---|---|
| `terminals` | – | terminal names, matrix row/column order |
| `c_farad` | F | N×N Maxwell capacitance matrix: diagonal `> 0`, off-diagonal `≤ 0` (the mutual capacitance between *i* and *j* is `−c_farad[i][j]`) |
| `c_sigma_farad` | F | per terminal, the row sum `Σ_j C_ij`: its capacitance to ground with every other terminal also grounded (a fully enclosed terminal has `≈ 0`) |
| `c_flux_diag_farad` | F | independent surface-flux cross-check of the diagonal (`∮ ε(−∇φ)·n̂ dS` with a piecewise-constant field): a looser sanity signal (~8–15 % on the curved coax), **not** the result; `null` for a terminal not entirely on the mesh boundary (e.g. a zero-thickness sheet with dielectric on both sides), where a one-sided flux would be wrong |
| `max_rel_asymmetry` | – | `max \|C_ij − C_ji\| / max(\|C_ij\|, \|C_ji\|)` — structural only: the energy method fills `C_ji` from `C_ij`, so it is `0` unless something is corrupted; it is **not** a solver residual (the solve is a direct LU) |
| `maxwell_sign_structure` | – | `true` if the diagonal is positive, the off-diagonals non-positive and the row sums non-negative (to `1e-9` relative). `false` flags a mesh violating the discrete maximum principle (badly obtuse tets); inspect the matrix — it is not an error |
| `spice_file` | – | `{ path, sha256 }` of the SPICE `.subckt` written by `--spice` (issue #715; `path` as given on the command line); omitted without the flag |

**`kind = "inductance"`** (additive in v1, issue #714) adds `regions[]`
(as for `check`, with `mu_r`) and:

- `inductance`: `paths[]` (each `name`, `conductor`, `conductor_tag`,
  `n_conductor_tets`, `source`, `n_source_triangles`, `n_source_nodes`,
  `sink`, `n_sink_triangles`, `n_sink_nodes`), `excitation`
  (`"open_path_conduction"`), `return_path` (`"pec_wall_return"`),
  `element` (`"nedelec1_tet"`), `gauge` (`"tree_cotree"`), `n_dof`
  (= `mesh.n_edges`), `n_free_dof` (= `mesh.n_interior`: edges left after
  the PEC wall and the terminal contacts, before the gauge), `n_solves`
  (one per path).
- `solver`: `method` (`"energy"`), `inner` (`"direct_lu"`),
  `solenoidal_tol` (the compatibility gate, relative), `wall_time_s`
  (conduction solves + assembly + every magnetostatic solve, seconds).

| Field | Units | Meaning |
|---|---|---|
| `paths` | – | path names, matrix row/column order |
| `l_henry` | H | N×N Maxwell inductance matrix: symmetric, positive diagonal (self inductance), off-diagonals the mutual inductances |
| `flux_linkage_diag_henry` | H | independent flux-linkage cross-check of the diagonal (`A⁽ⁱ⁾ᵀ b⁽ⁱ⁾ / I_i²`, a different contraction than the energy form; agrees to round-off) |
| `current_a` | A | Galerkin net current each path was driven with — `1` by normalisation |
| `sink_current_a` | A | Galerkin current leaving through the sink — equals `current_a` to round-off (discrete conservation) |
| `source_face_flux_a` / `sink_face_flux_a` | A | geometric face flux `∫ J·n̂ dA` through the source / sink triangles: an independent check of the 1 A normalisation (exact for a uniform current, discretisation-order close otherwise; ~0.6 % on the coarse golden mesh) |
| `max_solenoidal_residual` | – | largest discrete-divergence residual of any path's `J`, relative to its RHS norm (round-off for this construction) |
| `max_rel_asymmetry` | – | structural only (the energy method fills `L_ji` from `L_ij`) |
| `is_spd` | – | `true` if the matrix is symmetric positive definite (Cholesky); `false` flags a corrupted solve |
| `spice_file` | – | `{ path, sha256 }` of the SPICE `.subckt` (self inductors + `K` couplings) written by `--spice` (issue #719; `path` as given on the command line); omitted without the flag |

**`kind = "error"`** adds `command` (the subcommand) and
`error: { code, message }` with `code` one of `io`, `spec_parse`,
`schema_version`, `invalid_spec`, `mesh`, `unresolved_physical_group`,
`backend_mismatch`, `solve_failed` (including an electrostatic,
conduction or magnetostatic factorization failure), `non_finite`, `serialize`,
`gmsh_not_found`, `gmsh_failed` (the last two from `geode mesh`; a bad
layout reports `spec_parse` / `invalid_spec`)
(`not_implemented` was retired once `extract` went live).

## JSON Schema (`geode schema`, issue #709)

The three JSON contracts are published as **JSON Schema, draft 2020-12**
(`"$schema": "https://json-schema.org/draft/2020-12/schema"`), generated
with [`schemars`](https://docs.rs/schemars) from the same serde types the
binary parses and emits, so they cannot silently drift from it:

| Contract | Committed file | Print it | Covers |
|---|---|---|---|
| problem spec | [`schemas/spec.schema.json`](schemas/spec.schema.json) | `geode schema spec` | input of `check` / `driven` / `eigen` / `extract` / `capacitance` / `inductance` (JSON or TOML) |
| report | [`schemas/report.schema.json`](schemas/report.schema.json) | `geode schema report` | the one JSON document every subcommand but `schema` writes |
| layout | [`schemas/layout.schema.json`](schemas/layout.schema.json) | `geode schema layout` | input of `geode mesh` (JSON or TOML) |

`geode schema <kind> [-o PATH]` writes the schema itself (pretty JSON),
not a report; the files are versioned with the repository and attached to
nothing else. Field descriptions are the Rust doc comments (units,
defaults, conventions).

How serde maps onto the schema:

- `deny_unknown_fields` → `"additionalProperties": false` on every spec /
  layout object: an unknown key is schema-invalid, exactly as `geode`
  rejects it (`spec_parse`).
- `#[serde(default)]` fields are optional and carry their `default`.
- The internally tagged enums become a `oneOf` with one closed branch per
  variant, keyed by a `const` tag: `solver` (`"mode": "direct" |
  "iterative"`) and the layout's `boundary` (`"kind": "pec" | "upml"`).
- `schema_version` is pinned with `"const": 1` in all three.
- The spec and layout schemas describe what a caller may **write**
  (defaulted fields optional); the report schema describes what `geode`
  **writes** — every field it always emits is `required` (nullable ones as
  `[T, "null"]`), fields present only with a flag (`touchstone_file`,
  `spice_file`, `field_file`, …) are optional.
- The report schema is a hand-assembled `oneOf` over the eight kinds
  (`check`, `driven`, `eigen`, `extract`, `capacitance`, `inductance`,
  `mesh`, `error`). Each branch pins `kind` — and `status` (`"ok"`, or
  `"error"` for `kind = "error"`) — with a `const`, so exactly one branch
  matches a report. Report objects are **open** (no
  `additionalProperties: false`): fields added within report schema v1 are
  additive, and consumers should ignore keys they do not know.

**Schema-valid is not `geode check`-valid.** The schemas are structural.
The cross-field and semantic rules are enforced by the binary only
(`problem::load` for specs, layout resolution for `geode mesh`) and are not
expressed in the schemas: which sections each analysis requires or
forbids (see [Problem spec](#problem-spec-schema-v1)), at most one of
`eigen` / `extract` / `capacitance` / `inductance`, `mu_r ≠ 1` only in an
inductance spec, a physical group in at most one role, value ranges
(`> 0`, `im ≤ 0`, `≥ 2` anchors, …), `values` **or**
`start`/`stop`/`count`, `between` **or** `rect`, and every name resolving
against the mesh or layout. Use a schema to catch typos and type errors
early (editors, CI, agents); run `geode check` (spec) or `geode mesh`
(layout) as the authority.

Other known limits of v1: numeric fields carry informative `format`
annotations (`double`, `uint`, `uint32`) that validators treat as
annotations, not assertions; the `minimum: 0` on integer fields is the
only range the schemas enforce. JSON Schema's `"type": "integer"` accepts
any number with a zero fractional part (`3.0`, `"schema_version": 1.0`);
`geode`'s parser is stricter and rejects these with `spec_parse` — this is
standard JSON Schema semantics, not a generator defect.

Checked in CI by `tests/schema.rs`: the committed files equal what the
binary generates (drift guard), they are valid 2020-12 schemas, every
committed spec and layout (`tests/fixtures/`, `examples/`) validates, the
schema rejects the serde-rejected cases `geode check` rejects, and real
reports of every kind validate against the report schema
(`tests/cookbook.rs` covers the `mesh` report). After changing a spec,
layout or report type, regenerate from the repository root and review the
diff — a schema change is a contract change:

```sh
for k in spec report layout; do
  cargo run -q -p geode-cli -- schema $k -o crates/geode-cli/schemas/$k.schema.json
done
```

## Examples cookbook (`examples/`, issue #709)

[`examples/`](examples/README.md) has one runnable example per analysis —
`driven`, `extract`, `eigen`, `capacitance`, `inductance`, and `mesh` →
driven / capacitance / inductance — each with a README giving the command
line, what it models and the output to expect. The specs (and the spiral
layout) are byte-identical copies of the golden-test fixtures, so the
quoted numbers are the pinned ones; `tests/cookbook.rs` fails if a copy
drifts, runs `geode check` on every cookbook spec, and (with Gmsh) runs
`geode mesh` on every cookbook layout and `geode check` on the resulting
starter spec. (`crates/geode-cli/examples/` holds only JSON / TOML and
Markdown — no `cargo run --example` targets.)

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

`tests/mesh_golden.rs` (issue #704) re-expresses the spiral inductor as a
**layout** (`tests/fixtures/spiral_layout_{smoke,benchmark}.json`: the
`reference/gmsh/spiral_3p5_{smoke,generic}.yaml` geometries as PEC-shell
m1 / m2 / via layers with a gap port between the feed and return stubs)
and runs `geode mesh` → `geode check` → `geode driven`:

```sh
cargo test -p geode-cli --test mesh_golden                                        # default CI (needs gmsh)
cargo test -p geode-cli --release --test mesh_golden -- --include-ignored         # + benchmark oracle bands
```

A Gmsh-generated mesh is not node-identical to the committed fixtures,
so the per-point 1 % bands above do not apply. The default tier checks
the generated groups / roles / counts, `geode check` on the starter spec,
a tiny two-pad UPML layout driven end to end on its **unedited** starter
spec (and re-meshed to identical bytes), a two-slab layout whose thick
trace and via cross the slab interface (issue #721: no tet inside a
thick conductor, conductor triangles only on cavity walls, none at the
interface, the thin ground sheet still two-sided), and — with the
benchmark's Leontovich copper swapped onto the generated conductor
groups — the smoke spiral's 1 GHz L within 3 % and R / Q within 2 % of
`results_smoke.toml` (sanity bands, not the #211 oracle bands; measured
+0.92 % L / +0.37 % R / +0.56 % Q on Gmsh 4.15.2 with hollowed
conductors, see [Layout → mesh](#layout--mesh-geode-mesh-issue-704)). The ignored tier holds the generic spiral's 1 GHz
L to the issue-#211 oracle bands (Mohan current-sheet 10 %, projected
mom-PEEC mean 12 %, inside the mom bracket), its L to 2 % and R / Q to
2 % of the committed `results.toml`, and the unedited PEC starter spec
to 2 % of the same PEC model on the committed `spiral_3p5.msh`. Gmsh-dependent tests skip with
a loud banner when no `gmsh` is runnable; CI installs Gmsh and sets
`GEODE_REQUIRE_GMSH=1`, which turns a skip into a failure.

`tests/mesh_static_golden.rs` (issue #720) runs layout → `geode mesh
--analysis capacitance | inductance` → the **unedited** starter spec →
`geode check` → `geode capacitance` / `geode inductance` with `--spice`:

```sh
cargo test -p geode-cli --test mesh_static_golden                                 # default CI (needs gmsh)
cargo test -p geode-cli --release --test mesh_static_golden -- --include-ignored  # + convergence tier
```

- **Capacitance: Kelvin guard-ring capacitor.** A 1 mm plate and a
  coplanar guard ring (0.1 mm gap, 1.5 mm wide) — two **nets** of one
  sheet layer — 1 mm over a grounded bottom plate (`ε_r = 4` slab) and
  1 mm under the grounded lid. The plate's row sum `C₁₁ + C₁₂` (plate and
  guard at one potential; its `c_sigma_farad` and SPICE ground branch)
  is held to Maxwell's guarded-plate value with his gap correction,
  `ε₀ [ε_r (w+g−2α_d)²/d + (w+g−2α_h)²/h]`, `α_s = (2s/π) ln cosh(πg/4s)`
  (*Treatise* Arts. 196–201: each effective edge sits `α` inside the gap
  midpoint; here `α ≈ 0.002 mm`, −0.71 % in area vs the uncorrected
  `(w+g)²`), within **`−0.5 % … +2.5 %`**. Measured +1.15 % on 7 835 nodes
  (Gmsh 4.15.2); the ignored tier checks monotone convergence from above,
  +1.15 / +0.64 / +0.22 % as the plate-layer size goes 0.1 / 0.05 /
  0.025 mm (7.8k / 26k / 97k nodes), and the finest within 0.5 %. The
  corrected rule's residual is second order (plate corners,
  `O((g/w)(α/w))` ≈ 0.02 %; the guard's far edge, `e^{−πb/d}`-suppressed);
  ignoring the gap would miss by 21 %.
- **Inductance: shielded microstrip shorted by end caps.** A thick trace
  0.5 mm over the ground plane of a rectangular PEC shield, its end faces
  touching two end caps (**nets** of the wall layer) — the two
  **contacts**. Perpendicular PEC caps make the field exactly the 2-D
  cross-section field, so `L = L′ℓ` with no end effect; `L′` (external
  plus internal, uniform DC current) comes from an independent in-test
  2-D finite-difference solve, Richardson-extrapolated. The energy method
  converges from below: the default tier holds `−5 % < ΔL/L < +0.5 %`
  (measured −2.60 % on 3 624 nodes); the ignored tier checks monotone
  convergence −2.60 / −1.26 / −0.75 % on 3.6k / 9.2k / 23k nodes and
  the finest within 1.5 %.
- `--analysis driven` is the default byte for byte (script, mesh,
  spec), and the static-analysis layout errors fail before Gmsh runs.

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
`examples/mie_sphere` UPML benchmark: that example's open-cavity
quasi-modes use a spherical-shell PML, which the box `absorbing_regions`
cannot express (issue #706).

`tests/sphere_lossy_pec_golden.rs` (issue #706) runs the same mesh
uniformly filled with `ε_r = 2.25(1 − j tan δ)` through `geode eigen`
at `tan δ = 0.01` (the committed fixture, with `--outdir`) and `0.1`,
and holds every mode to the **exact** uniform-fill relations
`Im(λ)/Re(λ) = tan δ` and `Q = ½·cot(δ/2)` at `10⁻⁶` relative (measured
`4e-8` / `2.6e-8`, the f32 weight upload), plus decay sign, complex
`E_real` / `E_imag` export and a loss-independent `|λ|·|ε_r|` across the
two loss levels. `tests/eigen_upml_smoke.rs` is the smoke tier for
`absorbing_regions` in an eigen spec (no analytic oracle; see
[lossy / open eigen](#lossy--open-cavity-eigenmodes-issue-706)):

```sh
cargo test -p geode-cli --test sphere_lossy_pec_golden --test eigen_upml_smoke
```

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

`tests/patch_extract_golden.rs` (issue #683) re-expresses the probe-fed
FR-4 patch antenna (`examples/patch_antenna`, issue #228 — the repo's
driven open radiator) as an extract spec with an `absorbing_regions`
box-UPML shell and runs `geode extract`:

```sh
cargo test -p geode-cli --test patch_extract_golden                               # smoke mesh, default CI
cargo test -p geode-cli --release --test patch_extract_golden -- --ignored        # 30.6k-edge benchmark mesh
```

`f_res` is the first `Im Z = 0` crossing, i.e. `extraction[0].srf_hz`
(`l0_h` is meaningless for an open radiator and is not asserted). The
smoke tier (`patch_2g4_smoke.msh`, different geometry) checks passivity
(`Re Z ≥ 0`, `|S11| ≤ 1`), convergence and CLI vs library `Z` parity
(1e-9) against the fixture's own `matched_upml_materials` path — once
with the PEC outer wall and once with it swapped for a Silver-Müller
wall. The ignored tier sweeps `examples/patch_antenna`'s 13 points over
2.0–3.0 GHz and holds `f_res` to the Balanis cavity model
(`geode_core::analytic::patch::PatchCavity`) within the repo's existing
calibrated **8 %** band (`geode-core/tests/patch_antenna_extraction.rs`;
the FEM resonance, 2.2745 GHz, sits −6.5 % below the ~3–5 %-class cavity
model's 2.4332 GHz), requires an interior S11 dip ≤ −3 dB, and checks
library parity at the dip. It does not read
`benchmarks/patch_antenna/results.toml`. A third smoke test (issue #684)
runs the extract with `--outdir`: every `{path, sha256}` reference
matches its file, the `.vtu` `E_real` / `E_imag` round-trip against a
fresh library solve + `edge_field_to_nodes` (1e-9 of max |E|), and the
`far_field` directivity / gain / efficiency match an in-process
`geode_core::postproc::ntff` computation built the
`examples/patch_antenna` way (1e-9 relative). `sphere_pec_golden.rs` runs
its default tier with `--outdir` (one finite, real `E_mode_<i>.vtu` per
mode) and `wave_port_driven.rs` checks that wave-port reports carry no
export fields even with `--outdir`.

`tests/capacitance_golden.rs` (issue #705) re-expresses the
`benchmarks/electrostatic` coax oracle through `geode capacitance` on a
committed Gmsh mesh (`reference/gmsh/coax_capacitance.geo` →
`geode-core/tests/fixtures/coax_capacitance{_smoke,}.msh`: a triaxial
coax, radii 1 / 1.6 / 2.5 mm, natural end caps):

```sh
cargo test -p geode-cli --test capacitance_golden                                 # both tiers, default CI (~5 s debug)
```

Both tiers run by default (the static solve is cheap). The **coax** spec
(terminal `inner`, ground `outer`, the `shield` surface left unlisted) is
exactly the benchmark coax and is held to its **1 %** bar against
`2πε₀L/ln(b/a)` (smoke 0.32 %, benchmark 0.14 %); the **triax** spec
(terminals `inner` + `shield`, `ε_r = 2` in the inner annulus) is held to
1 % per entry of the analytic 2×2 Maxwell matrix `[[C1, −C1], [−C1,
C1 + C2]]` (smoke ≤ 0.40 %, benchmark ≤ 0.19 %). Also: CLI-vs-library
parity (1e-9, the library driven straight from the named groups),
monotone convergence smoke → benchmark, the flux cross-check band, exact
`ε_r` linearity, the report / `check` contract, and the validation
errors (wrong subcommand pre-mesh, frequency-domain features, lossy
`ε_r`, shorted conductors, unknown groups).

`tests/inductance_golden.rs` (issue #714) re-expresses the
`benchmarks/magnetostatic_inductance` coax oracle through `geode
inductance` on a committed Gmsh mesh (`reference/gmsh/coax_inductance.geo`
→ `geode-core/tests/fixtures/coax_inductance{_smoke,}.msh`: a solid core
`a = 1`, a tube `1.5 < r < 2.2`, a PEC shield `b = 3` mm, every region
meshed, PEC end caps):

```sh
cargo test -p geode-cli --test inductance_golden                                  # smoke tier, default CI
cargo test -p geode-cli --test inductance_golden -- --ignored                     # benchmark tier (~1 min debug)
```

The **coax** spec (one path through the core; the tube's end faces
listed as PEC) is exactly the benchmark coax and is held to its **1 %**
bar against `μ₀L/(2π) [ln(b/a) + 1/4]` (smoke 0.46 %, benchmark
0.16 %); the **triax** spec (paths `core` + `tube`) is held to 1 % per
entry of the closed-form 2×2 matrix from the enclosed-current field
(smoke ≤ 0.83 %, benchmark ≤ 0.27 %). Also: CLI-vs-library parity
(1e-9), monotone convergence smoke → benchmark, exact `μ_r` linearity and
the `μ_r = 4` core closed form `μ₀L/(2π)[ln(b/a) + μ_r/4]`, the report /
`check` contract (round-off solenoidality, conservation, flux linkage),
and the validation errors (wrong subcommand
pre-mesh, frequency-domain features, missing PEC wall, `source == sink`,
shared conductors, terminals off the conductor or off the PEC wall,
`mu_r` outside inductance).

`tests/spice_golden.rs` (issue #715) runs `geode capacitance --spice`
on the capacitance coax / triax smoke specs: the `spice_file` hash, the triax shielded ground
branch dropped and the other two kept, the network re-read into the
report's Maxwell matrix, and — only when an `ngspice` binary is runnable
(`GEODE_NGSPICE`, else `PATH`; otherwise a loud `SKIPPED` on stderr, CI
has none) — an ngspice AC admittance check of every matrix entry.

`tests/inductance_spice_golden.rs` (issue #719) runs `geode inductance
--spice` on the inductance coax (one `L`, no `K`) and triax (two `L` +
one kept `K`) smoke specs, plus the triax with the tube's source / sink
swapped (same node order, negative `k`): the `spice_file` hash, the
element shape, the network re-read into the report's `l_henry`, and —
only when ngspice is runnable, as above — an ngspice AC check driving
each path with 1 A and reading every `L_jk` from the open-circuit node
voltages (including the negative mutual). The synthetic drop
(`|k| < 1e-9`) and reject (`|k| ≥ 1`, non-positive `L_ii`) paths are
unit tests in `src/spice.rs`.

`tests/wave_port_driven.rs` (issue #683) runs `geode driven` with two
wave ports on a synthetic tagged rectangular waveguide (geode-core's
extruded section written as MSH 4.1): the channel S-matrix matches an
in-process tag-built `solve_wave_port_sweep` to 1e-12, and the straight
section meets geode-core's `S₂₁ ≈ e^{−jβL}` acceptance. The projection
primitive itself is validated in `geode-core/tests/wave_port_from_tags.rs`
(cutoffs vs the hand-built cross-section to 1e-9 and the analytic
rectangular cutoffs, `S_p`-orthonormality, S-matrix parity with the
hand-built ports).
