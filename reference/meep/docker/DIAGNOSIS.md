# Meep/GEODE disagreement on the bent patch: diagnosis (#1088)

This record answers the 2026-10-15 operator ruling: diagnose why Meep and GEODE disagree on
the bent `patch_conformal_L0` fixture before running Meep at any higher resolution
(budget about $5–15).

**The disagreement** (`AWS_R4.md`, "Comparison with GEODE on the same fixture"). GEODE's
bent dip rises with refinement to at least 2.3566 GHz (−23 to −29 dB, Re Z 44–47 Ω), and
Palace agrees with GEODE on the same meshes. Meep R1/R2/R4 show nothing there (about
−1.2 dB, Re Z 3–4 Ω). Meep's only deep feature sits at or below the 2.20 GHz window edge.

**Finding.** The cause is in the Meep model, and it is identified and reproduced here:

- **The disagreement is caused by how the harness voxelizes its metal on a curved board.**
  `matched_antenna.py` models the patch and ground as one-cell `mp.metal` shells outside
  the substrate. It uses no subpixel averaging. Meep zeroes each E component wherever
  *that component's own* sample point lies in metal.
- **On the curved board this creates mismatched columns.** In many columns under the
  patch, an Ez sample next to the substrate face falls inside the shell, but the
  tangential (Ex, Ey) metal plane stays one sample further out. The discrete gap is then
  thinner for Ez (h_E) than for the tangential field (h_H). The cavity's electric
  energy sees h_E and its magnetic energy sees h_H, so the resonance drops by about
  √(⟨1/h_H⟩/⟨1/h_E⟩).
- **The model explains the ~250 MHz artifact.** Nothing in it is fitted, but it is
  anchored: every predicted dip is the measured flat R2 dip times a factor computed from
  the grid. At R2 the grid dump gives a factor of 0.893, which predicts **2.086 GHz**
  for the bent board. The measured dip is **2.087 GHz**.
- **The model is only good to about 1–2%.** On the grid-aligned bent board it misses by
  23.5 MHz, and on the inset test by 29.5 MHz (section 3). It cannot resolve
  GEODE-sized bend shifts of about 1 MHz.
- **The flat board does not disagree with GEODE.** It is consistent at R2 (factor 1),
  and Meep's flat R2 dip is **2.335 GHz**, 8 MHz below GEODE's flat P1/L3 dip.
- **The same curved board with Yee-cell-aligned metal behaves like the flat one.** Its
  dip is **2.333 GHz**, a bend shift of **−2.6 MHz** against GEODE's −1.0 to −1.6 MHz.
  With the harness's shells the shift is −248 MHz. This agreement with GEODE is a
  *measurement*. The model predicts a shift of about −26 MHz for this board, so it does
  not predict the agreement.

So GEODE (and Palace) are not contradicted. The committed Meep R2/R4 bent spectra (R1 was not dumped) are
dominated by a staircasing artifact of the harness. **R8 with the current harness is
not recommended** (see the last section).

Records: `benchmarks/fdtd_density_baseline/meep_runs/diag/`. Verify with the standard
library (the checks stay active under `python -O`):

```sh
python3 benchmarks/fdtd_density_baseline/meep_diag_verify.py
python3 -O benchmarks/fdtd_density_baseline/meep_diag_verify.py
python3 benchmarks/fdtd_density_baseline/meep_diag_mutation_test.py   # every corruption must fail
```

`--write` regenerates `meep_runs/diag/diagnosis.json`. The verifier checks the
environment replay, provenance and spectra of every run, and the grid-dump analysis
(including the E/H gap model). It also checks the GEODE comparison, re-derives every
number quoted below, and asserts the findings.

## 1. Setup audit (no compute)

Meep side: `reference/meep/docker/matched_antenna.py` (the harness, unchanged here) and
`matched_initial_2g4.json`. GEODE side: `reference/gmsh/patch_conformal_L0.yaml`,
`reference/gmsh/patch_antenna.geo`, `reference/gmsh/generate_patch_fixture.py`,
`crates/geode-core/src/mesh/patch.rs`, `crates/geode-core/src/driven/ports/lumped.rs`,
`crates/geode-core/examples/patch_conformal_convergence.rs`. Palace side:
`benchmarks/patch_antenna_conformal/scripts/palace_patch_config.py`. "Discrete" entries
come from the grid dumps in section 2, which read Meep's own `chi1inv` at its Yee samples.

