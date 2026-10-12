# Litsearch re-run: transmon-benchmark.9.litsearch (for v10)

**Trigger.** Issue #1093 (squad goal #6). The two independent v9 reviews posted on
#1063 (Fable 24/44, Opus 22/44) both flagged misstated prior art as a must-fix.
They named their leads from memory, with web search off, so each lead had to be
verified before anyone cites it.

**Directory name.** The issue asked for a `.10.litsearch` sibling. Anvil's reviser
reads `<thread>.{N}.litsearch/` at the latest version N (`paper-revise.md`, Reads),
and `paper-litsearch.md` names a re-run after revision N `<thread>.{N}.litsearch/`.
The pass that writes v10 therefore reads `.9.litsearch`, and this sibling uses that
name. No version directory and no `refs.bib` was changed.

**Method.** This run used live web search, which `BRIEF.md` already enables
(`web_search: true`). It covered four axes:
- the reviewers' named leads;
- adjoint and shape sensitivity for frequency-domain FEM EM;
- certified two-sided capacitance bounds via equilibrated flux;
- gradient-based design in superconducting-qubit EM.

Every DOI or arXiv entry in `candidates.bib` was produced by
`anvil/lib/cite.py::resolve()` from an identifier a search hit surfaced, then
emitted by its formatter. **All 49 resolved.** Three URL-kind documentation
sources (HFSS, COMSOL) are kept separately at the end of the file, because they
have no DOI. Each was fetched directly, and the supporting quote is recorded
below. `candidates.bib` passes `bib_lint.lint_bib_file` with no issues.

## Positioning summary

**Cluster 1: commercial FEM EM solvers do expose design derivatives.**
- **HFSS.** Ansys documents analytic S-parameter derivatives with respect to
  geometric and material parameters (`ansys_hfss_derivatives`). The derivatives
  are computed "analytically for the most part in HFSS", and the forward LU
  factors are reused: "Once the LU factors have been obtained, it is very
  efficient to solve additional right-hand side vectors". The help page cites
  `vardapetyan2008sensitivity` (Vardapetyan, Manges & Cendes, IMS 2008), the
  transfinite-element S-parameter sensitivity paper including port variations.
  The page never uses the word "adjoint"; it describes a direct (forward)
  sensitivity solve. So the reviewer's lead "HFSS adjoint design derivatives" is
  wrong on the word, but right that HFSS has a derivative path.
- **COMSOL.** COMSOL documents both forward and adjoint sensitivity. The adjoint
  method "solves for the derivatives of a single scalar objective function"
  "requiring only one single additional linear system solution"
  (`comsol_sensitivity_method`). A COMSOL blog post shows gradient-based shape
  optimization of a waveguide filter and a diplexer with S-parameter objectives
  on a deformed mesh (`jensen2022comsolshape`).
- **Consequence.** v9's sentence that HFSS and COMSOL have "no derivative path,
  and none exposes an adjoint of its solve" is false for COMSOL and false in
  substance for HFSS. What remains true: neither is open, and neither gives
  derivatives of an eigenmode/LOM chain through an autodiff-composable assembly.
  We found no documentation of HFSS eigenmode derivatives.

**Cluster 2: adjoint and design sensitivity for frequency-domain FEM EM is a
25-year-old literature.**
- **Review.** `nikolova2004adjoint` (Nikolova, Bandler & Bakr, IEEE TMTT 2004)
  reviews adjoint-variable methods across FEM, MoM, TLM and FDTD.
- **Webb line.** On 3-D tetrahedral edge elements specifically, the Webb line
  derives S-matrix design sensitivities with ports: `webb2000design` (published
  byline Akel & Webb, see the caveat below), `webb2001design` (high-order vector
  elements) and `webb2002design` (frequency response). It then optimizes
  microwave devices with them (`nair2003optimization`).
