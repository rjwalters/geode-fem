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
geode driven patch.json --outdir fields/            # + per-frequency E-field .vtu and NTFF
geode driven filter.json --touchstone filter.s2p    # + Touchstone 2.0 S-parameters
geode mesh layout.json --mesh-out m.msh --spec-out s.json && geode driven s.json   # layout → mesh → solve
```

| Subcommand | Status |
|---|---|
| `check`   | live — parse + validate spec, load mesh, resolve every named physical group, report DOF counts and an order-of-magnitude memory / cost estimate (#703); never solves |
| `driven`  | live — lumped-port (or wave-port, #683) frequency sweep with PEC / Leontovich / Silver-Müller BCs and matched box-UPML absorbing regions (#683), direct LU or COCG; optional Touchstone 2.0 `.sNp` (#703) |
| `eigen`   | live (#681) — lossless PEC-cavity eigenmodes near a shift frequency, sparse shift-invert Lanczos; `Q` is `null` (lossless) |
| `extract` | live (#682) — the `driven` sweep post-processed per port into L / R / Q, the quasi-static `L₀` (f → 0 Richardson extrapolation, with a consistency error estimate and an optional convergence gate) and the SRF |
| `mesh`    | live (#704) — layout (2-D rectilinear polygons + layer stack, JSON/TOML) → tagged Gmsh MSH 4.1 mesh with automatically named physical groups + a starter problem spec, via the external `gmsh` binary (see [Layout → mesh](#layout--mesh-geode-mesh-issue-704)) |

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
both through the same schema.

A spec describes **one** analysis, decided by which optional analysis
section it carries:

- **driven spec** (no `eigen`, no `extract`): needs `ports` (≥ 1) **or**
  `wave_ports` (≥ 1), and `frequencies`; run with `geode driven`.
- **eigen spec** (has `eigen`): must not have `ports`, `wave_ports`,
  `frequencies`, Leontovich or Silver-Müller walls or
  `absorbing_regions`, and every `eps_r` must be real (`im = 0`); run with
  `geode eigen`.
- **extract spec** (has `extract`): exactly a driven spec plus the
  `extract` section (`"extract": {}` takes every default) — needs lumped
  `ports` (≥ 1; wave ports define no `Z_kk`, so they are rejected),
  `frequencies`, and ≥ 2 distinct `L₀` anchor frequencies; run with
  `geode extract`.

Each **dimension-2** physical group carries at most one role — lumped
port, wave port, PEC, Leontovich or Silver-Müller; ports, wave ports and
impedance walls may not be listed twice (repeating a PEC name is
harmless).

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
| `boundary_conditions.silver_muller[]` | strings, driven / extract only | **dimension-2** groups carrying the first-order Silver-Müller absorbing condition — the impedance wall with `Z_s = η₀` (no parameters). Composes with Leontovich walls, PEC and `absorbing_regions` |
| `absorbing_regions[]` | driven / extract only | matched (full Sacks) **box UPML** shells, see below |
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

Not in schema v1: the matrix-free iterative solver, field / NTFF export,
and lossy / open-cavity eigenmodes (complex `ε_r`, Leontovich / absorbing
walls in an eigen spec → finite `Q`) — the latter needs a complex
non-Hermitian quasi-mode eigensolve and is a future eigen-analysis phase,
not part of the driven open-boundary support above.

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
| `eigen` | `E_mode_<mode>.vtu` — the real, `M_ε`-normalized eigenvector (arbitrary overall sign; PEC edges 0) | `modes[].field_file` |
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
sweep's S-parameters as a **Touchstone 2.0** file at `<PATH>` (the
parent directory must exist; an existing file is overwritten).
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
  bit for bit. `Z` / `Y` / `MA` / `DB` are not offered.
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

## Resource estimate (`geode check`, issue #703)

`geode check` reports a `resources` block so an agent can pick mesh and
solver before a long run. **It is an order-of-magnitude estimate from a
single measured anchor, not a prediction** — read the caveats.

| Field | Units | Meaning |
|---|---|---|
| `solver_mode` | – | `"direct"` \| `"iterative"` (eigen specs: always `"direct"`) |
| `scalar` | – | `"complex"` (driven / extract pencil) \| `"real"` (eigen pencil) |
| `nnz_a` | – | non-zeros of the full Nédélec system pattern (before PEC elimination — the anchor's convention); computed from the mesh, no assembly |
| `n_factorizations` | – | LU factorizations: one per frequency (driven / extract direct), one (eigen), `0` (iterative) |
| `n_rhs_per_frequency` | – | ports, or `2 × channels` for wave ports; `0` for eigen |
| `peak_memory_gb` | GB (10⁹ B) | estimated peak resident memory |
| `wall_time_s` / `wall_time_per_factorization_s` | s | direct only (`null` iterative) |
| `flops_per_iteration` / `flops_max` | flop | iterative only: one complex SpMV + vector updates; worst case at `max_iters` for every RHS and frequency |
| `peak_memory_confidence` | – | `"order_of_magnitude"` |
| `wall_time_confidence` | – | `"conservative_below_anchor"` (direct), `null` (iterative) |
| `calibration_basis` | – | the anchor, its date and the scaling assumptions (machine-readable, updated on re-calibration) |

**Direct model.** Peak memory and wall time scale **linearly in
`nnz(A)`** from one measurement (2026-07-15, the 1 157 564-DOF transmon
eigen run: `nnz(A)` = 20 467 522, COLAMD + faer supernodal LU, 565.5 s
wall, 92 166 884 KiB ≈ 94.4 GB peak RSS on a 128 GB cloud box;
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
  anchor as a floor, not a ceiling.
- **Why not a symbolic-fill model**: a fill-reducing ordering that won
  on *symbolic* fill (`nnz(L)`) was **OOM-killed at 128.5 GB** by the
  real supernodal LU at the anchor scale (same log), so symbolic fill is
  not used as the primary predictor. A symbolic-fill refinement is a
  possible follow-up, not this model.

**Iterative model.** Memory is a vector count (operator storage, 16
Krylov vectors, per-tet assembly buffers), not an extrapolation — on the
spiral smoke mesh it came out ~1.6× **low** (0.044 vs 0.072 GB; process
and mesh overhead are not modelled). Cost is reported in flops only:
there is **no measured iterative wall-time anchor**, and iteration
counts depend on the problem and preconditioner (they cannot be
predicted from the mesh).

## Layout → mesh (`geode mesh`, issue #704)

```sh
geode mesh <layout.json|layout.toml> [--mesh-out mesh.msh] [--spec-out spec.json] [--gmsh PATH] [-o report.json]
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
   version and the SHA-256 of the layout, script and mesh;
