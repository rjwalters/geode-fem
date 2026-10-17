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
  10, Sonnet 6, counted once per commit per family). These are now recomputed
  by the committed `count_provenance.py` (read-only, offline, pinned base
  `3fe5f706`), which `verify_evidence.py` calls and pins. It also gives the
  3 trailer-less (Codex) commits `ec9e0539`, `2efa0057`, `4c6faaaf` with
  15,761 of 144,615 added text lines (10.9 %), and Crutcher Dunnavant's 36
  non-merge commits on main (`--author=crutcher@gmail.com`, from
  `git shortlog -sne`; 2026-06-24 to 06-29).
- **Review coverage** (read-only `gh pr view --json comments,reviews` over
  the 48 PRs): 46 have a review or comment; #539 and #592 have none. The v7
  reviews (#1040) already mention mutation testing, and v7 is the first
  version with `verify_evidence.py`. Transcribed, not machine-checked (needs
  the forge).
  Scope (re-review of 2aea0d1b): 7 of the 22 reviews from #1040 on (#1047,
  #1055, #1057, #1064, #1068, #1071, #1094) did not mutation-test, so §9
  scopes the claim to the reviews of each paper revision (#1040, #1060,
  #1082, #1119, #1121).
- **Judge-review fixes (PR #1121, 18fdabed), mutation check:** 16 single
  edits, each restored, all caught: Sonnet 6→5, Crutcher 36→35, "GPT Sol and
  GPT Astra"→"GPT Astra", 11 %→12 %, 46→48 PRs, "not counted" reinserted
  (twice, in the counts sentence and as a new sentence), "for his
  contributions to"→"for his work on", the NOTE back to "offered", the
  pinned `commits` 85→84 in `PROVENANCE`, and in §6 "gives the same shift",
  "same shift", "confirms", "confirm". Two legitimate sentences, "confirmed
  with the same shift-invert target" and "confirmation used the same
  shift-and-invert spectral transform", now pass. Rebuild: 24 pages, 0
  undefined references, 0 overfull boxes, 0 bibtex warnings; §9 (page 15)
  inspected at 80 dpi.
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

Not checked: the §9 review coverage (46 of 48) is transcribed from the
forge; `count_provenance.py` needs a full clone containing `3fe5f706`;
everything v10 listed as not
checked; no Anvil critic was run.