- **Further S-parameter and material work.** Self-adjoint S-parameter
  sensitivities on commercial FEM field solutions are in `nikolova2006network`
  and `nikolova2009sparameter`. Continuum shape sensitivity for high-frequency
  devices is in `choi2011generalized`. Adjoint sensitivity for uncertain
  materials in 3-D frequency-domain FEM is in `harmon2021adjoint`. Adjoint
  density topology optimization of waveguide filters is in
  `aage2017topology`.
- **Cavity eigenvalues.** Shape adjoints for Maxwell cavity eigenvalues exist:
  - the SLAC/CMU accelerator-cavity work, `akcelik2005adjoint` (adjoint shape
    optimization of the ILC cavity) and `akcelik2008shape` (shape
    determination, an inverse problem, J. Comput. Phys. 2008), plus the
    follow-up `akcelik2009large`;
  - the analysis paper `lamberti2021shape`;
  - most recently `herter2026numerical`, mixed Nédélec/Lagrange FEM with
    hand-derived adjoint eigenvalue shape gradients and BFGS;
  - `ziegler2023mode`.
- **Open-source automated shape differentiation.** This exists for FEM in general
  in UFL/Firedrake (`ham2019automated`, `paganini2021fireshape`) and NGSolve
  (`gangl2021fully`). We did not confirm a Maxwell/H(curl) example in those
  three papers.
- **What the paper adds (ingredient-level).** Discrete-adjoint shape *and*
  material gradients through an autodiff-capable tensor assembly (forward-mode
  duals in the element kernels), validated by FD and mutation tests, applied to
  the electrostatic → LOM → E_C chain of a superconducting qubit. Shape adjoints
  for frequency-domain FEM EM, and S-parameter/eigenvalue sensitivities with
  Nédélec elements, are prior art and must be cited as such.

**Cluster 3: capacitance shape gradients and the "vanishing adjoint".**
- **Prior art.** Adjoint capacitance sensitivities are standard in IC extraction
  (`elmoselhy2008efficient`: on-chip capacitance sensitivities to many
  parameters from one BEM extraction) and in FEM (`ren2012computation`, adjoint
  second-order capacitance sensitivity). `park2019continuum` gives continuum
  shape design sensitivity for electrostatic systems.
- **What is not new.** The energy-stationarity identity in v9 §(chain), that the
  shape derivative of C = φᵀKφ is the explicit term φᵀ(∂K/∂X)φ, is the standard
  self-adjoint result for an energy functional. It is not a new simplification.
  Cite it as known and keep the numerical confirmation (implicit part below
  10⁻⁸) as a check.

**Cluster 4: gradient-based design in superconducting-qubit EM.**
- `eun2023shape` (J. Phys. D 2023) shape-optimizes transmon pads and junction
  wires with FEM and a global, derivative-free optimizer. The authors note that
  the adjoint route needs a sensitivity analysis.
- `ni2022integrating` and `wang2025supergrad` (SuperGrad, Quantum 2025)
  differentiate at the Hamiltonian/circuit level, like the SQcircuit line
  already cited.
- `wu2025quantum` (a quantum design automation review, arXiv 2025) frames
  repeated EM simulation as the bottleneck.
- No search found an adjoint field-solve gradient used for transmon layout.
  Within superconducting-qubit layout, the wedge the paper claims survives. It
  is narrow, and the paper must say "to our knowledge, within qubit EM design",
  not "uncontested".

**Cluster 5: certified two-sided capacitance brackets.**
- **Method attributions.** The certificate's methods are now citable:
  - `prager1947approximations` (hypercircle);
  - `raviart1977mixed` (RT spaces);
  - `braess2007equilibrated` (Math. Comp. 77(262), issue dated 2008, online
    2007, for edge elements);
  - `braess2009equilibrated` (p-robust patch equilibration, the closest template
    for the RT flux build);
  - `destuynder1999explicit`, `vejchodsky2006guaranteed` and `ern2015polynomial`
    (guaranteed bounds from equilibrated flux).
