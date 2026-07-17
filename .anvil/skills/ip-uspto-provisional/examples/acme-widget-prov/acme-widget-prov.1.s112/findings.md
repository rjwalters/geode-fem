# s112 findings — acme-widget-prov.1

Per-feature enablement findings, ordered: enablement (dim 1), coverage (dim 2),
possession (dim 3), conversion readiness (dim 9). All are advisory revision
opportunities; none rises to a critical flag.

## Dimension 1 — §112(a) enablement depth

### F1 (minor) — bandgap current sources named but not exemplified
- **Location**: `spec.tex` § Detailed Description ¶[0011]; `BRIEF.md#3.1`.
- **Rationale**: the constant-current source (42) and PTAT source (44) are
  identified by type ("bandgap-referenced current mirror", "current mirror whose
  reference is the difference in base-emitter voltage between two bipolar
  junctions"), which a PHOSITA can build, but no worked component example is
  given. Enablement is met; ceiling is not.
- **Suggested fix**: add one concrete exemplar of the bandgap reference (device
  ratio, mirror topology) to lift dim 1 to 8/8.
- **Question for inventors**: is there a preferred reference topology you intend
  to disclose, or is any standard bandgap acceptable?

## Dimension 2 — embodiments, alternatives & ranges coverage

### F2 (minor) — alternative span-trim path named but not dimensioned
- **Location**: `spec.tex` § Detailed Description ¶[0014]; `BRIEF.md#5`.
- **Rationale**: the laser-trimmed PCB resistor network is offered as an
  alternative to the thin-film span trim resistor (30) but without tolerance or
  TCR figures. Every undimensioned alternative attaches priority only as far as
  the disclosure carries.
- **Suggested fix**: state the tolerance/TCR target for the laser-trimmed
  alternative, mirroring the thin-film resistor's "0.05 % ... 25 ppm/°C".

## Dimension 3 — written-description possession

No possession deficiencies found. Each §3 feature is described with concrete
structure (resistor types, placement, trim procedure) rather than aspiration;
the score sits at ceiling (5/5).

## Dimension 9 — conversion readiness

### F3 (minor) — same-die adjacency lacks a placement tolerance
- **Location**: `spec.tex` § Detailed Description ¶[0016]; `BRIEF.md#4.2`.
- **Rationale**: the integrated embodiment's process-corner tracking rests on
  the dummy half-bridge (20) being "immediately adjacent" to the sense bridge
  (10), disclosed qualitatively. A dependent claim drawn to the integrated
  embodiment would benefit from a quantified adjacency fallback.
- **Suggested fix**: add a preferred maximum separation (or "within the same
  implant well") so the dependent claim has a quantified narrower position.

## Claim-seed note

`claims.tex` is present and every seed limitation traces to enabling disclosure
(split-path leg, ratio resistor, dummy half-bridge, single-temperature span
trim, same-die integration). No seed limitation lacks support, so no
disclosure-gap finding is routed from the seed. The seed raises dim 9; its
absence would never have been a finding.
