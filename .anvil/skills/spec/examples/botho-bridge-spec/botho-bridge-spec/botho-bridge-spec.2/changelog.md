# Changelog — botho-bridge-spec.2 (revised from .1)

Consumes both critic siblings of `botho-bridge-spec.1`:
- **review** (`botho-bridge-spec.1.review/`) — BLOCK, total **38/44** (< 39 audit-grade), **0 critical flags**.
- **audit** (`botho-bridge-spec.1.audit/`) — **audit_clean: TRUE**, 0 `implementation_contradicts_spec` critical flags, 0 `code-wrong` escalations, 0 `unregistered` gaps, 1 advisory `major` (M-1, tooling limitation).

Combined verdict: not terminal (review below threshold), no `code-wrong` block, iteration 1 of max 4 — proceed to revise. Every review deduction is prose/structure/LaTeX-mechanical and cheap to close; the audit was already clean because the target-state register rows are doing their job.

---

## Critic note → change (or reasoned decline)

### review — major: `\addlinespace` undefined in whitepaper preamble (not drop-in `\input`-able) — dim 7
**FIXED.** The whitepaper `preamble.tex` does not load `booktabs`; it redefines `\toprule`/`\midrule`/`\bottomrule` as `\hline` variants (`preamble.tex:112–114`) and defines no `\addlinespace`. Removed all 5 `\addlinespace` occurrences from the `## Implementation status` register table so it matches exactly how `sections/10-economics.tex` builds tables (plain `\toprule`/`\midrule`/`\bottomrule`, no inter-row spacer). No other table used `\addlinespace`. **Verified drop-in**: compiled the revised section via `\input{preamble}` + the real preamble; clean full compile (EXIT 0, 0 undefined control sequences, 8-page PDF) with only the expected missing-`exhibits/*.png` and forward `\ref` warnings.

### review — major: dangling caption reference "register row IMP-1" — dim 7 / audit nit
**FIXED.** Added stable row IDs `IMP-1`…`IMP-6` as a leading column to the register table, and added an explanatory sentence ("Rows carry stable IDs (IMP-1–IMP-6) so that figure captions and cross-references resolve to an exact row."). Corrected the fig2 caption to reference **IMP-2** (the actual "Import cluster tagging (ADR 0007)" row) — the v1 caption said "IMP-1", but with IDs assigned in table order IMP-1 is the Ethereum-path *live* row and IMP-2 is the import-tagging *target-state* row the caption is about. Also wired two other prose cross-refs to their rows: the demurrage-settlement on-ramp now cites row IMP-3, and the mainnet-gate paragraph now cites row IMP-6. The §Implementation-Status divergence note now cites "register row IMP-2".

