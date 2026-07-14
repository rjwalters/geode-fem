# Paper review rubric

The reviewer scores a paper against 9 weighted dimensions summing to **44**. The threshold to advance is **≥35/44**. Any **critical flag** short-circuits the verdict — the paper is blocked regardless of total score until the flagged issue is addressed.

The rubric is tuned so that **rigor + evidence + citation hygiene (6 + 6 + 5 = 17/44 ≈ 38.6%)** dominate the score. A paper's primary job is to defend a claim with evidence; prose polish is necessary but not sufficient. Compared to the memo rubric, this rubric demotes recommendation/decision clarity (papers do not recommend; they argue) and promotes positioning against prior work and reproducibility. The dim 9 *Rhetorical economy* addition (weight 4) provides explicit countervailing pressure against bloat — top venues penalize bloat heavily and dim 9 catches the failure mode where every other dim rewards adding more.

## Dimensions

| # | Dimension | Weight | What it measures |
|---|---|---|---|
| 1 | **Rigor of method / argument** | 6 | The methodology (experimental setup, proof structure, derivation) is sound and adequate to support the claims. The highest-weighted dimension. Distinct from evidence sufficiency: a method can be rigorous in structure but undersupported in results. |
| 2 | **Evidence sufficiency** | 6 | The experiments / proofs / data presented are adequate for the claims made. Sample sizes are justified, baselines are appropriate, ablations exist where claims rest on a specific design choice. Distinct from rigor: a rigorous method that produces only weak signal scores low here. |
| 3 | **Clarity of contribution** | 5 | What is new is unambiguous and stated in the abstract and introduction. A reviewer can extract the contribution(s) in one sentence per item. Failure mode: papers whose contribution is unclear are rejected even when results are strong. |
| 4 | **Related-work positioning** | 5 | Honest and accurate placement against prior art. Closest prior work is engaged on its actual merits. Failure mode: ignoring close prior work — often grounds for desk rejection. |
| 5 | **Reproducibility** | 5 | Code, data, seeds, hyperparameters, environment are referenced or supplied. Methods section is sufficient for an independent group to replicate. Increasingly a hard requirement at top venues. |
| 6 | **Figure & table quality** | 4 | Figures and tables are self-contained (caption tells the story), readable at print size, have axis labels with units, and avoid chartjunk. Tables have correct alignment and meaningful column headers. |
| 7 | **Prose & structural quality** | 4 | Abstract → introduction → methods → results → discussion flow is intact; prose is concise; no hand-waving; tense and voice are consistent. Includes LaTeX-specific concerns: no overfull hboxes in body sections, no broken cross-references (`Section ??`), no unused macros. |
| 8 | **Citation hygiene** | 5 | Every non-trivial claim has a citation; cited papers actually support the surrounding claim (audit phase verifies — this dimension catches the unsourced-claim half); bibliography entries are complete and consistent (author, title, venue, year all present). |
| 9 | **Rhetorical economy** | 4 | Is every paragraph load-bearing? Could the same argument land in fewer words? Are the most important claims surfaced early? Is hedging proportional to genuine uncertainty, not used as a cushion? Could a busy reviewer extract the contribution in 90 seconds? |
| | **Total** | **44** | Advance threshold: ≥35 |

## Perspective substrate (dim 4)

Per `anvil/lib/snippets/rubric.md` §"Rubric–perspective interaction",
the pub-side perspective sibling — historically named `litsearch` (see
`commands/pub-litsearch.md`) and the load-bearing precedent that the
framework primitive `anvil/lib/snippets/perspective.md` generalizes —
participates in **dim 4 *Related-work positioning*** as
**opportunistic substrate**.

This subsection codifies the **pre-existing implicit rule** that
litsearch has always served (a related-work section grounded in the
litsearch sibling's annotated bibliography fragment scores higher
than one grounded in drafter recall alone) and anchors that rule to
the framework-wide perspective interaction contract. No behavioral
change is intended for existing pub threads; the subsection makes the
rule **visible at the rubric surface** so consumers of other anvil
skills see the canonical pattern alongside the per-skill adopters
(deck dims 3 + 4, memo dim 3, proposal dims 4 + 6).

The rule:

