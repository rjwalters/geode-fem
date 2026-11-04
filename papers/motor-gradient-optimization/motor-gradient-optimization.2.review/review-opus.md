# paper-review: motor-gradient-optimization.2 (PR #1171, head 30664b3a)

Reviewer: Opus (anvil:paper-review, read-only). Nothing in `/Volumes/Stripe/geode-fem/review-1171v2`
or `/Volumes/Stripe/geode-fem/review-main` was modified (worktree `git status` clean; review-main was
only fetched and read through `git show origin/main:<path>`).
Rubric: installed `/Users/rwalters/GitHub/geode-fem/.anvil/skills/paper/rubric.md`, 9 dimensions,
**/44, advance threshold ≥35**. House rules: `.anvil/skills/paper/voice.md`.

## Verdict

- **Total: 36 / 44**
- **advance: true** (meets the threshold, no critical flags). Two items below are labelled
  "fix before audit". They are precision defects, not critical flags.
- Critical flags: **none**. Every number I checked in the text, tables and figures matches the
  merged evidence (see "Numeric verification"). `python3 verify.py` passes with exit 0.
  All 78 manifest snapshots are byte-identical (SHA-256) both at their pinned commits
  (581ee7ad x17, 910593ba x61) and at current `origin/main` (b487ad45). 910593ba is an ancestor
  of origin/main.
- Outstanding dependencies: none (no `[PENDING]` markers).

| # | Dimension | v1 | v2 |
|---|---|---|---|
| 1 | Rigor of method / argument | 4 | 5 / 6 |
| 2 | Evidence sufficiency | 4 | 5 / 6 |
| 3 | Clarity of contribution | 3 | 4 / 5 |
| 4 | Related-work positioning | 3 | 4 / 5 |
| 5 | Reproducibility | 4 | 5 / 5 |
| 6 | Figure & table quality | 3 | 3 / 4 |
| 7 | Prose & structural quality | 3 | 3 / 4 |
| 8 | Citation hygiene | 3 | 4 / 5 |
| 9 | Rhetorical economy | 3 | 3 / 4 |
| | **Total** | 30 | **36 / 44** |

Top priorities for the next pass:
1. Put "best-step" into the abstract's FD sentence, and disclose that 96 of the 800 cases are
   bit-identical duplicates (704 distinct, 688 of them ≤ 2.8e-7) (F1, F2).
2. State the Jinit effect fully: it changes 7 of the 10 geometric/scale gradients, flips three signs
   and rescales others by 10 to 1000 times. Fix Fig. 4(c) so the reversal is visible (S1, S2).
3. Use the gradients already recorded for both couplings to test the Discussion's conjecture.
   Ripple sensitivities differ by up to 2.5 times between the models (S3).

## v1 blocker resolution

| v1 item | Status in v2 | Evidence |
|---|---|---|
| Opus B1: filter diagnostic reported for cogging only | **Resolved.** All three currents and all five cuts reported, with non-monotonicity and a non-evidentiary reading (L397-405). | Values match v1 `forward/tables.md` (0.0075→0.0184, 0.04280→0.04460, 0.08832→0.08775; 0.0077/0.0300/0.0184/0.0077/0.0075). |
| Opus B2: "two explanations" not exhaustive | **Resolved, now by experiment.** Coupling torque, half-band Arkkio and free ends measured (L365-370). Reference IGA and straight-sided geometry are kept as unbounded (L407-417). High-field iron segment explicitly untested (L413-415). | `forward_harmonic` estimators < 9e-5 N m, free ends < 5.4e-5 (verify.py asserts). |
| Opus B3 / Fable B3: missing harmonic-coupling and torque literature | **Resolved.** Bontinck 2018, Egger 2022, Arkkio 1987, Arkkio 2012 added and engaged (L80-90). The cogging/FE-discretization lead (Howe & Zhu 1992) is still absent (see D4). | refs.bib, citation-notes.md |
| Opus B4: Ion 2018 dropped | **Resolved** (L92-98), with what the eventual comparison must add. | |
| Opus S1 / Fable B1: title/content mismatch | **Resolved.** Retitled around results, prospective section cut to two Discussion paragraphs. | L6-7, L557-570 |
| Fable B2: unresolved cause, no arbitration | **Largely resolved for load.** The paired h45 solve is the decisive experiment Fable asked for. Loaded ripple gap falls 9.9-11.3% → 1.3-1.7%. Cogging (+12.9%) stays unresolved and is described that way. | `forward_harmonic/README.md` tables |
| Opus S2-S12, Fable S1-S14 | Addressed per changelog. Spot-checked: arc wording (L279), "low-field" scoping (L413), n/a cells, θ_r and p defined (L603), GPL-3.0-or-later (L594), CPU and threading named (L516-518), 500 = baseline + sensitivity (L186), Scotzniovsky year 2025. | |

