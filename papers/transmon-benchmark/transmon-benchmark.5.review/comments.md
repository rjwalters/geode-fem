# Comments — transmon-benchmark.5

Line/section-keyed feedback, grouped by severity, followed by procedural
notes. Line numbers reference `transmon-benchmark.5/main.tex`.

---

## blocker

(none)

## major

- **[§1 L113–115] Inexact quotation of `eriksson2025automated` presented as
  verbatim.** The body reads: QDesignOptimizer "iterates ``time-consuming
  electromagnetic simulations'' of HFSS-class solvers". The cited paper's
  abstract (arXiv:2508.18027, verified against the live arXiv record this
  pass) reads "time-consuming **iterative** electromagnetic simulations
  requiring manual intervention" — the quoted span drops a word from
  inside the quotation. The claim itself is accurately attributed and the
  litsearch's harder caution IS honored (the non-differentiability claim is
  attributed to solver architecture, not to their words), but a
  one-word-off quotation of the closest prior work — whose authors are
  this paper's outreach audience — is exactly what a careful reader
  cross-checks. **Fix**: either quote verbatim ("time-consuming iterative
  electromagnetic simulations") or drop the quotation marks and paraphrase
  (the abstract already does the latter correctly at L70–72). The
  changelog's own description of this edit says "phrasing paraphrases
  their abstract" — make the body match that intent. Scored under D8.

## minor

- **[TODO(operator) markers ×5]** Title final wording (L56–57),
  affiliations (L60–61), burn/cubecl f64 tracking-issue URL (§3 footnote,
  L370–373), repository URL + archival DOI (§12 footnote, L1184–1185),
  acknowledgment wording + whiteroom cite-vs-acknowledge decision
  (L1377–1379). Operator-gated per thread convention (not scored down; not
  placeholder-class criticals) but **every one is a hard arXiv-submission
  blocker** — the paper cannot be submitted until all five are resolved.
- **[length, whole document] 25 compiled single-column pages vs the
  BRIEF's 8–12-page two-column (~15 single-column) target.** The
  compression at v5 was real (~35% on the old benchmark material, with
  ~8 pp of new contribution content added), and most sections are now
  load-bearing for either the contribution or the credential. Remaining
  levers, in order of yield: (1) the venue two-column layout itself;
  (2) the Conclusion (L1338–1370) — a single ~33-line paragraph that
  restates the abstract, the contribution list, and §11's readings nearly
  item for item; a 3–4-sentence close would do; (3) the Discussion's
  credential paragraph (L1269–1280) partially re-walks §5's V&V framing;
  (4) §6.2's gauge/projection arc (L603–638) is the longest single
  paragraph in the paper — the three-step arc could carry its numbers in a
  compact enumerated list, or the step-level detail could move to an
  appendix with the artifact pointer.
- **[§9.2 L902 / §9.1 L827] Anchor E_C stated as 0.2155 GHz (pad) and
  0.2156 GHz (plate/anchor).** Both are artifact-faithful
  (`pad_results.toml` `e_c_target_ghz = 0.215464` from the 89.9 fF target;
  `results.toml` `e_c_target_ghz = 0.215600` from the 89.843 fF back-solve)
  but the unexplained 0.2155-vs-0.2156 proximity can read as a typo. A
  half-sentence noting the two anchors differ because the pad run targets
  the rounded 89.9 fF would immunize it.

## nit

- **[figures/ naming]** `fig4-cpu-wallclock.pdf` renders as Figure 6 and
  `fig6-participation.pdf` renders as Figure 4 (the participation figure
  moved forward with the reframe). Cosmetic — filenames and scripts only;
  no reader-visible defect. Rename at a convenient pass to keep the
  figure-source map obvious.
- **[Fig. 6 inset]** The per-core inset bar labels round to integers (29,
  232, 131, 356) while the caption quotes 28.7 core-s; consistent with the
  stated displayed-value convention, but worth a glance at final polish.

---

## Procedural notes

- **web-search** (`web_search: true`): the sandbox provides no
  search-engine tool; per the knob's D4 contract the reviewer substituted
  one read-only arXiv-API verification (the abstract of 2508.18027, to
  adjudicate the §1 quotation) and otherwise relied on the
  `.4.litsearch` substrate (8 resolver-verified candidates, all
  dispositioned) + domain knowledge. No new close-prior-work leads
  surfaced for the reframed claim; no citations or `.bib` entries were
  written.
- **numeric-consistency**: automated detector ran
  (`anvil.lib.numeric_consistency`, sidecar
  `transmon-benchmark.5.numeric/_review.json`): 573 numbers extracted,
  0 claim findings, pass. Manual claim-vs-artifact spot-checks of every
  new §7–§10 number and the corrected scale record are tabulated in
  findings.md — all pass.
- **evidence-check**: automated verifier ran against the resolved body;
  9 dimensions checked, zero `fabricated_evidence`/`missing_evidence`
  findings.
- **render-gate**: skipped fail-open per step 4b — no
  `transmon-benchmark.5.audit/compile-log.txt` exists (pub-audit has not
  run at N=5). The reviewer compiled the source independently to a clean
  fixpoint: 25 pp, zero undefined citations/references, single 1.13 pt
  overfull hbox (below the 5 pt gate; the two v4 gate-exceeding boxes are
  resolved), no placeholder markers beyond the five conventioned
  TODO(operator) items. The shipped `main.pdf` (807,881 bytes) matches
  the fresh fixpoint compile byte-for-byte in size — the v4 stale-PDF
  defect is fixed.
- **score_history**: per the task constraint this reviewer did not modify
  `transmon-benchmark.5/`; the orchestrator should append
  `{iteration: 5, total: 41, threshold: 35, rubric_id: "anvil-pub-v2"}`
  to `transmon-benchmark.5/_progress.json.metadata.score_history`.
