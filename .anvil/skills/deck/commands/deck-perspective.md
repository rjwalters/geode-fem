---
name: deck-perspective
description: Pre-draft (or re-run) external-substrate critic for the deck skill. Read-only. Gathers market signals, competitor evidence, comparable transactions, and regulatory/customer context the drafter or reviser consumes as load-bearing pitch context. Refuses to fabricate candidates.
---

# deck-perspective — External-substrate critic (perspective sibling)

**Role**: perspective critic (sibling, read-only).
**Reads**: `<thread>/BRIEF.md`, `<thread>/refs/` (any operator-supplied source material: founder transcripts, exported financials, website exports, prior decks, market reports, analyst PDFs, regulatory filings). For a re-run after a reviewer flags missing substrate: also the latest `<thread>.{N}/deck.md` and any `<thread>.{N}.review/comments.md` / `<thread>.{N}.market/findings.md` entries tagged as market / TAM / competitive-positioning concerns.
**Writes**: `<thread>/<thread>.0.perspective/` (initial, pre-draft) or `<thread>/<thread>.{N}.perspective/` (re-run after revision `N`) — the perspective sibling is nested under the thread root per the artifact contract. Bare `<thread>.{N}/` / `<thread>.{N}.perspective/` references below are shorthand for these nested paths.

This command is the optional pre-draft step described in `SKILL.md` for the deck skill. It is a **sibling critic**, not a phase that gates the state machine. The drafter consumes the initial perspective; the reviser consumes any re-run perspective alongside `.review/`, `.market/`, `.narrative/`, `.design/`, and `.audit/`. The framework contract for the perspective shape lives in `anvil/lib/snippets/perspective.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/perspective.md`); this command is the deck-skill instantiation of that contract.

`deck-perspective` is the canary-skill consumer of the perspective primitive promoted in #143/#148. It mirrors `anvil/skills/paper/commands/paper-litsearch.md` (the load-bearing existing precedent that the perspective primitive generalizes), tuned for pitch-deck substrate: market signals, competitor decks, comparable financings, and regulatory context — NOT academic literature.

## Why this is a separate role

Folding external-substrate gathering into the drafter conflates two distinct failure modes:

- The drafter may write good prose around bad market numbers / unverified competitor claims.
- The drafter may write bad prose around good market numbers / verified competitor claims.

Separating perspective lets each role do one job. It also lets the reviser **re-run** perspective when the reviewer (or `deck-market`) points out a gap in market substrate, without re-drafting the deck — the next revision picks up the new perspective sibling and updates the affected slides (market, competition, comparables) specifically.

The architectural choice of "perspective" over "research" disambiguates from `anvil:paper`'s "research papers" domain and from consumer-local research directories some adopters maintain (per #117 / `perspective.md` §"Naming: perspective, not research"). For deck the substrate is *market perspective*, not academic literature.

## Critical constraint: do not invent candidates

Pure-LLM market research hallucinates competitors, fabricates funding amounts, and invents customer logos. This role MUST NOT invent entries from training-data recall. Every entry in `candidates.md` MUST carry a **source pointer** (per `anvil/lib/snippets/perspective.md` §"No-fabrication rule"):

