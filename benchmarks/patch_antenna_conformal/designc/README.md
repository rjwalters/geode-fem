# Design C: eigenmode, efficiency, a refinement datapoint, and an FD-gradient control (#1148)

The antenna-v6 review asked for four measurements on the headline design C, `../recentre/rbG_L1_copper_38_sm57_w16` (38 copper DOFs, fixed 1.6 mm FR-4, a 1.6 mm probe). Every number below comes from a raw record committed in this directory. `../scripts/designc_summary.py --write` collects them into `designc_summary.toml`, and `crates/geode-core/tests/patch_conformal_designc.rs` pins them. The test re-derives each pin from the raw record and checks the summary's `[[input]]` sha256 list against the committed files, so a stale summary fails. σ₀ = 1 throughout.

## Summary

| item | question | answer |
|---|---|---|
| C1 | Where is the port-free resonance relative to the matched dip? | **Below it.** On L1 at p = 2 the matched dip lies **16.3 MHz above** the port-free TM01 eigenfrequency for the initial patch and **30.9 MHz above** it for design C. That is the series-inductive-probe direction the review gave, and the opposite of v6's stated expectation. The shift is a few tens of MHz, not a few MHz. The closed-form (Hammerstad) estimate sits **92 MHz above the port-free cavity itself**, so the probe does not explain the gap to the closed form. |
| C2 | What are design C's efficiency and gain? | **η_rad = 0.61–0.63, gain 4.45–4.66 dBi** (PEC conductors, FR-4 loss only). The power balance closes to 0.8 % on L1 and 1.6 % on the 0.2 mm rim. Design A gave 0.61–0.62 and 4.38–4.46 dBi on L1, so the narrower 1.6 mm feed does not change the loss split. On the Palace p = 3 match (worst in band −10.46 dB, mismatch loss 0.41 dB), the realized gain is at least **4.13 dBi**. |
| C3 | Does a refinement on design C itself confirm the drift estimate? | **Partly.** Palace p = 2 on design C's 0.2 mm-rim mesh (2.47M DOFs) gives a band of **2.3981–2.4912 GHz, worst in band −10.32 dB**, so ISM is still covered. The lower edge sits **4.6 MHz above** Palace p = 3 on L1, which leaves **1.9 MHz** of lower-edge margin (6.5 MHz at p = 3 on L1). That is between the review's two thresholds: within 2 MHz would have corroborated the order-1 estimate (6.4 MHz of further drift), and 8 MHz would have confirmed the rim fit (13.5 MHz). |
| C4 | What does the adjoint gradient buy against a finite-difference gradient? | **A factor of 39 in solves, at every level.** L-BFGS with a forward-difference gradient (38 + 1 band evaluations per gradient) takes the adjoint's path step for step and reaches the same −10.04 dB. It needs 1 365 solves to −9.5 dB and 2 340 to −10 dB, against the adjoint's 35 and 60. That puts it behind sep-CMA-ES (385–520 to −9.5 dB). The run is censored at its 2 500-solve budget, after it had reached −10 dB. |

## C1: port-free eigenmode (`c1/`)

Palace's eigenmode driver ran on the same meshes, materials, PEC surfaces and first-order absorbing wall as the X1 / #1134 driven sweeps (the same Palace build, `palace-build-info.txt`; meshes in `meshes.txt`). The port gap (tag 11) is **open**: no boundary condition, so the feed is removed and the 1.6 mm gap is plain substrate. Shift-and-invert targets 2.30 GHz. `scripts/palace_patch_config.py --eigen-target 2.30 --eigen-n 6 --port open` writes the config. The linear solve is MUMPS on the true complex p = 2 operator (one level, complex coarse solve). The default iterative solve took about 4 minutes per shift-and-invert step and was aborted. With the absorbing wall the problem is damped, so each eigenvalue is complex, and Q = Re f / (2 Im f). The patch mode is the mode with the largest substrate electric participation (`domain-E.csv`, `p_elec[1]`). It is unambiguous: Q 19–22 and 0.80–0.83 of the electric energy in the substrate, against Q < 3 and at most 1.3 % for each of the six air-box modes.

