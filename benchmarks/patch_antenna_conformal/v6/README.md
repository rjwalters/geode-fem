# Antenna v6 experiments (#1126)

These are the computations the v5 review of the conformal-antenna paper asked for: M1 (forward-model accuracy), M6 (curvature), M7 (efficiency, gain and absorber loading), M2 (baselines) and M5 (realizable feed). Every number below comes from a committed TOML in this directory. `scripts/v6_summary.py --write` collects them into `v6_summary.toml`, and `crates/geode-core/tests/patch_conformal_v6.rs` pins them. The design under test is the #1120 headline `rbD_L1_copper_38_sm45` (`../copper/`). σ₀ = 1 throughout. The v5 numbers quoted for comparison come from `../convergence.toml` and `../copper/`.

Fixtures: the new YAMLs are `reference/gmsh/patch_conformal_{x1*,rbDflat,rbDair60,rbDair80,rbG,rbH,rbR}_L1.yaml`. Their generator provenance (sha256, gmsh 4.15.2 on the Mac, resolution) is in `../provenance/`. The meshes themselves are not committed. The `.geo` gained two optional refinements, `lc_sub_patch` (in-slab size under the patch plus 4 mm) and `lc_rim` (size within 0.5 mm of the patch edges). Both are inactive by default, and every committed fixture regenerates to the same sha256 (checked for `rbD_L1` and `rbD_L3`).

All solves ran on AWS (r8g Graviton for geode, r6i x86 for Palace). Hosts are listed in each `[meta].host`.

## Summary

| item | question | answer |
|---|---|---|
| X1 | Where does the initial patch's dip converge? | **About 2.366–2.380 GHz**, 9–23 MHz above v5's finest model (2.357). The error lives at the patch **rim**, not in the substrate layer count. Palace reproduces geode on the same mesh at p = 1 and p = 2 (within 1.2 MHz), and Palace p = 3 equals geode's finest rung to 0.02 MHz. **The final design misses at p = 3**: band 2.4029–2.4975 GHz, worst in-band −9.56 dB. |
| X2 | Does the bend matter? | **It does not move the resonance (under 0.4 MHz). It changes the match by about 1 dB.** The same copper on the flat board misses −10 dB at P2/L1 (−9.93 against −10.78 dB). |
| X3 | Efficiency and gain, and is the band absorber-loaded? | **η_rad = 0.62, FR-4 loss 0.36, broadside gain 4.4–4.6 dBi** (PEC conductors). The power balance closes to 1.2–2.3 %. The air margin moves the dip by at most 2.4 MHz and the band by at most 1.4 MHz. The FEM's own loss fraction bounds its Q and accounts for the bandwidth gap to the closed form. |
| X4 | Is the 38-DOF basis or the adjoint needed? | **The basis: not to meet the target. The adjoint: yes, for cost.** The 4-parameter rectangle under the same band-shift and re-base protocol meets −10 dB at P2/L1 (−10.33 dB against −10.78 dB, 6 MHz narrower, 340 solves). On the 38 DOFs, separable CMA-ES (3 seeds) needs 385–520 solves to reach −9.5 dB against the adjoint's 35 (11–15×). It reaches −10 dB in one seed of three (695 solves against 60), and then matches the adjoint's final −10.04 dB. |
| X5 | Does a realizable 1.27–1.6 mm feed still cover ISM? | **Yes, on P2/L1.** Re-running the final step with the probe pinned gives a worst-of-band of **−10.62 dB (1.6 mm)** and **−10.65 dB (1.27 mm)**, against −10.78 dB with the 4.27 mm strip. |

## X1: forward-model accuracy (M1)

### The geode P1 ladders (`x1/dip_*.toml`)

All runs use the bent initial patch (38 × 29 mm, 1.6 mm probe inset 7 mm), the |S11| dip by pre-scan plus golden section, and `examples/patch_conformal_convergence.rs dip` with `PATCH_CONFORMAL_GEOMS=bent`. Each rung starts from L1 and refines either the slab under the patch (`x1sub*`), the rim (`x1rim*`), or both (`x1comb`). Everything else is L1's.

