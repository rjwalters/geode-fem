# Motor experiment 1: validating the optimization target

Issue #1181 covers experiment 1 of
`papers/motor-gradient-optimization/EXPERIMENTS.md`. The question is
whether the improvement from the initial design (θ0) to Opt120 survives
refinement in five directions: mesh level, coupling model, angle grid,
current, and mesh source. The answer is used to propose an error margin
and a feasibility margin that M4/M5 can freeze before any optimizer is
compared. The margins are proposals; the paper and the operator decide
them. No optimization was run here.

Every statement below is bounded by the runs in `runs/`. A statement
labelled *Hypothesis* has not been tested.

## Verdict

**Observed.**

- **Ordering holds; the margin depends on the coupling model.**
  - Opt120 has the lower training objective in every cell where both
    designs ran: L0–L3, h45, h93 and conforming, all four angle grids,
    training and withheld currents.
  - The ratio θ0/Opt120 of `F_train` (fixed current, g20) is 7600 at
    h45 L3, 2450 at h93 L3 and 2440 at conforming L3.
  - Going from 45 to 93 does not reorder the two designs, but it
    multiplies Opt120's `F_train` by 3.6: 1.07e-6 becomes 3.86e-6 N m²
    at L3. θ0's rises by only 16%, from 8.17e-3 to 9.48e-3.
  - At full current, Opt120's loaded std rises from 0.00066 N m (h45)
    to 0.00164 (h93) and 0.00165 (conforming). Its cogging std falls from
    0.00061 to 0.00035.
  - Conforming and h93 agree on `F_train` to within 2.6% at L3 for both
    designs.
- **Spatial refinement.** Across all fresh and multipatch families and
  all couplings,
  `|F_train(L3) − F_train(L2)| / F_train` is at most 1.4% for θ0 and at
  most 3.2% for Opt120. For Opt120 the increments do not shrink:
  L1→L2 is +3.05e-8 and L2→L3 is +2.90e-8 (h45, fresh export mesh). L3
  is therefore not shown to be in the asymptotic range for Opt120. For
  the loaded mean torque, L3−L2 is at most 1.3e-3 relative (conforming)
  and about 1e-4 relative for the harmonic couplings.
- **Angle grids.**
  - At L2–L3, g20, g40 and the 0.5°-shifted g20s agree on `F_train`
    to within 1.4%. Under the harmonic couplings they agree to within
    0.3%. At L0–L1, g20s−g20 reaches 3.8% (conforming L0) and 0.9%
    (harmonic).
  - g10 is the reference optimizer's grid
    (`history_optimization_angles_degrees` = 0, 2, …, 18). It
    underestimates Opt120's `F_train` at L3 against g40 by 1.0% (h45),
    5.9% (h93) and 17% (conforming). For θ0 the shortfall is 0.02% to
    1.0%.
- **Rotation support.** Every mesh supports every selected angle,
  already at L0. The coupling step is `0.5°/2^L`, so all of g10, g20,
  g40 and g20s (all multiples of 0.5°) fall on the grid at every level,
  for all 19 meshes (`tables.md`, Rotation support). Contrary to the
  issue's expectation, L ≥ 1 is not needed. L ≥ 1 would be needed for a
  0.25° shift.
- **Moved mesh vs remeshing (Opt120).**
  - The moved mesh is valid at L0–L2. Its minimum angle is 12.4°, 2.2°
    and 1.3°, and its smallest area ratio at L2 is 0.016.
  - At L3 it inverts 4 triangles. For each coupling, that L3 case is
    recorded as `invalid_mesh` with no field.
  - At L2, h45, moved and fresh export differ by 1.3e-4 N m in the
    loaded mean and by 0.55% in `F_train`. Under the conforming band the
    moved mesh's `F_train` is 9.4% low (3.56e-6 against 3.93e-6).
  - The fresh construction mesh (GEODE's NURBS curves, any design
    vector) and the fresh export mesh agree on `F_train` to 0.3% (h45 L3)
    and 1.4% (conforming L3).
  - Remeshing changes the topology. At L2 the moved mesh has 36083 nodes
    and 70757 triangles; the fresh export mesh has 35462 and 69529. At
    L3 the fresh export mesh has 136892 nodes and 271006 triangles,
    against 139268 and 275736 for the base.
  - Remesh cost at L3 is 7.4 s for the export mesh (gmsh through uv,
    with startup included). The mesh motion takes 1.9 s at L3 and 0.40 s
    at L2.