| Item | GEODE / Palace | Meep harness | Verdict |
|---|---|---|---|
| Patch, substrate, ground | 38 × 29 mm patch, 1.6 mm FR-4, 62 × 53 mm substrate = ground (`patch_conformal_L0.yaml:20-25`, ground = substrate footprint `patch_antenna.geo:225`) | Same values (`matched_antenna.py:45`, `:58-61`) | Match |
| Bend | φ = x/R about an axis R below the substrate mid-plane, patch on the convex side, arc length on the mid-plane (`patch.rs:441-444`, `generate_patch_fixture.py:312-315`) | Ring sectors about z = −R, half-angles w/(2R) (`matched_antenna.py:32-38`, `:58-61`) | Match |
| Probe position | x = 0, y = +y edge − 7 mm = 7.5 mm (`patch_antenna.geo:137-138`, `patch_conformal_L0.yaml:25`) | (0, 7.5, 0) (`matched_antenna.py:56`) | Match |
| Port model | Uniform lumped port on the x = 0 rectangle w × h, loading both tangential components, Z_s = R w/h (`lumped.rs:9-37`); Palace `LumpedPort` R = 50, `+Z` (`palace_patch_config.py:63-69`) | Ez current sheet (0, w, h) in a dx × w × h resistor block loading Ey and Ez with σ = h η0/(R w dx) (`matched_antenna.py:53-55`, `:59`, `:63`) | Equivalent in the continuum |
| Port, discrete | — | R2: 3 × 4 Ez cells (1.5 × 2.0 mm), **66.7 Ω**. R4: 7 × 6 (1.75 × 1.5 mm), **42.9 Ω**, and 7.7% of the sheet source lands on metal samples. The voltage monitor spans 1.6 mm of a 2.0 mm discrete gap at R2. | **Discrepancy**, grid-dependent, identical for flat and bent |
| Source and S11 | Thevenin 2V_inc behind R, I = (2V_inc − V)/R, Z = V/I, S11 = (Z − R)/(Z + R), R = 50 Ω (`patch_conformal_convergence.rs:455`, `:490-492`; `lumped.rs:47-54`) | Norton I_inj − V/R, same Z and S11, R = 50 Ω (`matched_antenna.py:100-106`) | Match. Meep exp(−iωt) against GEODE exp(+iωt) flips Im Z only; \|S11\| and Re Z are unaffected |
| FR-4 | ε_r = 4.4, constant tan δ = 0.02 (`patch.rs:212-219`; Palace `palace_patch_config.py:56`) | ε = 4.4, D-conductivity 2π f_c tan δ at f_c = 2.45 GHz (`matched_antenna.py:49-51`) | Minor: Meep's tan δ ∝ 1/f (0.0208 at 2.36 GHz). Shifts loss, not resonance |
| Conductors | Zero-thickness PEC faces (`patch.rs:18-23`; Palace `"PEC"` `palace_patch_config.py:61`) | One-cell `mp.metal` shells **outside** the substrate faces, `eps_averaging=False` (`matched_antenna.py:60-61`, `:149`) | **Root cause on the curved board** (section 3) |
| Gap, discrete | 1.6 mm | Tangential-E metal planes at ±1.0 mm (a 2.0 mm gap) at R2 and R4. The cell z length 131.6 mm is rounded to 131.5 mm (log), so the planes do not land on ±0.8 mm. | **Discrepancy**: +25% gap, same for flat and bent |
| Box and absorber | 40 mm air, 25 mm UPML σ0 = 1, PEC backed (`patch_conformal_L0.yaml:28-30`, `:52`); Palace: first-order ABC, air shell (`palace_patch_config.py:62`) | Same extents, `mp.PML(25)` (`matched_antenna.py:46-48`, `:66`) | Different absorbers. GEODE's dip moves ≤ 2.4 MHz with the air margin (v6 X3), and Palace's ABC agrees with GEODE's UPML. Not a 250 MHz cause. Meep's PML is not convergence-tested |
| Staircasing of the curved board | Conforming tets (mesh facets) | Voxel samples per component, 256-segment prisms, no subpixel averaging | **Root cause** (section 3) |
| DFT window | — | 2.2–2.7 GHz (`matched_antenna.py:137`) | Hid the bent dip (2.09 GHz at R2) below the window |

The audit found no wrong dimension, unit, bend direction, feed position, reference
impedance or material value. The discrepancies are all in the discretization: the port
size, the gap and, decisively, the per-component metal sampling on the curved surface.