- A **URL** (vendor website, Crunchbase / PitchBook page, news article, press release, analyst report, regulatory filing, podcast transcript, the competitor's own deck if publicly posted).
- A **citation pointer** to a known artifact (a `.pdf` filename in `<thread>/refs/`, a transcript line cite, a SEC filing accession number).
- A **pointer to operator-supplied material on disk** (`<thread>/refs/<file>`, `<thread>/BRIEF.md` content, founder-supplied notes).

The only entries allowed in `candidates.md` are:

1. Entries derived from `<thread>/refs/` source material the operator explicitly supplied (a Crunchbase export, a market-map PDF, a competitor website export, a podcast transcript, a press-release archive).
2. Entries the brief explicitly mentions (e.g., the brief lists "Competitors: Acme Co (Series B, 2024, $40M), Beta Inc (acquired 2023 by MegaCorp)" — copy/format, do not autocomplete missing details).
3. Entries the agent fetched live from a URL it can show, when the caller's environment provides web access (per `perspective.md` §"Subprocess-only by default — no mandated fetcher"). Every fetched entry MUST carry its source URL in the candidate row.

If the brief or the reviewer's comments name a substrate area but no source material exists on disk and no fetcher is available, the role surfaces the gap in `notes.md` for the operator to fill manually (e.g., by running their own search, dropping competitor PDFs into `<thread>/refs/`, or pasting verified comparables into the brief). The role does NOT invent a plausible-looking "Series A, $15M, led by Sequoia, 2024" to close the gap.

This rule is the deck-skill restatement of `paper-litsearch.md`'s "Critical constraint: do not invent citations" — both inherit the no-fabrication discipline from the perspective framework primitive.

## Subprocess-only by default — operator brings the fetcher

Anvil does **not** mandate a market-research fetcher (per `perspective.md` §"Subprocess-only by default — no mandated fetcher"). The perspective shape is a **convention**, not a runtime. The agent invoking `deck-perspective` brings its own web access; the framework specifies the on-disk shape and the no-fabrication rule.

Operator workflows the command supports, in order of typical use:

- **Pre-staged (most common for pitch decks)**: the operator (or founder) drops material into `<thread>/refs/`: a Crunchbase CSV export, a competitor-deck PDF dump, a market-sizing report, a competitor-website export, analyst PDFs. The perspective command re-formats from the pre-staged sources only. This is the dominant workflow because investor-quality market substrate usually comes from paid sources (Crunchbase, PitchBook, CB Insights) the operator has already exported.
- **Agent-driven (when web access is available)**: the orchestrator invokes `deck-perspective` with an agent that has `WebFetch` (or equivalent). The agent populates `notes.md` and `candidates.md` from live web sources — competitor product pages, public press releases, regulatory filings, analyst blog posts. Every fetched entry carries its source URL. Useful for the "fill the gap the founder didn't pre-stage" case.
- **Hybrid**: operator pre-stages high-confidence material (verified competitors, founder-attested customer LOIs); the agent web-fetches to fill specific gaps the operator names (e.g., "find the most recent comparable Series A in industrial automation 2024–2025"). The agent's fetches are still bounded by the no-fabrication rule.

## Inputs

- **Thread slug** (positional argument).
- **Brief** (`<thread>/BRIEF.md`): freeform prose with optional YAML frontmatter. Recognized frontmatter keys include `sector`, `stage`, `target_investors`. Unrecognized keys are passed through as context. The brief's "Competition", "Market", and "Traction" sections are the load-bearing inputs — they name competitors the perspective sibling can verify and gaps the sibling can flag.
- **Reference material** (`<thread>/refs/**`): any supporting material the operator has supplied. Treated as read-only context. Competitor decks, market reports, regulatory filings, founder transcripts, analyst PDFs, press-release archives are all in scope.
- **Re-run context** (re-run path only): the latest `<thread>.{N}/deck.md` (to understand current positioning), `<thread>.{N}.review/comments.md` entries tagged `market` / `competition` / `comparables`, and any `<thread>.{N}.market/findings.md` blockers naming missing substrate.

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.0.perspective/                (initial; or <thread>.{N}.perspective/ for re-runs)
  notes.md             Narrative synthesis: what the market substrate says + gaps surfaced
  candidates.md        Structured candidate list (competitors / comparables / customer evidence / regulatory) with source URLs / refs pointers
  _meta.json           { critic, role, started, finished, model, scorecard_kind: human-verdict, search_params }
  _progress.json       Phase state (phase: perspective; for_version: N)
```

**Atomicity** (issue #350, #376): the perspective sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`notes.md`, `candidates.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.perspective.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.perspective/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.perspective.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.perspective)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

### `notes.md` structure

