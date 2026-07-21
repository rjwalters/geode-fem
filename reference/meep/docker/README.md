# `reference/meep/docker/` — reproducible Meep 3-D FDTD + adjoint baseline

In-repo build recipe so the **FDTD-density adjoint oracle** can be rebuilt
**from geode-fem alone** — the structured-grid contrast environment for the
head-to-head in **epic #647 Phase 4 (issue #651)**.

This is the DEFINITIVE 3-D adjoint-FDTD baseline for the apples-to-apples
comparison against GEODE's unstructured-tetrahedral shape-adjoint −10 dB
curved-conformal result
(`benchmarks/patch_antenna_conformal/conformal_results.toml`).

Unlike the Palace oracle (`reference/palace/docker/`, compiled from source),
Meep ships as a **conda-forge binary** (`pymeep` + `pymeep-extras`), so this
is a mamba install on top of `condaforge/miniforge3` — no from-source build.

| File | Role |
|------|------|
| `Dockerfile`               | CPU Meep-adjoint env (`pymeep`, `pymeep-extras`, autograd/scipy/matplotlib) |
| `smoke_test.py`            | Minimal end-to-end proof the adjoint stack runs (imports + one 2-D gradient) |
| `conformal_baseline_3d.py` | The real 3-D head-to-head (curved patch, density-on-Yee-grid, |S11| objective) with a fully wired nlopt-MMA + conic-filter + tanh-projection topology optimizer |

## Build

```sh
docker build -t meep-baseline:cpu -f reference/meep/docker/Dockerfile reference/meep/docker
```

The conda solve + package download dominates the first build (expect a few
minutes to ~20 min depending on network; the layer is cached afterward).
Verified build: `meep 1.34.0` (conda-forge), image ~648 MB.

### Environment notes (verified against conda-forge 2026-07-20)

- **`pymeep-extras` does not exist** on conda-forge — the name does not
  resolve. `meep.adjoint` ships **inside `pymeep`**; the only extra deps it
  needs are `autograd` (objective backprop) and `nlopt` (the MMA optimizer
  driver), which the Dockerfile installs explicitly.
- **Python is pinned to 3.10**: the current `pymeep` builds (1.31–1.34) are
  `py310`-only, so `python=3.11` fails to solve.
- Matplotlib runs headless via `MPLBACKEND=Agg` (no libGL needed).

## Verify the adjoint stack (smoke test)

```sh
docker run --rm meep-baseline:cpu python /opt/meep-baseline/smoke_test.py
```

Prints the `meep` / `meep.adjoint` / numpy versions, then runs ONE
forward+adjoint FDTD evaluation on a tiny 2-D design region and reports the
returned gradient's shape and L2 norm. A finite, non-zero gradient of the
expected shape means the forward solve, the adjoint solve, and the
design-region backprop all executed — not just that the modules import.

## The 3-D head-to-head scaffold

```sh
# Stage 2 — construct the full problem (fast, always runs):
docker run --rm meep-baseline:cpu \
  python /opt/meep-baseline/conformal_baseline_3d.py
# Stage 3 — one 3-D forward+adjoint gradient (HEAVY; verify the adjoint
#   plumbing at a coarse resolution). MEEP_RES sets pixels/mm:
docker run --rm -e MEEP_RES=3 -e RUN_GRAD=1 meep-baseline:cpu \
  python /opt/meep-baseline/conformal_baseline_3d.py --gradient
# --smoke-opt — validate the ENTIRE optimizer loop CHEAPLY (filter ->
#   projection -> forward -> adjoint gradient -> nlopt MMA step) on a tiny
#   throwaway cell in ~30 s. Proves the plumbing without a heavy run:
docker run --rm meep-baseline:cpu \
  python /opt/meep-baseline/conformal_baseline_3d.py --smoke-opt
# Stage 4 — full optimization loop (operator, production hardware).
#   MEEP_RES sets pixels/mm; results -> meep_conformal_results.json in CWD:
docker run --rm -e MEEP_RES=16 -e RUN_FULL=1 \
  -v "$PWD":/out -w /out meep-baseline:cpu \
  python /opt/meep-baseline/conformal_baseline_3d.py --full
# Staircase-penalty RESOLUTION SWEEP — full optimize per resolution, writes
#   meep_conformal_results_res<N>.json + a sweep summary (operator, HEAVY):
docker run --rm -e MEEP_RES_SWEEP=8,12,16 -e RUN_SWEEP=1 \
  -v "$PWD":/out -w /out meep-baseline:cpu \
  python /opt/meep-baseline/conformal_baseline_3d.py --sweep
```

The optimizer knobs are env-overridable: `MEEP_RES` (Yee pixels/mm — the
staircase knob), `MEEP_FILTER_R` (conic filter radius / min metal feature in
mm, default 0.5), `MEEP_MAXEVAL` (MMA evals per beta stage, default 12),
`MEEP_RESULTS` (results-file path), and `MEEP_RES_SWEEP` (comma-separated
resolutions for `--sweep`). The beta binarization schedule is `[8, 16, 32,
64]`. `--full` records per-iteration objective (`sum_f |S11(f)|^2`) and
worst-of-band |S11|-proxy to `meep_conformal_results.json`; mount a host dir
(`-v "$PWD":/out -w /out`) so that file lands outside the container.

**`--smoke-opt` note:** the tiny throwaway smoke cell uses a finite
*dielectric* design material (index 3.4), not production PEC. A PEC density
grid is effectively a step function — even a few percent metal shorts the
feed-side reflected field, so its gradient degenerates to a cliff and MMA
cannot take a meaningful interior step. The dielectric contrast gives a
well-conditioned gradient so the wired loop *visibly* drives the objective
down (the only thing the plumbing test asserts). The `--full` / `--gradient`
production paths keep PEC unchanged.

