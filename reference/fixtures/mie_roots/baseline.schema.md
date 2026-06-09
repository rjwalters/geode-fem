# `mie_roots/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
analytic Mie resonance root catalogue (issue #170, parent epic #88,
Phase J.1).

## What the fixture pins

Root-for-root cross-check between the Burn-side
`geode_core::mie::{resonance_roots, merged_roots, mie_roots_catalog}`
and the SciPy reference in `reference/numpy/mie_roots.py`. This is the
purest cross-check in Epic #88: no mesh, no eigensolve — just spherical
Bessel special functions (hand-rolled upward/Miller recurrences on the
Rust side vs `scipy.special.spherical_jn`/`spherical_yn`) and 1-D root
finding (60-step bisection vs `scipy.optimize.brentq`).

Physical setup (PEC-cavity dielectric sphere, the v0 Mie benchmark
ground truth — see the `crates/geode-core/src/mie.rs` module docs):

- Inner dielectric sphere `0 ≤ r ≤ R_s = 1.0`, refractive index
  `n = 1.5` (`N_INSIDE` in `examples/mie_sphere.rs`).
- Vacuum buffer `R_s ≤ r ≤ R_b = 2.0`.
- PEC wall at `r = R_b` (`R_SPHERE` / `R_BUFFER` in
  `crates/geode-core/src/mesh/sphere.rs`) — closed cavity, purely real
  spectrum. *Not* the open-space Mie scattering problem.

Catalogue extent mirrors the heaviest Burn consumer
(`examples/mie_sphere.rs`): `l_max = 4`, `n_max = 5` ⇒ 2 polarisations
× 4 angular orders × 5 radial orders = 40 roots, all inside the search
window `k ∈ (0.1, 20.0]`.

## Ordering convention

Roots are stored sorted by the canonical integer key **(pol, l, n)**
(`pol`: 0 = TE, 1 = TM), *not* by ascending `k`. The catalogue contains
near-degenerate cross-channel pairs (e.g. TE(1,1) at k ≈ 1.88943 vs
TM(2,1) at k ≈ 1.89074) whose global `k`-order would be fragile under
sub-tolerance perturbation; the harness joins the two catalogues on the
exact integer tags instead.

## Output fields (under `outputs`)

`n_roots = 40` on the shipped fixture. Integer-valued fields use the
schema-v1 `tolerance_abs = 0.5` strict-integer idiom.

| Field               | Shape       | Tolerance | What it pins                                                       |
|---------------------|-------------|-----------|--------------------------------------------------------------------|
| `l_max`             | `[1]`       | `0.5`     | Maximum angular order (4).                                          |
| `n_max`             | `[1]`       | `0.5`     | Roots per (l, polarisation) channel (5).                            |
| `n_roots`           | `[1]`       | `0.5`     | Total catalogued roots (40).                                        |
| `n_inside`          | `[1]`       | `1e-15`   | Refractive index 1.5 — bit-exact constant pin.                      |
| `r_sphere`          | `[1]`       | `1e-15`   | `R_s = 1.0`; must equal Burn `mesh::R_SPHERE` bit-exactly.          |
| `r_buffer`          | `[1]`       | `1e-15`   | `R_b = 2.0`; must equal Burn `mesh::R_BUFFER` bit-exactly.          |
| `root_pol`          | `[n_roots]` | `0.5`     | Polarisation tag per root: 0 = TE, 1 = TM.                          |
| `root_l`            | `[n_roots]` | `0.5`     | Angular order `l ≥ 1` per root.                                     |
| `root_n`            | `[n_roots]` | `0.5`     | Radial order `n ≥ 1` per root (1 = lowest in window).               |
| `root_multiplicity` | `[n_roots]` | `0.5`     | Degeneracy `2l + 1` per root.                                       |
| `root_k`            | `[n_roots]` | `2e-9`    | Resonance positions `k` (the load-bearing payload).                 |
| `root_count_te`     | `[l_max]`   | `0.5`     | TE root count per `l = 1..l_max` after the `n_max` cap.             |
| `root_count_tm`     | `[l_max]`   | `0.5`     | TM root count per `l = 1..l_max` after the `n_max` cap.             |

The cross-check contract on `root_k` is **≤ 1e-10 relative** (issue
#170 acceptance). Schema v1 only carries absolute tolerances, so
`tolerance_abs = 2e-9` records the equivalent bound at the catalogue's
largest root (`k < 20`); the Rust harness applies the relative check
itself. Measured agreement on the shipped fixture is ≤ 4e-13 relative
(worst root TM(l=4, n=1)) — the residual is bisection-termination
noise, not Bessel-implementation drift.

## Bracketing parity (why root *counts* match structurally)

The SciPy reference replicates the Burn bracket walk exactly rather
than inventing its own:

- same window `k ∈ (0.1, 20.0]`, same 30 000-interval sampling grid;
- same pole-rejection heuristic (skip sign changes where both endpoint
  magnitudes exceed 1e8 — spurious flips across large excursions of
  the characteristic function);
- same consecutive-dedup at 1e-5 before the `n_max` cap.

Only the *refinement* differs (bisection vs `brentq`), so a root-count
disagreement per `(l, polarisation)` would localise a genuine
characteristic-function discrepancy, not a bracketing-policy delta.

## Reproduction

```sh
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_mie_roots_baseline.py
```

## Cross-check harness

The Rust-side cross-check lives at
`crates/geode-validation/tests/mie_roots_numpy_reference.rs`. It loads
this fixture and asserts:

1. Geometry constants (`n_inside`, `r_sphere`, `r_buffer`) match the
   Burn constants bit-exactly.
2. Root counts agree per `(l, polarisation)` window, with contiguous
   radial-order labelling `n = 1..count` on the Burn side.
3. `mie_roots_catalog(1.5, 4, 5)` joins against the fixture on
   `(pol, l, n)` with identical key sets and per-root relative error
   ≤ 1e-10.
4. `merged_roots(1.5, &[1, 2, 3, 4], R_SPHERE, R_BUFFER, 5)` produces
   the same root map as `mie_roots_catalog` and matches the fixture.
