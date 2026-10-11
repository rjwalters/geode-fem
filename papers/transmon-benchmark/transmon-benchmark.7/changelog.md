# transmon-benchmark.6 → transmon-benchmark.7

Prepared for #1039. This is an explicitly requested post-v6 evidence
revision, not an automatic attempt to improve a passing score. Revising
past the old cap of 6 is authorized by the operator: on 2026-10-10 the
operator gave standing permission to keep revising this paper ("we can
continue revising the paper as many times as we want"), recorded on
PR #1040 by an agent session on the operator's behalf. `max_iterations`
is now 12 so that the cap does not gate the next revisions.
No operator critic or independent review is fabricated. v6 and every v6
critic remain byte-identical.

| Input | Change or disposition |
|---|---|
| #594 and #1034 curation | Added harmonic motion, FD validation, safe budget, stalled endpoint and conditional endpoint-slope shortfall. Distinguished 3.54× initial-slope estimate from the conditional extrapolation of at least 6.25× (computed 6.256959, floored because it is a lower bound). |
| #964 / #927 / #763 | Replaced the old CPU table, chart, contribution bullet, discussion and conclusion claims. Report all three mode-recovering routes, extra modes, reference tuning, pinning, measured core-seconds, repeat counts, and output/estimator caveats. No winner chosen. |
| Later tuned-Palace evidence | Corrected Palace's timed default solver identity to SuperLU; qualified tuning by its same-box control. Removed the unsupported direct-versus-AMS interpretation. |
| #1002 / #1003 | Added single-factorization port-subspace result. Kept loaded-Mac CPU-time evidence separate from the EC2 table. The earlier wording here (a retained "spurious-mode limitation", then a planned `LumpedPort`-like formulation to remove the 3.45 GHz mode) is superseded by the #1003 spike's finding; see the in-place corrections below. |
| #967 | Added separate assembled CPU-f64 driven AMS section/table with time-boundary, mesh, repeat and memory limits. No GPU or eigenmode extrapolation. |
| #1034 (Phases A and B merged) | The `[PENDING issue-1034]` placeholder was replaced by the #1035/#1036 coarse-mesh result, which pass 3 below re-reads as a cautionary precursor. No pending marker remains after pass 3. |
| v6 review D2 (evidence sufficiency) | Harmonic, AMS and three-parameter optimizer evidence added; pass 3 adds the mesh-convergence study and the refined real-device optimization. No new score asserted. |
| v6 review minor: operator inputs | Every TODO(operator) line retained verbatim, including title, affiliations, tracking URL, repository/DOI and acknowledgments. |
| v6 review minor: length/recap | Shortened conclusion and replaced the old CPU section. New evidence still leaves the paper above the original length target; final compression remains open. |
| v6 review minor / audit: off-disk citations | Bibliography and citation-key set preserved. No new literature verification claimed; existing support limitations remain. |
| v6 review nit: figure filename/number skew | Retained historical filenames to avoid unrelated source churn; rebuilt CPU figure with direct TOML input and a self-contained script. |
| v6 review nit: rounded core-second inset | Replaced obsolete inset with all mode-recovering wall-time routes; table reports measured user+sys CPU seconds. |
| v6 audit: prior quotation fix and traceability notes | Retained resolved quotation and anchor-rounding clarifier. Removed superseded off-target table (and its borrowed RSS cells), preserved unchanged GPU 136× convention. |
| v6 numeric sidecar | No prior findings. Added a focused executable check for new table values, evidence counts, harmonic arithmetic and marker preservation. |

New version vendors the canonical paper class, carries all existing figures
and their sources forward, adds `xurl` for long artifact-path wrapping, and
ships a rebuilt PDF and BibTeX output. See `validation.md` for observed checks.
This revision is a draft, pending independent review/audit and the operator
submission inputs; it is not READY or AUDITED. The converged-mesh capacitance
(#1051) is now reported; the two-target experiment (#1037) is roadmap.

## In-place corrections on PR #1040 (2026-10-10)

v7 was unmerged when `main` moved, so it was corrected in place instead of
opening a v8. v1–v6 and their critics remain byte-identical. Four of the
draft's claims were wrong or stale.

| Finding on `main` | What changed in v7 |
|---|---|
| The 3.4528 GHz mode is the transmon qubit mode (#1003 spike, PR #1047; Palace re-run #1043, PR #1055). | **Withdrew two claims carried since v1:** that no qubit mode exists by construction with Palace confirming its absence, and that 3.4528 GHz is a spurious port artifact with no Palace counterpart. §6 is rewritten. PROVEN: Palace at `Target = 2 GHz` returns 3.452749 GHz with port EPR 0.99424 (geode 3.452751, p = 0.9942); L-doubling ratio 0.70810; the control cell reproduces the committed spectrum to 6.5e-13; the earlier run could not return the mode because Palace searches only above `Target`. New Table 2. The same-mesh caveat is stated: this is not a converged qubit frequency. Abstract, introduction, contributions, §5, §7, §11.1, §12, §13 and the conclusion follow. |
| The 17.49 GHz "junction LC" mode has Palace port EPR 2.49e-8 in both runs. | Reclassified as a zero-voltage resonance of the distributed port term that both solvers share (new §6.3). Kept four evidence tiers apart: PROVEN (Palace's committed port EPR, the 0.70711 scaling), MEASURED BY A SCRATCH PROBE (uniform-voltage share 0.00 % / 100.00 %), DERIVED (the patch-bump mechanism; its falsifier is not run), ROADMAP (the rank-1 port-voltage formulation, #1044, not built; the probe result is not a shipped feature). The f_LC and L-doubling tripwires are now described as tests of the port operator. The 0.001 % agreement on this mode is described as agreement on a shared artifact. The gauge/projection arc is re-read (§6.2): the bulk projector kept the qubit mode and removed the artifact. The previous Doctor's sentence about a planned `LumpedPort`-like formulation to remove the 3.45 GHz mode is deleted. |
| Palace refines the input mesh: 183,891 ND unknowns against geode's 156,863 (run logs; #1047 note §1). | Every "identical mesh / identical discrete problem" statement now says "identical input mesh". §4.1 discloses the 4,913 bisection elements and 6,014 duplicated vertices. Table 1, Table 4, §11.1, §12 and the limitations carry the count; the timing comparison gets a fairness caveat (Palace solves about 17 % more unknowns; not normalized). The ≤0.033 % agreement stands. Table 4's "six physical modes" is now "six reference modes", and its "extra modes" are stated to include the qubit mode in two rows. |
| The committed mesh is not converged for C_Σ: one red refinement lowers X⁰ from 137.706752 to 101.359397 fF, −26.3 % (#1036 review, PR #1050). | New §7 paragraph: coarse values are upper bounds, the 36.34 fF shift is a lower bound on the coarse error, one level only, converged value unmeasured. **Withdrew the "model-definition difference, not a truncation artifact" diagnosis** of the 136.7 vs ~90 fF gap. The caveat is attached to C_Σ = 136.7 fF, E_C = 0.142 GHz, the #589 pad demonstration, the #594 harmonic run, the 33× and 6.25× shortfalls and the 89.9 fF anchor discussion. The #1051 convergence study is in progress and unmerged; none of its results is cited. |
| #1034 Phase A (#1035, PR #1042) and Phase B (#1036, PR #1050) merged. | §9.3 replaces the placeholder with a METHODS result: three junction-pinned gradients, FD-validated; a bounded optimizer with per-step volume-stiffened re-morphing; the coarse-mesh scalar-ε C_Σ reaches 89.9 fF at α = 3 in 18 steps. Stated limits: the design is not unique (α = 4 reaches another), α was selected post hoc, and on the once-refined mesh the design sits 25.9 % below the anchor, so it is not a device-design result. `[PENDING issue-1051]` and `[PENDING issue-1037]` mark what is outstanding. |
| Lower-bound wording. | "At least about 6.26×" (computed 6.256959) rounded a lower bound up. It is now "at least 6.25×", floored, with the computed value shown. 26.3 %, 25.9 % and 36.34 fF are floored as well. |
| Cap authorization. | "Pending operator confirmation" replaced by the recorded authorization in `BRIEF.md`, this file, `_progress.json` and the PR body. `max_iterations` 7 → 12. |
| TM-guard work (#955/#1005/#1032/#1041). | Not mentioned in v7; left alone. |

Figures 3 and 6 are regenerated from the committed TOML and Palace CSV with
the corrected labels (Figure 3 now also shows the qubit mode in both
solvers). Figure 2 is unchanged; its caption says which six modes it shows.
`verify_evidence.py` gains assertions for every new number and for the
absence of the withdrawn wording. The committed benchmark artifacts still
use the old keys (`[spurious_mode]`, `excluded = true`, `junction_lc`,
`spurious_port`); re-baselining them is #1045 and is not part of this PR.
All six `TODO(operator)` lines are unchanged.

## In-place corrections on PR #1040, pass 3 (2026-10-11)

`main` gained two results that the pass above could only mark as pending:
the C_Σ convergence study (#1051, PR #1057) and the refined real-device
optimization (#1056, PR #1058). `origin/main` (`919a739d`) was merged and v7
was corrected in place again. Where this section and the one above disagree,
this one is current. v1–v6 and their critics remain byte-identical.

| Finding on `main` | What changed in v7 |
|---|---|
| The converged scalar-ε C_Σ of the unmodified device is below 83.02 fF (rigorous), estimated at 74.14–79.62 fF (#1051). | **New §7 convergence study** with Table 3 and Figure 5a: P1 at h, h/2, h/4 (8.5M tets, AMG-PCG) and P2 at h, h/2. Rigorous bound, estimate and scope ("converged" means the committed polygonal geometry) are kept apart; the band is stated as not asymptotic and not a bound. The tensor-ε series and the sign change of the scalar-versus-tensor difference are reported, replacing an inference. Every "being measured" statement and `[PENDING issue-1051]` are gone. |
| The 137.7 vs ~90 fF gap was discretization error (same study). | **§7 diagnosis rewritten.** The coarse mesh overstates C_Σ by at least 54.69 fF, more than the whole 47.8 fF gap. Converged, X⁰ is at least 7.6 % *below* the anchor. The residual gap has the opposite sign; the 5.5 fF junction self-capacitance is outside this C_Σ and its role is stated as an inference. Pass 2 had only withdrawn the "model difference" diagnosis; this pass replaces it with the measurement. |
| Refined optimization (#1056): adaptive refinement, P2 equilibrium-energy shape gradient, three accepted steps, seven held-out rounds. | **New §9.4, the real-device result**, with Table 5 and Figure 5b. Primary run: 78.387918 → 89.900518 fF on 448,279 tets (an increase), FD worst 3.87e-7. Held-out series to 89.014998 fF at 3,289,543 tets. Rigorous: below 89.02 fF. Estimate: about 88.8–88.9 fF, slower-rate alternatives disclosed. The 1 % threshold is not claimed. Design increment about 11.5 fF, changing by less than 1 %. Stated: the numerical path is native f64, outside the Burn tape. |
| Resolution-corrected design, and the PR #1058 judge's caveat (#1062). | Reported with the caveat in bold: the series is the first one rescaled (ratio constant to a relative spread of 8.1e-5, computed by the verifier), so the 89.84–89.96 fF band contains the anchor **by construction**; its width is the spread between methods, not an uncertainty; only the upper bound (below 90.08 fF, at most +0.20 %) is proven; at slower rates the design lands 0.25–0.57 % below. A two-sided bound is roadmap. The caveat also sits in the abstract, the Figure 5 caption, the discussion and the conclusion. |
| Palace P2 on the adapted meshes agrees to 2.5e-6; Palace P3 was killed. | Reported as solver agreement on the same discretization, "not evidence about the discretization error". P3 is stated as not evidence. |
| Coarse-mesh Phase A/B (#1035/#1036). | §9.3 shrinks from a "methods result" to **a cautionary precursor**: every self-consistency check passed and the optimizer moved the wrong way, lowering C_Σ. Converged, that design is below 52.37 fF, at least 41.7 % below the anchor. The stalled variants, the tensor retarget and the per-design dimensions are dropped from the text (they stay in the artifact). |
| Coarse-mesh shortfall (#589/#594). | §9.2 compresses the rigid-scale and harmonic discussion into one historical passage and says the "anchor shortfall" was an artifact of an unconverged mesh. The 6.25× figure leaves the abstract and the contributions. **Wording reconciled:** the manuscript uses "at least 6.25×" (floored; computed 6.256959 shown) everywhere it appears; the committed `multiparam_gradient.toml` comment and the #1034 thread say "at least about 6.26×". `provenance.md` records both. |
| Phase C (#1037). | Ruled and not yet built: the operator ruled on it on 2026-10-10 (cut = the claw plus the resonator trace; hold β = C_g/C_Σ at its θ = 0 value while C_Σ goes to 89.9 fF; add a claw-gap parameter, giving 4 parameters and 2 targets). The issue is open and sequenced after #1036; no result depends on it. `[PENDING issue-1037]` becomes a roadmap paragraph in §9.4. The manuscript has no pending marker. |
| Abstract, contributions, discussion, limitations, conclusion. | Rewritten around the above: a refinement-validated real-device optimization, a rigorous upper bound plus an extrapolated estimate, no continuum certification, and the convergence finding as a result of its own. §9 is retitled. |
| §4.1 mesh sentence. | One sentence that had become false ("one uniform refinement for the C_Σ check") now lists the uniform and adaptive refinements. The rest of §4.1, and all of §6, are untouched; nothing is said about the TM-guard. |

New Figure 5 (`figures/src/fig7_convergence.py`, output
`figures/fig7-convergence.pdf`) is generated from the three committed TOMLs.
Figures 1–4 and the CPU figure are byte-identical; the optimization figure
(file `fig5-diffopt.pdf`) is unchanged and its caption now marks panel (b) as
historical. Two new tables and one new figure renumber the later ones: the
CPU table is Table 6, the AMS table Table 7, the CPU figure Figure 7 (the
pass-2 notes above use the old numbers). `verify_evidence.py` recomputes the
new bounds, bands and percentages from the committed series with their
rounding direction, pins every occurrence of the headline values, and
asserts that no pending marker or "being measured" statement remains. The
citation set and `main.bbl` are unchanged. All six `TODO(operator)` lines are
unchanged.

## In-place corrections on PR #1040, pass 4 (2026-10-11)

Wording and scope fixes requested by the judge at `1cee07e1`. No number
changes its source, and no solver or generator was re-run. Where this
section and the ones above disagree, this one is current. v1–v6 and their
critics remain byte-identical.

| Judge finding | What changed in v7 |
|---|---|
| The abstract, contributions and conclusion dropped the scalar-permittivity scope. | All three now name the model. 83.02 fF, 54.69 fF and "at least 7.6 % below the anchor" are scalar-ε. The tensor-ε figures stand beside them: converged below 89.50 fF, at least 0.4 % below the anchor (0.44 % floored), coarse overstatement at least 47.18 fF. The abstract's headline sentence says the optimized C_Σ is the scalar-ε electrostatic value without the junction's 5.5 fF. |
| The Burn / native-f64 disclosure was missing where the headline is claimed. | The abstract, the third contribution and the conclusion now say the headline gradient is a hand-derived equilibrium-energy shape derivative, solved by AMG-preconditioned CG in native f64, outside the Burn tape and without a factorization. The inherited §3 sentence ("the gradients in this paper … the same assembly and the same factorization") is scoped to the adjoint layer and the coarse-mesh tiers, and a sentence says in what narrower sense the refined run is solver-derived. |
| #1037 status was stale. | Changelog, provenance, `_progress.json` and BRIEF now say: ruled by the operator on 2026-10-10, not yet built. The manuscript roadmap paragraph was already accurate and is unchanged. |
| Figure 6 panel (b) contradicted its caption. | `figures/src/fig5_diffopt.py` retitles the panel "Historical, coarse 133k-tet device mesh: anchor shortfall was a mesh artifact" and the annotation ends "a coarse-mesh artifact". `fig5-diffopt.pdf` is regenerated; the plotted data are unchanged. |
| Verifier gaps (8 surviving mutations). | `verify_evidence.py` pins each occurrence: all six "by construction" copies in context, Table 2 row by row from the artifact, and the counts of 3.452749, 3.452751, 0.99424, 0.70810, 183,891, 4,913, 6,014 and "about 17 %". 33.384 and 33.896 GHz are now read from the artifact's mode list. |
| "constant to within 8.1e-5". | Now "a relative spread of 8.1e-5 (largest ratio over smallest, minus one)". The value is 8.06e-5; the absolute max − min is 8.15e-5. |
| Coarse-mesh FD errors rounded down. | 7.945e-7 and 1.141e-8 are now 8.0e-7 and 1.2e-8, rounded up like every other error in the paper. |
| "the driven solve that geode-fem's GPU path accelerates". | §8.1 now says the GPU path runs that solve correctly but slower than the CPU at every measured size, and points to §11.3. The opening of §11.3 says "runs" for the same reason. |
| Acknowledgments said every number traces to a committed artifact. | The sentence now matches the abstract: results trace to committed artifacts, and statements resting on a scratch probe, a reading of Palace's source or inference are labelled. The two `TODO(operator)` comment lines under it are untouched; the wording is still the operator's to finalize. |

The title and all six `TODO(operator)` lines are unchanged. The citation set
and `main.bbl` are unchanged. The PDF is 36 pages (it was 35).
