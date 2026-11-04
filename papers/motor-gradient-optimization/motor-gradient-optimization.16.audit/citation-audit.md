# Citation audit: motor-gradient-optimization.16

Audited content: `motor-gradient-optimization.16/` at `463c7319` (unchanged at worktree HEAD `481e32c8`, branch `claude/motor-paper-v11`, PR #1226).

## Resolution

- 47 distinct keys are cited across `main.tex` and its `\input` child `optimization.tex` (tables carry no citations). `refs.bib` has 47 entries. Every key resolves, and every entry is cited.
- The BibTeX run (`main.blg`) shows 0 warnings. The rebuilt PDF has 0 undefined citations.
- **Crossref.** 43 of the 44 DOIs resolve on Crossref `works/{doi}`. The 44th, `ragonneau2022` (`10.48550/arXiv.2210.12018`), is a DataCite DOI. It resolves through doi.org (HTTP 200) and on OpenAlex, where the title, year and author match.
  - Compared with Crossref, title, year, volume, issue, pages, first author and author count all agree, with three cosmetic exceptions:
    - `mckay1979comparison`: Crossref drops the leading "A" from the title. The published title includes it, so keep the bib entry.
    - `park1992design`: Crossref's family field reads "Il-Han Park", a Crossref quirk.
    - `kuci2016shape`: pages `1` against Crossref's `1-1`.
  - No correction is needed.
- **Entries without a DOI.**
  - `arkkio1987`: the Aalto handle returns HTTP 200, and the author's on-disk metadata is in `citation-notes.md`.
  - `kraft1988`: the report scan was checked in `refs/citation-source-check-v9.md`.
  - `eaton2026octave`: the GNU manual URL returns HTTP 200.
- **Lookup hygiene.** Lookups went only to `api.crossref.org`, `api.openalex.org` and `doi.org`. The only header was `User-Agent: anvil-paper-audit/1.0`. No email address, `mailto` parameter or other user identifier was sent. The scripts and raw responses are in `auditor-scripts/` (`crossref.py`, `crossref.json`, `cmp.py`, `cmp.out`, `abstracts.py`, `abstracts.json`, `abstracts2.json`).

## Claim-support spot check

Verdict sources:
- **abstract**: the OpenAlex abstract retrieved in this audit, archived in `auditor-scripts/abstracts*.json`.
- **notes**: the author's on-disk source notes: `motor-gradient-optimization.16/citation-notes.md`, `refs/citation-source-check-v9.md` and `refs/yu2018-fulltext-check.md`. The Yu notes are the operator's reading of the non-public full text.
- **title**: title-level only. Where no source is on disk, these are recorded as "unverified — source not on disk" per the skill.

| Key | Resolved | Surrounding claim (main.tex / optimization.tex line) | Verdict | Notes |
|---|---|---|---|---|
| giles2000introduction | yes | Adjoint cost does not grow with parameters; FD needs one solve per parameter (main:41-45); "introduce the adjoint approach to design" (main:114-115) | unverified — source not on disk | No abstract on Crossref/OpenAlex. The use is title-level and textbook-standard. |
| martins2013review | yes | Same premise (main:45); "review finite differences, complex steps, AD and direct and adjoint methods in one framework" (main:112-114) | supports (abstract) | Abstract: unified chain-rule review of FD, complex step and symbolic differentiation, plus direct and adjoint methods. |
| zingg2008comparative | yes | Aerodynamic adjoint needed far fewer evaluations than a GA (main:49-51); GA needed 5 to 200 times as many evaluations (main:157-159) | supports (abstract) | "requires from 5 to 200 times as many function evaluations". |
| wiesheu2024spline | yes | Spline-based rotor/stator shape plus parameter optimization of a nonlinear PMSM; reproduction target (main:57, 90-94) | supports (abstract) | 2-D nonlinear IGA, geometric parameters and control-point shapes, gradient-based. The public snapshot link returns 200. |
| wiesheu2025combined | yes | Combined parameter and shape optimization with IGA (main:92-93) | supports (abstract) | |
| park1992design | yes | Adjoint variable method on FE equations of a saturated 2-D magnet design (main:99-101) | supports (abstract) | Quadrupole magnet in saturation, adjoint variable method on the FE algebraic equations. |
| elbechari2022adjoint | yes | Discrete adjoint for linear and nonlinear magnetostatics, geometric and physical variables, verified against FD (main:102-104) | supports (abstract) | "verification based on an analytical test case and using the finite-difference method". |
| gangl2015shape | yes | Rotor shape sensitivities, shape-Lagrangian formulation, smoothing the rotation pattern (main:105-106) | supports (abstract) | |
| kuci2017design | yes | Shape sensitivities from the Lie derivative (main:106-107) | unverified — source not on disk | No abstract retrievable; title-level ("…based on the Lie derivative"). |
| kuci2016shape | yes | …applied to an IPM machine (main:108-109) | supports (abstract) | |
| scotzniovsky2025 | yes | FFD, differentiable mesh warping and adjoint derivatives (main:109-110) | supports (notes) | `citation-notes.md`: the publisher abstract names FFD, PDE-based mesh warping with exact derivatives and FE EM. No OpenAlex abstract. |
| merkel2021shape | yes | IGA shape derivatives with harmonic rotor–stator coupling (main:111-112) | supports (abstract) | |
| nikolova2004adjoint | yes | Review of adjoint-variable sensitivities for EM structure design (main:115-116) | supports (abstract) | |
| peter2010numerical | yes | Survey of sensitivity-analysis approaches for aerodynamic optimization (main:117-119) | unverified — source not on disk | Title-level; the title states exactly this. |
| kenway2019effective | yes | Adjoint practice for aerodynamic optimization (main:117-119) | unverified — source not on disk | Title-level ("Effective adjoint approaches for CFD"). |
| more2011estimating | yes | Show how to estimate computational noise (main:122-124); frozen step in the truncation-dominated regime (main:867-869) | supports (abstract) | ECnoise. |
| more2012estimating | yes | Use the noise to choose a near-optimal step (main:124) | supports (abstract) | |
| haftka1985sensitivity | yes | Modified procedure improves derivatives of iteratively solved problems (main:126-128) | supports (abstract) | |
| eyi2011finite | yes | Chooses the FD step from an estimate of the convergence error (main:129) | supports (abstract) | "optimum step size … function of … convergence error and higher order sensitivities". |
| adelman1986sensitivity | yes | Direct method, one linearized solve per parameter (main:130-131) | supports (abstract) | Survey of sensitivity-derivative methods for discrete structural systems. The direct-method cost is standard. |
| barthelemy1990accuracy | yes | Semi-analytic method can be inaccurate for shape variables (main:131-133) | supports (abstract) | "serious accuracy problems for shape design variables". |
| degersem2004harmonic | yes | Formulate harmonic weighting functions at the sliding interface (main:137) | supports (abstract) | The wording has been non-originating since the v5 audit. |
| bontinck2018isogeometric | yes | Harmonic interface constraints between IGA subdomains, stability analysis (main:138-139) | supports (notes) | `citation-notes.md` v4: arXiv §3, Fig. 7, inf-sup behavior. |
| egger2022torque | yes | Torque from a harmonic mortar formulation; transmitted order varied in a linear cogging experiment (main:140-142); coupling torque (main:226, 600) | supports (abstract + notes) | The abstract covers torque formulas and torque ripple. The notes (§5, Fig. 3) cover the linear test at orders 39–129. |
| arkkio1987 | yes (no DOI; handle 200) | Gap-averaged (Arkkio) torque (main:143, 224, 568) | supports (notes, metadata) | Thesis record in `citation-notes.md`; standard attribution. |
| howe1992cogging | yes | Discretization effects on torque (main:144-145) | supports (abstract) | |
| arkkio2012proper | yes | Same (main:144-145) | supports (abstract) | |
| duan2013review | yes | RS against DE on a five-variable PMSM (main:147-149) | supports (abstract) | |
| bramerdorfer2018modern | yes | Survey of machine-optimization practice (main:150-151) | supports (abstract) | |
| lei2017review | yes | Review of design-optimization methods (main:151-152) | supports (abstract) | |
| ion2018robust | yes | Compare deterministic gradient-based and stochastic search for electric devices (main:152-153) | supports (abstract) | Advantages over PSO. |
| gangl2022multi | yes | Minutes (gradient free-form) against hours (stochastic parametric, JMAG), similar shapes; comparison also changes parametrization and solver (main:153-157) | supports (abstract) | "in the order of minutes, compared to several hours"; "of similar shape". |
| **yu2018influence** | yes | "On an eight-variable wing twist problem, … found gradient-free optimizers 4 to over 100 times as expensive as adjoint-driven ones **at the same optimum**" (main:160-162); no adjoint–FD comparison, but they state the scaling premise (main:163-165) | **partial** (notes) | The operator's full-text notes say all optimizers **except NSGA2** reach the same optimum. The 4× end is NOMAD and the >100× end is NSGA2. "At the same optimum" therefore holds for the 4× end but not for the >100× end. The no-FD-arm and scaling-premise statements are supported. See flags.md N2. |
| dolan2002benchmarking | yes | Time-to-target follows performance profiles (main:166-167; optimization:31-32) | supports (notes) | `refs/citation-source-check-v9.md`. The v9 "derivative-free" misattribution is fixed: main:166-168 now splits the two attributions. |
| more2009benchmarking | yes | …and data profiles, with a budget and censored targets (main:167-169; optimization:31-32) | supports (notes) | Same source check. |
| geuzaine2009gmsh | yes | Mesh generator built on Gmsh 4.15.2 (main:195) | supports (software) | The version appears in `benchmarks/motor_reference/geometry/README.md` at `910593ba`. |
| kemmetmueller2015 | yes | Nonlinear magnetic equivalent circuit as an untested reduced model (main:415-416) | supports (notes) | The postprint notes cover saturation in the MEC. Title: "…using magnetic equivalent circuits". |
| roomi2022 | yes | Waveform-targeting surrogate (main:416) | supports (abstract) | |
| wen2023 | yes | Adaptive reduced model (main:417) | supports (title) | On-the-fly hyperreduction for shape optimization; the title-level use matches the title. |
| mckay1979comparison | yes | Latin-hypercube starts (main:644) | supports | Standard attribution. |
| harris2020array | yes | NumPy (main:645) | supports (software) | |
| virtanen2020scipy | yes | SciPy (main:646; optimization:19) | supports (software) | |
| kraft1988 | yes (no DOI) | SLSQP (optimization:15) | supports (notes) | `refs/citation-source-check-v9.md`. |
| ragonneau2022 | yes (DataCite) | COBYQA (optimization:17) | supports (notes) | Same. The thesis introduces COBYQA. |
| eaton2026octave | yes (URL 200) | GNU Octave 11.3.0 (main:1042) | supports (software) | |
| defalco2011geopdes | yes | GeoPDEs (main:1042-1043) | supports (software) | |
| vazquez2016geopdes | yes | GeoPDEs 3.0 (main:1043) | supports (software) | |

## Summary

- Unresolved keys: **0**. Claim-support failures (`does-not-support`): **0**.
- Partial: **1** (`yu2018influence`, non-critical note N2).
- Unverified, source not on disk (title-level only, recorded but not flagged): **4** (`giles2000introduction`, `kuci2017design`, `peter2010numerical`, `kenway2019effective`). The paper uses each only in its title-level sense.
