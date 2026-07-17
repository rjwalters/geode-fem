---
name: ip-uspto-provisional-draft
description: Drafter command for the ip-uspto-provisional skill. Two-stage procedure (outline pass, then section pass) producing a provisional specification + drawing stubs (+ optional claim-seed) from the inventor brief. Enablement depth is the drafting priority — priority only attaches to what is disclosed.
---

# ip-uspto-provisional-draft — Drafter

**Role**: drafter.
**Reads**:
- New thread: `<thread>/BRIEF.md` (required), `<thread>/refs/**`, `<thread>/prior-art/**` (for positioning awareness during drafting).
- Revise-from-feedback path (rare; the reviser is preferred): also the latest `<thread>.{N}/` and all `<thread>.{N}.*/` critic siblings.

**Writes**: `<thread>.{N+1}/` containing `_outline.json`, `spec.tex`, `anvil-uspto.cls` (copied), `drawings/`, optional `claims.tex`, and `_progress.json`.

## Drafting north star

A provisional's only legal function is **priority attachment** (35 U.S.C. §119(e)): the later non-provisional gets this filing date only for subject matter disclosed here at §112(a) written-description-and-enablement depth. Therefore:

- **Disclose deep**: every inventive feature in `BRIEF.md` §3 must be described at make-and-use depth — mechanism, not result language.
- **Disclose wide**: every embodiment, alternative, and range in `BRIEF.md` §4/§5 is conversion scope. Omissions are scope the conversion cannot claim with priority. When in doubt, include.
- **No claims required**: do not pad with claim machinery. A **claim-seed** is optional and encouraged (see below) — it sharpens feature articulation for dim 9 (*Conversion readiness*) — but its absence is never a defect.
- **No abstract**: provisionals require none; do not produce `abstract.txt`.

## Two-stage procedure