`conformal_baseline_3d.py` mirrors GEODE's curved-conformal patch
(`crates/geode-core/src/mesh/patch.rs::bent_conformal`,
`reference/gmsh/patch_2g4_smoke.yaml`): the FR-4 substrate slab (ε_r 4.4,
tanδ 0.02), PEC ground, cylinder-bent (R = 40 mm) patch metal as a density
design region, box + PML, a coax-probe current-source feed, and an
|S11|-proxy match objective over the recorded band ω ∈ {0.30, 0.35, 0.40}
(natural units, target −10 dB). The script has explicit stages: it always
**constructs** the full problem; a single 3-D forward+adjoint gradient is
**gated** behind `--gradient`/`RUN_GRAD=1`; the full **nlopt-MMA topology
optimizer** (conic length-scale filter + increasing-beta tanh projection,
minimizing `sum_f |S11(f)|^2` via the meep.adjoint gradient) is gated behind
`--full`; and `--smoke-opt` runs that entire optimize() path on a tiny
throwaway cell to validate the plumbing cheaply. See its module docstring for
the full geometry mapping and the natural-units convention.

### Why the 3-D gradient is gated (heaviness — measured)

Construction is instant, but a **single** 3-D forward+adjoint over this
64 × 60 × 42 mm open-radiator cell is genuinely expensive: the DFT-decay
stop condition needs many light-crossing times. Measured on the build host:
`MEEP_RES=2` (~1.3 M Yee cells) did **not** finish one gradient in 15 min,
and `MEEP_RES=1` never converges at all (an 8-cell-thick PML is too coarse
to absorb, so the fields never decay). So a valid 3-D solve needs production
resolution **and** production hardware — exactly the cost argument the paper
makes about structured-grid FDTD on this geometry. The **adjoint stack
itself** is proven end-to-end by `smoke_test.py` (a real 2-D gradient in
~2 s); the 3-D scaffold proves the head-to-head problem **constructs**
faithfully and stops honestly.

### Feed / objective caveat (meep 1.34.0)

The textbook S11 tool, `EigenmodeCoefficient`, is **not** used here: meep's
eigenmode solver MPB cannot mode-solve a cross-section containing PEC
(a microstrip-over-PEC feed raises *"invalid dielectric function for MPB"*),
and this conda-forge build's `EigenmodeCoefficient` adjoint also aborts with
*"number of adjoint chunks != forward chunks (0)"* unless the monitor sits
on mode-carrying material present in the forward run. The scaffold instead
drives the lumped port with a z-directed current source and reads a
reflected-field |S11|-proxy via `FourierFields` (both verified to
differentiate end-to-end). A dielectric-clad / coax feed that MPB *can*
mode-solve — enabling a true mode-decomposed S11 — is a documented
production refinement.

### Natural-units convention (must match GEODE)

GEODE runs in `c = μ₀ = ε₀ = 1`, so `ω ≡ k₀` and the band is recorded
**dimensionless** (do NOT convert to GHz — paper honesty constraint). Meep
also uses `c = 1` but parameterizes by frequency `f` with `ω = 2πf` and a
length unit `a`. We set **a = 1 mm** (all mm dimensions used as-is) and
**f = ω / (2π)**, so Meep's angular frequency reproduces GEODE's band:
`f = [0.30, 0.35, 0.40] / (2π) ≈ [0.04775, 0.05570, 0.06366]`.

## The three-way comparison (honest framing)

The paper (`papers/conformal-antenna-diffopt/`, BRIEF.md) writes the
head-to-head as a **Planned-Evaluation / future-work subsection with NO
fabricated baseline numbers**. Three legs:

1. **GEODE** — unstructured-tet, moving-boundary (node-motion) **shape**
   adjoint on **body-fitted** curved metal. Reaches worst-of-band −12.06 dB
   over the band (the committed `conformal_results.toml` result). The curved
   conductor is represented exactly by the mesh.
2. **This 3-D Meep-adjoint density baseline** — a **permittivity density**
   field on a **Cartesian Yee grid**. The curved conductor must be rasterized
   onto axis-aligned pixels, so it **STAIRCASES**; the discretization penalty
   (and its scaling with grid resolution) is exactly what this environment
   measures. Same physics family (open radiator + PML + |S11|), different
   geometry representation.
3. **ceviche-2D interim baseline** — `benchmarks/fdtd_density_baseline/`
   (owned by another agent; do NOT modify from here). A lighter 2-D FDTD
   density optimizer used as an interim/cross-check; this 3-D Meep leg is the
   definitive structured-grid comparison.

**The honest claim today:** GEODE *reaches* a freeform curved-conformal
−10 dB design; the *quantified* superiority over FDTD-density (worst-of-band
|S11| at matched cost, and the staircase-vs-resolution curve) is the
announced next step this environment exists to produce.

## Feeding the paper's Planned-Evaluation section

Outputs an operator should capture when running `--full` (for
`papers/conformal-antenna-diffopt/`):

- worst-of-band |S11| (dB) of the converged Meep-adjoint density design vs
  GEODE's −12.06 dB, at matched cost;
- a **RESOLUTION sweep** (`--sweep` with `MEEP_RES_SWEEP=...`, or the
  `MEEP_RES` knob run by run) quantifying the staircase penalty on the curved
  conductor as the Yee grid is refined — the load-bearing figure for the
  paper's argument;
- provenance: `meep.__version__` (printed by both scripts), the pinned image,
  cell size / design-DOF count (printed by the 3-D scaffold), and wall/RSS.

Keep a durable second copy of any produced results in-repo — do not leave a
converged run only on an ephemeral cloud box.
