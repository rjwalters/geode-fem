# Changelog: conformal-antenna-diffopt.7 → .8

Inputs: the v7 reviews (Opus 40/44, Fable 40/44, no critical flags; scratchpad files, not
`.review/` siblings). No new experiment; every new number is read from committed records.
v1–v7 and their critic directories are unchanged (`verify_evidence.py` diffs them against the
merge base with `origin/main`).

## Must-fix

| Source | Resolution |
|---|---|
| Opus M1 "rim-resolved" | Abstract and §11: "re-centred using Palace at third order and a rim-refined mesh"; §1 keeps "re-centred on the refined model" |
| Opus M2 adjoint cost | §5 Setup: an adjoint band evaluation takes 95.1 s against 72.1 s forward (1.32×), so in geode-fem wall time the 11× at −9.5 dB is about 8.3× (385 × 14.42 = 5,552 s against 35 × 19.03 = 666 s, `palace_loop_summary.toml [cost.geode]`); abstract and §11 carry the 8.3× |
| Opus M3 curvature attribution | §4: "Design A's copper (the strip-fed design of Section 6) on a flat board ..."; "Design C was not run flat" |
| Fable M1 / Opus S2 air margin | §4: the dip moves 4.34 / 4.31 MHz at 60 / 80 mm, levelled by 60 mm; edges 3.0 / 4.3 (lower) and 4.2 / 2.9 (upper); bandwidth 91.0 / 92.3 / 89.7 MHz; about 1.3 MHz re-meshing scatter (the upper-edge reversal 4.24 − 2.95). Table 5's air row is the dip's −4.3 MHz. §7 budget: 6.5 − 4.6 − 4.3 = −2.4 ± 1.3 MHz; up to 13.5 − 4.6 = 8.9 MHz more if the drift estimates transfer; miss 2.4 − 1.3 = 1.1 to 2.4 + 1.3 + 8.9 = 12.6, stated "1 to 13 MHz" (abstract, §1, §7, §11). "had not levelled off" removed |

## Should-fix and nits

| Source | Resolution |
|---|---|
| Opus S1 "tuned" | Title "... Against Gradient-Free Search at Several Settings"; §5 defines it (two or three values of each optimizer's main setting) and says the best CMA-ES setting is the smallest σ0 tried |
| Opus S3 scaling mechanism | §5: adjoint 85 / 40 / 35 solves to −9.5 dB at 4 / 14 / 38, best gradient-free 205 / 115 / 385; only the step to 38 is gradient-free slowing; the bases are separate, not nested; §11 says so in a clause |
| Opus S4 reduced-cost FD | Added `koziel2020expedited` (Broyden updates, Eng. Comput. 37(3), DOI 10.1108/EC-01-2019-0023) and `koziel2012robust` (adjoint / FD trust region, IJRFMiCAE 22(1), DOI 10.1002/mmce.20579), verified on Crossref (DOI, title, authors, pages; the 2012 abstract read on Semantic Scholar); "a plain forward-difference gradient"; "we did not try a reduced-cost finite-difference scheme" |
| Opus S5 re-centring feasibility | §7: the 93.0 MHz band is 9.5 MHz wider than ISM, less than the 11.5 MHz spread of the estimate, so the bandwidth binds |
| Opus S6 sep-CMA-ES check | Not run; §5 notes NEWUOA from PRIMA gives 11.4× on its own |
| Opus S7 / Fable S4 closed form | Labelled design A's rectangle (C's outline 49.72 × 29.00 mm); the Q inequality now uses design C's loss fraction: Q_t ≥ 0.3667 / tan δ = 18.3 |
| Opus S8 / Fable S3 FDTD | Moved into §4 as "An FDTD cross-check"; the first harness cut to one clause; flat comparison notes P2/L1 is itself about 9 MHz below p = 3; FR-4 conductivity model tan δ 0.0195–0.0205; bent widths 37.2 / 37.6 mm; the refined models place the bent dip between 2.3611 and 2.3667 GHz; the cost side note cut (Opus S10, Fable N4) |
| Opus S9 | Five equally spaced band frequencies 2.400–2.4835 GHz; inset and width coefficients 1 mm per unit |
| Opus S11 abstract density | 4/14-parameter sentence folded; −10.46 / −10.32 dB pair dropped; "with 230 against 30"; the "fastest to reach" qualifier once per ratio |
| Fable S1 guard-limited L-BFGS | §5: the adjoint's final level is set by step rejection at the volume guard, while CMA-ES and NEWUOA move along the boundary; projected or penalized step not tried; §11 clause |
| Fable S2 probe analogy | §4: the port is a resistive sheet; the load raises the mode 12.2 / 23.1 MHz and the driven dip sits 4.1 / 7.7 MHz above the loaded mode; "A series inductance, as in a coaxial probe, would move the match the same way" |
| Fable S5 n = 4 crossover | §5: gradient-free faster to −5 and −8 dB (5 and 45 against 30 and 55 solves), crossover near −9 dB |
| Fable S6 | Table 2 caption: FD budget checked after each gradient; §5: the adjoint run's recorded 190 driven solves include 65 for the gradient checks |
| Fable S7 / Opus nit | Table 4 caption: Palace duplicates the vertices on interior PEC and port surfaces |
| Nits | "perfectly conducting (PEC) conductors"; eigenmode settings (7 modes, backward error below 5e-9); Figure 3(a) marker on NEWUOA npt 77 and the σ0 = 1.0 omission named; Table 2 caption: "Best" excludes the uncounted start, seed values; "seed 2 also reaches −9.5 dB at the same count under both"; "8 finite-difference jobs in parallel". Kept: the §9 mutation-testing sentence (approved section) and the `v6/` artifact paths |

## Judge round on the v8 PR (changes requested)

| Item | Resolution |
|---|---|
| The 65 uncounted adjoint solves are not all gradient checks | §5: "65 that are not counted: 35 for the three gradient checks and a fresh evaluation of the reported design, and 30 for the verification sweeps (9 in-band and 21 wide points)"; the verifier rebuilds 30 + 5 + 9 + 21 from `copper/L1_copper_38.toml` and checks 125 + 65 = 190; provenance row corrected. The error came from a reviewer's "(65 for the FD gates)" taken without checking |
| Nits | Table 4 caption gives the 80,200 extra Palace unknowns, pinned with the Palace log line; provenance miss-band row now 1.1–12.6 MHz; §7: the ±1.3 MHz "is the scatter of the air re-meshes only, applied once to the sum"; Figure 4 caption: the budget uses the dip's 4.3 ± 1.3 MHz. Five further mutations each killed (35 → 30, 21 → 20, "eliminates the edges", 80,200 → 80,000, "twice") |

## Build and checks

- `papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.8`: 19 pages, 0 overfull boxes, no `??`, 74 entries, all cited.
- `python3.14 -I verify_evidence.py`: PASS. It pins every new number from the records. Seventeen mutations,
  each killed: 8.3 → 8.4; 95.1 → 95.2; 666 → 667; 4.31 → 4.21; 89.7 → 89.6; "1 to 13" → "2 to 13";
  12.6 → 12.5; "Design A's copper" → "The same copper"; "was not run flat" → "was run flat";
  "rim-resolved" reinserted; "Tuned" in the title; 40 → 45 (adjoint scaling); 55 → 50 (crossover);
  4.1 → 4.0 (loaded-mode offset); 37.6 → 37.8 (Meep width); 18.3 → 18.4; 65 → 60.
- Figures: all four scripts run under `python3.14 -I`; Figure 3 adds a marker to the NEWUOA npt 77 curve.
