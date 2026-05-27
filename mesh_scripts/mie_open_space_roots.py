"""Offline reference root-finder for open-space Mie WGM resonances.

Solves for the complex resonance positions `k = Re(k) - i Im(k)` of a
homogeneous dielectric sphere of refractive index `n`, radius `R_s`, in
open space (radiation BC at infinity). These are the genuine
whispering-gallery modes the FEM-with-PML setup in
`crates/geode-core/examples/mie_sphere.rs` is trying to reproduce.

The PEC-cavity catalog already shipped in `crates/geode-core/src/mie.rs`
is the `sigma_0 -> 0` closed-shell limit. This script (and the Rust
companion in `mie_open.rs`) extends to the genuinely radiative case.

Physics
-------

For a non-magnetic sphere (mu_r = 1) of radius `R_s` and refractive
index `n` immersed in vacuum, the TE and TM Mie scattering coefficients
have poles at the resonance positions `k`. With Riccati-Bessel
`psi_l(x) = x j_l(x)` (regular) and `xi_l(x) = x h_l^(1)(x)`
(outgoing), and size parameter `x = k R_s`:

  TE pole (b_l):  psi_l(nx)   * xi_l'(x) - (1/n) psi_l'(nx) * xi_l(x)  = 0
  TM pole (a_l):  psi_l(nx)   * xi_l'(x) -    n  psi_l'(nx) * xi_l(x)  = 0

These exactly match the PEC-cavity equations in `mie.rs` after the
replacement `chi_l(k R_b) <-> 0`, `psi_l(k R_b) -> outgoing
h_l^(1)(k R_s)` (i.e., move the outer wall to infinity).

The roots are complex with Re(k) > 0 (oscillation) and Im(k) > 0 in our
sign convention `exp(-i omega t)` (radiative decay). The PEC roots are
real and lie close to the open-space `Re(k)`, so they make good Newton
seeds.

Output
------

Emits a Rust `const`-array literal of `(MiePolarisation, l, n,
Complex64)` entries, ready to paste into
`crates/geode-core/src/mie_open.rs`.

Run:
    python3 mesh_scripts/mie_open_space_roots.py

Requires: numpy, scipy.
"""

from __future__ import annotations

import cmath
import math
import sys
from dataclasses import dataclass

import numpy as np
from scipy.special import spherical_jn, spherical_yn  # real arg only


# -----------------------------------------------------------------------------
# Complex spherical Bessel / Hankel via upward recurrence.
# -----------------------------------------------------------------------------


def sph_jn_complex(l: int, z: complex) -> complex:
    """Spherical Bessel j_l(z), complex z, upward recurrence."""
    if abs(z) < 1e-14:
        return 1.0 + 0j if l == 0 else 0.0 + 0j
    s = cmath.sin(z)
    c = cmath.cos(z)
    j0 = s / z
    if l == 0:
        return j0
    j1 = s / (z * z) - c / z
    if l == 1:
        return j1
    prev, curr = j0, j1
    for k in range(2, l + 1):
        nxt = (2 * k - 1) / z * curr - prev
        prev, curr = curr, nxt
    return curr


def sph_yn_complex(l: int, z: complex) -> complex:
    """Spherical Bessel y_l(z), complex z, upward recurrence."""
    s = cmath.sin(z)
    c = cmath.cos(z)
    y0 = -c / z
    if l == 0:
        return y0
    y1 = -c / (z * z) - s / z
    if l == 1:
        return y1
    prev, curr = y0, y1
    for k in range(2, l + 1):
        nxt = (2 * k - 1) / z * curr - prev
        prev, curr = curr, nxt
    return curr


def sph_h1_complex(l: int, z: complex) -> complex:
    """Spherical Hankel of the first kind: h_l^(1)(z) = j_l(z) + i y_l(z)."""
    return sph_jn_complex(l, z) + 1j * sph_yn_complex(l, z)


def sph_jn_prime_complex(l: int, z: complex) -> complex:
    """j_l'(z) = j_{l-1}(z) - (l+1)/z * j_l(z)."""
    if l == 0:
        return -sph_jn_complex(1, z)
    return sph_jn_complex(l - 1, z) - (l + 1) / z * sph_jn_complex(l, z)