- **Current rule decides Opt120's torque feasibility.** All values are
  g20, h45 at L3, full current.

  | rule | Opt120 mean | against θ0's 1.4433 N m |
  |---|---|---|
  | fixed peak current (primary) | 1.3147 N m | −8.9% |
  | fixed physical current | 1.2550 N m | −13% |
  | mesh-area Jinit | 1.4983 N m | +3.8% |
  | saved reference current | 1.4986 N m | +3.8% |

  The mesh-area Jinit current converges to the saved current, from
  10.939 A at L0 to 11.046 A at L3 against 11.048 A. The 1% shortfall at
  L0 is the group-1 mesh-area error.

  A mean-torque floor set at θ0's mean would therefore make Opt120
  infeasible by about 9% under the primary rule. That is far beyond any
  margin below.
- **Paired estimators on the same harmonic field.**
  - Coupling torque and the outer-band Arkkio (44.5–45.0 mm) agree at L3
    to 1.3e-4 N m in the mean and 1e-5 in the std, for both designs.
  - The both-band Arkkio does not work for Opt120. The free-form rotor
    offsets make the inner gap-band region 2.24 times its nominal
    annulus, and the both-band value is 2.30 N m against 1.31 N m. The
    tables mark it `invalid`.
  - The ring-sampled estimator agrees with coupling torque only to about
    2e-3 N m in the mean at L3 (3e-3 at L2), so it is a weaker
    cross-check.
- **Reproduction.**
  - θ0 h45 reproduces the committed `forward_harmonic/runs/L{0..3}_h45`
    torques to at most 5.0e-7 N m.
  - Opt120 moved, with the saved current, reproduces
    `parametric_cp/runs/opt120cp_L{0,1,2}_h45` to at most 1.9e-7 N m.
  - The `file:` and `vector:` design inputs reproduce the `opt120`
    record bit for bit (`slotcheck_*`).

**Hypotheses (not tested here).**

- Opt120's very low h45 ripple may partly come from optimizing against
  the truncated coupling. The modes above order 45 that h45 does not
  transmit carry loaded ripple that reappears under h93 and conforming;
  the L3 waveforms show it as about 5° content (`figures/waveforms_L3.png`).
- Because L2→L3 increments for Opt120 are not shrinking, the L3 error in
  `F_train` may be several times the observed L2→L3 increment.

## Proposed margins (estimates for M4/M5 to freeze)

The objective is `F = Σ_f (std_f / T_ref)²` with unit weights and
`T_ref = 1 N m`. Training currents are f = 0, 0.5 and 1; the withheld
currents are 0.25 and 0.75. `records.csv` holds every per-current std, so
these margins can be rescaled to any frozen `T_ref` and weights.

1. **Discretization error margin on F (one coupling model, L3, g20 or
   denser):** `δF = max(0.10 · F, 2.5e-7 N m²)`. An improvement smaller
   than δF counts as unresolved. Basis:
   - roughly 3× the largest observed L3−L2 increment at Opt120: 3.2%
     relative and 7.5e-8 N m² absolute. The factor 3 is there because
     those increments are not shrinking;
   - the angle-grid term: g20s−g20 at most 1.4% at L2–L3;
   - the mesh-family term: construction against export at most 1.4%
     at L3.
2. **Model margin:** none is small enough to state. At L2–L3, 45→93
   changes Opt120's F by +260% to +273% (2.8e-6 N m², about 11 times
   δF) and θ0's by +16%. Following the brief's rule:
   - report model dependence;
   - evaluate every candidate and every method on a common refined
     model for validation. The suggestion is h93, or the conforming
     band, which agrees with h93 to within 2.6% on F, at L3 (or L2 if
     cost requires), with g20 or g40;
   - do not use g10 as the validation grid (up to 17% low).

   Whether an optimizer run should also *train* on h93 is a protocol
   decision for experiment 2. It is not settled here.