- **Positioning summary** (3–5 paragraphs): how the deck's claim relates to the supplied market substrate. For each cluster (competitors, comparable transactions, customer evidence, regulatory context), name the cluster, identify the closest 1–3 entries (with anchor references back to `candidates.md`, e.g., `[#acme-series-b]`), and state how the deck extends / contradicts / complements the substrate. Examples: "The deck positions Acme as the first sub-$500k automation-stack for mid-market — `candidates.md#acme-series-b` shows the closest competitor (Acme Co, $40M Series B, 2024) targets enterprise; the brief's competitive framing is supported by the substrate." / "The deck's '$8B TAM' claim is unsupported by the supplied substrate; the closest market report (`candidates.md#mckinsey-2024-industrial`) sizes the addressable segment at $1.2B."
- **Confirmed coverage**: bullet list of substrate areas the supplied references cover adequately (e.g., "competitor landscape: 6 named direct competitors with funding stage + product positioning verified", "comparables: 3 recent Series A transactions in adjacent sector with disclosed valuations").
- **Identified gaps**: bullet list of areas where the brief or the drafter would clearly benefit from additional substrate but none was supplied or fetched. Each gap names the area precisely enough that the operator can search ("recent Series A comparables in industrial-automation, mid-market segment, 2024–2025"; "customer-side regulatory context — OSHA/ISO certifications referenced in deck but not attested in refs/"). **The role does not invent placeholder entries to fill these gaps** — it names the gap and stops.
- **Re-run delta** (re-run path only): a short paragraph naming what changed since the previous perspective sibling — which review comments / market-critic findings drove the re-run, which gaps were closed by new substrate, which remain open. Mirrors `paper-litsearch.md`'s "re-run delta" convention.

### `candidates.md`

A markdown document with one section per substrate cluster (recommended: Competitors / Comparable transactions / Customer evidence / Regulatory & market reports). Each entry is a small markdown subsection with a stable anchor and a structured body:

```markdown
### Competitors

#### `acme-series-b` — Acme Co
- **Stage**: Series B (closed 2024-09)
- **Round size**: $40M
- **Lead**: Sequoia Capital
- **Product positioning**: enterprise-tier industrial automation stack (>$500M-revenue customers)
- **Source**: https://www.crunchbase.com/funding_round/acme-series-b-2024 (verified 2026-05-30); also `refs/acme-press-release-2024-09.pdf`
- **Relevance**: closest direct competitor on category; differs on segment (enterprise vs. mid-market — see deck slide 5)

#### `beta-acquired-2023` — Beta Inc
- **Outcome**: acquired by MegaCorp 2023-Q4 (terms undisclosed)
- **Product positioning**: workflow-automation SaaS, partial overlap with deck's solution scope
- **Source**: `refs/megacorp-q4-2023-earnings.pdf` page 14; press release at https://newsroom.megacorp.com/2023/q4-acquisition
- **Relevance**: validates exit market for the deck's category; supports the "Why now" slide's consolidation thesis

### Comparable transactions

#### `gamma-seed-2025` — Gamma Industries (Seed)
- **Round size**: $5M Seed
- **Lead**: Founders Fund
- **Date**: 2025-Q1
- **Sector match**: adjacent (industrial IoT, not direct automation)
- **Source**: https://techcrunch.com/2025/02/15/gamma-industries-seed
- **Relevance**: benchmarks the ask in the deck against recent adjacent comparables

### Customer evidence

#### `verified-loi-megacorp` — MegaCorp pilot (named in BRIEF)
- **Source**: `refs/megacorp-loi-2026-04.pdf` (signed LOI); cross-referenced in BRIEF.md §Traction
- **Relevance**: deck slide 8 traction number — pilot conversion path documented

### Regulatory & market reports

#### `mckinsey-2024-industrial` — McKinsey 2024 Industrial-Automation Report
- **Source**: `refs/mckinsey-industrial-automation-2024.pdf`
- **Key figure**: $1.2B addressable mid-market segment (page 23)
- **Relevance**: contradicts BRIEF's $8B TAM — flag as gap in notes.md
```

