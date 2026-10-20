# Audit flags for motor-gradient-optimization.9

Audited head: `1813ec9c` (branch `claude/motor-paper-v7`). This verdict applies to this head only.

**Verdict: PASS. 0 critical flags, 0 outstanding dependencies.** The v8 critical flag C1 is resolved, and so are the v8 advisory notes the changelog claims to address (N1-N6, N8, N9, N11). N7 and N10 were deliberately left unchanged, and that is acceptable. The v8 → v9 diff touches only `main.tex`, `optimization.tex`, `verify_exp2.py`, `README.md` and `changelog.md` (plus build outputs). `refs.bib`, `figures/`, `tables/` and `figures/src/data/exp2/` are byte-identical to v8, so no number in the data, tables or figures changed. Every changed sentence was recomputed from `runs.json` and `refinement.json`. To do that, the v8 auditor script was re-run unmodified against v9 (`auditor-scripts/x9.py`). Its output matches v8's `x8.out` line for line, and every new or reworded value agrees with it (see numerical-audit.md). The build is clean: 4 pdflatex passes to the fixpoint, 26 pages, 0 undefined references or citations, 0 overfull boxes. The rebuilt PDF text is identical to the committed `main.pdf`. `python3 verify.py` passes all seven blocks.

State impact: the audit has no critical flags and no pending markers. There is no `motor-gradient-optimization.9.review/` sibling on disk, and the version README still reads "fresh independent review and audit pending". The thread therefore becomes AUDITED once v9 is READY (has a review with `advance: true`). If the operator treats the v7 reviews as carrying forward, it is AUDITED now.

## v8 critical flag: resolved

- **v8 C1 (space-A best designs "near 0.043F0")**: resolved. The Discussion (main.tex:801-805) now reads: "every method's best design sits on the active mean-torque floor, and four of the five reach the same $0.043F_0$ (C reaches $0.049F_0$). Within this budget and these methods, the parameterization and constraint, more than the choice among A, B, D and E, set the attainable ripple there." The recomputed values are A 0.04321, B 0.04320, D 0.04321, E 0.04332 and C 0.04893. All five lie 0.12-0.16% above the mean floor. The narrowed inference is supported. verify_exp2.py now binds `C reaches $0.049F_0$` and forbids the old `near $0.043F_0$, so`.

## Critical flags (block advancement to AUDITED)

None.

## Outstanding dependencies (hold AUDITED; not scored defects)

None. `anvil.lib.pending_marker` passes with 0 markers. It was run without `--write-review`, so no `.9.pending/` sidecar was written.

## Non-critical notes

- **A1. Unclear antecedent of "the level" in the Fig. 8 sentence** (optimization.tex:137-139). "...and C's reaches $0.049F_0$; in Fig.~8, which includes the allowance, the level appears near $0.048$." The phrase follows C's value directly, so a reader can take "the level" to mean C's. C's plotted repeat-1 minimum is 0.0538. The 0.048 level is the A/B/D/E plateau (repeat-1 minima 0.04752-0.04765; `auditor-scripts/x9fig8.out`). The value is correct and the next sentence names "the level shared by A, B, D and E". Suggested fix: "...the A, B, D and E level appears near $0.048$ (C's near $0.054$)."
- **A2. Version README wording** (`motor-gradient-optimization.9/README.md`, not in the PDF). "V9 corrects the v8 audit flags in v8, which revised v7, which added..." does not read cleanly. The state line still says "DRAFTED; fresh independent review and audit pending". Update it after this audit.
- **A3. Verifier Fig. 8 binding covers both repeats** (verify_exp2.py: `min(v['F_upper'] ... space == 'a')`). Fig. 8 plots repeat 1 only. The repeat-1 minimum (0.04752) and the all-repeat minimum (0.04752) round to the same 0.048, so nothing is hidden. Informational.
- **A4 (carried v8 N7). Host type**: c7a.4xlarge is recorded only in `refs/main-run-runbook.md`, not in the bundled session records. The author's reason for leaving this unchanged is accepted. Informational.
- **A5 (carried v8 N10). Unverified citations (4)**: more2009benchmarking, dolan2002benchmarking, kraft1988 and ragonneau2022 were checked at metadata level only, and no source is in `refs/`. The reworded §7.1 attribution ("data- and performance-profile practice for benchmarking optimization solvers") now matches the two cited works' known content (Moré-Wild data profiles; Dolan-Moré performance profiles). The author should confirm claim support off-disk. No citation lookups were made in this audit, and no email address was sent to any service.

## v8 advisory notes: disposition

| v8 note | v9 status | Evidence |
|---|---|---|
| N1 Discussion paired qualifier | Resolved | main.tex:790-794 "wherever both reach a common validated target from the same start"; verifier-bound |
| N2 abstract qualifiers | Resolved | main.tex:35 "3.6 under fixed unscaled current" (body 504-507); main.tex:42-44 "at the initial design on the second-coarsest mesh, at order 45 and full current" (body 624-633 at L1, the second of L0-L3; conclusion 833-834) |
| N3 2.45 display precision | Resolved | Abstract, intro and §7.2 use two decimals, matching Table 6 (2.45/2.75, 3.10/3.25, 3.14/7.20, 5.06/7.79) |
| N4 "tie" 0.99 | Resolved | "near tie ... D is 0.75% faster, within the timing noise between repeats"; raw 0.9925, repeat-timing median 2.13% |
| N5 held-out wording | Resolved | "remaining objective fraction at the held-out currents is 1.23-1.73 times that at the training currents"; raw 1.226-1.7275; B's best 0.8922 |
| N6 F vs F+δF | Resolved (see A1 for wording) | Fig. 8 level 0.0475 → "near 0.048" |
| N7 host type | Not changed (accepted) | A4 |
| N8 verifier hygiene | Resolved | docstring and PASS lines version-neutral; held-out min asserted `== 1.07` (raw 1.0713); no-op replace removed |
| N9 profile attribution | Resolved | optimization.tex:28-29 |
| N10 unverified citations | Not changed (accepted) | A5 |
| N11 "binary" link label | Resolved | main.tex:869-870 "the source of the final-refinement program"; href unchanged, returns 200 |

## Audience-fit notes

None. `anvil.lib.audience_check`: public_repo_url declared (`anvil_json`, `https://github.com/rjwalters/geode-fem`); 0 unlinked artifact paths, 0 governance vocabulary, 0 private locators. All 11 `\href` targets return HTTP 200.
