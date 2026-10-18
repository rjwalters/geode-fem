# Main-study figure preparation

This is a downstream presentation tool for the complete output of
`benchmarks/motor_optimization/final_refinement/study_selection.py`.
It prepares common-validation histories and per-start target attainment;
it does not authenticate the original capture or certify final refinement.
No actual main-study outcome has been rendered during this preparation.

The producer interface was checked against PR #1207 at
`19cc493ed9992885cefcc3192657255ef1eeddc2`, which is still under independent
review. Recheck compatibility after any producer revision. The tool also
accepted its existing synthetic full-pipeline selection (100 jobs, 300 target
cells); that fixture is not measured optimization evidence.

## Inputs and outputs

First run the **reviewed** `study_selection.py` on a verified closed capture
using its README. Retain the output digest printed by that command and the
calibration unit from the verified frozen setup. Supply those independently
retained values here; merely hashing arbitrary input does not verify a study.
This renderer never reads a live session or a directory of optimizer traces.

```sh
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python \
  papers/motor-gradient-optimization/refs/main-study-figures/render.py \
  --selection "$SELECTION_JSON" --selection-sha256 "$SELECTION_SHA256" \
  --unit-seconds "$FROZEN_UNIT_SECONDS" --out "$NEW_FIGURE_DIRECTORY"
```

The output directory must not exist. Outputs are:

- `quality-by-start-repeat1.pdf` and `.png`: five starts × two spaces, with
  all five methods. Color identifies the optimizer and line style identifies
  training resolution. The 60-FE checkpoint is marked on the 120-FE axis.
- `quality-by-start-repeat2.pdf` and `.png`: the second timing repeat in the
  same layout and axis limits, suitable for a supplement. Repeats are not
  additional independent design starts.
- `target-attainment.csv`: all 100 jobs × T50/T90/T99. Unattained cells have
  `status=not_attained` and blank attainment values; actual stopping reason
  and charged duration remain explicit. A failed or early-stopped run is
  not assigned 120 FE of observed search or a fabricated speedup ratio.
- `plot-data.json`: input/output/source hashes, plotting version, exact
  series, 60/120-FE checkpoint values, validation decisions and excluded
  attempts. Excluded attempts remain separate from selected-run curves.

Curves use the best feasible `(F + delta_F) / F0`, only after completed,
eligible validations. They end at actual charged stopping times. Earlier
eligible crossings survive later scientific failure; no feasible observation
is a missing curve, never zero. All target-table entries and plotted points
come from the same prepared records. Ledger/unit, cohort, validation census,
trace identity, feasibility flags and target arithmetic are checked again.
The upstream verifier remains responsible for raw source/runtime/mesh and
waveform authentication. A consistent forged summary is outside this
presentation adapter's verification scope.

These plots show in-run common-validation costs. Shared setup/calibration,
excluded-attempt costs, post-run checks, CPU/RSS and the end-to-end comparison
need their separate final analysis. Final checks cannot retroactively change
these curves or target-credit times. Report refinement outcomes alongside
the affected conclusions. Different motion maps in spaces A and B also mean
their contrast does not isolate the ten added offsets.

## Checks and synthetic layout preview

```sh
python3 -I -m unittest discover \
  -s papers/motor-gradient-optimization/refs/main-study-figures -p 'test_*.py'
```

The tests cover the 60-FE boundary, failed runs with earlier crossings, absent
feasible validations, rejected observations, excluded attempts, incomplete
and ambiguous cohorts, wrong units, changed hashes and false target credit.
They launch no FEM or optimizer processes.

To inspect the layout, serialize `test_prepare.preview_fixture()` to a new
scratch JSON file and render it with unit 2 and **`--synthetic`**. The fixture
deliberately varies five artificial paths and includes failures and missing
curves. Both pages must visibly say **SYNTHETIC LAYOUT CHECK — NOT EXPERIMENTAL
RESULTS**. Keep these previews outside the manuscript and its evidence data.
The plots still need visual review with the actual completed data, at the
final manuscript's printed size; synthetic legibility is not that review.

The [retained layout preview](synthetic-layout.png) shows repeat 1 from the
artificial fixture. [Check results](checks.json) bind the scripts and preview
hashes, record both-repeat rendering and distinguish the separate existing
synthetic pipeline compatibility check.
