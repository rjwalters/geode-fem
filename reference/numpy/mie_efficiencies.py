"""NumPy/SciPy reference for the analytic Mie scattering efficiencies
``Q_ext`` / ``Q_sca`` (issue #195, Epic #193 Phase 1).

Independent sidecar for ``crates/geode-core/src/mie_scattering.rs`` —
the analytic oracle of the driven Mie scattering benchmark
(``examples/mie_driven_scattering.rs``).

Implementation independence
===========================

The Rust side evaluates the Bohren & Huffman coefficient formulas
*directly*: Riccati-Bessel ``psi_l(mx)`` / ``psi_l'(mx)`` at the
dielectric argument via the hand-rolled (Miller-stabilized) spherical
Bessel ladders of ``geode_core::mie``. This module instead uses the
**BHMIE logarithmic-derivative algorithm** (Bohren & Huffman, Appendix
A): the interior solution enters only through

    D_n(mx) = psi_n'(mx) / psi_n(mx),

computed by *downward* recurrence

    D_{n-1} = n/(mx) - 1 / (D_n + n/(mx)),

seeded ``n_start = n_max + 15`` terms above the truncation order with
``D_{n_start} = 0``, and the coefficients assemble as

    a_n = [ (D_n/m + n/x) psi_n(x) - psi_{n-1}(x) ]
        / [ (D_n/m + n/x)  xi_n(x) -  xi_{n-1}(x) ]
    b_n = [ (m D_n + n/x) psi_n(x) - psi_{n-1}(x) ]
        / [ (m D_n + n/x)  xi_n(x) -  xi_{n-1}(x) ]

with ``psi``/``chi`` from ``scipy.special.spherical_jn`` /
``spherical_yn`` and ``xi = psi - i chi`` (``exp(-i omega t)``
convention; the efficiencies are real and convention-free). Agreement
with the Rust direct-formula evaluation therefore pins the mathematics
(the Mie series itself), not a shared algorithm or a shared Bessel
implementation.

Series truncation mirrors the Burn side: the Wiscombe criterion
``n_max = ceil(x + 4 x^{1/3} + 2)`` clamped below at 3
(``geode_core::mie_scattering::mie_series_order``).

Efficiencies (Bohren & Huffman eq. 4.61/4.62):

    Q_ext = (2/x^2) sum_n (2n+1) Re(a_n + b_n)
    Q_sca = (2/x^2) sum_n (2n+1) (|a_n|^2 + |b_n|^2)
"""

from __future__ import annotations

import numpy as np
from scipy.special import spherical_jn, spherical_yn

# Geometry / material constants — mirror the Burn side
# (`examples/mie_driven_scattering.rs`).
N_INSIDE = 1.5
R_SPHERE = 1.0

# Extra downward-recurrence headroom above the truncation order.
LOG_DERIVATIVE_HEADROOM = 15


def mie_series_order(x: float) -> int:
    """Wiscombe truncation order, mirror of ``mie_series_order``."""
    if x <= 0.0:
        raise ValueError("size parameter must be positive")
    return max(int(np.ceil(x + 4.0 * np.cbrt(x) + 2.0)), 3)


def log_derivative(m: float, x: float, n_max: int) -> np.ndarray:
    """``D_n(mx)`` for ``n = 1..n_max`` by downward recurrence."""
    mx = m * x
    n_start = n_max + LOG_DERIVATIVE_HEADROOM
    d = np.zeros(n_start + 1, dtype=np.complex128)
    for n in range(n_start, 0, -1):
        rn = n / mx
        d[n - 1] = rn - 1.0 / (d[n] + rn)
    return d[1 : n_max + 1]


def mie_a_b(m: float, x: float) -> tuple[np.ndarray, np.ndarray]:
    """BHMIE coefficients ``(a_n, b_n)``, ``n = 1..mie_series_order(x)``."""
    n_max = mie_series_order(x)
    n = np.arange(1, n_max + 1)

    jx = spherical_jn(n, x)
    yx = spherical_yn(n, x)
    jx_m1 = spherical_jn(n - 1, x)
    yx_m1 = spherical_yn(n - 1, x)

    psi = x * jx
    psi_m1 = x * jx_m1
    chi = -x * yx
    chi_m1 = -x * yx_m1
    xi = psi - 1j * chi
    xi_m1 = psi_m1 - 1j * chi_m1

    d = log_derivative(m, x, n_max)

    fa = d / m + n / x
    fb = m * d + n / x
    a = (fa * psi - psi_m1) / (fa * xi - xi_m1)
    b = (fb * psi - psi_m1) / (fb * xi - xi_m1)
    return a, b


def mie_efficiencies(m: float, x: float) -> tuple[float, float]:
    """``(Q_ext, Q_sca)`` for refractive index ``m``, size parameter ``x``."""
    a, b = mie_a_b(m, x)
    n = np.arange(1, a.size + 1)
    w = 2 * n + 1
    q_ext = (2.0 / x**2) * float(np.sum(w * (a + b).real))
    q_sca = (2.0 / x**2) * float(np.sum(w * (np.abs(a) ** 2 + np.abs(b) ** 2)))
    return q_ext, q_sca


def _self_check() -> None:
    """Internal sanity: Rayleigh limit and lossless unitarity."""
    # Rayleigh: Q_sca -> (8/3) x^4 |(m^2-1)/(m^2+2)|^2.
    m, x = N_INSIDE, 0.05
    _, q_sca = mie_efficiencies(m, x)
    pol = (m * m - 1.0) / (m * m + 2.0)
    q_ray = (8.0 / 3.0) * x**4 * pol * pol
    assert abs(q_sca - q_ray) / q_ray < 1e-2, (q_sca, q_ray)

    # Lossless: Q_ext == Q_sca and per-coefficient Re(c) == |c|^2.
    for x in (0.3, 1.0, 1.9, 3.0, 5.0):
        q_ext, q_sca = mie_efficiencies(m, x)
        assert abs(q_ext - q_sca) / q_ext < 1e-10, (x, q_ext, q_sca)
        a, b = mie_a_b(m, x)
        assert np.max(np.abs(a.real - np.abs(a) ** 2)) < 1e-10
        assert np.max(np.abs(b.real - np.abs(b) ** 2)) < 1e-10


if __name__ == "__main__":
    _self_check()
    print("ka      Q_ext        Q_sca")
    for x in (1.0, 1.5, 1.9, 2.4, 3.0):
        q_ext, q_sca = mie_efficiencies(N_INSIDE, x)
        print(f"{x:4.2f}  {q_ext:.9f}  {q_sca:.9f}")
