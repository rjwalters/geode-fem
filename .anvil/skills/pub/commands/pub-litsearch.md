---
name: pub-litsearch
description: Literature-search critic for the pub skill. Read-only. Produces an annotated bibliography fragment and a positioning note for the drafter or reviser to consume. Refuses to invent citations.
---

# pub-litsearch — Literature-search critic

**Role**: literature-search critic (sibling, read-only).
**Reads**: `<thread>/BRIEF.md`, `<thread>/refs/` (any author-supplied source material), `<thread>/refs.bib` (if present). For a re-run after a reviewer flags missing prior work: also the latest `<thread>.{N}/main.tex` and any `<thread>.{N}.review/comments.md` entries tagged as related-work concerns.
**Writes**: `<thread>.0.litsearch/` (initial, pre-draft) or `<thread>.{N}.litsearch/` (re-run after revision `N`).

This command is the optional pre-draft step described in `SKILL.md`. It is a **sibling critic**, not a phase that gates the state machine. The drafter consumes the initial litsearch; the reviser consumes any re-run litsearch alongside `.review/` and `.audit/`.

## Why this is a separate role

Folding literature search into the drafter conflates two distinct failure modes:

- The drafter may write good prose around bad citations.
- The drafter may write bad prose around good citations.

Separating litsearch lets each role do one job. It also lets the reviser **re-run** litsearch when the reviewer points out a gap in related work, without re-drafting the paper — the next revision picks up the new litsearch sibling and updates the related-work section specifically.

## Critical constraint: do not invent citations

Pure-LLM literature search hallucinates citations. This role MUST NOT invent BibTeX entries from training-data recall. The only entries allowed in `candidates.bib` are:

