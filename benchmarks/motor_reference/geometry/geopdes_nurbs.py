"""Independent reader and exact evaluator for GeoPDEs "nurbs mesh v.2.1" exports.

Written for GEODE from the published file layout, without reference to the
upstream MATLAB/Octave readers. The input is treated as untrusted data: every
count is checked against what is actually present and every token must parse
as a number.

Layout (2-D parametric, 2-D physical, as used by the ICEM exports)::

    # comment lines
    NDIM RDIM NPATCH NINTERFACE NSUBDOMAIN
    PATCH k
    p_u p_v                      polynomial degree per direction
    n_u n_v                      control points per direction
    knots_u                      n_u + p_u + 1 values
    knots_v                      n_v + p_v + 1 values
    x_0 ... x_{n_u n_v - 1}      control-point x (times weight), u fastest
    y_0 ...                      control-point y (times weight)
    w_0 ...                      weights
    ...
    INTERFACE k / patch side / patch side / orientation
    BOUNDARY k / nsides / (patch side) * nsides

Whether the coordinate rows are Cartesian or weight-premultiplied is not
stated in the file; :func:`check_coordinate_convention` decides it from the
data (exported circular arcs are exact circles only under one reading).

Side numbering (GeoPDEs 2-D convention): 1 is u=0, 2 is u=1, 3 is v=0,
4 is v=1.
"""

from dataclasses import dataclass, field
from pathlib import Path

import numpy as np


@dataclass
class Patch:
    index: int  # one-based, as in the file
    degree: tuple
    ncp: tuple
    knots: tuple  # (knots_u, knots_v) numpy arrays
    cp: np.ndarray  # shape (n_v, n_u, 2), Cartesian
    w: np.ndarray  # shape (n_v, n_u)


@dataclass
class Multipatch:
    patches: list
    interfaces: list  # (ifc_id, (patch, side), (patch, side), orientation)
    boundaries: list  # (bnd_id, [(patch, side), ...])
    header: tuple = field(default=())


def _numbers(line, count, what):
    toks = line.split()
    if len(toks) != count:
        raise ValueError(f"{what}: expected {count} numbers, got {len(toks)}")
    return [float(t) for t in toks]


def _ints(line, count, what):
    vals = _numbers(line, count, what)
    out = [int(v) for v in vals]
    if any(o != v for o, v in zip(out, vals)):
        raise ValueError(f"{what}: non-integer value")
    return out


