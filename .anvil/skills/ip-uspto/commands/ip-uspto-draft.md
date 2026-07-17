---
name: ip-uspto-draft
description: Drafter command for the ip-uspto skill. Two-stage procedure — outline pass writes the section plan to _outline.json (operator edit point); section pass renders each section in the order the outline specifies. Produces a new patent application version directory from the brief + inventorship matrix (and prior-art context if supplied), or revises from a prior version + critic siblings.
---

# ip-uspto-draft — Drafter

**Role**: drafter.
**Reads**:
- New thread: `<thread>/BRIEF.md`, `<thread>/inventorship.md`, `<thread>/refs/**`, `<thread>/prior-art/**` (for §102/§103 awareness during drafting).
- Revise-from-feedback path (rare; reviser is preferred): also the latest `<thread>.{N}/` and all `<thread>.{N}.*/` critic siblings.

**Writes**: `<thread>.{N+1}/` containing `_outline.json`, `spec.tex`, `claims.tex`, `abstract.txt`, `drawings/`, and `_progress.json`.

## Two-stage procedure overview

The drafter operates in two stages. See SKILL.md "Outline control surface" for the `_outline.json` schema and the rationale for the split.

- **Stage 5A — Outline pass (cheap)**: read inputs, produce `_outline.json` with one entry per planned section and per-section `status: pending`. If the operator passed `--outline-only`, exit after this stage so the operator can edit the JSON in place.
- **Stage 5B — Section pass (expensive)**: iterate `_outline.json.sections` in array order, render each section into its target `file`, and advance per-section `status` from `pending` → `in_progress` → `done` after each successful write.

### Invocation modes

| Invocation | Stage 5A | Stage 5B |
|---|---|---|
| `ip-uspto-draft <thread>` (fresh version, `_outline.json` absent) | run | run |
| `ip-uspto-draft <thread> --outline-only` (fresh version) | run | skip — exit after writing outline |
| `ip-uspto-draft <thread>` (`_outline.json` present, all sections `pending`) | skip — outline already there | run |
| `ip-uspto-draft <thread>` (`_outline.json` present, some `done`, some `pending`/`in_progress`/`failed`) | skip | resume — render only the not-done sections |
| `ip-uspto-draft <thread>` (`_outline.json` present, all sections `done` and all target files validate) | skip | skip — idempotent no-op |

The presence of `_outline.json` is the sole trigger for skipping 5A. There is no separate "section-only" command; re-running `ip-uspto-draft` after editing the outline does the right thing automatically.

## Inputs

- **Thread slug** (positional argument).
- **`<thread>/BRIEF.md`** (required): structured brief produced by `ip-uspto-intake` or hand-authored to the same shape.
- **`<thread>/inventorship.md`** (recommended): the inventorship matrix. Drafter does not consume attribution per se but uses the named-inventor list for the spec front matter. Drafting can proceed without it (with a warning); `finalize` will refuse to proceed if the matrix is missing or stale.
- **`<thread>/prior-art/`** (optional): operator-supplied prior art. Drafter uses this for §102/§103 awareness — distinguishing language should be present in the spec from draft 1 so the `priorart` critic has something to evaluate.
- **`<thread>/refs/`** (optional): additional reference material.

## Outputs

A new version directory:

```
<thread>.{N+1}/
  _outline.json       Section-by-section plan; per-section status tracked here (see SKILL.md "Outline control surface")
  spec.tex            Specification (LaTeX, \documentclass{anvil-uspto})
  claims.tex          Claims block (\begin{claim}...\end{claim} per claim)
  abstract.txt        Abstract (plain text, ≤150 words)
  drawings/
    drawing-descriptions.md  Stub descriptions for human illustrator (default v0 figures path)
    (or fig-1.tex, fig-1.svg, etc., when figures phase has been run)
  _progress.json      Phase state with draft: done after successful write
```

For a new thread, `N+1 == 1` so the output is `<thread>.1/`.

