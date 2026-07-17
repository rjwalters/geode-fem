---
project: botho-bridge-spec
audience:
  - Protocol implementers, auditors, and exchange/bridge integrators reading the bridge design as the source of truth
max_iterations: 4
documents:
  - slug: botho-bridge-spec
    artifact_type: spec
    # code_ref is illustrative-only in this vendored example: it points at
    # botho's own bridge Rust workspace, which is deliberately NOT vendored
    # here. Standalone the glob resolves nothing, so resolve_code_ref returns
    # a structured missing:true entry — the consistency tier activates but
    # degrades gracefully (a `major` finding, never a crash). See
    # ../expected-thread.N/README.md and the Provenance section below.
    code_ref: ../../bridge/**/*.rs
---

# Botho wBTH Bridge — normative spec section (destined for whitepaper §11)

A normative treatment of the BTH↔wBTH cross-chain bridge, authored as an
`anvil:spec` thread so the `code_ref` consistency audit verifies every claim
against the bridge implementation. The AUDITED LaTeX body is integrated into
the whitepaper as a new section.

## Provenance (vendored worked example)

This is a **trimmed snapshot** of a real, committed, terminal-`AUDITED`
`anvil:spec` thread, vendored from the public consumer repo as the skill's
worked example (issue #709). It was NOT re-run to produce this example — the
committed botho thread was read and trimmed in place (the primer #700
Phase-4 pattern):

- **Source**: `botho-project/botho`, path `whitepaper/bridge-spec/`, commit
  `d8c628dc40e3bb3d04ecefb835774cea95487fd5` (the last commit touching
  `whitepaper/bridge-spec/` on `main`; the wBTH bridge normative spec,
  integrated as whitepaper §11 in botho#945).
- **Trim**: only the terminal `AUDITED` version (`botho-bridge-spec.2`) plus
  its two parallel critic siblings (`.2.review`, `.2.audit`) are vendored —
  the intermediate `.1` trajectory survives in
  `botho-bridge-spec.2/_progress.json` (`metadata.score_history` v1=38/44 and
  `metadata.revise_note`) and `changelog.md`, so the 38 → advance story is
  preserved without the extra `.1` bodies and their two critic siblings. The
  three exhibit PNGs (~456 KB, `spec`'s canonical output is the LaTeX source
  per `SKILL.md` §"Output format") and the wholesale `refs/` ADR set (~40 KB
  of botho's own design docs) are dropped; `refs/` is replaced with a small
  `context-note.md`. The body's `\includegraphics{exhibits/figN-*.png}`
  references dangle standalone as a result — expected, matching the primer
  worked example's dropped-figure precedent.
- **`% anvil-const:` markers are preserved verbatim** on the authoritative
  constants (`wbth_decimals=12`, `ring_size=20`, `import_epoch_blocks=17280`,
  `import_factor_floor=1.5×`, plus the symbolic `bridge_threshold_floor`), so
  the constant-consistency gate has real constants to check — an unmarked
  example would teach false confidence.
- **`code_ref` is illustrative-only.** The BRIEF declares
  `code_ref: ../../bridge/**/*.rs` (de-pathed from the original absolute
  `/Users/rwalters/GitHub/botho/bridge/**/*.rs`), which points at botho's own
  bridge Rust workspace — deliberately NOT vendored here (out of scope,
  large, not anvil's to maintain). The glob will **not resolve** when this
  example is copied standalone; the spec↔implementation consistency audit
  tier simply degrades gracefully (`resolve_code_ref` returns a structured
  `missing: true` entry, never raises). See `../expected-thread.N/README.md`
  for the full structural contract.

## Source of truth (in `refs/`)

The five ratified bridge ADRs are the design source (see `refs/context-note.md`
for the de-pathed summary; the wholesale ADRs are not vendored):
- **ADR 0002** — custody / trust model (SCP-validator threshold-multisig federation)
- **ADR 0003** — the peg (factor-1-only wrapping + the demurrage-settlement on-ramp)
- **ADR 0004** — privacy semantics at the boundary (amount revelation on lock, re-shield on unwrap)
- **ADR 0005** — v1 chain scope (Ethereum + Solana)
- **ADR 0007** — bridge-import cluster tagging (epoch-keyed import factor, K=1 day, F=1.5×)

The `code_ref` implementation (`bridge/core`, `bridge/service`,
`contracts/ethereum`, `contracts/solana`) is the consistency oracle — the
audit must confirm the spec matches it, or mark a divergence with a three-way
disposition.

## Implementation-status discipline (load-bearing)

The bridge is partially shipped. The spec MUST carry an `## Implementation
status` register distinguishing live from target-state, because a claim that
reads as "is" when the code says "will be" is exactly the drift ADR 0006's
audit machinery exists to catch. Known live/target split at authoring time:
- **Live**: the Ethereum wrap/unwrap path (Phase 0–3, PRs #832–#864);
  exactly-once semantics; factor-1 peg.
- **Target-state (tracked)**: bridge-import cluster tagging (ADR 0007
  ratified; implementation in flight as #938) — mark target-state until #938
  merges; Solana transports (stubbed, #856/#857/#858/#853); the
  demurrage-settlement operation (#831, blocked on the shared reset-horizon
  ratification); external security audit (#616/#830, the mainnet gate).

Do not describe target-state mechanisms in the present tense without a
register row.
