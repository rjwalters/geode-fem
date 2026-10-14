# v11 author-side checks — 2026-10-13 (tracker #1063)

Author-side checks of transcription and packaging. Not an independent review,
audit or numeric critic; no score is claimed. No solver, generator or cloud
resource was used. v10's checks (`transmon-benchmark.10/validation.md`) cover
everything v11 did not change; v11 changes only the text listed in
`changelog.md` and adds two bibliography entries. Figures are byte-identical
to v10's.

Environment: Python 3.14; Poppler (`pdftotext`, `pdfinfo`, `pdftoppm`);
TeX Live / pdfTeX.

- `papers/build.sh papers/transmon-benchmark/transmon-benchmark.11`: PASS,
  three pdflatex passes, **24 pages** (v10: 23). `main.log` has no undefined
  reference or citation and no overfull box; `main.blg` reports 0 warnings.
- `python3.14 verify_evidence.py`: PASS (all v10 checks, with the operator
  checks rewritten against v10 and the static-route wording pinned in §1 and
  the conclusion; see `changelog.md`).
- `python3.14 verify_figures.py`: PASS, now also asserting that every file
  under `figures/` is byte-identical to v10's.
- **Mutation check:** 42 single edits to `main.tex`, `refs.bib` and v10's
  `main.tex`, each restored, all caught by `verify_evidence.py`: the title
  (two edits); the author block (equal-contribution footnote, a second
  author, the coauthorship note, the affiliation); the validation claim
  ("first independent" back in §1, the v10 bullet restored, the SQDMetal
  citation dropped, "third-party" inserted, "which has no publication of its
  own" restored); the static route in §1 and the conclusion ("gives the same
  shift", "confirms that shift" in each, "by a static route", a "confirms"
  sentence added to the conclusion) and the abstract ("confirms this"); §9
  (the whiteroom citation, the model list, Codex, the Loom citation, the
  review description, each of the three examples, both counts, the Codex
  caveat, the responsibility sentence, the section title); the
  acknowledgments (the thanks dropped, the AI disclosure repeated); the two
  remaining `TODO(operator)` comments, each removed, and a rendered one
  added; the repository URL; "deposit" → "submission"; the cubecl URL promise
  restored; the whiteroom and Loom URLs and an access date in `refs.bib`; a
  change to v10's `main.tex`. Two further edits to `figures/` (a line added to
  `figures/src/README.md`, an extra data file) are caught by
  `verify_figures.py`. The harness is scratch and not committed.
- **§9 counts** (local git, read-only; no GitHub write):
  `git log --first-parent origin/main --format=%s -- papers/transmon-benchmark 'benchmarks/transmon*'`,
  PR numbers from "Merge pull request #N" and trailing "(#N)", deduplicated
  → 48; `git log origin/main --no-merges --format='@@%h%n%(trailers:key=Co-Authored-By,valueonly)'`
  over the same paths → 85 commits, 82 with a Claude trailer (Opus 66, Fable
  10, Sonnet 6, counted once per commit per family).
- **cubecl tracking issue:** `gh search issues` on tracel-ai/cubecl and
  tracel-ai/burn ("f64", "f64 cuda", "double precision", "float64",
  "f64 support") and a web search found no issue that tracks the CUDA
  runtime's missing f64; the footnote no longer promises a URL.
- **Visual inspection** at 80 dpi of page 1 (title, solo author block),
  page 15 (§9, §10 and the repository footnote) and page 17 (conclusion and
  acknowledgments).
- **Anvil pre-flight** (from the main checkout's `.anvil`):
  `python3.14 -m anvil.lib.pending_marker <v11> --write-review` exit 0, no
  marker; `.11.pending/_review.json` written by the tool, same shape as v10's.
- **Immutability:** `git status` touches only `transmon-benchmark.11/`,
  `transmon-benchmark.11.pending/`, `BRIEF.md` and `papers/README.md`.

Not checked: the §9 counts are transcribed, not recomputed by the verifier
(a shallow clone would not reproduce them); everything v10 listed as not
checked; no Anvil critic was run.
