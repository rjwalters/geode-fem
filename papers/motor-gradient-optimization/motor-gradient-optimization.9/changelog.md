# Revision 8 → 9

V8 is preserved unchanged with its audit (`.8.audit/`: READY-WITH-AUDIT-FLAGS; all four v7 critical flags resolved; one new critical flag). V9 corrects that flag and the advisory notes below. No numerical result changes.

| Source | Finding | Resolution |
|---|---|---|
| v8 audit C1 | Discussion: every method's best space-A design "near 0.043F0", but C's is 0.049F0 | Four of five reach 0.043F0, C 0.049F0, all on the active floor; the conclusion is limited to the choice among A, B, D and E. Verifier forbids the old wording |
| N1 | Discussion lacks the "wherever both reach" qualifier | Added |
| N2 | Abstract lost two scope qualifiers | "under fixed unscaled current" for the 3.6× result; the initial/L1/order-45/full-current state for the three reversed sensitivities |
| N3 | Text ranges rounded to one decimal disagree with Table 6's two decimals at 2.45 | Text uses the table's two-decimal geometric means (abstract and introduction 2.45–7.79) |
| N4 | "Tie" for 0.9925 | "Near tie: D is 0.75% faster, within the timing noise between repeats" |
| N5 | "Keep less of their reduction, by factors" applies the factor to the wrong quantity | The remaining objective fraction at the held-out currents is 1.23–1.73 times that at the training currents (0.89 for B's best) |
| N6 | Fig. 8 plots (F+δF)/F0, whose space-A level is 0.048 | Stated in the text |
| N8 | Verifier hygiene | Version-neutral docstring and PASS lines; the held-out minimum is checked exactly; the no-op `replace` is removed, so every phrase check is strict |
| N9 | "Data-profile practice" for both citations | "Data- and performance-profile practice for benchmarking optimization solvers" |
| N11 | Link labelled "binary" points to source | "The source of the final-refinement program" |

Not changed: N7 (the instance type comes from the launch record in `refs/main-run-runbook.md`, not the bundled session records) and N10 (metadata-level checks of four citations, all resolved on Crossref or the publishers' records).

## After the v9 audit

The v9 audit (`.9.audit/`) passed with no critical flags. Two advisories were applied afterwards. A1, a clarifying clause: the Fig. 8 level near 0.048 is that of A, B, D and E; C's is near 0.054. The verifier binds both values to repeat 1. A2: the README wording and state line. No number in the PDF changed.
