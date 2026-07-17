# Machine-readable audit summary — botho-bridge-spec.2

```json
{
  "critic": "audit",
  "role": "auditor",
  "rubric_id": "anvil-spec-v1",
  "audit_clean": true,
  "factual_findings": 8,
  "major_findings": 1,
  "code_ref": {
    "declared": "../../bridge/**/*.rs",
    "resolved": true,
    "resolved_file_count": 35,
    "scalar": true,
    "manually_consulted_beyond_glob": [
      "contracts/ethereum/contracts/WrappedBTH.sol",
      "contracts/solana/programs/wbth/src/lib.rs",
      "cluster-tax/src/simulation/bridge_import_sweep.rs",
      "cluster-tax/src/demurrage.rs",
      "transaction/clsag/src/lib.rs"
    ]
  },
  "spec_consistency": {
    "ran": true,
    "resolved": [
      "../../bridge/**/*.rs (35 files)",
      "bridge/service/src/bth_scan.rs",
      "bridge/service/src/release/bth.rs",
      "bridge/core/src/attestation.rs",
      "contracts/ethereum/contracts/WrappedBTH.sol",
      "contracts/solana/programs/wbth/src/lib.rs",
      "cluster-tax/src/simulation/bridge_import_sweep.rs",
      "transaction/clsag/src/lib.rs"
    ],
    "missing": false,
    "claims_checked": 44,
    "contradictions": 0,
    "disposition_counts": {
      "spec_wrong": 0,
      "code_wrong": 0,
      "intentional_gap": 4,
      "unregistered": 0
    }
  },
  "figures_audited": {
    "fig1-wrap-mint-flow.png": "match (non-factor-1 rejection branch drawn; matches P12/P6/P17)",
    "fig2-unwrap-import-flow.png": "match (Live/Target split; IMP-2 pointer correct; matches bth_scan.rs:218 + ADR 0007)",
    "fig3-federation-custody.png": "match (Ed25519/secp256k1 per chain; three-Safe split; matches P8/P9)"
  }
}
```

## Notes on disposition_counts

- `contradictions` = spec_wrong (0) + code_wrong (0) + unregistered (0) = **0** → `audit_clean: true`.
- `intentional_gap` = 4 counts ALL intentional-gap contradictions, every one register-suppressed:
  import-tagging (I1, I2) + the two constants (C4, C5) folding under row IMP-2/#938, and the
  demurrage-settlement op (D1) under row IMP-3/#831. `unregistered = 0`.
- No `implementation_contradicts_spec` critical flag fired. The sole real code-vs-spec
  divergence set (bridge-import tagging + demurrage-settlement) is register-suppressed with
  exact Live/Target/Tracking matches.
- Same clean posture as v1 (was 42 claims / 0 contradictions / 4 registered gaps). v2 checked
  44 claims (added V1 RFC-2119 truth-value check, V2 CT-reword check) plus the 3 rendered
  figures' content; the v2 prose/structure/figure edits introduced NO new contradiction.
- Import-tagging gap disposition on re-audit: **intentional-gap, REGISTERED (row IMP-2 /
  botho#938), suppressed — confirmed NOT code-wrong** (production bth_scan.rs:218 empty-tag
  factor-1 output is the acknowledged live behavior, not a vestigial path).
