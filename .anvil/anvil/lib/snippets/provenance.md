# Claim provenance: local-corpus ground-truth verification

This snippet codifies the **local-corpus claim-provenance contract**
(issue #597) — how a project declares a read-only ground-truth corpus,
how the drafter records a per-version claim→source map, how the
reviewer spot-checks it, and how an audit critic verifies each mapped
claim against the corpus on disk and classifies it. It defends against
the failure mode a grounded artifact fears most: an LLM drafting pass
hallucinating plausible dates, quotes, and attributions that no source
supports.

The canary is `nitas-mama` (a family memoir): every quote and every
factual claim must trace to a local ground-truth corpus — seven
interview transcripts and nine family letters. But the contract
generalizes to any artifact grounded in a private evidence base:
engagement notes for `report`, lab notebooks for `pub`, customer
interviews for `proposal`.

## Scope boundary (vs. voice fidelity, #598)

This contract owns **substance verification** — *does the corpus
actually contain a passage supporting this named fact, date, memory, or
event?* It does **not** own whether a reconstructed line *sounds* like
the speaker; that voice/cadence-fidelity half is the `voice.subjects`
tier (issue #598, `anvil/lib/snippets/voice_grounding.md`). The
touchstone for a line "She said X happened in 1924":

- **#597 (this contract)** asks: *does the transcript corpus contain
  any passage supporting that event in 1924?*
- **#598** asks: *does the reconstructed line sound like how she would
  phrase it?*

Both matter; neither contains the other. **Misattribution** sits at the
boundary and is split cleanly: substance-level misattribution — an
event or memory attributed to a speaker whose corpus does not contain
it — is **this contract's** `misattribution_of_substance` flag;
voice-identity misattribution — right substance, rendered in the wrong
voice — is #598's flag. The two tiers are independent and a memoir may
declare both.

## Section 1 — BRIEF activation

A project declares its factual ground truth via ONE optional
**top-level** key in the project `BRIEF.md` frontmatter (parsed by
`anvil/lib/project_brief.py::_normalize_corpus_dirs`, resolved by
`resolve_corpus_dirs`):

```yaml
corpus:                    # NEW top-level key (issue #597): factual ground truth
  - transcripts/           #   read-only directories of source evidence
  - letters/
```

This is **distinct from `voice.corpus`** — a single glob nested *under*
`voice:` naming author-persona *published* exemplars (`ResolvedVoiceDoc`,
issue #461). The top-level `corpus:` is a **list of directory paths** for
factual sources. Different YAML level, different purpose, no naming
conflict. A project may legitimately carry both:

```yaml
voice:
  corpus: writing-corpus/**/*.md   # author voice exemplars (VoiceDocs.corpus, #461)
corpus:                            # factual ground-truth sources (ProjectBrief.corpus, #597)
  - transcripts/
  - letters/
```

Activation rules (byte-identical when absent):

- **`corpus:` declared with ≥1 path → tier ACTIVE.** The drafter writes
  a `provenance.md` map, the reviewer back-checks it, the audit critic
  verifies it.
- **Absent key, `corpus: null`, or `corpus: []` → tier INACTIVE.**
  Byte-identical no-corpus behavior: no `provenance.md` required, no
  findings, no extra reads. `resolve_corpus_dirs` returns `[]`; callers
  branch on `if not resolved:` for the inactive path.
- **A single string** (`corpus: transcripts/`) normalizes to a
  one-element list. A **non-string list element** raises `ValueError`
  with the field path (e.g. `BRIEF.corpus[1]`).
- **Declared-but-missing corpus directory → the tier ACTIVATES** and the
  breakage surfaces as a **`major` review finding** (a structured
  `missing: true` `ResolvedCorpusDir`, never a raise — the same
  defect-to-surface posture as voice grounding). Resolution is
  project-root first, then consumer-root; git status is never consulted
  (a `.gitignored` corpus resolves identically to a committed one).

Resolve with
`anvil/lib/project_brief.py::resolve_corpus_dirs(project_dir,
consumer_root=None)` — do not re-implement the walk. It returns one
`ResolvedCorpusDir` per declared path, in declared order, each carrying
`declared` / `path` (absolute, `None` when missing) / `missing` /
`source` (`project` | `consumer` | `absolute`).

## Section 2 — the per-version `provenance.md` claim→source map

When the corpus tier is active, the drafter writes a
`<thread>.{N}/provenance.md` file **at draft time, before prose**, and
keeps it current through every revise pass (each `<thread>.{N+1}/`
carries a refreshed map). It is a markdown table mapping each attributed
quote or factual claim to its supporting corpus passage:

```markdown
# Claim provenance — <thread>.{N}

| Claim | Source file | Line range | Notes |
|-------|-------------|------------|-------|
| "The factory burned down" | transcripts/nita3.txt | 412-415 | verbatim recall |
| Journey took six weeks | letters/1940-aug.rtf | 3-7 | inferred from dates |
```

Column contract:

- **Claim** — the attributed quote (verbatim, in quotes) or the factual
  assertion (paraphrased) as it appears in the artifact.
- **Source file** — a path **relative to a declared corpus directory**
  (e.g. `transcripts/nita3.txt`), resolvable under one of the
  `resolve_corpus_dirs` roots.
- **Line range** — a `start-end` line span (or a single line) locating
  the supporting passage.
- **Notes** — the drafter's brief characterization (`verbatim recall`,
  `inferred from dates`, `paraphrase`). The drafter fills `Notes`; the
  **audit critic** — not the drafter — assigns the five-way
  classification (Section 5).

Drafter discipline:

- The drafter writes one row per attributed quote and per checkable
  factual claim (named dates, names, events, places).
- **Fabricating a source-line mapping is prohibited.** If no corpus
  passage supports a claim, the drafter does not invent a citation:
  either cut the claim or record it with a `NOT_FOUND` source note so
  the audit critic sees it explicitly.
- A **missing `provenance.md`** when the corpus tier is active is a
  **`major`** finding (a broken contract), **not a crash** — the
  reviewer surfaces it and drafting still proceeds.

## Section 3 — reviewer back-check contract

When the corpus tier is active and `provenance.md` exists, the reviewer
runs a **provenance back-check** as a sub-step of its pass:
**spot-sample 5–10 rows per review pass**, opening each cited file + line
range in the resolved corpus.

- Findings are `kind: judgment` findings with `evidence_span` pointing at
  the map row (`provenance.md:L<N>`).
- **A row whose cited file does not exist** (not resolvable under any
  corpus root) is a **`major`** finding.
- **A row whose cited passage does not support the claim as written** is
  a **`blocker`** finding.
- The reviewer quotes both the claim and the cited passage in the
  finding — the same load-bearing evidence discipline as the voice
  corpus-quote rule. Vague back-check feedback without a quoted passage
  is itself a defective finding.

The back-check is a **sampling** check (cheap, every review pass); the
exhaustive verification is the audit critic's job (Section 4).

## Section 4 — audit-critic contract (`kind: tool_evidence`)

The corpus-provenance audit critic is a `kind: tool_evidence` critic
(the pattern already documented in `anvil/lib/snippets/audit.md`;
`Kind.TOOL_EVIDENCE` already exists in `anvil/lib/review_schema.py` and
the schema validator already enforces `tool_calls` on every
`tool_evidence` finding — **no schema change is needed**). It runs the
**exhaustive** pass the reviewer only samples:

1. **Inventory** every attributed quote and factual claim in the
   artifact, and every row in `provenance.md`. A claim in the artifact
   with **no `provenance.md` row is a finding in itself** (unmapped
   claim).
2. For **each** map row, open the cited file + line range in the resolved
   corpus and **classify** it with the five-way vocabulary (Section 5).
3. Every `MISMATCH` / `NOT_FOUND` / `FABRICATED` row emits a finding with
   a non-empty **`tool_calls`** array recording the file-read operation
   that produced the evidence (the passage read, the lines inspected).
4. Fabrication-class entries additionally emit **`critical_flags`**
   (Section 6), which route through the existing verdict machinery
   (`anvil/lib/critics.py::_compute_verdict_impl` already short-circuits
   any `critical_flags` → `Verdict.BLOCK` — no change needed).

`kind: tool_evidence` with `findings == []` is valid — a corpus whose
every claim VERIFIED / PARAPHRASE_OK is a clean audit.

## Section 5 — five-way classification vocabulary

The audit critic classifies each `provenance.md` row as exactly one of:

- **`VERIFIED`** — exact or near-exact textual match in the cited
  passage.
- **`PARAPHRASE_OK`** — the substance is present in the passage; the
  wording is clearly authorial paraphrase (legitimate reconstruction,
  not invention).
- **`MISMATCH`** — the passage exists but does not support the claim as
  written (e.g. a different year, a different person, a different place).
- **`NOT_FOUND`** — no matching passage found in the declared line range
  or the surrounding context.
- **`FABRICATED`** — the claim conflicts with the corpus, or the corpus
  explicitly contradicts it.

`VERIFIED` and `PARAPHRASE_OK` are passing classifications. `MISMATCH`
and `NOT_FOUND` are findings. `FABRICATED` is a finding **and** a
critical flag.

## Section 6 — fabrication-class critical flag types

These are the `CriticalFlag.type` strings the audit critic (and, at the
boundary, the reviewer) raises. They are **skill-defined vocabulary**
(the lib does not enforce a `CriticalFlag.type` enum); any one forces
`Verdict.BLOCK` regardless of rubric score:

- **`fabricated_quote`** — verbatim-quoted text that does not appear in
  the corpus.
- **`fabricated_fact`** — a named date, name, or event not traceable to
  any corpus passage.
- **`misattribution_of_substance`** — an event or memory attributed to a
  speaker whose corpus does not contain it. This is the **substance-level**
  flag; voice-level misattribution (right substance, wrong voice) belongs
  to #598.
- **`anachronism`** — an era-incompatible detail contradicted by the
  corpus chronology.
- **`unattributed_paraphrase`** — authorial invention presented as a
  subject's memory without any corpus grounding.

Each flag's `justification` quotes the offending artifact text and the
corpus evidence (or its absence). The flag is *additive* — it uses the
existing critical-flag machinery, not a rubric-total change.

## Section 7 — `_progress.json` extension

The corpus-audit critic records a roll-up of its classification pass in
the `_progress.json` inside its sibling dir, under
`metadata.provenance_summary`:

```json
{
  "metadata": {
    "provenance_summary": {
      "total_claims": 42,
      "verified": 30,
      "paraphrase_ok": 8,
      "mismatch": 2,
      "not_found": 1,
      "fabricated": 1
    }
  }
}
```

The six counts sum to `total_claims`. The field is **omitted entirely**
when the corpus tier is inactive (no new `_progress.json` surface for
ungrounded projects — the byte-identical-when-absent posture).

## Section 8 — sibling dir naming

The corpus-provenance audit critic writes its sidecar to
`<thread>.{N}.corpus-audit/`, following the `version_layout.md` critic-tag
convention (`<thread>.{N}.<tag>/`, a single short token, no nested dots).
It is a normal critic sibling: immutable once written, discovered by the
`enumerate_siblings` machinery, and re-pointed by the
`<thread>.latest.corpus-audit` symlink family. It coexists with the
general `.audit/` sibling — the corpus audit is the substance-verification
specialist, not a replacement for the general audit pass.

## Relationship to `<thread>/refs/` and `cite.py`

This contract is deliberately separate from two existing surfaces:

- **`<thread>/refs/`** (issue #144) holds *per-thread* author-supplied
  PDFs for `pub-audit`. The top-level `corpus:` is a **project-level**
  read-only evidence base shared across all threads. The two coexist —
  `pub` keeps its per-thread `refs/`; corpus-aware skills get the
  project-level corpus.
- **`anvil/lib/cite.py`** is strictly *external* identifier resolution
  (DOI/arXiv → BibTeX). It knows nothing about local files or line-level
  citation maps. Claim provenance is local-corpus, line-range, and
  substance-verifying — an orthogonal concern.