def read_multipatch(path, premultiplied=True):
    """Parse a GeoPDEs v2.1 2-D multipatch export.

    ``premultiplied=True`` (default) reads coordinate rows as w*x
    (homogeneous) and divides by the weight; ``False`` reads them as
    Cartesian. The ICEM exports are premultiplied: under that reading the
    exported rotor arc is a circle to 3e-16 m, under the Cartesian reading it
    is off by 1.5 mm (see :func:`check_coordinate_convention`).
    """
    raw = Path(path).read_text(encoding="ascii")
    lines = [l.strip() for l in raw.splitlines()]
    lines = [l for l in lines if l and not l.startswith("#")]
    pos = 0

    def take():
        nonlocal pos
        if pos >= len(lines):
            raise ValueError("unexpected end of file")
        pos += 1
        return lines[pos - 1]

    ndim, rdim, npatch, nifc, nsub = _ints(take(), 5, "header")
    if (ndim, rdim) != (2, 2):
        raise ValueError(f"only 2-D/2-D exports supported, got {ndim}/{rdim}")
    patches = []
    for k in range(1, npatch + 1):
        tag = take().split()
        if tag != ["PATCH", str(k)]:
            raise ValueError(f"expected PATCH {k}, got {tag}")
        pu, pv = _ints(take(), 2, f"patch {k} degree")
        nu, nv = _ints(take(), 2, f"patch {k} ncp")
        ku = np.array(_numbers(take(), nu + pu + 1, f"patch {k} knots u"))
        kv = np.array(_numbers(take(), nv + pv + 1, f"patch {k} knots v"))
        n = nu * nv
        x = np.array(_numbers(take(), n, f"patch {k} x"))
        y = np.array(_numbers(take(), n, f"patch {k} y"))
        w = np.array(_numbers(take(), n, f"patch {k} w"))
        if np.any(w <= 0):
            raise ValueError(f"patch {k}: non-positive weight")
        for kn in (ku, kv):
            if np.any(np.diff(kn) < 0):
                raise ValueError(f"patch {k}: decreasing knot vector")
        if premultiplied:
            x, y = x / w, y / w
        cp = np.stack([x, y], axis=-1).reshape(nv, nu, 2)
        patches.append(Patch(k, (pu, pv), (nu, nv), (ku, kv), cp, w.reshape(nv, nu)))
    interfaces = []
    for k in range(1, nifc + 1):
        tag = take().split()
        if tag != ["INTERFACE", str(k)]:
            raise ValueError(f"expected INTERFACE {k}, got {tag}")
        a = tuple(_ints(take(), 2, "interface side"))
        b = tuple(_ints(take(), 2, "interface side"))
        flag = _ints(take(), 1, "interface orientation")[0]
        interfaces.append((k, a, b, flag))
    boundaries = []
    k = 0
    while pos < len(lines):
        tag = take().split()
        k += 1
        if tag != ["BOUNDARY", str(k)]:
            raise ValueError(f"expected BOUNDARY {k}, got {tag}")
        ns = _ints(take(), 1, "boundary nsides")[0]
        sides = [tuple(_ints(take(), 2, "boundary side")) for _ in range(ns)]
        boundaries.append((k, sides))
    if nsub != 0:
        raise ValueError("subdomain records are not supported")
    return Multipatch(patches, interfaces, boundaries, (ndim, rdim, npatch, nifc, nsub))


# --------------------------------------------------------------------------
# Rational B-spline curves
# --------------------------------------------------------------------------


@dataclass
class NurbsCurve:
    degree: int
    knots: np.ndarray
    cp: np.ndarray  # (n, 2) Cartesian
    w: np.ndarray  # (n,)

    def reversed(self):
        a, b = self.knots[0], self.knots[-1]
        return NurbsCurve(self.degree, (a + b - self.knots)[::-1].copy(),
                          self.cp[::-1].copy(), self.w[::-1].copy())


def side_curve(patch, side):
    """The boundary curve of ``patch`` on GeoPDEs ``side`` (exact NURBS)."""
    pu, pv = patch.degree
    ku, kv = patch.knots
    if side == 1:
        return NurbsCurve(pv, kv, patch.cp[:, 0], patch.w[:, 0])
    if side == 2:
        return NurbsCurve(pv, kv, patch.cp[:, -1], patch.w[:, -1])
    if side == 3:
        return NurbsCurve(pu, ku, patch.cp[0, :], patch.w[0, :])
    if side == 4:
        return NurbsCurve(pu, ku, patch.cp[-1, :], patch.w[-1, :])
    raise ValueError(f"bad side {side}")


def _find_span(knots, p, t):
    n = len(knots) - p - 1  # number of basis functions
    if t >= knots[n]:
        return n - 1
    lo, hi = p, n
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if t < knots[mid]:
            hi = mid
        else:
            lo = mid
    return lo


def _basis_and_deriv(knots, p, span, t):
    """Nonzero B-spline basis values and first derivatives at t (Cox-de Boor)."""
    # Triangular table of basis functions of increasing degree.
    left = np.zeros(p + 1)
    right = np.zeros(p + 1)
    ndu = np.zeros((p + 1, p + 1))
    ndu[0, 0] = 1.0
    for j in range(1, p + 1):
        left[j] = t - knots[span + 1 - j]
        right[j] = knots[span + j] - t
        saved = 0.0
        for r in range(j):
            ndu[j, r] = right[r + 1] + left[j - r]  # knot difference
            temp = ndu[r, j - 1] / ndu[j, r]
            ndu[r, j] = saved + right[r + 1] * temp
            saved = left[j - r] * temp
        ndu[j, j] = saved
    vals = ndu[:, p].copy()
    ders = np.zeros(p + 1)
    if p >= 1:
        # derivative from degree p-1 functions: N'_{i,p} = p( N_{i,p-1}/(u_{i+p}-u_i)
        #                                               - N_{i+1,p-1}/(u_{i+p+1}-u_{i+1}) )
        lower = ndu[:p, p - 1]  # N_{span-p+1 .. span, p-1}
        for r in range(p + 1):
            i = span - p + r
            d = 0.0
            if r >= 1:
                den = knots[i + p] - knots[i]
                if den > 0:
                    d += lower[r - 1] / den
            if r <= p - 1:
                den = knots[i + p + 1] - knots[i + 1]
                if den > 0:
                    d -= lower[r] / den
            ders[r] = p * d
    return vals, ders


