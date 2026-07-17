# Findings — botho-from-the-basics.3 (audit)

Spec oracle: `spec_ref: ../sections/*.tex` → 18 files under `whitepaper/sections/` (resolved, active).

**Scope**: the v3 delta from the clean-audited v2 is exactly 7 hunks (diff re-run by this auditor): five implicit-figure paragraphs + two acronym glosses. Unchanged prose carries the v2 clean verdict (`botho-from-the-basics.2.audit/findings.md`: delta table D1–D18, carried sample S1–S15, all verified). Rows below cover the NEW claim surface — the five captions and the five rendered diagrams (all PNGs visually inspected against their `.mmd` sources) plus the two glosses. Where a figure claim is identical to a v2-verified claim, the v2 row is cited rather than re-derived.

Severity legend: **critical** = fires an audit flag (none); **minor** = precision note, no false belief created; **obs** = observation / operator-facing.

## Diff verification

| # | Check | Result |
|---|---|---|
| V1 | `diff` v2 body vs v3 body | **7 hunks exactly**: fig1 insert (after §3 classical-construction ¶), fig2 insert (after §6 design-rule ¶), fig3 insert (§8 wall subsection end) + ASIC gloss (§8 RandomX ¶), fig4 insert (§9.5 end), fig5 insert (§10 head), TEE gloss (§11 map table). No other change; word delta is captions + glosses only. v2 clean verdict carries for all remaining prose. |
| V2 | PNGs vs `.mmd` sources | All five PNGs render their sources faithfully; no divergent text between rendered diagram and source. |

## Per-claim table — Figure 1 (stealth-address flow, §3)

| # | Claim | Kind | Verified? | Evidence / cited source |
|---|---|---|---|---|
| F1a | Caption: sender derives fresh one-time output key from recipient's two published keys via one-way handshake (classical DH in shape; ML-KEM in Botho, §6); recipient redoes handshake with view key; others see unlinkable random-looking key | factual + spec | **verified** | Restates body §3 "How the classical construction works" (unchanged, v2-audited); WP §4 Key Hierarchy + Post-Quantum Stealth Addresses; hybrid table `04-cryptography.tex:326–343` |
| F1b | Diagram: address = public view key (recognizing) + public spend key (spending), published once | spec | **verified** | Body §3 + capstone framing ¶ (unchanged); WP §4 key hierarchy |
| F1c | Diagram: on-chain output = one-time key + amount commitment + KEM ciphertext | spec-consistency | **verified** | WP §5 byte table row "Output × 2 (commitment, key, ciphertext)" (`05-transactions.tex:420`) |
| F1d | Diagram: match ⇒ decrypt the amount + derive one-time private key to spend; no match ⇒ unlinkable | factual | **verified** | Body capstone steps 3 & 11 (encrypted amount field, one-time private key); WP §4 |
| F1e | Diagram: one-time key "never existed before, never recurs" | factual | **verified-with-simplification** | Standard uniqueness-with-overwhelming-probability compression; body uses identical phrasing (v2-audited) |

## Per-claim table — Figure 2 (hybrid PQ envelope, §6)

