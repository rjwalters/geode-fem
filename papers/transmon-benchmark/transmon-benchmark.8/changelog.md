# v8 figure revision — 2026-10-11 UTC

Operator-requested improvement pass under #1059, based on v7 at `a50a446a`.
No previous version or critic artifact is modified. This remains DRAFT;
no independent score, review, or audit is carried forward.

- Figure 1: replace the crowded mesh drawing with physical conductor outlines,
  labeled device parts, an actual junction-sheet detail, and an explicitly
  schematic material cross-section. Geometry remains tied to the input mesh.
- Figure 2: add the confirmed qubit and signed residuals alongside parity.
  Use its recorded 3.452751 GHz value when calculating the +0.00005% error;
  separate the port-sheet artifact visually and disclose unequal unknown counts.
- Figure 3: separate qubit and artifact inductance checks, retain input precision,
  show independently anchored inverse-square-root references and the Palace check.
- Figure 4: put GEODE surface participation and Palace port-voltage EPR on
  separate axes, with explicit observable labels and a logarithmic EPR scale.
- Figure 5: show the recorded refined P2 optimization, Palace/finer-mesh checks,
  and actual initial/final island outlines. Snapshot the #1058 data with hashes.
  The ±1% shading is a working tolerance, not an error bound or confidence band.
- Figure 6: replace differing timing scales with one linear axis, median labels,
  min/max whiskers, and visible workload qualifications.
- Figure 7: retain the earlier analytic and one-parameter experiments in an
  appendix, with clearer labels and spacing. Its failed target is historical.
- Use a shared, self-contained vector plotting style; enlarge figures in the
  manuscript and eliminate the private plotting-style dependency.
- Update captions and add the methods/scope needed to interpret Figure 5.
  Refinement is outside the differentiable map; scalar-epsilon electrostatic
  checks do not establish continuum convergence or full-wave inverse design.
  Preserve pending dependencies and all operator submission markers.
- Add deterministic regeneration and numerical provenance checks. See
  `validation.md` for the checks actually run and their scope.
