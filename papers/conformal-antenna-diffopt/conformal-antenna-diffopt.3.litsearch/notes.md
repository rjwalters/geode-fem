# Litsearch re-run — conformal-antenna-diffopt.3 (2026-07-21)

**Trigger:** issue #659 items 4+5 — the deferred litsearch-gap closure plus the
mandatory pre-submission novelty re-scan (the AD-FDTD ecosystem moves fast; the
v2 audit and the 2026-07-20 deep-research scan both required this before arXiv).
Operator-authorized revise pass; the paper is AUDITED at v3 but unpublished, so
there is no downside to folding improvements into a v4.

**Provenance:** an adversarial web-scan agent (2026-07-21, session f6bb8db3)
searched four axes: conformal-boundary adjoint FDTD; recent differentiable/adjoint
antenna inverse design; differentiable Palace / JAX-FEM / warp; unstructured
time-domain adjoints. Every candidates.bib entry in the RESOLVER-VERIFIED section
was resolved through `anvil/lib/cite.py::resolve()` (Crossref / arXiv API) from an
identifier the scan surfaced — none from memory. Scan verdict: **no scoop; submit
with citations added + one mandatory framing repair.**

## Verdict for the reviser

The paper's core combination claim stands unoccupied: nothing found combines
(driven-Maxwell FEM shape adjoint with ports + UPML + lossy media) ×
(curved conformal metal radiator result) × (tensor-compiled AD stack).
Axis 3 (differentiable Palace / JAX-FEM Maxwell / NVIDIA warp EM) returned
**zero hits** — the differentiable-complement positioning is uncontested.

But two of the paper's supporting axes must be re-scoped against the current
best structured-grid practice, not against naive staircase Meep. This is a
**framing repair of roughly two scoping sentences plus citation wiring — not a
rewrite.** No number, method, or result changes.

## Findings and how to fold each in

### F1 — Tidy3D autograd (`tidy3dautograd`) — UNDERCUT, highest priority
Flexcompute's commercial AD-FDTD: 2.8.0 (2025-03) polyslab vertex gradients;
2.10.0 (2025-12) dispersive + LossyMetalMedium autograd; 2.11.0 (2026-04)
shape-derivative boundary sampling; 2.12.0-dev (2026-05/06) TriangleMesh vertex
gradients + PEC gradients for *dielectric-embedded-in-PEC*; forward solver has
PECConformal subpixel averaging; a rectangular-patch-antenna adjoint example
ships in their learning center. What it does NOT do (as of 2.12-dev): shape
derivatives *of a curved PEC/lossy-metal structure itself* through a driven-port
objective. **Required change:** cite in §2 (related work) and argue explicitly —
closed-source/cloud-only, PEC-structure gradients absent, patch example is
grid-aligned rectangular. The title's "Cannot Reach" must read as a claim about
what any *current* structured-grid AD framework provides, and §2/§5 must say so.

### F2/F3 — Subpixel-smoothed projection, SSP (`hammond2025ssp`) + SSP2 (`romano2026ssp2`) — UNDERCUT (partial)
Meep/Johnson-group work: differentiable subpixel-accurate binarized boundaries
on structured grids (SSP, Opt. Express 2025; SSP2 2026 adds twice-differentiable).
Kills any naive "density methods = staircasing" sentence. The save (verify
wording against `meepadjointdocs`): Meep's material-grid adjoint does not
support dispersive/metallic media, so the staircasing axis survives **scoped to
metallic radiators**. **Required change:** cite both next to the staircasing
discussion (§2 and §5.2) and scope the claim to curved *metal*; cite
`meepadjointdocs` for the no-metal-adjoint limitation.

### F6 — PNGF near-real-time antenna inverse design (`sun2025pngf`) — UNDERCUT (intractability axis)
Sun/Sideris et al.: precomputed numerical Green functions + direct binary
search; full-wave, lumped-port-driven, fabricated 5G antenna prototypes, up to
16,000× speedup over commercial solvers. Not adjoint, not curved-boundary
(pixelated DBS) — but it flatly contradicts any unscoped "structured-grid
antenna inverse design is computationally intractable" reading. **Required
change:** cite in §5 (intractability axis) and scope the measured claim to
*matched conformal-boundary fidelity, single-process, open-source
density-adjoint* comparison — the 61 GB Meep number otherwise reads as a
strawman to an RF-aware reviewer.

