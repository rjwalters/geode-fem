# Litsearch re-run: conformal-antenna-diffopt.4.litsearch (for v5)

**Trigger.** Issue #1093 (squad goal #6). The independent v4 re-review on #659
(Fable 20/44, Opus 19/44) asked for a litsearch re-run. Its leads came from the
reviewers' memory with web search off, so each one had to be verified before it
is cited:
- Akel & Webb 2000 and Webb 2002;
- Hassan, Wadbro & Berggren 2014;
- Nikolova et al. 2004;
- Dey–Mittra and Yu–Mittra conformal FDTD;
- the staircasing-convergence literature.

The earlier #659 gap list (ceviche, the Meep adjoint, an antenna
topology-optimization reference) was also re-checked.

**Directory name.** The issue asked for a `.5.litsearch` sibling. Anvil's reviser
reads `<thread>.{N}.litsearch/` at the latest version N (`paper-revise.md`,
Reads). The pass that writes v5 therefore reads `.4.litsearch`, and this sibling
uses that name. No version directory and no `refs.bib` was changed.

**Web-search knob.** This thread's `BRIEF.md` had `web_search: false`. The
operator's issue (#1093, acceptance item 1) explicitly asks for live web search,
so this PR sets `web_search: true` in `BRIEF.md`, the Anvil-sanctioned carrier.
That is the only change outside the litsearch sibling.

**Method.** Live web search on four axes:
- the reviewers' named leads;
- FEM shape and S-parameter sensitivity with ports;
- conformal FDTD for curved PEC, plus the staircasing-error literature;
- FDTD vs FEM cost-to-accuracy for antennas, plus density design of metallic
  antennas.

Every DOI entry in `candidates.bib` was produced by
`anvil/lib/cite.py::resolve()` from an identifier a search hit surfaced, then
emitted by its formatter. **All 39 carried DOIs resolved.** A 40th (FDTDX JOSS)
also resolved but is already in v4 as `mahlau2026fdtdx`, so it was dropped. The
HFSS and COMSOL documentation pages are kept separately at the end of the file,
because they are URL-kind. `candidates.bib` passes `bib_lint.lint_bib_file` with
no issues.

## Positioning summary

