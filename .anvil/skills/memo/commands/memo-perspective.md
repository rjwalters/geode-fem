---
name: memo-perspective
description: Pre-draft (or re-run) external-substrate critic for the memo skill. Read-only. Gathers comparable memos, market signals, prior-art analyses, regulatory context, and customer-side evidence the drafter or reviser consumes as load-bearing decision context. Refuses to fabricate candidates.
---

# memo-perspective — External-substrate critic (perspective sibling)

**Role**: perspective critic (sibling, read-only).
**Reads**: `<thread>/BRIEF.md`, the resolved refs-dir list from `anvil/skills/memo/lib/refs_resolver.py::resolve_refs_dirs(<thread_dir>)` — `<thread>/refs/` for the legacy single-thread shape, plus `<portfolio>/research/` for the portfolio-shared shape (issue #280) when a sibling research/ pool exists (any operator-supplied source material: founder CVs, interview transcripts, public filings, research papers, customer LOIs, comparable memos, market reports, regulatory filings, prior versions of the memo, portfolio-level vertical briefs / comp matrices / case studies). For a re-run after a reviewer flags missing substrate: also the latest `<thread>.{N}/<thread>.md` and any `<thread>.{N}.review/comments.md` entries tagged as market / evidence / comparables / risk concerns.
**Writes**: `<thread>.0.perspective/` (initial, pre-draft) or `<thread>.{N}.perspective/` (re-run after revision `N`).

This command is the optional pre-draft step described in `SKILL.md` for the memo skill. It is a **sibling critic**, not a phase that gates the state machine. The drafter consumes the initial perspective; the reviser consumes any re-run perspective alongside `.review/` and any optional `.audit/` / `.critic/` siblings. The framework contract for the perspective shape lives in `anvil/lib/snippets/perspective.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/perspective.md`); this command is the memo-skill instantiation of that contract.

`memo-perspective` is the second-consumer rollout of the perspective primitive shipped via Epic #143 (#148 framework snippet, #149 deck-perspective canary, #150 deck-market cross-check). It mirrors `anvil/skills/deck/commands/deck-perspective.md` (the load-bearing precedent) and the earlier `anvil/skills/paper/commands/paper-litsearch.md` (the original skill-local pattern), tuned for memo substrate: comparable memos, sector / market signals, customer-side traction evidence, and prior-art analyses — NOT pitch-deck competitive substrate and NOT academic literature surveys.

## Why this is a separate role

Folding external-substrate gathering into the drafter conflates two distinct failure modes:

- The drafter may write a coherent memo around bad market numbers / unverified comparable transactions / mis-attributed traction.
- The drafter may write an incoherent memo around verified market numbers and verified comparables.

Separating perspective lets each role do one job. It also lets the reviser **re-run** perspective when the reviewer points out a substrate gap (e.g., "no comparable transactions cited", "no public-filings substrate behind the sized claim"), without re-drafting the memo — the next revision picks up the new perspective sibling and updates the affected sections (market framing, risks, financial reasoning) specifically.

The architectural choice of "perspective" over "research" disambiguates from `anvil:paper`'s "research papers" domain and from consumer-local research directories some adopters maintain (per #117 / `perspective.md` §"Naming: perspective, not research"). For memo the substrate is *decision perspective* — market positioning, comparable transactions, customer-side evidence — not academic literature.

## Critical constraint: do not invent candidates

Pure-LLM substrate gathering hallucinates comparable companies, fabricates funding rounds, and invents customer logos. This role MUST NOT invent entries from training-data recall. Every entry in `candidates.md` MUST carry a **source pointer** (per `anvil/lib/snippets/perspective.md` §"No-fabrication rule"):

- A **URL** (vendor website, Crunchbase / PitchBook page, news article, press release, analyst report, regulatory filing, podcast transcript, SEC filing).
- A **citation pointer** to a known artifact (a `.pdf` filename in `<thread>/refs/`, a transcript line cite, a SEC filing accession number, a DOI for a research paper backing a technical claim).
- A **pointer to operator-supplied material on disk** (`<thread>/refs/<file>`, `<thread>/BRIEF.md` content, author-supplied notes).

The only entries allowed in `candidates.md` are:

