---
name: project-book
description: Assemble a multi-thread project into one compiled book — stage the latest version of every chapter thread into a consumer-owned master LaTeX document, compile it (two-pass XeLaTeX), and report per-thread convergence state.
domain: anvil
type: skill
user-invocable: true
---

# anvil:project-book — Assemble a multi-thread project into one book

The `project-book` skill is a recurring **build** tool: given a project root
(the directory carrying `BRIEF.md`), it stages the `.latest`-resolved version
of every chapter thread into a consumer-owned master document and compiles one
deliverable — `book.pdf`:

```
<project>/
  BRIEF.md                 # documents: list + build: block
  00-introduction/         # chapter thread (any artifact skill)
    00-introduction.5/
      chapter.tex          # per-thread chapter source
  01-childhood/ ...
  book/
    book.tex               # consumer-owned master document (memoir/…)
    preamble.tex           # consumer-owned shared preamble
    chapters/              # ← staged here (gitignored build dir)
      .anvil-book-build    #   marker authorizing the blow-away rebuild
      00-introduction.tex  #   copied from the latest version dir
      01-childhood.tex
      appendix.tex         #   placeholder (thread not started)
    book.pdf               # ← compiled output
  BOOK_REPORT.md           # ← per-thread build report (gitignored)
```

The **consumer owns the master document** (`book.tex` + `preamble.tex`,
typically the LaTeX `memoir` class). This skill never owns the template — it
orchestrates *staging + compile + reporting* around the consumer's document.
Like `project-share` and `project-scout`, it is project-scoped and
artifact-agnostic: any thread from any artifact skill (memo, essay, report, …)
can be a chapter, so it lives as a standalone skill rather than on any single
artifact skill's command surface. Its output is a genuine deliverable
(`book.pdf`) — "skill identity = artifact identity" (CLAUDE.md).

## Build does not block on quality

Quality gates live at per-thread convergence (each thread's own `review` +
`audit`). The assembly step must **always** be able to produce a preview PDF:

- **EMPTY threads** (no version dir) and threads whose resolved version dir
  lacks the chapter file get a generated **placeholder chapter** — the master
  document always compiles, even during early project phases.
- Threads **not at READY/AUDITED**, or whose review score is **below the
  advance threshold**, generate a **warning** in `BOOK_REPORT.md` — never a
  compile block.

The only hard failures are structural: a marker-guard refusal, a path
collision, an `order` slug missing from `documents:`, or xelatex absent.

## What this skill does

- **Resolves** each thread's current version via the canonical resolver
  (`anvil/lib/latest_resolution.py::resolve_latest`): pinned `.latest` symlink
  > real `.latest` dir > walk-to-highest.
- **Derives** each thread's lifecycle state from its version-dir
  `_progress.json` (and a clean `.audit` sibling promotes it to AUDITED),
  reads the review score from the highest-N `.review` sibling (numerator via
  `anvil/lib/critics.py`, denominator + threshold from the sibling's
  `_meta.json` version stamp), and notes the audit state.
- **Stages** one `<slug>.tex` per chapter into `chapters_dir` (in declared
  `order`), copying the resolved `chapter_filename` or generating a
  placeholder. The chapters dir is a **marker-guarded blow-away rebuild** —
  stale chapters from threads removed from `order` disappear by construction.
- **Compiles** the consumer `master_doc` with **two-pass XeLaTeX** via
  `anvil/lib/render_gate.py::compile_and_gate` (the skill does not roll its
  own LaTeX invocation) and writes `out_pdf`.
- **Reports** `BOOK_REPORT.md`: a per-thread table (slug, resolved version,
  state, score/44, audit state, recommended next command) plus a "Build
  warnings" section.

## Configuration (BRIEF.md `build:` block — all optional)

```yaml
build:
  order:                        # authoritative include-list AND ordering
    - 00-introduction
    - 01-childhood
    - appendix
  master_doc: book/book.tex     # consumer-owned master document (required for compile)
  chapters_dir: book/chapters   # where to stage per-thread chapter files
  chapter_filename: chapter.tex # per-thread filename to stage
  out_pdf: book/book.pdf        # output PDF path (default: <chapters_dir>/../book.pdf)
```

