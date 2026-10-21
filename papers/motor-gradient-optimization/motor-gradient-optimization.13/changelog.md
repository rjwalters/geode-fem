# Revision 12 → 13

Inputs: `motor-gradient-optimization.12.review/` (`verdict.md`, `review-opus.md` 38/44, `review-fable.md` 37/44; both advance, no critical flags) and the coordinator's v13 instructions of 2026-10-21. V12 and earlier versions are unchanged.

**No earlier number changed.** New derived quantities are recomputed by `verify_exp2b.py`:
- per-gradient costs (from a new `cost_fe` field in the ablation extract, re-derived exactly from the raw traces);
- training-solve costs and the share of F's evaluations that request a gradient;
- the fixed-sign per-component offsets at L1 and L0;
- the archive digest.

**Outstanding dependencies (hold READY):**
- `[PENDING fd-step-sweep]` (2 markers, §5 and Appendix D): a step-sweep/central-difference diagnostic at the failing L1 point, being measured in a separate PR.
- `[PENDING zenodo-doi]` (1 marker, data statement): the operator's deposit of the raw-trace archive.

No other marker remains; the verifier asserts exactly these two sources.

**Raw-trace archive (deposit-ready, not uploaded).**
- File: `/Volumes/Stripe/scratch-codex-review/motor-paper-raw-deposit/motor-paper-raw-traces.tar.zst`, 2,179,756,244 bytes, SHA-256 `2de2132cb6b94c768ace042ad8cd85051000aee148d520cd1843d1c10d40b2a2`.
- Contents: the 5,453 main-study and 2,118 ablation inventory-listed files, the two inventories with their RFC 3161 and OpenTimestamps records, and a README.
- Build: `figures/src/build_deposit.py` hash-checked every file against its inventory before adding it. The tar is deterministic (sorted, mtime 0, uid/gid 0), compressed with zstd -19 --long=27.
- Checks: `zstd -t` OK; member counts 5,453 / 2,118 / 10 / 1; digest re-checked.
- Record: `figures/src/data/deposit/deposit-record.json`.

**Results:**
- Build: 3 pdflatex passes, 0 overfull boxes, 0 undefined references, no `??`.
- `verify.py`: 13 PASS blocks. `verify_exp2.py`: 4. `verify_exp2b.py`: 5. `verify_exp2b.py` with the raw ablation copy and the archive: 7, including the exact re-derivation and the archive re-hash.
- `numeric_consistency` and `audience_check` pass. `pending_marker` reports exactly `fd-step-sweep` and `zenodo-doi`.
- Main text: pp. 1–14, data statement from p. 15. Total 36 pages. Abstract: 241 words.

**Length trims made to keep the main text at 14 pages, all moved or folded, nothing deleted:**
- the excitation-rule definitions moved to Appendix H;
- the mean/std display equation inlined;
- the §7 plateau sentence shortened;
- the reduced-model citations moved from related work into the §7 limits sentence;
- the Conclusion tightened.

