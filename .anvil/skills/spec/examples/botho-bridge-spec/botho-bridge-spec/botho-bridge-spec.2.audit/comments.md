# Audit comments — botho-bridge-spec.2

Line-level audit comments keyed to `botho-bridge-spec.tex` (v2). Re-audit; only notes that
changed or are worth flagging on the delta are recorded — the v1 comments still stand for
unchanged spans.

- **L23–32 (Conformance keywords / RFC-2119 clause)** — NEW in v2. Correct and load-bearing:
  it explicitly ties the target-state register's Live/Target columns to the respective MUST
  obligations ("the register's Live column states the obligation the current implementation
  MUST meet and the Target column states the obligation a future implementation MUST meet").
  This is the right way to reconcile RFC-2119 absoluteness with a partially-shipped bridge.
  No truth-value drift introduced by the uppercasing.

- **L61 / L102 / L268 / L348–354 (anvil-const markers)** — all 5 distinct constants match
  code/ADR. `import_epoch_blocks` and `import_factor_floor` are each declared twice (inline
  table-row + standalone) with identical value+unit — benign, no drift. Same as v1.

- **L218–221 (§Peg, factor-1-only wrapping)** — matches `bth_scan.rs:126` (deposit gate) and
  `release/bth.rs` (release spends only factor-1). MUST/MUST-NOT uppercasing preserves truth.

- **L270–276 (§Privacy, base-layer CT reword)** — REWORDED in v2. Now states the live chain
  records public amounts and confidential amounts are a base-layer target (ADR 0006 / botho#902),
  explicitly out of this section's register. TRUE of the code (clsag lib.rs transparent-amount
  model). Correctly avoids overloading the bridge register with a non-bridge property. Good.

- **L400–409 (fig2 caption)** — CORRECTED in v2: caption now cites "register row IMP-2"
  (v1 said IMP-1, which pointed at the Ethereum-path live row, not the import-tagging row).
  The rendered PNG's Live-note also reads "register row IMP-2". Pointer now resolves correctly.

- **L466–532 (§Implementation Status register)** — six rows now carry stable IDs IMP-1…IMP-6.
  Every Live/Target/Tracking cell audited from the code side (see findings register-accuracy
  table); all accurate. The divergence note (L523–532) quotes `bth_scan.rs` empty-cluster-tags
  and cites IMP-2 — an exemplary target-state disclosure. No row weakened vs v1.

- **exhibits/fig1,fig2,fig3 (rendered content)** — all three audited against prose + code;
  all match. fig2 is the load-bearing one (draws the IMP-2 Live/Target split correctly).

- **Advisory (M-1)**: the BRIEF's scalar `code_ref` cannot span WrappedBTH.sol + Solana lib.rs
  + the cluster-tax simulation. Consulted manually per the BRIEF note. anvil#718.