1. Entries derived from `<thread>/refs/` source material the operator explicitly supplied (a Crunchbase export, a comparable-memo PDF, a public filing, a customer LOI, a research paper, a market report).
2. Entries the brief explicitly mentions (e.g., the brief lists "Comparable transactions: Acme Series A 2024 $12M, Beta acquired 2023 by MegaCorp" — copy/format, do not autocomplete missing details).
3. Entries the agent fetched live from a URL it can show, when the caller's environment provides web access (per `perspective.md` §"Subprocess-only by default — no mandated fetcher"). Every fetched entry MUST carry its source URL in the candidate row.

If the brief or the reviewer's comments name a substrate area but no source material exists on disk and no fetcher is available, the role surfaces the gap in `notes.md` for the operator to fill manually (e.g., by running their own search, dropping comparable-memo PDFs into `<thread>/refs/`, or pasting verified comparables into the brief). The role does NOT invent a plausible-looking "Series A, $12M, led by Sequoia, 2024" to close the gap.

This rule is the memo-skill restatement of `deck-perspective.md`'s and `paper-litsearch.md`'s no-fabrication discipline — all three inherit it from the perspective framework primitive.

**Refs disambiguation.** `<thread>/refs/` carries two coexisting file-roles per SKILL.md §"Source-of-truth materials": author-supplied source-of-truth materials (named for their content — `cv.pdf`, `transcript-foo.md`, `filing-s1.pdf`) AND drafter-written citation stubs (named for citation keys — `<key>.md` with `# TODO: source for <claim>`). The perspective role reads BOTH file-shapes as candidate substrate: source-of-truth materials are direct references the perspective entries can point to, and partially-completed citation stubs MAY be filled out as side effects (see "Side-effect: filling refs/ citation stubs" below).

## Subprocess-only by default — operator brings the fetcher

Anvil does **not** mandate a substrate fetcher (per `perspective.md` §"Subprocess-only by default — no mandated fetcher"). The perspective shape is a **convention**, not a runtime. The agent invoking `memo-perspective` brings its own web access; the framework specifies the on-disk shape and the no-fabrication rule.

Operator workflows the command supports, in order of typical use:

- **Pre-staged (most common for investment memos)**: the operator (or analyst) drops material into `<thread>/refs/`: a comparable-memo PDF dump, a Crunchbase CSV export, founder CVs, public filings, customer LOIs, market-sizing reports, sector analyses. The perspective command re-formats from the pre-staged sources only. This is the dominant workflow because investor-quality memo substrate usually comes from paid sources (Crunchbase, PitchBook, sector reports) the operator has already exported, plus operator-controlled material (LOIs, founder bios) that lives only on disk.
- **Agent-driven (when web access is available)**: the orchestrator invokes `memo-perspective` with an agent that has `WebFetch` (or equivalent). The agent populates `notes.md` and `candidates.md` from live web sources — competitor product pages, public press releases, regulatory filings, analyst blog posts, recent comparable funding rounds. Every fetched entry carries its source URL. Useful for the "fill the gap the operator didn't pre-stage" case (e.g., "most recent Series A comparable in industrial-automation 2025").
- **Hybrid**: operator pre-stages high-confidence material (verified comparables, founder-attested LOIs, public filings); the agent web-fetches to fill specific gaps the operator names (e.g., "find the regulatory filing for the FDA pathway the memo's technical thesis depends on"). The agent's fetches are still bounded by the no-fabrication rule.

## Inputs

