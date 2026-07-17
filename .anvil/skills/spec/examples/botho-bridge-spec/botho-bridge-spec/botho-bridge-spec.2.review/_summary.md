# Summary — botho-bridge-spec.2 (spec-review)

```json rubric
{
  "id": "anvil-spec-v1",
  "total": 44,
  "advance_threshold": 39,
  "dimensions": 9,
  "score": 44,
  "advance": true,
  "critical_flags": 0
}
```

```json dimensions
{
  "1_normative_correctness": 7,
  "2_internal_consistency": 6,
  "3_claim_precision": 6,
  "4_completeness": 5,
  "5_technical_accuracy": 5,
  "6_structure_navigation": 4,
  "7_crossref_versioning": 4,
  "8_prose_clarity": 4,
  "9_rhetorical_economy": 3
}
```

```json code_ref
{
  "ran": true,
  "resolved": "../../bridge/**/*.rs (35 files)",
  "missing": false,
  "note": "Scalar glob does not reach WrappedBTH.sol / Solana Anchor program / cluster-tax bridge_import_sweep.rs (anvil#718/#724); auditor reads those manually. dim 1 scored by judgment against resolved impl; exhaustive sweep is spec-audit's."
}
```

```json constant_consistency
{
  "found": true,
  "declarations": 7,
  "distinct_names": 5,
  "violations": 0,
  "passed": true
}
```

```json figure_gate
{
  "referenced": 3,
  "rendered": 3,
  "missing": 0,
  "stale": 0,
  "dim_6_7_capped": false
}
```

```json scope_distribution
{
  "preserve": 7,
  "expand": 0,
  "reduce": 0
}
```

```json delta_from_v1
{
  "v1_total": 38,
  "v2_total": 44,
  "delta": 6,
  "recovered": {
    "dim_1": "6 -> 7 (base-layer CT claim reworded)",
    "dim_3": "5 -> 6 (RFC-2119 discipline)",
    "dim_6": "2 -> 4 (figure cap lifted)",
    "dim_7": "2 -> 4 (figure cap lifted + addlinespace/IMP-row fixes)"
  }
}
```