When `--outline-only` is passed, only `_outline.json` and `_progress.json` (with `draft.state = in_progress`) are written; the spec/claims/abstract/drawings files appear on the subsequent invocation that runs Stage 5B.

## Flags

- `--outline-only` — Run Stage 5A only. Write `_outline.json` and exit. Leaves `_progress.json.phases.draft.state == in_progress`. Equivalent to invoking the drafter, editing the outline, then invoking again; this flag just promises not to start Stage 5B in the same call.

## Procedure

1. **Discover thread state**: enumerate existing `<thread>.{N}/` dirs. Compute the next `N`.
2. **Resume check**: read `<thread>.{N+1}/_progress.json` if present.
   - If `phases.draft.state == done` AND every outline section has `status: done` AND all four required artifacts validate → version is already drafted; exit early (idempotent).
   - If `phases.draft.state == in_progress` AND `_outline.json` exists → enter resume mode (skip Stage 5A, run Stage 5B starting from the first not-`done` section; see "Resume semantics" below).
   - If `phases.draft.state == in_progress` AND `_outline.json` does NOT exist → treat as a crashed pre-outline run. Delete any partial output and start fresh from Stage 5A.
3. **Read inputs**: load `BRIEF.md` (required — error if missing), `inventorship.md` (warn if missing), enumerate `refs/` and `prior-art/`. If revising from feedback, also load the prior version's full content and concatenate all critic siblings' `_summary.md` + `findings.md`.
4. **Initialize `_progress.json`**: `phases.draft.state = in_progress`, `phases.draft.started = <ISO>`, `metadata.iteration = N+1`, `metadata.max_iterations` (inherit from `<thread>/.anvil.json` if set, else 5). If resuming, preserve `started` and update only the in-progress signal.

5. **Draft the application** in two stages.

### 5A. Outline pass — write `_outline.json`

Build the outline from `BRIEF.md` + `inventorship.md` + `prior-art/`. The outline records one entry per planned section in render order; every section starts with `status: pending`.

