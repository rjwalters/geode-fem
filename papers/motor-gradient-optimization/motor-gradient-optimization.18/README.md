# Motor paper, revision 18

State: **DRAFT (v18, not yet reviewed).** A reader-clarity revision of v17 for a reader from tensor compilers and ML systems who is not a motor or EM-FEM specialist, and the addition of a coauthor. One outstanding dependency remains from v17: `[PENDING zenodo-doi]`, the operator's deposit of the raw-trace archive (unchanged). V18 adds `[PENDING coauthor-affiliation]`. Authors: Robb Walters and Crutcher Dunnavant.

V18 renames the design spaces (A/B to S8/S18) and the finite-difference arms (F/F_w to FD-cold/FD-warm), annotates the geometry figure, labels the targets and start identities in the study figures, adds tables of methods, parameter sets and operation costs, and adds a decomposition of SLSQP's advantage over COBYQA into a gradient factor and an optimizer factor (Table `tab:decomp`, Fig. `fig:decomp`), computed from the committed timelines and extracts. No measured result changed and no solver was rerun. `changelog.md` maps every change to its reason. V17 and earlier versions are unchanged.

Package: <https://github.com/rjwalters/geode-fem/tree/main/papers/motor-gradient-optimization/motor-gradient-optimization.18>.

## Rebuild

From this directory:

```sh
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_v6.py
# (render_v6.py runs render_geometry.py for Fig. 1; it alone needs figures/src/data/cp_table.json)
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python figures/src/render_exp2.py
python3 -I figures/src/render_exp2_validation.py
python3 -I figures/src/render_exp2b.py
python3 -I figures/src/render_exp2c.py
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python figures/src/render_exp2b_figure.py
uv run --with numpy==2.5.4 --with matplotlib==3.11.2 python figures/src/render_decomp.py
bash ../../build.sh .
python3 -I -B verify.py
python3 -I -B verify_exp2.py
python3 -I -B verify_exp2b.py
python3 -I -B verify_exp2c.py
# optional exact re-derivation of every extract from the unpacked raw-trace archive:
MOTOR_RAW_ROOT=<unpacked>/motor-paper-raw-traces python3 -I -B verify.py
```

`verify.py` runs `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py` at its end, so the package has four standard-library verifiers with one entry point; `.anvil.json` lists `verify.py`, `verify_exp2b.py` and `verify_exp2c.py`. The local class and committed `main.bbl` make the paper standalone. The verifiers need only this directory and Python's standard library: they also run when the directory is downloaded on its own, and then skip only the check of the thread-root `.anvil.json` (they print a NOTE). Of the rebuild steps above, only `bash ../../build.sh .` needs the repository layout; in a lone copy, `pdflatex main` (run until the references settle) builds the paper from the local class and the committed `main.bbl`.

With `MOTOR_RAW_ROOT` pointing at the unpacked archive, the verifiers additionally check every archived file against the archive's `MANIFEST.sha256` and re-derive, byte for byte or exactly, the run extracts, final-check extracts and evaluation timelines of all three sessions, the two ablations' selection checks and the binary-equivalence comparison. The older per-input variables (`EXP2_SESSION`/`EXP2_REFINE`, `EXP2B_SESSION`/`EXP2B_REFINE`/`EXP2B_SELECTION`, `EXP2C_SESSION`/`EXP2C_REFINE`/`EXP2C_SELECTION`) still work and override the archive layout.

The stdlib verifiers independently reconstruct the numerical claims from raw arrays, without importing renderer helpers. They check 130 input hashes, original forward/gradient controls, 704 distinct FD cases, 592 Taylor windows, 3,240 candidate solve statuses, reference refinement, size normalizations, the matched study (100 runs, 213 refinement checks), the ledger charges of all 100 runs, the validation-charge table, training/validation objective ratios, the training-gradient FD records of both spaces and the preregistration export, and, for the two post-hoc ablations, the 40 FD-cold-session and 20 FD-warm-session traces, their final checks, accuracy records, protocol timing and evaluation timelines. `verify_exp2c.py` also checks that this README's archive size, digest and member counts equal `deposit-record.json` and the paper, and that its headline numbers match the paper's; v18 adds the gradient/optimizer decomposition, the time split and the cost table, recomputed from the timelines and extracts. This does not rerun FEM or replace independent scientific review.

## Raw-trace archive

`motor-paper-raw-traces.tar.zst` is deposit-ready but not uploaded. It holds the raw traces of all three sessions (main study, cold-stencil ablation FD-cold, warm-stencil ablation FD-warm; the archive names them F and F_w), the 5,453, 2,118 and 1,180 inventory-listed files with their inventories and timestamp records; the three sessions' final-check cohorts (`bundle/`, `tools/`, `results/`, `report.json`); the two ablations' independent selection outputs; and the binary-equivalence record with the rebuilt binary (`c75fe847`, built from `63b3e3d8`). The refinement mesh cache is not included: no extract reads it, and the repository regenerates its four meshes from `63b3e3d8` with hash checks (`benchmarks/motor_optimization/final_refinement/smoke/REGENERATE.md`). `MANIFEST.sha256` at the archive root lists the SHA-256 of every other member (10,781 files); its copy is `figures/src/data/deposit/MANIFEST.sha256`. The archive is 2,790,582,024 bytes, SHA-256 `1dfbf3883a9f1f3d8ddc44f67ce594b88291ec235a154337d24efa07a1b98b07`, and was built by `figures/src/build_deposit.py`; its record is `figures/src/data/deposit/deposit-record.json`. The DOI is pending the operator's deposit (`[PENDING zenodo-doi]` in the data statement of `motor-gradient-optimization.18`). Set `MOTOR_RAW_ARCHIVE=<path>` to make `verify_exp2b.py` re-hash it, and `MOTOR_RAW_ROOT=<unpacked>/motor-paper-raw-traces` to re-derive the extracts from it. The earlier archives (two sessions; three sessions without the final-check results) are superseded and are not to be deposited.

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