| fixture | in-slab size under patch | rim size | substrate layers (median, patch interior) | patch-rim edge mean (mm) | DOFs | bent dip (GHz) | vs L1 (MHz) |
|---|---|---|---|---|---|---|---|
| L1 (v5) | 0.80 | – | 2.00 | 0.78 | 150 561 | 2.31182 | 0 |
| x1sub053 | 0.533 | – | 2.54 | 0.52 | 234 691 | 2.32727 | +15.5 |
| x1sub040 | 0.40 | – | 3.44 | 0.39 | 414 837 | 2.34040 | +28.6 |
| x1sub030 | 0.30 | – | 4.19 | 0.29 | 828 775 | 2.34802 | +36.2 |
| x1rim040 | 0.80 | 0.40 | 1.99 | 0.39 | 317 424 | 2.34197 | +30.2 |
| x1rim020 | 0.80 | 0.20 | 1.99 | 0.20 | 430 808 | 2.35693 | +45.1 |
| x1comb | 0.30 | 0.10 | 4.19 | 0.10 | 1 738 557 | **2.36595** | +54.1 |

The layer count is the generator's median of h / (tet z-extent) over the substrate tets more than 3 mm inside the patch outline. Through-thickness, an isotropic mesh of size `lc` has h / lc = 2, 3, 4 and 5.3 nominal layers at 0.8, 0.533, 0.4 and 0.3 mm. The median metric reads lower (2.0, 2.5, 3.4, 4.2).

What the ladders show:

- **The rim decides the dip, the interior layer count does not.** `x1sub040` refines the whole slab under the patch, giving 3.4 layers and a 0.39 mm rim as a side effect. `x1rim040` keeps the 2-layer interior and refines only within 0.5 mm of the edges, to the same 0.39 mm rim. They land within 1.6 MHz of each other (2.3404 and 2.3420 GHz). The substrate ladder raises the dip because its fine box also refines the rim.
- **Every rung raises the dip**, and the shift tracks the rim edge size. A fit `f = f∞ − C h_rim^p` over all seven P1 rungs gives **p = 0.80 and f∞ = 2.380 GHz**, with a 1.3 MHz rms residual (`v6_summary.toml [x1.rim_fit]`). An order-1 Richardson step on the rim pair (0.39 → 0.20 mm) gives 2.372 GHz. Halving the rim again in the combined rung moves the dip only +9.0 MHz (2.3569 → 2.3659). That is less than either extrapolation predicts, so the rate is still rising toward the asymptotic range.
- **A 0.2 mm rim at p = 1 (431k DOFs) already matches P2/L1 (805k DOFs)**: 2.3569 against 2.3566 GHz. So v5's "not converged on any affordable mesh" was a consequence of the uniform-graded mesh design, not of the antenna.

### The Palace reference (`x1/palace_summary.toml`, `x1/palace/`)