| # | Claim | Kind | Verified? | Evidence / cited source |
|---|---|---|---|---|
| F2a | Caption + diagram: permanent secrets (recipient identity → ML-KEM-768; minting attribution → ML-DSA-65) get lattice PQ; sender anonymity decays → classical CLSAG | spec-consistency | **verified** | WP §4 rationale table verbatim (`04-cryptography.tex:326–343`): Recipient identity/Permanent/ML-KEM-768; Sender identity/Ephemeral/CLSAG; Minting authority/Permanent/ML-DSA-65 |
| F2b | Diagram: threat = "harvest now, decrypt later"; chain = permanent public archive whose secrets must survive future cryptanalysis | factual | **verified** | Body §6 (unchanged, v2-audited); WP §4 "Why not full post-quantum?" + §1 threat framing |
| F2c | Diagram: ML-KEM-768 1,088-byte ciphertext per output — "the biggest line item, priced in" | spec-consistency | **verified** | v2 audit S2 (`03-preliminaries.tex:145`, `04-cryptography.tex:91`); largest single component per §5 byte table (output triple 1,152 B each vs CLSAG 704 B) |
| F2d | Diagram: ML-DSA-65 ~3.3 KB "but only once per block" | spec-consistency | **verified** | v2 S3 (3,309 B); minting-only role `04-cryptography.tex:314–316`; one minting tx per block (`05-transactions.tex:133`). WP↔code divergence on the role = carried operator obs O1 — primer follows the declared oracle |
| F2e | Diagram: CLSAG ~700 B/input; PQ rings ~50× larger today; desktop and phone nodes would stop being realistic | spec-consistency | **verified** (phone clause = verified-with-simplification) | `04-cryptography.tex:345–350` ("~700 bytes", "50× size overhead", "~35 KB", ">100 KB, making desktop nodes impractical"); `05-transactions.tex:430–431` (v2 S1/S4). "Phone" extends the spec's "desktop" — already in v2-audited body line 439–440, consistent with §8 light-client design; lossy-but-true |
| F2f | Diagram: sender identity "decades stale before a quantum computer could recover it" | spec-consistency | **verified** | WP §4 "Why is ephemeral sender privacy acceptable?" — the 2025→2045 (two-decade) framing (`04-cryptography.tex:352–357`) |
| F2g | Caption + diagram: "documented migration path" to swap in PQ ring signatures when research shrinks them | spec-consistency | **verified** | `11-implementation.tex:419–421` ("Post-Quantum Ring Signatures … Future protocol upgrade may introduce…"); `13-conclusion.tex:72–73`; body line 447 (unchanged) |

## Per-claim table — Figure 3 (SCP/mining decoupling, §§7–8)