### Adjacent (cite, one clause each; these mostly SUPPORT the paper)
- `mahlau2026fdtdx` (FDTDX JOSS 2026): per its own paper, linear non-dispersive
  materials only — time-reversal AD breaks on lossy media. Supports the gap.
  The paper currently cites the FDTDX arXiv preprint (`fdtdx`); ADD the JOSS
  reference alongside or supersede — reviser's choice, keep both keys valid.
- `liu2024imagerep`: subpixel-smoothed rasterized shape gradients on grids,
  dielectric photonics only. Cite next to `hooten2025` in §2.
- `arens2026tubular` (2026-06-29): BEM/EFIE domain-derivative shape optimization
  of freeform PEC scatterers — newest PEC shape-sensitivity work; scattering
  only, no ports/PML/AD. Cite in the §2 shape-adjoint prior-art paragraph.
- `takahashi2025tdbie`: time-domain BIE PEC shape derivative (forward/adjoint
  surface currents). Further bounds "first PEC shape adjoint" — consistent with
  the paper's already-narrowed claim.
- `balouchev2024balun`: isogeometric shape optimization of coax baluns via
  integral equations — the IE-method curved-geometry alternative; driven RF
  component. Cite in §2.
- `gelly2026dgtd`: DGTD metasurface inverse design via *Bayesian* search, not
  adjoints — supports the "unstructured time-domain lacks practical adjoints"
  gap. Optional cite.

## Web leads (unverified — do NOT cite)
- ISAP-2005 conformal-FDTD adjoint shape optimization,
  https://www.ieice.org/cs/isap/ISAP_Archives/2005/pdf/1C2-5.pdf — archive URL
  returned empty on verification; author list unconfirmed. If the author wants
  the defensive footnote about the conformal-FDTD escape hatch, the fetchable
  fact base is Tidy3D's PECConformal docs (`tidy3dautograd`) instead.
- "Mosaic: A Benchmark Suite for Differentiable Physics Solvers,"
  arXiv:2606.27895 — could not confirm whether it includes a Maxwell task; skim
  before submission, cite only if it does.
- Stanford dissertation finding that adjoint sensitivities fail at sharp metal
  corners (favors FEM + rounded curved boundaries) — no stable identifier
  surfaced; lead only.

## Defensible claim wording (the two scoping sentences)
1. §2 or §5 (staircasing axis): "As of this writing, no structured-grid AD
   framework provides shape derivatives of curved *metallic* radiators through
   a driven-port objective: Meep's adjoint excludes dispersive/metallic media
   [meepadjointdocs], FDTDX's time-reversal AD requires lossless propagation
   [mahlau2026fdtdx], and Tidy3D's autograd (2.12-dev) stops at
   dielectric-embedded-in-PEC gradients [tidy3dautograd]." (Adapt wording; the
   title's "cannot" is a fidelity claim, not a wall-clock claim — say so once.)
2. §5 (intractability axis): scope explicitly to matched conformal-boundary
   fidelity, single-process, open-source density-adjoint comparison, and name
   `sun2025pngf` + GPU-FDTD (Tidy3D) as the objections the scoping answers.

## Prior operator pass to fold in (see prior-operator-pass.patch)
A #659 operator pass was applied in-place to `.3/` and then reverted to keep the
audited v3 pristine; `prior-operator-pass.patch` carries the exact wording.
Reapply its substance in v4:
1. Artifact-availability section: add the public repo URL
   `https://github.com/rjwalters/geode-fem` (repo visibility verified PUBLIC).
2. Wire the four base litsearch entries — `hughes2019forward`,
   `hammond2022meepadjoint`, `oskooi2010meep`, `erentok2011topology` (already
   resolver-verified, present in the thread-root `refs.bib`) — into the intro
   tool list, the §2 contrast-class paragraph (including the Erentok–Sigmund
   sentence), and the §5 Meep-1.34.0 first use.
3. Remove the two "named-but-uncited" disclaimer passages (§2 and §6) that
   scaffolded this litsearch pass.

## Build note (from the operator pass)
`latexmk` does not trigger bibtex in this environment. Build with
`pdflatex && bibtex main && pdflatex && pdflatex`, TEXINPUTS pointing at
`anvil-paper.cls` in `../../transmon-benchmark/transmon-benchmark.6/`. Commit
`main.bbl` with the version — arXiv does not run bibtex.
