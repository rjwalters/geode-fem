# `tools/viz/` — `geode_viz` Python package

Visualization helpers for `geode-fem` benchmark TOMLs. Lands with
**#277** as Phase 1A of **Epic #276** (visualization tooling). This
package is foundation-only — it exposes a shared loader, a matplotlib
style, and an artifacts-path resolver. The headline line plots that
consume the scaffold land in the following Phase 1 issues:

| Issue | Plot family                                                |
| ----- | ---------------------------------------------------------- |
| #278  | Spiral / patch: |S11| dB + polar Smith                     |
| #279  | Spiral: L / Q / R vs f (Mohan + mom PEEC + SRF) and        |
|       | Mie: Q_ext / Q_sca / Q_abs vs ka with analytic overlay     |
| #280  | Mie sphere: open / driven mode catalog                     |

## Install

The package is self-contained under `tools/viz/` (no new top-level
repo deps — acceptance criterion for #277). Install in editable mode
so plot modules can import the helpers without re-packaging on every
change:

```bash
pip install -e tools/viz
```

Requirements:

- Python **3.11+** (uses stdlib `tomllib`; the `tomli` fallback in
  `pyproject.toml` is purely defensive for sibling tools on 3.10).
- `numpy >= 1.24`, `matplotlib >= 3.7` (pulled in transitively).

Sanity check after install:

```bash
python -c "from geode_viz.io import load_results; \
    print(load_results('spiral_inductor')['meta']['generated_at_commit'])"
```

Should print the commit hash from
`benchmarks/spiral_inductor/results.toml` (`14659c1d...` at the time
of writing).

## Output convention

Every plot module writes its outputs under a single gitignored tree:

```
artifacts/viz/<benchmark>/<plot>.{png,svg}
```

The top-level `artifacts/` is in `.gitignore` (verify with
`git check-ignore artifacts/viz/foo.png`). Resolve the directory with:

```python
from geode_viz.paths import artifacts_dir
out = artifacts_dir("spiral_inductor")  # creates the dir if missing
fig.savefig(out / "L_vs_freq.png")
```

`artifacts_dir` rejects multi-component paths — keep the tree two
levels deep so `ls artifacts/viz/` is a single-glance benchmark list.

## Package surface

The package re-exports five names from `geode_viz`:

```python
from geode_viz import (
    load_results,   # -> dict (benchmark TOML + injected _source.path)
    artifacts_dir,  # -> Path under artifacts/viz/<subdir>/, created
    repo_root,      # -> Path to the geode-fem checkout root
    apply_style,    # mpl rcParams: viridis cycle, gridlines, dpi
    footer,         # stamp commit / fixture-SHA / source on a Figure
)
```

### Loader

`load_results(benchmark, filename=None)` resolves
`benchmarks/<benchmark>/<filename>.toml`. When `filename` is omitted,
the loader walks a default priority list:

```
results.toml
results_matched.toml
driven_results.toml
pattern.toml
pattern_matched.toml
open_results.toml
baseline.toml
```

The first existing file wins. To load a specific variant explicitly:

```python
results = load_results("mie_sphere", filename="driven_results_fine.toml")
```

The returned dict mirrors the TOML structure 1:1, plus an injected
`_source.path` (repo-relative) so plot footers can name their input.
Don't invent new top-level keys downstream — drill into the existing
benchmark-specific tables (`results["oracles"]["mohan"]...`).

### Style

`apply_style(mode="light")` installs an rcParams snapshot calibrated
for the artifacts/ PNGs:

- Sans-serif body, 10-pt; 12-pt axis titles.
- 8-color line cycle sampled from `viridis` (colorblind-OK; degrades
  to grayscale gracefully). Sequential / diverging colormaps exposed
  as `geode_viz.style.SEQUENTIAL_CMAP` and `DIVERGING_CMAP`.
- Gridlines on (light, behind data, major only).
- 120-dpi screen render, 300-dpi `savefig` with `bbox="tight"`.
- Constrained-layout on by default.

`mode="dark"` switches to a dark backdrop palette for README
screenshots / presentation slides. The on-disk PNGs in `artifacts/`
should stay on `"light"`.

### Footer

`footer(fig, results)` stamps a 7-pt monospace provenance line at the
bottom-left of `fig`:

```
commit 14659c1d | fixture c9707fb9 | benchmarks/spiral_inductor/results.toml
```

Pulled from the `[meta]` block (`generated_at_commit`,
`fixture_sha256`) so every plot carries its reproducibility receipts.

## Phase 1B: S-parameter + Smith-chart plots (#278)

The `geode_viz.plots.s_params` module renders the |S11| dB sweep and
the polar Smith-chart view for the two driven benchmarks that already
carry an N-port result table on disk (spiral inductor + patch antenna).
The CLI entry point lives at `geode_viz.scripts.plot_benchmark` (and
also as a script wrapper at `tools/viz/scripts/plot_benchmark.py`).

```bash
# Spiral inductor: writes s11_db.png + smith.png + lqr_vs_f.png
python -m geode_viz.scripts.plot_benchmark spiral_inductor

# Patch antenna (matched): overlays the unmatched sweep automatically
python -m geode_viz.scripts.plot_benchmark patch_antenna --variant matched

# Restrict to one of the plot families
python -m geode_viz.scripts.plot_benchmark spiral_inductor --smith-only
python -m geode_viz.scripts.plot_benchmark patch_antenna --s11-only
```

The Smith chart uses matplotlib's polar projection — no `scikit-rf`
dependency. For the spiral the complex S11 is reconstructed from
`z_re_ohm` / `z_im_ohm` via Γ = (Z − Z₀) / (Z + Z₀); for the patch the
recorded `s11_re` / `s11_im` fields are consumed directly. The dB axis
floor defaults to −30 dB and tightens when the data dips deeper.

## Phase 1C: L/Q/R vs f + Q vs ka with oracle overlays (#279)

Phase 1C adds two more plot families to the same CLI:

- `geode_viz.plots.spiral.plot_lqr_vs_f` — three-panel L_eq / Q / R vs
  frequency for the spiral inductor. Overlays the Mohan L₀ band
  (current-sheet / modified-Wheeler / monomial-fit) and the mom PEEC
  n=3 / n=4 shaded bracket on the L panel; drops a dotted SRF
  guideline on every panel from `meta.srf_ghz`. R uses a log y-axis
  with a hatched "≤ 0 post-SRF" overlay so the parallel anti-resonance
  isn't silently dropped.
- `geode_viz.plots.mie.plot_efficiency_vs_ka` — Q_ext / Q_sca / Q_abs
  vs ka for the driven Mie sphere. Analytic series (B&H) drawn as a
  solid line, FEM samples as scatter markers. Lower thin panel shows
  the per-point relative error in % on a log axis (with a 5 % guide).

```bash
# Spiral: writes s11_db.png + smith.png + lqr_vs_f.png in one shot
python -m geode_viz.scripts.plot_benchmark spiral_inductor

# Just the L/Q/R panel
python -m geode_viz.scripts.plot_benchmark spiral_inductor --lqr-only

# Mie sphere (coarse fixture, default)
python -m geode_viz.scripts.plot_benchmark mie_sphere

# Mie sphere on the fine fixture (driven_results_fine.toml, issue #215)
python -m geode_viz.scripts.plot_benchmark mie_sphere --fine
```

Both plot families echo the first caveat from the TOML's
`meta.notes = [...]` array as a small italic subtitle so the figure
self-documents its known limits (Leontovich validity floor on the
spiral; matched-Sacks UPML choice on the Mie sphere).

## Phase 2C: headless ParaView render (#288)

Phase 2 of Epic #276 is a field-export pipeline rather than a TOML line
plot. Phase 2A (#286) added a `.vtu` writer in `geode_core::viz_vtu`;
Phase 2B exports a solved field to a `.vtu` `UnstructuredGrid`; Phase 2C
(this script) renders that file to a PNG **headlessly** via ParaView's
`pvbatch`, so a developer gets a recognisable field-slice image without
opening the GUI.

The renderer lives at `tools/viz/geode_viz/scripts/pvbatch_render.py`. It
is a thin wrapper over the `paraview.simple` API
(`OpenDataFile` / `Slice` / `GetColorTransferFunction` / `SaveScreenshot`).

> **ParaView is not a CI/pip dependency.** This is a local developer
> debugging tool — install ParaView 5.x yourself
> (<https://www.paraview.org/download/>) and run it under `pvbatch`, not
> plain `python`. Importing the module under plain `python` fails with an
> actionable error rather than a raw `ImportError` traceback.

End-to-end (2B export → 2C render):

```bash
# 2B: export a solved field to a .vtu (producer; see the 2B issue).
#     Writes e.g. artifacts/viz/E_patch.vtu under the gitignored tree.
cargo run -p patch_antenna --release -- --export-field --out-dir artifacts/viz

# 2C: render a z-slice coloured by |E| to a PNG (run under pvbatch).
# Direct-path form (no PYTHONPATH needed):
pvbatch tools/viz/geode_viz/scripts/pvbatch_render.py \
    artifacts/viz/E_patch.vtu --slice z=0.5 --out artifacts/viz/E_patch.png

# Module form, if pvbatch's interpreter can see the editable-installed
# geode_viz package (point it at tools/viz if not):
PYTHONPATH=tools/viz pvbatch -m geode_viz.scripts.pvbatch_render \
    artifacts/viz/E_patch.vtu --slice z=0.5 --out artifacts/viz/E_patch.png
```

CLI flags:

- `input` (positional, required): input `.vtu` `UnstructuredGrid`.
- `--out`: output PNG. Default `artifacts/viz/renders/<stem>.png` via
  `geode_viz.paths`, falling back to a sibling `<stem>.png` if that
  package is not importable under `pvbatch`.
- `--slice <axis>=<value>`: axis-aligned plane, e.g. `z=0.5`. Default: a
  slice through the mesh bounding-box centre on the z axis.
- `--field <name>`: `PointData` array to colour by. Default `|E|` (the
  scalar magnitude array emitted by the Phase 2A writer).
- `--colormap <name>`: ParaView colormap preset. Default
  `"Viridis (matplotlib)"` (perceptually uniform).
- `--size W H`: output image size in pixels. Default `1200 900`.

`pvbatch`'s bundled Python often cannot see the `pip install -e tools/viz`
package; prefix the command with `PYTHONPATH=tools/viz` (as above) if the
module form or the `artifacts/viz/` default-path resolution fails. The
output goes to the gitignored `artifacts/viz/` tree — never commit a
rendered PNG.

## Phase 3C: frequency-sweep animation (.pvd → ffmpeg → MP4) (#291)

Phase 3C is the last item of Epic #276. It turns a frequency sweep of
exported `.vtu` fields into an MP4 so a developer can watch a resonance
build and decay as the source frequency steps across a band — the key
debugging artifact for resonant structures (the patch antenna).

> **Frequency-domain, not time-domain.** GEODE-FEM is a frequency-domain
> solver: this is **not** an `E(r, t)` movie. "Animation" means one
> rendered frame per *source frequency* `ω`, stitched into a video.

The pipeline composes the 2B field export and the 2C render core (it does
not introduce new field/render logic):

1. **Sweep export (Rust).** `patch_antenna -- --export-sweep <dir>` solves
   the benchmark fixture once per swept frequency and writes one
   `E_<index>.vtu` per frequency into `<dir>`, plus a ParaView `.pvd`
   collection (`sweep.pvd`) mapping each frame to a `timestep` = the swept
   frequency (GHz). Each frame is byte-for-byte the `--export-field`
   output at that frequency (same 2B per-node field eval). The `.pvd` is
   hand-rolled XML (no deps), consistent with the Phase 2A `.vtu` writer.
2. **Render + stitch (Python).**
   `tools/viz/geode_viz/scripts/sweep_animate.py` reads the `.pvd`, renders
   each frame with the **same** slice/colormap core as `pvbatch_render.py`
   (refactored into the shared `geode_viz.scripts.render_core` so 2C and 3C
   cannot diverge), then shells out to `ffmpeg` to stitch
   `frame_%04d.png` → an MP4 (configurable fps, default 10).

> **Neither ParaView nor ffmpeg is a CI/pip dependency.** Both are
> local-only developer tools. The render step runs under ParaView's
> bundled Python (`pvbatch`); a plain-`python` invocation fails with an
> actionable "run under pvbatch" message. A missing `ffmpeg` binary fails
> with a clear, actionable error too (and `--frames-only` lets you get the
> PNG frames without it).

End-to-end (3C):

```bash
# (1) Rust: export one .vtu per swept frequency + sweep.pvd.
#     --f-start / --f-stop are GHz; --n is the frame count
#     (defaults: 2.0–3.0 GHz over 11 points).
cargo run -p patch_antenna --release -- \
    --export-sweep --out-dir artifacts/viz/patch_sweep --f-start 2.0 --f-stop 3.0 --n 11

# (2) Python: render frames + stitch to MP4, under pvbatch (ParaView 5.x).
#     Direct-path form (no PYTHONPATH needed):
pvbatch tools/viz/geode_viz/scripts/sweep_animate.py \
    artifacts/viz/patch_sweep/sweep.pvd \
    --out artifacts/viz/patch_sweep.mp4 --fps 10

# Module form, if pvbatch can see the editable-installed package:
PYTHONPATH=tools/viz pvbatch -m geode_viz.scripts.sweep_animate \
    artifacts/viz/patch_sweep/sweep.pvd --out artifacts/viz/patch_sweep.mp4

# Render the PNG frames only (skip the ffmpeg stitch):
pvbatch tools/viz/geode_viz/scripts/sweep_animate.py \
    artifacts/viz/patch_sweep/sweep.pvd --frames-only
```

`sweep_animate.py` CLI flags:

- `pvd` (positional, required): the `.pvd` collection from the sweep
  export.
- `--out`: output MP4. Default `artifacts/viz/animations/<name>.mp4` (name
  from the `.pvd`'s parent dir) via `geode_viz.paths`, falling back to a
  sibling `<dir>.mp4`.
- `--frames-dir`: directory for the `frame_%04d.png` images. Default: a
  `frames/` subdirectory next to the `.pvd`.
- `--fps`: frames per second for the MP4 (default `10`).
- `--frames-only`: render the PNG frames but skip the ffmpeg stitch.
- `--ffmpeg`: ffmpeg binary name or path (default `ffmpeg` on `PATH`).
- `--slice` / `--field` / `--colormap` / `--size`: identical to
  `pvbatch_render.py` (shared `render_core`) — axis-aligned slice plane,
  `PointData` array to colour by (default `|E|`), colormap preset (default
  `"Viridis (matplotlib)"`), and frame size in pixels (default `1200 900`).

The camera is reset once on the first frame and then frozen so the sweep
doesn't jitter as the per-frame field range shifts. Output goes to the
gitignored `artifacts/viz/` tree — never commit frames or MP4s.

## Adding a new plot module

Phase 1B/1C/1D land plot scripts under
`tools/viz/geode_viz/plots/<benchmark>.py`. The shape every module
follows:

```python
"""Spiral inductor plots (issue #278, Phase 1B)."""
from geode_viz import apply_style, artifacts_dir, footer, load_results

def main() -> None:
    apply_style("light")
    results = load_results("spiral_inductor")
    fig, ax = plt.subplots(figsize=(6, 4))
    # ... build the plot from results[...] ...
    footer(fig, results)
    fig.savefig(artifacts_dir("spiral_inductor") / "L_vs_freq.png")

if __name__ == "__main__":
    main()
```

The contract every plot module honors:

1. Call `apply_style()` exactly once at the top of `main()`.
2. Pull data from `load_results()` — no direct `tomllib` opens.
3. Write outputs under `artifacts_dir("<benchmark>")`.
4. Stamp `footer(fig, results)` before `savefig`.
5. Stay headless — no `plt.show()` in `main()`.

Following the contract is what makes the per-benchmark plot families
visually consistent.

## Patterns

- The `[meta]` block convention (`description`, `fixture`,
  `fixture_sha256`, `generated_at_commit`, `notes = [...]`) is
  *reused*, not re-invented. See e.g.
  `benchmarks/spiral_inductor/results.toml`.
- Loader / package shape mirrors `reference/numpy/` (PEP 420 namespace
  packages, package-qualified imports). When `geode_viz.plots.<X>`
  modules land in Phase 1B+, they import siblings as
  `from geode_viz.io import load_results`, not via relative `.io`
  imports — consistent with the `reference/numpy/` convention from
  commit `fd0a586`.