| design (L1, p = 2) | port-free f (GHz) | Q | substrate p_elec | matched dip, driven Palace p = 2, same mesh (GHz) | dip − port-free (MHz) |
|---|---|---|---|---|---|
| initial 38 × 29 mm | **2.34075** | 22.2 | 0.83 | 2.35703 (X1 `init_L1_p2`) | **+16.3** |
| design C | **2.39913** | 19.3 | 0.80 | 2.42998 (#1134 `sm57_p2`; band centre 2.4309) | **+30.9** |

The same mode with the 50 Ω lumped port as a load (`--port lumped`, no excitation) moves up by **12.2 MHz** (initial) and **23.1 MHz** (design C), and its Q drops to **11.5** and **8.6**, about half. That is what a resonator coupled near critically to its feed should do.

What this settles:

- **The sign.** v6 expected the probe reactance to pull the matched dip *below* the cavity resonance. It is the other way round, as the review's series-L model predicts: the dip is above the port-free resonance, on both designs. The re-optimized design C, with a 5.1 mm inset (the initial patch's is 7 mm), is pulled further, by 31 against 16 MHz.
- **The size.** The shift is 16–31 MHz. That is more than the "few MHz" the review estimated from h/λ₀, but still far from closing a 50–90 MHz gap.
- **The closed form.** The textbook transmission-line estimate for the initial patch is 2.4332 GHz. That is **92.5 MHz above the port-free eigenfrequency** and 76 MHz above the matched dip at the same order. The port-free cavity is *lower* than the closed form, so the gap is in the cavity model (finite 12 mm ground and substrate margin, the curved board, the closed form's own 1–2 % accuracy, and the remaining discretization: X1 puts p = 2 on L1 about 9 MHz low), not in the feed. Removing the feed moves the resonance *away* from the closed form.
- These are p = 2 numbers on L1. p = 3 moved the initial patch's dip by +8.9 MHz (X1). The eigenfrequency was not computed at p = 3, so the dip-minus-eigenfrequency separation is quoted at p = 2, where both are computed on the same discrete model.

## C2: efficiency and gain (`c2/`)

The v6 X3 path (`--verify-only --radiation`, P1): accepted port power, Poynting flux through the air box shrunk by 10 %, FR-4 loss from the exact P1 quadrature, and the Love-equivalence NTFF on the same box. Conductors are PEC, so η_rad is radiation against dielectric loss only.

| design / mesh | points | η_rad | P_diel / P_in | balance residual | D broadside (dBi) | gain (dBi) |
|---|---|---|---|---|---|---|
| **C, rbG_L1**, on its shifted band (2.343–2.4265 GHz, the band it was optimized on, 57 MHz below ISM) | 5 | **0.625–0.626** | 0.367–0.368 | 0.7–0.8 % | 6.49–6.60 | **4.45–4.56** |
| C, rbG_L1, at the ISM frequencies (2.38–2.50 GHz, as X3 for design A) | 7 | 0.618–0.626 | 0.367–0.372 | 0.7–1.0 % | 6.54–6.67 | 4.51–4.59 |
| **C, 0.2 mm rim** (on-cylinder air morph), in band | 5 | **0.612–0.613** | 0.371–0.372 | 1.5–1.6 % | 6.67–6.79 | **4.54–4.66** |
| A (v6 X3), rbD_L1, 2.38–2.50 GHz | 7 | 0.611–0.616 | 0.362–0.366 | 2.2–2.3 % | 6.49–6.60 | 4.38–4.46 |
| A (v6 X3), rbD_L3 | 3 | 0.622–0.623 | 0.365–0.366 | 1.2 % | 6.55–6.67 | 4.49–4.61 |

- On L1, design C is 0.012 more efficient than design A, and its gain is about 0.1 dB higher. The two models of C (L1 and the 0.2 mm rim) differ by 0.013 in η_rad and 0.1 dB in gain, about as A's L1 and L3 rows do.
- **Realized gain** is gain × (1 − |S11|²). The rows in the TOMLs list it at each model's own S11. On the most accurate match, Palace p = 3 (worst in band −10.46 dB, #1134), the mismatch loss is at most 0.41 dB. So the realized broadside gain is at least 4.54 − 0.41 = **4.13 dBi** across ISM.

## C3: Palace p = 2 on design C's 0.2 mm-rim mesh (`c3/`)

The X1 / #1134 driven setup (`AdaptiveTol` 1e-3, 1 MHz step, GMRES to 1e-8, 16 ranks) on design C's moved 0.2 mm-rim mesh with the on-cylinder air morph (`meshes.txt`; the mesh the #1134 geode run wrote). `scripts/palace_ingest.py` wrote `c3/palace_summary.toml`.

| model | DOFs | −10 dB band (GHz) | width | worst in band | margin lo / hi (MHz) |
|---|---|---|---|---|---|
| geode P1, 0.2 mm rim (#1134) | 431 528 | 2.3849–2.4782 | 93.3 MHz | −9.23 dB | 15.1 / −5.3 |
| Palace p = 2, L1 (#1134) | 884 536 | 2.3847–2.4770 | 92.3 MHz | −9.08 dB | 15.3 / −6.5 |
| Palace p = 3, L1 (#1134) | 2 503 077 | 2.3935–2.4864 | 93.0 MHz | −10.46 dB | 6.5 / 2.9 |
| **Palace p = 2, 0.2 mm rim (this)** | **2 465 984** | **2.3981–2.4912** | **93.1 MHz** | **−10.32 dB** | **1.9 / 7.7** |

- **ISM is still covered**, at −10.32 dB worst in band, so the target holds on the most refined model computed so far. But the lower-edge margin is down to 1.9 MHz.
- Against Palace p = 3 on L1, the band moves up **4.6 MHz** (dip 4.7 MHz). These are two different refinements of comparable size: p + 1 on L1, and rim h-refinement at p = 2. Neither contains the other, so 4.6 MHz is not a converged drift. It says that the rim still matters at p = 2 and p = 3 on L1 is not converged.
- Against the #1134 drift estimates carried over from the initial patch: 4.6 MHz is below the order-1 Richardson value (6.4 MHz), well below the rim fit (13.5 MHz), and above the review's "within 2 MHz" corroboration threshold. **The README's "which may be low" was right.** The data give the design no evidence of slack beyond the 1.9 MHz left here. If the drift above this model is more than about 2 MHz, the design misses 2.400 GHz.
- The step from p = 2 on L1 to p = 2 on the 0.2 mm rim is +13.4 MHz. geode P1 on the same 0.2 mm-rim mesh sits 13.2 MHz below it, next to Palace p = 2 on L1, as X1 found for the initial patch.
- Wall time 1.5 h on an r6i.4xlarge (12 PROM samples).

## C4: forward-difference-gradient L-BFGS (`c4/`)

The X4 problem exactly: the 38-DOF copper basis on L1, the soft-max |S11|² objective at the 5 band points, x = 0 as the start, the same L-BFGS (memory 8), Armijo line search, step cap and 0.25 volume guard as the adjoint run `../copper/L1_copper_38.toml`. The only change is the gradient: `--fd-gradient` takes one-sided forward differences (step 1e-5 design units) of the band objective, one forward band evaluation per DOF, at the start and at every accepted iterate. Line-search trials are forward only. The 38 perturbations ran on 8 threads (`--fd-jobs 8`, wall time only). The adjoint is evaluated only at the start and at the reported design, for comparison and for the FD gates.

**Counting** is X4's: one solve is one band frequency with one factorization. The start's band evaluation and every guard-passing line-search trial cost 5 solves, as for the adjoint. Each FD gradient costs 38 × 5 = 190 more, so a step costs 195 solves against the adjoint's 5. FD gates and verification sweeps are excluded on both sides. The run was given a 2 500-solve budget (`--max-solves 2500`) and stopped on it after step 12 (2 535 solves, terminal `solve_budget`), so it is **censored**. It had reached −10 dB by then.

| optimizer | solves to −5 dB | −8 dB | −9 dB | −9.5 dB | −10 dB | best (solves) |
|---|---|---|---|---|---|---|
| adjoint L-BFGS (X4) | 20 | 30 | 30 | 35 | 60 | −10.04 dB (125) |
| **FD-gradient L-BFGS (this)** | **780** | **1 170** | **1 170** | **1 365** | **2 340** | **−10.04 dB (2 535, censored at the 2 500 budget)** |
| sep-CMA-ES, 3 seeds (X4) | 265–300 | 370–395 | 385–405 | 385–520 | 695 (1 seed of 3) | −9.56 / −10.02 / −9.77 dB |

- **The FD gradient takes the adjoint's path.** It agrees with the adjoint to 4.6e-7 (relative norm) at the start and 5.2e-4 at the reported design. The worst-of-band value at each of the 12 steps matches the adjoint run's to 1.7e-5 dB, and the run reaches the adjoint's −10.04 dB at the same step (12).
- **So the cost ratio is exactly the gradient's price: 39× at every level** (195 against 5 solves per step, and every step took one line-search trial on both sides). With a correct gradient, the iterates do not depend on how it was computed, and the solve count is (1 + n) band evaluations per step against the adjoint's 1.
- **The FD-gradient run is behind gradient-free search.** sep-CMA-ES reaches −9.5 dB in 385–520 solves, 2.6–3.5× fewer than FD-gradient L-BFGS (1 365). Within the adjoint's whole 125-solve budget the FD run has not finished its first gradient. So the order of magnitude X4 measured is the adjoint's price for a gradient (one band evaluation plus an adjoint back-substitution on the same LU), not "gradient against no gradient". On these 38 DOFs a gradient by finite differences is worth less than none.
- **Wall time.** 16 888 s (4.7 h) on an r8g.4xlarge with 8 FD threads, about 20.6 min per step. One FD gradient took 1 164–1 175 s for 38 band evaluations, about 31 s each against about 79 s for one forward + adjoint band evaluation on one stream. The thread parallelism changes wall time, not the solve count.

## Not done

- **The eigenmode at p = 3**, which would put the dip-minus-eigenfrequency separation on the headline's footing. At p = 2 both are on the same discrete model, which is what the separation needs.
- **A port-shorted eigenmode** (`--port short` exists). A PEC strip at the inset is a shorting pin, a different structure, not the unloaded cavity.
- **Palace p = 3 on the 0.2 mm rim** (about 7M DOFs) and any further refinement of design C. So how much drift remains above the C3 value is not measured.
- **Copper loss.** Conductors are PEC, so η_rad and gain are upper bounds by the copper loss.

## Reproducing

Each geode TOML's `[meta].command` is the exact command. The MANIFEST in `crates/geode-core/tests/benchmark_provenance.rs` gives each one with its committed design source. The Palace runs used the vendored `reference/palace/docker/Dockerfile` (Palace `fba6a5b`), rebuilt on the x86 worker (the saved image is arm64). Their configs are committed beside their outputs and were written by `scripts/palace_patch_config.py` (driven: the X1 flags; eigenmode: `--eigen-target 2.30 --eigen-n 6 --port open|lumped`). The fixtures are the committed gmsh 4.15.2 YAMLs (`reference/gmsh/patch_conformal_{L1,rbG_L1,rbGrim020_L1}.yaml`, sha256 in `../provenance/`). The Palace meshes are listed in `meshes.txt`. Logs are in `logs/` and beside each Palace run, with host paths scrubbed.

geode ran on an r8g.4xlarge (Graviton4) and Palace on an r6i.4xlarge (x86), both in us-east-1.
