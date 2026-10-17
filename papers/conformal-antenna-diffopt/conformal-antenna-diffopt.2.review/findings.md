# Findings — conformal-antenna-diffopt.2

Cross-section observations from the second review pass (v1 34/44 -> v2 38/44, advance).

## The revision closed the v1 gap along the intended axis

v1 blocked (34/44) on three fixable defects: an unsupported comparative title, thin single-demonstration evidence (D2=3), and citation/figure hygiene (D8=3, D6=2). v2 addresses all three, and notably chose the *harder-but-honest* path on the title tension: rather than retreat to a reachability-only title, the reviser **substantiated** the comparative "cannot reach" claim with a genuinely-measured Section 5 evaluation. That is the right call and it is executed with unusual discipline -- the measured axes are committed to the repository and every number traces.

## Artifact traceability (spot-verified)

- **conformal_results.toml** -- headline design numbers all match: worst_s11_db -5.506281 -> -1.206262e1 (paper -5.51 -> -12.06), reduction_factor 5.563060 (x5.56), rel_err 1.166263e-10 (1.17e-10), n_freeform_dofs 73, n_steps 6 / max_steps 600, terminal target_reached, per-frequency S11 (w=0.30/0.35/0.40 -> -12.06/-23.92/-14.42 dB; mags 0.2494/0.0637/0.1900), analytic=central dir-deriv 1.751288204e-1, n_factorizations_per_freq 1, worst_vol_ratio 0.572, max_nodal_disp 0.560, grad_norm 1.751e-1->8.987e-2, fresh_vs_optimizer_rel 0.0, mesh 6173/997/4875.
- **staircasing_results.json** -- table (N=20..160) and derived quantities all match: cell sizes, perimeter rel-err (0.1304/0.1539/0.1421/0.1421), area rel-err (0.1358/0.0033/0.0033/0.0015), boundary RMS (0.291/0.157/0.095/0.045), log-log slopes (area 1.955, boundary 0.880, perimeter -0.026), cells_across_feature_needed 6028.6 (~6029), blow-up 53492 (~5.35e4), feature width 24.11, radii 40.8/39.2, phi_max 0.3.
- **meep_runtime_scaling.json** -- table (R=4..12) and scaling all match: cells (10.3M..278.7M, exactly 161280*R^3), s/step (0.233..5.761), peak RAM (1.6..37.4 GB), dt (0.125..0.0417), fits ~R^2.92 / peak ~R^2.89 / forward ~R^3.92~=R^4; measured anchor R=8 >=421 steps / >=12 min / >=24 min adjoint; projections >=14 h @R8, >=70 h @R12, 60 GB @R14, 83 GB @R16, ~2.5e15 cells curve-faithful -- all carried through verbatim and **explicitly labelled as projections**.

## The measured/projected boundary is drawn correctly

The single most important honesty property for this paper -- that the intractability rest on measurement + labelled projection and NOT on a converged FDTD-density optimization that was deliberately not run -- holds. Section 5's honesty note ("We did *not* run a converged FDTD-density optimization"), the "(extrapolated, not measured)" projection heading, and the runtime figure's measured-vs-projected visual split all reinforce it. There is no place where a projected number is dressed as measured, and no fabricated comparative result. This is the reason no critical flag fires despite a categorical title.

## Residual weaknesses (non-blocking)

- **Single-fixture GEODE result.** The reachability demonstration is still one bend radius / one band with no ablation isolating the 73-DOF design choice; the paper concedes this. The comparative axes lift the *paper* above threshold, but the *GEODE* evidence itself is not yet a swept capability claim. This is the main reason D2 sits at 4/6 rather than higher.
- **Box-bounded "intractable."** The compute-axis wall is single-process Meep on one 61 GB instance; the categorical title verb slightly overreaches the (honestly-scoped) body. A D1/D9 nuance, not a flag.
- **Stale placeholder captions.** Three captions retain "(Placeholder --- rendered by paper-figures ...)" though the figures render -- a pure-mechanical cleanup that costs a D6 point.

## Citation hygiene resolved

The v1 load-bearing defect (a wang2011 entry whose title conflated two papers) is fixed to a single clean AIAA-Journal title + DOI 10.2514/1.J050594. All 9 \cite keys resolve 1:1 to complete refs.bib entries. The three named-but-uncited references (ceviche/Hughes 2019, Meep-adjoint docs, a canonical antenna-topology-opt reference) are handled honestly in-text and deferred to a litsearch pass -- surfaced as a related-work lead in comments.md, not a hygiene defect.

## Handoff

Advance to paper-audit (READY). The auditor should: (1) verify the wang2011 / ghassemi2013 / ham2020 characterizations against the actual papers (claim-support, this reviewer's out-of-scope); (2) run the render-gate once main.pdf exists; (3) fold the caption-placeholder cleanup and the commit-SHA addition into the audited version.