| Source | Finding | Resolution |
|---|---|---|
| v12.review R1 (both) | Package pointer names `.11` | The data statement names `motor-gradient-optimization.13`. `verify_exp2b.py` asserts that the named path equals the verifier's own directory and that no older version is named |
| v12.review R2 (Opus R2, Fable A4) | SW3 "noise floor" reading contradicted by the record | Interpretation removed (the verifier asserts "noise floor" is absent). Appendix D states the measured fact. For every one of the eight scalar components, the FD-minus-adjoint error of the mean gradient has the same sign at all four points, at L1 and at L0. SW3's error is nearly constant (−1.8 to −1.9e-3 at L1, −3.0 to −3.1e-3 at L0, three distinct designs), so the failure is this offset divided by a small adjoint value. "Design-independent" holds for SW3 (and MH, SW2) but not for every component (MAG, SW1, SW4 vary up to about 4×), so the paper says "same sign", not "design-independent". Moré & Wild frame the step/noise question. Whether the offset is one-sided truncation or adjoint bias is left to `[PENDING fd-step-sweep]` |
| v12.review R2 / coordinator 2 | Move SW3 detail out of §5 | §5 has a new "Gradient accuracy" paragraph (Fable A6): norm-wise result plus one sentence pointing to Appendix D, then the pending marker. The SW3 values, the pattern and the L1-check timing (Opus A3: run after the ablation, not part of its frozen protocol) are in Appendix D |
| v12.review R3 (Opus R3, Fable R2) | Undefined "step" cost; 10.9 not reproducible | The step-cost sentence and the 5.3/10.9 ratios are removed. §5 defines the unit (the median cold validation solve, 66.66 s) and reports cost per gradient: F 1.90 and 4.25 units (about 0.24 per stencil solve), A′'s adjoint 0.057 and 0.061. Every training evaluation also costs a 0.23–0.24 training solve in both arms. It says why the no-validation ratios (5.58, 10.07) are below the per-gradient ratio: only a median 68% and 77% of F's training evaluations request a gradient. No printed ratio needs unprinted inputs |
| Fable A1 / coordinator 3 | Fig. 5: show the estimand | Each start's F/A′ ratio is printed above its pair (`render_exp2b_figure.py`); x-axis labelled "start"; the overlapping annotation is removed and the caption explains the cross. Caption updated |
| Fable A3 / coordinator 3 | Explain F's 8/10 budget stops | §5: in space A, F's SLSQP met its stopping test in 2 of 10 runs, and the extra time bought nothing (best objective within 0.4% of A′'s). Why it rarely met `ftol` 1e-15 was not tested and is stated as untested. The stops cannot affect time to target, because every crossing happens at the same validation count |
| Opus A5 / coordinator 3 | "times slower" | "took 2.5 and 4.1 times as long" (the verifier asserts "times slower" is absent) |
| Opus A4 / coordinator 3 | "Essentially the same path" | "Up to each target crossing the two take the same path, crossing after the same number of validations"; the introduction matches; post-crossing divergence is reported (verifier asserts the old phrase is absent) |
| Opus A6 / coordinator 3 | Reconcile "n-fold compute" | §7: the ratios compare compute but are smaller than the per-gradient cost ratio because shared validation and gradient-free line-search evaluations are included |
| Fable A5 / coordinator 3 | Intro paragraph 2 restates the abstract | The main-study ranges are removed from the introduction; 2.47/4.11 appears there once |
| Opus A1 / coordinator 3 | Title: per-core condition | Adopted: "Per core, adjoint gradients reach validated torque-ripple targets 2.5 and 4.1 times sooner than finite differences in a nonlinear motor model". It reads cleanly and pre-empts the parallel-stencil objection |
| Fable A8 / Opus A7 / coordinator 3 | Stale `_progress.json`; README 123 vs 125 | `_progress.json` note names the two outstanding dependencies. README reports 125 input hashes (the manifest count) and a v13 state line. The data statement says "three standard-library verifiers" once (Fable A9) |
| Opus A2 / Fable A2 / coordinator 4 | Related-work leads | Crossref/OpenAlex only, no identifier sent (`citation-metadata-v13.json`, `citation-notes.md`). Added Lei et al. 2017, Peter & Dwight 2010, Moré & Wild 2011 and 2012, Kenway et al. 2019. Peter & Dwight and Kenway have no retrievable abstract and are cited only for what their titles state. **Not added:** Lyu, Xu & Martins 2014 (ICCFD8), since neither service has a record, so positioning against it was not possible |
| Opus A7 / Fable A10 / coordinator 5 | Raw traces private | Deposit-ready archive built (not uploaded); the data statement names it with byte size and SHA-256 and carries `[PENDING zenodo-doi]`. `figures/src/data/deposit/deposit-record.json` records the digest, size and file counts. `verify_exp2b.py` checks that the record matches the text and the two inventories, and re-hashes the archive when `MOTOR_RAW_ARCHIVE` points at it |
| Opus A9 / Fable A7 | Method/space letter collision | Declined again: renaming spaces in prose, tables and figures is high-risk for a decoding nit |
| Opus A10 | Fig. 5 x-axis label | Added |