def sph_h1_prime_complex(l: int, z: complex) -> complex:
    """h_l^(1)'(z) = h_{l-1}^(1)(z) - (l+1)/z * h_l^(1)(z)."""
    if l == 0:
        return -sph_h1_complex(1, z)
    return sph_h1_complex(l - 1, z) - (l + 1) / z * sph_h1_complex(l, z)


# -----------------------------------------------------------------------------
# Riccati-Bessel wrappers.
# -----------------------------------------------------------------------------


def psi(l: int, z: complex) -> complex:
    return z * sph_jn_complex(l, z)


def psi_prime(l: int, z: complex) -> complex:
    return sph_jn_complex(l, z) + z * sph_jn_prime_complex(l, z)


def xi(l: int, z: complex) -> complex:
    return z * sph_h1_complex(l, z)


def xi_prime(l: int, z: complex) -> complex:
    return sph_h1_complex(l, z) + z * sph_h1_prime_complex(l, z)


# -----------------------------------------------------------------------------
# Characteristic functions (open-space).
# -----------------------------------------------------------------------------


def char_te(n: float, l: int, r_s: float, k: complex) -> complex:
    """TE resonance condition (pole of Mie b_l coefficient).

    psi_l(nx) * xi_l'(x) - (1/n) psi_l'(nx) * xi_l(x) = 0,  x = k R_s.
    """
    x_in = n * k * r_s
    x_s = k * r_s
    return psi(l, x_in) * xi_prime(l, x_s) - (1.0 / n) * psi_prime(l, x_in) * xi(l, x_s)


def char_tm(n: float, l: int, r_s: float, k: complex) -> complex:
    """TM resonance condition (pole of Mie a_l coefficient).

    psi_l(nx) * xi_l'(x) - n psi_l'(nx) * xi_l(x) = 0,  x = k R_s.
    """
    x_in = n * k * r_s
    x_s = k * r_s
    return psi(l, x_in) * xi_prime(l, x_s) - n * psi_prime(l, x_in) * xi(l, x_s)


# -----------------------------------------------------------------------------
# PEC-cavity real roots (for Newton seeds), mirrored from Rust mie.rs.
# -----------------------------------------------------------------------------


def psi_real(l: int, x: float) -> float:
    return x * spherical_jn(l, x)


def psi_prime_real(l: int, x: float) -> float:
    return spherical_jn(l, x) + x * spherical_jn(l, x, derivative=True)


def chi_real(l: int, x: float) -> float:
    return -x * spherical_yn(l, x)


def chi_prime_real(l: int, x: float) -> float:
    return -spherical_yn(l, x) - x * spherical_yn(l, x, derivative=True)


def pec_char_te(n: float, l: int, r_s: float, r_b: float, k: float) -> float:
    x_in = n * k * r_s
    x_s = k * r_s
    x_b = k * r_b
    big_a = chi_real(l, x_b)
    big_b = psi_real(l, x_b)
    buf = big_a * psi_real(l, x_s) - big_b * chi_real(l, x_s)
    buf_p = big_a * psi_prime_real(l, x_s) - big_b * chi_prime_real(l, x_s)
    return psi_real(l, x_in) * buf_p - (psi_prime_real(l, x_in) / n) * buf


def pec_char_tm(n: float, l: int, r_s: float, r_b: float, k: float) -> float:
    x_in = n * k * r_s
    x_s = k * r_s
    x_b = k * r_b
    big_a = chi_prime_real(l, x_b)
    big_b = psi_prime_real(l, x_b)
    buf = big_a * psi_real(l, x_s) - big_b * chi_real(l, x_s)
    buf_p = big_a * psi_prime_real(l, x_s) - big_b * chi_prime_real(l, x_s)
    return psi_real(l, x_in) * buf_p - n * psi_prime_real(l, x_in) * buf