Zero-config works: with no `build:` block, every `documents:` entry is a
chapter in BRIEF order with the framework defaults (`book/chapters`,
`chapter.tex`). The parser is skill-local (`lib/config.py::BookConfig`) — the
shared `ProjectBrief` model is not extended; the BRIEF parser already ignores
unknown top-level frontmatter keys, so the `build:` block is safe in any BRIEF
today (the same precedent `project-share`'s `export:` block relies on).

`order` semantics: when present it is the authoritative include-list and
ordering — slugs omitted from `order` are excluded (noted in the report);
slugs in `order` that don't appear in `documents:` are a hard error naming the
slug. Absent `master_doc` → **staging-only mode** (chapters staged, no
compile, report still produced).

## Commands

| Command                                        | What it does                                                                            |
|------------------------------------------------|------------------------------------------------------------------------------------------|
| `/anvil:project-book <project-dir>`            | Stage chapters + two-pass compile `master_doc` → `out_pdf` + write `BOOK_REPORT.md`.    |
| `/anvil:project-book <project-dir> --dry-run`  | Print the full per-thread plan (resolved versions, staged filenames, state). No writes. |

See `commands/project-book.md` for the operator-facing contract.

## Safety: the marker guard

The chapters dir is deleted and rebuilt on each run — but **only** when it
doesn't exist, is empty, or contains the `.anvil-book-build` marker from a
previous run. A non-empty chapters dir without the marker is a **hard
refusal** with no deletion. Defense-in-depth: at plan time, a `chapters_dir`
that contains a thread directory, or an `out_pdf` / `master_doc` nested inside
the chapters dir, is rejected before the rebuild ever runs.

## XeLaTeX preflight

`check_xelatex_available()` runs before the compile. When xelatex is absent
the command **stages the chapters** (so the consumer can compile manually) and
emits a hard error with the `XELATEX_REMEDIATION` install story — the staging
dir is preserved; it does not silently skip.

## Gitignore

`chapters_dir` and `out_pdf` and `BOOK_REPORT.md` are build artifacts. When
the chapters dir is not covered by the consumer's `.gitignore`, the run prints
a one-line suggestion. It does **not** auto-edit the consumer's files.

## State machine

The skill does not produce a versioned artifact. It runs to completion as a
single invocation; the on-disk evidence is the staged `chapters/` tree, the
compiled `book.pdf`, and the `BOOK_REPORT.md` provenance report.

## Lib primitives composed

- `anvil/lib/project_brief.py` — `load_project_brief_strict` for the
  `documents:` list and default ordering.
- `anvil/lib/latest_resolution.py` — `resolve_latest(thread_dir, slug)` per
  thread.
- `anvil/lib/render.py` — `check_xelatex_available()` + `XELATEX_REMEDIATION`.
- `anvil/lib/render_gate.py` — `compile_and_gate` (two-pass LaTeX + gate).
- `anvil/lib/critics.py` — `load_review` + `aggregate` for review scores.
- Skill-local `lib/`: `config.py` (BookConfig + `build:` parser), `collect.py`
  (per-thread resolution + state + score + audit), `stage.py` (marker-guarded
  rebuild + placeholder generation), `compile.py` (xelatex preflight + two-pass
  `compile_and_gate` wrapper), `report.py` (`BOOK_REPORT.md`), `orchestrate.py`
  (single `run()` entry).

## Out of scope (explicit follow-ons)

- Multi-format output (only LaTeX/PDF; Pandoc/Markdown assembly is future).
- Consumer-owned template management within the skill (template lives in the
  consumer project).
- Continuous watch mode; partial builds (always builds all threads in `order`).
- Per-chapter critic integration (per-thread critics remain per-skill).
- `build:` block absorption into the shared `ProjectBrief` model (lib promotion
  trigger: a second consumer).

## Tests

Fixtures are programmatic builders in `tests/_book_fixtures.py`. Test files
(distinct filenames per the #58 packaging convention; lib loaded under the
unique package name `project_book_lib` via `tests/_project_book_skill_lib.py`
per the #367 / #372 precedent):

- `test_project_book_config.py` — `build:` parsing + defaults + malformed shapes.
- `test_project_book_collect.py` — `.latest` precedence, state derivation,
  score/audit reading, EMPTY detection.
- `test_project_book_stage.py` — chapter copy, placeholder generation, marker
  guard, gitignore suggestion.
- `test_project_book_compile.py` — xelatex-missing hard error, two-pass compile,
  PDF relocation, gate failure passthrough.
- `test_project_book_report.py` — table rows, warnings section, excluded slugs,
  report written at project root.
- `test_project_book_dry_run.py` — SHA-256 snapshot: dry-run leaves the tree
  byte-identical.
- `test_project_book_idempotent.py` — re-run same layout; stale chapter removal;
  build-does-not-block; end-to-end smoke compile.
- `test_project_book_guard.py` — foreign-dir refusal, collision rejection,
  xelatex-missing staging preservation.