- **Thread slug** (positional argument).
- **Brief** (`<thread>/BRIEF.md`): freeform prose with optional YAML frontmatter. Recognized frontmatter keys include `company`, `sector`, `stage`, `check_size`, `recommendation_target`. Unrecognized keys are passed through as context. The brief's "Competition", "Market", "Traction", and "Risks" sections are the load-bearing inputs — they name comparables the perspective sibling can verify and gaps the sibling can flag.
- **Reference material** (resolved via `anvil/skills/memo/lib/refs_resolver.py::resolve_refs_dirs(<thread_dir>)`): the per-thread `<thread>/refs/**` AND, when a sibling `<portfolio>/research/` directory exists (issue #280), the portfolio-level `<portfolio>/research/**` evidence pool. Any supporting material the operator has supplied — comparable-memo PDFs, market reports, regulatory filings, founder transcripts, CVs, public filings, customer LOIs, sector analyses, portfolio-level vertical briefs / comp matrices / case studies — all in scope. Treated as read-only context. Both file-roles in `refs/` (source-of-truth materials and drafter-written citation stubs) are read; the portfolio-level `research/` is expected to carry source-of-truth materials only (citation stubs remain a per-thread author convention).
- **Re-run context** (re-run path only): the latest `<thread>.{N}/<thread>.md` (to understand current positioning), `<thread>.{N}.review/comments.md` entries tagged `market` / `evidence` / `comparables` / `risk`, and any auditor-sibling findings naming missing substrate.

## Outputs

```
<thread>.0.perspective/                (initial; or <thread>.{N}.perspective/ for re-runs)
  notes.md             Narrative synthesis: what the substrate says + gaps surfaced
  candidates.md        Structured candidate list (comparables / cited research / market reports / customer evidence / regulatory) with source URLs / refs pointers
  _meta.json           { critic, role, started, finished, model, scorecard_kind: human-verdict, search_params }
  _progress.json       Phase state (phase: perspective; for_version: N)
```

**Atomicity** (issue #350, #376): the perspective sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`notes.md`, `candidates.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.perspective.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.perspective/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.perspective.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.perspective)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

### `notes.md` structure

- **Positioning summary** (3–5 paragraphs): how the memo's planned recommendation relates to the supplied substrate. For each cluster (comparable transactions, cited research, market reports, customer evidence, regulatory context), name the cluster, identify the closest 1–3 entries (with anchor references back to `candidates.md`, e.g., `[#acme-series-a-2024]`), and state how the memo's anticipated framing extends / contradicts / complements the substrate. Examples: "The brief frames the company as the first sub-$5M-ARR seed-stage entrant in vertical-SaaS for industrial maintenance — `candidates.md#acme-series-a-2024` shows the closest comparable (Acme, $12M Series A, 2024) targeted a similar segment at the post-revenue stage; the memo's stage framing is supported by the substrate." / "The brief's '$8B TAM' claim is unsupported by the supplied substrate; the closest market report (`candidates.md#mckinsey-2024-industrial`) sizes the addressable segment at $1.2B — flag for the drafter."
- **Confirmed coverage**: bullet list of substrate areas the supplied references cover adequately (e.g., "comparable transactions: 4 named Series A comparables in adjacent sector with disclosed valuations and lead investors verified", "founder background: CV + LinkedIn-archive PDFs in refs/ cover both founders' prior roles end-to-end", "customer evidence: 3 LOIs in refs/ with explicit-permission copy for memo quotation").
- **Identified gaps**: bullet list of areas where the brief or the drafter would clearly benefit from additional substrate but none was supplied or fetched. Each gap names the area precisely enough that the operator can search ("recent Series Seed comparables in vertical-SaaS-industrial-maintenance, 2024–2025"; "regulatory pathway for OSHA-adjacent certifications referenced in BRIEF but not attested in refs/"; "comparable exit comparables — the memo will need an exit anchor and refs/ has only funding-round data"). **The role does not invent placeholder entries to fill these gaps** — it names the gap and stops.
- **Re-run delta** (re-run path only): a short paragraph naming what changed since the previous perspective sibling — which review comments / auditor findings drove the re-run, which gaps were closed by new substrate, which remain open. Mirrors `deck-perspective.md`'s and `paper-litsearch.md`'s "re-run delta" convention.

### `candidates.md`

A markdown document with one section per substrate cluster (recommended: Comparable transactions / Cited research / Market reports / Customer evidence / Regulatory context). Each entry is a small markdown subsection with a stable anchor and a structured body:

```markdown
### Comparable transactions

#### `acme-series-a-2024` — Acme Co
- **Stage**: Series A (closed 2024-09)
- **Round size**: $12M
- **Lead**: Sequoia Capital
- **Product positioning**: vertical-SaaS for industrial-maintenance scheduling (mid-market)
- **Source**: https://www.crunchbase.com/funding_round/acme-series-a-2024 (verified 2026-05-30); also `refs/acme-press-release-2024-09.pdf`
- **Relevance**: closest direct comparable on sector + stage; valuation anchor for the recommendation's check-size framing

#### `beta-acquired-2023` — Beta Inc
- **Outcome**: acquired by MegaCorp 2023-Q4 (terms undisclosed)
- **Product positioning**: workflow-automation SaaS, partial overlap with memo's solution scope
- **Source**: `refs/megacorp-q4-2023-earnings.pdf` page 14; press release at https://newsroom.megacorp.com/2023/q4-acquisition
- **Relevance**: validates exit market for the company's category; supports the memo's "Why now" framing on consolidation

### Cited research

#### `levenson-2006-systems-engineering` — Levenson, Engineering a Safer World (2006)
- **Citation**: Levenson, N. (2006). "Engineering a Safer World: Systems Thinking Applied to Safety." MIT Press.
- **Source**: `refs/levenson-2006.pdf` (operator-supplied PDF); also DOI: 10.7551/mitpress/8179.001.0001
- **Relevance**: load-bearing for the memo's technical-thesis section on STAMP/STPA — the brief names this citation but the drafter MUST cite the page for the specific claim.

### Market reports

#### `mckinsey-2024-industrial` — McKinsey 2024 Industrial-Automation Report
- **Source**: `refs/mckinsey-industrial-automation-2024.pdf`
- **Key figure**: $1.2B addressable mid-market segment (page 23, bottom-up sizing per disclosed methodology)
- **Relevance**: contradicts BRIEF's $8B TAM — flag as gap in notes.md; the memo's market section must reconcile

### Customer evidence

#### `loi-bigcorp-2026-04` — BigCorp pilot LOI
- **Source**: `refs/loi-bigcorp-2026-04.pdf` (signed LOI, explicit-permission quotation rights)
- **Relevance**: anchors the memo's traction section; the LOI's stated pilot scope is the data point the drafter cites

### Regulatory context

#### `fda-pathway-510k-precedent` — FDA 510(k) precedent
- **Source**: `refs/fda-510k-precedent-2023.pdf` (precedent device clearance letter)
- **Relevance**: load-bearing for the memo's risks section — the company's regulatory pathway depends on the precedent's reusability
```

Markdown (not BibTeX) is the right format for memo substrate: investment-memo consumers cite comparable URLs, public-filing page numbers, and LOI excerpts, not `\bibitem`-shaped citations. The structured anchor (`#acme-series-a-2024`) lets `notes.md` reference candidates by stable id; the drafter and any cross-check critic resolve those anchors to verify the prose against the substrate.

The drafter and reviser are free to cite entries from `candidates.md` in relevant memo sections (e.g., the Market & competitive framing section pulls from Comparable transactions and Market reports; the Risks section may pull from Regulatory context; the Traction discussion references Customer evidence). Entries the drafter does not cite remain in `candidates.md` only and do not pollute the memo.

### Side-effect: filling refs/ citation stubs

Per `SKILL.md` §"Citation stubs", the drafter MAY write `<thread>/refs/<key>.md` citation stubs during draft. The perspective role, when it has source material for a stub that already exists at `<thread>/refs/<key>.md` (carrying `# TODO: source for <claim>`), MAY fill in the stub with the resolved source pointer (URL or refs-document reference) it found. This is an opt-in side effect — the perspective sibling's primary outputs (`notes.md` + `candidates.md` + `_meta.json` + `_progress.json`) are unchanged regardless. Stubs created by the drafter that the perspective role can NOT resolve MUST stay as TODO stubs; the perspective role does NOT invent a source to clear a TODO.

The integration with the existing PR #140 citation-stub convention is intentional: stubs accumulate at the thread level across revisions, so a perspective sibling at `<thread>.0.perspective/` can productively fill stubs left by the drafter at `<thread>.1/` (when re-run as `<thread>.1.perspective/` after the reviewer flags an evidence gap). The reviewer reads the stub's content (resolved or unresolved) as part of the rubric dim 3 *Evidence quality* judgment — see `rubric.md` §"Citation hooks (dim 3)" for the deduction rule.

### `_meta.json`

```json
{
  "critic": "perspective",
  "role": "memo-perspective.md",
  "started": "<ISO-8601 UTC>",
  "finished": "<ISO-8601 UTC>",
  "model": "<model id, e.g., claude-opus-4-7>",
  "scorecard_kind": "human-verdict",
  "search_params": {
    "workflow": "pre-staged|agent-driven|hybrid",
    "refs_consumed": ["refs/<file1>", "refs/<file2>"],
    "urls_fetched": ["<url1>", "<url2>"],
    "candidate_count": <N>,
    "gap_count": <N>,
    "stubs_filled": ["refs/<key1>.md", "refs/<key2>.md"]
  }
}
```

`scorecard_kind: human-verdict` is the correct primary kind per `anvil/lib/snippets/scorecard_kind.md`: the drafter reads `notes.md` as a narrative, not as a per-dimension partial scorecard. The perspective sibling does NOT emit `_summary.md` + `findings.md` — those are `machine-summary` artifacts produced by skill-specific cross-check critics. A `memo-market` cross-check critic mirroring `deck-market` is intentionally OUT of scope for this rollout (see #143 Phase 2C / "Out of scope" in the issue body); the memo skill consumes perspective via the drafter only in this phase.

`search_params` documents the workflow and the substrate the role actually consumed, so the auditor can reproduce / spot-check. `stubs_filled` records any side-effect stub resolutions per the "Side-effect: filling refs/ citation stubs" section above (omit the field entirely when no stubs were filled).

## Procedure

1. **Discover state**: enumerate `<thread>.*.perspective/` siblings. If invoked without explicit version context, default to creating `<thread>.0.perspective/` (the pre-draft sibling). If the latest version dir is `<thread>.{N}/` and the caller requested a re-run (e.g., `memo-revise` triggered re-run because a substrate gap was flagged), create `<thread>.{N}.perspective/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.perspective)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.perspective.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same portfolio root are NOT touched (issue #350, #376). The sweep is idempotent and logs at INFO level when it removes a dir.
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a completed perspective sibling means the final-named `<thread>.{N}.perspective/` dir exists — the atomic-rename contract guarantees the dir only exists when complete. If `<thread>.{N}.perspective/` exists, exit early — the sibling is complete (idempotent per `perspective.md` §"Idempotence and resumability"). The completed sibling is read-only; re-run only by creating a NEW sibling at the next version. A partial perspective left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.perspective.tmp/` directory (NOT as a partially-filled `<thread>.{N}.perspective/`); the sweep in step 1 has already removed any such partial. Backwards-compat: if a legacy `<thread>.{N}.perspective/` exists WITHOUT `notes.md` or `candidates.md` (pre-#350 partial shape), delete the dir and re-run.
3. **Open the staged sidecar** for the perspective dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.perspective, required_files=["notes.md", "candidates.md", "_meta.json", "_progress.json"])`. Every file write from this step through step 10 MUST land **inside the yielded staging directory** (the path the context manager yields, of the shape `.<thread>.{N}.perspective.tmp/`), NOT inside the final `<thread>.{N}.perspective/` path. On clean context exit, the staged sidecar primitive verifies every name in the manifest exists, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.perspective.state = in_progress`, `phases.perspective.started = <ISO>`, `for_version = N` (0 for the pre-draft sibling).

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.perspective/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.perspective` → prints the staging path (`.<thread>.{N}.perspective.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.perspective/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`notes.md`, `candidates.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.perspective/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.perspective --required notes.md,candidates.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.perspective` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.perspective.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.perspective.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.perspective.tmp <thread>.{N}.perspective` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.perspective/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load `BRIEF.md`, enumerate the resolved refs-dir list returned by `anvil/skills/memo/lib/refs_resolver.py::resolve_refs_dirs(<thread_dir>)` — `[<thread>/refs/]` for the legacy single-thread shape, or `[<thread>/refs/, <portfolio>/research/]` for the portfolio-shared shape (issue #280). Classify each ref by format and role (source-of-truth material: CV / transcript / filing / paper / LOI / market-report / comparable-memo / portfolio-level vertical brief / comp matrix / case study; citation stub: per-thread `<key>.md` with `# TODO: source for <claim>`). On re-run, also load `<thread>.{N}/<thread>.md` and the latest `.review/comments.md` plus any `.audit/` findings.
5. **Choose workflow**: per `_meta.json.search_params.workflow`. Default is **pre-staged** if `<thread>/refs/` is non-empty; **agent-driven** if refs is empty but the agent has fetcher access; **hybrid** if both are available and the brief or re-run context names gaps the agent should fill. Refuse to run if the workflow is `agent-driven` but no fetcher is available — surface the missing-fetcher condition in stdout and exit non-zero with a clear message (the operator can rerun with pre-staged refs).
6. **Build `candidates.md`**: re-format pre-staged entries; if agent-driven or hybrid, fetch only with source pointers attached. Do not invent. If the brief lists comparables by name only without funding details, leave a `% TODO: needs round/date/lead — operator follow-up` comment per missing field and surface in `notes.md` gaps. Cluster the candidates by substrate area; assign stable anchor ids (`#<slug>-<short-descriptor>`) so `notes.md` can reference them.
7. **Side-effect: fill stubs where possible**: for each citation stub at `<thread>/refs/<key>.md` carrying `# TODO: source for <claim>`, check whether the perspective role has resolved a source pointer for that claim. If yes, overwrite the stub with the resolved source pointer (URL or refs-document reference + one-paragraph context); if no, leave the stub untouched. Record filled stub paths in `_meta.json.search_params.stubs_filled`. The perspective role does NOT delete stubs and does NOT create new stubs (stub creation remains the drafter's contract per `commands/memo-draft.md` step 6 *Evidence*).
8. **Write `notes.md`**: positioning summary (cross-referencing candidates by anchor) + confirmed coverage + identified gaps + (re-run only) delta paragraph. Each claim about the substrate MUST be backed by a candidate entry or by a direct quote from a ref file; vague handwave language ("the market is growing") is not allowed without a `candidates.md` entry to back it.
9. **Write `_meta.json`**: populate `critic`, `role`, `started`, `finished`, `model`, `scorecard_kind: human-verdict`, and `search_params` (workflow + refs_consumed + urls_fetched + candidate_count + gap_count + stubs_filled).
10. **Update `_progress.json`** inside the staging dir: `phases.perspective.state = done`, `phases.perspective.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.perspective.tmp/` → `<thread>.{N}.perspective/`. The final-named dir only ever exists in **complete** form.
11. **Report**: print the path to the (now-renamed) perspective sibling and a one-line status (e.g., `Perspective acme-seed.0.perspective/ (14 candidates across 5 clusters, 3 gaps surfaced, 2 stubs filled, workflow=hybrid)`).

## Failure modes

This role's primary failure modes (all of which the no-fabrication rule and the procedure above are designed to prevent):

- **Hallucinated comparable**: agent names a comparable transaction not in refs and not fetched from a verifiable URL. The role MUST refuse and surface the area as a gap. Caught downstream by `memo-review` dim 3 (Evidence quality) if it slips through (every load-bearing quantitative claim must trace to a brief, a refs/ source, or carry a hedge per the citation-hook contract).
- **Hallucinated funding round**: agent attaches a fabricated round size / lead / date to a real comparable name. Same rule: every numeric field MUST cite a source URL or refs file. A round with no source is dropped from `candidates.md` and the gap surfaced in `notes.md`.
- **Stale URL**: agent cites a URL that 404s or was edited after the date of citation. The role records the verification date in the `Source` field (e.g., `verified 2026-05-30`); a re-run can refresh stale entries. Operators concerned about source decay should archive critical URLs to `<thread>/refs/` (e.g., via `wget --page-requisites` or a screenshot PDF).
- **Hidden top-down market sizing**: a market-report PDF in refs uses top-down methodology without disclosure; the role re-formats it without flagging the methodology. The role MUST note "top-down sizing" or "bottom-up sizing" or "methodology unstated" on each market-report candidate so downstream reviewer / cross-check critics (memo-review dim 5 Market & competitive framing) can apply the standard "top-down-only sizing is a near-automatic disqualifier" rule.
- **Refs-vs-brief contradiction silently passed through**: the brief claims a $8B TAM but the only ref-supported market report sizes the segment at $1.2B. The role MUST surface this contradiction in `notes.md` "Identified gaps" — not silently propagate either number. The drafter sees the gap and either updates the memo or asks the operator to clarify.
- **Stub-fill fabrication**: the role fills a citation stub with a fabricated source URL because the stub names a claim the role found compelling but for which it has no actual substrate. The "Side-effect: filling refs/ citation stubs" section forbids this — stubs without resolved substrate remain TODO stubs; the role does NOT invent a source.

## Re-run pattern

This command follows the framework re-run pattern documented in `anvil/lib/snippets/perspective.md` §"Re-run pattern":

- Initial perspective lives at `<thread>.0.perspective/` (pre-draft, before `<thread>.1/` exists).
- A reviewer flagging a substrate gap on `<thread>.{N}/` triggers `memo-revise` to invoke `memo-perspective` again, producing `<thread>.{N}.perspective/`. Downstream consumers (next drafter pass via `memo-revise`) read the **latest** perspective sibling — they do not aggregate across versions.
- The previous sibling at `<thread>.0.perspective/` is preserved on disk for audit trail; nothing deletes it. The auditor can compare across siblings to track substrate evolution.
- A re-run perspective sibling MUST include a delta paragraph in `notes.md` naming what changed: which review comments / auditor findings drove the re-run, which gaps were closed by new substrate (operator added refs, agent fetched a missing comparable), which remain open. Mirrors `deck-perspective.md`'s and `paper-litsearch.md`'s re-run discipline.

## Idempotence and resumability

- A completed perspective (`perspective.state == done` AND `notes.md` + `candidates.md` exist) is never re-run automatically. Re-invoking the same target sibling is a no-op with a notice. To produce a new perspective at the next version, the caller (typically `memo-revise`) requests `<thread>.{N+1}.perspective/` explicitly.
- A crashed perspective (`perspective.state == in_progress` with partial output) is re-runnable after deleting any partial output in the target sibling dir. Validation is by file existence (does `notes.md` exist? does `candidates.md` exist?), not solely by the progress flag — consistent with the snippet's §"Idempotence and resumability".

## State-machine non-gating

**Absence of a perspective sibling does NOT block the state machine.** A memo thread with no `<thread>.0.perspective/` drafts, reviews, and revises normally; the drafter consults perspective context when present and proceeds without it when absent. Per `perspective.md` §"State-machine non-gating", perspective is opt-in input, not required output. The memo-skill lifecycle (`draft → review → revise → figures`) MUST NOT list `perspective` as a required phase; `memo-perspective` is non-gating by design.

This is the safety property that lets `memo-perspective` ship incrementally: existing memo threads (pre-#179) have no perspective sibling and continue to draft unchanged. New threads that opt in to the perspective workflow get the benefit; threads that don't pay no cost. This is the same backwards-compat property that PR #157 (`deck-perspective`) preserved for the deck skill.

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

- **The brief and refs are the contract.** When in doubt, refuse to fill. A named gap is a feature; a fabricated comparable or invented funding round is a critical-flag risk that propagates downstream to `memo-review` dim 3 (Evidence quality).
- **Source URLs in every row.** No anonymous claims. If you can't show where a fact came from, it doesn't belong in `candidates.md`.
- **Cluster substrate, don't dump.** A flat list of 30 random "comparables" is less useful to the drafter than 6 verified direct comparables + 4 cited research items + 3 customer-evidence pointers + 2 market reports + 2 regulatory-context entries. Cluster by intended memo consumer (market & competitive framing / evidence / risks / financial reasoning).
- **Surface contradictions explicitly.** If refs say one thing and the brief says another, the role's job is to NAME the contradiction in `notes.md` "Identified gaps", not to pick a side silently. The drafter (or operator) resolves.
- **Fill stubs only when you have substrate.** A stub the role can resolve from refs or a verifiable URL is a productive side effect; a stub the role cannot resolve MUST stay as TODO — never paper over a stub with invented substrate.

**Snippet references**: See `anvil/lib/snippets/perspective.md` for the framework contract (layout, no-fabrication rule, three consumer classes, re-run pattern, subprocess-only-by-default posture). See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. See `anvil/lib/snippets/scorecard_kind.md` for the `human-verdict` vs `machine-summary` discriminator. See `anvil/skills/deck/commands/deck-perspective.md` for the load-bearing precedent this command mirrors, and `anvil/skills/paper/commands/paper-litsearch.md` for the original skill-local pattern the perspective primitive generalizes.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.0.perspective/` (or `<thread>.{N}.perspective/` on re-run) — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own perspective sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(memo/perspective): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since perspective is non-gating and does not advance the state machine.