4. read the mesh back through `geode_core::mesh::read_tagged_tet_mesh`
   and fail with `gmsh_failed` unless every generated group has elements
   and every tet is tagged;
5. emit a **starter problem spec** (spec schema v1) wired to the
   generated names: every slab's `eps_r`, `pec = ["outer_boundary",
   <conductor layers>…]`, one lumped port per layout port (explicit
   `e_hat` / `width` / `length`), `absorbing_regions` for a UPML
   boundary, direct solver, and a **placeholder 1 GHz** frequency list to
   edit. It is always in the report (`starter_spec`); `--spec-out` also
   writes it, with `mesh.path` relative to the spec when both share a
   directory. `-o` is the report, as for every subcommand; the mesh
   defaults to the layout path with a `.msh` extension.

Automatically named physical groups:

| dim | name | tag | from |
|---|---|---|---|
| 3 | `<dielectric name>` | 1, 2, … (stack order) | each dielectric slab (conductor-shell interiors included) |
| 2 | `<conductor layer name>` | 101, … | every face of the layer's sheets / shells (PEC) |
| 2 | `<port name>` | next | the port rectangle |
| 2 | `outer_boundary` | last | the six outer walls (PEC) |

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
| `conductors[].thickness` | length ≥ 0, default `0` | `0`: zero-thickness PEC **sheets** at `z_bottom`; `> 0`: closed PEC **shells** (every face of the extruded solid; the interior stays meshed and is field-free under PEC). Vias are conductor layers spanning the metal layers they join |
| `conductors[].polygons[]` | ≥ 1 | shapes of the layer |
| `….polygons[].name` | name, optional | unique within the layer; referenced by `ports[].between` |
| `….polygons[].outer` | `[[x, y], …]` | simple **rectilinear** ring (every edge parallel to x or y), ≥ 4 vertices, either orientation, closing vertex optional |
| `….polygons[].holes` | must be empty | accepted by the schema, rejected in v1 |
| `conductors[].mesh_size` | length > 0, optional | per-layer target size (default `mesh.size_conductor`) |
| `ports[]` | ≥ 1 | lumped **gap** ports, horizontal, at the layer's mid-height |
| `ports[].name` | name | surface group name |
| `ports[].kind` | `"gap"` (default) | the only v1 kind |
| `ports[].layer` | conductor layer name | the port plane |
| `ports[].between` | `[shape, shape]` | the gap between the two shapes' bounding boxes (separated along exactly one axis, overlapping along the other), across their common extent; `e_hat` points from the first to the second |
| `ports[].rect` + `.direction` | `[x0, y0, x1, y1]` + `"x"`/`"y"` | explicit port rectangle and gap axis instead of `between` |
| `ports[].resistance_ohm` | Ω > 0, default `50` | port resistance / S-parameter reference |
| `margin` | length > 0 | lateral gap between the conductor / port footprint and the outer walls |
| `boundary` | `{ "kind": "pec" }` (default) | PEC outer walls |
| | `{ "kind": "upml", "thickness": t, "sigma_0": 25 }` | matched box UPML of depth `t` (< `margin`) inside PEC walls: every slab is split conformally at the inner wall and listed in the starter spec's `absorbing_regions` (`sigma_0` default `25`) |
| `mesh.size_max` | length > 0 | global maximum (far) element size |
| `mesh.size_conductor` | length > 0 | target size on conductor surfaces |
| `mesh.size_port` | length > 0, default `size_conductor / 2` | target size on ports |
| `mesh.near_distance` | length > 0, default: the target size | distance from a conductor over which its target size holds (ports: their gap length) |
| `mesh.far_distance` | length > 0, default `margin` | distance at which the size reaches `size_max` |

**Not in v1** (natural follow-ups): polygons with holes and
non-rectilinear polygons; thick conductors with the interior excluded
(boolean-subtracted cavity) and Leontovich / finite-conductivity
conductor models in the starter spec; wave ports and non-horizontal
ports; per-layer curvature / sloped sidewalls; GDS ingestion (owned by
klayout-tools, by design).

The report (`kind = "mesh"`) carries the provenance fields plus `layout`
/ `geo` (`path`, `sha256`), `gmsh` (`path`, `version`), `mesh` (`path`,
`sha256`, `length_unit_m`, `n_nodes`, `n_tets`, `n_triangles`),
`physical_groups[]` (`dim`, `tag`, `name`, `role` = `dielectric` \|
`pec_sheet` \| `pec_shell` \| `port` \| `outer_boundary`, `n_elements`),
`starter_spec_path` and `starter_spec`.

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
| `kind` | `"check"` \| `"driven"` \| `"eigen"` \| `"extract"` \| `"mesh"` \| `"error"` |
| `status` | `"ok"` \| `"error"` |

Complex numbers are `[re, im]`; matrices are row-major nested arrays
`m[row][col]` indexed by port.

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
`eps_r`, `eps_r_source` = `"spec"` \| `"default_vacuum"`), `pec[]`
(`physical_group`, `tag`, `n_triangles`), `leontovich[]` (… plus
`conductivity_s_m`, `conductivity_natural`), `frequencies[]`
(`frequency_hz`, `k0`; empty for an eigen spec), `solver` (`mode`, `tol`,
`max_iters`), `analysis` (`"driven"` \| `"eigen"` \| `"extract"`),
`eigen` (`null` unless an eigen spec, else `n_modes`, `shift_hz`,
`shift_k0`, `sigma` = `shift_k0²`, `max_iters`, `tol`, `residual_tol`),
`extract` (`null` unless an extract spec, else `anchor_source` =
`"frequencies"` \| `"anchor_frequencies"`, `anchor_frequencies[]`
(`frequency_hz`, `k0`; distinct, ascending) and `l0_rel_tol`) and
`resources` (additive in v1: the up-front resource estimate — see
"Resource estimate" above).
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
| `q` | – | quality factor — **always `null`** in this build: the pencil is lossless, so `Q` is undefined (infinite), not a number. Finite `Q` needs lossy / open-cavity eigenmodes (a future eigen-analysis phase) |
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

**`kind = "error"`** adds `command` (the subcommand) and
`error: { code, message }` with `code` one of `io`, `spec_parse`,
`schema_version`, `invalid_spec`, `mesh`, `unresolved_physical_group`,
`backend_mismatch`, `solve_failed`, `non_finite`, `serialize`,
`gmsh_not_found`, `gmsh_failed` (the last two from `geode mesh`; a bad
layout reports `spec_parse` / `invalid_spec`)
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
spec (and re-meshed to identical bytes), and — with the benchmark's
Leontovich copper swapped onto the generated conductor groups — the
smoke spiral's 1 GHz L within 3 % and R within 15 % of
`results_smoke.toml`. The ignored tier holds the generic spiral's 1 GHz
L to the issue-#211 oracle bands (Mohan current-sheet 10 %, projected
mom-PEEC mean 12 %, inside the mom bracket) and to 2 % of the committed
`results.toml`, and the unedited PEC starter spec to 2 % of the same PEC
model on the committed `spiral_3p5.msh`. Gmsh-dependent tests skip with
a loud banner when no `gmsh` is runnable; CI installs Gmsh and sets
`GEODE_REQUIRE_GMSH=1`, which turns a skip into a failure.

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
`examples/mie_sphere` UPML benchmark: an open-cavity quasi-mode needs a
complex non-Hermitian eigensolve (a future eigen-analysis phase; the
driven UPML support of issue #683 does not provide it).

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

`tests/wave_port_driven.rs` (issue #683) runs `geode driven` with two
wave ports on a synthetic tagged rectangular waveguide (geode-core's
extruded section written as MSH 4.1): the channel S-matrix matches an
in-process tag-built `solve_wave_port_sweep` to 1e-12, and the straight
section meets geode-core's `S₂₁ ≈ e^{−jβL}` acceptance. The projection
primitive itself is validated in `geode-core/tests/wave_port_from_tags.rs`
(cutoffs vs the hand-built cross-section to 1e-9 and the analytic
rectangular cutoffs, `S_p`-orthonormality, S-matrix parity with the
hand-built ports).
