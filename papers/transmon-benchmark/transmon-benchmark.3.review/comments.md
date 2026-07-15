# Line-level comments — transmon-benchmark.3

Grouped by severity. Section anchors reference `main.tex` headings.

## blocker

_None._

## major

_None._ (No dimension-blocking or critical defect found in the review scope.
Fact/claim-support verification and the authoritative compile-cycle render-gate
are deferred to the concurrent `pub-audit` pass.)

## minor

- **§9 Discussion + Abstract — arc restated three times.** The three-step
  gauge/projection arc is stated in full in the abstract ("tree--cotree
  elimination (spectrum-shifting, $1.64\%$)... a port-aware rank-1 re-admission"),
  re-derived at length across §9's four paragraphs, and re-summarized in the
  Conclusion. The §9 derivation is load-bearing and should stay; the abstract
  recap can drop to one clause and the Conclusion recap can shorten. (dim 9)

- **§12 Discussion — two paragraphs re-state earlier conclusions.** "What the
  cross-validation does and does not establish." and "What the performance
  evidence supports." largely restate the §7 agreement reading and §10's own
  honest-read. Consider merging into a single tighter "What this establishes"
  paragraph. (dim 9)

- **Evidence breadth (dim 2, −1).** The general thesis ("a general-purpose ML
  tensor stack can carry production-accuracy computational electromagnetics")
  rests on a single geometry. The BRIEF's Branch-B path contemplated a
  one-page "validation portfolio" table (Mie sphere, spiral inductor, waveguide
  modes, motor torque, SMF-28 fiber) drawn from committed TOMLs. Branch A was
  correctly selected, but a compact breadth row or two — committed artifacts
  only, no new measurement — would lift dim 2 to ceiling and directly answer the
  "one geometry" limitation the paper itself flags. Optional, not required.

## nit

- **§3 footnote — f64 tracking-issue URL is a TODO.** "tracking-issue URL to be
  added at revision. TODO(operator)." Operator-gated; flagged for submission
  completeness only, not scored.

- **§11 / Acknowledgments — archival DOI + whiteroom cite decision TODOs.**
  "Repository URL and archival DOI for the geode-fem source at commit
  \texttt{3174015} to be added at submission." and the whiteroom L1-L4
  cite-vs-acknowledge decision are operator-gated submission blockers, not review
  defects.

- **Author block — affiliations TODO.** `\author{Robb Walters \and Crutcher
  Dunnavant}` with equal-contribution footnotes resolved; affiliations remain
  `TODO(operator)` per the BRIEF. Correctly NOT penalized.

## related-work

- **`related-work`: `web_search: true` is set but no live searches were run this
  pass.** The reviewer grounded the dim 4 close-prior-work judgment in the
  `transmon-benchmark.2.litsearch` substrate plus domain knowledge (the substrate
  already surfaced SQDMetal / `sommers2025open`, the Palace-workflow /
  `ye2025electromagnetic`, and the concurrent TensorGalerkin / `wen2026learning`,
  all cited and distinguished head-on). No live search was executed in this
  environment. **Recommendation:** if a fresh close-prior-work sweep is desired
  for the physics.comp-ph submission, re-run `pub-litsearch` with web access —
  the resolver-verified candidate/lead decision is that command's job, not the
  reviewer's. No missing close prior work is asserted here; §2 is judged complete
  on the available substrate.

## procedural notes

- `numeric-consistency`: automated detector ran (uv on PATH); 400 numbers
  extracted, 0 claim-vs-claim inconsistencies (`pass: true`). The 51.2 s → ~21 s
  (PR #510) CPU number is honestly footnoted, not stale.
- `evidence-check`: automated verifier ran (uv on PATH); all 9 dimension
  quoted-spans validated verbatim against `main.tex` (`pass: true`) after two
  self-check correction passes (initial D8 quotes drawn from `refs.bib` rather
  than the body were re-derived from body prose).
- `render-gate`: skipped — v3 `main.pdf` present but `compile-log.txt` is produced
  by the concurrent `pub-audit` pass (fail-open per step 4b). The reviewer's own
  pdftotext scan of `main.pdf` found no unresolved `[??]` / `Section ??` tokens.
