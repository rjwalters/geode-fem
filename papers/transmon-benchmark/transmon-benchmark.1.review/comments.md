# Comments — transmon-benchmark.1

Line-level feedback keyed to `main.tex` sections. Grouped by severity. All line numbers refer to `transmon-benchmark.1/main.tex` (single-file thread; no `\input` children).

## blocker

All blockers below are instances of the one critical flag (`numerical_inconsistency`): the draft carries the brief's superseded agreement numbers; the committed source of truth is `benchmarks/transmon_eigen/results.toml` (worst 0.032%; junction 0.001%; geode values at 3 decimals: 5.153 / 15.465 / 17.490 / 18.693 / 20.703 / 26.088).

1. **Abstract, L52–53** — "to within $0.03\%$ across all physical modes, with the junction LC mode agreeing to $0.000\%$". Committed artifact: worst 0.032% (so ≤0.03% is false as written) and junction rel_err_pct = 0.001. Fix: "to within $0.033\%$ (worst $0.032\%$) … junction LC mode agreeing to $0.001\%$" (or equivalent phrasing sourced from results.toml).
2. **§1 contribution bullet, L101–102** — "reproduces Palace's physical eigenmodes to $\leq 0.03\%$, with the junction LC mode agreeing to $0.000\%$". Same fix as (1).
3. **§6 opening, L357–359** — "every physical mode agrees to $\leq 0.03\%$, and the junction LC mode … agrees to $0.000\%$". Same fix.
4. **Table 2 resonator row, L374** — "readout resonator & 5.1528 & 5.1513 & 0.029\%". Committed: geode 5.153, Palace 5.151335830348, rel_err_pct 0.032. The 4-decimal geode value 5.1528 appears in no committed artifact.
5. **Table 2 box/cavity row, L375–376** — "box/cavity modes & 15.4650 / 18.6927 / 20.6976 / 26.0809 & same to $\leq 0.03\%$". Committed geode values: 15.465 / 18.693 / 20.703 / 26.088. Note 20.6976 and 26.0809 are the *rounded Palace* frequencies (20.69755679425, 26.08089940472) presented in the geode-fem column — a column/value transposition inherited from the stale brief. Replace with six per-mode rows (see major #1).
6. **Table 2 junction row, L377** — "junction LC & 17.4901 & 17.4901 & 0.000\%". Committed: geode 17.490, Palace 17.49010903536, rel_err_pct 0.001.
7. **§6 discussion prose, L404** — "the junction mode sits at $17.4901$\,GHz in both solvers". The committed geode value is 17.490 (3 decimals); 17.4901 is Palace's rounded value. Restate consistently with the artifact (e.g., "at 17.490 GHz (geode-fem) and 17.4901 GHz (Palace)" or "at 17.49 GHz in both solvers").
8. **§6 closing prose, L414** — "the residual $\leq 0.03\%$" → ≤0.033% (worst 0.032%).
9. **§10 Discussion, L648** — "agreeing to $\leq 0.03\%$ on the identical discrete problem" → same fix.
10. **§11 Conclusion, L687–688** — "to $\leq 0.03\%$ across all physical modes ($0.000\%$ on the junction LC mode)" → same fix as (1).

## major

1. **Table 2 structure (§6, L361–380)** — The headline table collapses four modes into one cell and drops per-mode Palace values and per-mode Δ, which makes the caption's committed-artifact provenance claim unverifiable from the table itself. Rebuild as six rows (resonator / mode_2 / junction LC / mode_4 / mode_5 / mode_6) with columns mode | geode-fem (GHz) | Palace (GHz) | rel_err (%), mirroring results.toml.
2. **§9 Reproducibility — missing precision disclosure (BRIEF mandate)** — The brief requires the reproducibility section to state: the toml stores geode frequencies at 3 decimals while Palace carries full precision; rel_err_pct is therefore computed against rounded geode values; full-precision agreement is slightly better (~0.029% worst). Absent from §9 (L603–639). Add one short paragraph or bullet.

## minor

1. `related-work` **§8, L473–474** — "a divergence-free or tree--cotree gauge projection on geode-fem's eigenpath" is uncited. The litsearch sibling's "Identified gaps" names this exact hole (canonical tree–cotree gauging citation, Albanese–Rubinacci-era, IEEE Trans. Magn. ~1988–1998; not hunted in run 0). Recommend a targeted `pub-litsearch` re-run to resolve and promote a citable anchor; do not hand-add an unverified entry.
2. **refs.bib `logg2010dolfin`** — title field is the truncated "DOLFIN" (full title: "DOLFIN: Automated finite element computing"). Complete the title.
3. **refs.bib `oberkampf2010verification`** — typed `@article` with no `journal` field; it is a Cambridge University Press monograph. Retype as `@book` with `publisher = {Cambridge University Press}` (DOI unchanged).
4. **§7.1 footnote L578 and §9 footnote L607–609** — the literal strings "TODO(operator)." render in the compiled PDF footnotes. These are BRIEF-mandated pending items (tracking-issue URL; repo URL + archival DOI) and are correctly impossible to mistake for results, but confirm at revision/finalize that both are resolved or restyled before any submission artifact is produced.

## nit

1. Table 2 row label "readout resonator" vs. results.toml key `resonator` and Table 2's "box/cavity modes" grouping — when rebuilding per-mode rows, consider carrying the toml mode keys (or a stated mapping) so the audit's numbers-vs-artifact pass is mechanical.

## Procedural notes (this review pass)

- **Render gate skipped (audit-first fail-open)**: `transmon-benchmark.1.audit/compile-log.txt` not present (audit critic dispatched in parallel); `main.pdf` exists but the gate contract requires the audit-captured log, so no `_gate.json` was emitted and no gate flags were wired. A manual placeholder scan over the resolved source found only the BRIEF-mandated `\TBDGPU` / `TODO(operator)` markers — no un-mandated placeholders, no fabricated GPU numbers.
- **numeric-consistency**: automated detector ran (`python -m anvil.lib.numeric_consistency … --write-review`): pass, 189 numbers extracted, 0 arithmetic claims flagged; sidecar written to `transmon-benchmark.1.numeric/_review.json`. A manual claim-vs-claim cross-check of the ratio claims confirms internal consistency: 51.2 s ≈ 4 × ranks at 50.8 s (~4× per-core), 51.2/30.6 = 1.67 ≈ 1.7×, 12.37/17.49 = 0.7073 ≈ 0.7071 = 1/√2, and 17.49 vs 17.60 GHz = 0.625% ≈ "0.6% below". The critical flag above is a text-vs-committed-artifact mismatch, which the paragraph-local detector cannot see.
- **evidence-check**: automated verifier ran against the staged `scoring.md`: pass, 9 dimensions, 0 findings.
- **web_search knob**: the thread BRIEF sets `web_search: true`, but this review session has no web-search tool available; D4 was grounded in the litsearch sibling's same-day (2026-07-14) resolver-verified candidates and documented absence-of-artifact search trail instead of live searches. The one `related-work` lead above (tree–cotree) routes through the recommended `pub-litsearch` re-run per the contract (the reviewer writes no citations).
