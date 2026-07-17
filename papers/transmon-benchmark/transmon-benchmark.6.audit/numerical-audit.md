# Numerical audit — transmon-benchmark.6

Audited: 2026-07-16. Auditor: pub-audit (Fable 5).

**Scope note (surgical diff)**: v6 differs from v5 by exactly five
substantive `main.tex` hunks (verified by direct diff this audit);
figures, `refs.bib`, and `.cls` are byte-identical to v5. The v5 audit
(`transmon-benchmark.5.audit/numerical-audit.md`) swept the abstract,
all tables, and all six committed artifacts and found **zero numerical
inconsistencies** (two traceability notes, both addressed by v6 hunks).
That sweep carries over for all text outside the hunks. This audit
independently re-verifies (a) the two v5 traceability notes' resolutions
and (b) every number the five hunks introduce, re-reading the committed
artifacts directly — not the reviser's or reviewer's trace. The
benchmark TOMLs are unchanged since the v5 audit (last commit touching
`benchmarks/transmon_*` / `gpu_driven_scaling` is b42bf77,
2026-07-16 16:21 — before the v5 audit ran; working tree clean).

Artifacts re-read this audit:
`benchmarks/transmon_bench_cpu/results.toml`,
`benchmarks/gpu_driven_scaling/results.toml`,
`benchmarks/transmon_diffopt/results.toml`,
`benchmarks/transmon_diffopt/pad_results.toml`.

## 1. v5 traceability note 1 — Table 3 off-target Peak RSS (RESOLVED)

| Text claim (v6) | Source (artifact) | Source value | Match | Notes |
|---|---|---|---|---|
| ddag footnote: "The committed `[matched.off_target.*]` tables record wall times only" | `transmon_bench_cpu/results.toml` `[matched.off_target.*]` | four sub-tables carry `wall_s` (36.8 / 26.6 / 248.0 / 64.7) and metadata only — **no RSS keys** | yes | footnote's central factual claim verified against the artifact |
| ddag footnote: "the off-target Peak RSS cells repeat the physical-target measurements of the same session and the same 133,108-DOF pencil" | same TOML, `[matched.physical_target.*]` | `peak_rss_gb = 3.1` (geode 1t and 8t), `peak_rss_gb_per_rank = 0.5` (Palace np1, np8) | yes | the four ddag-marked cells (3.1 / 3.1 / 0.5 / 0.5) equal the physical-target values exactly; "not independently recorded in the artifact" is accurate |
| off-target wall times unchanged (36.8 / 26.6 / 248.0 / 64.7 s) | same TOML `[matched.off_target.*].wall_s` | 36.8 / 26.6 / 248.0 / 64.7 | yes | untouched by the hunk except the ddag markers |

The v6 fix takes the "footnote the carry-over" option the v5 audit
offered. The abstract's every-number-traces promise is now honest at
this site: the cells are explicitly labeled as not independently
recorded. **Resolved; no flag.**

## 2. v5 traceability note 2 — §8.3 GPU-loss ratio (RESOLVED)

| Text claim (v6, L1147–1151) | Source (artifact) | Source value | Match | Notes |
|---|---|---|---|---|
| "Computed from the artifact's unrounded medians ... $136\times$ faster at 1,854 edges (displayed: 0.032 versus 4.39 s)" | `gpu_driven_scaling/results.toml` solve-only medians | 4.388126 / 0.032302 = **135.85 → 136×**; artifact's own HONEST READ comment states "136x faster at n=6 (0.032 s vs 4.39 s)" | yes | v5's 137× (from displayed rounded values) replaced by the artifact-aligned 136×, with the basis ("unrounded medians") now stated in-text |
| "still $44\times$ faster at 25,695 (1.86 versus 81.8 s)" | same TOML | 1.864723 s; HONEST READ: "44x faster at n=15 (1.86 s vs 81.8 s)" | yes | unchanged by hunk except sentence reflow |
| "direct LU ... $13.5\times$ faster at the top size (6.04 versus 81.8 s)" | same TOML HONEST READ | "Direct LU is 13x faster at n=15 (6.04 s)"; 81.8/6.04 = 13.54 | yes | carried v5 verification; consistent |

**Resolved; no flag.**

## 3. §9.2 anchor clarifier (new v6 text) — artifact-faithful

| Text claim (v6, L916–919) | Source (artifact) | Source value | Match | Notes |
|---|---|---|---|---|
| "this run targets the rounded 89.9 fF" | `transmon_diffopt/pad_results.toml` `[anchor_attempt]` | `c_sigma_target_ff = 89.9` | yes | |
| "rather than the back-solved 89.843 fF" | `transmon_diffopt/results.toml` `[target]` | `c_target_ff = 89.843364` | yes | |
| "$E_C = 0.2155$ GHz" for the 89.9 fF anchor | derived: $E_C = e^2/(2Ch)$ | 19.3702/89.9 = 0.21546 GHz → 0.2155 | yes | consistent with the 0.2156 GHz ↔ 89.843364 fF pair in results.toml (19.3702/89.843364 = 0.215600, matching `e_c_target_ghz = 0.215600` exactly) |
| "the 0.2156 GHz quoted elsewhere" | `results.toml` `[target]`, `transmon_quantum/results.toml` | `e_c_target_ghz = 0.215600` / `expected_e_c_ghz = 0.2156` | yes | "both values are artifact-faithful" is correct — the two targets live in two different committed artifacts |
| surrounding paragraph unchanged: θ ≈ −0.241 / inversion −0.0097 / θ_safe −0.0073 / 136.5 fF / 33× | `pad_results.toml` | −0.2412 / −0.009677 / −0.007258 / 136.537467 / 33.2 | yes | re-verified this audit though only the parenthetical is new |

**No inconsistency.** The clarifier resolves an apparent last-digit
mismatch a careful reader would otherwise trip on — it is accurate as
written.

## 4. Remainder of the paper

No other line of v6 differs from v5 (diff-verified). The v5 audit's
full sweep — abstract headline claims, Table 1 eigenmode agreement,
Table 2 sensitivity matrix, Table 3 physical-target/large-scale rows,
Table 4 GPU scaling, §9 diffopt numbers — found zero mismatches and
carries over unchanged.

## 5. Figure source-of-truth check (informational)

- All six `figures/*.pdf` and all six `figures/src/*.py` are
  **byte-identical to v5** (cmp-verified this audit).
- mtimes show scripts newer than renders — the same benign signal the
  v5 audit investigated by **re-executing all six scripts** against the
  committed TOMLs (fig4/fig5 pixel-identical; the rest
  content-identical, font-rasterization deltas only).
- The TOML inputs are unchanged since that re-execution (last
  benchmarks commit 16:21, before the v5 audit; clean working tree),
  and scripts + renders are byte-identical to what was checked. The
  renders therefore remain content-current; **no stale figures**.
  Recorded as informational only (the mtime skew persists because v6
  copied v5's renders — a re-render at final packaging would clear it).