def pec_roots_real(
    pol: str, n: float, l: int, r_s: float, r_b: float, n_max: int
) -> list[float]:
    """Bracket+bisect on the real axis; same algorithm as Rust find_roots."""
    f = pec_char_te if pol == "TE" else pec_char_tm
    samples = 5_000
    ks = np.linspace(0.1, 20.0, samples)
    fs = np.array([f(n, l, r_s, r_b, k) for k in ks])

    roots: list[float] = []
    for i in range(samples - 1):
        fa, fb = fs[i], fs[i + 1]
        if not (np.isfinite(fa) and np.isfinite(fb)):
            continue
        if fa * fb > 0:
            continue
        if min(abs(fa), abs(fb)) > 1e8:
            continue  # spurious sign flip across pole
        # Bisect.
        lo, hi, f_lo = ks[i], ks[i + 1], fa
        for _ in range(60):
            mid = 0.5 * (lo + hi)
            f_mid = f(n, l, r_s, r_b, mid)
            if not np.isfinite(f_mid):
                break
            if abs(hi - lo) < 1e-12 * max(abs(mid), 1.0) or f_mid == 0.0:
                lo = hi = mid
                break
            if f_lo * f_mid < 0.0:
                hi = mid
            else:
                lo = mid
                f_lo = f_mid
        roots.append(0.5 * (lo + hi))

    # Dedup near-coincident roots and take lowest n_max.
    dedup: list[float] = []
    for r in sorted(roots):
        if not dedup or abs(r - dedup[-1]) > 1e-5:
            dedup.append(r)
    return dedup[:n_max]


# -----------------------------------------------------------------------------
# Complex Newton iteration.
# -----------------------------------------------------------------------------


def newton_complex(
    f, k0: complex, max_iter: int = 50, tol: float = 1e-12, h: float = 1e-6
) -> tuple[complex, float, int]:
    """Newton-Raphson with central-difference derivative for an analytic f.

    Returns (root, residual_abs, iters).
    """
    k = k0
    res = abs(f(k))
    for it in range(max_iter):
        fk = f(k)
        fkp = (f(k + h) - f(k - h)) / (2.0 * h)
        if abs(fkp) < 1e-30:
            break
        dk = fk / fkp
        k_new = k - dk
        res_new = abs(f(k_new))
        # Backtracking line search to keep |f| monotone (helps with
        # high-l roots where the seed is poor).
        alpha = 1.0
        while res_new > res and alpha > 1e-4:
            alpha *= 0.5
            k_new = k - alpha * dk
            res_new = abs(f(k_new))
        k, res = k_new, res_new
        if res < tol:
            return k, res, it + 1
        if abs(dk) < 1e-14 * max(abs(k), 1.0):
            break
    return k, res, max_iter


# -----------------------------------------------------------------------------
# Main: tabulate open-space WGM roots.
# -----------------------------------------------------------------------------


@dataclass
class Root:
    pol: str
    l: int
    n_radial: int
    k: complex
    residual: float


def tabulate(n: float, r_s: float, l_max: int, n_max: int) -> list[Root]:
    """For each (pol, l), seed a grid of complex starts spanning the
    expected resonance band and capture all distinct converged roots up
    to `n_max` ordered by ascending Re(k).

    The seeding grid uses PEC-cavity real roots for several `r_b` values
    (each "cavity" pushes the PEC seed closer to one of the open-space
    poles), plus an explicit complex-plane grid that catches roots PEC
    seeds miss when the cavity bracket is poorly tuned.

    Sign convention: we keep the root that has `Im(k) < 0`, i.e. the
    pole of the Mie scattering coefficient with `exp(+i k r)` outgoing
    wave under `exp(-i omega t)` time dependence — equivalently `omega
    = c k` with `Im(omega) < 0` (radiative decay). The Rust catalog
    documents this and the FEM-side comparison takes `|Im(k)|`.
    """
    out: list[Root] = []

    for l in range(1, l_max + 1):
        for pol in ("TE", "TM"):
            f = (lambda kc, pol=pol, l=l: char_te(n, l, r_s, kc)) if pol == "TE" \
                else (lambda kc, pol=pol, l=l: char_tm(n, l, r_s, kc))

            # Build seed grid.
            # (a) PEC seeds for r_b in {1.5, 2.0, 3.0, 5.0, 10.0}: each
            #     r_b gives a slightly different real-axis position for
            #     the same physical pole. The wider buffer pushes the
            #     PEC root closer to the open-space limit.
            seed_set: list[complex] = []
            for rb in (1.5, 2.0, 3.0, 5.0, 10.0):
                if rb <= r_s:
                    continue
                reals = pec_roots_real(pol, n, l, r_s, rb, n_max * 3)
                # Seed with a small negative Im(k) (decay convention).
                for k_r in reals:
                    if 0.2 <= k_r <= 18.0:
                        seed_set.append(complex(k_r, -0.05 * max(k_r, 1.0)))
            # (b) Coarse complex-plane grid to catch poles PEC seeds
            #     miss (especially when the cavity bracket is far from
            #     the actual Mie pole position).
            for k_r in np.arange(0.5, 18.0, 0.25):
                for k_i in (-0.05, -0.2, -0.5, -1.0):
                    seed_set.append(complex(k_r, k_i))

            # Newton from each seed; collect unique converged roots.
            roots_here: list[complex] = []
            for k0 in seed_set:
                k, res, _ = newton_complex(f, k0)
                if res > 1e-8:
                    continue
                if k.real <= 0.1 or k.real > 20.0:
                    continue
                # We want the Im(k) < 0 pole (outgoing-wave convention).
                # If Newton converged to its conjugate, mirror it back —
                # the characteristic equation has real coefficients in
                # the limit n -> real, so conjugates are paired.
                if k.imag > 0:
                    k = complex(k.real, -abs(k.imag))
                if k.imag > -1e-6:
                    # Effectively real — likely a spurious converged
                    # branch (no radiative decay). Skip.
                    continue
                # Dedup against already-collected roots.
                is_new = True
                for existing in roots_here:
                    if abs(k - existing) < 1e-3 * max(abs(k), 1.0):
                        is_new = False
                        break
                if is_new:
                    roots_here.append(k)

            # Sort by ascending Re(k) and take lowest n_max.
            roots_here.sort(key=lambda c: c.real)
            for idx, k in enumerate(roots_here[:n_max]):
                res = abs(f(k))
                out.append(
                    Root(pol=pol, l=l, n_radial=idx + 1, k=k, residual=res)
                )

    out.sort(key=lambda r: r.k.real)
    return out