The changelog states the v1 reviewer sidecars were not on disk in the worktree. The two v1 reviews
I was given (`review-{opus,fable}.md`) match the changelog's itemization.

## Codex-requested checks

| Check | Finding |
|---|---|
| **Model/estimator labelling** | Mostly correct. L245-249 states forward comparisons use Arkkio for both couplings, and the derivative study uses coupling torque (h45) vs Arkkio (conforming). This matches `design/README.md` "Couplings". Gaps: the Fig. 4 caption (L483-491) says "harmonic coupling" but not "coupling torque" for panels (b)/(c). Table 2 (L379-390) does not say its rows are conforming-model runs. The 800-case statistic mixes two model-estimator pairs, and the text says so only implicitly ("both couplings", L470). The conforming/Arkkio SW2 mean gradient is non-monotone in level (−46.5, −41.3, −44.4, −47.2 N m/m), while h45/coupling converges smoothly (−49.8 … −47.1). This is an estimator-dependent gradient effect squarely in the paper's theme and is not mentioned (S4). |
| **Best-step FD interpretation** | Body is correct: "This is a best-step verification statistic, not the accuracy of a single finite-difference step chosen in advance" (L474-475). **The abstract drops the qualifier**: "782 agree within $2.8\times10^{-7}$ relatively" (L26-27) reads as fixed-step accuracy (F1). For reference, at the fixed step h = 1e-6 m that the cost table uses, the 688 recorded loaded geometric cases have median error 2.8e-8, 106 exceed 1e-6, and the worst is 1.9e-4 (conforming SW3 mean, L1). The paper should give that number, or at least the step, next to the cost claim (S6). |
| **Fixed-I vs Jinit sign reversal** | Numbers correct: L1, h45/coupling torque, θ0, full current, `d_mean[SW1]` = +1.68373 vs `d_mean_jinit[SW1]` = −220.852 N m/m (verified-numbers.json and `design/README.md` "Current rules" table). Note that `design/README.md` L116-117 prose quotes the L0 values (+1.73 / −221.7) without saying so. The paper correctly uses and labels L1. Attribution to the area/current term rather than to an adjoint discrepancy is supported: the Jinit FD agrees at 6.1e-11, and the mutation check fails when the area term is dropped. **Framing undersells it (S1).** Jinit leaves MAG/MH/MW unchanged (rotor parameters do not move slot area) and changes **7 of the 10** geometric/scale gradients. For the mean it flips SW1, SW4 and SD1. It rescales SW3 by about 1,400×, Sr1 by about 280× and SW2 by about 0.09×. The paper never states that MAG/MH/MW are unaffected. It gives no one-clause physical reason (under fixed group-1 density, a wider tooth means a smaller slot, less current and less torque). "can reverse the sign of a design sensitivity" (L28-29) is true, but the stronger supported statement is that the excitation policy dominates every stator-side sensitivity. |
| **Timing caveats** | Adequate. CPU (M3 Ultra, 28 cores), default threading, unpinned threads and load average 14-24 (L516-518). L0/L1 medians of 3 and L2 single run (L490). "uncontrolled host load" appears in the abstract (L31). No speedup claim (L529-531, L579-581). Repeat spread is small: L1 h45 1e-14 forward 3.15-3.20 s, FD24 29.6-29.8 s. Two precision points (S6): (a) the FD figure of 7.2-9.5 counts only the 24 perturbed forwards, which warm-start from base states, while the gradient figure includes its forward, so the like-for-like FD cost is 8.2-10.5 forwards; (b) the FD step is not given. The phrase "The timing advantage" (L529) is acceptable but "The cost ratio" is more neutral. |
| **Jinit changes 7 of 10; MAG/MH/MW unaffected** | Confirmed from `study_L1_h45_initial.json` and the README table. **Consequence for the 800 count (F2):** the Jinit sweeps include MAG/MH/MW, which reproduce the prescribed-rule sweeps exactly. I found **96 of the 800 cases bit-identical** (same adjoint, same best rel_err) to prescribed cases. That leaves **704 distinct cases, 688 of them ≤ 2.8e-7 and 16 above**. The headline count is inflated by 14% with no disclosure. |
| **MW remeshed gap 2.6e-6** | The paper already avoids calling it a bound: "the fitted $2.6\times10^{-6}$ value is not a resolved accuracy bound" (L511). But it gives no usable bound either, and the number is a least-squares cancellation. At L2 the per-step remeshed-minus-moved values are −3.6e-5, +3.6e-4 and +6.3e-4 at h = 1e-3, 3e-4 and 1e-4 m (`design/README.md` table). Recommended wording (S5): the L2 gap "is below about 6e-4, the scatter of individual remeshed steps at h ≥ 1e-4 m". Drop 2.6e-6, or keep it parenthetically as the fit value. Per voice.md, this is an estimated upper level, not a proven "bounded by". Also report the worst remaining gap, which concerns the ripple objective: SW1 std 0.038 at L2. "show decreasing discrepancies" (L507) hides a 4% remeshed-vs-moved difference in the derivative of the quantity being optimized. |

