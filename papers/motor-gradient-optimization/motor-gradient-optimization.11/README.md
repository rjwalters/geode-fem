# Motor paper, revision 11

State: **REVISED after the v10 reviews (Opus 35/44, Fable 35/44, both advance, no critical flags), with the post-hoc finite-difference SLSQP ablation (method F, issue #1218) now filled in; no outstanding dependency.** Author: Robb Walters.

V11 retitles the paper to the ablation-supported result ("Adjoint gradients reach validated torque-ripple targets 2.5 to 4 times sooner than finite differences in a nonlinear motor model"), restructures it so the optimization result leads (main text and data statement 18 pages, appendices A–H), decomposes the charged time into validation and the rest, reports the training-gradient checks of both design spaces, specifies method C, and reports the ablation in §4.3 (labelled post hoc; protocol frozen before its runs). No pre-existing number changed; new derived quantities are recomputed by `verify_exp2.py` and `verify_exp2b.py`. V10 and earlier versions are unchanged. See `changelog.md`.

V11 retitles the paper to what the matched study measured (adjoint-driven SLSQP against derivative-free search), restructures it so the optimization result leads (main text 18 pages, appendices A–H), decomposes the charged time into validation and the rest, reports the training-gradient checks of both design spaces, specifies method C, and adds a skeleton for the post-hoc ablation F. No reported number changed; new derived quantities are recomputed by `verify_exp2.py`. V10 and earlier versions are unchanged. See `changelog.md` for every change.

## Rebuild

From this directory:

```sh
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_v6.py
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python figures/src/render_exp2.py
python3 -I figures/src/render_exp2_validation.py
python3 -I figures/src/render_exp2b.py
bash ../../build.sh .
python3 -I -B verify.py
python3 -I -B verify_exp2.py
python3 -I -B verify_exp2b.py
# optional exact re-derivation of the ablation extracts from the raw session copy:
EXP2B_SESSION=<copy>/exp2-fd-ablation/session EXP2B_REFINE=<refine-dir> EXP2B_SELECTION=<selection.json> python3 -I -B verify_exp2b.py
```

`verify.py` runs `verify_exp2.py` and `verify_exp2b.py` at its end. The local class and committed `main.bbl` make the paper standalone.

The stdlib verifiers independently reconstruct the numerical claims from raw arrays, without importing renderer helpers. They check 123 input hashes, original forward/gradient controls, 704 distinct FD cases, 592 Taylor windows, 3,240 candidate solve statuses, reference refinement, size normalizations, the matched study (100 runs, 213 refinement checks), and, from v11, the ledger charges of all 100 runs, the validation-charge table, training/validation objective ratios, the training-gradient FD records of both spaces and the preregistration export. This does not rerun FEM or replace independent scientific review.

## Frozen evidence

`evidence-manifest.json` records each copied input's source path, full source commit and SHA-256. Snapshots match their **pinned source commits**, not necessarily a later main branch.

- Original evidence: `581ee7ad40c9646f43e0d6df3de7b5a87d06f798` and `910593bad6f64f9f01671c2632b57034e72b9c61`.
- Candidate validation: `224332c526be1191183af82b610973c7778de720`.
- Independent reference refinement: `8acd21ecf85d89fe077275a3c04ca70b53963d43`.
- Training-gradient FD records and the harness README (v11): `9486a9da58beeca91968d4d3608f06b998abff45`, the study's harness commit. `figures/src/data/exp2/gradient-checks/` holds `fd_L1_space_{a,b}.json` (Newton 1e-12) and `harness_fd_L1_space_{a,b}_{default,tight}.json` (the evaluator's gradient replies at the training tolerance 1e-8 and at 1e-12).
- Preregistration export (v11): `06a9a0d9d4dcae56cbbd260e70b96a1fc1a802a5`, copied to `figures/src/data/exp2/provenance/preregistration-1185-export.json` (snapshot 2026-10-18T09:34Z; the main session id is `main-20261018T181644Z`).

The upstream source is ICEMsnapshot `b145aaf4ab1a6424828bc941eaf172f48fa0d458`; its license and attribution are preserved. Reference runs use Octave 11.3.0, GeoPDEs `53ae5ca1ce16fe81057047831cecbaa859c49080`, and nurbs 1.4.3.

## Matched optimization data

`figures/src/data/exp2/runs.json` holds, for each of the 100 runs, its validation history, target attainment, 60-unit checkpoint and charged time (`extract_exp2.py`). `supplement.json` (v11, `extract_exp2_supplement.py`) holds, from the same 100 traces, the ledger's per-category charges, the charge of every validation entry (one extra entry per run stopped at the budget: the validation aborted there), the training objective paired with every validation and the training objective at θ0. Both extracts record each trace's SHA-256, which the verifier checks against `provenance/exp2-main-inventory-0650f4ee.sha256.txt`, the inventory of all 5,453 study files. Its existence by 2026-10-20 03:50:04Z is timestamped by RFC 3161 tokens from FreeTSA and DigiCert and by OpenTimestamps (Bitcoin block 972256); these prove the inventory existed by those times, not when the study ended. The complete study tree is retained in two verified copies and an EBS snapshot. `refinement.json` holds the 213 final-check results for the 33 selected designs.

## Interpretation

The contrast between SLSQP and COBYQA changes the search strategy together with gradient access, so it does not by itself isolate the value of the adjoint. The post-hoc ablation of issue #1218 (method F, forward-difference SLSQP, with an in-session A′ control) isolates it; its results are not yet available, and the title is to be revisited once they are. The 0.10% cogging agreement is between finite-resolution results; it is not an accuracy bound. D is a coarse finite-element competitor; no magnetic-circuit or waveform model is compared.

## Post-hoc ablation data (method F and control A′)

`figures/src/data/exp2b/runs.json` and `refinement.json` (`extract_exp2b.py`) are compact extracts of the 40 ablation traces and of the 90 final checks of the 14 selected designs. Each trace's SHA-256 is checked against `provenance/exp2-fd-ablation-inventory-c2bc65ff.sha256.txt` (2,118 files, digest `c2bc65ff61fb8485d14a1e2b07dadb3f9c9fa4b040ce98aa1360f46f64788a83`), taken on the host before it stopped; RFC 3161 tokens from FreeTSA and DigiCert (both 2026-10-21 06:18:42Z) and an OpenTimestamps receipt (upgrade pending at packaging) are beside it. The session ran at producer commit `5cc86ad8` (c7a.4xlarge, 15 workers, unit 66.66 s; the main session's was 70.94 s). The full session is retained in two verified copies and EBS snapshot `snap-08f4a25ff2fdd727e`. `provenance/setup/` holds the session, calibration and anchor-recomputation records (inventory-bound); `provenance/protocol-1218-export.json` exports the protocol and its freeze comment with GitHub timestamps; `refinement-tools/` holds the direct refinement's prepare/report/run scripts and cohort; `selection-check.json` records the digest of the independent selection program's output and the designs it chose (identical to the cohort, bit for bit); `fd-gradient-runtime-checks.json` (pinned at `2d4fbcfe`, in the evidence manifest) is the L0 accuracy record of F's gradients.
