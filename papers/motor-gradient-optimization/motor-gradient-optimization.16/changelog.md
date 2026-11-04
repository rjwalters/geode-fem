# Revision 15 → 16

Inputs: `motor-gradient-optimization.15.review/` (`verdict.md`, `review-opus.md` 41/44, `review-fable.md` 40/44; both advance, no critical flags) and the coordinator's v16 instructions. This is a small text-only revision: no restructuring and no new claims. V15 and earlier versions are unchanged.

**No earlier number changed.** Every new or relabelled number is recomputed or bound in `verify_exp2c.py` / `verify_exp2b.py`.

**Outstanding dependency:** `[PENDING zenodo-doi]`. Pending; the marker is carried forward and is the only one.

**Results (measured on the rebuilt PDF):**
- Build: 38 pages, 0 overfull boxes, no `??`. The main text is pp. 1–14 (Conclusion on p. 14); Code and data availability starts p. 15.
- Abstract: 242 words by the verifier's count (`$…$` replaced by one token, `--` split). It is also 242 by a `pdftotext` word count. Fable's macro-stripped count gives 241.
- Verifiers:
  - `verify.py`: 20 PASS.
  - `verify_exp2.py`: 4.
  - `verify_exp2b.py`: 6 (7 with `MOTOR_RAW_ARCHIVE`).
  - `verify_exp2c.py`: 6 (8 with both raw sessions).
- Lints:
  - `numeric_consistency`: pass.
  - `audience_check`: pass, 0 hits.
  - `pending_marker`: `zenodo-doi` only.
- **Correction to the v15 changelog:** it recorded 39 pages. That was the `pdftotext` page split, which includes a trailing empty page. The v15 PDF has 38 pages, as both reviewers found.

| Source | Finding | Resolution |
|---|---|---|
| v15.review R1 (Opus) | Cost-check residual attributed to A′ alone | App. D "Cost estimate and measurement" now states that the model omits the per-evaluation overhead of both arms (evaluator start-up, mesh motion, optimizer work) and uses whole-run medians. It gives A′'s to-crossing excess (10–17%) and the FD arms' deviations from their model (+7% for F and F_w in space A; −5% F, +1% F_w in space B). The model errs most for A′ (asserted). The one-sided "within 4%" closure is removed, and §5 says App. D "discusses" the excess. All values are recomputed from the timelines |
| v15.review R2a (Opus) | Cold space-B factor 4.10 vs abstract's 4.11 | §5 and §7 now say "over the four starts where B attains the target, 4.10 and 1.9" |
| v15.review R2b (Opus) | 1.2% pooled F and A′ | Now F's own median, 1.0%. The verifier computes F alone, keeps the pooled 1.2% as an internal check, and asserts the old phrase is absent |
| Opus A1 | Unit on the space-B offset; SW3 slope sign | "$-3.11\times10^{-3}$ N² m²/m". App. D now compares SW3's slope "in magnitude" with MH's and SW2's; §5 keeps the signed values |
| Opus A4 | "That advantage" antecedent | Conclusion: "comparable factors to SLSQP's advantage over COBYQA" |
| Fable A1 | "as for F" read across the sweep | §5: "unlike F's, this error changes sign at $10^{-7}$ m" (asserted from both sweeps) |
| Fable A2 | Per-space zero-update split and fraction | "3.2% in all"; "SW1, SW4 or Sr1 in space A and SW4 or Sr1 in space B" (asserted) |
| Fable A6 | Ten-number sentence in §5 Gradient cost | Split into "First, … medians …" and "An evaluation with a gradient therefore costs …". The benchmark-paragraph "and … and" nit was not changed (no restructuring) |
| Opus A3 / Fable A8 | Changelog counts | Corrected above |
| verdict | BRIEF.md and EXPERIMENTS.md still describe v6 | Both updated briefly: current state (v16 package, #1185 main study, #1218/#1224 ablations, archive, pending DOI) and the current headline. The v6-era sections are marked historical |
| Opus A2 | Package only on a branch | Operator-owned; unchanged |
| Opus A5 | "In time units" for the cold optimizer factor | Not changed (no new text beyond the required scopes) |
| Related-work leads (verdict) | Adjoint-FE frameworks; Nikolova/Bakr EM adjoint-vs-FD timings | Not added: v16 adds no new claims or citations |
| Length | ≤14 pages | Main text still ends on p. 14 |