In the preregistered study, SLSQP with adjoint gradients beat COBYQA at the same training resolution in all 45 of the 50 paired comparisons where both succeeded, by geometric means of 3.10 (S8, 8 parameters, target T90) and 7.79 (S18, 18 parameters, target T99). Two post-hoc ablations kept SLSQP and replaced the adjoint by forward differences, each under a protocol frozen before any run and with one single-threaded process per run; they split that advantage into a gradient factor and an optimizer factor. The adjoint reached the primary target 1.71 and 2.70 times sooner than warm-started finite differences (FD-warm), and 2.47 and 4.11 times sooner than cold-started ones (FD-cold). Over the starts where COBYQA attains the target, the gradient factor is 1.71-2.47 (S8) and 2.68-4.10 (S18) and the optimizer factor, COBYQA against finite-difference SLSQP, 1.22-1.76 and 1.91-2.92; which end applies depends on the finite-difference variant. The adjoint cuts the gradient's share of the time to target from 45-77% to about 5%, so validation and training forwards bound the end-to-end gain. The 0.10% cogging agreement is between finite-resolution results, not an accuracy bound. D is a coarse finite-element competitor; no magnetic-circuit or waveform model is compared.

## Post-hoc ablation data (method FD-cold, F in the traces, and control A′)

`figures/src/data/exp2b/runs.json` and `refinement.json` (`extract_exp2b.py`) are compact extracts of the 40 ablation traces and of the 90 final checks of the 14 selected designs. Each trace's SHA-256 is checked against `provenance/exp2-fd-ablation-inventory-c2bc65ff.sha256.txt` (2,118 files, digest `c2bc65ff61fb8485d14a1e2b07dadb3f9c9fa4b040ce98aa1360f46f64788a83`), taken on the host before it stopped; RFC 3161 tokens from FreeTSA and DigiCert (both 2026-10-21 06:18:42Z) and an OpenTimestamps receipt (upgrade pending at packaging) are beside it. The session ran at producer commit `5cc86ad8` (c7a.4xlarge, 15 workers, unit 66.66 s; the main session's was 70.94 s). The full session is retained in two verified copies and EBS snapshot `snap-08f4a25ff2fdd727e`. `provenance/setup/` holds the session, calibration and anchor-recomputation records (inventory-bound); `provenance/protocol-1218-export.json` exports the protocol and its freeze comment with GitHub timestamps; `refinement-tools/` holds the direct refinement's prepare/report/run scripts and cohort; `selection-check.json` records the digest of the independent selection program's output and the designs it chose (identical to the cohort, bit for bit); `fd-gradient-runtime-checks.json` (pinned at `2d4fbcfe`, in the evidence manifest) is the L0 accuracy record of FD-cold's gradients.

## Warm-stencil arm data (method FD-warm, F_w in the traces, and its control A′)

`figures/src/data/exp2c/runs.json` and `refinement.json` (`extract_exp2c.py`) are compact extracts of the 20 FD-warm-session traces and of the 84 final checks of 13 selected designs. They are bound by SHA-256 to `provenance/exp2-fd-warm-inventory-df509322.sha256.txt` (1,180 files, digest `df5093225b52b74f622d4bfbd193c58ea24d6116eadb2f0a03b9e9830e8a2559`), which is timestamped by FreeTSA and DigiCert (22:48:56Z) and OpenTimestamps. The session ran at producer commit `669029c5` with unit 69.09 s, and is retained in two verified copies and EBS snapshot `snap-09f336c1928d1cddb`. Set `EXP2C_SESSION`, `EXP2C_REFINE` and `EXP2C_SELECTION` to re-derive the extracts from the raw copy.

Other contents:
- `gradient-checks/`: the L1 warm accuracy records, pinned at `669029c5`.
- `binary-equivalence/`: the rebuilt refinement binary's 12 results beside the 12 original results; the verifier compares every non-timing field.
- `provenance/protocol-1224-export.json`: the #1224 protocol and its freeze comment with GitHub timestamps.
- `timeline.json` (here and in `exp2b/`, v15, `extract_timeline.py`): for every SLSQP trace, the charged time at which each training evaluation and each finite-difference gradient began, with the trace's SHA-256; used for the gradient shares up to each crossing.

## Geometry and decomposition inputs (v18)

- `figures/src/data/cp_table.json`: the 55 control points (group, part, direction, initial position), copied from `benchmarks/motor_reference/parametric_cp/cp_table.json` at `6129bf68` and listed in the evidence manifest; `render_geometry.py` draws Fig. 1 from it and the reference mesh.
- `figures/src/render_decomp.py` writes `tables/decomp.tex` and `figures/decomp.{pdf,png}` from the three sessions' `runs.json` and `timeline.json`. A′'s adjoint time is an estimate (evaluations before the crossing times the run's mean adjoint charge per evaluation); the finite-difference arms' gradient time is the recorded charge of the gradients begun before the crossing.