def eval_curve(c, ts):
    """Points C(t) and derivatives C'(t) of a rational curve, shape (m, 2)."""
    ts = np.atleast_1d(np.asarray(ts, dtype=float))
    pts = np.empty((len(ts), 2))
    der = np.empty((len(ts), 2))
    p = c.degree
    hw = c.cp * c.w[:, None]
    for k, t in enumerate(ts):
        span = _find_span(c.knots, p, t)
        nvals, nders = _basis_and_deriv(c.knots, p, span, t)
        idx = slice(span - p, span + 1)
        A = nvals @ hw[idx]
        W = nvals @ c.w[idx]
        dA = nders @ hw[idx]
        dW = nders @ c.w[idx]
        pts[k] = A / W
        der[k] = (dA - pts[k] * dW) / W
    return pts, der


def knot_spans(c):
    u = np.unique(c.knots[c.degree:len(c.knots) - c.degree])
    return list(zip(u[:-1], u[1:]))


def gauss_legendre(n):
    return np.polynomial.legendre.leggauss(n)


def green_area_integral(c, nq=12):
    """Exact-to-quadrature 1/2 ∮ (x dy - y dx) contribution of a curve.

    Gauss-Legendre per knot span; the rational integrand is smooth within a
    span, so the error falls exponentially with ``nq``.
    """
    x, wq = gauss_legendre(nq)
    total = 0.0
    for a, b in knot_spans(c):
        t = 0.5 * (b - a) * x + 0.5 * (a + b)
        P, D = eval_curve(c, t)
        total += 0.5 * (b - a) * np.sum(wq * 0.5 * (P[:, 0] * D[:, 1] - P[:, 1] * D[:, 0]))
    return total


def curve_length(c, nq=12):
    x, wq = gauss_legendre(nq)
    total = 0.0
    for a, b in knot_spans(c):
        t = 0.5 * (b - a) * x + 0.5 * (a + b)
        _, D = eval_curve(c, t)
        total += 0.5 * (b - a) * np.sum(wq * np.hypot(D[:, 0], D[:, 1]))
    return total


def patch_boundary_loop(patch):
    """Counter-clockwise-in-parameter boundary: side 3, side 2, side 4 rev, side 1 rev."""
    return [side_curve(patch, 3), side_curve(patch, 2),
            side_curve(patch, 4).reversed(), side_curve(patch, 1).reversed()]


def patch_area(patch, nq=12):
    """Signed area via Green's theorem on the four exact boundary curves."""
    return sum(green_area_integral(c, nq) for c in patch_boundary_loop(patch))


def check_coordinate_convention(path, arc_tests):
    """Return the max radial deviation of named arcs under both readings.

    ``arc_tests`` is a list of ``(patch, side, radius)`` expected to be exact
    circles about the origin.
    """
    out = {}
    for pre in (False, True):
        mp = read_multipatch(path, premultiplied=pre)
        dev = 0.0
        for pidx, side, radius in arc_tests:
            c = side_curve(mp.patches[pidx - 1], side)
            P, _ = eval_curve(c, np.linspace(c.knots[0], c.knots[-1], 401))
            dev = max(dev, float(np.max(np.abs(np.hypot(P[:, 0], P[:, 1]) - radius))))
        out["premultiplied" if pre else "cartesian"] = dev
    return out