Markdown (not BibTeX) is the right format for deck substrate: pitch-deck consumers cite competitor URLs and analyst-report page numbers, not `\bibitem`-shaped citations. The structured anchor (`#acme-series-b`) lets `notes.md` reference candidates by stable id; the drafter and `deck-market` critic resolve those anchors to verify the prose against the substrate.

The drafter and reviser are free to cite entries from `candidates.md` on relevant slides (e.g., the competition slide pulls from the Competitors section; the comparables narrative in speaker-notes references the Comparable-transactions entries). Entries the drafter does not cite remain in `candidates.md` only and do not pollute the deck.

### `_meta.json`

```json
{
  "critic": "perspective",
  "role": "deck-perspective.md",
  "started": "<ISO-8601 UTC>",
  "finished": "<ISO-8601 UTC>",
  "model": "<model id, e.g., claude-opus-4-7>",
  "scorecard_kind": "human-verdict",
  "search_params": {
    "workflow": "pre-staged|agent-driven|hybrid",
    "refs_consumed": ["refs/<file1>", "refs/<file2>"],
    "urls_fetched": ["<url1>", "<url2>"],
    "candidate_count": <N>,
    "gap_count": <N>
  }
}
```

`scorecard_kind: human-verdict` is the correct primary kind per `anvil/lib/snippets/scorecard_kind.md`: the drafter reads `notes.md` as a narrative, not as a per-dimension partial scorecard. The perspective sibling does NOT emit `_summary.md` + `findings.md` — those are `machine-summary` artifacts produced by the deck-skill's specialist critics (`deck-market`, `deck-narrative`, `deck-design`). `deck-market` is the per-skill cross-check critic that CONSUMES this perspective sibling (Phase 1C, #150) and emits machine-summary findings against the deck's claims.

`search_params` documents the workflow and the substrate the role actually consumed, so the auditor can reproduce / spot-check.

## Procedure

