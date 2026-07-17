# Audit findings — botho-bridge-spec.2 (re-audit after revise + figures)

Per-claim table. `Kind`: factual (internal logic) | implementation-consistency.
`Verified?`: match | contradicts | unresolvable. `Disposition`: spec-wrong | code-wrong | intentional-gap | — (n/a).

This is a **re-audit** of v2. v1 was CLEAN (42 claims, disposition_counts spec_wrong 0 /
code_wrong 0 / intentional_gap 4 registered / unregistered 0). The v2 delta is
prose/structure/LaTeX-mechanical + figure-render: register row IDs (IMP-1…IMP-6), fig2
caption IMP-1→IMP-2, RFC-2119 keywords, base-layer CT reword, `\addlinespace` removed, 3
figures rendered. All prior claims re-swept against code; two new checks target the edits
whose truth value could have shifted (RFC-2119 uppercasing; the CT-claim reword) and the
rendered figure CONTENT.

## Constants (implementation-consistency)

| # | Claim | Kind | Verified? | Disposition | Evidence |
|---|---|---|---|---|---|
| C1 | wBTH 12 decimals; 1 base unit = 1 picocredit = 1 unit native BTH (no scaling) | impl-consistency | match | — | `WrappedBTH.sol:14/89/90` `DECIMALS=12` "1 wBTH base unit == 1 picocredit, 1:1"; `:174` `decimals()->12`; Solana `lib.rs:55` `mint::decimals = 12` |
| C2 | CLSAG ring size = 20 | impl-consistency | match | — | `transaction/clsag/src/lib.rs:571` `DEFAULT_RING_SIZE: usize = 20` |
| C3 | Threshold floor `t >= t_SCP` (symbolic) | impl-consistency | match | — | `release/bth.rs:101-103/131` threshold_floor "never lower than the SCP safety threshold"; ADR 0002; no numeric t pinned — spec correctly symbolic |
| C4 | Import epoch `K = 17,280` blocks (1 day) | impl-consistency | contradicts | intentional-gap (registered) | REGISTERED — IMP-2/#938. `bridge_import_sweep.rs:118` `blocks: 17_280` (simulation-only); 17280×5s=86400s=1day ✓. Production release path does not apply it (bth_scan.rs:218 empty tags). |
| C5 | Import-factor floor `F = 1.5x` | impl-consistency | contradicts | intentional-gap (registered) | REGISTERED — IMP-2/#938. `bridge_import_sweep.rs:140` candidate_floors incl. `1500` (FACTOR_SCALE units); simulation-only |
| C6 | Factor range [1x,6x]; identical log-sigmoid production curve, W_mid=100k, saturating 6x | impl-consistency | match | — | `bridge_import_sweep.rs:9` "the identical curve domestic clusters use"; reuses `crate::ClusterFactorCurve` |
| C7 | Demurrage charge = value*rate*(factor-1)/(max_factor-1)*elapsed/blocks_per_year | factual | match | — | `cluster-tax/src/demurrage.rs::demurrage_charge`; (factor-1)⇒factor-1 pays 0. Dimensionally sound. |
| C8 | "2M-BTH whale needs 541 epochs (541 days at K)" | factual | match | — | ADR 0007 §Calibration; sim epochs-to-floor |
| C9 | "≈9 domestic-mixing spends" to blend 6x flood to floor | factual | match | — | ADR 0007 §Calibration; sim decay_by_circulation (real TagVector::mix) |

## Protocol / mechanism claims (implementation-consistency)