| # | Claim | Kind | Verified? | Evidence / cited source |
|---|---|---|---|---|
| F3a | Caption: RandomX PoW meters out new coins + right to propose; operator-chosen quorum slices give deterministic finality (halt, don't fork); hashpower buys zero consensus votes; candidate block crosses the wall, voting power never does | spec-consistency | **verified** | Body §8 wall subsection (unchanged, v2-audited); WP §6 consensus/issuance split; `06-consensus.tex:105–107` (halting preserves safety) |
| F3b | Diagram: operators declare quorum slices (local, subjective); overlapping slices form quorums; "any two quorums share an honest node" | spec-consistency | **verified-with-simplification** | Spec wording verbatim at `06-consensus.tex:296` ("any two quorums share at least one honest node"); stated in spec as the safety condition (quorum intersection + Byzantine threshold not exceeded, `:92–94`). Diagram box compresses the assumption into a property — the same compression the spec's own summary and the unchanged body (§7, lines 503–509) make; carried-away belief correct |
| F3c | Diagram: Nominate → ballot → externalize | spec-consistency | **verified** | WP §6 phases (Nomination, Ballot, Externalize `06-consensus.tex:205–217`) |
| F3d | Diagram: "FINAL, seconds after proposal. No reorgs of externalized blocks, ever" | spec-consistency | **verified** | `06-consensus.tex:357` ("Once a block is externalized at height h, no reorganization can replace…"), `:444` ("Once externalized, blocks cannot be reverted"); latency table `:397` (Externalize <1 s); body step 9 "handful of seconds" |
| F3e | Diagram: "No quorum? Halt, don't fork (safety over liveness)" | spec-consistency | **verified** | `06-consensus.tex:105–107` ("preserves safety by halting rather than…"); Liveness §`:407–` |
| F3f | Diagram: "hashpower buys ZERO consensus votes, and coin ownership buys zero too (not proof-of-stake). 51% of hashrate cannot rewrite history" | spec-consistency | **verified** | WP §9 "51% Attack Resistance" (`09-security.tex:409–424`): majority hashpower can propose more frequently, cannot force finalization; `:690` ("Even with majority hashpower, attacker cannot force quorum to finalize different block"); quorum-based tolerance (`06-consensus.tex:467`); no-stake claim = unchanged body §8 (v2-audited) |
| F3g | Diagram: winner's lottery ticket weighted by computation — no ASIC edge, linear rewards | spec-consistency | **verified** | v2 D16 (`07-monetary.tex:430, :434`) |
| F3h | Diagram: proposer claims the block reward, "founding a new cluster" | spec-consistency | **verified** | `05-transactions.tex:133` ("Each minting transaction creates a new cluster"), `:140`, `:196`; body step 8 (unchanged) |

## Per-claim table — Figure 4 (anti-hoarding money flow, §9)

| # | Claim | Kind | Verified? | Evidence / cited source |
|---|---|---|---|---|
| F4a | Caption + diagram: cluster factor from the lineage's wealth, not the holder; 1× small/diffuse · 3.5× at ~100,000 BTH · past 5× toward the 6× ceiling for million-BTH lineages | spec-consistency | **verified** | v2 D11/D12/S13 (`05-transactions.tex:216–237`) |
| F4b | Diagram: progressive fees = per-byte base rate × cluster factor; factor-1 users pay nano-BTH | factual | **verified-with-simplification** | Full law f_min = b_dyn × size × φ + d plus output penalty (`07-monetary.tex:244–248, :387–391`); diagram omits penalty/d but shows demurrage as its own box and body capstone teaches the full instance (v2 D9: 20 nano at factor 1). Lossy-but-true |
| F4c | Diagram: demurrage when idle high-factor coins move — ~2% of value after a year idle at factor 6; factor-1 coins exempt | factual + spec | **verified** | v2 D1/D3 (`10-economics.tex:286`, `07-monetary.tex:254`, `cluster-tax/src/demurrage.rs`) |
| F4d | Diagram: "miners never receive fees — no MEV motive" | spec-consistency | **verified** | `07-monetary.tex:407` ("minter_income = R(h) (no fees)"), `:304` ("Anti-MEV: No miner incentive to reorder transactions"); `10-economics.tex:27–29` (MEV elimination), `:259` (fee destination: burned, not miners) |
| F4e | Diagram: 20% burned ("supply shrinks, a rebate to every holder equally") / 80% to lottery pool | spec-consistency | **verified** (rebate framing = verified-with-simplification) | v2 S5 (`07-monetary.tex:270–291`); "rebate to every holder equally" is the body §9.5 sentence verbatim (line 793–794, v2-audited) — pro-rata deflation framing, lossy-but-true |
| F4f | Diagram: scheduled slice of block reward ramping to 50% — "reaches wealth that never transacts" | spec-consistency | **verified** | v2 S6 (`07-monetary.tex:281, :502`); redistribution-without-fee-volume rationale = body §9.5 (unchanged) |
| F4g | Caption + diagram: per-block lottery of 4 randomly selected UTXOs, seeded by the previous finalized block's hash; odds = value × inverse-factor tilt | spec-consistency | **verified** | v2 S5/S7/S8 (`07-monetary.tex:270–291, :328–346`; `09-security.tex:729–739`) |
| F4h | Diagram: per BTH, factor-1 coin gets 6× the winning weight of a factor-6 coin; "splitting into a million UTXOs changes nothing" | spec-consistency | **verified** | v2 S8 (E[income] ∝ v(φ_max − φ + 1) — linear in v ⇒ split-invariant); body §9.5 states the same with the dust-floor/maturity parenthetical (unchanged) |

## Per-claim table — Figure 5 (capstone timeline, §10)

| # | Claim | Kind | Verified? | Evidence / cited source |
|---|---|---|---|---|
| F5a | Caption: four phases — build (1–6), propagate (7), mine+finalize (8–9), aftermath (10–11) | factual | **verified** | Structural restatement of the body's eleven steps; matches exactly |
| F5b | Diagram: inputs 30 + 25 BTH; 40 BTH to Ben + ~15 BTH change (indistinguishable on chain); factor 1 → 20 nano-BTH fee, no demurrage | factual + spec | **verified** | Body steps 1–3 exact match; arithmetic 55 − 40 − 2×10⁻⁸ ≈ 15 ✓; v2 D9 (fee), D3 (exemption) |
| F5c | Diagram: Pedersen commitments + one aggregated Bulletproof | spec-consistency | **verified** | Body step 4; WP §5 byte table "Bulletproof (2 outputs, aggregated)" (`05-transactions.tex:422`) |
| F5d | Diagram: two CLSAG rings of 20 with key images (no double spend) | spec-consistency | **verified** | Body step 5; v2 D15/S1 (ring size 20; key images) |
| F5e | Diagram: outputs inherit the value-weighted blend of input cluster tags | spec-consistency | **verified** | Body step 6; WP §5 tag propagation (v2 sweep) |
| F5f | Diagram: Dandelion++ stem → fluff; every node validates the ~5 KB tx | spec-consistency | **verified-with-simplification** | Body step 7; WP §8; ~5 KB = v2 D10 (N1 WP-internal size tension carried — magnitude-consistent with all spec statements) |
| F5g | Diagram: RandomX winner assembles block + ML-DSA-65 minting transaction | spec-consistency | **verified** | Body step 8; `04-cryptography.tex:314–316` |
| F5h | Diagram: SCP nominates, ballots, externalizes — FINAL seconds after proposal | spec-consistency | **verified** | Body step 9; `06-consensus.tex:397` |
| F5i | Diagram: fees split 20% burn / 80% pool; 4 lottery winners drawn, tilted small | spec-consistency | **verified** | Body step 10; v2 S5/S8 |
| F5j | Diagram: Ben's wallet decapsulates, matches — "mine — 40 BTH, quantum-safe forever" | factual | **verified-with-simplification** | Body step 11: quantum adversary replaying the chain cannot link the output to Ben's address (handshake is PQ). "Quantum-safe forever" is scoped by context to recipient linkage — which is the PQ-protected property (tab:hybrid); amounts are information-theoretically hidden (Pedersen). Carried-away belief (Ben's ownership stays hidden from future quantum adversaries) correct. Lossy-but-true |

## Per-claim table — Glosses

| # | Claim | Kind | Verified? | Evidence / cited source |
|---|---|---|---|---|
| X1 | §8: "ASICs (application-specific integrated circuits — chips custom-built for a single algorithm)" | factual | **verified** | Correct expansion; accurate one-clause definition; adopted verbatim from the v2 review's suggested parenthetical |
| X2 | §11 map: "trusted-execution-environment (TEE, secure-enclave) approaches → §2 Related Work" | factual + spec | **verified** | Correct expansion; matches WP §2's own terminology ("privacy through trusted execution environments (TEEs)", `02-related-work.tex:159–162`; Intel SGX secure enclaves `:56–75`) — the gloss preserves findability as intended |

## Non-flag findings

| ID | Severity | Finding |
|---|---|---|
| N1 | minor (carried from v2) | Fig5's "~5 KB tx" restates the body's hedged size; the WP-internal 2-in-2-out size tension (appendix "<5 KB" vs §5 byte table vs §7's 4,000-B instance) remains a whitepaper editorial item, not a primer/figure defect. |
| N2 | minor (carried from v2, unchanged) | The capstone summary's absolute "never how much" is untouched in v3 (out of the revision's operator-directed scope); the figures do not repeat the phrasing, so v3 adds no new exposure. Optional polish remains available. |
| O1 | obs (operator, carried from v1/v2) | WP §4 "Minting Signatures" vs live-code divergence on ML-DSA-65's role. Fig2 ("only once per block") and fig5 (minting transaction) follow the declared spec oracle, as the body does; reconciliation stays operator-side. |
| O2 | obs (operator, carried from v1/v2) | WP-internal inconsistencies unchanged (§10 "Min block time 5s" vs §7's 3 s floor; §2 CLSAG byte figure). No figure claim touches the divergent statements. |

**Majors: none.** (`spec_ref` is declared and resolves — the missing-spec major does not apply.)
**Critical flags: none.** `audit_clean: true`.