For a default USPTO non-provisional application, populate the following section entries (these map 1:1 onto the legacy §5a–§5i steps preserved as Stage 5B's renderer-per-id logic):

- **`field`** — `file: spec.tex`, `heading_macro: \fieldoftheinvention`, `target_tokens: 120`. `key_points` summarise the technical field from `BRIEF.md` frontmatter.
- **`background`** — `file: spec.tex`, `heading_macro: \background`, `target_tokens: 1200`. `subsections` typically split into `problem` (from `BRIEF.md` §1) and `prior-approaches` (from `BRIEF.md` §2). `sources_to_cite` is populated only for references whose publication dates have been confirmed (do NOT admit anything as prior art).
- **`summary`** — `file: spec.tex`, `heading_macro: \summary`, `target_tokens: 800`. `key_points` mirror the planned independent claims at a higher level (one bullet per inventive feature in `BRIEF.md` §3).
- **`brief-description-of-drawings`** — `file: spec.tex`, `heading_macro: \briefdescriptionofdrawings`, `target_tokens: 200`. `figures` is an array of `{n, caption}` objects, one per planned figure (4–6 is typical for the minimal disclosure example).
- **`detailed-description`** — `file: spec.tex`, `heading_macro: \detaileddescription`, `target_tokens: 6000`. `subsections` is one entry per inventive feature in `BRIEF.md` §3, each carrying:
  - `feature_ref` — a backpointer to the BRIEF section (`BRIEF.md#3.<i>`),
  - `key_points` — bullets covering at least one embodiment from §4,
  - `ranges` — typed array of numeric ranges from `BRIEF.md` §5,
  - `alternatives` — typed array of categorical alternatives from `BRIEF.md` §5,
  - `refnums` — reference numeral block reserved for this feature (the drafter picks the scheme),
  - `target_tokens` — per-subsection budget hint.
- **`claims`** — `file: claims.tex`, `target_tokens: 3000`, no `heading_macro` (`claims.tex` is a standalone file). `claim_tree` is an array of claim plans:
  - Each entry has `n` (claim number), `type` (`"independent"` | `"dependent"`), and `topic` (one-line summary of the claim's scope).
  - Independent claims carry `key_limitations` (the load-bearing limitations the drafter MUST recite).
  - Dependent claims carry `parent` (the claim number they depend from) and `drawn_from` (a pointer back into the detailed-description plan, e.g., `feature-1#alt:Si` or `feature-2#range:5GHz-10GHz`). This makes the dependent ladder traceable to the spec.
  - Default budget: ≤3 independents, ≤20 total claims (soft caps; flag overruns).
  - No multiple-dependent-on-multiple-dependent (37 CFR 1.75(c)).
- **`abstract`** — `file: abstract.txt`, `target_tokens: 200`, `word_cap: 150`. No `key_points` required — the abstract is generated from the rendered claims + summary at section-pass time.

Persist `_outline.json` to disk. If `--outline-only` was passed: report (`Outline-only pass for <thread>.{N+1}/_outline.json (<S> sections planned); edit and re-run ip-uspto-draft <thread> to render.`) and exit. `_progress.json.phases.draft.state` remains `in_progress`.

### 5B. Section pass — render each section in outline order

For each section in `_outline.json.sections` (in array order):

1. Skip if `status == done` AND the section's bytes already exist in its target `file`. (The drafter detects "the section's bytes" via the `heading_macro` for `spec.tex` sections, or by file existence for `claims.tex` / `abstract.txt`.)
2. If `status` is `pending`, `in_progress`, or `failed`: set `status = in_progress`, persist `_outline.json`, then render the section conditioned on the outline entry and the inputs from step 3.
3. On successful render, append (or replace, on resume) the section bytes in the target `file`, set `status = done`, and persist `_outline.json`.
4. On failure, set `status = failed` with an error note in `_progress.json.phases.draft.errors` and abort the pass (a follow-up invocation will resume from this section).

The renderer-per-id logic for the default sections (mapping directly onto the legacy §5a–§5i steps):

   #### 5a. Spec skeleton (rendered on the first `spec.tex` section)
   Load `anvil/skills/ip-uspto/assets/template-spec.tex.j2`. Fill in:
   - `\documentclass{anvil-uspto}` preamble.
   - Title (from `BRIEF.md` frontmatter `title`).
   - Inventors (from `BRIEF.md` frontmatter `inventors`).
   - Field of use (from `BRIEF.md` frontmatter `field_of_use`).
   - **§119(e) CROSS-REFERENCE paragraph (only when `BRIEF.md` carries a `converts_provisional` block)**: emit, as the spec's FIRST paragraph (before FIELD OF THE INVENTION), a "CROSS-REFERENCE TO RELATED APPLICATIONS" paragraph so the filed specification itself carries the priority claim (not only the ADS produced at finalize):

     ```
     CROSS-REFERENCE TO RELATED APPLICATIONS

     This application claims the benefit of U.S. Provisional Application No.
     <converts_provisional.application_number>, filed <converts_provisional.filing_date>,
     the entire disclosure of which is incorporated herein by reference.
     ```

     Drafter-time emission (here, into the spec body) is the canonical home for the priority claim; finalize emits the ADS *data* copy. **Fail loud, never silent**: if `converts_provisional` is present but `filing_date` is missing/empty, abort the draft with an error naming the missing field — never render a cross-reference paragraph with a blank filing date (the silent-priority-failure risk the conversion linkage exists to prevent). When `converts_provisional` is ABSENT, emit NO cross-reference paragraph — the spec's first paragraph is FIELD OF THE INVENTION as before (byte-identical to pre-#501).

   #### 5b. `field` — FIELD OF THE INVENTION (heading via `\fieldoftheinvention`)
   One paragraph naming the technical field, sized for a USPTO examiner classifier.

   #### 5c. `background` — BACKGROUND OF THE INVENTION (heading via `\background`)
   Two to four paragraphs. Describe the problem (from `BRIEF.md` §1) and the prior approaches (from `BRIEF.md` §2). **Do NOT admit any reference as prior art** — discuss approaches in terms of what was generally done in the field, citing only when the inventor has confirmed publication dates. Distinguishing language goes here (it will be refined by the `priorart` critic later).

   #### 5d. `summary` — SUMMARY OF THE INVENTION (heading via `\summary`)
   One to two paragraphs per inventive feature (`BRIEF.md` §3). State each inventive feature plainly and the benefit it provides. The SUMMARY should mirror the independent claims at a higher level — a reader of the summary should be able to anticipate roughly what the independent claims will cover.

   #### 5e. `brief-description-of-drawings` — BRIEF DESCRIPTION OF THE DRAWINGS (heading via `\briefdescriptionofdrawings`)
   One line per figure: `FIG. <N>. <one-line description>.` In v0, drawings are stubs (see `drawings/drawing-descriptions.md`); the brief description should still list every planned figure so the reviewer can check correspondence. Source: the `figures` array on this outline entry.

   #### 5f. `detailed-description` — DETAILED DESCRIPTION OF EMBODIMENTS (heading via `\detaileddescription`)
   The bulk of the spec. Iterate the section's `subsections` array; for each:
   - Describe at least one embodiment from `BRIEF.md` §4 (via `feature_ref`) in concrete detail. Use reference numerals (`\refnum{<N>}` macro from the class) consistently — each component referenced in spec must appear in a drawing. Pull numerals from the subsection's `refnums` slot.
   - For each entry in `ranges`, state the working range ("the operating frequency may range from 5 GHz to 80 GHz, preferably between 20 GHz and 60 GHz, most preferably about 40 GHz").
   - For each entry in `alternatives`, list the alternatives ("the substrate material may be silicon, germanium, or a III-V semiconductor including gallium arsenide and indium phosphide").
   - Acknowledge edge cases from `BRIEF.md` §6 without overstating the limitations.
   - **Use `\anvilpara{...}` for each numbered paragraph** so the class produces `[0001]`, `[0002]`, … numbering automatically.

   #### 5g. `claims` — CLAIMS (file: `claims.tex`, included from spec.tex)
   Produce `claims.tex` from the section's `claim_tree`. The tree IS the plan: render each entry as a `\begin{claim}...\end{claim}` block in tree order.
   - **3 independent claims maximum** by default (USPTO charges fees beyond 3 independents). Layer them: a broad apparatus claim, a method claim, a system-level claim — chosen based on the inventive features.
   - **Dependent claim ladder**: for each independent claim, write 3–6 dependent claims that progressively narrow. Each dependent should add a specific limitation drawn from an embodiment or alternative in `BRIEF.md` §4 or §5 — sourced from the dependent's `drawn_from` pointer in the outline.
   - **20 claims total maximum** by default (USPTO charges fees beyond 20). If the inventive material justifies more, raise it but flag the cost in the operator notes.
   - **No multiple-dependent-on-multiple-dependent** claims (37 CFR 1.75(c)). Multi-dependent ("any of claims 1 to 3") is permitted but its parents must themselves be single-dependent.
   - **Antecedent basis discipline**: every claim term introduced as `a widget` must be referenced subsequently as `the widget`, never as `said widget` (modern USPTO style preference).
   - Use `\begin{claim}...\end{claim}` for each claim; the class handles numbering.

   #### 5h. `abstract` — ABSTRACT (file: `abstract.txt`, plain text)
   ≤150 words, single paragraph. State what the invention is and the principal use. The abstract is for searchability; it does NOT limit claim scope and should not contain unnecessary detail.

   #### 5i. Drawings stubs (default v0 path)
   The drawings stubs are NOT a separate outline section in v0 — they are derived deterministically from `brief-description-of-drawings.figures`. Write `drawings/drawing-descriptions.md` after the `brief-description-of-drawings` section renders, one entry per figure:

   ```markdown
   # Drawing descriptions — <thread>.<N>

   Each entry below is a stub for a human illustrator. Follow 37 CFR 1.84 (black ink, numbered FIG. N, lead lines, reference numerals shared with spec).

   ## FIG. 1 — <one-line caption>
   - **Type**: <block diagram | flowchart | cross-section | perspective | schematic>
   - **Components shown** (reference numerals): 10 (housing), 12 (input port), 14 (processor), 16 (output port).
   - **Spatial relationships**: <one paragraph describing relative position and connection>.
   - **Annotations/lead lines**: each numeric reference is connected to its component with a lead line.

   ## FIG. 2 — <one-line caption>
   ...
   ```

   The figurer phase (`ip-uspto-figures`) can later replace these stubs with TikZ or rendered images.

### Resume semantics

The outline narrows the legacy "delete the whole partial draft on resume" rule to a per-section span. On a resume invocation:

- For each section in array order:
  - `status: done` → skip (already rendered, bytes validated).
  - `status: pending` → render as in a fresh pass.
  - `status: in_progress` or `status: failed` → treat the section's bytes (the span between its `heading_macro` and the next heading in `spec.tex`, or the full `claims.tex` / `abstract.txt` for those sections) as junk and re-render from scratch.
- The version directory is NOT deleted as a whole. Sections that completed in a prior partial run keep their bytes.
- Validation rule applies: a `status: done` section whose target bytes are missing is treated as not-done and re-rendered.

If `_outline.json` is present with all sections `status: done` AND `spec.tex`, `claims.tex`, `abstract.txt` validate, the entire draft command is a no-op (matches the legacy idempotence behavior).

6. **Validate before declaring done**:
   - `_outline.json` exists, parses, has `schema_version: 1`, and **every section has `status: done`**.
   - `spec.tex` exists and is non-empty.
   - `claims.tex` exists and contains at least one `\begin{claim}` block.
   - `abstract.txt` exists, is non-empty, and is ≤150 words.
   - `drawings/drawing-descriptions.md` exists with at least one figure entry.
7. **Update `_progress.json`**: `phases.draft.state = done`, `phases.draft.completed = <ISO>`.
8. **Report**: print the path to the new version dir and a one-line status (e.g., `Drafted acme-widget.1/ (outline: 7 sections done; spec: 4200 words / 45 paragraphs, 3 independent + 14 dependent claims, abstract 138 words, 4 drawing stubs)`).

## Voice and style overrides

If `.anvil/skills/ip-uspto/voice.md` exists in the consumer repo, load it and apply during drafting. This is how a firm customizes its house drafting style (e.g., preferred claim format conventions, sentence-length preferences) without forking the skill.

## Idempotence and resumability

- A completed draft (`_progress.json.draft.state == done` AND all four required artifacts exist) is never overwritten. Re-running is a no-op with a notice.
- A crashed draft is re-runnable after deleting partial output. Validation is by file existence + content non-emptiness, not solely by the progress flag.

## `_progress.json` snippet

Minimum schema this command writes (matches `SKILL.md`):

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "draft": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": {
    "iteration": <N>,
    "max_iterations": 5
  }
}
```

Merge rule: read existing `_progress.json` if present, update only `phases.draft` and `metadata`, preserve all other fields.

## Notes for the drafter agent

- **Antecedent basis is checked by the `s112` critic.** Do not be sloppy — every "the X" must have a prior "a X" or "an X" in the same claim chain.
- **Independent claims are the legal product.** A great spec with a bad independent claim is a worthless patent. Draft the independent claims with the most care.
- **Never copy claim language from cited prior art.** If `<thread>/prior-art/` contains a reference, distinguish from it — do not echo it. The `priorart` critic will catch this.
- **The class macros do work for you** (`\anvilpara`, `\refnum`, claim environment). Use them — manual paragraph numbering is error-prone and will fail pre-flight.
- **3 independents / 20 total claims is a soft cap.** Exceed it when the invention justifies, but note the additional USPTO fees in the operator's report.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.draft.state = done`.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(ip-uspto/draft): <thread>.{N+1} [DRAFTED]`.