- **Closest prior art.** Bracketing capacitance between dual or complementary
  energy principles is classical:
  - the analytic variational bounds of `polya1951isoperimetric`;
  - TEM-line capacitance and impedance bounds, `carson1966upper` and
    `daly1984upper`;
  - `hammond1976calculation` (Proc. IEE 1976, C and L by dual energy
    principles);
  - `penman1983dual` and `penman1983complementary` (complementary FEM for
    electrostatics);
  - most directly `specogna2011complementary` (IJNME 2011). Specogna gives 3-D
    complementary formulations that "provide the upper and lower bounds" for
    capacitance, using a cohomology-based vector potential rather than
    equilibrated RT fluxes;
  - follow-ups `ren2009complementarity`, `xu2016interconnect` (IC interconnect
    capacitance from dual formulations) and `vacalebre2025exploring`.
- **Goal-oriented output bounds.** `paraschivoiu1997posteriori`,
  `sauerbudge2004computing`, `mozolevski2015goal`, `pares2021posteriori` (two-sided
  output bounds from potential plus equilibrated flux) and `tang2021guaranteed`
  (an EM quantity of interest).
- **What is new.** The combination: a P2 conforming upper bound with a
  patch-equilibrated RT lower bound via Thomson's principle, for a capacitance
  with a floating conductor (the Schur-reduced feedline), on a production qubit
  geometry, as a design-acceptance test. We found no prior treatment of the
  floating conductor inside a bounding framework, though we did not read the
  Specogna or Ren full texts. We found no certified capacitance bound in
  superconducting-qubit design; that literature uses heuristic
  energy-convergence criteria.

**Cluster 6: prior independent checks of Palace (already cited, reposition).**
- `sommers2025open` (SQDMetal, arXiv:2511.01220) validates its Palace workflow
  with mesh-convergence studies against COMSOL and Ansys for eigenmode and
  capacitance, and compares with experiment.
- `ye2025electromagnetic` (arXiv:2511.09041) benchmarks a Palace workflow
  against cryogenic measurement (resonators within 0.3 %).
- Both are independent third-party validations of Palace that predate this
  paper. "The first independent third-party validation of the
  Palace/DeviceLayout.jl transmon stack" can stand only if narrowed to the
  DeviceLayout.jl SingleTransmon *example*, and to *same-mesh code-to-code*
  comparison against an independently implemented solver. The safer course,
  which both reviewers recommend, is to drop "first" and "validation".

## Claim changes required in v10 (quoting v9 `main.tex`)

1. **L134–137:** "is structurally unavailable from the production solvers ---
   commercial (HFSS, COMSOL) and open-source (Palace \citep{palace}) alike: their
   assembly and factorization pipelines are hand-written kernels with no
   derivative path, and none exposes an adjoint of its solve."
   - **Change to:** HFSS exposes analytic S-parameter derivatives reusing the LU
     factors (`ansys_hfss_derivatives`, `vardapetyan2008sensitivity`). COMSOL
     exposes forward and adjoint sensitivities and RF shape optimization
     (`comsol_sensitivity_method`, `jensen2022comsolshape`). Palace exposes
     none; a 2026 GitHub issue proposes only ROM-based parameter sweeps.
   - **What stays:** what the qubit loop lacks is open, composable gradients of
     the eigenmode/LOM observables. The state of practice still iterates EM
     solves (QDesignOptimizer).
2. **L153–158:** "What none of these supplies is ... frequency-domain finite
   elements on body-fitted tetrahedra with anisotropic materials --- distributed,
   field-level EM gradients from the same FEM solve ... That wedge is, to our
   knowledge, uncontested; this paper occupies it."
   - **Wrong as written:** see Cluster 2 (`webb2000design`,
     `nikolova2004adjoint`, `akcelik2008shape`, `herter2026numerical`).
   - **Change to:** delete "uncontested". Name the prior FEM EM adjoint line,
     and claim only the composable tensor-assembly derivative plus its
     application to the qubit LOM chain.
3. **L322–324:** "To our knowledge, no prior work delivers field-level design
   gradients from a frequency-domain 3D FEM electromagnetics solve of the class
   RF/superconducting design uses." **False.** Replace with a paragraph citing
   the Cluster 2 entries.
