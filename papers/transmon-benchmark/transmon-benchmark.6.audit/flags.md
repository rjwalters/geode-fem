# Audit flags for transmon-benchmark.6

## Critical flags (block advancement to AUDITED)

**None.**

The single v5 critical flag (misquotation of `eriksson2025automated`,
§1) is **formally verified CLOSED** at the changed site: v6 removes the
quotation marks (taking the second of the two fixes the v5 audit
prescribed), the span no longer asserts verbatim source words anywhere
in the paper (all four remaining ``...'' spans are non-bibliographic),
and the paraphrase was re-verified first-hand this audit against the
live arXiv:2508.18027 abstract — every element is supported, and
"iterates" now carries the source's "iterative" that the v5 quotation
had dropped. See citation-audit.md for the element-by-element evidence.

Both v5 traceability notes are also resolved by v6 hunks and verified
against the committed artifacts (numerical-audit.md §1–2): the Table 3
off-target Peak RSS cells now carry an accurate provenance footnote
(the `[matched.off_target.*]` tables indeed record wall times only),
and §8.3 now states 136× on the artifact's own unrounded-median basis.

**Build: clean.** bibtex + 4 pdflatex passes, all exit 0, converged at
the pass-4 `.aux` fixpoint (pass 3 still emitted the rerun warning —
the fixed-2-post-bibtex heuristic would again not have sufficed), zero
`??`/`[??]` in the rendered 25-page PDF. The shipped `main.pdf` is
current (pdftotext stream byte-identical to the fresh compile) and the
shipped `main.bbl` is byte-identical to the fresh one. Compile ran in a
scratch copy to honor the do-not-modify-version-dir directive; see
compile-log.txt header.

**Citations: 57/57 resolve**, zero unused bib entries, key set
identical to v5 (refs.bib byte-identical).

**Numerical: zero inconsistencies.** All five v6 hunks verified
directly against the committed TOMLs; the v5 full sweep carries over
for the unchanged remainder (diff-verified).

## State machine consequence

v6 is READY (review 42/44, advance:true, zero review critical flags)
and this audit records **zero critical flags** — per the pub state
machine, the thread **transmon-benchmark is now AUDITED (terminal)**.
transmon-benchmark.6/ is the deliverable.

## Non-critical notes (do not block AUDITED)

- **Five `TODO(operator)` markers remain** (title wording,
  affiliations, cubecl f64 tracking URL, repo URL/DOI,
  acknowledgment) — hard **arXiv submission blockers** but
  operator-owned inputs, not artifact-quality defects. The paper must
  not be submitted until the operator supplies them.
- **Unverified citations (49)**: claim-support could not be verified
  because source PDFs are not in `<thread>/refs/`. The 8 keys verified
  live (7 at v5 + eriksson re-verified at v6) all support. Author
  should verify the remainder off-disk before submission.
- **Length vs target**: the compiled PDF is 25 pages single-column
  against the BRIEF's "8–12 pages two-column (or ~15 single-column)"
  target — over target. The reviewer scored length/economy within the
  rubric (42/44, advance:true); recorded here for the operator's
  venue decision, not blocking.
- **Uncitable architectural claim** (§1): "no derivative path / none
  exposes an adjoint of its solve" for HFSS/COMSOL remains uncited and
  (per the litsearch gap analysis) uncitable for closed-source
  products; framed as architecture with a "to our knowledge" hedge.
  Carried from v5; acceptable.
- **Figure mtime skew (informational, not stale)**: all six renders
  are byte-identical to v5, whose audit re-executed every script
  against the committed TOMLs and found the renders content-current;
  the TOMLs are unchanged since. A re-render at final packaging would
  clear the script-newer-than-render mtimes.