Same control surface as `anvil:ip-uspto` (see that skill's SKILL.md §"Outline control surface" for the `_outline.json` schema): **Stage A** writes the outline (cheap; `--outline-only` exits here so the operator can edit the plan), **Stage B** renders sections in outline order with per-section `status` checkpointing (`pending → in_progress → done | failed`), persisting `_outline.json` after each transition. Presence of `_outline.json` skips Stage A; an all-`done` outline with validating files is an idempotent no-op. Resume semantics follow the ip-uspto drafter: `done` sections keep their bytes, `in_progress`/`failed` section spans are re-rendered, file existence wins over the flag.

### Outline sections (provisional shape)

- **`field`** — `file: spec.tex`, `heading_macro: \fieldoftheinvention`, ~120 tokens.
- **`background`** — `file: spec.tex`, `heading_macro: \background`, ~1000 tokens. Problem + prior approaches from `BRIEF.md` §1/§2. **Do NOT admit any reference as prior art** — admissions bind the whole application family, including the conversion.
- **`summary`** — `file: spec.tex`, `heading_macro: \summary`, ~800 tokens. One to two paragraphs per inventive feature, stating each plainly with its benefit — this section seeds the conversion's independent claims, so name each feature's load-bearing elements explicitly.
- **`brief-description-of-drawings`** — `file: spec.tex`, `heading_macro: \briefdescriptionofdrawings`. `figures` array `{n, caption}`, one per planned figure.
- **`detailed-description`** — `file: spec.tex`, `heading_macro: \detaileddescription`, the bulk (budget generously; ~6000 tokens). One subsection per `BRIEF.md` §3 feature with `feature_ref`, `key_points`, `ranges`, `alternatives`, `refnums`. This is the enablement-depth surface the `s112` critic scores at weight 8 — for each feature describe at least one embodiment concretely enough that a PHOSITA can build it, state working ranges with preferred values, and enumerate alternatives. Use `\anvilpara{...}` paragraph numbering (`[0001]` style — not required for provisionals, but it makes the conversion's cross-references cheap) and `\refnum{N}` reference numerals.
- **`claim-seed`** — OPTIONAL; `file: claims.tex`, `claim_tree` shape. Include when the brief's features are mature enough to sketch claim language, or when the operator asks. 1–3 seed independents with `key_limitations`, a few dependents with `drawn_from` pointers into the detailed description. Every seed limitation MUST be backed by enabling disclosure — a seed claim is a promissory note the spec must already cover. Omit the section entirely rather than write unsupported seeds.

There is no `abstract` section and no `claims` numbering/fee discipline (3-independent / 20-claim caps are non-provisional fee rules; irrelevant here).

## Procedure

1. **Discover thread state**: enumerate `<thread>.{N}/` dirs; compute next `N`.
2. **Resume check** per `_progress.json` + `_outline.json` (identical contract to `ip-uspto-draft` steps 2): completed draft → idempotent exit; in-progress with outline → resume Stage B; in-progress without outline → delete partials, fresh Stage A.
3. **Read inputs**: `BRIEF.md` (error if missing — run `ip-uspto-intake <thread>` or hand-author one to the same shape), enumerate `refs/` and `prior-art/`. If revising from feedback, load prior version + all critic `_summary.md` + `findings.md`.
4. **Initialize `_progress.json`**: `phases.draft = in_progress`, `metadata.iteration = N+1`, `metadata.max_iterations` (from `<thread>/.anvil.json`, default 5). Shallow merge per `anvil/lib/snippets/progress.md`.
5. **Stage A — outline pass**; persist `_outline.json`; exit here on `--outline-only`.
6. **Stage B — section pass** in outline order. On the first `spec.tex` section, load the spec scaffold from `anvil/skills/ip-uspto/assets/template-spec.tex.j2` (consumer repo: `.anvil/skills/ip-uspto/assets/`) and fill `\documentclass{anvil-uspto}`, title, and inventors from `BRIEF.md` frontmatter. **Copy `anvil-uspto.cls`** from `anvil/skills/ip-uspto/assets/anvil-uspto.cls` into `<thread>.{N+1}/` so the version compiles standalone. If the ip-uspto assets are missing (consumer installed this skill without `anvil:ip-uspto`), abort with the remediation: `re-run install-anvil.sh --skills=ip-uspto,ip-uspto-provisional`.
7. **Drawings stubs**: after `brief-description-of-drawings` renders, write `drawings/drawing-descriptions.md` — one entry per figure (type, components shown with reference numerals, spatial relationships, lead-line annotations; same stub format as `ip-uspto-draft` §5i). A feature whose understanding requires a figure MUST have one — a missing essential drawing is an `s112` critical flag downstream.
8. **Validate before declaring done**:
   - `_outline.json` parses, `schema_version: 1`, every section `status: done`.
   - `spec.tex` exists, non-empty; `anvil-uspto.cls` present alongside.
   - `drawings/drawing-descriptions.md` exists with at least one figure entry.
   - `claims.tex` exists IFF the outline has a `claim-seed` section (optional both ways).
   - No `abstract.txt` (presence is a shape error — remove it).
9. **Update `_progress.json`**: `phases.draft = done`.
10. **Report**: e.g., `Drafted acme-widget-prov.1/ (outline: 5 sections done; spec 5200 words / 48 paragraphs; 4 drawing stubs; claim-seed: 2 independents + 5 dependents)` — or `claim-seed: omitted` when absent.

## Flags

- `--outline-only` — run Stage A only; write `_outline.json` and exit (`draft.state` stays `in_progress`).

## Voice and style overrides

If `.anvil/skills/ip-uspto-provisional/voice.md` exists in the consumer repo, load and apply it during drafting.

## Idempotence and resumability

Standard: completed drafts are never overwritten; crashed drafts resume per-section; validation is by file existence + non-emptiness, not flag.

## Notes for the drafter agent

- **Result language is the enemy.** "The scheduler reduces tail latency" discloses nothing. "The scheduler maintains a per-queue EWMA of service time and migrates any request whose predicted completion exceeds the deadline budget, as follows: …" attaches priority.
- **Ranges and alternatives are cheap to write and expensive to omit.** Every disclosed variant is conversion scope.
- **Never copy language from supplied prior art, and never admit it as prior art** in the Background.
- **A thin claim-seed is worse than none.** Unsupported seed limitations generate critic findings without buying conversion readiness.

## `_progress.json` snippet

```json
{
  "version": 1,
  "thread": "<slug>",
  "phases": {
    "draft": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  },
  "metadata": { "iteration": <N>, "max_iterations": 5 }
}
```

**Snippet references**: `anvil/lib/snippets/progress.md` (read-merge-write recipe), `anvil/lib/snippets/timestamp.md` (ISO-8601 UTC).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `_progress.json` records `phases.draft.state = done`.
- **Staging target**: ONLY the new `<thread>.{N+1}/` version dir.
- **Commit**: `anvil(ip-uspto-provisional/draft): <thread>.{N+1} [DRAFTED]`.

