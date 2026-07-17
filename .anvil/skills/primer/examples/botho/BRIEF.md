---
project: botho
audience:
  - Technically-curious non-cryptographers (primary — developers, prospective node operators, informed users)
  - Cryptographers auditing the intuition against the formal spec (secondary)
hard_rules:
  - Teach from intuition first; defer formal rigor to the whitepaper via cross-reference.
  - Never duplicate a formal spec section — teach it, then point ("see §X of the whitepaper").
  - Never contradict the cited spec; a simplification may be lossy-but-true, never false.
  - Introduce every piece of jargon before using it; standard primitives with
    external literature may be cited out rather than re-taught in full.
  - Spend the most ink on the novel-to-Botho pieces — no external tutorial exists
    to defer to for those.
documents:
  - slug: botho-from-the-basics
    artifact_type: primer
    spec_ref: ../../whitepaper/sections/*.tex
---

# Botho from the Basics — project brief

A ground-up teaching companion to the Botho whitepaper (issue #881). The
whitepaper (`whitepaper/botho-whitepaper.tex`, sections under
`whitepaper/sections/01..13` + appendices) is a terse, citable, spec-style
document for cryptographers, auditors, and implementers. This companion is the
"Mechanics of MobileCoin" genre: a separate ground-up explainer that teaches
the same primitives with intuition, sitting alongside (not replacing) the
formal spec.

## Provenance (vendored worked example)

This is a **trimmed snapshot** of a real `anvil:primer` run, vendored from the
public consumer repo as the skill's worked example (issue #693):

- **Source**: `botho-project/botho`, path `docs/primer/`, commit
  `32626b48bc74572b23d52c4232202faf78fd573e` (the last commit touching
  `docs/primer/` on `main`, dogfooded via botho-project/botho#881 → PR #900,
  merged 2026-07-14).
- **Trim**: only the terminal `AUDITED` version (`botho-from-the-basics.3`,
  44/44) is vendored — the full 41→43→44 trajectory survives in
  `botho-from-the-basics.3/_progress.json` `metadata.score_history` and
  `changelog.md`. The compiled PDF (~1.2 MB) and the five full-resolution
  exhibit PNGs (~1.1 MB) are dropped (primer's canonical output is the
  markdown source, per `SKILL.md` §"Output format"); the `.mmd` mermaid
  sources are kept so the figure references in the body resolve to a source.
- **`spec_ref` is illustrative-only.** The BRIEF declares
  `spec_ref: ../../whitepaper/sections/*.tex`, which points at botho's own
  whitepaper — deliberately NOT vendored here (out of scope, large, not
  anvil's to maintain). The glob will **not resolve** when this example is
  copied standalone; the spec-consistency audit tier simply degrades
  gracefully (a `major` finding, never a crash). See
  `../expected-thread.N/README.md` for the full structural contract.

## What this primer teaches (and what it defers)

- **Teaches from intuition**: why each mechanism exists, why this design
  choice, what breaks without it — in plain language, with load-bearing
  analogies, before any notation.
- **Defers to the spec**: formal derivations, security proofs, and normative
  parameter tables live in the whitepaper. This companion cross-references
  them ("for the formal treatment, see §X of the whitepaper") — it does not
  restate them.

## Scope (from issue #881)

1. **Privacy building blocks** (intuition-first, minimal formalism; external
   literature exists — cite out rather than re-teach in full):
   - Stealth addresses (recipient privacy)
   - Ring signatures / CLSAG (sender privacy)
   - Pedersen commitments + Bulletproofs (amount hiding)
2. **The novel-to-Botho pieces — spend the most ink here**:
   - Hybrid **post-quantum** stealth addresses (ML-KEM-768 encapsulation +
     ML-DSA-65 authorization) and why classical-only privacy coins are exposed.
   - **SCP consensus paired with proof-of-work mining** — unusual (MobileCoin
     uses SCP but does not mine); explain the split between consensus and
     issuance, and why mining weight is decoupled from consensus voting weight.
   - **Anti-hoarding economics**: demurrage + progressive fees + the
     cluster-tilted lottery, and the reasoning vs. fixed-supply/tail-emission
     designs.
3. **"Putting it together" capstone**: follow one transaction end-to-end.

## Non-goals

- NOT a specification. Defer all rigor, proofs, and exact parameters to the
  whitepaper; cross-reference it throughout.
- Do not duplicate the whitepaper's formal sections — teach intuition, then
  point to the spec.

## Reference material

- `botho-from-the-basics/refs/` — issue context and supporting notes.
- The `spec_ref` whitepaper sections are the consistency oracle:
  `primer-audit` reads them to flag any primer claim that contradicts them;
  `primer-review` reads them to flag any formal section the primer duplicates
  instead of cross-referencing.