**Cluster 1: the structured-grid density method *has* been applied to metallic
antennas, on FDTD, with an adjoint.** This is the most important new prior art.
- **Hassan, Wadbro & Berggren 2014** (`hassan2014topology`, IEEE TAP
  62(5):2488–2500):
  - local conductivity values are the design variables;
  - the gradient comes from the FDTD discretization through an adjoint field
    problem ("only two FDTD simulations, one for the original antenna problem
    and another for the adjoint field problem");
  - the antennas are coax-fed;
  - they optimize the radiating patch of UWB monopole and *microstrip patch*
    antennas with "more than 20,000 design variables".
- **Their companion paper** (`hassan2014patch`, PIER B 59:89–102) designs the
  patch *and* ground plane of microstrip antennas the same way. A later paper
  does the same for near-field coupling (`hassan2015topology`).
- **Implications for v4.**
  - v4 cites only Erentok & Sigmund for conductor antennas, and v4 calls the
    density representation "a superb fit for topology optimization of dielectric
    photonics". Hassan et al. are the direct comparator: structured grid,
    metal, driven feed, adjoint, patch antenna.
  - What they do *not* do is represent a curved conductor boundary exactly or
    move it. Their designs live on the staircased FDTD grid. That ingredient
    still separates GEODE's node-motion shape adjoint from the density
    approach.
- **Structured-grid shape-parameter sensitivities also exist.**
  `nikolova2004sensitivity` (IEEE TMTT 52(4), FDTD on structured grids) and
  `nikolova2006scattering` (S-parameter sensitivities, including shape
  parameters, with time-domain simulators) give them.
  - The claim "no structured-grid AD framework provides shape derivatives of a
    curved metallic radiator through a driven-port objective" must be scoped.
    It holds for AD frameworks with *exact curved-boundary* representation; it
    fails for adjoint sensitivity on structured grids in general.
  - We found no peer-reviewed conformal-FDTD adjoint for curved-PEC *shape*. The
    nearest is `zhu2009accuracy` (accuracy of adjoint S-parameter shape
    sensitivities). That is the defensible gap.

**Cluster 2: structured grids do not have to staircase. Conformal FDTD is 30
years old and commercial.**
- **Development line:**
  - contour-path FDTD for curved surfaces (`jurgens1992finite`);
  - the locally conformal PEC algorithm of Dey & Mittra (`dey1997locally`);
  - its PEC curved-surface and edge extension (`yu2000conformal`);
  - a conformal FDTD package aimed squarely at *antennas and microstrip
    circuits* (`mittra2000conformal`);
  - the dielectric version (`yu2001conformal`);
  - a stability/accuracy comparison of conformal schemes
    (`railton1999analytical`).
- **Second order without staircasing.** `zagorodnov2003uniformly` (FIT/CST
  lineage) is uniformly stable conformal FDTD, second-order convergent for
  general 3-D PEC geometry, with no time-step reduction. `benkler2006new` and
  `benkler2008mastering` (SEMCAD) are conformal PEC with user-defined precision.
- **Tidy3D.** Its `PECConformal` documentation cites Dey–Mittra and Benkler (per
  the search agent; the v4 bib already cites the Tidy3D changelog).
- **Dielectric interfaces.** Subpixel smoothing (`farjadpour2006improving`, used
  in Meep) handles dielectric interfaces.
- **Implication.** Statements that a structured Yee grid *cannot* represent the
  curved conductor without a persistent bias are false as general statements.
  They are true only for plain staircased Yee grids, which is what the Meep
  density adjoint uses for metal.

**Cluster 3: staircasing error, and what does and does not converge.**
- **The literature:**
  - `cangellaris1991analysis` (stair-stepped PEC error, spurious surface
    waves);
  - `holland1993pitfalls` (practical staircase errors);
  - `ditkowski2001convergent` (Yee loses convergence at non-aligned
    interfaces; embedded-boundary fix);
  - `tornberg2008consistent` (staircasing is an inconsistent boundary
    condition);
  - `haggblad2014accuracy` (Numer. Math. 2014).
- **What Häggblad & Runborg prove:** staircasing "will produce O(1) errors" near
  the boundary, from evanescent waves; the error is first order away from it;
  the L2 rate falls to O(√h).
- **That is the citable form of v4's "does not converge" point.** It concerns
  *near-boundary fields*, which set the patch-edge current, and it is
  first-order convergent for far quantities.
- **The v4 perimeter metric is different.** Its +14 % plateau is a purely
  geometric fact: a staircase converges to the taxicab length of the curve, not
  its arc length. The #659 review noted this, and the same JSON's
  resonance-shift proxy converges. v5 should replace the perimeter argument with
  these citations, or with a field-level or resonance-level measurement.

**Cluster 4: FEM S-parameter shape sensitivity with ports is established, and
commercial.**
- **Hand-derived 3-D edge-element S-matrix design sensitivities exist:**
  - `webb2000design` (published byline Akel & Webb; Crossref lists Webb first);
  - `webb2002design` (frequency response);
  - their use in gradient optimization of microwave devices
    (`nair2003optimization`);
  - continuum shape sensitivity for high-frequency devices
    (`choi2011generalized`);
  - the review `nikolova2004adjoint` and the self-adjoint
    frequency-domain S-parameter method (`nikolova2006network`).
- **Commercial.** HFSS computes analytic S-parameter derivatives with respect to
  geometric and material parameters, reusing the LU factors
  (`ansys_hfss_derivatives`, `vardapetyan2008sensitivity`). It documents this as
  a direct method, not as an adjoint. COMSOL documents an adjoint sensitivity
  method (`comsol_sensitivity_method`) and demonstrates gradient-based shape
  optimization on a deformed mesh of a waveguide filter and a diplexer with
  S-parameter objectives (`jensen2022comsolshape`).
- **Automated shape differentiation.** This exists for FEM in general in
  UFL/Firedrake (`ham2019automated`, `paganini2021fireshape`) and NGSolve
  (`gangl2021fully`).
- **What still separates GEODE (ingredient-level):**
  - an *open radiator*: box-UPML, a lumped port and lossy FR-4. The Webb, Nair
    and COMSOL examples are closed waveguide devices;
  - 73 freeform boundary DOFs moved by a harmonic morph, against a few
    geometric parameters;
  - AD element kernels in a tensor stack with single-factorization reuse,
    against a hand-derived or symbolic shape derivative.
- **The novelty must be stated as this combination.** v4's prior-art paragraph
  must not omit the Webb/Nikolova/HFSS/COMSOL lineage, because that lineage is
  closer than the Wang, Ghassemi and Ham papers v4 does cite.

**Cluster 5: FDTD vs FEM cost-to-accuracy.**
- **No peer-reviewed cost-to-accuracy study was found** for antennas comparing
  FDTD and FEM. The multi-solver benchmarks report accuracy *spread* across
  commercial MoM/FEM/FDTD/FIT tools, not cost curves:
  - `vrancken2007benchmark` (cavity-backed aperture antennas);
  - `vasylchenko2007benchmarking` (six packages on microstrip antennas);
  - `mustra2007comparison` (a circularly polarized patch);
  - `vasylchenko2009quality` (planar antennas).
- **Consequence.** v5's compute axis cannot lean on the literature and must
  stand on its own measurement. The comparator must be the best structured-grid
  practice: conformal FDTD for the forward solve, or the Hassan-style density
  method for design. The 250 cells/mm projection is a staircase requirement. It
  does not bound a conformal-FDTD forward solve, which reaches second order at
  far coarser resolution (`zagorodnov2003uniformly`).

## Claim changes required in v5 (quoting v4 `main.tex`)

1. **Title, L21–22:** "A Curved Conformal Antenna Structured-Grid Inverse Design
   Cannot Reach".
   - **Problem:** Hassan et al. do structured-grid inverse design of metal
     microstrip patches, and conformal FDTD represents curved metal.
   - **Change:** drop "Cannot Reach" (the #659 review independently asked for
     this), or scope it to "staircased density".
2. **Abstract, L33–35:** "optimize a permittivity density field on a structured
   Yee/Cartesian grid, a representation that staircases curved metal". Add
   "plain (non-conformal)" or "staircased". Conformal FDTD does not staircase
   (`dey1997locally`, `zagorodnov2003uniformly`).
3. **Abstract, L53–54, and §5.1, L451–457:** "leaves a perimeter error that does
   not vanish but plateaus at ∼+14 %", and "A staircased boundary trades area
   accuracy for a persistent perimeter bias no amount of uniform refinement
   removes."
   - **Problem:** the plateau is the taxicab-length property of any staircase.
     It is not shown to be an EM error.
   - **Change:** cite `haggblad2014accuracy` and `cangellaris1991analysis` for
     the O(1) near-boundary field error, and `holland1993pitfalls`. Report a
     field-level or resonance metric.
4. **Intro, L80–85:** "This representation is a superb fit for topology
   optimization of dielectric photonics, but ... (i) it staircases curved
   metal". Cite `hassan2014topology` and `hassan2014patch` as the
   metallic-antenna density precedent, and `mittra2000conformal` as conformal
   FDTD for antennas.
5. **Related work, L144–150:** "a recipe applied to conductor-based antenna
   design as early as Erentok and Sigmund's sub-wavelength antenna work". Add
   Hassan et al. 2014. They are FDTD-based, coax-fed and microstrip-patch, which
   makes them closer than Erentok & Sigmund.
6. **Related work, L164–173:** "as of this writing, no structured-grid AD
   framework provides shape derivatives of a curved metallic radiator through a
   driven-port objective".
   - **Problem:** too broad. Structured-grid adjoint shape-parameter and density
     sensitivities for metal exist (`nikolova2004sensitivity`,
     `nikolova2006scattering`, `hassan2014topology`).
   - **What survives:** no peer-reviewed *conformal* (curved-boundary-exact)
     structured-grid adjoint for curved-PEC shape was found. Say exactly that.
7. **Related work, L189–206** ("True EM shape (node-motion) adjoints on
   unstructured meshes exist"). Add the closest prior art, which v4 omits:
   - `webb2000design`, `webb2002design`, `nair2003optimization` (3-D
     edge-element FEM S-parameter design sensitivities and optimization, with
     ports);
   - `nikolova2004adjoint`, `nikolova2006network`;
   - `vardapetyan2008sensitivity` and `ansys_hfss_derivatives` (HFSS);
   - `comsol_sensitivity_method` and `jensen2022comsolshape` (COMSOL RF shape
     optimization);
   - `choi2011generalized`;
   - `ham2019automated` and `gangl2021fully` (automated shape derivatives).
8. **White space, L214–222:** "(2) full open-radiator physics versus lossless
   PEC; and (3) differentiable-by-construction and composable versus a
   hand-derived adjoint."
   - **(2):** the FEM S-parameter lineage is lossy and ported, so the right
     contrast is open radiator (UPML, lumped port) versus closed waveguide
     devices.
   - **(3):** automated shape differentiation exists in Firedrake and NGSolve,
     so the contrast is AD element kernels in a general tensor stack versus
     symbolic UFL/NGSolve shape derivatives or hand derivations.
   - **"No found work occupies this exact combination"** can stay only after
     the lineage above is cited.
9. **§5, L536–541:** "a structured Yee grid cannot represent the curved
   conductor without a persistent ∼14 % perimeter bias unless refined to ∼250
   cells/mm". False for conformal FDTD. Scope it to staircased grids (the Meep
   material-grid adjoint for metal) and cite the conformal-FDTD line.
10. **§5.3 and the abstract** ("computationally intractable", "∼10¹⁵ cells").
    - The #659 review found the 10¹⁵ figure is really 2.5×10¹².
    - No published cost-to-accuracy study supports an intractability claim
      (Cluster 5), and a conformal-FDTD comparator would not need 250 cells/mm.
    - **Change:** make the claim a measured matched comparison, or remove it.
11. **Bibliography fixes in v5 `refs.bib`:**
    - replace `ghassemi2013` with `ghassemi2013antenna` (the first author is
      Massoud Ghassemi; adds vol. 7(4):268–276);
    - cite the published SSP, `hammond2025unifying` (Opt. Express
      33(16):33620, 2025), alongside or instead of the arXiv-only
      `hammond2025ssp`.

## Reviewer leads: verdicts

| Lead (as given) | Verdict | Entry / reason |
|---|---|---|
| Akel & Webb 2000 (3-D edge-element S-parameter shape sensitivity) | **Verified** (byline caveat) | `webb2000design`, IEEE Trans. Magn. 36(4):1043–1046, 10.1109/20.877620. Crossref order "Webb, Akel"; citing works give "H. Akel and J. P. Webb". Check on Xplore |
| Webb 2002 | **Verified** | `webb2002design`, IEEE Trans. Magn. 38(2):1109–1112, 10.1109/20.996284 |
| Hassan, Wadbro & Berggren 2014 (density-based metallic antenna design) | **Verified** | `hassan2014topology`, IEEE TAP 62(5):2488–2500, 10.1109/TAP.2014.2309112 (authors from the accepted-manuscript PDF; Crossref has none). Also `hassan2014patch` |
| Nikolova et al. 2004 (FDTD adjoint shape sensitivity) | **Verified, two papers** | `nikolova2004sensitivity` (Nikolova, Tam, Bakr, TMTT 52(4):1207–1216, FDTD on structured grids) matches the description; `nikolova2004adjoint` (Nikolova, Bandler, Bakr, TMTT 52(1):403–419) is the review |
| Dey–Mittra conformal FDTD | **Verified** | `dey1997locally`, IEEE MGWL 7(9):273–275, 10.1109/75.622536 |
| Yu–Mittra conformal FDTD | **Verified, three papers** | `yu2000conformal` (PEC, MOTL 27(2):136–138), `yu2001conformal` (dielectric, IEEE MWCL 11(1):25–27), `mittra2000conformal` (antennas and microstrip, IEEE APM 42(5):28–39) |
| Staircasing-convergence literature | **Verified** | `cangellaris1991analysis`, `holland1993pitfalls`, `ditkowski2001convergent`, `tornberg2008consistent`, `haggblad2014accuracy` |
| Ceviche, Hughes 2019 (#659) | **Verified, already cited** | `hughes2019forward`, ACS Photonics 6(11):3010–3016 |
| Meep adjoint solver (#659) | **Verified, already cited** | `hammond2022meepadjoint`, Opt. Express 30(3):4467 |
| Antenna topology-optimization reference (#659) | **Verified, already cited, now outranked** | `erentok2011topology`, IEEE TAP 59(1):58–69; Hassan 2014 is closer |

**Count.** 10 reviewer leads. 10 verified (3 already cited). None rejected.

## Rejected or unverified items

| Item | Reason |
|---|---|
| Akyurtlu / Werner staircase-error paper | Not found; Crossref returns only their chiral and dispersive FDTD papers. Dropped |
| A peer-reviewed FDTD-vs-FEM cost-to-accuracy curve for antennas | Not found; the benchmarks report accuracy spread only. Do not claim one exists |
| A peer-reviewed conformal-FDTD adjoint for curved-PEC shape | Not found; this is the defensible gap |
| DOI 10.1109/TAP.2006.875910 (circulating as Benkler 2006) | Resolves to Weinmann, ray tracing. The correct DOI is 10.1109/TAP.2006.875909 |
| Holland 1993 DOI …247848 | Wrong. The correct DOI is 10.1109/15.247856 |
| Swillam, Bakr, Nikolova, Li 2007 (FDTD adjoint, dielectric discontinuities) | Resolves (10.1080/02726340601166233) but is dielectric-only; not carried |
| Lu, Wadbro, Starck, Berggren, Hassan 2026 (decoupling feeding networks) | Resolves (10.1109/TAP.2025.3621265); later work in the same group, not carried |
| Oskooi, Kottke, Johnson 2009 (anisotropic subpixel smoothing) | Dielectric/anisotropic only; not carried |
| SSP "works on FEM solvers too" | Reported by a search agent from the abstract; not checked. Do not assert |

## Confirmed coverage (already in v4 `refs.bib`)

- Photonics AD and density: ceviche, Meep adjoint, Meep, SPINS-B, FDTDX (arXiv
  and JOSS), invrs-gym, SSP (arXiv), SSP2, Hooten 2025, Liu image
  representation, the Tidy3D changelog, the Meep adjoint docs, Piggott, Chung.
- Unstructured and EM shape adjoints: Wang & Anderson, Ghassemi (with errors),
  Ham–Mitusch–Schmidt, Arens, Takahashi, Balouchev, Gelly DGTD, PNGF.

## Identified gaps

- **Akel/Webb byline.** Confirm on IEEE Xplore.
- **Meep and conformal metal.** Check whether Meep's subpixel averaging covers
  conductors. If it does, v4's Meep staircase comparison weakens further.
- **The 2009 cost-and-accuracy claim.** Read Vasylchenko et al. 2009 before
  quoting any number from it.
- **Tidy3D's `PECConformal` docs.** Confirm they cite Dey–Mittra and Benkler, if
  v5 says so.

## Re-run delta (vs `.3.litsearch`)

The `.3` run scanned the AD-FDTD and differentiable-FEM ecosystem: Tidy3D, SSP,
PNGF, FDTDX, BIE shape derivatives. It did not search three areas: the CEM
adjoint and S-parameter sensitivity lineage (Webb, Nikolova), conformal FDTD,
and density design of *metallic* antennas (Hassan). All three bear directly on
v4's central claims. This run closes those gaps and corrects two v4 bib entries.

## Web provenance

| bib key | identifier | resolver |
|---|---|---|
| webb2000design | 10.1109/20.877620 | crossref |
| webb2002design | 10.1109/20.996284 | crossref |
| nair2003optimization | 10.1109/TMAG.2003.810520 | crossref |
| vardapetyan2008sensitivity | 10.1109/MWSYM.2008.4633219 | crossref |
| choi2011generalized | 10.1109/TMAG.2010.2087009 | crossref |
| nikolova2004adjoint | 10.1109/TMTT.2003.820905 | crossref |
| nikolova2006network | 10.1109/TMTT.2005.862663 | crossref |
| zhu2009accuracy | 10.1109/MWSYM.2009.5165750 | crossref |
| akcelik2008shape | 10.1016/j.jcp.2007.09.029 | crossref |
| ham2019automated | 10.1007/s00158-019-02281-z | crossref |
| paganini2021fireshape | 10.1007/s00158-020-02813-y | crossref |
| gangl2021fully | 10.1007/s00158-020-02742-w | crossref |
| nikolova2004sensitivity | 10.1109/TMTT.2004.825710 | crossref |
| nikolova2006scattering | 10.1109/TMTT.2006.871350 | crossref |
| hassan2014topology | 10.1109/TAP.2014.2309112 | crossref (authors added from the author PDF) |
| hassan2014patch | 10.2528/PIERB14030605 | crossref |
| hassan2015topology | 10.1109/TAP.2015.2449894 | crossref |
| aage2017topology | 10.1002/nme.5551 | crossref |
| jurgens1992finite | 10.1109/8.138836 | crossref |
| dey1997locally | 10.1109/75.622536 | crossref |
| railton1999analytical | 10.1109/22.740077 | crossref |
| yu2000conformal | 10.1002/1098-2760(20001020)27:2<136::AID-MOP16>3.0.CO;2-Q | crossref |
| mittra2000conformal | 10.1109/74.883505 | crossref |
| yu2001conformal | 10.1109/7260.905957 | crossref |
| zagorodnov2003uniformly | 10.1002/jnm.488 | crossref |
| benkler2006new | 10.1109/TAP.2006.875909 | crossref |
| benkler2008mastering | 10.1109/MAP.2008.4562256 | crossref |
| farjadpour2006improving | 10.1364/OL.31.002972 | crossref |
| cangellaris1991analysis | 10.1109/8.97384 | crossref |
| holland1993pitfalls | 10.1109/15.247856 | crossref |
| ditkowski2001convergent | 10.1006/jcph.2001.6719 | crossref |
| tornberg2008consistent | 10.1016/j.jcp.2008.03.045 | crossref |
| haggblad2014accuracy | 10.1007/s00211-014-0625-1 | crossref |
| vrancken2007benchmark | 10.1016/j.aeue.2006.07.001 | crossref |
| vasylchenko2007benchmarking | 10.1049/ic.2007.1176 | crossref |
| mustra2007comparison | 10.1109/ICECOM.2007.4544469 | crossref |
| vasylchenko2009quality | 10.1109/MAP.2009.4939017 | crossref |
| hammond2025unifying | 10.1364/OE.563512 | crossref |
| ghassemi2013antenna | 10.1049/iet-map.2012.0374 | crossref |

**Keys renamed by hand** (resolver key, then the key used here):
- the two `nikolova2006sensitivity` keys, which collided →
  `nikolova2006network` and `nikolova2006scattering`;
- `2014topology` → `hassan2014topology`;
- `wenhuayu2001conformal` → `yu2001conformal`.

**Formatting fix:** `&amp;` → `\&` in container names.

**URL-kind documentation sources** (in `candidates.bib`, fetched 2026-10-12):

| bib key | URL | supporting quote |
|---|---|---|
| ansys_hfss_derivatives | ansyshelp.ansys.com … OverviewoftheTechnicalApproachforDerivativesinHFSS.htm (v252) | "HFSS can compute the derivatives of S-parameters and related matrix quantities"; "The variables of differentiation could be geometric parameters, such as the length of a waveguide"; "Once the LU factors have been obtained, it is very efficient to solve additional right-hand side vectors" |
| comsol_sensitivity_method | doc.comsol.com/6.2/…/comsol_ref_sensitivity.31.07.html | "The adjoint method solves for the derivatives of a single scalar objective function"; "requiring only one single additional linear system solution" |
| jensen2022comsolshape | comsol.com/blogs/shape-optimization-in-electromagnetics-part-2 | waveguide iris filter and 5G diplexer with S-parameter objectives; shape optimization "works by deforming the mesh"; "evaluating the sensitivity requires solving 52 linear problems" |

## Web leads (unverified)

| title | authors | year | URL as found | reason unresolved |
|---|---|---|---|---|
| tidy3d.PECConformal API docs (cites Dey–Mittra and Benkler) | Flexcompute | 2026 | https://docs.flexcompute.com/projects/tidy3d/en/latest/api/discretization/_autosummary/tidy3d.PECConformal.html | url-kind unsupported in v0; reported by the search agent, not fetched by the lead author |
| Häggblad & Runborg author PDF (quotes used above) | J. Häggblad, O. Runborg | 2014 | https://www.csc.kth.se/~olofr/Publications/staircase.pdf | url-kind; the DOI version is carried as `haggblad2014accuracy` |

## Search log (2026-10-12)

1. Read #1093, #659 (the v4 re-review notes), v4 `main.tex` (L20–230, L427–470,
   L536–553) and the v4 `refs.bib`.
2. Reviewer leads: Crossref lookups for each.
   - Fetched the Hassan et al. 2014 accepted manuscript to confirm the authors
     and the FDTD/adjoint/coax/microstrip content, since Crossref lacks
     authors.
   - Web search for the Akel/Webb byline: citing works give "H. Akel and J. P.
     Webb".
3. Conformal-FDTD and staircasing axis (search agent): Jurgens, Dey–Mittra,
   Yu–Mittra, Railton–Schneider, Benkler, Zagorodnov, Farjadpour,
   Cangellaris–Wright, Holland, Ditkowski, Tornberg–Engquist, Häggblad–Runborg
   (the author PDF was read).
4. Cost-to-accuracy axis (search agent): HFSS/CST/FEKO patch benchmarks and the
   Vandenbosch-group benchmarks. No cost-curve study was found.
5. FEM S-parameter sensitivity axis (shared with the transmon run): Webb/Nair,
   Nikolova, the HFSS help page and the COMSOL docs and blog (fetched),
   Firedrake/NGSolve.
6. AD-FDTD conformal-PEC pre-emption check: the Tidy3D changelog (2.10.x
   PEC-embedded dielectric gradients only), FDTDX JOSS, SSP published version.
7. Verification: `cite.py::resolve()` on 40 identifiers (40 OK; 39 carried, the
   FDTDX duplicate dropped), plus 3 side leads (OK, not carried) and the 2 wrong
   circulating DOIs (resolve to unrelated papers). `bib_lint.lint_bib_file`
   reported no issues.