## Numeric verification (against merged evidence)

All of these were checked against `origin/main` evidence (byte-identical snapshots) or recomputed
from raw runs:

- Table 1 / L19-23, L308-312: conforming −0.153/−0.156% mean and +9.86/+11.27/−29.09% std.
  Harmonic −0.0024/+0.051% mean and +1.70/+1.27/+12.89% std. Pointwise gap 0.0272 → 0.00339 N m.
  `forward_harmonic/README.md` tables are consistent.
- L324-325: half-current reference lies 4.6e-5 N m below the h45 L3 band [0.038995, 0.040229]
  ("approximately 5e-5", ok). Full-current reference is inside.
- L339-341: cogging −29.1% → +12.9%, about 1.13× scaling, reference outside the band by 1.19e-3. Ok.
- L348-350: 59% zero-current and 7.4/9.0% loaded (h45 vs conforming at L3). Order 189 is within
  1.02e-4 of conforming at L1/L2. Ok.
- L254-255: hall vs conforming 4.1e-9 (L0), 1.24e-9 (L1). "1.3e-9" is a valid upper value. Ok.
- L367-369: estimators < 9e-5, free ends 5.33e-5 < 5.4e-5. Ok.
- Table 2: 2.06e-6, 7.27e-8, 3.86e-4, 7.30e-5. Ok.
- L186-187: 500 + 1,140 solves, all `tolerance`. Ok (verify.py).
- L472-474, abstract: 800 / 782 / 1.0668e-5. Ok as recorded, but see F2.
- L499-500: zero-current mean best error up to 7.0e-4, std up to 8.9e-7. Ok.
- L525-527, Table 3: gradient/forward 1.13-1.20, FD/forward 7.24-9.53. Table 3 cells match
  `cost_L*_h45_tol1e-14.json` medians. Ok.
- L455-456: +1.68 / −220.85 N m/m. Ok. Fig. 4(c) plots the same values in N m/mm (−0.22).
- Table 4: design points match `study_L1_h45_{initial,perturbed}.json` (verify.py).

**numeric-consistency:** the automated detector was not run (the `.anvil` venv was not used against
the worktree). A manual claim-vs-claim cross-check was done this pass, and no inconsistency was
found.

## Scoring with quoted evidence

**D1 Rigor 5/6.** The coupling question is now settled by a controlled experiment, not by post hoc
filtering: "Forward comparisons use Arkkio for both GEODE couplings to isolate the interface change;
coupling torque is a separate check" (§3.3). The interval is correctly tiered: "This interval is a
refinement diagnostic, not a proven error bound" (§4). Deductions: the 800-case statistic counts 96
duplicated cases as independent (F2), and the MW remesh number is a fit cancellation with no stated
resolution (S5).

