"""NumPy/SciPy reference for the analytic Mie root catalogue (issue #170).

Port of ``crates/geode-core/src/mie.rs`` — the analytic resonance roots
of a dielectric sphere inside a PEC cavity (the v0 Mie benchmark ground
truth). Pure functions: spherical Bessel ``j_l``/``y_l`` and
derivatives, Riccati-Bessel ``psi``/``chi``, the TE/TM characteristic
functions, and the dense-sampling + bracketing root finder. No mesh, no
eigensolve.

This is the backbone fixture for Epic #88 Phase J: every backend's FEM
eigenvalues get matched against this catalogue, so the catalogue itself
must be a cross-IR anchor first.

Physical setup (see the Rust module docstring for the full derivation):

- Inner sphere ``0 <= r <= R_s``: dielectric, refractive index ``n``.
- Vacuum buffer ``R_s <= r <= R_b``.
- PEC wall at ``r = R_b`` (closed cavity — purely real spectrum; this
  is *not* the open-space Mie scattering problem).

Implementation notes (Burn parity)
==================================

The Rust side hand-rolls the spherical Bessel ladder (upward recurrence
for ``l <= x + 1``, Miller's downward recurrence above); here we lean on
``scipy.special.spherical_jn`` / ``spherical_yn`` instead. Both are
accurate to ~1e-13 relative over the catalogue's argument range
(``x in [0.05, 30]``, ``l <= 4``), so root positions agree to the root-
finder tolerance rather than to Bessel-implementation noise.

The *bracketing* logic is replicated exactly — same ``k`` window, same
dense-sampling grid (30_000 intervals on ``(0.1, 20.0]``), same pole-
rejection heuristic (skip sign changes where both endpoint magnitudes
exceed 1e8), and the same consecutive-dedup at 1e-5 — so the two
catalogues see the same bracket set and root-count agreement per
``(l, polarisation)`` is structural, not accidental. Refinement uses
``scipy.optimize.brentq`` to near machine precision (the Rust side
bisects to ~1e-12 relative half-width), comfortably inside the 1e-10
relative cross-check tolerance.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
from scipy.optimize import brentq
from scipy.special import spherical_jn, spherical_yn

# ---------------------------------------------------------------------------
# Geometry / material constants — mirror the Burn side.
#
# `N_INSIDE` matches `crates/geode-core/examples/mie_sphere.rs::N_INSIDE`;
# `R_SPHERE` / `R_BUFFER` match `crates/geode-core/src/mesh/sphere.rs`.
# ---------------------------------------------------------------------------

N_INSIDE = 1.5
R_SPHERE = 1.0
R_BUFFER = 2.0

# Root-search window and sampling density — mirror
# `geode_core::mie::resonance_roots` exactly so the bracket set matches.
K_MIN = 0.1
K_MAX = 20.0
N_SAMPLES = 30_000

# Consecutive near-duplicate dedup tolerance (Rust: `dedup_by` at 1e-5).
DEDUP_TOL = 1e-5

# Pole-rejection scale (Rust: skip brackets where min(|fa|, |fb|) > 1e8 —
# spurious sign flips across large-magnitude excursions of the
# characteristic function).
POLE_SCALE_REJECT = 1e8

TE = "TE"
TM = "TM"


@dataclass(frozen=True)
class MieRoot:
    """One analytic resonance root (mirror of `geode_core::mie::MieRoot`)."""

    pol: str  # "TE" or "TM"
    l: int  # angular order, >= 1
    n: int  # radial order, >= 1 (1 = lowest root in the window)
    k: float  # resonance position
    multiplicity: int  # 2l + 1


# ---------------------------------------------------------------------------
# Riccati-Bessel functions (Bohren-Huffman convention, matching mie.rs).
# All accept scalar or ndarray `x` (scipy.special is vectorized).
# ---------------------------------------------------------------------------


def psi(l: int, x):
    """Riccati-Bessel ``psi_l(x) = x * j_l(x)``."""
    return x * spherical_jn(l, x)


def psi_prime(l: int, x):
    """Derivative ``psi_l'(x) = j_l(x) + x * j_l'(x)``."""
    return spherical_jn(l, x) + x * spherical_jn(l, x, derivative=True)


def chi(l: int, x):
    """Riccati-Bessel ``chi_l(x) = -x * y_l(x)``."""
    return -x * spherical_yn(l, x)


def chi_prime(l: int, x):
    """Derivative ``chi_l'(x) = -y_l(x) - x * y_l'(x)``."""
    return -spherical_yn(l, x) - x * spherical_yn(l, x, derivative=True)


# ---------------------------------------------------------------------------
# Characteristic functions — direct transliteration of
# `characteristic_te` / `characteristic_tm` in mie.rs.
# ---------------------------------------------------------------------------


def characteristic_te(n: float, l: int, r_s: float, r_b: float, k):
    """TE characteristic function; zeros in ``k`` are TE resonances.

    Buffer coefficients up to overall scale: ``A = chi(x_b)``,
    ``B = psi(x_b)`` so that ``A psi(x_b) - B chi(x_b) = 0`` (PEC,
    E_theta = 0 at the wall) with no spurious pole when
    ``chi(x_b) -> 0``. Matching at ``r = R_s``:
    ``psi(x_in) buf' - psi'(x_in)/n * buf = 0``.
    """
    x_in = n * k * r_s
    x_s = k * r_s
    x_b = k * r_b

    big_a = chi(l, x_b)
    big_b = psi(l, x_b)

    buf = big_a * psi(l, x_s) - big_b * chi(l, x_s)
    buf_prime = big_a * psi_prime(l, x_s) - big_b * chi_prime(l, x_s)

    return psi(l, x_in) * buf_prime - (psi_prime(l, x_in) / n) * buf


def characteristic_tm(n: float, l: int, r_s: float, r_b: float, k):
    """TM characteristic function; zeros in ``k`` are TM resonances.

    TM PEC condition is ``psi'(x_b) = 0``, so ``A = chi'(x_b)``,
    ``B = psi'(x_b)``; the magnetic matching picks up the permittivity
    factor: ``psi(x_in) buf' - n psi'(x_in) buf = 0``.
    """
    x_in = n * k * r_s
    x_s = k * r_s
    x_b = k * r_b

    big_a = chi_prime(l, x_b)
    big_b = psi_prime(l, x_b)

    buf = big_a * psi(l, x_s) - big_b * chi(l, x_s)
    buf_prime = big_a * psi_prime(l, x_s) - big_b * chi_prime(l, x_s)

    return psi(l, x_in) * buf_prime - n * psi_prime(l, x_in) * buf


# ---------------------------------------------------------------------------
# Root finding — same dense-sampling bracket walk as mie.rs::find_roots,
# with brentq refinement instead of 60-step bisection.
# ---------------------------------------------------------------------------


def find_roots(f, k_min: float, k_max: float, n_samples: int) -> list[float]:
    """Real roots of ``f`` on ``[k_min, k_max]``.

    Dense sampling on the same grid as the Rust side (``n_samples``
    intervals, endpoints included), sign-change bracketing with the same
    pole-rejection heuristic, then ``brentq`` refinement.
    """
    assert k_max > k_min
    assert n_samples >= 3

    dk = (k_max - k_min) / n_samples
    ks = k_min + dk * np.arange(n_samples + 1)
    fs = f(ks)

    roots: list[float] = []
    for i in range(n_samples):
        a, b = ks[i], ks[i + 1]
        fa, fb = fs[i], fs[i + 1]
        if not (np.isfinite(fa) and np.isfinite(fb)):
            continue
        if fa == 0.0 and fb == 0.0:
            continue
        if fa * fb > 0.0:
            continue
        # Reject brackets where the *magnitude* on both sides is
        # enormous — spurious sign flips across large excursions of the
        # characteristic function (mirror of the Rust 1e8 heuristic).
        if min(abs(fa), abs(fb)) > POLE_SCALE_REJECT:
            continue
        roots.append(
            float(
                brentq(
                    lambda k: float(f(np.asarray(k))),
                    float(a),
                    float(b),
                    xtol=1e-14,
                    rtol=4.0 * np.finfo(float).eps,
                    maxiter=200,
                )
            )
        )
    return roots


def _dedup_consecutive(values: list[float], tol: float) -> list[float]:
    """Mirror of Rust ``Vec::dedup_by`` with ``|a - b| < tol``: drop a
    value when it is within ``tol`` of the previously *retained* one."""
    out: list[float] = []
    for v in values:
        if out and abs(v - out[-1]) < tol:
            continue
        out.append(v)
    return out


def resonance_roots(
    pol: str,
    n: float,
    l: int,
    r_s: float,
    r_b: float,
    n_max: int,
) -> list[MieRoot]:
    """Lowest ``n_max`` resonance roots for one ``(l, polarisation)``
    channel — mirror of ``geode_core::mie::resonance_roots``."""
    assert n > 0.0
    assert l >= 1
    assert r_b > r_s
    assert pol in (TE, TM)

    if pol == TE:
        f = lambda k: characteristic_te(n, l, r_s, r_b, k)  # noqa: E731
    else:
        f = lambda k: characteristic_tm(n, l, r_s, r_b, k)  # noqa: E731

    raw = find_roots(f, K_MIN, K_MAX, N_SAMPLES)
    raw = _dedup_consecutive(raw, DEDUP_TOL)

    return [
        MieRoot(pol=pol, l=l, n=idx + 1, k=k, multiplicity=2 * l + 1)
        for idx, k in enumerate(raw[:n_max])
    ]


def merged_roots(
    n: float,
    l_set: list[int],
    r_s: float,
    r_b: float,
    n_max: int,
) -> list[MieRoot]:
    """Lowest ``n_max`` TE and TM roots for ``l in l_set``, merged and
    sorted by ascending ``k`` (mirror of ``geode_core::mie::merged_roots``)."""
    all_roots: list[MieRoot] = []
    for l in l_set:
        all_roots.extend(resonance_roots(TE, n, l, r_s, r_b, n_max))
        all_roots.extend(resonance_roots(TM, n, l, r_s, r_b, n_max))
    all_roots.sort(key=lambda r: r.k)
    return all_roots


def mie_roots_catalog(m: float, l_max: int, n_max: int) -> list[MieRoot]:
    """Extended catalogue: lowest ``n_max`` roots for every
    ``(l, polarisation)`` with ``l in [1, l_max]``, sorted globally by
    ascending ``k`` — mirror of ``geode_core::mie::mie_roots_catalog``
    on the bundled fixture geometry (``R_SPHERE``, ``R_BUFFER``)."""
    assert m > 0.0
    assert l_max >= 1
    assert n_max >= 1
    return merged_roots(m, list(range(1, l_max + 1)), R_SPHERE, R_BUFFER, n_max)


# ---------------------------------------------------------------------------
# Self-checks (run `python3 mie_roots.py` directly).
# ---------------------------------------------------------------------------


def _self_check() -> None:
    # n -> 1 vacuum limit: TE characteristic reduces to psi_l(k R_b) = 0,
    # i.e. j_l(k R_b) = 0. First zero of j_1 is at x ~ 4.4934.
    roots = resonance_roots(TE, 1.0, 1, 0.5, 1.0, 1)
    assert roots, "vacuum-limit TE l=1 root not found"
    assert abs(roots[0].k - 4.4934) < 1e-2, roots[0]

    # Catalogue extent: 2 pol x l_max x n_max entries, globally sorted.
    cat = mie_roots_catalog(1.5, 3, 3)
    assert len(cat) == 2 * 3 * 3, len(cat)
    assert all(c.k > 0.0 and np.isfinite(c.k) for c in cat)
    assert all(c.multiplicity == 2 * c.l + 1 for c in cat)
    assert all(a.k <= b.k for a, b in zip(cat, cat[1:]))
    print("mie_roots.py self-check OK")


if __name__ == "__main__":
    _self_check()