## 2. Runs (one r8g.8xlarge, us-east-2)

Environment: `diag/environment.json`. The image was rebuilt from `Dockerfile.matched` and
the explicit lock; all 179 package URLs were replayed and the exported set equals the lock.
The R2 RC oracles reproduce the committed us-east-1 `aws_pilot/rc_{volume,sheet}_R2.json`
spectra **bit for bit**, and `matched_geometry_verify.py` passes. Every run uses the
unchanged harness through `reference/meep/docker/diag_antenna.py`, which adds only a flat
geometry, alternative metal placements, a wider DFT window (1.6–3.0 GHz, or 1.2–3.0 GHz
for the two inset runs, in 0.01 GHz steps; the harness's Gaussian pulse is kept), Ez field maps and a grid dump. The
`diag_antenna.py` versions used are archived in `diag/sources/` by SHA-256. All runs use
32 ranks with the R2 stopping settings (1200/600, tolerance 0.001); every run met the
temporal tolerance. Wrapper: `diag/run_diag.sh`.

**Equivalence to the harness.** At R1 on this host (8 ranks), the harness reproduces the
committed Mac R1 record (4 ranks) bit for bit. `diag_antenna.py` with the harness's
window matches the harness to 3e−15. With the wide window it differs by 0.0058–0.0071 in complex
S11 at the shared frequencies, and the bent R2 wide-window run differs from the
committed R2 by 0.0045–0.0055. That offset comes from the DFT window alone. It is
recorded and is two orders of magnitude below the effects measured here.

| Run | Board | Metal | Dip (GHz, parabolic) | Dip (dB) | Max Re Z (Ω) at (GHz) |
|---|---|---|---|---|---|
| `flat_R2` | flat (blocks) | harness (outward) | **2.3351** | −17.7 | 39.9 at 2.33 |
| `bent_R2` | R = 100 mm | harness (outward) | **2.0869** | −13.1 | 32.9 at 2.08 |
| `ctrl_R2` | R = 10⁴ mm prisms (sagitta 0.018 mm < dx) | harness (outward) | **2.3354** | −17.5 | 39.5 at 2.33 |
| `cells_bent_R2` | R = 100 mm | Yee-cell-aligned staircase | **2.3325** | −13.2 | 33.0 at 2.33 |
| `inset_flat_R2` | flat | shells 0.4 dx inside the faces (deliberate E/H mismatch) | **1.6807** (TM010); 2.6128 (TM020-like) | −15.9; −21.5 | 59.2 at 2.61 |

- **Flat R2 against GEODE flat** (`convergence.toml [s11.flat]`): GEODE gives 2.3434 GHz
  (P1/L3) and 2.3580 GHz (P2/L1), and estimates the converged dip at 2.366–2.380 GHz.
  Meep flat R2 is 8 MHz below P1/L3 and 23 MHz below P2/L1. Neither code is converged,
  and Meep's R2 port and gap are off by the amounts in section 1. On the flat board
  there is no qualitative disagreement. The flat board resonates inside the window,
  with Re Z 40 Ω at the dip. The 66.7 Ω discrete port resistor and the shortened voltage
  path bias Meep's Re Z, so its level is not comparable to GEODE's 49–50 Ω.
- **The bend.** Meep's harness moves the dip by **−248 MHz** (2.335 → 2.087 GHz). GEODE
  moves it by **−1.0 to −1.6 MHz** on the initial patch (`convergence.toml [detune]`),
  and by +0.3 MHz on the optimized design (v6 X2).
- **The Prism machinery is not the cause.** Built from the same ring-sector prisms but
  bent about R = 10⁴ mm, the board gives 2.3354 GHz, the flat answer.
- **Corrected bent R2.** With the metal on whole Yee cells (`--shell-mode cells`), the
  bent board gives **2.3325 GHz, −2.6 MHz from flat**. That is the size of GEODE's bend
  shift. The port and the crown gap are the same as `flat_R2`. Away from the crown,
  though, the cell staircase rounds each column to the nearest plane, giving a 1.5 mm
  gap in 27 of the 38 columns against 2.0 mm on the flat board (section 3). So this
  isolates the E/H mismatch, and the remaining differences from `flat_R2` are local gap
  heights. Their net effect on the dip is the measured −2.6 MHz.

The committed bent R2/R4 records are not wrong as computations. The wide window shows
that the R2 bent dip exists at 2.087 GHz, just below the 2.2–2.7 GHz window. `AWS_R4.md`
read the edge minima as Meep's feature "moving down with refinement". The R4 dip is
predicted at about 2.15 GHz (section 3), above the R2 dip, so on this evidence it moved
*up*. That prediction is not measured.

## 3. Mechanism: per-component voxelization of a one-cell PEC shell

Section 2's dumps (`diag/grid_*.json`, `--dump-grid`) read `fields.get_chi1inv` at each
component's Yee samples. The sample offsets are found empirically: along a line crossing
a material boundary, only the true offset returns unmixed values. The gap model reads
the y = 0 slice over the half patch, x in [0, 19] mm, by symmetry. Using the full width
changes the factors by at most 0.001. In each column, h_E is the height of the free Ez
samples, n·dx, and h_H is the separation of the Ex metal planes, (n + 1)·dx.

| Grid | h_H (mm: columns) | h_E (mm: columns) | Factor √(⟨1/h_H⟩/⟨1/h_E⟩) |
|---|---|---|---|
| flat R2, harness | 2.0: 38 | 2.0: 39 | 1.000 |
| bent R2, harness | 2.0: 32, 2.5: 6 | 1.5: 25, 2.0: 14 | **0.893** |
| R = 10⁴ control R2 | 2.0: 38 | 2.0: 39 | 1.000 |
| bent R2, cell staircase | 1.5: 27, 2.0: 11 | 1.5: 31, 2.0: 8 | 0.989 |
| flat R2, inset | 2.0: 38 | 1.0: 39 | 0.707 |
| flat R4, harness | 2.0: 76 | 1.5: 77 | 0.866 |
| bent R4, harness | 1.75: 38, 2.0: 38 | 1.5: 48, 1.75: 29 | 0.922 |
| flat R2.5, face shells | 1.6: 48 | 1.6: 48 | 1.000 |
| bent R2.5, face shells | 1.6: 42, 2.0: 6 | 1.2: 40, 1.6: 8 | 0.874 |

Why the factor follows. A layer whose Ez is held at zero but whose tangential E is
free is, in the continuum, a layer with ε_z = ∞. It carries the TEM mode's tangential H,
so it stores magnetic energy but no electric energy. The parallel-plate line under the
patch then has an inductance per length set by h_H and a capacitance set by h_E. Its
phase velocity is c/√ε · √(h_E/h_H), and averaging over the x columns gives the factor
above. Nothing is fitted. The model is anchored on one measurement: every predicted
dip below is the measured `flat_R2` dip (2.3351 GHz) times the factor of that grid
relative to the flat R2 grid.

| Test | Predicted (GHz) | Measured (GHz) | Error |
|---|---|---|---|
| bent R2, harness | 2.0859 | 2.0869 | +0.05% |
| R = 10⁴ control | 2.3351 | 2.3354 | +0.01% |
| bent R2, cell staircase | 2.3090 | 2.3325 | +1.0% |
| flat R2, inset (TM010 identified by field map) | 1.6512 | 1.6807 | +1.8% |

**Precision of the model.** The model explains the 250 MHz harness artifact to 1 MHz
and the inset shift to 1.8%. Elsewhere it misses by more:

- **Cell staircase.** On the grid-aligned bent board it predicts 2.3090 GHz and the run
  measures 2.3325 GHz, a **23.5 MHz** miss. Equivalently, it predicts a bend shift of
  about −26 MHz where −2.6 MHz is measured. That miss is about ten times GEODE's bend
  shift.
- **Inset.** The miss is **29.5 MHz**.

The model's precision is therefore about 1–2% (25–45 MHz). It identifies the cause and
the size of the 250 MHz artifact, but it cannot resolve GEODE-sized bend shifts. That
the corrected bent board agrees with GEODE comes from the measurement in section 2, not
from the model. `meep_diag_verify.py` holds each test to its own bound, bracketing
these errors: 0.2% for the bent and control runs, 1.5% for the cell staircase, 2% for
the inset. `meep_diag_mutation_test.py` checks that a forged 30 MHz shift of any of the
flat, bent or cells spectra fails, as do eight other corruptions of the records.

The inset board was built as a deliberate test of the model on a *flat* board. It has
two dips, and Ez maps on the gap mid-plane identify them (`map_inset_flat_R2`, a repeat
of the inset run with a DFT plane added).

- **1.69 GHz is TM010.** Here |Ez| is odd in y, with a node at the patch centre and
  maxima at the y edges, and nearly uniform in x. Its correlation with the flat board's
  2.34 GHz map (`map_flat_R2`) is 0.9997. The model predicts this dip.
- **2.61 GHz is an x-directed TM020-like mode.** It is even in y, with nodes near
  x = ±10 mm (W/4 = 9.5 mm), and its correlation with the flat TM010 map is 0.076.
  The centred feed sits at its maximum.

A ~28% shift of TM010 on a flat board, with nothing but a different metal placement,
confirms that per-component voxelization alone moves the patch resonance by amounts
of this size.

The conductors are closed in every dump: in the y = 0 slice, the metal Yee edges form
exactly two connected conductors (patch and ground). A slotted or leaky staircase is
ruled out.

## 4. What is ruled out, and what remains open

Ruled out as causes of the 250 MHz bent shift:

- **Geometry and set-up values.** Dimensions, units, bend direction and side, probe
  position, reference impedance, time convention and FR-4 ε (section 1).
- **The port and extraction model.** The flat board, with the same port, agrees with
  GEODE. The Norton total impedance η0·V/I_inj does not depend on the 50 Ω bookkeeping,
  and it moves with the dip: it peaks at 2.30 GHz flat and 2.05 GHz bent. The bent shift
  is therefore physical in the discrete model, not an extraction artifact.
- **FR-4 loss as 1/f.** It shifts loss, not the resonance, and it is the same for flat.
- **The absorber and box.** They are identical for flat and bent.
- **Meep's Prism construction.** See the R = 10⁴ control.
- **A leaky staircase.** The dumps show closed conductors.
- **GEODE or Palace.** Both agree with each other and with Meep's flat board, and with
  Meep's corrected bent board.

Not resolved here:

- **Meep's own convergence.** It is not established. The port (66.7/42.9 Ω at R2/R4) and
  the gap (2.0 mm) are discretized differently at each resolution, so successive grids
  need not converge monotonically. That is consistent with the R1→R2→R4 differences not
  shrinking (`AWS_R4.md`).
- **The inset board's TM020-like resonance.** It is identified by its map. On a
  consistent grid it lies above the sampled 3.0 GHz, so the model's pull-down of it is
  not tested quantitatively. The bent harness run's Re Z rising toward 3.0 GHz is
  consistent with the same mode, and also not resolved.
- **The R2.5 face-shell runs.** They were dumped, not run. Their factors in section 3 are
  predictions.

## 5. Consequences for R8

The error does not decrease with resolution. It depends on where the shell faces fall
between samples. The dumps give factors of 0.866 for the flat board and 0.922 for the
bent board at R4. At R8 the tangential samples sit on multiples of 0.125 mm, if the
pattern seen at R2 and R4 holds. The outward shell [0.8, 0.925] mm would then contain
the Ez sample at 0.8125 mm below the Ex plane at 0.875 mm, so even the flat board would
carry a factor of about 0.93 (h_E = 1.5, h_H = 1.75 mm). That is an inference from the
pattern; no R8 grid was dumped.

**Running R8 with the current harness would spend $175–250 on a known
model error.** Before any converged Meep run, the harness needs metal that is
consistent per Yee cell, as in `--shell-mode cells`. It also needs a gap and port that
are whole numbers of cells: R2.5 or R5 make the 1.6 mm gap exactly 4 or 8 cells, and the
R2.5 face-shell dump gives a 1.6 × 1.6 mm, 50.0 Ω discrete port. With those changes it
needs its own convergence ladder. Those are operator decisions; nothing here launches R8.

## Spend and teardown

One r8g.8xlarge in us-east-2c (`geode-worker-meep-1088-diag`). It launched at 15:08:17Z
and was **terminated** at 19:35:18Z (16,021 s). A scheduled OS shutdown at 23:05Z bounded
it independently. Its root volume was deleted with it, and no tagged volume remains in
us-east-2. The conservative elapsed-time estimate is **$8.39**: 4.45 h × $1.88512/h, the
us-east-1 table rate in `scripts/aws/geode-worker.sh`, assumed equal in us-east-2. That
is not an AWS bill. The #1088 total so far is about $21.4 of the $250 cap (R2 $1.45, R4
$11.54, this diagnosis $8.39). All results are also kept outside the repository. No Rust
was changed, and nothing was run on the Mac.
