# v4 reviews (head fea6ba5b): combined must-fix list

- **Opus:** 38/44.
- **Fable:** 36/44.
- **Both:** advance = false. Each review raises the same one critical flag.

## Critical (both reviewers)
- **C1. verify.py fails from a clean checkout.** It raises FileNotFoundError on `motor-gradient-optimization.4/figures/src/data/m1_tables.log`. The file is excluded by `papers/.gitignore` (`*.log`) and was never `git add -f`. In v3 it was tracked. Preflight passed only because the file sits untracked in the author's working tree.
  - **Fix:** `git add -f` the file.
  - **Check:** rerun verify.py from a fresh clone or `git archive`.
  - **Then:** regenerate preflight.json.
  - With the file restored, both reviewers saw every PASS line.

## Major
- **M1. The Opt120 mean-torque deficit is mostly stack length, not current policy.** Fable D1/D2; Opus top-2 is the same conclusion sentence.
  - The saved Opt120 has LENGTH 0.080 m (initial 0.100), SD1 0.130 and angle −11.6°. The records scale every Opt120 torque by ScaleR²·LENGTH/0.1 = 0.878.
  - Per unit length, under fixed unscaled current, the cross-section is 1.497 N m: **+3.7%** above the initial design.
  - Conclusion L731, "fixing physical current also removes its mean-torque advantage", is wrong as stated.
  - **Fix:** separate the length/sizing effect from the current rule, and add an Opt120 column to Table 4.
- **M2. §5.5 doesn't compare GEODE's Opt120 with the saved Opt120 arrays,** although those arrays are in the manifest (Fable). Under the reference current at h45, GEODE vs saved:

  | quantity | GEODE | saved |
  |---|---|---|
  | mean | 1.4986 | 1.5000 (−0.1%) |
  | full-current std | 3.53e-4 | 1.76e-4 (2.0×) |
  | half-current std | 5.53e-4 | 4.37e-4 |
  | θ0/Opt120 ratio | 7,631 | 11,982 |

  The maximum pointwise difference is 2.1e-3 N m.
  - **Fix:** state these numbers. Note that the reference refinement (#1191) was run only at θ0.
- **M3. Opt120's L2→L3 increments don't shrink:** h45 F gives +3.05e-8, then +2.90e-8 (both reviewers). Another fresh mesh family reaches 3.2%.
  - **Fix:** say so in §5.5, then note that the 3.6× interface effect is about 100× larger, so the conclusion still holds.
  - **In the abstract's ×3.6 sentence:** θ0 rises only 16%, and the reduction stays above 2,400-fold.
- **M4. Cogging caveat placement** (Opus). "Not fully converged" doesn't appear next to "12.9% → 0.10%" in the abstract or conclusion.
  - The extrapolated reference limit is about 0.3% below its finest value, and GEODE's L3 cogging band is ±1.6%. So 0.10% is a point agreement, not an accuracy.
  - Fable: for the half current, "may still change by about 0.3%" understates it. The last two increments are −0.3% and −0.3%, and are not shrinking.

## Minor
- **Captions.** Table 1, Fig. 2 and Fig. 3 measure deviation from the under-resolved saved reference; point them to the refined reference (Opus). Fig. 6 is L2-only, and its perturbed h45 MAG ripple moves from 0.75 to 0.57 at L3, so quote the L3 values.
- **Mechanism.** State it as a labelled conjecture: Opt120 was optimized against the truncated h45 model. Under h93 its loaded std rises 0.00066 → 0.00164, while its cogging std falls 0.00061 → 0.00035 (Fable).
- **Paired-estimator check** (Fable). At L3 on the Opt120 field, outer-band Arkkio vs coupling torque gives |Δmean| 1.3e-4 and |Δstd| 1.1e-5 N m. Reporting this justifies the conforming point in Fig. 4(c).
- **Fig. 4(c)** joins categorical models with lines. Optional: an Opt120 waveform under h45, h93 and conforming.
- **Amendments cited.** The changelog, BRIEF and EXPERIMENTS cite "#1185 amendments 1–4". Amendments 5 and 6 now exist, and the text already complies with them.
- **§7.** The prospective Exp 2 protocol summary takes 19 lines; trim it (D9).
- **Snapshot wording.** 111 of 112 snapshots equal origin/main. The odd one, forward_harmonic/README.md, gained a later #1189 note. It matches its pin, so "identical to origin/main" is not literally true.
- **Literature lead** (not a flag). De Gersem & Weiland 2004, IEEE Trans. Magn., doi 10.1109/TMAG.2004.824616, originated the harmonic sliding-interface coupling the paper uses. Cite it if it is not already cited.

## Verified by both reviewers
- About 30 numbers, recomputed from raw blobs, all match.
- The #1191 3×3 rerun matches the saved torques to 9.7e-13.
- Ratios 7,631 / 2,454 / 2,444; current rules −8.91 / −13.04 / +3.81%; four inverted triangles; the L3 cross-model gaps.
- Every v3 comment is addressed or declined with a reason.
- No speedup claim, and C is labelled an empirical surrogate.
- Sole author.
- 1.3e-9 is kept.
