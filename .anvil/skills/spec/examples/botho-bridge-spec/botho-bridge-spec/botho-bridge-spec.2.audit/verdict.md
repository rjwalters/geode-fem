# Audit verdict — botho-bridge-spec.2 (re-audit after revise + figures)

**audit_clean: TRUE**

- **Critical flags: none.** Zero `implementation_contradicts_spec` critical flags.
  - `code-wrong` escalations: **0**
  - `unregistered` intentional-gap flags: **0**
  - `spec-wrong` flags: **0**
- **Major findings: 1** (M-1, advisory, non-blocking — carried from v1; scalar `code_ref` glob tooling limitation, anvil#718).

## code_ref resolution

Active and resolved: `bridge/**/*.rs` → **35 files** (scalar glob per BRIEF). The
spec↔implementation consistency sweep RAN. Per the BRIEF note, four additional files the
section normatively describes but which the scalar glob cannot reach were consulted
manually: `contracts/ethereum/contracts/WrappedBTH.sol`,
`contracts/solana/programs/wbth/src/lib.rs`,
`cluster-tax/src/simulation/bridge_import_sweep.rs`, and — for the release→factor-1 path —
`bridge/service/src/bth_scan.rs` (in-glob). Also `transaction/clsag/src/lib.rs` (ring size,
public amounts) and `cluster-tax/src/demurrage.rs`.

## Sweep result

**44 claims checked, 0 blocking contradictions.** Same clean posture as v1, re-verified
after the v2 edits.

- **spec-wrong: 0**, **code-wrong: 0**, **unregistered: 0**.
- **intentional-gap (all registered): 4** — the bridge-import-tagging set (I1, I2 + the two
  constants C4/C5 that fold under it) and the demurrage-settlement op (D1). Every one is
  register-suppressed with exact Live/Target/Tracking matches (IMP-2/#938 and IMP-3/#831).
  Register-suppressed gaps are clean passes, NOT contradictions that block.

## The import-tagging gap disposition (the load-bearing case)

**Still a correctly-registered intentional-gap. Confirmed NOT code-wrong.**

The import-factor machinery (c_import(m), import_factor(m), K=17,280, F=1.5×) exists ONLY
in the calibration simulation `cluster-tax/src/simulation/bridge_import_sweep.rs`. The
production unwrap/release path `bridge/service/src/bth_scan.rs:218` emits a recipient
output with EMPTY cluster tags — i.e. factor-1 — and asserts it
(`debug_assert!(recipient_output.cluster_tags.is_empty())`). This exactly matches register
row **IMP-2**: Live = "Unwrap releases a factor-1 / background output (bth_scan.rs recipient
output carries empty cluster tags)"; Target = "tags 100% to c_import(m) at import_factor(m)
>= F"; Tracking = botho#938. Because the Live column matches the code and the Target column
matches the spec claim, the contradiction is **suppressed** — the register is doing exactly
its job. This is NOT the botho near-miss shape: the code is not a vestigial path being
canonized; it is the acknowledged live behavior with a ratified ADR-0007 target and a
tracking issue. No escalation, no spec rewrite.

## v2-edit verification (no new contradiction introduced)

The v2 delta is prose/structure/LaTeX-mechanical plus figure-render. Two edits could in
principle have shifted a claim's truth value; both were explicitly re-checked:

1. **RFC-2119 uppercasing (V1)** — must→MUST/SHALL/SHOULD/MAY across the normative points.
   Stylistic; every affected obligation was already `match` and remains so. No inversion.
2. **Base-layer CT-claim reword (V2)** — the reworded claim ("the live chain records PUBLIC
   amounts; confidential amounts are a base-layer target tracked at the base layer") is
   TRUE of the code (`transaction/clsag/src/lib.rs:597/654/712` — public / trivial-zero-
   blinding transparent-amount model). The reword CORRECTED v1's Pedersen-adjacent phrasing
   toward the live reality and is correctly scoped OUT of this section's register (base-layer
   property, ADR 0006 / botho#902). No contradiction.

The remaining edits are non-semantic: register row IDs IMP-1…IMP-6 added (all cross-refs
resolve; v1's dangling fig2 "IMP-1" pointer corrected to IMP-2); `\addlinespace` removed
(compile-only); const-marker re-declarations unchanged (benign, identical value+unit).

## Figure content verification (rendered PNGs are in scope)

All three rendered figures were audited against both the prose and the code:

- **fig1** (wrap/lock→mint): factor-1 lock → SCP finality → t-of-n attest → bridgeMint(to,
  amount,orderId) → wBTH minted exactly-once; **alt branch draws the non-factor-1 rejection**
  ("Rejected before any mint, audit event, never mints"). Matches P12/P6/P17. No contradiction.
- **fig2** (unwrap→release/import): the load-bearing figure. Draws the Live/Target split with
  two notes — Target "tag 100% to c_import(m) at import_factor(m) >= F" and **"Live (register
  row IMP-2): releases a factor-1 output"**. The IMP-2 pointer is correct (v1 had IMP-1).
  Live-note matches bth_scan.rs:218; Target-note matches ADR 0007 / sim. Reinforces the
  register.
- **fig3** (federation custody): validators each Ed25519 + secp256k1 → aggregated t-of-n →
  {Ethereum mint secp256k1 → minterSafe MINTER_ROLE; BTH release Ed25519; Solana mint Ed25519
  → SPL/Squads}; Ethereum **three-Safe split** minterSafe/adminSafe/pauserSafe. Matches the
  §Custody per-chain-scheme table (P8) and three-Safe table (P9) exactly.

No figure contradicts the normative prose or the code.

## Audit priorities (for the reviser / operator)

1. (none) — no `implementation_contradicts_spec` critical flag of any disposition.
2. M-1 (major, advisory): scalar `code_ref` glob does not reach the counterparty contracts /
   simulation. Operator/tooling fix (anvil#718); auditor consulted them manually and every
   claim matched. Not a spec change.

**Conclusion:** the v2 spec is audit-clean. The advance gate on the audit side is satisfied
(no unresolved audit critical flag). Terminal advance still requires the parallel
`spec-review` to clear ≥39/44 (v1 review was 38/44 BLOCK on prose/structure; v2 revise
targeted those deductions and the figures are now rendered — a fresh `spec-review` scores that).
