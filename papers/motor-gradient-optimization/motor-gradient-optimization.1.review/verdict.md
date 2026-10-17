# Delivered independent reviews of v1

Source: https://github.com/rjwalters/geode-fem/pull/1171#issuecomment-6230341480

The following consolidated Opus/Fable review was delivered by Claude; archived verbatim. This is not a new review by the revising author. Full individual reviewer sidecars have been requested.

## Paper review of v1 (commit 4e471f9b): two independent Anvil reviews

Both reviews score **30/44** (anvil-pub-v2 rubric, threshold 35), so the draft does not advance yet. Neither found a critical flag. `verify.py` passes.

| Dimension | Opus | Fable |
|---|---|---|
| Rigor | 4/6 | 5/6 |
| Evidence | 4/6 | 4/6 |
| Contribution | 3/5 | 3/5 |
| Related work | 3/5 | 2/5 |
| Reproducibility | 4/5 | 4/5 |
| Figures | 3/4 | 3/4 |
| Prose | 3/4 | 3/4 |
| Citations | 3/5 | 3/5 |
| Economy | 3/4 | 3/4 |

**Verified by both reviews.**
- Every number matches the merged forward artifacts (`summary.json`, `tables.md`, `runs/*.json`, `m1_tables.log`), including 500 solves / 3491 s.
- Data copies are byte-identical to main.
- Conventions match `conventions.json`: Hc = Br/(mu0 mur), magnet at pi/2, I_q phases, per-sector torque with L = 0.1 m.
- The ripple mismatch is reported as a mismatch. Harmonic coupling is stated as a hypothesis. No speedup is claimed, and M3/M5 are prospective.

### Blockers
1. **The title and contribution do not match the evidence** (L6-7, L40-48; §6, L382-434). The title asks when gradients pay off, which nothing in the paper answers. Either retitle as a forward-validation result and cut §6 to a short outlook, or hold the paper for M3-M5.
2. **The harmonic-filter check is reported selectively** (L348-355). It is shown only at zero current. At 0.5 I, filtering moves ripple *away* from the reference (0.04280 to 0.04460, reference 0.03895). At 1.0 I it barely moves it (0.08832 to 0.08775). Report all three currents, and the non-monotone cut-order data (0.0077/0.0300/0.0184/0.0077/0.0075 at cuts 9/21/45/93/189). Say that the check neither supports nor excludes the hypothesis.
3. **The list of open explanations is incomplete** (L348, L363-364). Add two:
   - the torque evaluation itself: Arkkio vs the derivative of the harmonic coupling (see L211-215);
   - geometry error from straight-sided triangles on curved boundaries.
   The cause should be settled, or at least narrowed, before release. The harmonic-truncated solve is in progress in a separate issue. Other possible steps are P2 at L2/L3, a third code on the committed geometry, or refining the public reference.
4. **Related work is missing** (L68-95, L199).
   - No harmonic stator-rotor coupling method papers (Bontinck et al. 2018; Egger et al. 2022).
   - No Arkkio citation.
   - No work on how sensitive cogging and ripple are to the mesh and torque method.
   - **Ion et al. 2018** (doi 10.1007/s00202-018-0716-6), a gradient vs stochastic comparison on a PM motor, is in the thread's own literature screen but not cited.
   - Verify each with a literature search before adding.

### Should-fixes
- **Front matter and abstract:**
  - Remove the author placeholder and `pdfauthor={}` (L8, L10). Authorship and venue are the operator's call.
  - The abstract has three disclaimers in a row (L27-29); drop "No optimization speedup is claimed".
  - "Material-endpoint checks" (L23): only the low-field end was tested. The 2.25-2.30 T segment was not, and the bridges reach 3.14 T.
- **Statements in the text:**
  - L176-177: H/B is non-monotone up to about 0.75 T, not only between 0.05 and 0.10 T.
  - L238: "coupling circle has 120·2^l segments" should be "60-degree coupling arc".
  - L341-346: the Maxwell-stress range "0.0023 to 0.025" is the per-radius maximum. The L3 minimum is 0.00165.
  - L371: "full sensitivity study". The 500 solves include the 240 baseline runs.
- **Terminology:** rename "uncertainty interval" to "refinement interval (estimated)".
- **Hedged support for the hypothesis:** Fig. 2 shows a 10-degree residual period at all currents and 5-degree content in the GEODE cogging. These are slot-passing features.
- **Figures and tables:**
  - Fig. 1: the panel (b) title collides with its annotation, and the rotor pockets need an "Air" legend entry.
  - Table 1: write "n/a" instead of "---".
- **Appendix (L491):** define theta_r and p, and drop the word "fixture".
- **Citations and licence:**
  - The licence is GPL-3.0-or-later (L481-483).
  - The Scotzniovsky entry has year 2024 but vol. 26(1) is 2025.
  - Cite the adjoint-cost property.
- **Reproducibility:**
  - Give the CPU model and thread count (L368).
  - `verify.py` does not cover the cost table, Table 2 or the §5 numbers.

**Assessment (Fable reviewer).** As a pinned technical report with a result-based title, the paper is useful now, because it resolves the conventions and documents a 10-30% ripple discrepancy in a published optimization objective. As a venue paper it needs the cause settled. If the truncated-coupling solve reproduces the reference, the finding becomes "harmonic truncation at order 45 makes the ripple objective model-dependent at the level the optimizer resolves." That is a crisp, citable result.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