**D2 Evidence 5/6.** The paired h45 solves on four meshes, the order ladder at L1/L2/L3, three
estimator controls and the free-end control support "Transmitting the reference's eight harmonic
orders reduces that ripple discrepancy to 1.3--1.7\%" (abstract). The gradient verification covers
two designs, two couplings, three levels, two rules, Taylor sweeps, a tolerance study and a remesh
cross-check. Deduction: the Discussion asserts a consequence ("A gradient of that objective can be
numerically correct and inexpensive while pointing toward a design whose improvement does not
survive a change in the model", §7) that the recorded h45-vs-conforming gradients could test but
the paper does not test (S3). Cogging stays unresolved, and the paper says so.

**D3 Contribution 4/5.** The lede is first and result-shaped: "A small error in mean torque can
conceal a large change in the quantity being optimized" (§1). Mild underclaiming: "The contribution
is a reproducible account of model and derivative errors in this published benchmark" (§1) frames
the work as bookkeeping. The memorable result is that the reference's order-45 cut is a model-level
choice that changes the ripple objective by 7-9% (59% for cogging), more than the spatial error at
L3. The paper also shows that the excitation policy, not the field model, dominates the stator
sensitivities. The second point is reduced to "can reverse the sign of a design sensitivity".
Cold-reader check: the contribution is extractable in two sentences. The two halves (interface,
gradients) connect only through the untested Discussion conjecture.

**D4 Related work 4/5.** Now engages the closest method literature: "\citet{egger2022torque} derive
torque from the energy of a harmonic mortar formulation" (§2) and "\citet{ion2018robust} compare
deterministic gradient-based optimization with stochastic search" (§2), each with an explicit
"what this does not establish". Remaining gap: classical FE cogging-torque prediction sensitivity
(lead: Howe & Zhu, IEEE Trans. Magn. 28(2), 1992), and the IGA shape-optimization line of the
reference group (lead: Merkel, Gangl, Schöps, IEEE TEC 2021). These are unverified leads for a
`paper-litsearch` re-run. `web_search: true` is set, but no web tool was used in this pass.

**D5 Reproducibility 5/5.** "The evidence used here is fixed at GEODE commit" 910593ba (Code and
data availability), with commit-pinned links. A 78-file SHA-256 manifest is byte-identical to main.
verify.py recomputes every FD from raw plus/minus values, all solve statuses, sensitivity and cost
tables. The CPU and threading are stated. Small gaps (FD step h, gmsh version for the regenerated
.msh files) are covered by the linked READMEs.

**D6 Figures/tables 3/4.** Figs. 1-3 are clean, v1 Fig. 1 defects are fixed, and captions are
self-contained ("All GEODE curves use Arkkio torque, so the paired difference changes the coupling
while holding the estimator fixed", Fig. 2). Deductions: in Fig. 4(c) the fixed-current SW1 bar
(+0.0017 N m/mm) is invisible, so the reversal the text cites cannot be seen, and its N m/mm unit
differs from the text's N m/m. The Fig. 4 caption omits the coupling-torque estimator. Fig. 3(a)
encodes coupling by line style with no legend entry. In Fig. 2, conforming (dark blue) and harmonic
(teal) are close in hue and close in grayscale. Table 2 does not name its coupling model.

**D7 Prose 3/4.** Voice rules hold. Grep found no virtue words, no "certified", no novelty claims,
no issue/PR numbers, and no prose em-dashes ("---" absent; "--" only in ranges and compounds).
Deductions: one version-history word, "For completeness, the earlier order-45 post-processing
diagnostic" (§5.3). The abstract's FD sentence is less precise than the body (F1). The empty
`\author{}` must be resolved before deposit (process, not scored heavily).

**D8 Citation hygiene 4/5.** Methods are now cited where used: "We average Maxwell stress over the
air gap using the Arkkio expression \citep{arkkio1987}" (§3.3). Deductions: `arkkio2012proper` is an
`@article` whose `journal` is the SPEEDAM proceedings with duplicated words ("Power Electronics Power
Electronics"). It should be `@inproceedings` with a correct booktitle. `wiesheu2025combined` is an
arXiv 2311 preprint dated 2025; check for a journal version (carried over from v1 S11). The
manufactured-solution and consistent-tangent statements remain uncited (minor).

**D9 Economy 3/4.** Much tighter than v1, and the prospective section is gone. Remaining padding:
the post-filter paragraph keeps eight numbers of a diagnostic the paper itself calls non-evidentiary
("That non-monotone diagnostic neither supports nor excludes the coupling explanation", §5.3). With
paired solves in hand, it belongs in one sentence or an appendix. §6.2's last two paragraphs
(L494-513) pack six distinct caveats into 20 lines. A short table of zero-current, remesh and
mesh-convergence caveats would read faster.

## Fix before audit (not critical flags)

- **F1 (L26-28, abstract).** "782 agree within $2.8\times10^{-7}$ relatively" → "782 agree with the
  best-step central difference to within 2.8e-7 relatively". Optionally add the fixed-step figure
  (median 2.8e-8 at h = 1e-6 m).
- **F2 (L26, L470-479, Fig. 4a caption L483-484).** Disclose that the fixed-density rule leaves the
  rotor parameters MAG/MH/MW unchanged. Their 96 Jinit cases duplicate prescribed cases exactly.
  Report 704 distinct comparisons (688 ≤ 2.8e-7, worst 1.07e-5), or keep 800 and say "including 96
  rotor-parameter cases repeated under both rules". Make the abstract and Fig. 4(a) match.

## Should-fix

- **S1 (L447-460, L28-29).** State the full Jinit effect: rotor parameters unchanged; 7 of 10
  geometric/scale gradients change; mean-torque signs flip for SW1, SW4 and SD1; SW3/SW4/SD1/Sr1
  grow by 2-3 orders of magnitude, SW2 shrinks by ~10×. Add one physical clause: "under fixed
  group-1 density a wider tooth narrows the slot and lowers the current". Consider the stronger
  abstract sentence: "The excitation policy changes most stator-side sensitivities by orders of
  magnitude and reverses three of them."
- **S2 (Fig. 4c).** Use a symlog axis or annotate values so the fixed-current SW1 bar (+1.68 N m/m)
  is visible. Use N m/m to match the text. Add "coupling torque" to the caption.
- **S3 (L541-546; new short paragraph in §6).** The h45/coupling and conforming/Arkkio gradients are
  already recorded at the same designs. From `study_L{1,2}_*` (full current): the mean-gradient
  vectors agree closely (unscaled cosine ≥ 0.999, dominated by SW2/MW/MAG). The ripple sensitivities
  differ materially: MAG std 1.28 vs 1.94 N m/m (L2, θ0), 0.75 vs 1.87 (L2, θ1); SD1/SW1/SW4 std
  differ by 25-50%. Report this, ideally box-width-scaled. It turns the Discussion's conjecture into
  a measured statement and links the paper's two halves.
- **S4 (L503-506).** Add the conforming/Arkkio SW2 mean-gradient non-monotonicity (−46.5, −41.3,
  −44.4, −47.2 N m/m, L0-L3) against smooth h45/coupling convergence. It is the clearest
  estimator-dependent derivative effect in the data.
- **S5 (L506-511).** Replace the 2.6e-6 sentence with an estimated level: "at L2 the MW mean gap is
  below about 6e-4, the scatter of single remeshed steps at h ≥ 1e-4 m". Name the largest remaining
  gap, SW1 ripple 3.8e-2 at L2, instead of "decreasing discrepancies".
- **S6 (L522-527, Table 3, Fig. 4d).** State the FD step (h = 1e-6 m) and that the 24 FD forwards
  exclude the base forward they warm-start from (like-for-like 8.2-10.5). "The timing advantage"
  (L529) → "The cost ratio".
- **S7 (Table 2 caption, L380-381).** Say "conforming coupling" (the half-band row is the conforming
  L3 field; the h45 estimator controls are in §5.2).
- **S8 (Fig. 4 caption L485-488).** Panel (b)/(c): "harmonic coupling, coupling torque, fixed current".
- **S9 (L397-405).** Cut the post-filter paragraph to one sentence plus a pointer (or move it to an
  appendix). Delete "the earlier" (voice.md: no version history).
- **S10 (Fig. 3a).** Add legend entries for solid = harmonic order 45, dashed = conforming.
  Fig. 2: pick a conforming colour more distinct from teal, or add a dash style.
- **S11 (refs.bib).** `arkkio2012proper` → `@inproceedings`, booktitle "2012 International Symposium on
  Power Electronics, Electrical Drives, Automation and Motion (SPEEDAM)". Check `wiesheu2025combined`
  for a journal version.
- **S12 (L8).** Resolve `\author{}` before any deposit.
- **Evidence-side (not the paper).** `benchmarks/motor_shape_gradient/design/README.md` L116-117
  quotes L0 values (+1.73 / −221.7) next to a table labelled L1. Label the level.

## Procedural notes

- Rendered all 14 pages. Figures 1-4 were inspected at 130 dpi. No `??` references in the PDF text.
  preflight.json reports zero overfull boxes.
- verify.py run with `python3 -I -B` from the version dir: PASS, exit 0. No files were written.
- Review written to scratchpad per dispatch. No staged `motor-gradient-optimization.2.review/`
  sibling was created, so the worktree is untouched.
