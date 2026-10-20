# Motor paper, revision 9

State: **DRAFTED; fresh independent review and audit pending.** Author: Robb Walters.

V9 corrects the v8 audit flags in v8, which revised v7, which added the preregistered matched optimization study of #1185 (Section 7): 100 budget-matched runs of five optimizers in two design spaces, judged at a common order-93/L2/g20 validation, followed by the final refinement checks of the selected designs. V8, v7 and earlier versions are unchanged; the v8 audit is archived in `.8.audit/`; v7 reviews (Opus 37/44, Fable 37/44, both advance) and its audit (four critical wording flags) are archived beside it. See `changelog.md` for every change and for the protocol deviation that replaced closed capture by inventory-bound verification.

## Rebuild

From this directory:

```sh
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_v6.py
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python figures/src/render_exp2.py
bash ../../build.sh .
python3 -I -B verify.py
python3 -I -B verify_exp2.py
```

The local class and committed `main.bbl` make the paper standalone. The renderer produces seven manuscript figures, three manuscript tables, inherited supplements, and `figures/opt120-waveforms.pdf`. The waveform figure, now included in the manuscript, compares h45/h93 coupling torque and conforming outer-band Arkkio on the fresh L3 Opt120 meshes at fixed unscaled full current over 40 angles. Its left panel preserves each mean; its right panel subtracts each curve's own 40-angle mean to reveal ripple. It is not a reference-current reproduction comparison.

The stdlib verifier independently reconstructs the numerical claims from raw arrays, without importing renderer helpers. It checks 115 input hashes, original forward/gradient controls, 704 distinct FD cases, 592 Taylor windows, 3,240 candidate solve statuses, reference refinement, the v6 audit regressions, and size normalizations, matched-current ratios, refinement increments and paired-estimator comparisons. This does not rerun FEM or replace independent scientific review.

## Frozen evidence

`evidence-manifest.json` records each copied input's source path, full source commit and SHA-256. Snapshots match their **pinned source commits**, not necessarily a later main branch. In particular, the harmonic-study README predates a subsequent update note.

- Original evidence: `581ee7ad40c9646f43e0d6df3de7b5a87d06f798` and `910593bad6f64f9f01671c2632b57034e72b9c61`.
- Candidate validation: `224332c526be1191183af82b610973c7778de720`. V5 added the L1 fresh-export h45 record and alternate fresh-construction L2/L3 h45 records to the prior L2/L3, current-policy and angular controls. One invalid moved-mesh record is retained.
- Independent reference refinement: `8acd21ecf85d89fe077275a3c04ca70b53963d43`.

The upstream source is ICEMsnapshot `b145aaf4ab1a6424828bc941eaf172f48fa0d458`; its license and attribution are preserved. Reference runs use Octave 11.3.0, GeoPDEs `53ae5ca1ce16fe81057047831cecbaa859c49080`, and nurbs 1.4.3. The source benchmark READMEs provide mesh and solver commands and environment details.

## Interpretation

The 0.10% cogging agreement is between finite-resolution results, with larger remaining refinement changes; it is not an accuracy bound. Opt120's ripple reduction survives, but its magnitude changes with interface order. Its mean-torque comparison changes with both motor size and current policy. Removing both explicit size factors gives 1.4971 N m at fixed unscaled current (+3.7%), 1.4291 N m at fixed physical current (−1.0%), and 1.7061 N m under fixed group-1 density (+18.2%). Length-only normalization at fixed unscaled current gives 1.6434 N m (+13.9%) while retaining s² = 1.098. The matched-reference-current objective reduction is 10,170-fold in GEODE versus 11,982-fold in the saved reference; 7,631-fold instead holds unscaled current fixed. Reference refinement at Opt120 remains unperformed.

The main study follows #1185 amendments 7 and 8, the amendment 8 addenda and the later refinement deviation recorded in comment 6284369486. D is a coarse finite-element competitor; no magnetic-circuit or waveform model is compared. No smoke-run result enters this paper. See `changelog.md` for every review response and `../EXPERIMENTS.md` for the experiment handoff.

## Matched optimization data

`figures/src/data/exp2/runs.json` holds, for each of the 100 runs, its validation history, target attainment, 60-unit checkpoint and charged time. `extract_exp2.py` produced it from the study's per-run traces. Each trace's SHA-256 is recorded and checked against `provenance/exp2-main-inventory-0650f4ee.sha256.txt`. That inventory of all 5,453 study files was taken before any host reboot. Its existence is timestamped by RFC 3161 tokens from FreeTSA and DigiCert (`inv.*.tsr`, both 2026-10-20 03:50:04Z) and by OpenTimestamps (`.ots`, Bitcoin block 972256). These prove the inventory existed by those times, not when the study ended. The complete study tree is retained in two verified copies and an EBS snapshot. `provenance/setup/` holds the session, calibration and anchor-recomputation records, each bound to the inventory. `refinement.json` holds the 213 final-check results for the 33 selected designs. `verify_exp2.py` recomputes target credit from the validation records and checks every reported ratio and count.