| # | Claim | Kind | Verified? | Disposition | Evidence |
|---|---|---|---|---|---|
| P1 | Peg: Sum(wBTH ETH)+Sum(wBTH SOL) = locked BTH reserve | impl-consistency | match | — | `reserve.rs` reconcile; ADR 0003 |
| P2 | Exact peg, tolerance default 0 | impl-consistency | match | — | `reserve.rs` "default 0 — the ADR 0003 exact peg" (factor-1 pays 0 demurrage forever) |
| P3 | Domain tags attest-{eth,sol,bth}-v1; release-v1; mint-{eth,sol}-v1; pairwise-distinct + distinct from operator-action | impl-consistency | match | — | `core/src/attestation.rs:131/230/239/278/282/286` all verbatim; `attestation_domain()`; VERSION=1, SKEW=30 |
| P4 | Threshold auth; distinct-signer exact (equivocator counts once) — in `release/bth.rs::validate_release_attestation` | impl-consistency | match | — | `release/bth.rs:108/190` distinct-valid-signer >= threshold; #842 threshold-0 guard :258 |
| P5 | Equivocating signer flagged by dedicated audit event | impl-consistency | match | — | attestation equivocation outcome/audit (v1-verified; unchanged) |
| P6 | Exactly-once mint: 32-byte orderId; contract records + reverts on duplicate | impl-consistency | match | — | `WrappedBTH.sol:117/197-211` `processedOrders` "Order already processed" revert; Solana `lib.rs:36-40` order-marker PDA fails at `init` on duplicate |
| P7 | Exactly-once release: record signed tx before broadcast; re-broadcast reuses recorded tx | impl-consistency | match | — | `engine.rs` record_release_tx; `db.rs` release_claims UNIQUE on order_id_hash (v1-verified; unchanged) |
| P8 | Schemes: BTH release + Solana mint = Ed25519; Ethereum mint = secp256k1 via Gnosis Safe MINTER_ROLE | impl-consistency | match | — | `attestation.rs` scheme; `WrappedBTH.sol:86/166` MINTER_ROLE; `mint/ethereum.rs` SafeTx |
| P9 | Three-Safe split (MINTER/ADMIN/PAUSER distinct Safes; deployer no roles; t-of-n in Safe not token) | impl-consistency | match | — | `WrappedBTH.sol:35-46/152/165-167` distinct Safes, "deployer receives NO roles", threshold in Safe; Solana `lib.rs:26-33` three DISTINCT multisigs |
| P10 | Relayer EOA submits Safe.execTransaction w/ threshold sigs over EIP-712 SafeTx wrapping bridgeMint | impl-consistency | match | — | `mint/ethereum.rs` execTransaction + SafeTx (v1-verified; unchanged) |
| P11 | Solana authority = SPL/Squads multisig; startup guard refuses if on-chain authority == local relayer key | impl-consistency | match | — | `lib.rs:19-34/94-97` no single-key mint, multisig-only; `mint/solana.rs` startup custody guard |
| P12 | Factor-1-only wrapping; non-factor-1 deposit MUST be rejected w/ audit event, never mints | impl-consistency | match | — | `bth_scan.rs:126` `factor_one = explicit_cluster_weight()==0`; :409 tagged output !factor_one; scan flags non-factor-1 |
| P13 | Releases MUST spend ONLY factor-1 reserve outputs; change zero-demurrage to reserve | impl-consistency | match | — | `release/bth.rs:7/32/88` "spending only factor-1"; `bth_scan.rs:221` change to reserve default subaddress (ADR 0003) |
| P14 | Unwrap re-shields: fresh one-time stealth, never reused | impl-consistency | match | — | `bth_scan.rs:18/168-169/215` FRESH one-time stealth output (ADR 0004) |
| P15 | Lock reveals amount (leaks amount not source ring); wBTH public | factual | match | — | ADR 0004; transparent-amount model; consistent with §Privacy |
| P16 | Proof-of-reserves: both drift bounds; drift/shortfall trips fail-closed breaker+alert; unverified chain excluded, never healthy | impl-consistency | match | — | `reserve.rs` reconcile_once + unverified_status (v1-verified; unchanged) |
| P17 | Finality: SCP on BTH; depth+canonical on ETH; Finalized on Solana | impl-consistency | match | — | `release/bth.rs:41-42` check_confirmation (0=SCP finality); watchers/ethereum + watchers/solana |
| P18 | Auto-pause on-chain breaker + two-layer (tight service cap first, looser on-chain last-resort) | impl-consistency | match | — | `WrappedBTH.sol:221` `_pause()` auto-pause; Solana `lib.rs:63` auto_pause_threshold; engine backlog cap |
| P19 | Open ERC20 burn removed; only bridgeBurn emits BridgeBurn | factual | match | — | `WrappedBTH.sol:23-29/234-242` bridgeBurn sole burn path emitting BridgeBurn |
| P20 | Envelope single-use/freshness (nonce, expiry, skew, max lifetime); v1 pinned | factual | match | — | `attestation.rs:290/298` VERSION=1, SKEW=30 (v1-verified; unchanged) |

## Import-tagging present-tense claims (the load-bearing register case — IMP-2)