### review — major (accumulates dim 1, NOT a flag): base-layer target-state privacy claim lacks a register row — §Privacy
**FIXED (reworded, no register row added).** The v1 sentence "amounts are (on the confidential-amounts roadmap) hidden in Pedersen commitments" read as a present-tense-adjacent bridge claim. Confidential amounts are a **base-layer** property, not a bridge component, and the live chain records **public** amounts. Rather than add a bridge register row for a non-bridge property (which would overload the register's bridge scope), reworded the claim to state plainly that confidential amounts are a base-layer target tracked at the base layer (ADR 0006 / §transactions, §economics; botho#902), that the live chain records public amounts, and that this is explicitly out of scope for this section's register. Accuracy preserved: live = public amounts, CT = target. The downstream "public-at-the-boundary" and "confidential-amounts-clean by construction" arguments (which rely on unwrap amounts being public) remain correct and consistent with this rewording.

### review — minor: RFC-2119 keyword discipline absent — dim 3
**FIXED.** Added a **Conformance keywords** paragraph near the section start (per RFC 2119) defining MUST / MUST NOT / SHALL / SHALL NOT / SHOULD / MAY and stating that a conformant implementation must satisfy every MUST/SHALL except where explicitly scoped target-state (in which case the register's Live/Target columns state the respective obligations). Lifted genuine normative obligations to uppercase at their normative points across §Custody (authorized by signatures MUST / MUST NOT rely on on-chain proof), §Threshold (≥ t distinct sigs, distinct-signer-counts-once, t ≥ t_SCP "MUST never be easier"), §Per-chain schemes (each authorization MUST), §Attestation/exactly-once (MUST be carried, MUST mint/release at most once, MUST revert on duplicate), §three-Safe (MUST be split, deployer MUST receive no roles, startup guard MUST refuse), §Peg (peg MUST hold; MUST mint only factor-1; non-factor-1 MUST be rejected / MUST never mint; releases MUST spend only factor-1; MAY settle-once-then-wrap), §Privacy (MUST learn amount, SHOULD warn, MUST pay to fresh stealth address), §Import (unwrap MUST mint into epoch cluster), §Security (MUST uphold all five invariants; MUST mint/release at most once; equivocating signer MUST count once; peg MUST hold / MUST trip breaker; action MUST fire only against final block; MUST trip breaker + emit alert; unavailable chain MUST be reported unverified; opsec MUST meet validator grade; no value MUST move on mainnet until audit clears).

### review — nit: `import_epoch_blocks` / `import_factor_floor` declared twice
**DECLINED (scoping deduction, arguable).** The review itself notes the gate treats these as benign matching re-declarations (0 violations) and this is a nit, not a flag. Kept both markers: the inline table-row `% anvil-const:` suffixes tie each constant to the table row it renders, and the standalone comment pair beneath the table keeps the constants grep-visible independent of the table markup. Identical value+unit, no drift risk. Retaining redundancy over a cosmetic reduction is the safer choice for a maintained-against-code spec; no scoring dimension is materially harmed.

### review — top-priority: render the three figures (caps dims 6 & 7 at 2)
**DEFERRED to spec-figures (by design, not declined).** Rendering figures is the next phase (`spec-figures` on v2), explicitly out of scope for this revise pass per the dispatch. The three figure references (`exhibits/fig1-wrap-mint-flow.png`, `fig2-unwrap-import-flow.png`, `fig3-federation-custody.png`) and the `metadata.figure_plan` (3 entries) are carried forward unchanged so `spec-figures` renders to the paths the revised body references. fig2's caption text was updated (IMP-1 → IMP-2) but its path/plan entry is unchanged.

### audit — major (advisory, non-blocking): M-1 — scalar `code_ref` glob does not reach counterparty contracts / cluster-tax simulation
**DECLINED — tooling limitation, not a spec issue.** M-1 is an anvil `code_ref`-resolution limitation (a scalar glob `bridge/**/*.rs` cannot express the multi-tree implementation this section spans — `WrappedBTH.sol`, the Solana Anchor program, and `cluster-tax/.../bridge_import_sweep.rs`). The auditor read those files manually per the BRIEF note and **every claim checked out** (C1, C4–C9, P9, P11, I1–I3 all `match`); the sweep is complete. Filed upstream as anvil#718/#724. No spec change is required or appropriate — the fix is operator-side BRIEF `code_ref` authoring / anvil tooling, not body editing.

### audit — import-tagging present-tense claims (I1/I2), demurrage-settlement (D1)
**NO CHANGE — registered intentional gaps, correctly suppressed.** The auditor confirmed these are registered target-state gaps (import tagging → row IMP-2 / botho#938 with exact Live/Target/Tracking match; demurrage-settlement → row IMP-3 / botho#831) and explicitly NOT critical flags, NOT escalations. Per the dispatch and the class discipline, the register rows and the import-tagging target-state framing (including the §Import "(target-state)" subsection headings and the §Implementation-Status divergence note quoting `bth_scan.rs`) are **preserved verbatim** — only the fig2 caption's row-ID pointer was corrected (IMP-1 → IMP-2) and the divergence note gained the IMP-2 pointer. No register row was weakened or removed.

---

## Preserved (review "What's working" list — not weakened)
- **Factor-1-only wrapping** rule + over-time peg-solvency argument (§Peg) — kept verbatim (only added RFC-2119 MUST/MAY at the surrounding obligations, not to the quoted rule block).
- **Target-state marking discipline** — the divergence note naming `bth_scan.rs` empty-cluster-tags vs the ADR-0007 target — kept verbatim (added only the IMP-2 pointer).
- **Threshold-floor invariant** `t ≥ t_SCP` with "never easier to move the reserve than to move consensus" (§Threshold) — kept, "must" → "MUST" for RFC-2119 discipline.
- **Three-Safe role split** (§three-Safe) — kept.
- **Exactly-once mint/release** invariant pair and **fail-closed** posture (§Security) — kept.

## Register rows + import-tagging gap framing: PRESERVED
All six register rows retained (now IMP-1…IMP-6); no Live/Target/Tracking text weakened. The import-tagging target-state framing (subsection headings, epoch equations, divergence note, botho#938 tracking) is intact.

## Delta (v1 → v2)
- Words (`wc -w` on the `.tex`): 3,209 → 3,418 (+209), driven by the RFC-2119 conformance clause and the base-layer CT rewording.
- Lines: 509 → 532 (+23).
- Structural: 5× `\addlinespace` removed; register table gained an ID column (5 → 6 columns) with IMP-1…IMP-6; fig2 caption IMP-1 → IMP-2; three prose row-cross-refs added (IMP-3, IMP-6, IMP-2 in divergence note).
