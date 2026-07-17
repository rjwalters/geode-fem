# anvil:datasheet

Customer-facing IC / component datasheets — the spec-bearing document a customer designs against. Produced via the canonical anvil lifecycle with a **mandatory audit pass** (`draft → review + audit → revise → … → READY → AUDITED → figures`), tuned for the way a datasheet actually fails: numbers that read fine in isolation but contradict the design source, pins assigned twice, bus fields that cannot represent their claimed range, silent spec changes, pre-silicon values presented as final, and sibling SKUs whose shared-die specs drift apart. All six failure modes were hit hand-authoring two real preliminary datasheets at the studio canary (issue #418); this skill encodes the cleanup.

## Quick orientation

| File | What it is |
|---|---|
| `SKILL.md` | Frontmatter + artifact contract + state machine (incl. `REVIEWED+AUDITED`) + the six canary failure modes. Read this first. |
| `rubric.md` | 9-dimension /44 scorecard (`anvil-datasheet-v1`). **≥39 advances** (customer-facing tier). Five critical-flag conditions (four audit-owned). |
| `commands/datasheet.md` | Portfolio orchestrator. Run from a project root to see per-thread/per-SKU state. |
| `commands/datasheet-draft.md` | Drafter. Brief + `refs/` spec bundle → `datasheet.tex` (XeLaTeX), emitting pin-map + bus-width integrity markers. |
| `commands/datasheet-review.md` | Reviewer. Deterministic pre-flight (render gate + pin-map + bus-width) then scores the 9 dims → `.review/` sibling. |
| `commands/datasheet-audit.md` | Auditor (REQUIRED by default). Spec source-of-truth cross-check (`VERIFIED`/`UNVERIFIED`/`CONTRADICTED`/`NOT-IN-REFS`) + mechanical checks + revision-history READY-gate + shared-die SKU coherence → `.audit/` sibling. |
| `commands/datasheet-revise.md` | Reviser. Aggregates `.review/` + `.audit/` → next version + `changelog.md`, bumping rev + revision-history row when specs changed. |
| `commands/datasheet-figures.md` | Figurer. Renders deterministic TikZ/data figures; stub-by-default for author artwork (package drawings, characterization plots). |
| `templates/anvil-datasheet.cls` | LaTeX class (XeLaTeX): navy `#1F4E7A` accent, part-vendor title block, consistent rev/footer, `featurecolumns` two-column first page, `\est{}`/`\simval{}`/`\meas{}` provenance macros, `\preliminarynotice`. |
| `templates/datasheet.tex.j2` | Section skeleton (Key Features \| Applications → Ordering → Specs → Performance → Pinout → Application → Package → Revision History → Legal) with the integrity markers pre-wired. |
| `templates/BRIEF.md.example` | Reference brief shape (frontmatter + prose). |
| `lib/pinmap_check.py` | Mechanical pin-map integrity checker (`% anvil-pinmap-begin/end` markers; every pin assigned exactly once). |
| `lib/buswidth_check.py` | Mechanical bus-width sanity checker (`% anvil-bus:` markers; `2^W` must cover the claimed set). |
| `examples/ax101-family/` | Vendored worked example: the dual-SKU **AX101** edge-AI family (object-detection `ax101-objdet` thread realized in-tree, `ax101-ocr` sibling declared) with a real stamped `.review/` + `.audit/` critic sidecar. |
| `examples/expected-thread.1/README.md` | Structural-properties reference for a drafted thread (NOT a golden file). |
| `tests/` | Structural skeleton test + checker unit tests + template/class compile smoke test (skips without `xelatex`) + the example-BRIEF strict-parse guard. |

## Reference skills

- **`anvil:proposal`** — the **structural + audit-by-default** reference: LaTeX/XeLaTeX skill with both `.review/` and `.audit/` REQUIRED to leave `DRAFTED` (`REVIEWED+AUDITED`), refs back-check with the four-valued verdict schedule, staged-sidecar atomic critic writes, render-gate pre-flight in the reviewer.
- **`anvil:report`** — the customer-facing-stakes reference (audit by default; the ≥39 tier).
- **`anvil:memo`** — the lifecycle / rubric-format reference.

## What is new in this skill

1. **Spec source-of-truth cross-check** — `refs/` holds the *spec bundle* (model/quant/RTL exports, foundry quotes, package drawings); the audit resolves every numeric claim against it. The spec bundle **outranks the brief** for numbers — the inverse of proposal's brief-is-the-contract rule, because a datasheet's numbers ARE the design's numbers.
2. **Mechanical integrity checkers** — `lib/pinmap_check.py` + `lib/buswidth_check.py`, driven by machine-readable marker comments the drafter is required to emit. Run in both review (pre-flight, `_gate.json`) and audit (findings). Violations are critical flag 2.
3. **Revision-history READY-gate** — spec-bearing changes vs the prior version without a rev bump + history row are critical flag 3; the audit diffs `N-1` vs `N`.
4. **Measured-vs-projected provenance** — `\est{}`/`\simval{}`/`\meas{}` macros + the `status` knob; bare pre-silicon values presented as final are critical flag 4.
5. **Shared-die SKU coherence** — the audit reads sibling SKU threads' latest sheets in the same project and compares the shared-die spec blocks; divergence is critical flag 5.

## Canonical worked instance

The grounding example is the **AX101** edge-AI inference family — a single base die
packaged into two preliminary SKUs (`AX101-OD` object detection, `AX101-OCR` text
recognition) that share one fabrication, die, QFN48 package, absolute-maximum table,
and DC characteristics block, differing only in the configured network and the
performance it delivers. The object-detection SKU is fully realized in-tree at
`examples/ax101-family/ax101-objdet/ax101-objdet.1/datasheet.tex` (XeLaTeX,
`\documentclass{anvil-datasheet}`, two-column first page, fresh-page Performance +
Pin Configuration, a complete QFN48 pin-map between the `% anvil-pinmap-begin/end`
markers, and an `% anvil-bus:` marker for the 7-bit ROI index — both mechanical
checkers pass) with `anvil-datasheet.cls` copied alongside so the version dir
compiles standalone. It ships **real stamped critic siblings** —
`ax101-objdet.1.review/` (40/44, advance: true) and `ax101-objdet.1.audit/`
(pass: true) — each carrying `rubric_id: anvil-datasheet-v1` / `rubric_total: 44`
/ `advance_threshold: 39` in `_meta.json`. This is a deliberate superset of the
`anvil:proposal` exemplar, which describes but does not vendor its critic siblings;
vendoring a real sidecar lets `tests/test_datasheet_example_brief_parses.py` assert
the stamping contract against an on-disk file. Because the project `BRIEF.md`
declares both SKUs of `family: AX101`, rubric dim 5 (family / SKU coherence) and
`datasheet-audit` step 9 (sibling shared-die cross-read) are active. The structural
contract a drafted thread should satisfy is documented in
`examples/expected-thread.1/README.md` (illustrative, not a golden file). All
content is synthesized and NON-CONFIDENTIAL — not a real part.

## Out of scope (v1)

- **Mechanical spec-diff checker** for the revision-history gate (v1 is auditor judgment over a real diff) — natural Phase-2 follow-on.
- **Automated byte-diff of marked shared blocks** across sibling SKU threads (v1 is documented audit judgment) — Phase-3 follow-up.
- **Realized `ax101-ocr` sibling sheet** — only the object-detection SKU is vendored; the OCR SKU is declared in the project BRIEF so dim 5 / audit step 9 are active, but its `datasheet.tex` is a tracked follow-up (the cross-read currently runs against the OD sheet's explicit shared-vs-per-SKU partition).
- **PDF text extraction** for spec-bundle PDFs (presence-only in v1, per issue #167).
- **No `anvil/lib/` changes.** The skill consumes `render_gate.py`, `sidecar.py`, `critics.py`, `latest_resolution.py`, and the snippets contracts as-is; critic siblings keep `scorecard_kind: "human-verdict"` with the v0.4.0 `rubric_id`/`rubric_total`/`advance_threshold` stamping.
