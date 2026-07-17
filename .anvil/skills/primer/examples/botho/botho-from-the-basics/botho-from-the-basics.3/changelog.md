# Changelog — botho-from-the-basics.2 → botho-from-the-basics.3

Consumes `botho-from-the-basics.2.review/` (ADVANCE, 43/44, zero critical
flags) and `botho-from-the-basics.2.audit/` (CLEAN, `audit_clean: true`,
zero critical flags). The combined verdict pre-check (primer-revise step 2)
would report AUDITED and exit without writing; **the operator explicitly
waived that early exit** with a single, narrow purpose: the deliverable is
the PDF, and the v1 `primer-figures` contract cannot place image references
in the body (its command says "reference the image" yet "never edit the
body" — a skill-design gap reported upstream). This v3 exists solely to
integrate the five already-rendered, audit-grounded teaching figures into
the body so the PDF render embeds them, plus the two one-clause acronym
glosses the v2 review left as its only remaining priority. Nothing else was
touched: no prose rewrites, no quantitative changes, no reordering.

The five figures were authored (v2 figures phase) from the AUDITED v2 body
using audit-verified claims only; their `.mmd` sources and PNGs are carried
forward unchanged from `botho-from-the-basics.2/exhibits/` into this
version's `exhibits/` so the body's relative references resolve. Image
references use pandoc's implicit-figure form (a paragraph containing only
the image), so the alt text becomes the rendered caption — the body had no
prior figure convention to match.

## Operator-directed figure integration

| # | Insertion | Placement rationale |
|---|---|---|
| G1 | `exhibits/fig1-stealth-address-flow.png` at §3, after "How the classical construction works" (immediately before the §3 teach-then-point blockquote) | The prose has just taught the full derive/scan/match/spend flow, including the "Botho swaps the handshake" forward-pointer to §6 that the diagram annotates. |
| G2 | `exhibits/fig2-hybrid-pq-envelope.png` at §6, after the design-rule paragraph ("permanent secrets get post-quantum protection; ephemeral secrets get efficient classical protection"), before "The post-quantum stealth address" | The diagram is the secrecy-lifetime split itself; the ML-KEM/ML-DSA/CLSAG boxes it shows are then unpacked by the three subsections that follow it. |
| G3 | `exhibits/fig3-scp-mining-decoupling.png` at the end of §8's "The load-bearing design decision: mining weight ≠ consensus weight" subsection, before "RandomX" | This is where "the wall" is taught — the diagram's center element. It sits at the §§7–8 conceptual boundary: SCP machinery (§7) on one side, the PoW issuance path (§8) on the other. |
| G4 | `exhibits/fig4-anti-hoarding-money-flow.png` at the end of §9.5, before §9.6 | Every element in the diagram (cluster factor → fees + demurrage → 20/80 burn/pool split → emission slice → tilted 4-UTXO lottery) has been introduced by this point and none earlier; §9.6 then argues *why* this shape, with the full picture in view. |
| G5 | `exhibits/fig5-capstone-payment-timeline.png` at the start of §10, after the framing paragraph, before step 1 | A map-before-the-walk: the reader sees the four phases (build / propagate / mine+finalize / aftermath) before the eleven steps, and can re-read it afterward as the recap. |

Self-discipline re-run on the placements: each figure appears only after
every concept it depicts has been taught (dependency-order walk holds); each
caption is one teaching sentence restating body claims — no new numbers, no
formal material, so the cross-reference-not-duplicate discipline is
unaffected; all caption claims are restatements of audit-verified v2 body
text (technical-accuracy check).

## Review notes (botho-from-the-basics.2.review/)

| # | Source | Note | Disposition |
|---|---|---|---|
| R1 | verdict priority 1a / comment 1 (minor, expand) | §8 "ASICs" never expanded at point of use | **Changed.** Adopted the reviewer's parenthetical verbatim: "hostile to ASICs (application-specific integrated circuits — chips custom-built for a single algorithm)". |
| R2 | verdict priority 1b / comment 2 (minor, expand) | §11 map table "TEE approaches" unglossed | **Changed.** Row now reads "trusted-execution-environment (TEE, secure-enclave) approaches" — glossed, acronym retained for whitepaper §2 findability. |
| R3 | verdict priority 2 ("Do not add further quantitative material") | Quantitative density at the teach-then-point ceiling | **Honored.** Zero new quantitative material; captions restate existing body numbers only. |
| R4 | comments 3–5 (nit, preserve) | Hedged demurrage anchor; 611M prose bridge; capstone fee instance | **Preserved** verbatim — none of the three passages was touched. |
| R5 | verdict "What's working" (v1 list + three v2 insertions + fee-leakage honesty) | Do-not-sand-off list | **Preserved** in full. The seven-hunk diff v2→v3 contains only the five figure paragraphs and the two acronym glosses; every flagged move (problems-first spine, recap-before-use, §9.6 graveyard, honest caveats, teach-then-point blockquotes, eleven-step capstone, the three v2 quantitative insertions, the fee-leakage caveat) is byte-identical. |

## Audit notes (botho-from-the-basics.2.audit/)

| # | Source | Note | Disposition |
|---|---|---|---|
| A1 | priority 1 (operator items: F2, F10, N1) | WP↔code ML-DSA-65 divergence; WP-internal 5s/3s and CLSAG-byte inconsistencies; 2-in-2-out size tension | **No primer change** — the audit itself states these are operator-side spec editorial items with no primer edit required; carried forward unchanged. |
| A2 | priority 2 (N2, minor, optional) | Optional half-clause hedge on "never how much" for factor >1 spends | **Declined — out of the operator-directed scope for this revision.** The audit rates the current text lossy-but-true ("not a flag"), and the v2 review lists the fee-leakage caveat on the do-not-sand-off list; left for a future operator-elected polish pass. |

Word count: 8,338 → 8,616 (+278, entirely the five caption sentences and
the two glosses).
