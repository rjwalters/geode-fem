---
name: proposal-perspective
description: Pre-draft (or re-run) external-substrate critic for the proposal skill. Read-only. Gathers comparable-project research, vendor-quote substrate, and regulatory/permitting context the drafter or reviser consumes as load-bearing pricing / scoping / deliverability context. Refuses to fabricate candidates.
---

# proposal-perspective — External-substrate critic (perspective sibling)

**Role**: perspective critic (sibling, read-only).
**Reads**: `<thread>/BRIEF.md`, `<thread>/refs/` (any operator-supplied source material: vendor quotes, datasheets, planning-range references, comparable-project case files, prior-project SOWs, site plans, permit / regulatory filings, CVs of named delivery leads, prior proposals migrating in). For a re-run after a reviewer flags missing substrate: also the latest `<thread>.{N}/proposal.tex` and any `<thread>.{N}.review/comments.md` / `<thread>.{N}.audit/findings.md` entries tagged as cost-sourceability, scope, deliverability, or regulatory concerns.
**Writes**: `<thread>/<thread>.0.perspective/` (initial, pre-draft) or `<thread>/<thread>.{N}.perspective/` (re-run after revision `N`) — the perspective sibling is nested under the thread root per the artifact contract. Bare `<thread>.{N}/` / `<thread>.{N}.perspective/` references below are shorthand for these nested paths.

This command is the optional pre-draft step described in `SKILL.md` for the proposal skill. It is a **sibling critic**, not a phase that gates the state machine. The drafter consumes the initial perspective; the reviser consumes any re-run perspective alongside `.review/` and `.audit/`. The framework contract for the perspective shape lives in `anvil/lib/snippets/perspective.md` (in an installed consumer repo: `.anvil/anvil/lib/snippets/perspective.md`); this command is the proposal-skill instantiation of that contract.