3. **Feasibility margin, loaded mean-torque floor:** require
   `mean ≥ floor · (1 + 0.0025)`. Basis:
   - 45→93 changes θ0's full-current mean by 0.17%;
   - L3−L2 changes it by at most 0.13% (conforming) and at most 0.01%
     (harmonic);
   - the coupling and outer-Arkkio torques agree to 0.01%;
   - moved against fresh differs by 0.01%.

   The current rule must be frozen first, because it moves Opt120's mean
   by 14%.
4. **Geometric feasibility:**
   - no inverted triangle on the mesh that is evaluated;
   - a candidate whose moved mesh falls below about 5° minimum angle at
     the validation level is re-evaluated on a fresh mesh. Basis: at 1.3°
     the moved mesh shifted the conforming `F` by 9.4% while h45 shifted
     by only 0.55%;
   - air gap: recorded as a mesh-node measurement (θ0 1.000 mm, Opt120
     1.048 mm scaled). No margin is proposed;
   - bridge thickness: GEODE has no functional for it, so it is not
     evaluated.

## Matrix (all cells ran)

- **Designs.**
  - θ0 on the NURBS-multipatch family (`geometry/meshes`) and on a fresh
    construction mesh.
  - Opt120, reconstructed from the 12 scalars and the 10 offsets
    (#1180), on three meshes:
    - moved, L0–L2; L3 is recorded invalid;
    - fresh export (`build_sector_mesh.py --design optimized`), L0–L3;
    - fresh construction (`icem_validation plan` →
      `build_design_mesh.py`), L0–L3.
  - The candidate slot `EXTRA_DESIGN=file:…` runs the moved and
    fresh-construction paths. It was checked with the Opt120 vector.
- **Coupling.** h45 (n = 3:6:45), h93 (n = 3:6:93) and the conforming
  band, at every level.
- **Angles.** One 40-angle chain (0:0.5:19.5, each solve warm-started)
  per run. g10, g20, g40 and g20s are subsets of it. The 1° chain of the
  committed records gives the same torques to at most 5e-7 N m.
- **Currents.** f = 0, 0.25, 0.5, 0.75 and 1 under the primary fixed
  peak-current rule (I0 = 9.6777 A, unscaled geometry, the `Prescribed`
  rule of `tests/common/icem_design.rs`).
- **Controls, Opt120 only.** At θ0 every rule gives the fixed current.
  Three controls ran at f = 0.5 and 1, on the export and moved meshes,
  for every coupling:
  - `fixed-physical`: `I0 / ScaleR`;
  - `jinit`: the mesh-area group-1 density rule;
  - `reference`: the saved current.

  Every record names its rule (`current_rule`, `current_rule_definition`).

In total there are 123 matrix records and 16,440 nonlinear solves, with
0 solver failures. The 3 invalid-mesh records are kept; their field is
neither computed nor reported. On top of these come the 2 slot checks and
3 thread probes.

## Records

Each `runs/<run>.json` carries:

- the design: names and values, the 22-component `design_vector`, and
  the vector order;
- `current_rule` and its definition, `coupling`, `n_max`, `level`,
  `mesh_source` and `mesh_family`;
- mesh counts, `mesh_quality` (minimum angle, inverted count) and, for
  moved meshes, `motion.quality`, which includes the area ratios to the
  base;
- constraint values: magnet area, the mesh-node air gap, the gap-band
  area ratios and the group-1 area;
- rotation support per angle;
- per current, the unscaled and physical `I_peak`, and per angle
  `T_coupling`, `T_arkkio` (both bands), `T_arkkio_inner`,
  `T_arkkio_outer` and `T_ring`, all from the same field, plus Newton
  iterations, status, residual and wall time;
- the commit and the thread settings.

Torques are N m per modelled 60° sector, with `ScaleR² · LENGTH / 0.1`
applied. `records.csv` holds the per-(run, grid, current, estimator) mean,
population std and std², plus `F_train` and `F_withheld`. The objective
rows and the no-field rows carry the design vector. Note: the `file:`
input is parsed by serde_json without `float_roundtrip`. One offset (CPR7)
came back 1 ULP different from the `opt120` path. The torques were
identical, and `vector:` is exact.

## Environment and cost

- **Commit.** Code `258c670c` (`GEODE_COMMIT` in every record).
- **Host.** Apple M3 Ultra, 28 logical CPUs (20 performance and 8
  efficiency), 96 GiB, macOS 27.0.1. rustc 1.98.1, release profile.
  Python goes through `uv` with numpy 2.5.4, gmsh 4.15.2, matplotlib
  3.11.2 and pillow 11.3.0, always with `python -I`.
- **Threads.** Every eval process runs single-threaded:
  `RAYON_NUM_THREADS=1`, `GEODE_NUM_THREADS=1`, `--threads 1`, faer
  `Par::Seq`. Eight processes ran concurrently (`JOBS=8`). The host was
  shared with other work, with a load average of 10 to 31 during the run.
- **Thread probe.** On an L3 chain, 1, 4 and 8 faer/rayon threads gave
  1.68, 1.72 and 1.93 s per warm solve, with bit-identical torques. The
  2-D sparse LU did not speed up, so concurrency across processes was
  used instead.
- **Wall time.** The whole eval stage took 1466 s for 123 runs. Mean
  per-solve wall time under 8-way concurrency:

  | level | h45 | h93 | conforming |
  |---|---|---|---|
  | L0 | 0.03 s | 0.04 s | 0.02 s |
  | L1 | 0.17 s | 0.19 s | 0.09 s |
  | L2 | 0.40 s | 0.51 s | 0.50 s |
  | L3 | 2.4 s | 2.7 s | 3.2 s |

  One fixed-rule cell (200 solves) takes about 8–11 min at L3 and 1.5–2
  min at L2.
- **Meshes.**
  - The export meshes took 2.4–7.4 s (L0–L3), and the construction
    meshes, both designs together, 1.0–8.4 s.
  - The initial L1–L3 family took 12.0 s.
  - Moved-mesh motion takes 0.02 to 1.9 s.

`tables.md` has the full breakdown, including the per-run inventory.

## Reproduce

From the repository root, with the upstream cache present
(`uv run --with scipy==1.18.1 python -I benchmarks/motor_reference/extract_reference.py --fetch`)
and a private `CARGO_TARGET_DIR`:

```sh
benchmarks/motor_validation/run_all.sh            # meshes, eval, slot, threads, analyze
JOBS=8 benchmarks/motor_validation/run_all.sh eval
EXTRA_DESIGN=file:path/to/candidate.json benchmarks/motor_validation/run_all.sh all
```

One cell by hand:

```sh
RAYON_NUM_THREADS=1 GEODE_NUM_THREADS=1 \
  target/release/examples/icem_validation eval --design opt120 --level 2 \
  --source moved --coupling harmonic:3:6:93 --angles 0:0.5:40 \
  --currents 0,0.25,0.5,0.75,1 --rule fixed --threads 1 --out run.json
```

## Files

| file | content |
|---|---|
| `run_all.sh` | stages `meshes`, `eval`, `slot`, `threads`, `analyze` |
| `analyze.py` | summary script: writes `tables.md`, `summary.json`, `records.csv` and `figures/` from `runs/` |
| `runs/*.json` | raw records, including the invalid-mesh ones; `construction_meshes_L*.json` are the gmsh summaries |
| `records.csv`, `tables.md`, `summary.json` | generated |
| `figures/objective_vs_level.png`, `figures/waveforms_L3.png`, `figures/objective_vs_grid_L3.png` | generated |
| `meshes/plan.json`, `meshes/*_report.json` | fresh-mesh plan and export-mesh reports (the `.msh` files are gitignored and regenerated by `meshes`) |
| `candidate_template.json` | the design-file format, filled with Opt120 |
| `logs/*.log` | console output of every stage and run |

Driver: `crates/geode-core/examples/icem_validation.rs`. It reuses
`tests/common/icem_forward.rs`, `mesh::icem_controlpoints` and
`build_design_mesh.py`.

## Out of scope

Optimization runs (experiment 2) and the reference IGA refinement
(experiment 3) are not part of this experiment. The residual 12.9% cogging gap
of θ0 against the saved reference is not addressed here. It stays at h45:
0.01208 here against 0.01071 saved.
