---
name: ip-uspto-inventorship
description: Inventorship interview generator. Produces a per-independent-claim attribution matrix the human attorney countersigns. Run before first draft AND re-run before finalize once claims are stable. Opt-in --evidence mode mines repo git history into reduction-to-practice citations that pre-fill the matrix Notes column only.
---

# ip-uspto-inventorship — Inventorship interviewer

**Role**: inventorship interviewer.
**Reads**: `<thread>/BRIEF.md`. If a latest `<thread>.{N}/claims.tex` exists, also read it for per-claim attribution.
**Writes**: `<thread>/inventorship.md` — the inventorship matrix, with one row per independent claim and a column per named inventor.

**Why this matters**: 37 CFR 1.63 (the inventor's oath/declaration) requires correct inventorship. Mis-attributed inventorship is grounds for **unenforceability** of the issued patent — the issue can be raised during litigation and the patent invalidated. This is one of the highest-stakes correctness questions in the entire filing.

## Inputs

- **Thread slug** (positional argument).
- **`<thread>/BRIEF.md`**: required. Provides the named inventors and the inventive features.
- **`<thread>.{N}/claims.tex`** (optional): if a draft exists, the inventorship matrix attributes each independent claim's inventive concept(s) to named inventors. Without claims, the matrix attributes the inventive features from `BRIEF.md` §3.
- **`--evidence [<repo_path>]`** (optional flag, opt-in): additionally mine the git history of the implementation repository at `<repo_path>` (default: the git toplevel of the current working directory) into reduction-to-practice evidence artifacts, and pre-fill the matrix **Notes column only** with commit citations. See "Evidence mode" below. **Without this flag, behavior is byte-identical to the base command — no git access, no evidence artifacts.**
- **`--reseed`** (optional flag, only meaningful with `--evidence`): discard the cached `inventorship_map.json` and re-seed the element→paths map from scratch.

## Outputs

```
<thread>/
  inventorship.md   Inventorship interview prompts + attribution matrix + attestation block
  inventorship-evidence/        (--evidence mode only; thread-level, like the matrix)
    inventorship_map.json       Element/feature → repo-paths map (semi-manual seed; cached)
    evidence.jsonl              Append-only git evidence rows (reduction-to-practice citations)
```

The file has the following structure:

```markdown
---
thread: <slug>
inventors:
  - name: <Full Name>
    role: <e.g., "principal investigator", "lead engineer">
generated_against: BRIEF.md  # or "thread.3/claims.tex" once claims exist
generated_at: <ISO>
matrix_locked: false           # set to true once human attorney countersigns
---

# Inventorship matrix — <thread>

## Source basis

This matrix attributes inventive contribution either to:
- (A) **Inventive features** as enumerated in `BRIEF.md` §3 (used when no claims exist yet), OR
- (B) **Independent claims** as drafted in `<thread>.{N}/claims.tex` (used once claims are stable).

Current basis: <A or B with version reference>.

## Interview prompts (give these to each named inventor)

For each <feature | claim>, ask:

1. **Who conceived this <feature | claim limitation>?** (Conception = the formation in the mind of a definite and permanent idea of the complete and operative invention. The conceiver is an inventor.)
2. **Was this conceived in collaboration?** If yes, name every collaborator and describe each person's contribution to the conception.
3. **When was this first conceived?** (Date, even approximate.)
4. **Was conception communicated to anyone (orally, in writing, code commits) before reduction to practice?** Reduction to practice (a working implementation or constructive reduction via filing) is distinct from conception.
5. **Has anyone NOT named here contributed to the conception?** (Reduction to practice alone is NOT inventorship. Lab assistants who built but did not conceive are NOT inventors.)

## Matrix

| #  | Feature or claim                                                       | Inventor 1 | Inventor 2 | Inventor 3 | Notes |
|----|------------------------------------------------------------------------|------------|------------|------------|-------|
| F1 | <feature 1 from BRIEF §3, or claim 1 from claims.tex>                  | ●          |            |            |       |
| F2 | <feature 2, or claim N>                                                | ●          | ●          |            | Joint conception over a 2-week period |
| ...|                                                                        |            |            |            |       |

Mark `●` for each inventor who conceived (in whole or part) the feature or claim limitation.

## Attribution rules

- An inventor must conceive at least one limitation of at least one issued claim to qualify. If after the matrix is filled, a named inventor has no `●` against any claim, they should be **removed** from the inventor list. Conversely, if anyone is `●` who is NOT in the named inventor list, they must be **added** (37 CFR 1.48 covers correction post-filing, but the cleaner path is to fix before filing).
- Lab assistants, technicians, and engineers who built a working implementation without conceiving are NOT inventors. Include them in the spec acknowledgments if appropriate.
- A supervisor or PI who funded or directed the work but did not conceive is NOT an inventor.
- Joint conception requires actual collaboration on the inventive concept. Two people who independently arrived at the same idea are not joint inventors of that idea; only one can be the inventor of that limitation (the earlier in time, generally).

## Attestation block (for human attorney countersignature)

I have reviewed the matrix above and the underlying interviews. I confirm:

- [ ] All conceiving inventors are named.
- [ ] No non-conceiving contributors are named.
- [ ] The matrix is consistent with the current claim set (or, if drafted pre-claims, the inventive features in `BRIEF.md` §3).
- [ ] Each named inventor has separately agreed to sign the 37 CFR 1.63 declaration.

Attorney signature: ___________________________  Date: ___________
```

## Procedure

1. **Discover state**: check whether `<thread>/inventorship.md` already exists.
   - If yes AND `matrix_locked: true` in frontmatter AND it was generated against the same basis (BRIEF.md or the same `claims.tex` version), exit early with a notice (idempotent).
   - If yes AND it was generated against an OLDER basis (claims have advanced since), back it up to `inventorship.{N-1}.md` and proceed with a fresh generation.
   - If yes AND `matrix_locked: false` and the basis is current, exit with a notice: "matrix exists and is current basis; attorney signature pending."
2. **Read inputs**:
   - `<thread>/BRIEF.md` — extract named inventors from the frontmatter and inventive features from §3.
   - Latest `<thread>.{N}/claims.tex` — if present, extract independent claims (parse `\begin{claim}...\end{claim}` blocks numbered 1, M, ... that are not dependent on a prior claim).
3. **Pick basis**:
   - If `claims.tex` exists at any version, use **basis B (claims-based)** with the highest-N version.
   - If no claims yet, use **basis A (feature-based)** from `BRIEF.md` §3.
4. **Generate the matrix**:
   - Frontmatter: thread slug, named inventors (from BRIEF), basis identifier, `generated_at` timestamp, `matrix_locked: false`.
   - Interview prompts: the 5-question list above (copy verbatim — these are legally derived).
   - Matrix: one row per feature (basis A) or per independent claim (basis B). Pre-fill `●` entries based on:
     - (basis A) The inventor most likely associated with each feature based on `BRIEF.md` context. **If uncertain, leave the cell blank and add a note "ATTRIBUTION TBD — pending inventor interview".** Never guess at attribution.
     - (basis B) The features-to-claims mapping should be evident from the spec's reference numerals and the claim language. Again, only pre-fill where the attribution is unambiguous from the source material.
   - Attribution rules: copy verbatim (these are 37 CFR 1.45 and case law derived).
   - Attestation block: copy verbatim, leave all checkboxes unchecked and attorney signature blank.
5. **Report**: print the path written and a one-line summary (e.g., `Inventorship matrix generated: acme-widget/inventorship.md (basis: thread.3/claims.tex, 3 independent claims, 2 named inventors, 4 attribution cells pre-filled, 5 marked TBD)`).

## Evidence mode (`--evidence`) — v1, opt-in

`ip-uspto-inventorship <thread> --evidence [<repo_path>] [--reseed]`

Mines the implementation repository's git history into an evidentiary trail backing the matrix. For AI-assisted invention this trail is increasingly load-bearing: reduction-to-practice attribution backed by commits, not recollection.

**What evidence mode is — and is not (advisory-only contract):**

- Git history documents **reduction to practice** (who committed working implementation), NOT **conception** (the legal test for inventorship). Every git-derived annotation MUST carry the reduction-to-practice label and the conception caveat.
- Evidence **informs the attorney interview; it never adjudicates**. It never adds or removes named inventors, and it never marks or unmarks `●` cells — the `●` pre-fill rules in the Procedure above (including "Never guess at attribution") govern unchanged.
- Evidence pre-fills the matrix **Notes column only**.

### Step E1 — Path map (`inventorship_map.json`): seed, cache, reseed

The map associates each matrix row key (feature IDs under basis A, claim element labels under basis B — matching the basis selected in Procedure step 3) with the repo paths that implement it:

```json
{
  "thread": "acme-widget",
  "basis": "B:thread.3/claims.tex",
  "seeded_at": "2026-06-12T00:00:00Z",
  "vendored_prefixes": ["third_party/", "vendor/"],
  "elements": {
    "C1": {
      "label": "Independent claim 1 — adaptive widget controller",
      "paths": [
        {"path": "src/controller.py", "role": "primary", "manually_seeded": true, "seeded_at": "2026-06-12T00:00:00Z", "lines": [40, 120]}
      ]
    }
  }
}
```

- `role` is one of `primary` / `vendored-primary` / `diverged-copy` / `supporting`.
- **Seeding is semi-manual**: on first run the agent proposes the element→paths map (from the basis rows and its reading of the repo) and the **operator confirms it** before the map is written. Path attribution is never guessed silently.
- **Cache semantics**: on reruns the cached map is reused and re-validated. A mapped path that has moved or disappeared produces a `stale-path` finding that **prompts the operator** for the new location — the cached map is never silently updated. `--reseed` discards the cache and seeds fresh.
- `vendored_prefixes` is an optional operator-maintained list; any mapped path under a listed prefix (or with role `vendored-primary`) is **BLOCKED** for evidence purposes: local git history attributes the importer, not the author, so upstream history is required. BLOCKED paths surface in the matrix Notes and the command report — never silently skipped.

### Step E2 — Deterministic mining (`inventorship_evidence.py`)

Run the promoted shared lib via its module entry point (`inventorship_evidence.py` was promoted to `anvil/lib/` in issue #516 once the provisional's inventorship-lite pass became its second consumer; the module lives in the `anvil.lib` package — not the hyphenated skill dir — so it runs via `python -m anvil.lib.inventorship_evidence`, resolving to `.anvil/anvil/lib/inventorship_evidence.py` in an installed consumer repo):

```bash
# From a consumer repo (uv-runnable install per issue #230):
uv run --project .anvil python -m anvil.lib.inventorship_evidence \
  <thread>/inventorship-evidence/inventorship_map.json \
  --repo <repo_path> \
  --write-evidence <thread>/inventorship-evidence/evidence.jsonl

# Or from the anvil source repo (development):
python -m anvil.lib.inventorship_evidence \
  <thread>/inventorship-evidence/inventorship_map.json \
  --repo <repo_path> \
  --write-evidence <thread>/inventorship-evidence/evidence.jsonl
```

JSON report to stdout. Exit codes per the tool-evidence convention: `0` = clean collection; `1` = findings (vendored/BLOCKED paths, `suspected-vendored` bulk-import heuristic hits, stale map paths, zero-history paths) — review each finding with the operator; `2` = invocation error (invalid map, git unavailable, not a git repository) — evidence mode degrades gracefully: report the error and continue with the matrix un-annotated.

`evidence.jsonl` is **append-only**, one JSON object per (path, sha): `{path, sha, author, email, date, subject, claim_element, classification, rationale}`. The miner emits `classification: "unclassified"`; rows already present (including rows the classification step has annotated) are never rewritten.

The miner also flags `suspected-vendored` when a path's add-commit touches more than 50 files AND its message matches the vendor heuristic (`vendor|import|port|migrat|consolidat`, case-insensitive) — prompt the operator before treating that history as authorship evidence.

### Step E3 — Classification (LLM step, in this command)

For each unclassified row, read the commit's **diff content** (via the lib's `commit_diff` helper, ~4000-char per-commit budget) and classify it as `conception` / `implementation` / `mixed` / `unclassified`, writing the classification and a one-line `rationale` back to the row. **Classify on diff content, never on the commit message alone** — commit messages are the #1 documented misclassification source. A commit whose diff introduces the inventive mechanism itself may evidence conception-adjacent activity; note it for the interview, but it still proves only reduction to practice.

### Step E4 — Matrix pre-fill (Notes column ONLY)

For each matrix row with classified evidence, append citations to the **Notes** cell in this shape:

```
git evidence (RTP): abc1234 Alice Author, 2025-03-02 — adds adaptive threshold loop
```

- `(RTP)` — the reduction-to-practice label — is mandatory on every annotation.
- BLOCKED paths render as `BLOCKED — vendored path (upstream history required): third_party/blob/` in the row's Notes.
- Add this caveat once, directly beneath the matrix table:

> Git evidence above documents **reduction to practice only**. Conception — the legal test for inventorship — must be established through the inventor interviews. A commit author is not thereby an inventor; an inventor need not appear in the commit log.

- **Never touch any other column.** `●` cells, inventor columns, and TBD markers follow the base rules exactly as if `--evidence` were not passed.
- **Locked matrix**: if `matrix_locked: true`, the matrix file is never modified (same rule as the base command); evidence artifacts are still written/refreshed under `<thread>/inventorship-evidence/`, and the report notes that Notes pre-fill is pending the next unlocked regeneration.

### Evidence-mode report

Extend the step-5 report line with an evidence summary, e.g. `evidence: 14 rows mined (3 new), 11 classified, 2 findings (1 stale-path, 1 suspected-vendored), 1 BLOCKED vendored path`.

## Interview mode (`--interview`) — v2, opt-in

`ip-uspto-inventorship <thread> --interview`

Generates one structured **interview packet** per candidate inventor from the v1-mined artifacts. Git history documents reduction to practice; the actual §115/§116 inventorship determination requires a *conception* interview that git logs never capture (whiteboard / hallway / phone conception). `--interview` does what counsel does: it turns each candidate into a personalized, structured packet they (or counsel, in a ~1-hour interview) complete.

**What `--interview` is — and is NOT (advisory-only contract carries over from v1):**

- **IS** deterministic template-fill from existing v1 artifacts (`inventorship_map.json` + `evidence.jsonl` + the matrix rows). Every sentence in a packet body is either statutory boilerplate (copied verbatim) or a deterministic projection of mined rows.
- **IS NOT** a re-run of v1 evidence collection — `--interview` consumes v1 outputs and **never re-mines** — and **never adjudicates**. The packet asks questions; it never answers them. Evidence anchors in the packet are labelled **memory aids only**, NOT evidence of conception.
- **Legal framing (load-bearing, do not weaken)**: packets are **ATTORNEY WORK PRODUCT**. They **never touch** the `●` matrix, inventor columns, or TBD markers. The `●` rules + "never guess attribution" + the attestation block above stay byte-identical. As with `--evidence`, the interview surface **informs the attorney interview; it never adjudicates** and **never adds or removes named inventors**. Without `--interview`, behavior is unchanged.

Packets are written to `<thread>/inventorship-evidence/interviews/{slug}.md`, where `{slug}` is the lowercased hyphenated full name (e.g. `alice-author.md`). (The issue body specifies `interviews/`; the native consumer uses `interview_packets/` — `interviews/` is chosen here per the issue body.)

### Step I1 — Locate v1 inputs (no auto re-mine)

`--interview` runs **only** against existing v1 artifacts. Read `<thread>/inventorship-evidence/inventorship_map.json` and `<thread>/inventorship-evidence/evidence.jsonl`. If either is absent, emit the notice "run `ip-uspto-inventorship <thread> --evidence` first" and exit cleanly — write no packets, touch nothing. Evidence is never re-mined here.

### Step I2 — Candidate list (never invent inventors)

The candidate list is the **union** of: (1) named inventors from `BRIEF.md` frontmatter (anvil's analog of the title page), (2) `(author, email)` pairs surfaced by v1 classification as conception-class committers NOT in the named list, (3) resolved human director(s) for bot rows. Anvil never invents inventors beyond this union — same "never invent inventors" rule as the base command. A bot identity is **never** a candidate.

### Step I3 — Vendored detection (reuse the v1 helper)

Reuse v1's `is_vendored_path` + `vendored-primary` / `vendored_prefixes` logic from `anvil/lib/inventorship_evidence.py` (do NOT reimplement). When a candidate's mapped/evidence paths intersect a vendored path, the packet MUST include the upstream-conception prompt (the `VENDORED_CODE_PROMPT` constant): the in-repo history attributes the importer, not the upstream author, so the packet asks the candidate to confirm any upstream-conception involvement directly.

### Step I4 — Bot-author resolution

For rows whose `author` / `email` match a CI/agent identity (operator-configurable pattern, defaulting to the documented `…[bot]` / `…-agents` / `noreply@` shape), apply the 5-step resolution chain: (1) triggering-issue author — **auto** when pre-resolved; (2) triggering chat thread — counsel-resolved; (3) channel-agent operator — counsel-resolved; (4) project lead — fallback; (5) sync-commit author — weakest signal. Steps 2–5 are surfaced as questions / flagged fallbacks for counsel, **never** silently auto-attributed. The bot is **never** a §115 inventor (not a natural person).

> Open question for counsel: the bot-chain step ordering, the Q5 derivation phrasing, and the ex-employee distribution channel are native open questions for counsel ratification. The documented defaults above are implemented; do not block on them.

### Step I5 — Render packets

Run the skill-local lib by direct file path (the skill dir is hyphenated, so there is no dotted `python -m` path; in an installed consumer repo the path is `.anvil/skills/ip-uspto/lib/inventorship_interview.py`):

```bash
python3 anvil/skills/ip-uspto/lib/inventorship_interview.py \
  <thread>/inventorship-evidence/inventorship_map.json \
  <thread>/inventorship-evidence/evidence.jsonl \
  --thread <thread> \
  --inventor "Alice Author:alice@example.com" \
  --inventor "Bob Builder:bob@example.com" \
  --out-dir <thread>/inventorship-evidence/interviews
```

`render_packet` assembles, per candidate: a **sensitivity header** (`counsel-eyes-only` for the stored template by default; `distribute-to-named-candidate-only` for a distributed copy; `confidential-internal` for working drafts) → confidential top disclaimer → `STATUTORY_INTRO` (Burroughs Wellcome conception standard, MPEP §2138.04, plain-English §115/§116/§256) → bot-resolution block (when applicable) → vendored prompt (when applicable) → evidence-anchors disclaimer → **per-element Q1–Q7 blocks**: Q1 conception moment, Q2 definiteness (Burroughs Wellcome), Q3 joint conception (§116), Q4 corroboration, Q5 derivation (§102(f)/§102(a)), Q6 prior art, Q7 post-conception RTP authorship — **exactly one Q1–Q7 block per element**, where a composite label like `1(b)(iv-v)` collapses to one block at the composite label, not one per leaf → per-element evidence anchors (from `evidence.jsonl` rows matching the candidate by email OR display-name, case-insensitive, labelled memory-aids-only) → attorney-work-product footer (`CONFIDENTIAL_FOOTER`, top and bottom). Exit `0` = packets written; exit `2` = missing v1 artifacts (the "run --evidence first" notice).

### Step I6 — Commit / report

Report the packet count and any open-question-for-counsel flags, e.g. `interview: 3 packets written (alice-author, bob-builder, carol-coder); 1 vendored prompt, 1 bot-resolution block (1 UNRESOLVED — counsel follow-up)`. **Locked matrix** (`matrix_locked: true`): packets still generate (they are read-only consumers of v1 artifacts) and the matrix file stays byte-unchanged.

### Companion mode: `--synthesize` (v2, the judgment-laden half)

Determination synthesis (`--synthesize`, the section below) — parsing completed interview packets back into per-element inventorship determinations (the candidacy table, disputed-elements classification, convergence rollup) — is the judgment-laden half. It is **now implemented** (issue #511); the synthesis parser depends on the exact `--interview` packet markdown shape (now frozen by the merged `--interview` mode above). Synthesis proposes; the attorney attests. The advisory-only contract governs both halves.

## Synthesis mode (`--synthesize`) — v2, opt-in

`ip-uspto-inventorship <thread> --synthesize`

Parses the **filled-in** interview packets (from `--interview`) back into a per-element inventorship **determination that the attorney reviews**, written to `<thread>/inventorship-evidence/synthesis.md`. Git history documents reduction to practice; the `--interview` packets capture each candidate's *conception* claim; `--synthesize` aggregates those claims into a candidacy table, classifies disputes, and rolls up convergent inventors — **all FOR COUNSEL, never adjudicated here**.

**What `--synthesize` is — and is NOT (advisory-only contract carries over from v1/--interview):**

- **IS** a determination FOR COUNSEL: a 7-section `synthesis.md` aggregating every candidate's filled packet (candidacy table → disputed elements → convergent inventors → suggested inventors → open questions → bot-resolution status → partial-response handling). Synthesis proposes; the attorney attests.
- **IS NOT** an adjudication. It **never** reads or writes the `●` matrix (`inventorship.md`), **never** adds or removes named inventors, and **never infers conception in the absence of a candidate response** — `unanswered` (no returned date) and `partial` (returned packet, element skipped) are surfaced in §5 / §7 and **never** resolved to a conceiver. The bot is **never** a §115 inventor; bot-resolution status is reported in §6 but never auto-confirmed.
- **Legal framing (load-bearing, do not weaken)**: `synthesis.md` is **ATTORNEY WORK PRODUCT**, defaulting to `counsel-eyes-only` (it aggregates every candidate's packet). The `●` rules + attestation block stay byte-identical. Without `--synthesize`, behavior is unchanged.

`synthesis.md` is written to `<thread>/inventorship-evidence/synthesis.md`.

### Phase S1 — Parse filled packets (LLM-in-command, calling `parse_packet` for the deterministic skeleton)

Read every `<thread>/inventorship-evidence/interviews/{slug}.md` packet. If the `interviews/` dir is absent or empty, emit the notice "run `ip-uspto-inventorship <thread> --interview` first" and exit cleanly (exit `2`) — write no synthesis, touch nothing.

The **deterministic skeleton** is lifted by the lib helper `parse_packet(markdown) -> ParsedPacket`: it extracts (a) the candidate display name from the `**Candidate:**` header, (b) the returned date from the signature block (`None`/unanswered when the date line is blank), (c) per-`### Element <key>` raw Q1–Q7 answer strings, and (d) a per-answer `placeholder_unchanged` flag (the `> _Your answer:_` line was not filled). This is the unit-testable contract against the frozen `--interview` `render_packet` shape.

**Interpreting** those raw free-text answers into a `CandidateResponse` is the **LLM-in-command** half (the rubric-rebackport precedent: deterministic extraction in the lib, judgment in the runtime). A free-text Q1 like "around mid-Feb, on the whiteboard with Bob" needs light normalization; Q3 "Bob and I sketched it" needs name extraction. Construct one `CandidateResponse(candidate, returned_date, answers, notes)` per packet, where `answers` is `{element_key -> {"Q1": ..., "Q3": ..., …}}` and an element the candidate genuinely skipped is simply **absent** (→ `partial`). **Never** synthesize an answer the candidate did not give; an unanswered/partial element stays unanswered/partial. For a purely mechanical run (no LLM interpretation available), the lib's `response_from_parsed` / `build_synthesis` give a deterministic projection that drops placeholder-only elements — same invariants.

### Phase S2 — Render the synthesis (deterministic lib)

Once `CandidateResponse` objects exist, the classification + rollup is a **pure function** of structured input — call the lib's `render_synthesis(filing, thread, generated_date, inv_map, responses, bot_resolutions)`. It produces the 7-section `synthesis.md`:

1. **Candidacy table** — per-element rows × candidate columns (`claimed-sole` / `claimed-joint` / `claimed-none` / `unanswered` / `partial`).
2. **Disputed elements** — `CONFLICTING` (≥2 candidates claim sole conception), `MIXED` (sole + joint to reconcile), `NAMED NON-RESPONDENT` (a joint claimant names a conceiver who returned no packet). v2 never resolves these.
3. **Convergent inventor list** — elements where the responses agree (one sole conceiver, or a consistent joint set).
4. **Suggested inventor list (advisory-only)** — the per-candidate rollup of the convergent map, with §116 framing. Counsel makes the final call.
5. **Open questions for counsel** — non-respondents and unclaimed elements.
6. **Bot-author resolution status** — surfaced from the v1 bot-resolution chain, never auto-confirmed.
7. **Partial-response handling** — every `unanswered` / `partial` element, with the explicit "v2 does NOT infer conception in the absence of a candidate response" reminder.

The classification helpers (`_summarize_response_for_element`, `_identify_disputed_elements`, `_identify_convergent_elements`, `_suggest_inventors`, `_open_questions`, `_q1_indicates_no_claim`, `_q3_named_others`) are ported verbatim-adapted from the native `render_synthesis`, on anvil's `inv_map["elements"]` basis (the element *key* is the join — it is what `parse_packet` lifts from `### Element <key>` and what the §1/§2/§3 tables key on).

Run the skill-local lib by direct file path (the skill dir is hyphenated; in an installed consumer repo the path is `.anvil/skills/ip-uspto/lib/inventorship_interview.py`):

```bash
python3 anvil/skills/ip-uspto/lib/inventorship_interview.py \
  <thread>/inventorship-evidence/inventorship_map.json \
  <thread>/inventorship-evidence/evidence.jsonl \
  --thread <thread> \
  --synthesize \
  --interviews-dir <thread>/inventorship-evidence/interviews
```

This writes `<thread>/inventorship-evidence/synthesis.md`. Exit `0` = synthesis written; exit `2` = missing v1 artifacts OR missing/empty `interviews/` dir (the graceful "run `--interview` first" notice). The `●` matrix is never read or written.

### Phase S3 — Commit / report

Report the candidate count and any disputed-element flags, e.g. `synthesize: 3 packets parsed (alice-author, bob-builder, carol-coder); 1 CONFLICTING, 1 convergent-joint, 1 unanswered, 1 bot-row UNRESOLVED — counsel follow-up`. **Locked matrix** (`matrix_locked: true`): synthesis still generates (it is a read-only consumer of the interview packets + v1 artifacts) and the matrix file stays byte-unchanged.

## Re-validation pre-finalize

After the claim set stabilizes (during AUDITED → FINALIZED transition), re-run this command to regenerate the matrix against the final `claims.tex`. The previous matrix is backed up. The human attorney must re-attest against the final matrix before `ip-uspto-finalize` will proceed.

## Idempotence

- A locked (`matrix_locked: true`) matrix generated against the current basis is never overwritten.
- An unlocked matrix against the current basis is preserved (a no-op with a notice).
- An out-of-date matrix is backed up before being replaced.
- The operator can force regeneration by deleting `inventorship.md`.

## Notes for the inventorship agent

- **Pre-fill conservatively.** It is far less harmful to leave a cell blank and let the human attorney fill it after interviews than to pre-fill incorrectly and have the attorney accept the bad attribution by inattention.
- **Never invent inventors.** Only the inventors named in `BRIEF.md` frontmatter may appear in the matrix.
- **Conception ≠ reduction to practice.** This distinction is the source of most inventorship errors. The matrix attribution rules document it; the matrix itself enforces it by only listing the conceiving step.
- **Re-validation is mandatory pre-finalize.** Claims often change during revision (a claim limitation gets added, removed, or shifted between independents and dependents). The matrix MUST track the final claims, not just the first-draft features.


**Snippet references**: See `anvil/lib/snippets/progress.md` for the `_progress.json` read-merge-write recipe and `anvil/lib/snippets/timestamp.md` for the ISO-8601 UTC timestamp convention. The merge is shallow: preserve fields and phases not touched by this command.

## Git sync (opt-in, off by default)

Per `anvil/lib/snippets/git_sync.md` (`.anvil/anvil/lib/snippets/git_sync.md` in an installed consumer repo): if `.anvil/config.json` exists and `git.commit_per_phase` is `true`, end this phase: stage only the dirs this phase wrote, commit as `anvil(<skill>/<phase>): <thread>.{N} [<state>]`, push if `git.push` is `true`. Git failures warn and continue — never fail the phase. When the config or knob is absent, skip this step entirely (default off).

This phase's specifics:

- **Ordering**: after `<thread>/inventorship.md` is written. A preserved-matrix no-op run writes nothing, so the hook has nothing to commit and is a silent no-op.
- **Staging target**: ONLY `<thread>/inventorship.md`, staged explicitly by path (a thread-level file per the snippet's staging rules). In `--evidence` mode, additionally stage `<thread>/inventorship-evidence/inventorship_map.json` and `<thread>/inventorship-evidence/evidence.jsonl` (explicitly by path) when written/appended this run. In `--interview` mode, additionally stage `<thread>/inventorship-evidence/interviews/` (explicitly by path) when interview packets were written this run. In `--synthesize` mode, stage ONLY `<thread>/inventorship-evidence/synthesis.md` (explicitly by path) when the synthesis was written this run. Default (no-flag) runs stage exactly what they staged before.
- **Commit**: `anvil(ip-uspto/inventorship): <thread> [<state>]` — a thread-level command with no version dir, so the version token is the bare thread slug per `git_sync.md` §Non-thread commit shapes; the bracket is `INVENTORSHIP_DONE` on the pre-draft run, or the thread's current derived state on a pre-finalize re-validation. The `--evidence`/`--interview`/`--synthesize` modes use a generic commit subject (no claim language, contributor names, or findings — `synthesis.md` is `counsel-eyes-only` attorney work product; the subject must leak nothing).

