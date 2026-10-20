# Review: motor-gradient-optimization.4 (PR #1171, head fea6ba5b)

Reviewer: Fable (Anvil `paper-review` role; independent, skeptical referee from computational electromagnetics / electric-machine design). Fresh score of v4 at head `fea6ba5b5243f5513b8804bfcdc2117d6cf2ced9`; v3 scores (38/44, 37/44) do not carry over.
Reviewed: `/Volumes/Stripe/geode-fem/review-1171v4/papers/motor-gradient-optimization/motor-gradient-optimization.4/main.tex` (791 lines; 17 pp PDF; 6 figures, 4 tables, 13 references) with `tables/*.tex`, `refs.bib`, `changelog.md`, `README.md`, `preflight.json`, `evidence-manifest.json`, `verified-numbers.json`, `verify.py`, `citation-notes.md`, `../BRIEF.md`, `../EXPERIMENTS.md`, `../.anvil.json`, issue #1185 (pre-registration and amendments 1–5), and the v3 reviews/audit at `/Volumes/Stripe/scratch-codex-review/motor-paper-v3-reviews/` (used only to check that `changelog.md` answers each v3 comment).
Evidence: all 112 manifest sources were pulled from `origin/main` (`a35591b9`) with `git show` and SHA-256 compared against the manifest: 111 match; the one mismatch (`forward_harmonic/README.md`) is a post-pin edit on main (the #1189 note) and the snapshot matches the pinned commit `910593ba` byte-for-byte, so it is not a defect. All four evidence commits are ancestors of `origin/main`. Every number below was recomputed from those raw blobs with my own script (`scratchpad/motor-v4/scripts/recheck_v4.py`), not read back from `verify.py`, `verified-numbers.json` or the benchmark summaries.
Rendered PDF: all 17 pages read; pages 3, 7, 8, 10, 12, 13 re-rasterized at 170 dpi.
Deterministic checks (run on a scratch copy, `.anvil` venv via `uv`): `numeric_consistency` 366 numbers, 1 finding (a confirmed false positive, see below); `audience_check` 0 hits; `pending_marker` 0 markers; `preflight.json` compile ok, 0 overfull boxes, 0 placeholders, bib lint clean.
Literature: four anonymous OpenAlex queries (no identity, no `mailto`, generic User-Agent); no `.bib` written.
Worktree: nothing under `/Volumes/Stripe/geode-fem/review-1171v4` created or modified (`git status` clean after the review); no `.review` sidecar staged. `verify.py` was run only on a scratch copy.

## Verdict

**Total: 36 / 44**
**Decision: advance: false** (total clears the 35 threshold; blocked by one critical flag).
**Critical flags: 1** (`artifact_verify_1`, below).
**Outstanding dependencies: none** (0 pending markers).

The science of v4 is in good shape. The two new experiments do what the v3 reviews asked for: the independent reference rerun first reproduces all 60 saved torques to 9.7e-13 N m and then, at 24×24 subdivisions, closes the cogging gap from +12.9% to +0.10%, with every refined statistic inside GEODE's L3 refinement interval; the saved optimized design is evaluated on fresh meshes under three interface models and three current rules, and the 3.6-fold objective change and the current-rule dependence are reported as results with their caveats. The pinned L3 gradients now anchor §6.3 the way the v3 Opus review asked. Every one of the 30-odd numbers I recomputed from `origin/main` matches. The voice rules hold (0 em-dashes, 0 virtue words, 0 version-history phrases, no "certified", no novelty claims), the author line is set, and the figure defects of v3 (geometry label, legend overlap) are gone.

What blocks the paper is mechanical, and what keeps the score at 36 is presentational:

- **Critical (S1).** The declared artifact-verification command (`python3 verify.py`, `.anvil.json`) **fails on the committed tree**: the manifest snapshot `figures/src/data/m1_tables.log` is excluded by `papers/.gitignore` (`*.log`) and was not force-added, unlike the same file in `.3/`. On a clean checkout `verify.py` raises `FileNotFoundError` at the manifest hash loop and exits 1. `preflight.json`'s `artifact_verify: PASS` was recorded in a working tree where the untracked file existed. With the blob restored from `origin/main` (`benchmarks/motor_nonlinear_m1/m1_tables.log` at `581ee7ad`, SHA `ccc86e29…` as in the manifest) `verify.py` passes completely, so the flag is isolated to this one file. Fix: `git add -f` the snapshot and re-run `verify.py` from a fresh checkout before the next review.
- **Major (M1).** §5.5 reports Opt120's mean-torque deficits (−8.9% fixed unscaled, −13.0% fixed physical) without telling the reader that the saved design also has **axial length 0.080 m** (initial 0.100 m), SD1 0.130 m and operating angle −11.6°. The validation records apply `ScaleR²·LENGTH/0.1 = 0.878` to every Opt120 torque. Removing that sizing factor, Opt120's unscaled cross-section produces 1.3147/0.878 = 1.497 N m per 0.1 m at fixed unscaled current, 3.7% **above** the initial 1.4433. The "mean-torque feasibility depends on current policy" finding is correct as stated, but the deficit is dominated by the 20% shorter stack, not by the current rule. The paper names only ScaleR ≈ 1.048.
- **Major (M2).** The paper never compares GEODE's Opt120 to the **saved Opt120 arrays**, although the record (`opt120_export_L3_h45_reference.json`) is in its own manifest. Under the reference's current, GEODE L3 order-45 gives full-current mean 1.4986 vs saved 1.5000 (−0.1%) but std 0.000353 vs saved 0.000176 (2.0×); half-current 0.000553 vs 0.000437 (1.27×); max pointwise gap 2.1e-3 N m. The saved reference's own initial/Opt120 ratio is 11,982 against GEODE's 7,631. This is the paper's thesis in action (mean within 0.1%, ripple off by 2×) and should be reported, with the statement that the reference refinement was run only at the initial design so the cause at Opt120 is unresolved.

| # | Dimension | Score | Weight |
|---|---|---|---|
| 1 | Rigor of method / argument | 5 | 6 |
| 2 | Evidence sufficiency | 5 | 6 |
| 3 | Clarity of contribution | 4 | 5 |
| 4 | Related-work positioning | 4 | 5 |
| 5 | Reproducibility | 3 | 5 |
| 6 | Figure & table quality | 3 | 4 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 3 | 4 |
| | **Total** | **36** | **44** |

### Top 5 revision priorities

1. **S1, critical.** Force-add `motor-gradient-optimization.4/figures/src/data/m1_tables.log` (or drop it from the manifest if it is not a figure input; `figures/src/data/README.md` says it is the `--nocapture` test output, and no renderer reads it). Re-run `python3 verify.py` from a clean checkout and regenerate `preflight.json` from that run. Consider making `verify.py` print the missing snapshot path instead of a traceback.
2. **M1.** State Opt120's sizing scalars (LENGTH 80 mm, SD1 130.3 mm, ScaleR 1.048, angle −11.6°), say that the reported torques carry the factor s²L/L₀ = 0.878, and separate the sizing effect from the current-rule effect in §5.5 and the conclusion ("fixing physical current also removes its mean-torque advantage" → "at the saved design's 80 mm stack and fixed physical current, the mean falls 13% below the initial 100 mm design; per unit length and at fixed unscaled current it is 3.7% higher"). An Opt120 column in Table 4 would do most of the work.
3. **M2.** Add the saved-vs-GEODE Opt120 comparison under the reference current rule (one sentence plus a row in Fig. 4(c) or a short table), and say the reference refinement at Opt120 was not run.
4. **M3.** §5.5: say that Opt120's L1→L2→L3 increments in F do not shrink (+3.05e-8, +2.90e-8 N²m² at order 45), so L3 is not shown to be asymptotic for the candidate; then say the 3.6-fold interface effect is 100× that increment, so the conclusion survives. The current "these increments are not error bounds" undersells the point.
5. **m, hypothesis sentence.** State the mechanism the data suggest, labeled as a conjecture: a design optimized against the order-45 model cannot see the modes that model does not transmit; under order 93 Opt120's loaded std rises 0.00066 → 0.00164 N m while its cogging std falls 0.00061 → 0.00035 N m. "We conjecture" is permitted; the present text ("its magnitude depends strongly on the interface model") leaves the reader to infer it.

## Focus questions from the dispatch

**Does the contribution stand without experiment 2?** Yes. The paper no longer tests the BRIEF's claim ("whether exact field gradients reduce time to a validated target"); it tests what has to be true before that comparison means anything: that the ripple objective is interface-dependent on a fixed mesh (7–9% loaded, 59% cogging at L3; identity check 4.1e-9/1.2e-9 N m; ladder), that the independent reference agrees with GEODE once refined (0.10% cogging, 0.35%/0.27% loaded), that a published optimum's reported improvement is model-dependent (ratio 7,631 → 2,454 → 2,444) and current-rule-dependent, and that verified discrete gradients (688/704 ≤ 2.8e-7) still differ by model (ripple gap 6.6%/10.3% at L3, persisting beyond the 0.5–1.2% refinement change). Title, abstract and conclusion claim exactly that and no speedup. The three paragraphs of §7 that describe the unperformed protocol (lines 697–715) are the only place the missing experiment leaks into the paper, and they cost rhetorical economy rather than validity (see D9).

**Model-dependence findings as findings, not excuses.** The ×3.6 change is in the abstract, §5.5, §7 and §8 as a measured result with the design-ratio consequences spelled out; the current-rule reversal (+1.68 vs −220.85 N m/m for SW1; seven of ten derivatives changed, SW1/SW4/SD1 reversed) is given with its mechanism (the winding-area/current term) and scoped to the state where it was measured; the Arkkio band-validity limit is stated as a geometric fact (the inner band is no longer annular) with the estimator substitution declared and the restriction explicitly not generalized (lines 449–454). Two things weaken the presentation: M1 (the sizing omission makes the current-rule finding look larger than the current rule alone accounts for) and the missing paired-estimator statement for the Opt120 field (the evidence has it: on the Opt120 order-45 field at L3, outer-band Arkkio and coupling torque agree to 1.3e-4 N m in mean and 1.1e-5 N m in std at full current; 6.1e-5 / 6.6e-7 at zero current), which is what justifies Fig. 4(c)'s conforming point.

**Cogging resolution caveats.** Argued correctly. The chain is: 3×3 rerun reproduces the saved arrays (9.7e-13 N m, so the environment is the reference's); refinement at fixed degree, orders, torque definition and Newton stop; sequence 0.01071 → 0.01220 → 0.01212 → 0.01208 N m; GEODE L3 0.01209 (+0.10%); max pointwise 2.1e-3 → 3.4e-5 N m; all five finest statistics inside the Eq. (10) bands (recomputed: cogging band [0.01190, 0.01228], half [0.03899, 0.04023], full [0.07914, 0.08157]; the saved cogging and half-current values lie outside, the saved full-current value inside). The paper then says what the agreement is not: finite discretizations, an extrapolated limit "near 0.0120 N m, an estimate without a bound" (my geometric-tail estimate from the last ratio 0.55 is 0.01202), Newton stop not a factor (≤ 4.4e-8 N m), straight-sided geometry, high-field law unvaried, and "neither cross-code agreement nor refinement proves physical accuracy". Conclusion wording "identifying coarse reference resolution as its cause at matched order" is right: it is a cause at the matched truncated model, and the paper keeps open which model is closer to the machine. One sentence understates: "its standard deviation may still change by about 0.3%" for half current. The last two increments are −0.3% and −0.3% (ratio 0.90), so 0.3% is the size of the last step, not an estimate of what remains; say the half-current increments are not shrinking and leave the remainder unestimated.

**Do the figures carry the argument?** Mostly. Fig. 2 (waveforms and residuals with the harmonic band) and Fig. 3 (spatial vs order refinement) carry §5.1–5.2; Fig. 4 is new and carries §5.4–5.5 in four panels that each match a numbered claim; Fig. 5 carries the verification statistics and the reversal; Fig. 6 carries the cross-model gap. All have units, legends and self-contained captions. What is missing is the one figure a machine-design referee would ask for: the Opt120 torque waveform under order 45, order 93 and conforming coupling (the benchmark's `figures/waveforms_L3.png` shows the reappearing content at about 5° period). Without it, "×3.6" is a number rather than a visible mechanism. Fig. 4(c) also joins three categorical interface models with line segments, which implies a continuum that does not exist.

## Numbers recomputed from `origin/main` blobs

All from raw run records (`T_coupling`, `T_arkkio*`, `torque_Nm`, gradient vectors), not from summaries.

| Claim (main.tex line) | Paper | Recomputed | Status |
|---|---|---|---|
| Initial/Opt120 F ratios at L3 (461–462) | 7,631 / 2,454 / 2,444 | 7,631 / 2,454 / 2,444 | ok |
| Opt120 F ×(93/45); initial +16% (458–461) | 3.6; 16% | 3.608; +16.03% | ok |
| Opt120 full-current std (462–463) | 0.00066 → 0.00164 | 0.000660 → 0.001636 | ok |
| L2→L3 F change / F(L3), max over models (465–467) | 1.36% initial, 2.71% Opt120 | −1.36/−1.29/−1.14%; +2.71/+1.51/+0.76% | ok |
| Current rules, L3 order 45 (469–473) | 1.4433; 1.3147 / 1.2550 / 1.4983; −8.9 / −13.0 / +3.8% | 1.44326; 1.3147 / 1.2550 / 1.4983; −8.91 / −13.04 / +3.81% | ok |
| g10 vs g40, Opt120 L3 (479–482) | +1.05% / −5.90% / −17.1% | +1.05 / −5.90 / −17.11% | ok |
| Moved finest mesh (447–448) | four inverted triangles | `invalid_mesh`, inverted = 4 | ok |
| 3×3 rerun vs saved (416–417) | ≤ 9.8e-13 N m | 9.73e-13 | ok |
| Reference cogging std sequence (423–424) | 0.01071 / 0.01220 / 0.01212 / 0.01208 | same at 5 d.p. | ok |
| GEODE vs finest reference (424–429) | 0.10%; 0.35% / 0.27% | +0.102%; +0.349% / +0.269% | ok |
| Max pointwise cogging gap (426–427) | 2.1e-3 → 3.4e-5 | 2.11e-3 → 3.41e-5 | ok |
| Finest loaded std (428) | 0.03947 / 0.08014 | 0.03947 / 0.08014 | ok |
| Five finest statistics inside L3 bands (429–431) | all inside | all inside (bands above) | ok |
| Newton stop 1e-6 → 1e-14 (438–440) | ≤ 4.4e-8 | 4.40e-8 / 4.28e-8 / 4.25e-8 | ok |
| DOF 4,079 / 12,518 / 43,220 / 159,920 (420–421) | | from `tables.md` (not in run JSON) | ok |
| L3 order 45 vs conforming std (359–360) | −7–9% loaded, +59% cogging | −7.4%, −9.0%, +59.2% | ok |
| Order 189 at L1/L2 (360–361) | within 1.1e-4 | 1.02e-4 / 6.63e-5 (order 93: 3.9e-3 / 4.0e-3) | ok |
| Full-order identity (268–270) | 4.1e-9 (L0), 1.3e-9 (L1) | 4.057e-9, 1.2417e-9 | ok (1.3e-9 is the agreed fixed point) |
| Table 1 nine rows; ripple and mean deviations (316–321) | | all nine at 5 d.p.; +9.86/+11.27 → +1.70/+1.27; −29.09 → +12.89; −0.153/−0.156; −0.002/+0.051 | ok |
| Max full-current pointwise gap (332–333) | 0.0272 → 0.0034 | 0.0272 → 0.0034 | ok |
| Saved half-current std below its interval (333–334) | ≈ 5e-5 | 4.6e-5 | ok |
| Estimator and free-end controls (377–381) | < 9e-5; < 5.4e-5 | 8.84e-5; 5.33e-5 | ok |
| L3 cross-model gaps (628–630) | ripple 6.58 / 10.25%; mean 0.16 / 0.26% | 6.58 / 10.25; 0.16 / 0.26 | ok |
| L2→L3 within-model change (629–630) | ripple 0.46–1.19%; conforming mean 0.52–0.54% | 0.46, 0.49, 1.19, 1.13; mean conf 0.54 / 0.52 (h45 0.05 / 0.07) | ok |
| L2 norm gaps; MAG ripple derivatives (618–625) | 0.58/0.66; 6.7/10.2; 1.28 vs 1.94; 0.75 vs 1.87 | 0.58/0.66; 6.67/10.24; 1.283/1.937; 0.753/1.865 | ok |
| SW2 sequences (605–607) | −46.5, −41.3, −44.4, −47.2; −49.8 → −47.1 | same | ok |
| SW1 under two rules; 7 changed; SW1/SW4/SD1 reversed (540–547) | +1.68 / −220.85 | +1.68 / −220.85; changed = SW1 SW2 SW3 SW4 SD1 Sr1 ScaleR; flipped = SW1 SW4 SD1 | ok |
| Cost ratios (663–665) | 1.13–1.20; 8.2–10.5 | 1.126–1.199; 8.243–10.532 | ok |
| Box widths (Fig. 6 caption) | 9, 10, 15, 4, 3, 1, 18, 70, 1.5 mm | same | ok |
| Validation solve statuses (not stated for §5.5) | | 2,560 rows over the 14 runs used, all `tolerance` | ok (worth one clause) |

`numeric_consistency` flags one claim: "0.27% higher" (line 429) paired by the ±1-paragraph window with "from 0.01071 to 0.01220". That pairing is wrong; the claim compares GEODE's full-current std (0.08036) with the finest reference (0.08014), recomputed +0.269%. Confirmed false positive, already recorded in `preflight.json`.

## Resolution of v3 comments (changelog coverage)

| v3 item | v4 status |
|---|---|
| Opus S1: three reversals over-generalized | Resolved: abstract (31–33) and conclusion (734–735) name "the initial L1, order-45, full-current state". |
| Opus S2: compare gradient gap with refinement | Resolved: lines 628–632 give L3 gaps and L2→L3 changes; recomputed above. |
| Opus S3, Fable v2 S3: Opt120 deferral | Superseded by the #1182 evidence (§5.5). Full-band Arkkio correctly not used. See M1/M2 for what the new section still omits. |
| Opus S4, Fable S1: "all resolved interface orders" | Resolved: "Retaining every order below the trace Nyquist limit" (22–23). |
| Opus S5, Fable S3: norm hides components | Resolved: SW2/SW3 6.5%/17% named (625–627); per-parameter ratio range declined with a stated reason (near-zero components); acceptable. |
| Opus N1/N2, audit figure note | Resolved: coupling-circle label outside the band; Fig. 5(c) legend below axes, +1.68 in clear space (checked at 170 dpi). |
| Opus N3/N4, Fable S2: signs | Resolved everywhere ("lowers … raises"). |
| Opus N5: name Taylor failures | Resolved (593–595). |
| Opus N6, Fable S9: author | Resolved: Robb Walters, also in PDF metadata. |
| Fable S4–S8 (captions, % cells, qualifiers, SW3 wording) | All resolved; Table 1 unit in header only; "increments shrink with refinement" (603). |
| Fable S10, Opus D4: harmonic-coupling positioning | Resolved: Egger's order ladder and Bontinck's stability named; "torque sensitivity to interface order is already established; our study quantifies …" (97–99). |
| Fable S11: solver-verification paragraph | Resolved, condensed to two sentences (274–278). |
| Fable nits (p.7 blank, Fig. 4d legend) | Legend fixed; p.7 still has roughly a quarter page blank below §5.3 with Table 2 forced to p.8 by `[H]`. |
| Audit note 1: 1.3e-9 vs 1.2417e-9 | Declined with reason; agreed fixed point. |
| Audit: ICEM title case | Resolved (sentence case in refs.bib, renders correctly p.17). |
| Audit: verify.py hardening | Resolved: verifier extended to reruns, current rules, aliasing, invalid mesh, L3 norms. |

Every v3 comment is accounted for in `changelog.md`; none is silently dropped.

## Scoring with quoted evidence

**D1 Rigor 5/6.** The forward comparison is paired ("Forward comparisons use Arkkio for both GEODE couplings to isolate the interface change" — §3.3), carries the identity check and the ladder, and the reference rerun is set up correctly: "The $3\times3$ rerun first reproduces all 60 saved torques to within $9.8\times10^{-13}$\,N\,m" (§5.4) before anything is refined, with degree, orders and torque definition held fixed. The candidate validation declares its estimator substitution and refuses to generalize it ("This estimator restriction is specific to these geometries; it does not validate outer-band Arkkio on arbitrary moved meshes or stator offsets" — §5.5). The gradient chain reports deduplicated best-step errors, Taylor windows, remesh scatter and mesh convergence, and §6.3 now measures the cross-model gap against refinement rather than FD error. Deductions: M1 (the mean-torque feasibility argument omits the sizing scalars that produce most of the effect); M3 (Opt120's non-shrinking refinement increments are not stated, only "these increments are not error bounds"); the Opt120 paired-estimator check that justifies the conforming point exists in the records but not in the paper; the cross-model gradient comparison still changes estimator and interface together (acknowledged, §6.3).

**D2 Evidence 5/6.** 1,640 original forward solves, 2,560 validation rows over the 14 runs the section uses (all at tolerance), 300 reference reruns across four subdivision levels, 704 distinct FD cases with 592 Taylor windows, and pinned L3 gradients. The new evidence answers both open v3 questions: "GEODE L3 at order 45 gives 0.01209\,N\,m, a 0.10\% difference from the finest reference" (§5.4) and "raising the transmitted order from 45 to 93 multiplies Opt120's $F$ by 3.6" (§5.5). Deductions: M2 (the saved Opt120 arrays are in the manifest and the GEODE/saved ripple differs 2× at matched order, unreported); the reference refinement is at one design and the candidate study at one candidate (both stated); no Opt120 waveform is shown.

**D3 Clarity of contribution 4/5.** The organizing idea opens the abstract ("A motor optimization target can change with the numerical interface even when average torque agrees closely") and each contribution is extractable in one sentence from §1 paragraph 3. Deductions: the abstract is a list of 14 numbers with no sentence that says why a reader should care about the Opt120 result; the mechanism hypothesis (optimizing against a truncated model) is never stated even as a conjecture; and M1 means the feasibility claim ("fixing physical current also removes its mean-torque advantage" — §8) is clearer than it is true, since the design is also 20% shorter.

**D4 Related-work positioning 4/5.** The section now says what is known and what this paper adds: "Thus torque sensitivity to interface order is already established; our study quantifies its effect on loaded nonlinear ripple, a saved optimized design, and design sensitivities" (§2), and the Egger ladder (39–129 vs order 200) and Bontinck stability discussion are engaged on their merits. No litsearch sibling (no deduction). Deduction: the primary source for the harmonic sliding-interface coupling the paper uses, De Gersem and Weiland, "Harmonic weighting functions at the sliding interface of a finite-element machine model incorporating angular displacement", IEEE Trans. Magn. 2004, doi 10.1109/TMAG.2004.824616 (surfaced by an anonymous OpenAlex query), is not cited; Bontinck 2018 and Egger 2022 are its descendants. Lead for `paper-litsearch`, tag `related-work`; not a critical flag. The v3 audit's optional Merkel–Kapidani–Schöps–Vázquez 2022 (TMAG, isogeometric mortar torque) remains optional.

**D5 Reproducibility 3/5.** Commit-pinned links for every evidence set, 112-file manifest, Octave/GeoPDEs/nurbs versions for the reruns ("The run uses Octave 11.3.0, GeoPDEs revision \texttt{53ae5ca1} and nurbs 1.4.3" — §5.4), and a verifier that reconstructs the new statistics from raw torques. Deductions: S1 — the declared artifact check does not run from the committed tree (one gitignored snapshot missing), and `preflight.json` records a PASS that the checkout cannot reproduce; the GEODE-side mesh/solver toolchain versions (gmsh, Rust toolchain) are only in benchmark READMEs; timing is unreproducible in absolute terms (stated, and no longer a headline).

**D6 Figure & table quality 3/4.** Six figures, all with units and legends; captions name the model/estimator pairs ("Harmonic values use coupling torque; conforming values use outer-band Arkkio" — Fig. 4) and the rules ("under three current rules; the dashed line is the initial mean" — Fig. 4). The v3 overlaps are fixed. Deductions: Fig. 4(c) draws lines through categorical interface models; no Opt120 waveform; p.7 leaves a quarter page blank with Table 2 forced to p.8; p.16 holds only Table 4 and the first references.

**D7 Prose & structural quality 4/4.** Flow intact, tense consistent, no AI-tell adjectives, no "load-bearing", 0 em-dashes, no `??`, 0 overfull boxes, author present, audience check 0 hits. No instance of virtue-signaling language found. Nits that did not cost a point: §5.5's sentence with three means followed by three percentages (469–474) reads as a table; "the earlier cogging mismatch" (432) is fine in context but "the §5.1 cogging mismatch" would remove any version-history reading.

**D8 Citation hygiene 5/5.** All 13 keys resolve, every entry has author/title/venue/year/DOI (Arkkio 1987 has the institutional URL), the ICEM title is now sentence case, and each new v4 claim about prior work is specific ("Their linear-material cogging experiment varies the maximum transmitted order from 39 to 129 and compares against a refined order-200 reference" — §2) and matches the primary-source notes in `citation-notes.md`. No claim that needs a citation lacks one; the De Gersem omission is positioning (D4), not hygiene.

**D9 Rhetorical economy 3/4.** The paper leads with the thesis and the headline numbers appear where they should. Deductions: §7's third and fourth paragraphs ("The prospective optimization comparison freezes outer diameter … Targets not reached within budget remain censored outcomes" — §7, 19 lines) describe an experiment with no results in the paper; three sentences would tell the reader what comes next; §6.2 still stacks caveats across three paragraphs; Appendix A is cited once and could be two sentences in §7.

## Critical-flag check

- **Build / compile, including external artifact (`artifact_verify`):** PDF present, 17 pages, compile ok, 0 overfull, bib clean. **`python3 verify.py` exits 1 on the committed tree** (missing `figures/src/data/m1_tables.log`, gitignored by `papers/.gitignore:4`). Flag `artifact_verify_1` raised. With the single blob restored, `verify.py` exits 0 with both PASS lines, so the flag is isolated and the fix is one `git add -f`.
- Citation error: none.
- Numerical inconsistency: none; 30 claims recomputed from `origin/main` match; the one detector finding is a confirmed false positive.
- Missing experiment for a claim: none; no speedup claim, the prospective protocol is labeled as unmeasured.
- Close prior work ignored: no; De Gersem–Weiland 2004 is a positioning lead, not a novelty misrepresentation.
- Audience: 0 governance or private-locator hits.
- Pending markers: 0.

## Findings by severity

S = blocking (critical flag), M = major, m = minor.

**S1 (D5; critical).** `motor-gradient-optimization.4/figures/src/data/m1_tables.log` is listed in `evidence-manifest.json` (source `benchmarks/motor_nonlinear_m1/m1_tables.log` at `581ee7ad`, SHA `ccc86e29…`) but is not tracked at `fea6ba5b` (`git ls-files` shows only the `.msh` under `data/`; `git check-ignore -v` → `papers/.gitignore:4:*.log`). The `.3/` package has the same file force-added. `verify.py` line 19 raises `FileNotFoundError` and exits 1 from a clean checkout. `preflight.json` (`artifact_verify: "PASS …"`) was produced in `/Volumes/Stripe/geode-fem/motor-paper-v1/…`, not from the committed tree. Fix as in priority 1.

**M1 (D3, D1; §5.5 lines 469–477, §8 line 731).** Opt120's scalars in the validation records: LENGTH 0.0800 m, SD1 0.1303 m, ScaleR 1.0477, OPERATING_ANGLE −11.65°; `torque_factor = ScaleR²·LENGTH/0.1 = 0.878` is applied (initial: 1.0). State these, add an Opt120 column to Table 4, and separate the sizing factor from the current-rule effect. Suggested sentence: "Opt120 also shortens the stack to 80 mm and scales the radius by 1.048, so every Opt120 torque carries the factor $s^2L/L_0=0.878$; per unit length at fixed unscaled current its cross-section gives 1.497 N m, 3.7% above the initial design."

**M2 (D2, D1; §5.5).** Add the saved-vs-GEODE Opt120 comparison at order 45 under the reference current: mean 1.4986 vs 1.5000 N m (−0.1%); std 0.000353 vs 0.000176 (full), 0.000553 vs 0.000437 (half), cogging 0.000611 vs 0.000663 (fixed rule, zero current); max pointwise 2.1e-3 N m; saved ratio 11,982 vs GEODE 7,631. State that the reference was not refined at Opt120, so the cause is open (coarse 3×3 IGA is the obvious suspect, by analogy with §5.4, but untested). This is the strongest illustration of the paper's first sentence and it is currently absent.

**M3 (D1; §5.5 lines 465–467).** Report that Opt120's order-45 F increments are +3.05e-8 (L1→L2) and +2.90e-8 (L2→L3) N²m², i.e. not shrinking, whereas the initial design's are −2.33e-4 and −1.11e-4. Then say the interface effect (+2.8e-6) is two orders of magnitude larger, so the 3.6-fold conclusion does not depend on L3 being asymptotic.

**m1 (D1; §5.5).** Cite the paired estimator check on the Opt120 harmonic field (L3, order 45, full current: |Δmean| 1.3e-4 N m, |Δstd| 1.1e-5 N m between outer-band Arkkio and coupling torque; zero current 6.1e-5 / 6.6e-7) as the justification for using outer-band Arkkio for the conforming Opt120 value.

**m2 (D1; §5.4 lines 436–437).** "may still change by about 0.3%" → the half-current increments (−0.3%, −0.3%) do not shrink; leave the remainder unestimated or say "by at least the last increment".

**m3 (D3; §5.5 or §7).** One labeled hypothesis sentence on mechanism (priority 5). The h93 asymmetry (loaded std up 2.5×, cogging std down 0.42×) is already in the records and supports it.

**m4 (D4).** De Gersem and Weiland 2004 (doi 10.1109/TMAG.2004.824616) as the origin of harmonic sliding-interface coupling; lead for `paper-litsearch`, reviewer writes no `.bib`.

**m5 (D6).** Fig. 4(c): use markers without connecting lines, or a grouped bar chart, for the three interface models. Add an Opt120 waveform panel (order 45 / 93 / conforming, full current) or a supplementary figure. Fig. 4(b): say in the caption that the IGA 24×24 point is zero by construction.

**m6 (D6; layout).** p.7: `[H]` on Table 2 forces it below Fig. 3 on p.8 and leaves the bottom of p.7 blank; `[t]` or `[htbp]` would fill p.7. p.16/17: Table 4 alone on p.16; harmless.

**m7 (D9; §7 lines 697–715).** Compress the prospective-protocol paragraphs to three or four sentences (spaces, validation model, targets, censoring) or move them to an appendix; the reader of this paper gets no result from them.

**m8 (D9; §6.2 lines 588–613).** The remesh/zero-current/mesh-convergence caveats would read faster as one short list; the content is right.

**m9 (D5; §Code and data).** Name the GEODE-side mesh toolchain version (gmsh) once, or say explicitly that the benchmark READMEs carry it.

**m10 (D7/D2; §5.5).** One clause stating that all validation solves used stopped at tolerance (2,560 rows over the 14 runs the section uses), matching §3.2's statement for the original 1,640.

**m11 (procedural).** The `forward_harmonic/README.md` snapshot matches `910593ba` but not `origin/main` (the #1189 update note was added later). Not a defect; the manifest pins the commit. A one-line note in `README.md` that snapshots are pinned to `source_commit`, not to main, would pre-empt the question.

## Procedural notes

- Review written to this file per dispatch; no `.review` sidecar staged; nothing under the worktree created or modified (`git status --short` empty before and after).
- Recheck script: `scratchpad/motor-v4/scripts/recheck_v4.py`; extracted `origin/main` blobs: `scratchpad/motor-v4/evidence/`; scratch copy with the restored log for the `verify.py` run: `scratchpad/motor-v4/v4copy/`; 170-dpi page rasters: `scratchpad/motor-v4/pages/`; OpenAlex responses: `scratchpad/motor-v4/lit/`.
- Network: four OpenAlex `works?search=` requests with a generic User-Agent, no `mailto`, no email or identity in any header or parameter. No Crossref requests.
- `web_search: true` is set in the BRIEF; the D4 lead above is the only close-prior-work candidate surfaced; it is reported as a lead, not written to any `.bib`.
