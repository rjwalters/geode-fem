# Verdict — botho-bridge-spec.2 (spec-review)

**Reviewer scope**: prose / structure / consistency / normative correctness (by judgment against the resolved `code_ref`). The parallel `spec-audit` owns the exhaustive code↔spec factual cross-check and the three-way `implementation_contradicts_spec` disposition — not duplicated here.

## Decision

**ADVANCE.** Total **44/44** (≥ 39 audit-grade threshold). Review critical flags: **none**.

`advance: true` — total is at ceiling and there are zero unresolved review critical flags. This is contingent on a clean audit sibling (`spec-audit` combines at revise/publish time); v1's audit was already clean (0 flags, 1 declined advisory M-1). Every v1 review deduction has been closed: figures render (dims 6/7 recover from the caps), RFC-2119 discipline is in (dim 3), the `\addlinespace`/IMP-row drop-in defects are fixed (dim 7), and the base-layer CT claim is reworded to an explicit target cross-ref (dim 1).

## Constant-consistency gate (step 3b)

`check_constant_consistency_multi({botho-bridge-spec.tex: ...})` → **found=true, 7 declarations, 0 violations, passed=true**. Five distinct constants (`wbth_decimals=12`, `bridge_threshold_floor=t_scp`, `ring_size=20`, `import_epoch_blocks=17280`, `import_factor_floor=1.5`); `import_epoch_blocks` and `import_factor_floor` are each re-declared once with identical value+unit (the inline table-row suffix plus a standalone comment beneath the table) — benign, no `value-mismatch`. **No Self-contradiction flag fires from the mechanical half**, and the judgment sweep found no unmarked prose-level drift. Identical to the v1 gate result (the nit-declined double-declaration was retained, correctly, as a benign redundancy).

## Figure-exhibit gate (step 4c)

**Clean — dims 6 and 7 are NOT capped.** All three referenced exhibits exist and are fresher than the body: `exhibits/fig1-wrap-mint-flow.png`, `exhibits/fig2-unwrap-import-flow.png`, `exhibits/fig3-federation-custody.png` (mtime 19:28:03) vs the body `.tex` (mtime 19:23:11). This is the load-bearing delta from v1, where the missing `exhibits/` capped dims 6/7 at 2. Captions are accurate against the diagrams they label, including the target-state annotation on fig2 ("the current implementation releases a factor-1 output (register row IMP-2)").

## Critical flags

Critical flags: none.

- **Self-contradiction (flag 1)**: not raised. Gate clean; the peg invariant `\eqref{eq:bridge:peg}` and the factor-1/zero-demurrage predicate are each stated once and reused by reference across §Peg / §Privacy / §Security with no incompatible restatement.
- **Undefined normative term (flag 2)**: not raised. Every term in a normative obligation is defined at or before first normative use — `t_{\mathrm{SCP}}` ("$t_{\mathrm{SCP}}$ is the SCP safety threshold" — §Threshold authorization), `factor-1` ("a \textbf{factor-1} (background / commerce) coin pays exactly \emph{zero} demurrage" — §Peg), `c_{\mathrm{import}}(m)` and `\mathrm{import\_factor}(m)` (eqs. in §Import), `orderId` ("a deterministic 32-byte on-chain \texttt{orderId}" — §Attestation). The newly-added RFC-2119 conformance clause defines every keyword it uses normatively.

## Register-completeness finding (step 5b, prose half)

The `## Implementation status` register carries six ID-stamped rows and **every bridge-scoped target-state claim in the prose maps to a row**: epoch-keyed import tagging ("It is \textbf{target-state}, tracked in the register (\S\ref{sec:bridge:status}, row \textbf{IMP-3})" for the settlement on-ramp; import tagging → IMP-2 with the divergence note), the demurrage-settlement on-ramp (IMP-3 / botho#831), Solana transports (IMP-4), live-supply transport (IMP-5), and the mainnet-gate external audit ("a hard gate, tracked in the register (\S\ref{sec:bridge:status}, row \textbf{IMP-6})" — §Security). The v1 outstanding review-side finding — the base-layer confidential-amounts claim reading target-state without a row — is **resolved**: it is reworded to an explicit base-layer target cross-ref stated as out of this section's register scope ("Confidential amounts ... are a \emph{base-layer} target, not a bridge component: the live chain records \emph{public} amounts" — §Privacy). No unregistered target-state prose claim remains. **No step-5b major finding.**

## Top revision priorities

None — the spec is at ceiling with no critical flags. Proceed to combine with the audit sibling (expected clean per v1). If the audit re-confirms clean, the thread is READY/AUDITED-terminal.

## What's working — do NOT weaken if a further revise occurs

- The **factor-1-only wrapping** normative rule and its over-time peg-solvency argument ("Only factor-1 coins are wrappable. The reserve holds only zero-demurrage coins, so \eqref{eq:bridge:peg} holds over time by construction" — §Peg).
- The **target-state marking discipline**: the divergence note naming `bth_scan.rs` empty-cluster-tags as the live behavior against the ADR-0007 target — the exact drift-guarding the class exists for. It matches code (`bth_scan.rs:218` asserts empty tags). Keep verbatim.
- The **threshold-floor invariant** `t \geq t_{\mathrm{SCP}}` with "it MUST never be easier to move the reserve than to move consensus" (§Threshold), and the three-Safe role split (§three-Safe).
- The **exactly-once mint/release** invariant pair and the **fail-closed** posture (§Security).
- The **RFC-2119 conformance clause** and the **IMP-1…IMP-6 stable row IDs** added in v2 — these are the fixes; do not regress them.