| # | Claim | Kind | Verified? | Disposition | Evidence |
|---|---|---|---|---|---|
| I1 | Unwrap mints into epoch import cluster c_import(m)=H("bridge-import"‖m), m=floor(h/K), import_factor(m)=max(F,Curve(Σ epoch unwraps)) | impl-consistency | contradicts | intentional-gap (registered) | REGISTERED — suppressed by row **IMP-2** ("Import cluster tagging (ADR 0007)"): Live=factor-1/empty tags matches `bth_scan.rs:218` `debug_assert!(recipient_output.cluster_tags.is_empty())`; Target=c_import>=F matches ADR 0007 + `bridge_import_sweep.rs:5-14` (SIMULATION ONLY); Tracking=botho#938. NOT a critical flag. |
| I2 | Output carries 100%-weight tag to c_import(m) as a mint output does to its cluster | impl-consistency | contradicts | intentional-gap (registered) | REGISTERED — same row **IMP-2**/#938. §Import subsections titled "(target-state)"; fig2 caption + fig2 rendered note now correctly cite **IMP-2** (v1 said IMP-1). Divergence note (§Impl-Status) cites IMP-2 + quotes `bth_scan.rs`. Suppressed. |
| I3 | Confidential-amounts-clean: import factor from public boundary amounts, no ZK gadget | factual | match | — | ADR 0007; amounts public at boundary (ADR 0004 / P15); `bridge_import_sweep.rs` computes factor from Σ public unwrap amounts. Internally consistent. |

## Demurrage-settlement on-ramp (second registered target-state item — IMP-3)

| # | Claim | Kind | Verified? | Disposition | Evidence |
|---|---|---|---|---|---|
| D1 | Demurrage-settlement op (pay to reclassify to factor-1; fee→lottery pool) — explicitly target-state | impl-consistency | contradicts | intentional-gap (registered) | REGISTERED — row **IMP-3** Live="No consensus settlement op"/Target=paid reclassification/Tracking=botho#831 (horizon #833/#925). No consensus settlement transaction op in `bridge/` or ledger; "settlement" hits are ONLY calibration sims (`cluster-tax/src/simulation/settlement_horizon_sweep.rs` + its CLI). Suppressed. |

## v2-edit truth-value checks (new this re-audit)

| # | Edit | Kind | Verified? | Disposition | Evidence |
|---|---|---|---|---|---|
| V1 | RFC-2119 uppercasing (must→MUST/SHALL/SHOULD/MAY across §Custody/Threshold/Peg/Privacy/Import/Security) did NOT invert any obligation's truth value | factual | match | — | Spot-checked the load-bearing obligations: "MUST mint only factor-1"/"non-factor-1 MUST be rejected" (P12), "releases MUST spend only factor-1" (P13), "MUST NOT rely on on-chain proof" (ADR 0002 / P8-custody), "distinct signer MUST count once" (P4), "no value MUST move on mainnet until audit" (IMP-6). All were already `match`; uppercasing is stylistic, semantics unchanged. |
| V2 | Base-layer CT-claim reword ("live chain records PUBLIC amounts; confidential amounts are a base-layer target tracked at base layer, out of this section's register") is TRUE of the code | impl-consistency | match | — | `transaction/clsag/src/lib.rs:597` "amounts are public", `:654` "current transparent-amount model (trivial zero-blinding Pedersen)", `:712` "amounts are public". The v2 reword CORRECTED v1's Pedersen-adjacent phrasing to match live public-amount reality; introduced no contradiction; correctly scoped OUT of the bridge register (base-layer property, ADR 0006 / botho#902). |

## Figure content audit (rendered PNGs — new this re-audit; figure content is in scope)

