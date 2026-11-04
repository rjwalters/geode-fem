# Motor paper, revision 4

State: **REVISED; fresh independent reviews and audit pending.** Author: Robb Walters. Venue is open. No new optimization or time-to-target result is claimed.

This version adds reviewed candidate validation (PR #1182) and independent reference refinement (PR #1191), alongside corrections from both v3 reviews and its passing audit. V1–v3 are unchanged. The original v3 reports are archived verbatim in `.3.review/` and `.3.audit/`; their 38/44, 37/44 and PASS verdicts apply only to c5f83e45, not this revision.

## Rebuild

From this directory:

```sh
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_v4.py
bash ../../build.sh .
python3 verify.py
```

The local class and committed `main.bbl` make the paper standalone. The renderer creates six manuscript figures, three generated manuscript tables and inherited supplemental plots/tables. `refinement_data.py` reads raw torque records; it does not solve a field problem. `verify.py` separately reconstructs the new statistics without importing the renderer's helpers. It checks 112 input hashes, forward statuses, 704 distinct loaded finite-difference cases, 592 Taylor windows, candidate objective/current-rule comparisons, the reference reproduction and refinement, and L3 gradient comparisons. It does not replace independent review or rerun FEM.

## Frozen evidence

`evidence-manifest.json` lists source path, full commit and SHA-256 for every copied input:

- Original evidence: `581ee7ad40c9646f43e0d6df3de7b5a87d06f798` and `910593bad6f64f9f01671c2632b57034e72b9c61`.
- Candidate validation: `224332c526be1191183af82b610973c7778de720`. The selected fresh export/NURBS records cover L2/L3, h45/h93/conforming, raw 40-angle waveforms and current-rule controls. One failed L3 moved-mesh record is included to substantiate the remesh requirement. The complete experiment, including other failures, is linked in the manuscript.
- Independent reference refinement: `8acd21ecf85d89fe077275a3c04ca70b53963d43`. Includes 3×3 reproduction, 6×6/12×12/24×24 raw runs, tolerance controls and source-preparation provenance.

The upstream source is ICEMsnapshot `b145aaf4ab1a6424828bc941eaf172f48fa0d458`; the included license and attribution are preserved. Reference runs use Octave 11.3.0, GeoPDEs `53ae5ca1ce16fe81057047831cecbaa859c49080`, and nurbs 1.4.3. Source benchmark READMEs provide full solver/mesh commands and environment details.

## Interpretation

Refining the reference accounts for the original cogging discrepancy at the matched interface order. It does not certify physical accuracy or full convergence. Opt120 preserves a large ripple reduction across the tested models, but its objective is sensitive to interface order and its mean-torque feasibility changes with current policy. Fixed unscaled current, fixed physical current and fixed group-1 density are reported separately. Opt120 is outside the prospective fixed-size design spaces.

The prospective method comparison remains unmeasured. An empirical RBF surrogate is not a reduced physical model; further evidence is needed for the full research hypothesis. Existing derivative timings were collected on a loaded host. See `changelog.md` for every review response and `../EXPERIMENTS.md` for the experiment handoff.