1. Entries from `<thread>/refs.bib` (re-formatted for consistency if the author's `.bib` had issues).
2. Entries from `<thread>/refs/` source material the author explicitly supplied (e.g., a Semantic Scholar export, a `.bib` file dumped from Zotero, a list of titles + authors + venues + years in `BRIEF.md`).
3. Entries the brief explicitly mentions (e.g., the brief says "compare against Smith et al. 2024, arXiv:2405.12345 — full BibTeX in refs/"). The role copies/formats — it does not autocomplete.
4. **(only when `web_search: true` is set — issue #424)** Web-discovered entries that have passed the resolver-verified-or-dropped contract: the role extracted a DOI or arXiv ID from a live web-search hit AND `anvil/lib/cite.py::resolve()` returned a `BibRecord` for it. See "Opt-in web search" below. When the knob is absent or `false`, this source does not exist and the list above is exactly items 1–3 — behavior is byte-identical to the no-web contract.

If the brief or the reviewer's comments say "there should be related work on X" and no source material exists on disk, the role surfaces the gap in `notes.md` for the author to fill manually (e.g., by running their own search tool and adding to `<thread>/refs/`). The role does NOT invent a plausible-sounding Smith et al. 2024.

## Opt-in web search (`web_search: true`) — issue #424

**Off by default.** When the `web_search` key is absent from the thread's BRIEF frontmatter, or set to `false`, NOTHING in this section fires and the command's behavior is byte-identical to the no-web contract above (mirroring the `--rescore-mode` byte-identical-when-unset precedent in `pub-review.md`). The "Critical constraint: do not invent citations" section stays in force in both modes — web search changes where candidates may COME FROM, never the bar for what may ENTER `candidates.bib`.

**Knob locus.** `web_search: true` (a YAML boolean, not a string) in the per-thread `<thread>/BRIEF.md` frontmatter. In a post-#295 project layout, the equivalent carrier is the `web_search: true` key on the thread's `documents:` entry in the project `BRIEF.md` (recognized by `anvil/lib/project_brief.py` — document entries are STRICT, so the key is schema-validated as a strict bool). If both carriers are present, the per-thread frontmatter wins (more specific).

**What the knob enables.** The role MAY run live academic web searches (driven by the brief's `claim_area` / `closest_prior_work`, the identified gaps, and — on a re-run — the reviewer's `related-work`-tagged comments) to discover candidate prior work the author did not supply.

**Resolver-verified-or-dropped invariant.** Every web-discovered candidate is routed through the verification chokepoint at `anvil/lib/cite.py`:

1. Extract an identifier from the search hit and parse it via `parse_identifier()` (DOI forms: `10.xxxx/yyyy`, `doi:...`, `https://doi.org/...`; arXiv forms: `2305.14325`, `arxiv:...`, `https://arxiv.org/abs/...`).
2. Call `resolve()` on the identifier. The candidate may enter `candidates.bib` ONLY after `resolve()` returns a `BibRecord` (Crossref for DOIs, arXiv API for arXiv IDs; cache-first via `~/.cache/anvil/cite/`). Keys via `bib_key()`; formatting consistent with the lib's BibTeX emitter.
3. Any failure to verify demotes the hit to a **lead** (see below), never a citation:
   - `parse_identifier()` yields a `pmid` or `url` kind → `resolve()` raises `UnsupportedIdentifierError` in v0 → lead.
   - `resolve()` raises `CiteResolutionError` after retry exhaustion → lead.
   - No identifier extractable from the hit at all (title-only search result, dead link, paywall stub) → lead.

No resolver hit, no BibTeX entry. The role MUST NOT hand-write a BibTeX entry from the search hit's rendered metadata — that path is exactly the hallucination vector the no-web contract exists to close.

**Leads vs citations.** Unverified hits land in the `## Web leads (unverified)` section of `notes.md` (shape below). Leads carry no BibTeX key and MUST NOT be cited by the drafter or the reviser. The author promotes a lead to a citation by supplying a resolvable DOI / arXiv ID (in `BRIEF.md`, `refs/`, or `refs.bib`) and re-running litsearch.

**Provenance.** Every verified web-discovered entry is recorded in the `notes.md` provenance table (shape below) so author-supplied vs web-verified entries stay auditable.

## Inputs

- **Thread slug** (positional argument).
- **Brief** (`<thread>/BRIEF.md`): freeform prose with optional YAML frontmatter. Recognized frontmatter keys include `claim_area`, `closest_prior_work`, `venue`, and `web_search` (boolean, default `false` — see "Opt-in web search" above; issue #424). Unrecognized keys are passed through as context.
- **Reference material** (`<thread>/refs/**`): any supporting material the author has supplied. Treated as read-only context.
- **Existing bibliography** (`<thread>/refs.bib`): if the author has started a bibliography, re-use it as the basis for `candidates.bib`.
- **Re-run context** (re-run path only): the latest `<thread>.{N}/main.tex` (to understand current positioning) and `<thread>.{N}.review/comments.md` entries tagged `related-work` or `blocker:related-work`.

## Outputs

```
<thread>.0.litsearch/                (initial; or <thread>.{N}.litsearch/ for re-runs)
  notes.md             Positioning narrative + gaps the author should fill
  candidates.bib       Candidate BibTeX entries (re-formatted from author-supplied sources only)
  _progress.json       Phase state (phase: litsearch)
```

### `notes.md` structure

- **Positioning summary** (3–5 paragraphs): how the paper's claim relates to the supplied prior work. For each cluster of prior work, name the cluster, identify the closest 1–3 papers (with `\citetentry`-style references back to `candidates.bib`), and state how the paper extends / contradicts / complements it.
- **Confirmed coverage**: bullet list of related-work areas the supplied references cover adequately.
- **Identified gaps**: bullet list of areas where the brief or the drafter would clearly benefit from additional citations but none were supplied. Each gap names the area precisely enough that the author can search for it ("recent results on noise robustness in transformer architectures, ~2023–2025"). **The role does not invent placeholder entries to fill these gaps** — it names the gap and stops.
- **Re-run delta** (re-run path only): a short paragraph naming what changed since the previous litsearch (which review comments drove the re-run, which gaps were closed by author-supplied additions, which remain open).
- **Web provenance** (`web_search: true` runs with at least one verified web discovery only): a markdown table mapping each web-verified `candidates.bib` entry to its verification trail — columns `bib key | identifier | resolver` (e.g., `vaswani2017attention | arXiv:1706.03762 | arxiv`). Entries originating from author-supplied material do NOT appear here; the table's absence-or-presence per key is what keeps author-supplied vs web-verified entries auditable.
- **`## Web leads (unverified)`** (`web_search: true` runs with at least one unverified hit only): one entry per web-search hit that failed the resolver-verified-or-dropped contract, as a markdown table with columns `title | authors | year | URL as found | reason unresolved` (reason examples: `url-kind unsupported in v0 (UnsupportedIdentifierError)`, `pmid-kind unsupported in v0 (UnsupportedIdentifierError)`, `resolution failed after retries (CiteResolutionError)`, `no identifier extractable`). Leads have NO BibTeX key and MUST NOT be cited by the drafter or reviser. The author promotes a lead by supplying a resolvable DOI / arXiv ID. When the knob is absent or `false`, neither this section nor the provenance table is ever emitted — `notes.md` is byte-identical to the no-web contract.

### `candidates.bib`

A valid BibTeX file with one entry per cited paper. Entries follow standard BibTeX field conventions:

```bibtex
@inproceedings{smith2024example,
  author    = {Smith, Jane and Jones, Carol},
  title     = {An Example Paper Title},
  booktitle = {Proceedings of the Example Conference},
  year      = {2024},
}
```

The drafter is free to merge entries from `candidates.bib` into the version dir's `refs.bib`. Entries the drafter does not cite remain in `candidates.bib` only and do not pollute the final bibliography.

## Procedure

1. **Discover state**: enumerate `<thread>.*.litsearch/` siblings. If invoked without explicit version context, default to creating `<thread>.0.litsearch/` (the pre-draft sibling). If the latest version dir is `<thread>.{N}/` and the user (or orchestrator) requested a re-run, create `<thread>.{N}.litsearch/`.
2. **Resume check**: if the target litsearch dir's `_progress.json.litsearch.state == done` AND `notes.md` + `candidates.bib` exist, exit early — the sibling is complete.
3. **Initialize `_progress.json`**: `phases.litsearch.state = in_progress`, `phases.litsearch.started = <ISO>`.
4. **Read inputs**: load `BRIEF.md`, enumerate `<thread>/refs/`, load `<thread>/refs.bib` if present. On re-run, also load `<thread>.{N}/main.tex` and `<thread>.{N}.review/comments.md`.
5. **Build `candidates.bib`**: re-format author-supplied entries; do not invent. If the brief lists papers by title only, leave a `% TODO: needs BibTeX fields` comment per missing entry and surface in `notes.md` gaps.
5b. **Web search (conditional — `web_search: true` only; issue #424)**: when the knob resolves to `true` (per-thread BRIEF frontmatter, or the project-BRIEF document entry), run targeted live searches against the brief's claim area, the identified gaps, and (re-run path) the reviewer's `related-work`-tagged comments. For each hit, apply the resolver-verified-or-dropped invariant from the "Opt-in web search" section: extract identifier → `cite.resolve()` → on success append the `BibRecord` to `candidates.bib` (key via `bib_key()`) and record a provenance row; on `UnsupportedIdentifierError` / `CiteResolutionError` / no extractable identifier, record a `## Web leads (unverified)` row instead. When the knob is absent or `false`, this step DOES NOT fire — proceed directly from step 5 to step 6.
6. **Write `notes.md`**: positioning narrative + confirmed coverage + identified gaps + (re-run only) delta paragraph + (web-search runs only) the web provenance table and the `## Web leads (unverified)` section per the structure above.
7. **Update `_progress.json`**: `phases.litsearch.state = done`, `phases.litsearch.completed = <ISO>`.
8. **Report**: print the path to the litsearch sibling and a one-line status (e.g., `Litsearch q3-method.0.litsearch/ (12 candidates, 3 gaps surfaced)`).

## Idempotence and resumability

- A completed litsearch (`litsearch.state == done` AND `notes.md` + `candidates.bib` exist) is never re-run automatically. Re-invoking creates a NEW sibling at the next version (`<thread>.{N+1}.litsearch/`) only if the user/orchestrator requests it; otherwise it is a no-op with a notice.
- A crashed litsearch is re-runnable after deleting partial output.

## `_progress.json` snippet

This command writes the critic-sibling shape documented in `anvil/lib/snippets/progress.md`. Litsearch siblings live at `<thread>.0.litsearch/` (pre-draft) or `<thread>.{N}.litsearch/` (re-run after reviewer feedback):

```json
{
  "version": 1,
  "thread": "<slug>",
  "for_version": 0,
  "phases": {
    "litsearch": { "state": "done", "started": "<ISO>", "completed": "<ISO>" }
  }
}
```

`for_version: 0` for the pre-draft sibling; `for_version: <N>` for re-runs at version `N`. Merge rule (shallow): preserve fields not touched by this command. Use ISO-8601 UTC timestamps per `anvil/lib/snippets/timestamp.md`.

Litsearch outputs (`notes.md` + `candidates.bib`) are a domain-specific scorecard; the sibling SHOULD declare `scorecard_kind: human-verdict` in `_meta.json` per `anvil/lib/snippets/scorecard_kind.md` (the drafter reads the notes narratively, not as a programmatic per-dimension partial scorecard).

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the litsearch sibling's `_progress.json` records `litsearch.state = done` (the litsearch sibling is written directly, not via a staged-sidecar rename).
- **Staging target**: ONLY this command's own `<thread>.0.litsearch/` (pre-draft) or `<thread>.{N}.litsearch/` (re-run) sibling — never sibling critics' dirs.
- **Commit**: `anvil(pub/litsearch): <thread>.{N} [<state>]` (`<thread>.0` on the pre-draft run; the bracket carries the thread's current derived state per SKILL.md §State machine — litsearch is a sibling critic and does not gate the state machine).
