# Numerical audit — transmon-benchmark.1

Ground truth for this audit is the set of **committed repo artifacts** (per the
paper's own contract, main.tex lines 3–9 and 606–609):

- `benchmarks/transmon_eigen/results.toml` (agreement table source of truth)
- `reference/fixtures/transmon_palace/results_p1/eig.csv` (+ `port-EPR.csv`,
  `palace_run_v22.log`)
- `reference/fixtures/transmon_palace/palace_config.json` + `.provenance.txt`
- `crates/geode-core/tests/fixtures/transmon_smoke.provenance.txt`

## A. Text-vs-artifact disagreements (each is a critical flag; see flags.md)

| Text claim | Location (main.tex) | Committed artifact value | Match | Notes |
|---|---|---|---|---|
| "within $0.03\%$ across all physical modes" | L52 (abstract) | results.toml `worst_case_rel_err_pct = 0.032` | **NO** | 0.032% > 0.03%; BRIEF headline is "≤0.033% (worst 0.032%)" |
| "reproduces Palace's physical eigenmodes to $\leq 0.03\%$" | L101 (contributions) | 0.032 | **NO** | same stale bound |
| "every physical mode agrees to $\leq 0.03\%$" | L357–358 | 0.032 | **NO** | same |
| table row "same to $\leq 0.03\%$ / $\leq 0.03\%$" | L375–376 (Tab. 2) | cavity-mode rel_err 0.029/0.007/0.026/0.027 | ok for cavity rows only | bound holds for the four cavity modes but the adjacent resonator row breaks it (0.032) |
| "the residual $\leq 0.03\%$" | L414 | 0.032 | **NO** | same |
| "agreeing to $\leq 0.03\%$ on the identical discrete problem" | L648 (discussion) | 0.032 | **NO** | same |
| "to $\leq 0.03\%$ across all physical modes" | L687 (conclusion) | 0.032 | **NO** | same |
| junction LC "agreeing to $0.000\%$" | L53 (abstract) | results.toml `modes.junction_lc.rel_err_pct = 0.001` | **NO** | pre-correction value |
| "junction LC mode agreeing to $0.000\%$" | L102 | 0.001 | **NO** | same |
| "agrees to $0.000\%$" | L359 | 0.001 | **NO** | same |
| Tab. 2 junction row "$\Delta$ = 0.000\%" | L377 | 0.001 | **NO** | same |
| "($0.000\%$ on the junction LC mode)" | L688 (conclusion) | 0.001 | **NO** | same |
| Tab. 2 resonator: geode-fem 5.1528, $\Delta$ 0.029\% | L374 | results.toml: `f_ghz = 5.153`, `rel_err_pct = 0.032` | **NO** | 5.1528 is a full-precision value that exists in NO committed artifact; the committed geode value is 5.153 and the committed Δ is 0.032% |
| Tab. 2 cavity: 20.6976 | L375 | results.toml `modes.mode_5.f_ghz = 20.703` | **NO** | 20.6976 is Palace's value (20.69755679425) rounded — i.e. the wrong solver's number in the geode-fem column |
| Tab. 2 cavity: 26.0809 | L375 | results.toml `modes.mode_6.f_ghz = 26.088` | **NO** | 26.0809 is Palace's value (26.08089940472) rounded — same defect |
| Tab. 2 cavity: 15.4650 / 18.6927 | L375 | 15.465 / 18.693 | yes (within artifact precision) | but quoted at 4 decimals while the artifact carries 3 — the 4th digit is unsourced |
| Tab. 2 junction: geode-fem 17.4901 | L377, L404 | results.toml `f_ghz = 17.490` | yes (within artifact precision) | 17.4901 is Palace's rounded value; committed geode value is 17.490 (3 dp). Consistent under rounding, but the 4th digit is unsourced for the geode column |
| "Palace's per-port energy-participation output provides the same discriminant on its side, and the two assignments agree" | L345–347 | `results_p1/port-EPR.csv`: mode 3 (17.49 GHz) has p[1] = **+2.49e-08**, the SMALLEST magnitude of all six modes (largest is mode 1 at −4.7e-04) | **NO** | the committed Palace participation artifact does not show the junction mode as the high-participation mode; the results.toml prose comment ("only the 17.49 GHz mode has appreciable junction EPR") asserts the opposite of what the committed CSV contains |
| "junction participation is p = 1.000 for the 17.49 GHz mode and p ≤ 0.0005 for every other mode, **in both solvers' participation outputs**" | L408–410 | geode side: results.toml p = 1.000/0.000 ✓. Palace side: port-EPR.csv junction-mode p[1] = 2.49e-08 | **NO (Palace half)** | the "≤ 0.0005 for every other mode" bound is, ironically, satisfied by ALL SIX Palace rows including the junction mode. Possibly a units/normalization subtlety in Palace's port-EPR output — the auditor does not adjudicate; the text and the committed artifact cannot both stand as written |

## B. Numbers cited with NO committed artifact (provenance gap; critical flag 5)

The reproducibility section claims "Every artifact behind every number in this
paper is committed and scripted" (L606–607). Repo-wide search
(`benchmarks/`, `reference/`, `crates/`) finds **no committed artifact** for:

| Text claim | Location | Actual provenance | Notes |
|---|---|---|---|
| geode-fem wall 51.2 ± 0.4 s (n=3), peak RSS 3.1 GB | L521, Tab. 3 | Epic #476 issue comment ONLY (rjwalters/geode-fem#476, "Hardware: m6i.4xlarge...", table rows verified verbatim in the comment) | no `benchmarks/transmon_bench_cpu/` or equivalent exists |
| Palace 4 ranks: 50.8 s, ~0.7 GB/rank | L522, Tab. 3 | Epic #476 comment only | no committed artifact at any rank count for the 4-rank run |
| Palace 8 ranks: 30.6 ± 0.1 s, ~0.5 GB/rank | L523–524, Tab. 3 | Epic #476 comment; PARTIALLY corroborated by committed `palace_run_v22.log` (Total 31.25 s at 8 ranks; peak per-node 3.7 GB → ~0.46 GB/rank) | the log is one oracle run, not the n=3 /usr/bin/time full-pipeline measurement |
| derived 4× per-core and 1.7× whole-box factors | L56–58, L111–113, L539–545, L694–696 | arithmetic is internally consistent with Tab. 3 (51.2 vs 50.8×4 → 3.97×; 51.2/30.6 = 1.67×) | inherit the Tab. 3 provenance gap |
| L-doubling tripwire: 17.49 → 12.37 GHz, ratio 0.7071 | L326–328, Fig. 3 caption L397–399 | BRIEF only; the test `tripwire_real_junction_l_doubling` exists (crates/geode-core/tests/transmon_eigenmode.rs L577) but its measured output is not committed | minor: 12.37/17.49 = 0.7073 at quoted precision; 0.7071 requires the full-precision values |
| spurious-mode participation p = 0.994 | L456, Fig. caption L487 | BRIEF only; results.toml note gives "near 3.45 GHz" but no participation value | |

**Recommendation** (for the reviser/operator): commit a
`benchmarks/transmon_bench_cpu/results.toml` (or similar) carrying the Table 3
numbers + instance/commit provenance, and extend `results.toml` (or a sibling)
with the L-doubling and spurious-mode measured values, before this paper
advances past review.

## C. GPU-number scan (sanctioned-placeholder verification)

**PASS.** Every would-be GPU result in Section 9 is a `\TBDGPU{...}` marker
rendered red and unmistakable (L562–563, L588–595, L600–601). The only numeric
content in GPU context is hardware/scope specification (g6e.xlarge, 1× L40S
46 GB, CUDA 13.2, burn-cuda 0.21 f32-only) — consistent with the BRIEF's GPU
cell spec. **No fabricated GPU timing or correctness number appears anywhere
in main.tex or the rendered PDF.**

## D. Values verified consistent with committed artifacts

- Mesh: 22,684 nodes; 133,314 tets; 156,863 Nédélec edge DOFs; 133,108 interior
  after PEC (L211–213, L363–364) — match results.toml/provenance exactly.
- Fixture SHA-256 `5b3ff4c3…b33dd` (L615) — prefix/suffix match the committed
  hash `5b3ff4c357a4dc905e7a2e42abc8178778b626e47582c7bbe610f95d332b33dd`.
- Junction L = 14.860 nH, C = 5.5 fF (L235–236, Tab. 2 caption) — match
  results.toml and palace_config.json (1.486e-8 H, 5.5e-15 F).
- f_LC = 1/(2π√(LC)) = 17.60 GHz (L323–324) — recomputed: 17.605 GHz ✓; matches
  results.toml note.
- Palace hunt target σ = 4.5 GHz (L436, L458) — palace_config.json
  `Target: 4.5` ✓; lowest committed eigenvalue 5.1513 GHz ✓ ("nothing below
  5.15").
- Palace eigenvalues in Tab. 2 Palace column (5.1513, 17.4901) — match eig.csv.
- Six physical modes — eig.csv has exactly 6 rows ✓.
- Spurious mode "approximately 3.45 GHz" (L455) — results.toml note "near
  3.45 GHz" ✓ (participation 0.994 is unsourced, see B).
- Sapphire ε = diag(9.3, 9.3, 11.5) rotated ~36.87° (L204–205) —
  palace_config.json Permittivity [9.3,9.3,11.5], MaterialAxes
  [0.8,0.6,0]/[−0.6,0.8,0] (= 36.87°) ✓; fixture provenance ✓.
- PEC on metal + exterior (attrs 5, 3), readout ports open/omitted, junction
  LumpedPort reactive-only on attr 4 (+Y) — palace_config.json +
  palace_config.provenance.txt ✓.
- Seven named physical groups; 4-triangle lumped_element (L237) — fixture
  provenance ✓ (lumped_element: 4 tris).
- MSH 4.1 → 2.2 conversion, "vertices indices are not unique" rejection,
  node-preserving (all 22,684 nodes) (L618–622) — results.toml
  `mesh_converted` ✓ (bit-for-bit eigenvalue claim: see non-critical notes).
- Commits: geode-fem `3174015` exists in git (feat(eigen) transmon eigenmode,
  PR #496) ✓; Palace `fba6a5b` = results.toml `palace_version` ✓.
- DeviceLayout.jl v1.15.0; Burn/burn-cuda 0.21 (L638) — provenance + bib ✓.
- p=2 Palace config committed (L670) — `palace_config_p2.json` exists ✓.
- Blog band [4.14, 5.591] GHz (L422, L437) — results.toml note + provenance ✓.
- geode participation p = 1.000 / others 0.000 (geode side of L408–409) —
  results.toml ✓.
- m6i.4xlarge = 8 physical cores / 16 vCPU / 64 GB, us-west-2 (L496–498,
  L633–635) — matches the #476 comment; ~$0.77/hr footnote is external,
  qualified "approximately".

## E. Informational

- **Figures are unrendered**: `figures/` contains only `src/` (5 scripts,
  fig1/fig2/fig3/fig4/fig6). The PDF ships the declared framed placeholders
  via `\anvilfig`. Not stale (no render exists to be stale); pub-figures has
  not run. Non-critical.
- Committed `palace_run_v22.log` reports global unknowns H1 = 29,705 and
  ND (p=1) = 183,891, which differ from the mesh's 22,684 nodes / 156,863
  edges. The paper quotes neither number, so nothing flags, but the operator
  may want to understand the delta (MSH 2.2 conversion or Palace partition
  bookkeeping) before a reviewer asks.
- The BRIEF-mandated precision-note disclosure (results.toml stores geode
  frequencies at 3 decimals; rel_err_pct computed against rounded values;
  full-precision agreement ~0.029% worst) is **absent** from the paper's
  reproducibility section. Its absence is the likely origin of flags 1–3:
  the draft quotes the ~full-precision story the BRIEF says to disclose-but-
  not-quote. One fix (adopt committed numbers + add the disclosure paragraph)
  resolves all three flags coherently.
