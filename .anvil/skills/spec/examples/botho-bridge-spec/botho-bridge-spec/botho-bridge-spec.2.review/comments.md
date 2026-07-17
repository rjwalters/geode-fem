# Comments — botho-bridge-spec.2 (spec-review)

Severity: blocker / major / minor / nit. Scope: preserve / expand / reduce.

## Resolved from v1 (verification of the revise, no action)

- **[resolved · dim 7]** `\addlinespace` removed. v1 flagged 5× `\addlinespace` undefined in the whitepaper `preamble.tex` (no booktabs; `\toprule`/`\midrule`/`\bottomrule` are `\hline` redefinitions at `preamble.tex:112–114`). v2 body has **0** `\addlinespace` occurrences; all tables use only the three preamble-defined rules. Verified drop-in-`\input`-sound against the real preamble. `scope: preserve`
- **[resolved · dim 7]** Register row IDs + fig2 caption. The register now leads with an `ID` column IMP-1…IMP-6 and states "Rows carry stable IDs (\textbf{IMP-1}--\textbf{IMP-6}) so that figure captions and cross-references resolve to an exact row" (§Implementation Status). The fig2 caption correctly resolves to IMP-2 ("register row IMP-2" — Fig~\ref{fig:bridge:unwrap}), the demurrage-settlement on-ramp cites IMP-3, and the mainnet gate cites IMP-6. `scope: preserve`
- **[resolved · dims 6/7]** Three figures rendered. `exhibits/fig1-wrap-mint-flow.png`, `fig2-unwrap-import-flow.png`, `fig3-federation-custody.png` all present and fresher than the body (step 4c clean) — the caps are lifted. `scope: preserve`
- **[resolved · dim 3]** RFC-2119 discipline. Conformance-keywords clause added ("The key words \textbf{MUST} ... are to be interpreted as described in RFC~2119" — §Conformance keywords); 38 `MUST` + `MUST NOT`/`SHALL`/`SHALL NOT`/`SHOULD`/`MAY` lifted at normative points. `scope: preserve`
- **[resolved · dim 1]** Base-layer CT claim reworded. The v1 present-tense-adjacent Pedersen-commitments claim is now "the live chain records \emph{public} amounts, and the confidential-amounts transition is specified and tracked at the base layer (ADR~0006 ...; botho\#902), out of scope for this section's register" (§Privacy). Accurate (live = public), explicitly out of bridge-register scope. The downstream "public-at-the-boundary" and "confidential-amounts-clean by construction" arguments remain consistent. `scope: preserve`

## v2 observations

- **[nit · dim 2]** `import_epoch_blocks` / `import_factor_floor` are each declared twice (inline table-row `% anvil-const:` suffix + a standalone comment pair beneath the table). The gate treats these as benign matching re-declarations (identical value+unit, 0 violations). Retained from v1 by a reasoned decline; no scoring harm, no action needed. `scope: preserve`

- **[nit · dim 1, code_ref tooling — not a spec defect]** The scalar `code_ref` glob `bridge/**/*.rs` does not reach the Solidity token contract, the Solana Anchor program, or the `cluster-tax/.../bridge_import_sweep.rs` simulation that several claims cite. This is the anvil#718/#724 scalar-glob limitation, correctly declined in the v1 revise as operator/tooling-side (the auditor read those trees manually and every claim matched). Surfaced here for continuity; the fix is BRIEF `code_ref` authoring / anvil tooling, not body editing. `scope: preserve`

## Missing-figure check (step 4c)

No missing/stale finding. All three referenced `exhibits/*.png` exist and post-date the body. No dim-6/7 cap.

## Unregistered-target-state check (step 5b)

No finding. Every target-state prose claim maps to an IMP-row; the base-layer CT claim is reworded as an explicit out-of-scope base-layer cross-ref.
