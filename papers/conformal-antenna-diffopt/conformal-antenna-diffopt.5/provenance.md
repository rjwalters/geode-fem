# Evidence for conformal-antenna-diffopt.5

Every number in `main.tex` against its committed source, read at `main`
`219a089c` (2026-10-14). No solver, mesher or `cargo` build was run for this
revision. Paths are relative to `benchmarks/patch_antenna_conformal/` unless
they start with `crates/`, `reference/` or `papers/`. Tiers: **MEASURED** (a
committed solve on a stated discretization), **ESTIMATE** (an extrapolation
with a stated rule, not a bound), **derived** (arithmetic on committed values),
**literature** (a cited closed form evaluated here).

| Manuscript anchor | Committed source | Fields and interpretation |
|---|---|---|
| Abstract, §1, §6 headline | `copper/g2_summary.toml` `rbD_L1_copper_38_sm45_on_rbD_L1_p2` | MEASURED at P2/L1: `verify_worst_db` −10.7826, `bw_lo_ghz` 2.39384, `bw_hi_ghz` 2.4887, `bw_mhz` 94.8661, `ism_cover_pct` 100, `centring_mhz` 0.479, `margin_lo_mhz` 6.162, `margin_hi_mhz` 5.204, `min_vol_ratio` 0.250026 |
| Abstract, §6 L3 | `g2_summary.toml` `..._on_rbD_L3_cyl` and `..._on_rbD_L3` | MEASURED: −8.80226 (vol 0.2408), default air morph −8.80579 (vol 0.0996); band 2.38005–2.47463, 94.58 MHz, 89.38 %, centring +14.41 MHz. Offset against the P2/L1 band: 13.8 MHz (low edge), 14.1 MHz (high edge), 14.0 MHz (dip, 2.44048 − 2.42649), stated "about 14 MHz". **Derived:** S11 change 0.0035 dB, stated "0.01 dB" as in `copper/README.md` |
| Abstract, §4.1, §6 upshift | `convergence.toml` `[gate_g1.bent]` | ESTIMATE: `p2_extrapolated_dip_f_ghz` [2.377944, 2.405029, 2.369309] at orders [1, 0.497, 1.489] minus `finest_p2_dip_f_ghz` 2.356643 → **derived** +21.3, +48.4, +12.7 MHz |
| Abstract, §5, Conclusion: L1 → P2/L1 shift 45–48 MHz | `copper/README.md` "Strategy" table | MEASURED fit (`scripts/copper_shift_fit.py`): L1 47.85 MHz, re-based L1 44.94 MHz, L2 29.33 MHz; initial patch 44.8 / 27.9 MHz; rms misfit 0.018–0.025 shifted, 0.23–0.38 unshifted |
| §3.2 moving-feed FD | `copper/README.md` "The feed"; test `mesh::patch::tests::curved_conformal_moving_feed_gradient_matches_central_finite_difference` | below 1e-8 (7.8e-10 arm64, 4.7e-9 x86_64); misses by a relative 3 without the explicit term |
| §3.3 board check | `copper/verify/*.toml` `[copper]`, `[crater]` | `max_slab_radius_change_mm` 1.421e-14; thickness 1.600 everywhere |
| Table 1 | `copper/rbD_L1_copper_38_sm45.toml` `[basis]` | 32 outline + 4 corner + 2 feed = 38 |
| §3.4 objective, optimizer | `copper/rbD_L1_copper_38_sm45.toml` `[meta]`, `[optimization]` | soft-max β = 25, 5 band points, L-BFGS memory 8, Armijo, step cap 1.0, guard 0.25 |
| §3.4 FD gate | `copper/README.md` "FD gate"; `copper/*_fd.toml`; `[fd_validation]` of each run | worst 2.2e-5 (tol 5e-3); per group on L0/L1 ≤ 1.9e-7 |
| §4 fixture | `baseline_2g4_L1.toml` `[meta]`; `reference/gmsh/patch_conformal_L1.yaml` | 38 × 29 mm, h 1.6, pad 12, probe 1.6 mm at 7.0 inset, R 100, air 40, UPML 25, σ0 1, εr 4.4, tanδ 0.02, 50 Ω; ±0.31 rad and 1.8 mm sagitta from the YAML comment |
| §4 ladder | `convergence.toml` `[levels.*]` | edges 38,650 / 150,561 / 370,780 / 844,238; p2 unknowns 204,824 / 804,460 |
| §4 absorber | `convergence.toml` `[upml]` | `chosen_sigma_0` 1, `residual_sensitivity_s11` 0.0208 (stated ≤ 0.021) |
| §4 bending | `convergence.toml` `[detune]` | bent − flat dip −0.97 … −1.58 MHz (stated 1.0 to 1.6) |
| §4.1, Figure 2 | `convergence.toml` `[[s11.bent]]`, `[order.bent]`, `[gate_g1.bent]` | MEASURED dips 2.271708 (L0) … 2.342186 (L3), 2.356643 (P2/L1); local orders 1.176, 0.497; **derived** L0→P2/L1 rise 84.9 MHz ("85"), P2/L1 − L3 = 14.46 MHz |
| §4.1 initial worst | `nested_dof/verify/initial_on_L1_p2.toml` | −2.421677 dB, best −8.34 at 2.400 GHz, vol 1.0 |
| §4.1 cost | `convergence.toml` `[cost.*]` | L1 forward 44.27 s, band gradient 66.65 s, RSS 6.3 / 8.2 GiB (M3 Ultra 28 cores); P2/L1 752.6 s, 132.2 GiB (r8g.8xlarge); L3 495.5 s, 68.0 GiB |
| Table 2 | `copper/README.md` trajectory table; `copper/L1_copper_38.toml`, `rbB_L1_copper_38_warm.toml`, `rbB_L1_copper_38_sm48.toml`, `rbD_L1_copper_38_sm45.toml` `[optimization]` | worst initial → final, steps, terminal, wall; hosts r8g.4xlarge (16 vCPU) and r8g.8xlarge (32 vCPU) |
| §5 first corrected design | `g2_summary.toml` `rbB_L1_copper_38_sm48_on_rbB_L1_p2` | −9.9632 dB |
| §5, §6 re-base drift | `copper/README.md` "Re-base"; `g2_summary.toml` `copper_shorter_than_design_mm` | 0.468/0.47 mm (L2 chain on rbE), 0.09 mm (final on rbF) |
| §6 as built | `copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense0of3.toml` `[copper]` | L 28.803414, W 49.752266, probe 4.267542, inset 5.072777 |
| §6 sibling | `g2_summary.toml` `..._on_rbF_L1_p2` | −10.3475, vol 0.234 |
| §6 related L2 design, Table 3 | `g2_summary.toml` `rbC_L2_copper_38_sm29_on_rbE_*`; `copper/README.md` chain table | P2/L1 −10.1045, 2.39931–2.49364, 94.33 MHz, 100 %, margin_lo 0.69; L3 −9.2496, vol 0.228; P2/L1 vol 0.299; as optimized 28.89 × 48.95 mm, probe 4.49 |
| §6 cold L1 own-level band | `g2_summary.toml` `L1_copper_38_on_L1` | 84.341 MHz |
| §6 bandwidth context | `jackson_bandwidth.py` (this directory); Jackson & Alexopoulos (1991), `jackson1991simple`, in the form restated in D. R. Jackson's UH ECE 6345 Notes 15–16, `jackson_ece6345` | **literature/derived**: p = 1 + (a2/10)(k0W)² + (a2² + 2a4)(3/560)(k0W)⁴ + (c2/5)(k0L)² + (a2 c2/70)(k0W)²(k0L)², a2 = −0.16605, a4 = 0.00761, c2 = −0.0914153; Q_sp = (3π/8) εr (L/W)/(k0 h c1 p); e_r^hed = 1/[1 + (3π/4)(k0h)(1/c1)(1 − 1/n²)³] = 0.899; Q_sw = Q_sp e_r/(1 − e_r); Q_d = 1/tanδ; Q_c omitted (PEC model); BW = 1/(√2 Q) at VSWR 2, f = 2.44 GHz. 38 × 29 mm: p 0.902, 62.9 MHz (60.0 without Q_sw); 49.75 × 28.80 mm: p 0.865, 70.4 MHz (66.8 without Q_sw). Stated "about 63" and "70 MHz" and "about 10 %" surface-wave loss. The formulas follow the PR #1125 Judge's reading of the course notes |
| §6 cost | `copper/rbD_L1_copper_38_sm45.toml` `[optimization]`; dense shards `[verify]` | 125 solves, 2388.2 s; first gradient 103.6 s, 6.238 GiB; shards 4604.5/14, 4550.7/14, 6035.7/13 s → 5.4–7.7 min; peak 150.9 GiB |
| §7 no correction | `g2_summary.toml` `L1_copper_38_on_L1_p2`, `L2_copper_38_s30_on_L1_p2` | −4.75836; −9.91752 |
| §7 thickness ablation | `nested_dof/README.md` "Gate G2"; `nested_dof/verify/` | L2 cold −14.07 / −12.47 / −9.16, loss 4.91; L1 combined −14.12 → −5.69, loss 8.43; mean 3.73 mm, max 4.70 mm; detune 75 MHz from `copper/README.md` |
| §8 feed | `copper/README.md` "The feed model is idealized" | 4.3 mm port strip; SMA pin 1.27 mm |
| §8 Meep | `reference/meep/` harness and R2 pilot (PR #1105) | No number used; stated as unconverged future work (operator ruling) |
| §9 counts | `count_provenance.py` (keys `untrailered_*`; read-only `git log` at `219a089c` over `papers/conformal-antenna-diffopt`, `benchmarks/patch_antenna_conformal`) | 13 PRs (#655–#1124), first merged 2026-07-20; 30 non-merge commits; 26 Claude-trailered (Opus 19, Fable 7, Sonnet 0); 4 without trailer (`3698c4ac`, `b245de22`, `12c082f8`, `b130ebbd`), 5,904 of 63,288 added lines (9.3 %). The paper does not attribute the 4 untrailered commits to a tool, because the history does not show which made them. Crutcher Dunnavant: 36 commits, 2026-06-24 to 06-29 |
| §9 reviews | read-only `gh pr view` of #1097, #1104, #1107, #1122, #1124 | each has a Judge review comment (APPROVED on #1097, #1122; changes requested then approved on #1104, #1107, #1124); examples: #1107 Blocking 1 (G1 bracket), #1124 Blocking 2 (re-base drift) and 3 (direction of the converged answer) |
| §9 Codex figure review | `codex-figure-review.md` (this directory; host paths scrubbed) | Codex review of the figures and layout of the v4 draft |