`proposal-perspective` mirrors the Phase 1B `deck-perspective` shape (PR #157, the load-bearing prior precedent for cross-skill perspective rollout) and `anvil/skills/paper/commands/paper-litsearch.md` (the original load-bearing skill-local precedent the perspective primitive generalized). It is tuned for buildable-system proposal substrate: **comparable-project case research, vendor-quote pricing substrate, regulatory / permitting context, and named-lead deliverability evidence** — NOT academic literature or pitch-deck market positioning.

## Why this is a separate role

Folding external-substrate gathering into the drafter conflates two distinct failure modes:

- The drafter may write good prose around unverified vendor prices / unsourced comparable-project claims / unattested regulatory context.
- The drafter may write bad prose around verified vendor quotes / well-sourced comparables / accurate regulatory cites.

Separating perspective lets each role do one job. It also lets the reviser **re-run** perspective when the reviewer (or `proposal-audit`'s extended sourceability walk) points out a gap in pricing substrate or a missing comparable, without re-drafting the proposal — the next revision picks up the new perspective sibling and updates the affected sections (BOM, comparables-backed deliverability claims, regulatory references) specifically.

The architectural choice of "perspective" over "research" disambiguates from `anvil:paper`'s "research papers" domain and from consumer-local research directories some adopters maintain (per #117 / `perspective.md` §"Naming: perspective, not research"). For proposal the substrate is *external sourceability* — the pricing, regulatory, and deliverability bedrock the proposal's claims rest on — not academic literature.

## Critical constraint: do not invent candidates

Pure-LLM substrate gathering hallucinates vendor prices, fabricates comparable projects, and invents regulatory references. This role MUST NOT invent entries from training-data recall. Every entry in `candidates.md` MUST carry a **source pointer** (per `anvil/lib/snippets/perspective.md` §"No-fabrication rule"):

- A **URL** (vendor product page or list-price page, supplier catalog entry, regulatory filing or permit code online, public RFP record, prior-project case study on a contractor's website, supplier news / press release).
- A **citation pointer** to a known artifact (a `.pdf` filename in `<thread>/refs/`, a vendor quote number, a permit / code section number, a project case-file title).
- A **pointer to operator-supplied material on disk** (`<thread>/refs/<file>`, `<thread>/BRIEF.md` content, contractor-supplied notes, prior-thread case file).

The only entries allowed in `candidates.md` are:

1. Entries derived from `<thread>/refs/` source material the operator explicitly supplied (a vendor quote PDF, a datasheet, a prior-project case file, a permit code excerpt, a CV of a named delivery lead).
2. Entries the brief explicitly mentions (e.g., the brief lists "Prior project: Palazzo Roselli LAN, 2024-Q3, 8 spokes, $14k" — copy/format, do not autocomplete missing details).
3. Entries the agent fetched live from a URL it can show, when the caller's environment provides web access (per `perspective.md` §"Subprocess-only by default — no mandated fetcher"). Every fetched entry MUST carry its source URL in the candidate row.

If the brief or the reviewer's comments name a substrate area but no source material exists on disk and no fetcher is available, the role surfaces the gap in `notes.md` for the operator to fill manually (e.g., by emailing a vendor for a fresh quote, dropping the comparable's case file into `<thread>/refs/`, or pasting the relevant permit code into the brief). The role does NOT invent a plausible-looking "Acme Fiber Conduit, $18/m list price, in-stock" entry to close the gap.

This rule is the proposal-skill restatement of `paper-litsearch.md`'s "Critical constraint: do not invent citations" — both inherit the no-fabrication discipline from the perspective framework primitive.

## Subprocess-only by default — operator brings the fetcher

Anvil does **not** mandate a sourcing fetcher (per `perspective.md` §"Subprocess-only by default — no mandated fetcher"). The perspective shape is a **convention**, not a runtime. The agent invoking `proposal-perspective` brings its own web access; the framework specifies the on-disk shape and the no-fabrication rule.

Operator workflows the command supports, in order of typical use:

- **Pre-staged (most common for buildable-system proposals)**: the operator (or the project lead) drops material into `<thread>/refs/`: vendor quote PDFs, supplier datasheets, prior-project case files, permit-code excerpts, CVs of named delivery leads, prior anvil:proposal threads migrating in. The perspective command re-formats from the pre-staged sources only. This is the dominant workflow because buildable-system pricing substrate is usually quote-driven (multi-vendor quotes against a specific BOM); the operator already has the quotes in hand by the time drafting begins.
- **Agent-driven (when web access is available)**: the orchestrator invokes `proposal-perspective` with an agent that has `WebFetch` (or equivalent). The agent populates `notes.md` and `candidates.md` from live web sources — vendor list-price pages, public permit codes, regulatory filings, contractor case-study pages. Every fetched entry carries its source URL. Useful for the "fill the gap the operator didn't pre-stage" case (e.g., a planning-range price for a component the operator hasn't quoted yet).
- **Hybrid**: operator pre-stages high-confidence material (verified vendor quotes, project-lead CVs, site plans); the agent web-fetches to fill specific gaps the operator names (e.g., "find the most recent published list price for SFP+ LR transceivers in the 1310nm wavelength"). The agent's fetches are still bounded by the no-fabrication rule.

## Inputs

- **Thread slug** (positional argument).
- **Brief** (`<thread>/BRIEF.md`): freeform prose with optional YAML frontmatter. Recognized frontmatter keys include `customer_kind` (`external`/`internal`), `stage`, `signature_color`. Unrecognized keys are passed through as context. The brief's "Premise" (hard constraints), "BOM" / "Cost" sections (named components), "Deliverability" (named leads, prior projects), and "Compliance / References" sections (regulatory regime) are the load-bearing inputs — they name vendors, comparables, and regulatory regimes the perspective sibling can verify and gaps the sibling can flag.
- **Reference material** (`<thread>/refs/**`): any supporting material the operator has supplied. Treated as read-only context. Vendor quotes, datasheets, site plans, comparable-project case files, permit-code excerpts, CVs of named leads, prior proposals are all in scope. The proposal SKILL.md §"Source-of-truth materials" enumerates the canonical filename conventions (`quote-<vendor>.{pdf,md}`, `datasheet-<part>.pdf`, `sow-*.md`, `comparables/<project>.md`, `cv-<lead>.{pdf,md}`, `site-plan-*.pdf`, `prior/<vN>.{pdf,md}`); the perspective sibling consults these as substrate.
- **Re-run context** (re-run path only): the latest `<thread>.{N}/proposal.tex` (to understand current positioning), `<thread>.{N}.review/comments.md` entries tagged `scope` / `deliverability` / `comparables` / `compliance`, and any `<thread>.{N}.audit/findings.md` blockers naming missing or unverifiable substrate (e.g., `NOT-IN-REFS` entries from the audit's extended sourceability walk).

## Outputs

Nested under the thread root `<thread>/`:

```
<thread>.0.perspective/                (initial; or <thread>.{N}.perspective/ for re-runs)
  notes.md             Narrative synthesis: what the comparable / vendor / regulatory substrate says + gaps surfaced
  candidates.md        Structured candidate list (comparable projects / vendor quotes / regulatory & compliance / named leads) with source URLs / refs pointers
  _meta.json           { critic, role, started, finished, model, scorecard_kind: human-verdict, search_params }
  _progress.json       Phase state (phase: perspective; for_version: N)
```

**Atomicity** (issue #350, #376): the perspective sibling dir is written **atomically** via the staged-sidecar primitive at `anvil/lib/sidecar.py`. The four files (`notes.md`, `candidates.md`, `_meta.json`, `_progress.json`) are staged under a leading-dot sibling `.<thread>.{N}.perspective.tmp/` during writing; on clean completion the staging dir is renamed (one atomic `Path.rename`) to the final `<thread>.{N}.perspective/` name. A mid-cycle interrupt leaves a `.<thread>.{N}.perspective.tmp/` dir on disk that the next invocation's `cleanup_one_staging(<thread>.{N}.perspective)` per-critic sweep removes; the final-named dir never exists in partial form. Discovery (`anvil/lib/critics.py::discover_critics`) is unchanged — the leading-dot staging shape is invisible to the discovery glob.

### `notes.md` structure

- **Sourceability summary** (3–5 paragraphs): how the proposal's load-bearing claims trace to the supplied substrate. For each cluster (comparable projects, vendor quotes / pricing, regulatory & compliance, deliverability evidence), name the cluster, identify the closest 1–3 entries (with anchor references back to `candidates.md`, e.g., `[#palazzo-roselli-2024]`), and state how the proposal extends / contradicts / complements the substrate. Examples: "The proposal claims `$14k--$18k` for a comparable spoke-count fiber install — `candidates.md#palazzo-roselli-2024` (prior thread case file) shows the comparable closest-to-spec ran $13.9k actual; the BOM's planning range is supported by the substrate." / "The proposal cites OSHA 1910 conduit-routing requirements on §6 (Coverage / Capacity) — `candidates.md#osha-1910-permit-code` (web source) confirms the citation is current; no contradiction." / "§7 BOM lists `Acme SFP+ LR, qty 16, $15--20` — `candidates.md#acme-sfp-lr-quote` (vendor quote in refs/) shows the per-unit quote is $17.50; the planning range brackets the quoted figure correctly."
- **Confirmed coverage**: bullet list of substrate areas the supplied references cover adequately (e.g., "vendor quotes: 4 of 6 BOM line-items have direct quotes in refs/; 2 have planning-range list-price URLs", "comparables: 3 prior-project case files with disclosed cost outcomes in same scope band", "regulatory: OSHA conduit-routing + ITU-T G.652D fiber spec sheets in refs/").
- **Identified gaps**: bullet list of areas where the brief or the drafter would clearly benefit from additional substrate but none was supplied or fetched. Each gap names the area precisely enough that the operator can search ("planning-range list price for OM4 multimode fiber, 1km run, 2025"; "permit-code excerpt for fiber-conduit routing in historic-district properties — the proposal cites local code but no copy is in refs/"; "named lead deliverability: §4 claims `in-house fiber-splicing capability` but no CV / cert is in refs/cv-*"). **The role does not invent placeholder entries to fill these gaps** — it names the gap and stops.
- **Re-run delta** (re-run path only): a short paragraph naming what changed since the previous perspective sibling — which review comments / audit findings drove the re-run, which gaps were closed by new substrate, which remain open. Mirrors `paper-litsearch.md`'s "re-run delta" convention.

### `candidates.md`

A markdown document with one section per substrate cluster (recommended: Comparable projects / Vendor quotes & pricing / Regulatory & compliance / Deliverability evidence). Each entry is a small markdown subsection with a stable anchor and a structured body:

```markdown
### Comparable projects

#### `palazzo-roselli-2024` — Palazzo Roselli LAN (prior thread)
- **Scope**: 8-spoke fiber LAN, ground-floor + piano nobile, no visible conduit
- **Outcome**: delivered 2024-Q3, $13,900 final cost (materials $8,400 + labor $5,500)
- **Source**: `refs/comparables/palazzo-roselli-2024.md` (prior-thread case file, operator-supplied)
- **Relevance**: closest-to-spec comparable; proposal §7 BOM planning range `$14k--$18k` is supported by this outcome

#### `villa-conti-2023` — Villa Conti CAT6A retrofit
- **Scope**: 12-room CAT6A retrofit, partial conduit, non-fiber
- **Outcome**: $22,000 (materials $14,000 + labor $8,000), delivered 2023-Q2
- **Source**: `refs/comparables/villa-conti-2023.md` (operator-supplied)
- **Relevance**: scope-adjacent (copper not fiber) but informs labor estimate methodology — labor ran 38 hours for 12-room CAT6A; proposal §7 estimates 32 hours for 7-spoke fiber, consistent on hours-per-room basis

### Vendor quotes & pricing

#### `acme-sfp-lr-quote` — Acme SFP+ LR transceiver
- **Vendor**: Acme Networks (Q-2026-04-12)
- **Part**: SFP+ LR 10G 1310nm, MMF/SMF dual-mode
- **Quote**: $17.50/unit, qty 16+, lead time 2 weeks
- **Source**: `refs/vendor-quotes/acme-sfp-lr-q2026-04-12.pdf` (operator-supplied quote PDF)
- **Relevance**: backs §7 BOM line "SFP+ LR $15--20, qty 16" — quote brackets the planning range

#### `gossamer-fiber-list-price` — Gossamer OM3 multimode fiber
- **Vendor**: Gossamer Fiber (public list price)
- **Part**: OM3 multimode duplex armored fiber, 1m increments
- **List price**: $4.20/m for >100m bulk
- **Source**: https://gossamer.example/products/om3-armored (verified 2026-05-30)
- **Relevance**: planning-range basis for §7 BOM fiber-by-the-meter line; no quote in refs/ but list-price URL covers sourceability

### Regulatory & compliance

#### `itu-t-g-652d-fiber-spec` — ITU-T G.652D fiber specification
- **Source**: `refs/standards/itu-t-g-652d-2009.pdf` (operator-supplied spec sheet)
- **Cite location**: proposal §9 References / Compliance
- **Relevance**: backs the proposal's claim that the chosen SMF complies with ITU-T G.652D; the spec sheet is the authoritative source

#### `osha-1910-conduit-routing` — OSHA 1910 conduit routing requirements
- **Source**: https://www.osha.gov/laws-regs/regulations/standardnumber/1910 (verified 2026-05-30)
- **Cite location**: proposal §6 Coverage / Capacity
- **Relevance**: the proposal cites OSHA 1910 for conduit-routing constraints — the URL points to the canonical reg; no contradiction

### Deliverability evidence

#### `lead-electrician-cv` — Project electrician (named lead)
- **Source**: `refs/cv-lead-electrician.md` (operator-supplied CV)
- **Cite location**: proposal §4 The Core Subsystem (delivery-capability subsection)
- **Relevance**: backs the proposal's claim that fiber-splicing is performed in-house — CV lists Fluke OptiFiber Pro certification and 8 years of fiber-splicing experience

#### `splicer-tool-receipt` — Owned splicer (capability evidence)
- **Source**: `refs/splicer-purchase-2024.pdf` (operator-supplied receipt for owned Fujikura 70S+)
- **Cite location**: proposal §4 (delivery-capability subsection)
- **Relevance**: backs the "we own the splicer" claim — the receipt is presence-evidence (the splicer exists on the operator's bench)
```

Markdown (not BibTeX) is the right format for proposal substrate: buildable-system consumers cite vendor quote numbers, regulatory code sections, comparable-project case-file titles, and CV credentials — not `\bibitem`-shaped citations. The structured anchor (`#palazzo-roselli-2024`) lets `notes.md` reference candidates by stable id; the drafter, the reviewer, and `proposal-audit`'s extended sourceability walk all resolve those anchors to verify the proposal text against the substrate.

The drafter and reviser are free to cite entries from `candidates.md` on relevant sections (e.g., §7 BOM lines pull from the Vendor quotes section; the delivery-capability subsection in §4 cites the Deliverability evidence entries; §9 References / Compliance pulls from Regulatory & compliance). Entries the drafter does not cite remain in `candidates.md` only and do not pollute the proposal.

### `_meta.json`

```json
{
  "critic": "perspective",
  "role": "proposal-perspective.md",
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

`scorecard_kind: human-verdict` is the correct primary kind per `anvil/lib/snippets/scorecard_kind.md`: the drafter reads `notes.md` as a narrative, not as a per-dimension partial scorecard. The perspective sibling does NOT emit `_summary.md` + `findings.md` — those are `machine-summary` artifacts produced by `proposal-audit` (which CONSUMES this perspective sibling in its extended sourceability walk per issue #180 / Phase 2B contract).

`search_params` documents the workflow and the substrate the role actually consumed, so the auditor can reproduce / spot-check.

## Procedure

1. **Discover state**: enumerate `<thread>.*.perspective/` siblings under the thread root `<thread>/`. If invoked without explicit version context, default to creating `<thread>.0.perspective/` (the pre-draft sibling). If the latest version dir is `<thread>.{N}/` and the caller requested a re-run (e.g., `proposal-revise` triggered re-run because a sourceability gap was flagged), create `<thread>.{N}.perspective/`. Then **sweep a stale staging dir from a prior interrupt of THIS critic on THIS version** by invoking `anvil/lib/sidecar.py::cleanup_one_staging(<thread>.{N}.perspective)` (the per-critic, parallel-safe sweep — issue #376). This removes ONLY a leftover `.<thread>.{N}.perspective.tmp/` from a previously-killed run of this same critic on THIS version. Sibling critics' in-flight staging dirs under the same thread root are NOT touched (issue #350, #376).
2. **Resume check**: per the staged-sidecar shape introduced in issue #350, a completed perspective sibling means the final-named `<thread>.{N}.perspective/` dir exists — the atomic-rename contract guarantees the dir only exists when complete. If `<thread>.{N}.perspective/` exists, exit early — the sibling is complete (idempotent per `perspective.md` §"Idempotence and resumability"). A partial perspective left behind by a mid-cycle interrupt manifests as a leading-dot `.<thread>.{N}.perspective.tmp/` directory; the step 1 sweep has already removed it.
3. **Open the staged sidecar** for the perspective dir by invoking the context manager `anvil/lib/sidecar.py::staged_sidecar(final_dir=<thread>.{N}.perspective, required_files=["notes.md", "candidates.md", "_meta.json", "_progress.json"])`. Every file write below MUST land **inside the yielded staging directory** (the path of the shape `.<thread>.{N}.perspective.tmp/`), NOT inside the final `<thread>.{N}.perspective/` path. On clean context exit, the primitive verifies the manifest, then atomically renames the staging dir to its final name (issue #350). Then, **inside the staging dir**, initialize `_progress.json`: `phases.perspective.state = in_progress`, `phases.perspective.started = <ISO>`, `for_version = N` (0 for the pre-draft sibling).

   **Non-Python-driver ordering (fail-open, manual fallback)** — issue #645: `staged_sidecar` is a Python context manager. A manual/agent session with **no orchestrating Python driver** cannot hold its `with` block open across the file writes below (it writes files with its own editing tool between discrete steps), so it MUST use the equivalent CLI shim rather than writing straight into the final `<thread>.{N}.perspective/` dir (which silently reopens the #350 partial-write defect this primitive exists to close). Two tiers, in preference order:

   1. **Primary — `python -m anvil.lib.sidecar` CLI shim** (the common case). In an installed consumer repo (anvil vendored under `.anvil/`, not on `sys.path`), prefix every invocation below with `uv run --project .anvil` (the `.anvil/pyproject.toml` + `uv sync --project .anvil` shipped by the installer since #230 make the module resolvable from the consumer root — the same shape the other `anvil.lib.*` critics already use); in the anvil source repo the bare `python -m anvil.lib.sidecar` form works as-is. This wraps the *exact same* `staged_sidecar` code, so the manifest check + single atomic `Path.rename` are enforced by code, not agent discipline:
      - `uv run --project .anvil python -m anvil.lib.sidecar stage <thread>.{N}.perspective` → prints the staging path (`.<thread>.{N}.perspective.tmp/`). (Refuses with a nonzero exit if `<thread>.{N}.perspective/` already exists — matching `staged_sidecar`'s `FileExistsError` refuse-to-overwrite guard.)
      - Write **all** required files (`notes.md`, `candidates.md`, `_meta.json`, `_progress.json`) into that printed staging path — never into the final `<thread>.{N}.perspective/` name.
      - `uv run --project .anvil python -m anvil.lib.sidecar commit <thread>.{N}.perspective --required notes.md,candidates.md,_meta.json,_progress.json` → verifies the manifest, then atomically renames staging → final. **Nonzero exit (1) leaves the staging dir in place with no partial final dir** if any required file is missing — the `SidecarIncompleteError` analog; fix the gap and re-`commit`.
      - The stale-staging sweep of step 1 has an exact CLI analog: `uv run --project .anvil python -m anvil.lib.sidecar cleanup <thread>.{N}.perspective` (the parallel-safe per-critic sweep, issue #376).
   2. **Last resort — manual `mv`-based staging** when even `python`/`uv` is unavailable. Reproduce the staging contract by hand: (a) at entry, sweep any leftover `rm -rf .<thread>.{N}.perspective.tmp/` (the `cleanup_one_staging` analog); (b) `mkdir .<thread>.{N}.perspective.tmp/` and write **every** required file into it — writing `_progress.json` **last**, so a mid-write interrupt is caught by the missing-manifest check rather than producing a final-named partial; (c) confirm the staging dir holds the full required set — use a count check (`[ "$(ls -1 .<staging-dir> | wc -l)" -eq <N> ]`) or an `ls`-based presence check rather than per-file `[ -f ]`, which can false-negative under a restricted-`stat` sandbox — **then** `mv .<thread>.{N}.perspective.tmp <thread>.{N}.perspective` as the **last** step (POSIX `mv` on a same-filesystem dir-to-dir rename is atomic, matching `Path.rename`). Do NOT create `<thread>.{N}.perspective/` before all files are staged. **Record the fallback durably** so a reader can tell atomicity was reproduced by hand rather than tool-verified: stamp `_meta.json` with `"atomicity_fallback": "manual-mv"` (e.g. `sidecar: staged_sidecar CLI unavailable (uv/python not on PATH); atomicity reproduced via manual mv this pass`). Absent this note the manual staging is indistinguishable from an unsafe direct write.

   The two tiers land a byte-identical on-disk result to the `staged_sidecar` context-manager path; they exist only to give a Python-less session a code-enforced (tier 1) or contract-faithful (tier 2) route to the same atomicity guarantee. When an orchestrating Python driver IS present, use `staged_sidecar` directly as documented above — the CLI shim is not needed.

4. **Read inputs**: load `BRIEF.md`, enumerate `<thread>/refs/`, classify each ref by SKILL.md source-of-truth convention (vendor quote, datasheet, SOW, comparable case file, CV, site plan, permit / regulatory excerpt, prior-proposal version). On re-run, also load `<thread>.{N}/proposal.tex` and the latest `.review/comments.md` + `.audit/findings.md`.
5. **Choose workflow**: per `_meta.json.search_params.workflow`. Default is **pre-staged** if `<thread>/refs/` is non-empty (the common case for buildable-system proposals); **agent-driven** if refs is empty but the agent has fetcher access; **hybrid** if both are available and the brief or re-run context names gaps the agent should fill. Refuse to run if the workflow is `agent-driven` but no fetcher is available — surface the missing-fetcher condition in stdout and exit non-zero with a clear message (the operator can rerun with pre-staged refs).
6. **Build `candidates.md`**: re-format pre-staged entries; if agent-driven or hybrid, fetch only with source pointers attached. Do not invent. If the brief lists a vendor by name only without quote details, leave a `% TODO: needs quote number / price / lead time — operator follow-up` comment per missing field and surface in `notes.md` gaps. Cluster the candidates by substrate area; assign stable anchor ids (`#<vendor>-<part>` or `#<project>-<year>`) so `notes.md` and the audit's extended sourceability walk can reference them.
7. **Write `notes.md`**: sourceability summary (cross-referencing candidates by anchor) + confirmed coverage + identified gaps + (re-run only) delta paragraph. Each claim about the substrate MUST be backed by a candidate entry or by a direct quote from a ref file; vague handwave language ("vendors typically charge $X for Y") is not allowed without a `candidates.md` entry to back it.
8. **Write `_meta.json`**: populate `critic`, `role`, `started`, `finished`, `model`, `scorecard_kind: human-verdict`, and `search_params` (workflow + refs_consumed + urls_fetched + candidate_count + gap_count).
9. **Update `_progress.json`** inside the staging dir: `phases.perspective.state = done`, `phases.perspective.completed = <ISO>`. This is the LAST file write before the context manager exits — the manifest verification + atomic rename at exit (issue #350) requires `_progress.json` to be present. Then **exit the `staged_sidecar` context block**: the primitive verifies every name in the required-files manifest exists in the staging dir, then atomically renames `.<thread>.{N}.perspective.tmp/` → `<thread>.{N}.perspective/`. The final-named dir only ever exists in **complete** form.
10. **Report**: print the path to the (now-renamed) perspective sibling and a one-line status (e.g., `Perspective gossamer-lan.0.perspective/ (14 candidates across 4 clusters, 3 gaps surfaced, workflow=hybrid)`).

## Failure modes

This role's primary failure modes (all of which the no-fabrication rule and the procedure above are designed to prevent):

- **Hallucinated vendor quote**: agent attaches a fabricated unit price / lead time / quote number to a real vendor name. The role MUST refuse and surface the area as a gap. Caught downstream by `proposal-audit`'s extended sourceability walk if it slips through (every priced line must trace to a refs/ quote, a list-price URL, or a stated planning range).
- **Hallucinated comparable project**: agent names a prior install / case study not in refs and not fetched from a verifiable URL. Same rule: every comparable entry MUST cite a source URL or refs file. A comparable with no source is dropped from `candidates.md` and the gap surfaced in `notes.md`.
- **Stale URL**: agent cites a vendor list-price URL that 404s or was edited after the date of citation (vendor pages change frequently). The role records the verification date in the `Source` field (e.g., `verified 2026-05-30`); a re-run can refresh stale entries. Operators concerned about source decay should archive critical URLs to `<thread>/refs/` (e.g., via `wget --page-requisites` or a screenshot PDF).
- **Refs-vs-brief contradiction silently passed through**: the brief claims `Acme SFP+ LR $20/unit` but the only ref-supported quote shows `$17.50/unit`. The role MUST surface this contradiction in `notes.md` "Identified gaps" — not silently propagate either number. The drafter (or auditor) sees the gap and either updates the proposal or flags the conflict in Open Decisions. This becomes a `CONTRADICTED` finding in `proposal-audit`'s extended sourceability walk per `commands/proposal-audit.md` step 7.
- **Out-of-date regulatory citation**: agent cites an OSHA / ITU-T / local-code reference from training-data recall without verifying against a current URL. The role MUST fetch / cite a current source or refuse the entry. Regulatory citations that are out-of-date are a real liability for a buildable-system proposal.

## Re-run pattern

This command follows the framework re-run pattern documented in `anvil/lib/snippets/perspective.md` §"Re-run pattern":

- Initial perspective lives at `<thread>.0.perspective/` (pre-draft, before `<thread>.1/` exists).
- A reviewer (or `proposal-audit`'s extended sourceability walk) flagging a substrate gap on `<thread>.{N}/` triggers `proposal-revise` to invoke `proposal-perspective` again, producing `<thread>.{N}.perspective/`. Downstream consumers (next drafter pass via `proposal-revise`, next `proposal-audit` extended sourceability walk) read the **latest** perspective sibling — they do not aggregate across versions.
- The previous sibling at `<thread>.0.perspective/` is preserved on disk for audit trail; nothing deletes it. The auditor can compare across siblings to track substrate evolution (e.g., a `NOT-IN-REFS` claim from v1's audit becomes a `VERIFIED` claim once the re-run perspective adds the source).
- A re-run perspective sibling MUST include a delta paragraph in `notes.md` naming what changed: which review comments / audit findings drove the re-run, which gaps were closed by new substrate (operator added a vendor quote, agent fetched a missing comparable case file), which remain open. Mirrors `paper-litsearch.md`'s re-run discipline.

## Idempotence and resumability

- A completed perspective (`perspective.state == done` AND `notes.md` + `candidates.md` exist) is never re-run automatically. Re-invoking the same target sibling is a no-op with a notice. To produce a new perspective at the next version, the caller (typically `proposal-revise`) requests `<thread>.{N+1}.perspective/` explicitly.
- A crashed perspective (`perspective.state == in_progress` with partial output) is re-runnable after deleting any partial output in the target sibling dir. Validation is by file existence (does `notes.md` exist? does `candidates.md` exist?), not solely by the progress flag — consistent with the snippet's §"Idempotence and resumability".

## State-machine non-gating

**Absence of a perspective sibling does NOT block the state machine.** A proposal thread with no `<thread>.0.perspective/` drafts, reviews, audits, and revises normally; the drafter consults perspective context when present and proceeds without it when absent. Per `perspective.md` §"State-machine non-gating", perspective is opt-in input, not required output. The proposal-skill required critic set (`review + audit`, both REQUIRED per `SKILL.md`) MUST NOT list `perspective` as required; `proposal-perspective` is non-gating by design.

This is the safety property that lets `proposal-perspective` ship incrementally: existing proposal threads (pre-#180) have no perspective sibling and continue to draft / review / audit / revise unchanged. New threads that opt in to the perspective workflow get the benefit (better-sourced BOMs, traceable comparables, attested deliverability claims); threads that don't pay no cost.

## `_progress.json` snippet

This command writes the critic-sibling shape documented in `anvil/lib/snippets/progress.md`. Perspective siblings live at `<thread>.0.perspective/` (pre-draft) or `<thread>.{N}.perspective/` (re-run after reviewer / audit feedback):

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

Perspective outputs (`notes.md` + `candidates.md`) are read narratively by the drafter and traversed per-anchor by `proposal-audit`'s extended sourceability walk; the sibling declares `scorecard_kind: human-verdict` in `_meta.json` per `anvil/lib/snippets/scorecard_kind.md`.

## Notes for the perspective agent

- **The brief and refs are the contract.** When in doubt, refuse to fill. A named gap is a feature; a fabricated vendor quote or invented comparable project is a critical-flag risk that propagates downstream to `proposal-audit` (and may escalate to flag 2 *cost not credible/sourceable* or flag 4 *internal inconsistency* per the SKILL.md §"Source-of-truth materials" CONTRADICTED escalation path).
- **Source URLs in every row.** No anonymous prices. If you can't show where a quote / list price / case-study figure came from, it doesn't belong in `candidates.md`.
- **Cluster substrate, don't dump.** A flat list of 30 random "comparable installs" is less useful to the drafter than 4 closest-to-spec comparables + 6 verified vendor quotes against the BOM + 3 regulatory citations + 2 named-lead CVs. Cluster by intended proposal consumer (§7 BOM / §4 delivery-capability subsection / §9 References / Compliance / §4 deliverability evidence).
- **Surface contradictions explicitly.** If refs say one thing and the brief says another (e.g., the vendor quote in refs is $17.50/unit but the brief lists $20/unit), the role's job is to NAME the contradiction in `notes.md` "Identified gaps", not to pick a side silently. The drafter (or operator) resolves; the auditor will pick it up as a `CONTRADICTED` finding if not resolved before audit.
- **Date your URL verifications.** Vendor pages and regulatory references change. A `verified 2026-05-30` annotation gives the auditor and the re-run path the timestamp needed to spot stale substrate.

**Snippet references**: See `anvil/lib/snippets/perspective.md` for the framework contract (layout, no-fabrication rule, three consumer classes, re-run pattern, subprocess-only-by-default posture). See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. See `anvil/lib/snippets/scorecard_kind.md` for the `human-verdict` vs `machine-summary` discriminator. See `anvil/skills/paper/commands/paper-litsearch.md` for the load-bearing original precedent the perspective primitive generalizes; see `anvil/skills/deck/commands/deck-perspective.md` for the Phase 1B canary-skill consumer that this command mirrors. See `anvil/skills/proposal/SKILL.md` §"Source-of-truth materials" for the filename conventions the perspective sibling consults when classifying refs.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after the staged-sidecar atomic rename (issue #350) lands the final-named `<thread>.0.perspective/` (or `<thread>.{N}.perspective/` on re-run) — so only complete sidecars are ever committed.
- **Staging target**: ONLY this command's own perspective sidecar (never sibling critics' dirs — the narrow scope keeps the hook safe under parallel critic fan-out).
- **Commit**: `anvil(proposal/perspective): <thread>.{N} [<state>]` — the bracket carries the thread's current derived state per SKILL.md §State machine, since perspective is non-gating and does not advance the state machine.