| Fig | Claim made by the rendered image | Verified? | Evidence |
|---|---|---|---|
| fig1 (wrap/lock→mint) | factor-1 deposit lock → await SCP finality → confirmed final deposit → collect t-of-n attestation (domain-separated, order-bound) → bridgeMint(to,amount,orderId) → wBTH minted (exactly-once/orderId); **alt branch**: non-factor-1 "Rejected before any mint (audit event, never mints)" | match | Matches §Peg prose + code: bth_scan factor-1 gate (P12), WrappedBTH.sol bridgeMint/orderId exactly-once (P6/P10), release/bth SCP finality (P17). No contradiction with prose. |
| fig2 (unwrap→release/import) | burn wBTH → confirm (depth window / Finalized) → t-of-n release attestation (order-bound) → release reserve BTH → release to fresh stealth address → BTH received. **Two notes**: "Target-state: tag 100% to epoch import cluster c_import(m) at import_factor(m) >= F" AND "Live (register row IMP-2): releases a factor-1 output" | match | The load-bearing figure. The Live/Target split is drawn correctly and the IMP-2 pointer is now correct (v1 body/caption said IMP-1). Live-note matches `bth_scan.rs:218` empty tags = factor-1; Target-note matches ADR 0007 / sim eqs. The figure REINFORCES the register rather than contradicting prose. |
| fig3 (federation custody) | 3 SCP validators each "Ed25519 + secp256k1" → aggregated t-of-n attestation set → {Ethereum mint (secp256k1) → minterSafe MINTER_ROLE; BTH release (Ed25519) → BTH reserve release; Solana mint (Ed25519) → Solana SPL/Squads mint authority}; **Ethereum three-Safe split**: minterSafe MINTER_ROLE / adminSafe role admin / pauserSafe PAUSER_ROLE | match | Matches §Custody per-chain scheme table (P8) + three-Safe table (P9) exactly: Ed25519 for BTH-release+Solana-mint, secp256k1 for Ethereum, three distinct Safes, deployer implicit-absent. No contradiction with prose or code. |

## Register-row Live/Target accuracy (audited from the code side)

| Register row | Live accurate? | Target accurate? | Evidence |
|---|---|---|---|
| IMP-1 Ethereum wrap/unwrap = live | YES | YES (same) | mint/ethereum + watchers/ethereum + release/bth + WrappedBTH.sol all live; exactly-once ledger+contract; factor-1 peg |
| IMP-2 Import cluster tagging = target-state | YES (bth_scan.rs:218 empty tags = factor-1, asserted) | YES (matches ADR 0007) | I1/I2; mechanism only in bridge_import_sweep.rs simulation |
| IMP-3 Demurrage-settlement = target-state | YES (no consensus op) | YES | D1 |
| IMP-4 Solana transports = target-state | YES (code-complete, live-node #[ignore]d, unverified e2e) | YES | mint/solana + watchers/solana wired; live integration ignored |
| IMP-5 BTH/Solana live supply+reserve-balance = target-state | YES (unverified, excluded from drift) | YES | reserve.rs unverified→excluded |
| IMP-6 External security audit = target-state (mainnet gate) | YES | YES | internal adversarial/chaos/fuzz only |

All six rows carry stable IDs IMP-1…IMP-6 (v2 addition). Every prose/figure cross-ref
resolves: §demurrage on-ramp→IMP-3, §mainnet gate→IMP-6, §divergence note→IMP-2, fig2
caption+note→IMP-2. No dangling row reference remains (v1's fig2 "IMP-1" dangler fixed).

## Major findings

| # | Finding | Severity | Note |
|---|---|---|---|
| M-1 | `code_ref` scalar glob `bridge/**/*.rs` (35 files) does not resolve the counterparty contracts (`WrappedBTH.sol`, Solana `lib.rs`) nor `cluster-tax/.../bridge_import_sweep.rs` that this section normatively describes (C1,C4-C6; P6/P8/P9/P11; I1-I3; V2). | major (advisory, non-blocking) | CARRIED FROM v1 unchanged — read manually per BRIEF note; sweep complete, all matched. Track anvil#718 (scalar `code_ref` cannot span a multi-tree implementation). Not a spec defect, not a blocking flag. Operator-side BRIEF/anvil-tooling fix. |

## Internal-logic / factual audit

- Peg / demurrage / epoch-cluster-import-factor equations (eqs 1–7): dimensionally sound, mutually consistent, consistent with the §Impl-Status register. No same-named-constant drift across sections (`import_epoch_blocks` 17280/blocks + `import_factor_floor` 1.5/x each declared twice with IDENTICAL value+unit — benign re-declaration, same disposition as v1).
- No unsatisfiable predicate, no misused primitive, no dimensionally-unsound formula.
- The v2 edits (row IDs, fig2 IMP-2 fix, RFC-2119 uppercasing, CT reword, `\addlinespace` removal) introduced NO new contradiction. The two edits capable of shifting a truth value (V1, V2) were explicitly re-checked and both hold.
