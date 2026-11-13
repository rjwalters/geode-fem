# Self-heating, thermo-elastic stress and electromigration lifetime: build plan

**Issue:** #1555 (design note, no solver code) · **Date:** 2026-11-13 · **geode:**
`origin/main` at `2fcf4c79`.

**Status.** Plan only. Child issues: #1556 (heat conduction + self-heating), #1557
(thermo-elastic stress), #1558 (electromigration models). The choice of home, features in
existing engines with geode-fem owning the models, was auto-selected in an attended review
and is not an operator ruling. This note assumes it. Published physics only.

## 1. Order and dependencies

| # | Piece | Issue | Depends on |
|---|---|---|---|
| 1 | Steady heat conduction + Picard self-heating loop | #1556 | merged DC conduction (#1476) |
| 2 | Linear thermo-elastic stress | #1557 | piece 1 (`T`) |
| 3 | EM lifetime consumers (Black, Blech, Korhonen) | #1558 | Black/Blech: `J` and `T` (piece 1); Korhonen coupling to the 3-D field: piece 2 |

Rejected: one monolithic solver issue (not independently buildable); building piece 3
first (nothing to consume).

## 2. Piece 1: heat conduction and self-heating

- **Reuse.** `ConductionSolution.joule_density` (W/m³ per tet) in
  `crates/geode-core/src/assembly/conduction.rs` is the heat source `q`. The operator
  `∇·(k∇T) = -q` is the same as the conduction operator with `k` for `σ`, so reuse the
  scalar P1 stiffness kernel `tet_p1_local` / `tet_p1_local_tensor` in
  `assembly/electrostatic.rs` (mass part unused).
- **New.** Per-tet `k`, load vector from `q`, fixed-T and zero-flux boundaries, Picard
  loop `σ(T)` → `q` → `T` → `k(T)`, `σ(T)`.
- **Self-consistency.** `σ(T)` of a metal falls with `T`, so for fixed voltage `q` falls as
  the line heats and the loop is a contraction for mild coefficients. Under fixed current
  the sign flips and runaway is possible. The loop must use relaxation, stop when
  `max|ΔT|` grows over successive iterations, and report non-convergence.
- **Models.** Linear temperature coefficient of resistivity `ρ(T) = ρ0 [1 + a (T − T0)]`
  (user-supplied `a`); `k` user-supplied, or from the Wiedemann–Franz law (Wiedemann and
  Franz, "Ueber die Wärme-Leitungsfähigkeit der Metalle", Annalen der Physik 89, 497, 1853).
- **Validation target.** Uniformly heated 1-D bar of length `L`, fixed `T0` at both ends,
  uniform `q`: `T(x) = T0 + q x (L − x) / (2k)`, peak `T0 + qL²/(8k)`. In 1-D P1 is nodally
  exact for this load. On a tet mesh of a bar, require the peak within 1% for the committed
  mesh and a clean `h`-refinement trend. Also require total boundary heat flux equal to
  `power_volume` to solver tolerance.

## 3. Piece 2: linear thermo-elastic stress

- **Reuse.** Mesh, boundary tags and nodal `T` from piece 1.
- **New.** Vector-P1 isotropic elasticity element kernel. A search for
  `elasticity|thermal` over `crates/*/src` and `docs/` found nothing on 2026-11-13, so this
  is new code. Thermal eigenstrain `ε_th = α (T − T_ref) I`, load vector, displacement
  constraints, per-tet stress (including hydrostatic stress for piece 3).
- **Model.** Linear thermo-elasticity with the Duhamel–Neumann constitutive law
  (Duhamel, 1838; Neumann, 1841). Treated as textbook theory, not a modelling claim.
- **Validation targets.**
  1. Fully constrained bar under uniform `ΔT`: `σ = −E α ΔT` uniaxial, and
     `σ = −E α ΔT / (1 − ν)` for the equibiaxial plane-stress film case. Tolerance 1%
     (P1 tets are constant-strain, so a uniform field should be reproduced to solver
     tolerance).
  2. Bimaterial strip curvature against S. Timoshenko, "Analysis of bi-metal thermostats",
     J. Opt. Soc. Am. 11, 233–255 (1925). Tolerance a few percent, since the 3-D tet strip
     has finite thickness and end effects.
- **Out of scope.** Plasticity, creep, transient response.

## 4. Piece 3: electromigration lifetime

All three models are post-processing of `J`, `T` and, for Korhonen, stress.

- **Black's equation.** `MTTF = A j^(−n) exp(Ea / k_B T)`. J. R. Black, "Electromigration —
  A brief survey and some recent results", IEEE Trans. Electron Devices 16(4), 338–347
  (1969). `A`, `n`, `Ea` are user inputs, never defaults from this note.
- **Blech short-length condition.** A line between blocking boundaries is immortal when
  `j L` is below a critical product `(jL)_crit`, because the back stress gradient balances
  the electron wind. I. A. Blech and C. Herring, "Stress generation by electromigration",
  Appl. Phys. Lett. 29, 131–133 (1976); I. A. Blech, "Electromigration in thin aluminum
  films on titanium nitride", J. Appl. Phys. 47, 1203 (1976). The critical product depends
  on a critical stress, effective charge `Z*` and resistivity, supplied by the user.
- **Stress evolution (Korhonen-type).** 1-D drift–diffusion of vacancies with stress
  back-flow along a confined line: M. A. Korhonen, P. Børgesen, K. N. Tu and C.-Y. Li,
  "Stress evolution due to electromigration in confined metal lines", J. Appl. Phys. 73,
  3790–3799 (1993). The steady state is a linear stress profile in `x` whose slope
  balances the electron wind. DC and pulsed `j`: pulses are integrated in time, with the
  fast-pulse limit equal to the time-averaged `j`.
- **Validation targets.** Black: the closed form itself, across `j` and `T` (tests the
  plumbing and units). Blech: threshold bracket between blocking ends against the
  analytic `jL`. Korhonen: steady-state linear profile and its `j` threshold.
- **Limits.** Blech and the Korhonen steady state are valid only for short lines between
  blocking boundaries. Void nucleation and growth kinetics beyond Korhonen-type models,
  and proprietary foundry EM data, are out of scope.

## 5. Citation check

Black (1969), Blech–Herring (1976), Korhonen et al. (1993) and Timoshenko (1925) were
checked against bibliographic records on 2026-11-13 (title, venue, volume, pages). Their
full texts were not read here. Each equation used as a validation target must be checked
against the source by the child-issue builder before the test is written. The Korhonen et
al. author list used above, with K. N. Tu as the third author, was confirmed against the
DOI record `10.1063/1.354073`. The Wiedemann–Franz and Duhamel–Neumann references are
classical and not re-checked.