[Palace](https://github.com/awslabs/palace) (commit `fba6a5b`, the vendored `reference/palace/docker/Dockerfile`, `palace-build-info.txt`) solved the same problem on geode's own meshes. The meshes were written as MSH 2.2 by `patch_conformal_nested_dof --verify-only --write-mesh`, with sha256 in `x1/palace/meshes.txt`. Palace used the same materials, PEC patch and ground (interior surfaces), and the same uniform 50 Ω lumped port. It is an independent solver with an independent absorber: Palace has no UPML, so the shell is air and the outer wall carries a first-order absorbing boundary. Each run is an adaptive (PROM) driven sweep at 1 MHz spacing (`AdaptiveTol` 1e-3, GMRES to 1e-8). Configs come from `scripts/palace_patch_config.py`, and `scripts/palace_ingest.py` extracts the results.

| run | mesh | order | ND DOFs | dip (GHz) | geode on the same mesh (GHz) |
|---|---|---|---|---|---|
| init_L1_p1 | initial, L1 | 1 | 174 490 | 2.3130 | 2.31182 (P1) |
| init_L1_p2 | initial, L1 | 2 | 884 530 | 2.3570 | 2.35664 (P2, v5) |
| init_L1_p3 | initial, L1 | 3 | 2 503 179 | **2.3660** | – |
| fin_L1_p2 | final design, rbD_L1 | 2 | 962 662 | 2.4406; band 2.3946–2.4885 (94.0 MHz), worst of the 9 in-band points −10.77 dB | P2: band 2.3938–2.4887 (94.9 MHz), −10.78 dB |
| fin_L1_p3 | final design, rbD_L1 | 3 | 2 729 331 | **2.4492; band 2.4029–2.4975 (94.6 MHz), worst of the 9 in-band points −9.56 dB, ISM coverage 96.5 %** | – |

- **The two codes agree** to 1.2 MHz at p = 1 and 0.4 MHz at p = 2 on the same mesh, despite different absorbers. The final design's P2/L1 band reproduces in Palace to about 1 MHz. So the v5 numbers are a correct solution of the discrete model. What was missing was convergence of that model.
- **p = 3 on L1 gives 2.3660 GHz**, 9 MHz above p = 2. That is the same value, to 0.02 MHz, as geode's finest P1 rung (`x1comb`: 0.1 mm rim, 0.3 mm slab, 1.74M DOFs). Two independent refinements of the same model, p-refinement in Palace and rim h-refinement in geode, meet at the same number. The p-ladder on L1 (2.3129 → 2.3570 → 2.3660 GHz, increments 44 and 9 MHz) is converging fast.

### Where the true resonance sits

For the **initial patch**, every refinement raises the dip, so the finest computed values are lower bounds. Two independent ones agree: geode P1 `x1comb` gives 2.3659 GHz and Palace p = 3 on L1 gives 2.3660 GHz. The rim fit puts the limit at 2.380 GHz, and the order-1 rim Richardson at 2.372 GHz. **So the converged dip is about 2.366–2.380 GHz: 9–23 MHz above v5's finest model (P2/L1, 2.3566 GHz).** v5's estimate of the further rise was 12.7–48.4 MHz. This narrows it to 9–23 MHz, at the lower end. A 4–6 layer substrate by itself is not what changes the answer. The patch rim is.

For the **final design**: its P2/L1 −10 dB band starts 6.2 MHz below 2.400 GHz. If it shifts by the initial patch's measured 9 MHz (p = 2 → 3), the lower edge lands at about 2.403 GHz. If it shifts by the full 9–23 MHz range, the lower edge lands at 2.403–2.417 GHz. **Either way the converged design misses the lower band edge by roughly 3–17 MHz**, which confirms v5's "probably misses" with a narrower range. **Palace p = 3 on the final design's own mesh measures it directly.** The band moves up 8.4 MHz from p = 2 (2.3946 → 2.4029 GHz at the lower edge), the lower edge misses 2.400 GHz by **2.9 MHz**, the worst in-band point is **−9.56 dB**, and the band covers 96.5 % of ISM. **The v5 headline design does not meet −10 dB across the band once the rim is resolved.** It misses by about half a dB at the 2.400 GHz edge, and further refinement can only move it higher. The fix is the space-mapping step v5 proposed: re-centre with a band shift of about 44.9 + 9 to 23 MHz. That was not run here.

The textbook transmission-line (Hammerstad) estimate for the unloaded 38 × 29 mm patch is **2.433 GHz** (ε_eff 4.09, ΔL 0.74 mm; `[x1].textbook_tl_*`). The converged FEM dip sits about 50 MHz below it. That gap is not discretization. The probe's reactance pulls the matched |S11| dip below the unloaded cavity resonance, and the transmission-line model ignores the finite 12 mm ground margin and the probe. A port-free eigenfrequency would separate the two, and was not run.

## X2: curvature (M6)

`patch_conformal_rbDflat_L1` is `rbD_L1` with the bend radius set to 10⁶ mm. The gmsh mesh is identical node for node: the flat sha256 matches, and only the bend node map differs. The sagitta at the patch edge is 1.8e-4 mm. The design coefficients are chart coordinates (s = mid-plane arc length), so the flat fixture is the board *before bending*: the same etched copper. The as-built outline agrees to 2e-3 mm.

| model | bent (R = 100 mm) | flat | bent − flat |
|---|---|---|---|
| P1/L1 dense dip | 2.39634 GHz, −22.4 dB | 2.39601 GHz, −17.7 dB | **+0.34 MHz** |
| P1/L1 −10 dB band | 93.3 MHz (2.3505–2.4438) | 82.8 MHz (2.3554–2.4381) | **+10.5 MHz** |
| P2/L1 9 in-band points, worst | **−10.78 dB** (v5) | **−9.93 dB** (2.4835 GHz) | −0.86 dB |
| P2/L1 band edges (2.400 / 2.4835 GHz) | −11.01 / −10.78 dB | −10.01 / −9.93 dB | about 1 dB at each edge |

The complex frequency-shift fit (`scripts/copper_shift_fit.py`'s misfit) finds a 0.28 MHz flat-to-bent shift at P2, and the misfit does not improve with a shift (0.0512 → 0.0511). **The bend does not detune this patch. It changes its input impedance** (deeper match, wider band). The bend axis is parallel to y, so the curvature is along the non-radiating width. At R = 100 mm it is worth about 1 dB at the band edges and 10 MHz of P1 bandwidth for a design optimized on the bent board. A design optimized flat would presumably recover the 1 dB; that was not run. So the conformal qualifier matters for the match, not for the resonance.

## X3: power balance, gain, absorber loading (M7)

`--radiation f1,...` (P1, `--verify-only`) computes, per frequency:

- the accepted port power ½ Re(V I*);
- the Poynting flux through the air box shrunk by 10 % (inside the UPML, outside the board);
- the FR-4 loss ½ ω ε_r tan δ ∫|E|² (the exact P1 quadrature, `driven::scattering::e_norm_sq_integral`);
- the Love-equivalence NTFF directivity on the same box (`postproc::ntff`).

Conductors are PEC, so copper loss is not included and the efficiency is radiation against dielectric loss only.

| design / mesh | f (GHz) | η_rad = P_rad / P_in | P_diel / P_in | balance residual | D broadside (dBi) | gain (dBi) | front/back (dB) |
|---|---|---|---|---|---|---|---|
| final, rbD_L1 | 2.38–2.50 (7 points) | 0.611–0.616 | 0.362–0.366 | 2.2–2.3 % | 6.49–6.60 | 4.38–4.46 | 12.9–14.7 |
| final, rbD_L3 | 2.40 / 2.44 / 2.4835 | 0.622 / 0.623 / 0.622 | 0.366 / 0.365 / 0.366 | 1.2 % | 6.55 / 6.61 / 6.67 | 4.49 / 4.55 / 4.61 | 12.3–13.3 |
| initial 38 × 29, L1 | 2.29–2.33 (its own dip) | 0.573 | 0.419 | 0.8 % | 6.49–6.55 | 4.07–4.13 | 13.3–14.0 |

- **The balance closes**: port power = flux + FR-4 loss to 2.3 % on L1 and 1.2 % on L3. Efficiency and gain move by 1 % and 0.1 dB from L1 to L3.
- **Realized gain** is gain × (1 − |S11|²). The rows list it at the coarse model's own S11 (`realized_gain_broadside_dbi`). With the P2/L1 in-band |S11| ≤ −10.8 dB it is within 0.4 dB of the gain, 4.0–4.6 dBi across the band.
- **Absorber loading** (`x3/*air60*`, `*air80*`, P1/L1, the same design re-meshed with 60 and 80 mm air):

  | air margin | dip (GHz) | −10 dB band (MHz) | η_rad at 2.44 GHz |
  |---|---|---|---|
  | 40 mm | 2.39634 | 93.3 | 0.615 |
  | 60 mm | 2.39878 | 94.6 | 0.619 |
  | 80 mm | 2.39818 | 91.9 | 0.607 |

  The dip moves by at most 2.4 MHz and the band by at most 1.4 MHz from 40 mm, not monotonically. The air remesh is part of these differences. **The UPML does not load the antenna at the level of the 25 MHz bandwidth gap or the X1 drift.**
- **The bandwidth gap to the closed form** (94.9 MHz FEM against 68–70 MHz Jackson-Alexopoulos): the only loss in the model is tan δ. So P_d / P_tot = tan δ · Q_t · (W_e,sub / W_e) ≤ tan δ · Q_t, which gives **Q_t ≥ 0.362 / 0.02 = 18.1** and a VSWR-2 bandwidth ≤ 95.3 MHz. The FEM's 94.9 MHz (−10 dB, a stricter criterion) sits at that bound. The closed form puts Q_t at 24.5 (70.4 MHz) because it predicts a radiated fraction of 0.51 and a dielectric fraction of 0.49, where the FEM gives 0.62 and 0.36. So **the FEM patch radiates more strongly than the infinite-ground closed form** (Q_rad ≥ 29 against the closed form's 48.5 from space and surface wave). The bandwidth is not over-broadened by loss: the gap is in the radiation Q, which the closed form overestimates for this 62 × 53 mm finite-ground board (front/back only 13 dB). At the area-equivalent width (45.4 mm) the closed form gives 67.9 MHz (`[x3.closed_form_final_area_width]`), and the conclusion does not change.

## X4: baselines (M2)

**The 4-parameter rectangle** (`--rect4`). This is the same copper basis collapsed to [ΔW, ΔL] (each moves both opposite edges by half its value, as the sum of that edge's splines; `OutlineBasis::eval_rect`), plus the inset and width DOFs. It uses the same soft-max objective, guard and band shift (44.94 MHz, the final step's refitted δ), and the same feed re-base relief as the 38-DOF chain:

| stage | fixture | worst-of-band on the shifted band (L1) | solves | terminal | as built |
|---|---|---|---|---|---|
| cold | L1 | −2.31 → −9.75 dB | 165 | distortion-limited (feed) | 37.93 × 27.97 mm, probe 1.85 mm, inset 4.80 mm |
| re-based at its probe (`rbR_L1`) | rbR_L1 | −9.62 → **−9.92 dB** | 175 | distortion-limited (widening) | **46.07 × 27.89 mm**, probe 2.97 mm, inset 3.90 mm |
| the 38-DOF chain, for comparison | L1 → rbB → rbD | → −10.78 dB | 655 | distortion-limited (widening) | barrel outline, 49.75 mm at the centre line, area 1309 mm² |

On its own P1/L1 mesh the rectangle's −10 dB band is **86.9 MHz**, against 93.3 MHz for the 38-DOF design. **At P2/L1 its 9 in-band points are all below −10 dB: worst −10.33 dB** (at 2.4835 GHz; −10.79 at 2.400), against −10.78 dB for the 38-DOF design. The rectangle widens too (38 → 46 mm), so most of the gain is the textbook widening that the review expected. The barrel outline adds about 6 MHz of bandwidth and 0.9 dB of worst-of-band on the coarse model.

**Gradient-free on the 38 DOFs** (`--gradient-free N`, separable CMA-ES, λ = 14, σ₀ = 0.5, from x = 0 on L1, unshifted). This is the same problem, objective, basis, volume guard and fixture as `../copper/L1_copper_38.toml`, where the adjoint L-BFGS goes from −1.47 to −10.04 dB. Solves are counted the same way on both sides. One solve is one frequency of the 5-point band, with one LU factorization; the adjoint reuses the forward LU for its adjoint solve. The 5 solves of the x = 0 evaluation, the FD gates and the verification sweeps are excluded. The adjoint counts are rebuilt from its trajectory: every line-search trial that passed the volume guard costs 5 solves. Guard-rejected samples cost nothing on either side.

| optimizer | seed | solves to −5 dB | −8 dB | −9 dB | −9.5 dB | −10 dB | best reached (solves used) |
|---|---|---|---|---|---|---|---|
| adjoint L-BFGS (v5 `L1_copper_38`) | – | 20 | 30 | 30 | 35 | 60 | −10.04 dB (125) |
| sep-CMA-ES, 114 evaluations (`L1_copper_38_cma114`) | 1 (default) | 275 | 370 | 405 | 520 | not reached | −9.56 dB (535) |
| sep-CMA-ES, 190 evaluations (`L1_copper_38_cma190_s2`) | 2 | 300 | 385 | 385 | 385 | 695 | −10.02 dB (785) |
| sep-CMA-ES, 190 evaluations (`L1_copper_38_cma190_s3`) | 3 | 265 | 395 | 395 | 395 | not reached | −9.77 dB (690) |

**The gradient is worth about an order of magnitude here.** Over three seeds, CMA-ES needs **385–520 solves to reach −9.5 dB, against 35 for the adjoint (11–15×)**. Only one seed of three reaches −10 dB within its budget, at 695 solves against 60 (12×). At the adjoint run's whole optimization budget (125 solves, 25 band evaluations) it is still above −3.8 dB. With enough solves (seed 2: 785) the gradient-free search does match the adjoint's final result (−10.02 against −10.04 dB). So the gradient's advantage on this problem is speed, about 11–15× in solves, not a better optimum. Only σ₀ = 0.5 and the default population size were tried, so a tuned gradient-free method would do better than these rows.

## X5: realizable feed (M5)

`--freeze-feed-width` pins the probe-width DOF at 0, so the port keeps the fixture's `probe_w`. The fixtures `rbG_L1` (1.6 mm) and `rbH_L1` (1.27 mm, an SMA pin) are `rbD_L1` with only `probe_w` changed. The final design is carried with `--rebase-feed --feed-support-legacy`. The legacy support drags the +y edge, so the carried patch is 0.08–0.09 mm shorter at the feed (`[copper].patch_length_mm` 28.72 against 28.80 mm). That only affects where the re-optimization starts.

| design | probe | model | worst of the 9 in-band points | −10 dB band (P1 dense) |
|---|---|---|---|---|
| final (v5) | 4.27 mm strip | P2/L1 | −10.78 dB | 93.3 MHz at P1 |
| final copper, probe swapped, not re-optimized | 1.6 mm | P1/L1 | (P1 sits 45 MHz low) | 92.4 MHz, dip +6.9 MHz |
| final copper, probe swapped, not re-optimized | 1.27 mm | P1/L1 | | 91.9 MHz, dip +8.5 MHz |
| **re-optimized final step, width pinned** (`x5/rbG_L1_copper_38_sm45_w16`) | **1.6 mm** | **P2/L1** | **−10.62 dB** (−10.65 at 2.400, −11.09 at 2.4835) | 91.9 MHz at P1 |
| **re-optimized final step, width pinned** (`x5/rbH_L1_copper_38_sm45_w127`) | **1.27 mm** | **P2/L1** | **−10.65 dB** (−10.65 at 2.400, −11.04 at 2.4835) | |

The 1.6 mm re-optimization ran 10 steps and 130 solves, from −9.58 to −10.70 dB on the shifted band. The 1.27 mm one ran 11 steps and 130 solves, from −9.24 to −10.66 dB. Both stopped distortion-limited. **The 4.3 mm port strip was not what carried the bandwidth.** With an SMA-scale probe the design keeps 92 MHz at P1 and still clears −10 dB across the band at P2/L1. The patch outline barely changes (49.74 × 28.82 mm). The probe is still an idealized uniform lumped port of the pin's width, and a coax with its own reactance is not modelled.

## What was not done

- **X1 rim 0.1 mm alone** (`x1rim010`, 1.18M DOFs) was dropped for the shared vCPU quota and the budget. The combined 0.1 mm rim plus 0.3 mm slab rung covers the fine end.
- **Palace with uniform refinement** (`init_L1u1_p2`, 6.7M DOFs) was aborted after 1.3 h for the same reason, before its first frequency finished. The Palace evidence is p-refinement on L1 only.
- **A port-free eigenfrequency** (M1 item 4) was not run, so the feed reactance and the patch resonance are not separated.
- **P = 2 on the refined geode meshes**: geode's direct LU at P2 needs about 130 GiB at 0.8M DOFs, and every refined rung is larger.
- **The flat-optimized design** (X2: optimize flat, then wrap) was not run.
- **Copper loss**: conductors are PEC.

## Reproducing

Each TOML's `[meta].command` is the exact command. The MANIFEST in `crates/geode-core/tests/benchmark_provenance.rs` maps the design copies (`<stem>.design.toml`) to their committed sources. Runs on `rb*` fixtures need `--feed-support-legacy`, as in `../copper/`. The X1 dips used `PATCH_CONFORMAL_GEOMS=bent`. L1 and the coarse rungs used `PATCH_CONFORMAL_FIXED_GHZ=2.40,2.4835`, a 0.2 MHz tolerance and the default scan. The 0.20 mm rim, the 0.30 mm slab and the combined rung used `PATCH_CONFORMAL_FIXED_GHZ=2.40`, a 0.5 MHz tolerance and a 2.31–2.41 GHz scan (2.33–2.43 GHz for the combined rung); both settings are recorded in each `[meta]`. Logs are in `logs/` (host paths scrubbed).
