# Audit flags for conformal-antenna-diffopt.3

## Critical flags (block advancement to AUDITED)

**None.**

- Build: CLEAN. `pdflatex → bibtex → pdflatex ×3` (5 total invocations), every exit code 0. Converged at pass 4 (`main.aux` pass 3 == pass 4, byte-identical; the pass 2→3 `.aux` delta was a `\citation{hooten2025}` line reorder only, not a content change). Floor of 2 post-bibtex passes satisfied. Final pass carries no `Label(s) may have changed` rerun warning. TEXINPUTS pointed at the `anvil-paper.cls` template dir.
- No unresolved citations `[?]` and no unresolved cross-references `??` in the final PDF (0 occurrences in extracted PDF text; the undefined-citation/reference warnings in the log are confined to passes 1–2, before bibtex resolution settled — expected, not a flag).
- 9/9 citation keys resolve to `refs.bib`. 0 claim-support failures.
- 0 numerical inconsistencies; every value re-traces to the committed artifacts. No regression vs the AUDITED-clean `.2`.
- Placeholder caption text confirmed GONE from the rendered PDF (0 occurrences of "Placeholder" in extracted PDF text; the sole remaining occurrence in `main.tex` is a `%%` provenance comment, which does not render).

## Non-critical notes

- **Unverified citations (9):** no `refs/` directory exists for this thread, so claim-support for all 9 literature cites could not be verified against on-disk sources. Recorded per the paper-audit contract as `unverified — source not on disk`, not flagged. The author is responsible for off-disk verification. This is unchanged from `.2` (refs.bib byte-identical).
- **Named-but-uncited references (3):** the ceviche/Hughes-2019 paper, the Meep adjoint-solver docs, and a canonical antenna-topology-optimization reference are named in prose without a `\cite` key and surfaced as declared future-litsearch gaps. They invoke no unresolved key and produce no `[?]` — correct handling, not a finding.
- **Cosmetic-only provenance:** the v2→v3 `main.tex` diff is prose-only (caption placeholder strip ×3, "intractable" scoping to single-process/single-node Meep 1.34.0 on a 61 GB host ×4, `-12.06` dB parenthetical de-dup ×2). `refs.bib` and `figures/` are byte-identical / unchanged from `.2`. No number, claim, method, or citation changed — v3 remains AUDITED.

## Verdict

Zero critical flags. `.3` was `READY` (reviewer advanced) → thread is now **AUDITED** (terminal).
