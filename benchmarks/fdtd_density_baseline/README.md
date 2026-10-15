# FDTD-density baseline harness — epic #647 Phase 4 (issue #651)

Comparative baselines for the paper
`papers/conformal-antenna-diffopt/` ("Differentiable-by-Construction FEM for
Freeform Open-Radiator Design"). The paper's headline is a GEODE result — an
unstructured-tetrahedral H(curl) **shape adjoint** that gradient-designs a
curved-metal conformal radiator to drive the whole band below **−10 dB** return
loss (`benchmarks/patch_antenna_conformal/conformal_results.toml`,
worst-of-band −5.51 → −12.06 dB). This directory builds the **contrast class**:
a structured Yee/Cartesian-grid **density** method (FDTD/FDFD topology
optimization) that must *staircase* the curved conductor and therefore cannot
represent it as faithfully at feasible cost.

Per the paper BRIEF, the 3-way head-to-head is a **planned evaluation / future
work** — this harness is the substrate for it. **No fabricated comparative
numbers.** The one thing quantified deterministically today is the *geometric*
staircasing floor (`staircasing_demo.py`); the field-solve baselines are a
runnable 2D representative (ceviche) plus an operator-gated definitive 3D run
(Meep).

## The head-to-head design (three methods)

| Method | Geometry representation | Curved conductor | Status |
| --- | --- | --- | --- |
| **GEODE** shape adjoint (the −10 dB result) | Unstructured tets; boundary nodes **on** the curve; node-motion (moving-boundary) adjoint, single factorization | Nodes on the curve (~1e-14 mm); linear facets 4.4 µm max on L0, 1.1 µm on L1 (`staircasing_demo.py`) | **Done** — committed artifact |
| **FDTD-density** (Meep-adjoint / ceviche) | Fixed Yee/Cartesian grid; permittivity/density field, later binarized | **Staircased** — O(h) boundary error, cubic 3D refinement cost | 2D **runnable now** (ceviche); 3D **operator-gated** (Meep) |
| **Low-DOF parametric** | A handful of shape scalars swept / FD-optimized | Can be conformal but cannot reach freeform | Planned (not in this dir) |

The claim the baseline supports: a density method optimizes a **grid-locked
field**, never *moves the boundary*, so the curved conductor is permanently the
voxel rasterization whose error `staircasing_demo.py` measures — a floor that
shrinks only with grid refinement (cubic cost in 3D), never with iterations.

## Runnable now

Everything here uses only Python ≥ 3.11. A pinned venv is described by
`requirements.txt`. **Python 3.14 works for this 2D/ceviche track** (verified).

```bash
python -m pip install -r requirements.txt
```

### 1. `staircasing_demo.py` — deterministic geometric floor (PRIMARY)

Self-contained (numpy + matplotlib; no ceviche, no field solve). Takes the
physical 2.4 GHz bent-patch geometry (R = 100 mm, h = 1.6 mm, patch 38 mm,
ground 62 mm), rasterizes the patch (top) and ground (bottom) arc profiles onto
Yee grids at 1–16 cells/mm, and measures the facet error of the actual L0/L1
conformal volume meshes against the same cylinders. Reproduction steps, the
environment record and the citation pin are in `staircasing_notes.md`.

```bash
export PATCH_CONFORMAL_L1=/path/to/patch_conformal_L1.msh   # see the notes
python staircasing_demo.py
# -> staircasing_results.json    (committed artifact; host-independent)
# -> staircasing_results.sha256  (citation pin for the JSON)
# -> staircasing.png             (byte-identical only at the recorded matplotlib)
python staircasing_verify.py      # also passes under python -O
```

Measured Cartesian floor (patch top arc 38.304 mm; ground bottom arc 61.504 mm;
proxy = RMS vertical displacement / arc length):

| cells/mm | h (mm) | patch RMS (mm) | patch RMS/h | patch proxy | ground RMS (mm) | ground RMS/h | ground proxy |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 1.0 | 0.333 | 0.33 | 8.68e-3 | 0.278 | 0.28 | 4.52e-3 |
| 2 | 0.5 | 0.128 | 0.26 | 3.35e-3 | 0.140 | 0.28 | 2.28e-3 |
| 4 | 0.25 | 0.0727 | 0.29 | 1.90e-3 | 0.0715 | 0.29 | 1.16e-3 |
| 8 | 0.125 | 0.0371 | 0.30 | 9.68e-4 | 0.0359 | 0.29 | 5.84e-4 |
| 16 | 0.0625 | 0.0189 | 0.30 | 4.94e-4 | 0.0181 | 0.29 | 2.94e-4 |

Facet error of the actual conformal meshes against the same cylinders
(vertices sit on the circles to ~1.4e-14 mm; the straight facets between them
do not):

| Volume mesh | Surface | Triangles | Max radial error (µm) | Area-weighted RMS (µm) |
| --- | --- | ---: | ---: | ---: |
| L0 | patch | 1042 | 4.358 | 1.752 |
| L0 | ground | 3016 | 4.774 | 1.722 |
| L1 | patch | 4074 | 1.128 | 0.441 |
| L1 | ground | 12032 | 1.251 | 0.434 |

Key readings:
- **Cartesian boundary-position error is O(h)**: the fitted proxy-vs-cell-size
  powers are 1.0064 (patch) and 0.9850 (ground), and RMS/h stays at 0.26–0.33
  at every resolution, so the staircase never improves relative to its own
  cell.
- **The conformal mesh is not exact.** Nodes lie on the curve, but linear
  facets have a sagitta: 4.4–4.8 µm max (1.7 µm RMS) on L0 and 1.1–1.3 µm max
  (0.43 µm RMS) on L1, i.e. ~4× smaller for ~4× the triangles (second order in
  facet size). At 16 cells/mm the Cartesian RMS (18 µm) is still ~10× the L0
  facet RMS, but the two use different measures (vertical vs radial), so no
  equal-error cell count or cost ratio is inferred from them.
- **Slab-sector perimeter does not converge** (taxicab staircase paradox:
  +7–14 % at every grid, against limits of +9.2 % patch / +14.1 % ground) while
  **its area does**. That contrast is the diagnostic check that the code
  measures staircasing rather than a bug. The perimeter is of the slab sector,
  not the metal patch, and is not an electromagnetic error.

The `resonance_shift_proxy_abs` column is RMS profile displacement / arc
length — a dimensionless geometric proxy with an uncalibrated coefficient,
**not** a solved eigenvalue or S11. Whether these geometric floors translate
into electromagnetic accuracy or solver cost is the open question of
#1085/#1088; nothing in this directory answers it.

### 2. `ceviche_fdfd_baseline.py` — 2D FDFD density inverse-design (representative)

A runnable 2D frequency-domain density baseline in ceviche. Constructs the grid,
rasterizes the same curved conductor into a permittivity field, builds the
`fdfd_ez` operator + source, runs a forward solve, and wires an autograd
adjoint gradient around an |S11|-analog objective.

```bash
python ceviche_fdfd_baseline.py          # construct + one forward solve
python ceviche_fdfd_baseline.py --grad   # + one end-to-end autograd gradient
```

Verified in-env (ceviche 0.1.3, Python 3.14): builds a 119×77 Yee grid,
rasterizes 152 "metal" cells, forward-solves (|Ez|max ≈ 0.19), and computes a
reverse-mode autograd gradient (‖dJ/dε‖ ≈ 1.1×10⁻³). The heavy binarized
topology-optimization sweep is intentionally left as a `# TODO(run)` with a
documented description of what a converged run shows (grid-locked density,
projection re-introduces staircasing every step, |S11|-analog bounded away from
the conformal result).

## Operator-gated: Meep-3D (the *definitive* baseline)

The ceviche result is **2D and photonics-oriented** — representative, not
apples-to-apples with the 3D **metal open radiator** (box-UPML + lumped port +
lossy complex-ε + passive |S11|) GEODE solves. The definitive FDTD-density
baseline is a **3D Meep adjoint-FDTD** run on the curved conformal geometry.
This is **OPERATOR-ONLY**: `pymeep` ships only via conda-forge and does **not**
support Python 3.14 (this Mac's default) — it needs a separate conda env not
present here.

Matched-geometry records for #1088 (physical 2.4 GHz bent patch): R1/R2 in
`../../reference/meep/docker/MATCHED.md` and `AWS_PILOT.md`; the R4 run, the
R1/R2/R4 trend (no spatial convergence shown) and the R8 requirement in
`../../reference/meep/docker/AWS_R4.md`, verified by `meep_r4_verify.py`.

### Runbook (operator)

```bash
# 1. Install miniforge (conda) if absent.
#    macOS arm64:
curl -L -O https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-MacOSX-arm64.sh
bash Miniforge3-MacOSX-arm64.sh -b -p "$HOME/miniforge3"
source "$HOME/miniforge3/etc/profile.d/conda.sh"

# 2. Create the Meep env (pymeep needs Python 3.11, not 3.14).
conda create -n meep -c conda-forge python=3.11 pymeep pymeep-extras
conda activate meep

# 3. Sanity: adjoint solver present.
python -c "import meep as mp; import meep.adjoint as mpa; print(mp.__version__)"
```

### 3D adjoint-FDTD setup sketch (curved conformal geometry, PML)

Mirror the GEODE `bent_conformal` geometry (below) as a Meep `Simulation`:

- **Cell**: 3D `mp.Vector3` box sized to the substrate footprint + air margin +
  PML on all six faces (`mp.PML(thickness)`), matching the GEODE box-UPML shell.
- **Curved conductor**: the FDTD-density representation must voxelize the arc.
  Either (a) a `mp.MaterialGrid` density design region over the curved-metal band
  (the honest density baseline — it *staircases*, per `staircasing_demo.py`), or
  (b) `mp.Prism`/`GeometricObject` metal for a fixed-shape reference. The point
  of the comparison is that (a) is what a density optimizer actually controls.
- **Substrate**: FR-4 `mp.Medium(epsilon=4.4)` with the loss tangent (0.02);
  metal via a Drude/high-conductivity model or PEC-like `mp.metal`.
- **Port / excitation**: an `mp.EigenModeSource` (or a lumped-ish current sheet)
  at the feed plane; the objective is a **reflection / |S11|** functional via
  `mpa.EigenmodeCoefficient` on a mode monitor.
- **Adjoint**: `meep.adjoint.OptimizationProblem(objective_functions=...,
  design_regions=[MaterialGrid...], ...)`; gradient from one adjoint FDTD run per
  frequency; drive with `nlopt`/`scipy` + a density projection/binarization.
- **Output**: per-frequency |S11| over the same band GEODE used, plus the design
  evolution — to be reported **without** inventing numbers before the run exists.

## Honest caveats (enforced by the paper audit)

- The 3-way head-to-head is **planned evaluation / future work** (BRIEF §"Scope &
  honesty constraints"). This dir builds the substrate; it does **not** yet
  publish comparative S11 numbers.
- The ceviche 2D FDFD result is **representative** (2D, photonics-oriented), not
  apples-to-apples with the 3D metal open radiator. **Meep-3D is the definitive
  baseline.**
- No fabricated comparative numbers anywhere. The only committed numbers are the
  *geometric* staircasing metrics in `staircasing_results.json`, which are pure
  computational geometry (no physics claim beyond the labeled first-order Δf/f
  proxy).
- GEODE frequencies are dimensionless natural units as recorded; the mm/GHz
  scales used here are representative for the 2D/3D grids, not a claim about the
  GEODE run's physical units.

## The geometry (faithful to the committed fixture)

Source: `crates/geode-core/src/mesh/patch.rs`,
`PatchFixture::bent_conformal` (commit `eac4e85`, "Part of #647"). The flat FR-4
patch/ground slab is wrapped around a cylinder about the y-axis. In the plane of
curvature (x, z) the exact node map is:

```
phi = x / R_bend ;  r = R_bend + (z - z0)
x'  = r * sin(phi)
z'  = z0 + r * cos(phi) - R_bend
```

so a horizontal metal line wraps onto a **circular arc** of radius
`R_bend + (z - z0)` centered at `(0, z0 - R_bend)`. Committed parameters:
`R_bend = 40 mm` (`CURVED_SMOKE_BEND_RADIUS`), `h_sub = 1.6 mm` (FR-4 `h`),
`x_halfwidth ≈ 12 mm` ⇒ `phi_max = 0.30 rad` (patch.rs doc: "half-width (~12 mm)
subtends ~0.3 rad of arc"). Both `staircasing_demo.py` and
`ceviche_fdfd_baseline.py` rasterize this exact arc.

## Mapping to the paper's "planned evaluation" section

`papers/conformal-antenna-diffopt/` (BRIEF §5 "Planned evaluation (future
work)") promises the 3-way head-to-head. These artifacts feed it:

- `staircasing_results.json` / `staircasing.png` → the **geometric-fidelity**
  argument: a Yee-grid density method carries an O(h) boundary floor and a
  ~14 % perimeter (electrical-length) staircase-paradox error that a conformal
  tet mesh does not, at ~5×10⁴× the 3D cost to close. Committed, deterministic —
  citeable today as the *geometry* half of the contrast.
- `ceviche_fdfd_baseline.py` → the **representative** differentiable-density
  baseline (2D); a runnable illustration of "optimizes a grid-locked field, not
  the boundary."
- Meep-3D runbook → the **definitive** baseline the paper will report once run
  (operator-gated). Until then the paper's honest claim stands: GEODE *reaches*
  the freeform curved-conformal −10 dB design; the *quantified* superiority over
  FDTD-density is the announced next step.
