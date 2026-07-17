# Context notes for the drafter (from issue #881 and the repo)

## Model to emulate

"Mechanics of MobileCoin" — a separate, ground-up explainer that teaches the
primitives with intuition, sitting alongside (not replacing) the formal spec.
See the MobileCoin paragraph in `whitepaper/sections/02-related-work.tex` for
the comparison Botho itself draws: MobileCoin also pairs CryptoNote-style
privacy with SCP consensus, but does not mine (fixed supply, fees only),
and its privacy is classical-only.

## Facts worth getting exactly right (checked against the codebase)

- Monetary unit: **picocredits**; 1 BTH = 10^12 picocredits; u128 arithmetic
  internally. (A prior whitepaper pass fixed a 1000× fee-unit error — be
  careful with units.)
- Emission: ~611M BTH over a 5-year main emission with a 2% perpetual tail.
- Mining: RandomX CPU proof-of-work; economic issuance only. Mining weight is
  deliberately decoupled from consensus voting weight.
- Consensus: SCP (Stellar Consensus Protocol) / federated Byzantine agreement
  with operator-curated quorum slices (subjective trust). Not proof-of-stake,
  not Nakamoto longest-chain.
- Privacy: CLSAG linkable ring signatures, Pedersen commitments +
  Bulletproofs range proofs, stealth addresses, Dandelion++ propagation.
- Post-quantum: hybrid stealth addresses using ML-KEM-768 (key encapsulation)
  alongside the classical curve; ML-DSA-65 signs minting transactions (spends
  authorize via CLSAG) — per whitepaper §4. (Issue #881's text says "ML-DSA-65
  authorization" loosely; the spec is authoritative.)
- Anti-hoarding: demurrage + progressive fees fund a cluster-tilted lottery
  that biases redistribution against wealth concentration (Gini-style
  objective). This is the paper's most novel piece.

## Audience calibration

Technically curious readers who are NOT cryptographers — developers,
prospective node operators, informed users — who want to understand how Botho
works and why, before (or instead of) reading the formal paper. Assume
comfort with software concepts (keys, hashes, signatures at a "black box"
level) but not with elliptic curves, commitment schemes, or BFT literature.
