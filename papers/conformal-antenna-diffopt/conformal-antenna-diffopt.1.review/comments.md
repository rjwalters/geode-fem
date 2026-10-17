# Comments — conformal-antenna-diffopt.1

Keyed to `main.tex` sections. Grouped by severity.

## Blocker

_None._ No critical flag; the below are graded improvements, not desk-rejects.

## Major

- **Title vs. body — unsubstantiated comparative claim.** Title: "A Curved Conformal Antenna Structured-Grid Inverse Design **Cannot Reach**." The paper presents no experiment in which a structured-grid method fails to reach this geometry — the body honestly defers that: "The comparative superiority over structured-grid density methods is \emph{not} yet quantified --- we claim reachability, not a measured win" (§5). A sophisticated reviewer will read the title as an asserted comparative result and then find no comparison. Retitle to a claim the evidence supports (reachability / body-fitted node-motion), or run the §6 head-to-head before making the "cannot reach" claim. This did **not** rise to a critical flag only because the abstract, contributions, and results never assert the comparative win — the over-reach is confined to the title as motivational framing.

- **Evidence rests on a single fixture with no ablation or baseline (§4, §5).** "The reported design is a single fixture at one bend radius and one band; we have not swept mesh resolution, bend radius, or band placement." The reachability run is validated thoroughly, but nothing isolates *why* 73 freeform DOFs or the harmonic morph matter — e.g. a low-DOF parametric run on the same `bent_conformal` fixture, or a with/without-morph comparison. For the diff-sim audience this is the load-bearing evidence gap; adding one such ablation would move D2 substantially.

- **`wang2011` bibliography entry has a garbled/conflated title (refs.bib).** The entry title reads "Shape Sensitivity Analysis for the Compressible Navier--Stokes Equations via Discontinuous Galerkin Methods **and** Adjoint-Based Shape Optimization for Electromagnetic Problems" — this scans as two distinct paper titles merged into one field (AIAA Journal 2011, 49(6), pp. 1302–1305, a 4-page note). `wang2011` is the load-bearing citation for the paper's novelty-honesty positioning ("node-motion EM shape adjoints already exist" — §1, §2). If the entry is mis-assembled, the positioning rests on a shaky reference. `related-work`: route to a `paper-litsearch` re-run to re-resolve the DOI `10.2514/1.J050594` and confirm the entry (a) is a single real paper and (b) actually presents "a discrete Maxwell shape adjoint on unstructured meshes ... in 2-D, time-domain, lossless, PEC-walled" as the paper characterizes it (§2). Verification of claim-support is the auditor's job; the malformed title is visible now and is a D8 hygiene deduction.

- **Both figures are unrendered placeholders (§3 Fig.1, §4 Fig.2).** The source tree holds only `figures/src/` (`plot_s11_band.py`, `setup_schematic.md`); `\includegraphics{figures/setup_schematic}` and `{figures/s11_band}` have no target artifact, and each caption is stamped "(Placeholder --- rendered by \texttt{paper-figures} ...)." Run `paper-figures` so the PDF actually shows the schematic and the S11-band plot; until then the figures cannot be scored for legibility/axis-labels/palette and `paper-audit` will surface missing-image boxes.

## Minor

- **Body has no code/artifact-availability statement (§4, §5).** The repository, branch, and commit (`524db3b`, `feature/issue-650`) appear only in the `.tex` header comment, invisible in the rendered PDF; the figure captions say "committed artifact \texttt{conformal\_results.toml}" but never point the reader to where it is committed. Add a one-line availability statement (repo + commit) in the body, and land the artifact on a durable branch — a reader on the default branch cannot currently locate `benchmarks/patch_antenna_conformal/conformal_results.toml`.

- **Prior single-DOF capstone number is untraced (§4, Methodological foundation).** "validated by a prior single-DOF capstone that retuned a flat patch, at FD relative error $\sim\!10^{-9}$" — this figure does not appear in `conformal_results.toml` (it belongs to a different, earlier artifact). Either cite that prior artifact/result explicitly or drop the specific $\sim\!10^{-9}$ so every stated number has a traceable home. (BRIEF also carried a `-28$ dB flat-patch value; the drafter correctly omitted it rather than assert it unsourced — good.)

- **`ghassemi2013` entry is missing volume/number/pages (refs.bib).** Has DOI and year; complete the standard fields for consistency with the other entries.

- **Repeated positioning thesis (abstract, §1, §2.4, §5, §6).** The "narrow, previously-unoccupied combination" + three-differentiators framing recurs in five places. Stating it crisply once (in §2.4) and referencing it thereafter would tighten D9 without losing the honesty signal.

- **Unit reframing vs. artifact field names (§3, §4).** The paper presents bend radius, PML thickness, and port resistance as dimensionless "natural units," but the source artifact records them as `bend_radius_mm = 40`, `pml_thick_mm = 8`, `port_resistance_ohm = 50`. The reframing is defensible and BRIEF-directed (keep dimensionless, no GHz), but a careful reader comparing paper to artifact will notice the `_mm`/`_ohm` suffixes. A half-sentence noting that the solver's stored suffixes are nominal scaffolding would pre-empt the question.

## Nit

- Title is long (two lines) and the "Cannot Reach" flourish sits awkwardly against the deliberately hedged body voice; a shorter, claim-accurate title would read better and reduce the D2/D9 tension noted above.

## Procedural notes

- `sidecar`: `staged_sidecar` CLI shim used (uv present); atomicity tool-enforced this pass.
- Render-gate skipped — `main.pdf` / `compile-log.txt` absent (`paper-audit` not yet run); fail-open as documented. Overfull-hbox / unresolved-ref checks deferred to audit.
- Numeric-consistency detector ran clean (142 numbers extracted, 0 arithmetic-claim inconsistencies). Quoted-evidence self-check ran clean (9/9 dimensions verbatim-verified).
