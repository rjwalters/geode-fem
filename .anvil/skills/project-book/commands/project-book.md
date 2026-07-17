---
name: project-book
description: Assemble a multi-thread project into one compiled book — stage the latest version of every chapter thread into a consumer-owned master document, two-pass compile it, and report per-thread convergence state.
---

# `/anvil:project-book`

Utility skill. Stages the latest resolved version (via `resolve_latest`) of
every chapter thread into a consumer-owned master LaTeX document, two-pass
compiles it into one
`book.pdf`, and writes a per-thread build report. The **consumer owns the
master document** (`book.tex` + `preamble.tex`); this skill orchestrates
staging + compile + reporting only. **Build does not block on quality** —
EMPTY / below-READY threads warn but never stop the compile.

## Usage

```
/anvil:project-book <project-dir>
    [--dry-run]   # print the full per-thread plan; write nothing
```

`<project-dir>` is the project root (the directory carrying `BRIEF.md`). The
`build:` block in `BRIEF.md` controls `order`, `master_doc`, `chapters_dir`,
`chapter_filename`, and `out_pdf` (all optional; `master_doc` required for the
compile step). Zero-config falls back to the `documents:` order and framework
defaults (`book/chapters`, `chapter.tex`). See `SKILL.md` for the full config
surface and the marker-guard / placeholder contract.

## Procedure

### 1. Run the build

Load the skill lib (`anvil/skills/project-book/lib/`) and call the single
entry point:

```python
result = orchestrate.run(
    project_dir,
    dry_run=dry_run,   # False unless --dry-run
)
```

`result.threads` is the per-thread collection (resolved version, state, score,
audit state, recommended next command); `result.stage_result` and
`result.compile_result` carry the staging + compile outcomes;
`result.report` is the rendered `BOOK_REPORT.md` markdown (written to
`result.report_path` in apply mode).

### 2. Interpret the result

`result.success` is **structural-only** — placeholders, below-READY threads,
and below-threshold scores never make it false (build-does-not-block-on-
quality). Translate a False result into a **nonzero exit**:

- **Marker-guard refusal** (`result.stage_result.refused`): the chapters dir
  is non-empty and lacks the `.anvil-book-build` marker; nothing was deleted.
  Move/remove the directory or repoint `build.chapters_dir`.
- **xelatex absent** (`result.compile_result.xelatex_missing`): the chapters
  were staged for a manual compile; the report names the `XELATEX_REMEDIATION`
  install story. Install a TeX engine and re-run.
- **Compile gate failure** (`not result.compile_result.ok`): the master
  document did not compile clean; `result.compile_result.gate.reasons` and the
  report's "Build warnings" section carry the detail.
- **Config error** (`ValueError`): a `build.order` slug missing from
  `documents:`, or a chapters-dir / out-pdf / master-doc collision. Fix the
  `build:` block.

### 3. Print / hand off

Under `--dry-run`, print `result.report` to stdout — the full plan (resolved
versions, staged filenames, per-thread state) with nothing written. Otherwise
the deliverables are the staged `chapters/` tree, the compiled `out_pdf`, and
`BOOK_REPORT.md` at the project root. Surface any `result.gitignore_note`.

## Determinism + idempotence

The chapters dir is a marker-guarded blow-away rebuild: re-running produces the
same layout, and a thread removed from `order` between runs leaves no stale
chapter file. `--dry-run` is byte-identically side-effect-free.

## Out of scope

Multi-format output, consumer template management, watch mode, and partial
builds are out of scope — see `SKILL.md` "Out of scope".