- **With litsearch (or perspective) + cited candidates**: a related-
  work paragraph that cites entries from `candidates.bib`
  (synonymously `candidates.md` for non-academic substrate) — the
  closest 1–3 papers per cluster, with explicit "extends /
  contradicts / complements" framing per
  `commands/pub-litsearch.md` §"`notes.md` structure" — is treated
  as **substrate-backed**. Dim 4 may score at the **top of the
  calibrated range** on the evidence of substrate-grounded
  positioning. The reviewer notes the substrate backing in the dim 4
  justification (e.g., "Dim 4 = 5/5: §2 Related Work cites the
  closest-prior-work cluster from `candidates.bib` with explicit
  extends / contradicts framing; substrate-backed per litsearch
  sibling").
- **Without a litsearch / perspective sibling** (legacy pub threads):
  dim 4 scores against the legacy baseline alone. **No new
  deduction** is taken for perspective absence. A paper authored
  without a litsearch step continues to score on the pre-perspective
  rules — "honest and accurate placement against prior art" judged
  from the related-work prose alone. The reviewer continues to flag
  ignored close prior work per the §"Critical flags" *Close prior
  work ignored* rule below; that flag is independent of perspective
  presence.
- **With litsearch + a "known gap"**: when the litsearch sibling's
  `notes.md` "Identified gaps" names a related-work area as
  un-covered AND the paper's related-work section omits that area or
  treats it superficially, the existing dim 4 weakness (related-work
  positioning gap) is applied to a more-clearly-established miss —
  the litsearch sibling sharpens the diagnosis rather than introducing
  a new deduction. The reviewer cites both signals in the dim 4
  justification.

The rule is **opportunistic, not punitive** per the framework
contract: perspective can move dim 4 **up**, never **down**. Removing
the litsearch sibling from an otherwise-identical paper holds or
lowers the dim 4 score; it never raises it. The litsearch sibling is
non-gating per `anvil/lib/snippets/perspective.md` (it has always
been so per `commands/pub-litsearch.md` §"Why this is a separate
role"), so no paper fails dim 4 solely on litsearch absence.

**Litsearch is the load-bearing precedent.** The litsearch role
predates the perspective primitive — it shipped before
`anvil/lib/snippets/perspective.md` codified the cross-skill shape
(see `perspective.md` §"Naming: perspective, not research" for why
the framework primitive uses the name "perspective" instead of
"litsearch"). The two are the same shape; this subsection treats them
interchangeably for the dim 4 interaction. A future `pub-perspective`
wrapper is **explicitly deferred** per the canary signal — litsearch
already serves the load-bearing case.

See `commands/pub-litsearch.md` for the substrate-gathering contract,
`anvil/lib/snippets/perspective.md` for the cross-skill framework
primitive, and `anvil/lib/snippets/rubric.md` §"Rubric–perspective
interaction" for the opportunistic-not-punitive design rule.

## Scoring guidance

For each dimension, the reviewer assigns an integer between 0 and the dimension's weight. A short justification (1–3 sentences) accompanies each score, pointing to specific evidence in the paper.

Suggested calibration:
- **Full weight** — meets the standard convincingly; a sophisticated reader (a likely program committee member at a top venue) would have no substantive objection on this dimension.
- **~75% of weight** — meets the standard with a defensible gap or one specific weakness noted.
- **~50% of weight** — partial; multiple gaps or one significant weakness.
- **~25% of weight** — present but inadequate; major rework needed.
- **0** — absent or actively misleading.

**Quoted evidence (issue #464 / #475).** Every justification follows the quoted-evidence sub-rule in `anvil/lib/snippets/rubric.md` §"Dimension scoring guidance" rule 1: at least one verbatim inline quote from `main.tex` with a location anchor — `("the quoted span" — §2.1)` — per dimension, with the `no instance of <X> found` by-absence marker allowed at full weight only. The reviewer self-checks its `scoring.md` against the body via `anvil/lib/evidence_check.py` before the review sidecar lands (see `commands/pub-review.md` step 5b); a quote that does not appear verbatim in the body is fabricated evidence and the justification must be re-derived. No weight or threshold changes — this is an evidence-discipline contract on the justification prose, not a scoring change.

## Advance threshold

- **≥35/44** — advance to `READY` (proceed to `pub-audit`).
- **<35/44** — block; revise.
- **Any critical flag set (from `.review/` OR `.audit/`)** — block regardless of total. The next revision must address the flagged issue specifically and the next reviewer pass must re-evaluate the flag before the threshold check applies.

## Critical flags

A critical flag is an issue severe enough that **a sophisticated reader would immediately stop taking the paper seriously**, regardless of how well other dimensions score. Set a flag whenever such an issue is identified — this list is illustrative, not exhaustive:

- **Citation error** — A `\cite{}` resolves to a paper that does not support the surrounding claim, OR a `\cite{}` points to a `refs.bib` entry that does not exist (resolves to `[??]` in the rendered PDF).
- **Plagiarism risk** — Passages that closely mirror prior work without attribution. Includes the author's own prior work (self-plagiarism is a flag in venues that require novelty).
- **Missing experiment for a claim** — The paper claims a property (robustness, generality, efficiency) without the experiment that would substantiate it. A claim of "robust to noise" with no noise-sweep experiment is a flag.
- **Numerical inconsistency** — A number reported in the text disagrees with the corresponding figure or table. Examples: text says 87.3 accuracy, Table 2 says 87.1; abstract claims 5x speedup, results show 3x.
- **Close prior work ignored** — A paper exists in the literature that is so close to the claimed contribution that ignoring it constitutes a misrepresentation of novelty. This is harder to call than the first three — use only when the omitted paper is clearly known to a serious reviewer in the area.
- **Build / compile failure** — `pdflatex` + `bibtex` cycle does not complete cleanly, OR the rendered PDF contains unresolved citations or references. Caught by `pub-audit`; if surfaced by the reviewer (e.g., from a build log shared in the version dir), set a flag. **This flag class also covers an external companion artifact** the paper's claims rest on (issue #663): when a thread declares an `artifact_verify` block in `<thread>/.anvil.json`, `pub-review` step 4f runs the declared commands (e.g., `lake build` on a companion Lean 4 proof repository, a benchmark harness, a dataset checksum) and a non-zero exit or timeout raises an `artifact_verify_<n>`-typed critical flag — "run the artifact, not just the PDF." A broken *declaration* (missing `cwd`, unlaunchable command) fails open as a `major` finding rather than a flag. See `SKILL.md` § "External-artifact verification" and `commands/pub-review.md` step 4f.

The reviewer should also raise a flag for any other issue that, in their judgment, meets the standard above — these examples are starting points, not a closed set. **Critical flags from `pub-audit` carry equal weight** to those from `pub-review` and block advancement to `AUDITED` (i.e., the paper is not done) until addressed.

## Verdict format

The reviewer writes a `verdict.md` at the top of the review sibling dir with:

1. **Total score**: `XX / 44`.
2. **Decision**: `advance: true` or `advance: false`. (`advance: true` requires both `total ≥ 35` AND `no unresolved critical flag`.)
3. **Critical flags** (if any): bullet list, each with one-paragraph justification.
4. **Dimension summary**: a markdown table of per-dimension scores (full detail lives in `scoring.md`).
5. **Top 3 revision priorities** (if `advance: false`): the highest-leverage changes the reviser should focus on.

## Output layout

```
<thread>.{N}.review/
  verdict.md           Top-level decision (see above)
  scoring.md           Per-dimension score + justification
  comments.md          Line-level comments keyed to main.tex sections
  findings.md          Cross-section observations + rubric version transition subsection
  _review.json         Canonical critic JSON (anvil/lib/review_schema.py) — generic /44
                       scorecard, rubric: "anvil-pub-v2". Drives the convergence gate.
  _review.venue.json   (optional) Venue advisory overlay scorecard, written when
                       <thread>/.anvil.json sets a `venue` field that resolves to a
                       known venue YAML. Same JSON schema as _review.json; ADVISORY
                       ONLY (informational; does not change the convergence gate).
  _summary.md          JSON-in-markdown scorecard carrying the top-level `rubric` block +
                       dimensions; sibling to _review.json for cross-version score comparison.
  _meta.json           { critic, scorecard_kind: "human-verdict", started, finished, model,
                       schema_version, rubric_id, rubric_total, advance_threshold }
  _progress.json       Phase state for the reviewer (phase: review)
```

All 8 files (`verdict.md`, `scoring.md`, `comments.md`, `findings.md`, `_review.json`, `_summary.md`, `_meta.json`, `_progress.json`) are written atomically via `anvil/lib/sidecar.py::staged_sidecar`; the reviewer dir does not exist in the final path until the full manifest is complete (see `commands/pub-review.md` step 3). `_review.venue.json` is the lone documented-optional file (conditional on a declared venue), written inside the same staging dir but not in the required-files manifest.

The reviewer dir is **read-only once written** (state: `done` in its own `_progress.json`). Revisions consume it without modifying it.

## Venue-pinned advisory overlay rubrics

A paper thread may declare a target venue in `<thread>/.anvil.json`:

```json
{
  "max_iterations": 4,
  "venue": "neurips"
}
```

When set and a matching YAML is found, the reviewer also scores the paper against a **venue-pinned advisory rubric** (NeurIPS reviewer form, Nature broad-significance bar, arXiv reader norms, etc.) and writes a second `_review.venue.json` alongside the generic `_review.json`. The venue file uses the same `Review` schema in `anvil/lib/review_schema.py` (no new on-disk shape).

**Critical: the venue overlay is ADVISORY ONLY. It does NOT change the /44 convergence gate.** The generic 9-dimension rubric above (with its ≥35/44 threshold and the critical-flag short-circuit) remains the sole driver of the `advance` decision. The venue overlay produces additional findings the reviser consumes for venue-specific signal; it does NOT contribute points to the gate-deciding total. The per-skill `total` invariant is documented in `anvil/lib/snippets/rubric.md` §"Shape requirements".

Shipped venues:

| Venue YAML | Total | Notes |
|---|---|---|
| `rubrics/neurips.yaml` | /16 | Soundness, presentation, contribution, novelty, reproducibility. Sources NeurIPS reviewer form. |
| `rubrics/nature.yaml` | /15 | Broad significance, accessibility, evidence strength, novelty. Sources Nature reviewer instructions. |
| `rubrics/arxiv.yaml` | /10 | Citation completeness, reproducibility, clarity of contribution, scope classification. De-facto reader bar + arXiv moderation. |

Each YAML cites its public source in a header comment so it can be updated as venue guidelines change. The schema for these YAMLs is `anvil/lib/rubric.py::Rubric` with `advisory: true`; the loader skips the sum-to-total invariant for advisory rubrics. The venue discovery search order (per-thread → consumer-installed → skill-shipped) and the consumer override pattern are documented in `SKILL.md`.

When `venue` is set but no matching YAML is found, the reviewer emits a stdout warning and proceeds with the generic rubric only. The thread's review is not blocked by the missing venue — the generic gate continues to apply.

## Vision-owned dimensions (rendered-artifact overlay)

The optional `pub-vision` critic (see `commands/pub-vision.md`) scores the **compiled PDF** — not `main.tex`. It owns a four-dimension subset of the framework-wide vision rubric (`anvil/lib/vision.py::DEFAULT_VISION_DIMENSIONS`), composed via `VisionRubric(dimensions=[...], rubric_id="anvil-pub-vision-v1")`. These dims catch defects that are invisible in the LaTeX/Markdown source and that neither `pub-review` (prose/structure) nor `pub-audit` (citations/numerics) can see:

| Vision dim | Scored | What it catches for a paper |
|---|---|---|
| `label_cropping` | /5 | **Figure legibility + table overflow**: axis labels, legends, and caption text truncated by the figure box; wide `tabular`/`longtable` columns clipped at the page's right margin. |
| `axis_legibility` | /5 | **Figure legibility (font scale)**: axis labels and tick marks too small to read at print size on the rendered page. |
| `palette_adherence` | /5 | **Palette adherence for plots**: consistent print-safe palette (not raw matplotlib defaults); color-only encodings that fail in grayscale print. |
| `mathtext_artifacts` | /5 | **Mathtext artifacts** (highest stakes for a paper): rendered equations that diverge from LaTeX source intent — `$X` rendered as italic math, broken math spans, display equations overflowing the right margin. LaTeX is the source-of-truth, so a rendered-equation mismatch is a *correctness* defect, not a polish one. |
| | **/20** | Default `pub-vision` total. |

The dropped two framework dims (`vertical_overflow`, `slide_density`) are slide-centric and do not apply to a paginated, reflowing paper.

**These dims do NOT contribute to the /44 convergence gate.** Like the venue overlay, the vision scorecard is an **additive overlay** the reviser (`pub-revise`) consumes for actionable signal. The generic 9-dimension /44 rubric above remains the sole driver of the `advance` decision. The vision critic's `_review.json` (`kind=vision`) is discovered and aggregated by `anvil/lib/critics.py` alongside `.review/` and `.audit/`.

**Vision critical flags participate in the short-circuit.** The two framework vision flags — `rendered_overflow_unrecoverable` (a clipped table column or caption that loses load-bearing content) and `mathtext_artifact_breaks_meaning` (a rendered equation that changes the claim) — are real critical flags and block advancement exactly like the `.review`/`.audit` flags above until addressed. They map to rubric dim 6 (Figure & table quality) and dim 1/2 (rigor — a mis-rendered equation undermines the argument) when a reviewer later re-scores the source fix.
