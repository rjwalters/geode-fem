# Comments — conformal-antenna-diffopt.2

Line-level feedback keyed to section headings, grouped by severity. This review advances the paper (38/44); the items below are non-blocking polish for the reviser / auditor.

## blocker

_None._

## major

_None._

## minor

- **§3 / §4 / §5.2, figure captions — stale placeholder scaffolding.** All three `\caption{}`s still end with "(Placeholder --- rendered by `paper-figures` from `figures/src/...`)". The figures now render (`figures/s11_band.pdf|png`, `figures/runtime_scaling.pdf|png`, `figures/setup_schematic.pdf` all exist on disk). Strip the parenthetical placeholder note from each caption — as written it falsely tells a reader the figure is a placeholder, and it is the kind of leftover a program-committee member notices immediately. (D6/D7)

- **Title + §5.3 — categorical "cannot reach" vs box-bounded evidence.** The intractability is measured against *single-process* Meep 1.34.0 on one `m6i.4xlarge` (61 GB) instance; the RAM wall at R≥14 and the "computationally intractable" conclusion are therefore hardware-specific. The body is careful ("cannot *practically* reach ... at the resolution the curved geometry demands"), but the bare title verb "Cannot Reach" and the §5.3 "it cannot practically reach the design" read as a method-class claim. Recommend one clause acknowledging that the wall is single-node/single-process (distributed or GPU FDTD would move it, at added engineering cost) so the categorical framing matches the measured scope. (D1/D9)

- **§5.1, staircasing table — non-monotonic perimeter error deserves a half-sentence.** Perimeter rel-err is 13.0% (N=20) → 15.4% (N=40) → 14.2% (N=80=N=160). The paper faithfully reports this and calls it a "plateau at ∼+14%," which the data supports (N=80/160 identical at 14.2%), but a reader may stumble on the N=40 bump. A parenthetical noting the non-monotone approach to the plateau (a rasterization-phase artifact) would preempt the question. Verified against `staircasing_results.json` — the paper's numbers are correct. (D2)

- **§4 / Artifact availability — name the commit SHA.** The availability section says artifacts are "committed on the `main` branch" but gives no commit hash. The v1 review asked for a specific SHA; naming one (or a tag) makes the reproducibility claim pin-precise. (D5)

## nit

- **Rhetorical economy — headline-number repetition.** worst-of-band −5.51→−12.06 dB appears in the abstract, §1 contributions, §4, and §7; the ∼R³ cells / ∼R⁴ wall-clock pair appears in the abstract, §1, §5.2, §5.3, §6, and §7. §5.3 "Head-to-head" largely restates §5.1+§5.2. One consolidated statement each would tighten the paper without losing signal. (D9)

- **§2 Related Work — `related-work` tag.** The two contrast-class tools the paper leans on most (ceviche / Hughes et al. ACS Photonics 2019; the Meep adjoint-solver reference) are discussed but uncited, deferred to a litsearch pass in §7. This is handled honestly and is not a hygiene defect, but a `paper-litsearch` re-run should resolve DOIs/arXiv ids for both (and the requested canonical antenna-topology-optimization reference beyond ghassemi2013) so §2's contrast class is fully cited before submission. `related-work`

## Procedural notes

- Render-gate: skipped fail-open (`main.pdf` / `compile-log.txt` absent; `paper-audit` has not run) — expected at review time; `_gate.json` records the skip.
- Numeric-consistency: automated detector ran clean (280 numbers extracted, 0 arithmetic-claim inconsistencies).
- Evidence-check: automated verifier ran clean (9/9 dimension quotes verified verbatim against `main.tex`).