1. **Discover state**: enumerate `<thread>.*.perspective/` siblings under the thread root `<thread>/`. If invoked without explicit version context, default to creating `<thread>.0.perspective/` (the pre-draft sibling). If the latest version dir is `<thread>.{N}/` and the caller requested a re-run (e.g., `deck-revise` triggered re-run because a market-substrate gap was flagged), create `<thread>.{N}.perspective/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.perspective)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.perspective.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a completed perspective sibling means the final-named `<thread>.{N}.perspective/` dir exists. If it exists, exit early — the sibling is complete (idempotent per `perspective.md` §"Idempotence and resumability"). The completed sibling is read-only; re-run only by creating a NEW sibling at the next version. A partial perspective left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.perspective.tmp/` directory; the step 1 sweep has already removed it.
3. **Open the staged sidecar** for the perspective dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.perspective, required_files=["notes.md", "candidates.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.perspective.tmp/`), NOT inside the final `<thread>.{N}.perspective/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.perspective.state = in_progress`, `phases.perspective.started = <ISO>`, `for_version = N` (0 for the pre-draft sibling).

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.perspective/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.perspective` → prints the staging path (`.<thread>.{N}.perspective.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.perspective/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`notes.md`, `candidates.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.perspective/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.perspective --required notes.md,candidates.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.perspective` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.perspective.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.perspective.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.perspective.tmp <thread>.{N}.perspective` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.perspective/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load `BRIEF.md`, enumerate `<thread>/refs/`, classify each ref by format (transcript, deck PDF, market-report PDF, Crunchbase CSV, founder memo, website export). On re-run, also load `<thread>.{N}/deck.md` and the latest `.review/comments.md` + `.market/findings.md`.
5. **Choose workflow**: per `_meta.json.search_params.workflow`. Default is **pre-staged** if `<thread>/refs/` is non-empty; **agent-driven** if refs is empty but the agent has fetcher access; **hybrid** if both are available and the brief or re-run context names gaps the agent should fill. Refuse to run if the workflow is `agent-driven` but no fetcher is available — surface the missing-fetcher condition in stdout and exit non-zero with a clear message (the operator can rerun with pre-staged refs).
6. **Build `candidates.md`**: re-format pre-staged entries; if agent-driven or hybrid, fetch only with source pointers attached. Do not invent. If the brief lists competitors by name only without funding details, leave a `% TODO: needs round/date/lead — operator follow-up` comment per missing field and surface in `notes.md` gaps. Cluster the candidates by substrate area; assign stable anchor ids (`#<slug>-<short-descriptor>`) so `notes.md` can reference them.
7. **Write `notes.md`**: positioning summary (cross-referencing candidates by anchor) + confirmed coverage + identified gaps + (re-run only) delta paragraph. Each claim about the substrate MUST be backed by a candidate entry or by a direct quote from a ref file; vague handwave language ("the market is growing") is not allowed without a `candidates.md` entry to back it.
8. **Write `_meta.json`**: populate `critic`, `role`, `started`, `finished`, `model`, `scorecard_kind: human-verdict`, and `search_params` (workflow + refs_consumed + urls_fetched + candidate_count + gap_count).
9. **Update `_progress.json`** inside the staging dir: `phases.perspective.state = done`, `phases.perspective.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.perspective.tmp/` → `<thread>.{N}.perspective/`. The final-named dir only ever exists in **complete** form.
10. **Report**: print the path to the (now-renamed) perspective sibling and a one-line status (e.g., `Perspective acme-seed.0.perspective/ (12 candidates across 4 clusters, 3 gaps surfaced, workflow=hybrid)`).

## Failure modes

This role's primary failure modes (all of which the no-fabrication rule and the procedure above are designed to prevent):

- **Hallucinated competitor**: agent names a competitor not in refs and not fetched from a verifiable URL. The role MUST refuse and surface the area as a gap. Caught downstream by `deck-audit` if it slips through (every traction/competitor claim must trace to a brief or refs entry).
- **Hallucinated funding round**: agent attaches a fabricated round size / lead / date to a real competitor name. Same rule: every numeric field MUST cite a source URL or refs file. A round with no source is dropped from `candidates.md` and the gap surfaced in `notes.md`.
- **Stale URL**: agent cites a URL that 404s or was edited after the date of citation. The role records the verification date in the `Source` field (e.g., `verified 2026-05-30`); a re-run can refresh stale entries. Operators concerned about source decay should archive critical URLs to `<thread>/refs/` (e.g., via `wget --page-requisites` or a screenshot PDF).
- **Hidden top-down market sizing**: a market-report PDF in refs uses top-down methodology without disclosure; the role re-formats it without flagging the methodology. The role MUST note "top-down sizing" or "bottom-up sizing" or "methodology unstated" on each market-report candidate so `deck-market` can apply the deck-skill's "top-down-only sizing is a near-automatic disqualifier" rule downstream.
- **Refs-vs-brief contradiction silently passed through**: the brief claims a $8B TAM but the only ref-supported market report sizes the segment at $1.2B. The role MUST surface this contradiction in `notes.md` "Identified gaps" — not silently propagate either number. The drafter sees the gap and either updates the deck or asks the operator to clarify.

## Re-run pattern

This command follows the framework re-run pattern documented in `anvil/lib/snippets/perspective.md` §"Re-run pattern":

- Initial perspective lives at `<thread>.0.perspective/` (pre-draft, before `<thread>.1/` exists).
- A reviewer (or `deck-market` cross-check critic) flagging a substrate gap on `<thread>.{N}/` triggers `deck-revise` to invoke `deck-perspective` again, producing `<thread>.{N}.perspective/`. Downstream consumers (next drafter pass via `deck-revise`, next `deck-market` cross-check) read the **latest** perspective sibling — they do not aggregate across versions.
- The previous sibling at `<thread>.0.perspective/` is preserved on disk for audit trail; nothing deletes it. The auditor can compare across siblings to track substrate evolution.
- A re-run perspective sibling MUST include a delta paragraph in `notes.md` naming what changed: which review comments / market-critic findings drove the re-run, which gaps were closed by new substrate (operator added refs, agent fetched a missing comparable), which remain open. Mirrors `paper-litsearch.md`'s re-run discipline.

## Idempotence and resumability

- A completed perspective (`perspective.state == done` AND `notes.md` + `candidates.md` exist) is never re-run automatically. Re-invoking the same target sibling is a no-op with a notice. To produce a new perspective at the next version, the caller (typically `deck-revise`) requests `<thread>.{N+1}.perspective/` explicitly.
- A crashed perspective (`perspective.state == in_progress` with partial output) is re-runnable after deleting any partial output in the target sibling dir. Validation is by file existence (does `notes.md` exist? does `candidates.md` exist?), not solely by the progress flag — consistent with the snippet's §"Idempotence and resumability".

## State-machine non-gating

**Absence of a perspective sibling does NOT block the state machine.** A deck thread with no `<thread>.0.perspective/` drafts, reviews, and revises normally; the drafter consults perspective context when present and proceeds without it when absent. Per `perspective.md` §"State-machine non-gating", perspective is opt-in input, not required output. The deck-skill default critic set (`review + narrative + market + design`, per `SKILL.md`) MUST NOT list `perspective` as a required critic; `deck-perspective` is non-gating by design.

This is the safety property that lets `deck-perspective` ship incrementally: existing deck threads (pre-#149) have no perspective sibling and continue to draft unchanged. New threads that opt in to the perspective workflow get the benefit; threads that don't pay no cost.

## `_progress.json` snippet

This command writes the critic-sibling shape documented in `anvil/lib/snippets/progress.md`. Perspective siblings live at `<thread>.0.perspective/` (pre-draft) or `<thread>.{N}.perspective/` (re-run after reviewer feedback):

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": 0,
  "phases": {
    "perspective": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

`for_version: 0` for the pre-draft sibling; `for_version: <N>` for re-runs at version `N`. Merge rule (shallow): preserve fields not touched by this command. Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

Perspective outputs (`notes.md` + `candidates.md`) are read narratively by the drafter; the sibling declares `scorecard_kind: human-verdict` in `_meta.json` per `anvil/lib/snippets/scorecard_kind.md`.

## Notes for the perspective agent

- **The brief and refs are the contract.** When in doubt, refuse to fill. A named gap is a feature; a fabricated competitor or invented funding round is a critical-flag risk that propagates downstream to `deck-audit`.
- **Source URLs in every row.** No anonymous claims. If you can't show where a fact came from, it doesn't belong in `candidates.md`.
- **Cluster substrate, don't dump.** A flat list of 30 random "competitors" is less useful to the drafter than 6 verified direct competitors + 4 comparable transactions + 3 customer-evidence pointers + 2 market reports. Cluster by intended deck consumer (competition slide / comparables narrative / traction backing / market slide).
- **Surface contradictions explicitly.** If refs say one thing and the brief says another, the role's job is to NAME the contradiction in `notes.md` "Identified gaps", not to pick a side silently. The drafter (or operator) resolves.

**Snippet references**: See `anvil/lib/snippets/perspective.md` for the framework contract (layout, no-fabrication rule, three consumer classes, re-run pattern, subprocess-only-by-default posture). See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. See `anvil/lib/snippets/scorecard_kind.md` for the `human-verdict` vs `machine-summary` discriminator. See `anvil/skills/paper/commands/paper-litsearch.md` for the load-bearing existing precedent the perspective primitive generalizes.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.0.perspective/` (or `<thread>.{N}.perspective/` on re-run) — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own perspective sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(deck/perspective): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine; perspective is non-gating and does not advance the state machine.