def emit_rust(roots: list[Root], n: float, r_s: float) -> str:
    """Emit a Rust `&[(MiePolarisation, usize, usize, c64)]` literal.

    Caller is expected to paste this into a `pub const` definition.
    """
    lines = []
    lines.append(
        f"// Open-space Mie WGM roots for n = {n}, R_s = {r_s}, vacuum surround."
    )
    lines.append("// Generated by mesh_scripts/mie_open_space_roots.py.")
    lines.append(
        "// Each entry: (polarisation, l, n_radial, k = Re(k) + i Im(k))."
    )
    lines.append("// Sign convention: outgoing wave h_l^(1)(k r) ~ exp(+i k r)/r")
    lines.append("// under exp(-i omega t) time dependence. The Mie poles sit at")
    lines.append("// Im(k) < 0, i.e. the second Riemann sheet of the resolvent;")
    lines.append("// |Im(k)| is the radiative decay rate (linewidth in k).")
    lines.append("// FEM-side comparison should match on (Re(k), |Im(k)|).")
    lines.append("")
    for r in roots:
        lines.append(
            f"// {r.pol}_{r.l},{r.n_radial}: k = "
            f"{r.k.real:.10e} + {r.k.imag:.10e}i  (res = {r.residual:.2e})"
        )
    lines.append("")
    return "\n".join(lines)


def main():
    n = 1.5
    r_s = 1.0
    l_max = 5
    n_max = 3

    print(
        f"# Tabulating open-space Mie WGM roots: n={n}, R_s={r_s}, "
        f"l in [1, {l_max}], lowest {n_max} radial orders per (l, pol).",
        file=sys.stderr,
    )
    print(
        "# Seeds: PEC roots for r_b in {1.5, 2.0, 3.0, 5.0, 10.0} + complex grid.",
        file=sys.stderr,
    )

    roots = tabulate(n, r_s, l_max, n_max)

    print(f"# Found {len(roots)} converged roots.", file=sys.stderr)
    print(file=sys.stderr)

    # Pretty-print table to stderr.
    print(
        f"{'mode':>8}  {'Re(k)':>14}  {'Im(k)':>14}  {'|res|':>10}  {'Q':>10}",
        file=sys.stderr,
    )
    print("-" * 64, file=sys.stderr)
    for r in roots:
        q = r.k.real / (2.0 * abs(r.k.imag)) if abs(r.k.imag) > 1e-30 else math.inf
        print(
            f"{r.pol}_{r.l},{r.n_radial:>2}  {r.k.real:>14.10f}  "
            f"{r.k.imag:>14.10e}  {r.residual:>10.2e}  {q:>10.3e}",
            file=sys.stderr,
        )

    # Rust comment block to stdout for easy redirection.
    print(emit_rust(roots, n, r_s))


if __name__ == "__main__":
    main()