4. **L196–205 (contribution 1):** "A validated 2×2 discrete-adjoint sensitivity
   matrix for FEM electromagnetics". Keep it, but frame it as an implementation
   and validation contribution (FD and mutation-tested, single factorization),
   not a method contribution.
5. **L206–212 and L1305–1316 (the "vanishing adjoint", "an elegant
   simplification", "Hellmann--Feynman-like result").** Attribute the
   energy-stationarity shape derivative as known (`park2019continuum`,
   `ren2012computation`, `elmoselhy2008efficient`). Drop "elegant
   simplification" as a contribution.
6. **L1105–1108:** "equilibrated patch by patch into Raviart--Thomas fluxes of
   order one to three (Prager--Synge, Braess--Sch\"oberl)". Add
   `\citep{prager1947approximations,raviart1977mixed,braess2007equilibrated,braess2009equilibrated}`.
   In the certificate paragraph and in related work, cite the complementary
   capacitance-bound lineage: `hammond1976calculation`,
   `specogna2011complementary`, `pares2021posteriori`. State that the bracket
   *method* is classical and the floating-conductor qubit application is the new
   part.
7. **L188–192 and L258–260:** "the first independent third-party validation of
   the Palace/DeviceLayout.jl transmon stack, which has no publication of its
   own", and "doubling as the first independent validation of the
   Palace/DeviceLayout.jl stack".
   - **Change:** remove "first" and "validation" (SQDMetal and Ye validate
     Palace).
   - **If any version is kept:** say "a same-mesh code-to-code cross-check of
     the DeviceLayout.jl SingleTransmon example". L356–360 already has the right
     framing; promote it.
8. **Related work, new paragraphs:**
   - adjoint FEM EM (Cluster 2);
   - capacitance sensitivity (Cluster 3);
   - qubit shape optimization (`eun2023shape`, cited next to
     QDesignOptimizer);
   - complementary capacitance bounds (Cluster 5).

## Reviewer leads: verdicts

| Lead (as the reviewer gave it) | Verdict | Entry / reason |
|---|---|---|
| HFSS adjoint design derivatives | **Verified with correction** | HFSS has analytic *direct* S-parameter derivatives, not documented as adjoint. `ansys_hfss_derivatives`, `vardapetyan2008sensitivity` |
| COMSOL Optimization Module adjoint RF sensitivities | **Verified (general adjoint) / partly** | `comsol_sensitivity_method`; RF shape optimization in `jensen2022comsolshape`, which does not use the word adjoint (it reports 52 linear solves for 52 objectives, consistent with one adjoint solve per objective) |
| Nikolova, Bandler & Bakr 2004 | **Verified** | `nikolova2004adjoint`, IEEE TMTT 52(1):403–419, 10.1109/TMTT.2003.820905 (one McMaster record wrongly lists it as TAP) |
| Akçelik et al. 2008 (Nédélec cavity shape adjoint) | **Verified with correction** | `akcelik2008shape`, J. Comput. Phys. 227(3):1722–1738 is an inverse *shape determination* (Ko, Lee, Li, Ng, Xiao; no Biros or Ghattas). The *optimization* paper with Biros and Ghattas is `akcelik2005adjoint` (J. Phys. Conf. Ser. 16, 2005). Nédélec elements not confirmed from the abstracts |
| Prager–Synge | **Verified** | `prager1947approximations`, Quart. Appl. Math. 5(3):241–269 |
| Braess–Schöberl | **Verified** | `braess2007equilibrated`, Math. Comp. 77(262):651–672 (Crossref year 2007, issue 2008); also `braess2009equilibrated` |
| Raviart–Thomas | **Verified** | `raviart1977mixed`, LNM 606:292–315 |
| SQDMetal (reposition) | **Verified, already cited** | `sommers2025open`, arXiv:2511.01220 (no journal version listed). Validates Palace against COMSOL/Ansys |
| Ye (reposition) | **Verified, already cited** | `ye2025electromagnetic`, arXiv:2511.09041 (no journal version listed). Validates Palace against measurement |

**Count.** 9 reviewer leads. 9 verified: 4 as stated, 4 with corrections
(HFSS, COMSOL, Akçelik, Braess–Schöberl year), and 2 of the 9 were already cited.
None was rejected outright. "HFSS **adjoint**" is rejected *as worded*: the
correct statement is "HFSS derivatives".

## Rejected or unverified items

| Item | Reason |
|---|---|
| "HFSS exposes an adjoint" (wording) | The official help page describes a direct sensitivity solve; "adjoint" does not appear. Cite as derivatives |
| DOI 10.1049/piee.1976.0130 | Resolves to an unrelated fibre-optics paper (Inoue et al.); the correct Hammond–Penman DOI is .0125 |
| DOI 10.1109/TMAG.1982.1061852 | Resolves to Penman & Kamar (saturable fields), not Penman & Fraser; the correct DOI is 10.1109/TMAG.1982.1061883 (resolved, not carried) |
| Hammond & Penman 1976 end page | Crossref gives only the start page, 554; do not print an end page without checking |
| Bossavit, *Computational Electromagnetism* (1998) | No DOI or ISBN resolved; lead only |
| dolfin-adjoint / pyadjoint Maxwell example | None found; no entry |
| Palace adjoint | None. awslabs/palace issue #978 (2026-09-25) proposes ROM boundary-operator sensitivities, not an adjoint |
| Nédélec use in the Akçelik papers | Not confirmed from abstracts; do not write "Nédélec" next to the citation without checking the full text |

**Considered and not carried (redundant with carried entries, resolver OK):**
- Penman & Fraser 1982 (10.1109/TMAG.1982.1061883);
- Hammond & Penman 1978; Hammond & Baldomir 1988;
- Bossavit 1992/2003/2007;
- Ainsworth 2010; Ainsworth & Oden 2000; Ainsworth & Vejchodský 2011;
- Repin 2008; Ladevèze & Leguillon 1983;
- Kim, Sykulski & Lowther 2007; Park, Lee & Hahn 1991; Koh et al. 1993;
- Dyck & Lowther 1996; Hiptmair, Paganini & Sargheini 2015.

## Confirmed coverage (already in v9 `refs.bib`)

QDesignOptimizer, the SQcircuit line, SQuADDS, Qiskit Metal/EPR, the photonics AD
stack (ceviche, Meep adjoint, FDTDX, TorchGDM, meent), JAX-FEM, PyTorch-FEA,
TensorGalerkin and torch-sla, the FEM code-generation lineage, V&V, SQDMetal,
and Ye.

## Identified gaps

- **Akel/Webb byline.** Crossref lists `webb2000design` as "Webb, J.P.; Akel,
  H.", while citing works give "H. Akel and J. P. Webb". Check IEEE Xplore and
  fix the author order before citing.
- **Specogna 2011 and Ren 2009/2016.** Read the full texts to confirm whether
  they handle floating conductors. If they do, the "new" claim narrows further.
- **Tang et al. 2021.** Check whether its quantity of interest is a capacitance.
- **Peer-reviewed HFSS/COMSOL eigenmode derivatives.** If a citation is needed
  for HFSS eigenmode derivatives, none was found.

## Re-run delta (vs `.4.litsearch`)

The `.4` run positioned the differentiable reframe against photonics AD and the
qubit-design loop. It did not search the CEM adjoint literature or the
complementary-bounds literature, and it did not check the commercial-solver
derivative features. This run closes those three gaps. It also corrects the
"first independent validation" positioning using entries already on disk.

## Web provenance

| bib key | identifier | resolver |
|---|---|---|
| vardapetyan2008sensitivity | 10.1109/MWSYM.2008.4633219 | crossref |
| nikolova2004adjoint | 10.1109/TMTT.2003.820905 | crossref |
| webb2000design | 10.1109/20.877620 | crossref |
| webb2001design | 10.1109/20.952671 | crossref |
| webb2002design | 10.1109/20.996284 | crossref |
| nair2003optimization | 10.1109/TMAG.2003.810520 | crossref |
| nikolova2006network | 10.1109/TMTT.2005.862663 | crossref |
| nikolova2009sparameter | 10.1109/TMTT.2009.2020828 | crossref |
| harmon2021adjoint | 10.1109/TAP.2021.3070059 | crossref |
| choi2011generalized | 10.1109/TMAG.2010.2087009 | crossref |
| aage2017topology | 10.1002/nme.5551 | crossref |
| akcelik2005adjoint | 10.1088/1742-6596/16/1/059 | crossref |
| akcelik2008shape | 10.1016/j.jcp.2007.09.029 | crossref |
| akcelik2009large | 10.1088/1742-6596/180/1/012001 | crossref |
| lamberti2021shape | 10.1002/mma.7423 | crossref |
| herter2026numerical | 10.1080/10556788.2026.2651158 | crossref |
| ziegler2023mode | 10.1109/TAP.2023.3249907 | crossref |
| ham2019automated | 10.1007/s00158-019-02281-z | crossref |
| paganini2021fireshape | 10.1007/s00158-020-02813-y | crossref |
| gangl2021fully | 10.1007/s00158-020-02742-w | crossref |
| elmoselhy2008efficient | 10.1145/1391469.1391699 | crossref |
| ren2012computation | 10.1109/TMAG.2011.2172194 | crossref |
| park2019continuum | 10.1007/978-981-13-0230-5_3 | crossref |
| eun2023shape | 10.1088/1361-6463/acf7cf | crossref |
| ni2022integrating | 10.1038/s41534-022-00614-3 | crossref |
| wang2025supergrad | 10.22331/q-2025-04-24-1722 | crossref |
| wu2025quantum | arXiv:2511.10479 | arxiv |
| prager1947approximations | 10.1090/qam/25902 | crossref |
| raviart1977mixed | 10.1007/BFb0064470 | crossref |
| braess2007equilibrated | 10.1090/S0025-5718-07-02080-7 | crossref |
| braess2009equilibrated | 10.1016/j.cma.2008.12.010 | crossref |
| destuynder1999explicit | 10.1090/S0025-5718-99-01093-5 | crossref |
| vejchodsky2006guaranteed | 10.1093/imanum/dri043 | crossref |
| ern2015polynomial | 10.1137/130950100 | crossref |
| polya1951isoperimetric | 10.1515/9781400882663 | crossref |
| carson1966upper | 10.1109/TMTT.1966.1126312 | crossref |
| hammond1976calculation | 10.1049/piee.1976.0125 | crossref |
| penman1983dual | 10.1109/TMAG.1983.1062859 | crossref |
| penman1983complementary | 10.1109/TMAG.1983.1062907 | crossref |
| daly1984upper | 10.1108/eb009988 | crossref |
| ren2009complementarity | 10.1109/TMAG.2009.2012596 | crossref |
| specogna2011complementary | 10.1002/nme.3089 | crossref |
| xu2016interconnect | 10.1109/TVLSI.2015.2459043 | crossref |
| vacalebre2025exploring | 10.1109/TMAG.2024.3502700 | crossref |
| paraschivoiu1997posteriori | 10.1016/S0045-7825(97)00086-8 | crossref |
| sauerbudge2004computing | 10.1137/S0036142903425045 | crossref |
| mozolevski2015goal | 10.1016/j.cma.2014.09.025 | crossref |
| pares2021posteriori | 10.1016/j.cma.2021.114088 | crossref |
| tang2021guaranteed | 10.1109/TMAG.2021.3071641 | crossref |

**Keys renamed by hand** (resolver key, then the key used here):
- `nikolova2006sensitivity` → `nikolova2006network`;
- `nikolova2009s` → `nikolova2009sparameter`;
- `zhuoxiangren2009complementarity` → `ren2009complementarity`;
- `xu20163` → `xu2016interconnect`.

**Formatting fix:** `&amp;` → `\&` in container names.

**URL-kind documentation sources** (in `candidates.bib`, not resolver-verifiable;
fetched 2026-10-12):

| bib key | URL | supporting quote |
|---|---|---|
| ansys_hfss_derivatives | ansyshelp.ansys.com … OverviewoftheTechnicalApproachforDerivativesinHFSS.htm (v252) | "HFSS can compute the derivatives of S-parameters and related matrix quantities"; "The variables of differentiation could be geometric parameters, such as the length of a waveguide"; "Once the LU factors have been obtained, it is very efficient to solve additional right-hand side vectors" |
| comsol_sensitivity_method | doc.comsol.com/6.2/…/comsol_ref_sensitivity.31.07.html | "The adjoint method solves for the derivatives of a single scalar objective function"; "requiring only one single additional linear system solution" |
| jensen2022comsolshape | comsol.com/blogs/shape-optimization-in-electromagnetics-part-2 | waveguide filter and diplexer with S-parameter objectives; "evaluating the sensitivity requires solving 52 linear problems" |

## Web leads (unverified)

| title | authors | year | URL as found | reason unresolved |
|---|---|---|---|---|
| Computational Electromagnetism: Variational Formulations, Complementarity, Edge Elements | A. Bossavit | 1998 | (book; no DOI found) | no identifier extractable |
| COMSOL Optimization Module: The Optimization Solver | COMSOL AB | 2020 | https://doc.comsol.com/5.6/doc/com.comsol.help.opt/opt_ug_solver.8.21.html | url-kind unsupported in v0 (UnsupportedIdentifierError); reported by the search agent, not fetched by the lead author |
| COMSOL: Theory for Eigenvalue Sensitivity Analysis | COMSOL AB | 2025 | https://doc.comsol.com/6.4/doc/com.comsol.help.opt/opt_ug_theory.5.09.html | url-kind unsupported in v0; reported by the search agent ("referred to as the adjoint gradient method"), not fetched by the lead author |
| awslabs/palace issue #978 (ROM boundary-operator sensitivities) | simlapointe | 2026 | https://github.com/awslabs/palace/issues/978 | url-kind; context only |

## Search log (2026-10-12)

1. Read #1093, #1063 (v9 review notes and rulings), #659, v9 `main.tex`
   (L120–365, L1094–1112, L1296–1316) and the v9 `refs.bib` keys.
2. Reviewer leads: Crossref/web lookups for each name. Fetched the arXiv abstract
   pages for 2511.01220 and 2511.09041. Searched for any Palace publication
   (none beyond the AWS blog and docs).
3. Commercial derivatives: fetched the Ansys HFSS derivatives help page (v252),
   the COMSOL 6.2 "Choosing a Sensitivity Method" page and the COMSOL blog
   "Shape Optimization in Electromagnetics: Part 2".
4. Adjoint FEM EM axis (search agent): Nikolova/Bandler/Bakr, Webb/Akel/Nair,
   Akçelik/SLAC, Schöps group cavity shape optimization, continuum sensitivity
   (Kim, Park, Choi), Firedrake/NGSolve shape differentiation, Harmon/Notaroš,
   Aage. Also queried "differentiable FEM Maxwell", "pyadjoint Maxwell", "JAX-FEM
   electromagnetics" and "Palace adjoint" (all empty).
5. Qubit axis: "adjoint gradient-based optimization superconducting qubit layout
   capacitance shape" (extended), giving Eun 2023, SuperGrad, Ni 2022, and the
   QDA review. "differentiable capacitance extraction adjoint" gave El-Moselhy
   2008 and Ren 2012.
6. Certificate axis (search agent): Prager–Synge, Braess–Schöberl, RT,
   Destuynder–Métivet, Ern–Vohralík, Hammond–Penman, Penman–Fraser, Bossavit,
   Specogna, Ren, Daly, Carson–Cambrell, Paraschivoiu–Peraire–Patera,
   Sauer-Budge et al., Parés et al., Pólya–Szegő, plus "guaranteed capacitance
   bounds qubit" (nothing found).
7. Verification: `cite.py::resolve()` on all 49 identifiers (49 OK, 0 FAIL).
   Re-resolved the two guessed DOIs to confirm they are wrong.
   `bib_lint.lint_bib_file` reported no issues.
