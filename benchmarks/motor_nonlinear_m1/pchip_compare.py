"""Measured difference between GEODE's TabulatedBhLaw and the reference
code's MATLAB pchip on the M27 B-H table (issue #1160).

The reference (ICEMsnapshot, material/Mat_Magnetic.m at the commit in
benchmarks/motor_reference/sources.json) evaluates the iron as follows:

* ``HBspline = pchip(B, H)`` and ``HBsplineDer = fnder(HBspline)``;
* ``getMuNonlinear``: ``B(B<=1e-5) = 1e-5``, then ``mu = B/ppval(HBspline, B)``,
  and for ``B > B_end``, ``mu = B/((B - B_end)/mu0 + H_end)``;
* ``getNuPrimeNonlinear`` (dnu/dB, used by its tangent): ``B(B<=0) = 1e-5``,
  then ``(ppval(HBsplineDer, B)*B - ppval(HBspline, B))/B^2``, and for
  ``B > B_end``, ``(B/mu0 - nu(B)*B)/B^2``.

This script implements MATLAB ``pchip`` exactly (interior weighted-harmonic
slopes, the one-sided three-point endpoint formula and its shape-preserving
clamps), cross-checks it against ``scipy.interpolate.PchipInterpolator``,
reads GEODE's dense dump (``examples/m27_law_dump.rs``) and reports the
maximum relative differences by region. Every number is computed here from
the two evaluated curves; nothing is inferred from the method names.

Run (from the repository root):

    uv run --no-project --python 3.14 --with scipy==1.18.1 python -I \\
        benchmarks/motor_nonlinear_m1/pchip_compare.py \\
        --reference benchmarks/motor_reference/reference_data.json \\
        --dump benchmarks/motor_nonlinear_m1/geode_m27_law_dump.csv \\
        --solver-commit <git commit of the law source> \\
        --law-source-blob <git hash-object of magnetostatic_nonlinear.rs> \\
        --out benchmarks/motor_nonlinear_m1/pchip_compare.json
"""

import argparse
import csv
import hashlib
import json
import math
import platform
import sys

import numpy as np
import scipy
from scipy.interpolate import PchipInterpolator

MU0 = 4e-7 * math.pi  # Mat_Magnetic.Mu0 = 4*pi*1e-7; GEODE uses the same.
B_CLAMP = 1e-5  # getMuNonlinear: B(B<=1e-5) = 1e-5


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


# ---------------------------------------------------------------------------
# MATLAB pchip, transcribed from pchip.m / pchipslopes (R2014b and later; the
# algorithm is unchanged since Fritsch & Carlson 1980 / Moler, NCM ch. 3).
# ---------------------------------------------------------------------------


def matlab_pchip_slopes(x, y):
    """Knot slopes d of MATLAB pchip for strictly increasing x."""
    n = len(x)
    h = np.diff(x)
    delta = np.diff(y) / h
    d = np.zeros(n)
    if n == 2:
        d[:] = delta[0]
        return d
    # Interior: weighted harmonic mean where consecutive secants share sign.
    for k in range(n - 2):
        if np.sign(delta[k]) * np.sign(delta[k + 1]) > 0:
            hs = h[k] + h[k + 1]
            w1 = (h[k] + hs) / (3 * hs)
            w2 = (hs + h[k + 1]) / (3 * hs)
            dmax = max(abs(delta[k]), abs(delta[k + 1]))
            dmin = min(abs(delta[k]), abs(delta[k + 1]))
            d[k + 1] = dmin / (w1 * (delta[k] / dmax) + w2 * (delta[k + 1] / dmax))
    # Endpoints: one-sided three-point formula with shape-preserving clamps.
    d[0] = pchip_end(h[0], h[1], delta[0], delta[1])
    d[n - 1] = pchip_end(h[n - 2], h[n - 3], delta[n - 2], delta[n - 3])
    return d


def pchip_end(h0, h1, del0, del1):
    """pchipend: d = ((2h0+h1)del0 - h0 del1)/(h0+h1), then the clamps."""
    d = ((2 * h0 + h1) * del0 - h0 * del1) / (h0 + h1)
    if np.sign(d) != np.sign(del0):
        d = 0.0
    elif np.sign(del0) != np.sign(del1) and abs(d) > abs(3 * del0):
        d = 3 * del0
    return d


def hermite_eval(x, y, d, xq):
    """ppval of the pchip piecewise cubic and of its fnder, for xq in
    [x[0], x[-1]]; the interval of xq is the one with x[k] <= xq < x[k+1]
    (the last interval is closed), as in ppval."""
    xq = np.asarray(xq, dtype=float)
    k = np.clip(np.searchsorted(x, xq, side="right") - 1, 0, len(x) - 2)
    hk = x[k + 1] - x[k]
    t = (xq - x[k]) / hk
    y0, y1, d0, d1 = y[k], y[k + 1], d[k], d[k + 1]
    h00 = 2 * t**3 - 3 * t**2 + 1
    h10 = t**3 - 2 * t**2 + t
    h01 = -2 * t**3 + 3 * t**2
    h11 = t**3 - t**2
    val = h00 * y0 + h10 * hk * d0 + h01 * y1 + h11 * hk * d1
    dv = (
        (6 * t**2 - 6 * t) * y0 / hk
        + (3 * t**2 - 4 * t + 1) * d0
        + (-6 * t**2 + 6 * t) * y1 / hk
        + (3 * t**2 - 2 * t) * d1
    )
    return val, dv


class Reference:
    """The reference code's iron law, as Mat_Magnetic.m evaluates it."""

    def __init__(self, b, h):
        self.b, self.h = b, h
        self.d = matlab_pchip_slopes(b, h)
        self.b_end, self.h_end = b[-1], h[-1]

    def pp(self, bq):
        return hermite_eval(self.b, self.h, self.d, np.minimum(bq, self.b_end))

    def nu(self, bq):
        """getNuNonlinear = 1/getMuNonlinear."""
        bc = np.where(bq <= B_CLAMP, B_CLAMP, bq)
        val, _ = self.pp(bc)
        nu = val / bc
        tail = bc > self.b_end
        nu = np.where(tail, ((bc - self.b_end) / MU0 + self.h_end) / bc, nu)
        return nu

    def h_residual(self, bq):
        """H = nu(B) B, the field the reference residual uses."""
        return self.nu(bq) * bq

    def dhdb_curve(self, bq):
        """dH/dB of the pchip curve (fnder) and the 1/mu0 tail."""
        _, dv = self.pp(bq)
        return np.where(bq > self.b_end, 1.0 / MU0, dv)

    def dhdb_tangent(self, bq):
        """nu + B nu'(B) with getNuPrimeNonlinear (clamp only at B<=0)."""
        bp = np.where(bq <= 0, B_CLAMP, bq)
        val, dv = self.pp(bp)
        nup = (dv * bp - val) / bp**2
        tail = bp > self.b_end
        nup = np.where(tail, (bp / MU0 - self.nu(bp) * bp) / bp**2, nup)
        return self.nu(bq) + bq * nup


def region_stats(mask, b, rel):
    idx = np.flatnonzero(mask)
    j = idx[np.argmax(rel[idx])]
    return float(rel[j]), float(b[j])


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--reference", required=True)
    ap.add_argument("--dump", required=True)
    ap.add_argument("--solver-commit", required=True)
    ap.add_argument("--law-source-blob", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    with open(a.reference) as f:
        ref = json.load(f)
    b_tab = np.array(ref["material"]["B_T"], dtype=float)
    h_tab = np.array(ref["material"]["H_A_per_m"], dtype=float)
    assert len(b_tab) == 47 and len(h_tab) == 47

    rows = []
    with open(a.dump) as f:
        for r in csv.DictReader(f):
            rows.append([float(r[k]) for k in
                         ("B_T", "H_A_per_m", "nu_m_per_H", "dHdB_m_per_H")])
    g = np.array(rows)
    bg, hg, nug, dhg = g[:, 0], g[:, 1], g[:, 2], g[:, 3]

    # GEODE interpolates the table exactly (the dump contains every knot).
    for bk, hk in zip(b_tab, h_tab):
        i = np.flatnonzero(bg == bk)
        assert len(i) == 1, f"knot {bk} missing from dump"
        assert abs(hg[i[0]] - hk) <= 1e-12 * max(hk, 1.0), (bk, hg[i[0]], hk)

    R = Reference(b_tab, h_tab)

    # Cross-check against scipy's PchipInterpolator on the table range.
    sp = PchipInterpolator(b_tab, h_tab)
    inside = bg <= R.b_end
    mv, md = R.pp(bg[inside])
    sv, sd = sp(bg[inside]), sp(bg[inside], 1)
    xcheck = {
        "max_abs_dH_rel_to_max_H": float(np.max(np.abs(mv - sv)) / h_tab[-1]),
        "max_rel_dH_B_gt_0": float(np.max(np.abs(mv - sv)[bg[inside] > 0]
                                          / np.abs(mv)[bg[inside] > 0])),
        "max_rel_slope": float(np.max(np.abs(md - sd) / np.abs(md))),
        "endpoint_slope_first_matlab": float(R.d[0]),
        "endpoint_slope_first_scipy": float(sp(b_tab[0], 1)),
        "endpoint_slope_last_matlab": float(R.d[-1]),
        "endpoint_slope_last_scipy": float(sp(b_tab[-1], 1)),
        "interior_knot_slopes_max_rel_diff": float(np.max(
            np.abs(R.d[1:-1] - sp(b_tab[1:-1], 1)) / R.d[1:-1])),
    }

    pos = bg > 0
    b = bg[pos]
    h_ref = R.h_residual(b)
    nu_ref = R.nu(b)
    dh_curve = R.dhdb_curve(b)
    dh_tan = R.dhdb_tangent(b)
    rel_h = np.abs(hg[pos] - h_ref) / h_ref
    rel_nu = np.abs(nug[pos] - nu_ref) / nu_ref
    rel_dh = np.abs(dhg[pos] - dh_curve) / dh_curve
    rel_dh_tan = np.abs(dhg[pos] - dh_tan) / np.abs(dh_tan)

    b1, b45, b46 = b_tab[1], b_tab[45], b_tab[46]
    regions = [
        ("origin_clamp", "0 < B <= 1e-5 T (reference nu frozen at its 1e-5 T value)",
         b <= B_CLAMP),
        ("first_segment", "1e-5 < B <= 0.05 T (GEODE odd cubic vs pchip)",
         (b > B_CLAMP) & (b <= b1)),
        ("interior", "0.05 <= B <= 2.25 T (both use the same interior slopes)",
         (b >= b1) & (b <= b45)),
        ("last_segment", "2.25 <= B <= 2.30 T (GEODE last-knot slope 1/mu0 vs pchip endpoint slope)",
         (b >= b45) & (b <= b46)),
        ("tail", "2.30 < B <= 3.0 T (both: H_end + (B - B_end)/mu0)",
         b > b46),
    ]
    table = {}
    for key, desc, m in regions:
        e = {"description": desc, "n_samples": int(m.sum())}
        for name, rel in (("rel_dH_over_H", rel_h), ("rel_dnu_over_nu", rel_nu),
                          ("rel_d_dHdB_vs_pchip_curve", rel_dh),
                          ("rel_d_dHdB_vs_reference_tangent", rel_dh_tan)):
            v, at = region_stats(m, b, rel)
            e[name] = {"max": v, "at_B_T": at}
        table[key] = e

    # Knot slopes where the two laws differ by construction.
    geode_at = lambda bq: float(dhg[np.flatnonzero(bg == bq)[0]])  # noqa: E731
    _, d_ref_b46_left = R.pp(np.array([b46]))
    slopes = {
        "geode_nu0_m_per_H": float(nug[0]),
        "reference_nu_at_B_le_1e-5_m_per_H": float(R.nu(np.array([1e-6]))[0]),
        "pchip_slope_at_B0_m_per_H": float(R.d[0]),
        "first_secant_m_per_H": float(h_tab[1] / b_tab[1]),
        "slope_at_B1_geode": geode_at(b1),
        "slope_at_B1_pchip": float(R.d[1]),
        "slope_at_B_end_geode": geode_at(b46),
        "slope_at_B_end_pchip_endpoint": float(R.d[-1]),
        "slope_at_B_end_reference_left_of_join": float(d_ref_b46_left[0]),
        "reference_tail_slope_1_over_mu0": 1.0 / MU0,
        "reference_slope_jump_at_B_end_rel": float((1.0 / MU0 - R.d[-1]) / (1.0 / MU0)),
        "last_secant_m_per_H": float((h_tab[46] - h_tab[45]) / (b_tab[46] - b_tab[45])),
    }

    # The reference's own residual/tangent mismatch below its clamp: there
    # nu_ref is frozen, so the residual's dH/dB is nu_ref(1e-5), while its
    # tangent uses nu_ref + B*getNuPrimeNonlinear(B) with the unclamped B.
    clamp = b <= B_CLAMP
    self_mismatch = np.abs(dh_tan[clamp] - nu_ref[clamp]) / nu_ref[clamp]
    slopes["reference_self_tangent_mismatch_below_clamp_max_rel"] = float(np.max(self_mismatch))
    slopes["reference_self_tangent_mismatch_below_clamp_at_B_T"] = float(
        b[clamp][np.argmax(self_mismatch)])

    out = {
        "schema_version": 1,
        "issue": 1160,
        "what": "Measured difference between GEODE TabulatedBhLaw and the reference "
                "Mat_Magnetic.m law (MATLAB pchip(B,H) + 1e-5 T clamp + 1/mu0 tail) on M27.",
        "definitions": {
            "H_ref": "nu_ref(B)*B, the reference residual field (nu from getNuNonlinear)",
            "rel_dH_over_H": "|H_geode - H_ref|/H_ref",
            "rel_dnu_over_nu": "|nu_geode - nu_ref|/nu_ref",
            "rel_d_dHdB_vs_pchip_curve": "|dHdB_geode - pchip'(B)|/pchip'(B); 1/mu0 above 2.3 T",
            "rel_d_dHdB_vs_reference_tangent": "|dHdB_geode - (nu_ref + B*getNuPrimeNonlinear(B))| / |.|",
            "max": "maximum over the dump samples in the region (B > 0)",
        },
        "inputs": {
            "reference_data_json_sha256": sha256(a.reference),
            "geode_dump_csv_sha256": sha256(a.dump),
            "geode_dump_rows": int(len(bg)),
            "solver_commit": a.solver_commit,
            "law_source": "crates/geode-core/src/assembly/magnetostatic_nonlinear.rs",
            "law_source_git_blob": a.law_source_blob,
            "reference_source": "ICEMsnapshot material/Mat_Magnetic.m (sources.json commit)",
            "mu0": MU0,
        },
        "environment": {
            "python": platform.python_version(),
            "numpy": np.__version__,
            "scipy": scipy.__version__,
        },
        "matlab_pchip_vs_scipy": xcheck,
        "knot_slopes": slopes,
        "regions": table,
    }
    with open(a.out, "w") as f:
        json.dump(out, f, indent=2)
        f.write("\n")

    print(f"python {platform.python_version()}, numpy {np.__version__}, scipy {scipy.__version__}")
    print("MATLAB pchip vs scipy PchipInterpolator:", json.dumps(xcheck, indent=1))
    print("knot slopes:", json.dumps(slopes, indent=1))
    print("\n| region | samples | max dH/H (at B) | max dnu/nu (at B) | max d(dH/dB) vs pchip' (at B) |")
    print("|---|---|---|---|---|")
    for key, _, _ in regions:
        e = table[key]
        f = lambda k: f"{e[k]['max']:.3e} ({e[k]['at_B_T']:.6g} T)"  # noqa: E731
        print(f"| {key} | {e['n_samples']} | {f('rel_dH_over_H')} | {f('rel_dnu_over_nu')} | "
              f"{f('rel_d_dHdB_vs_pchip_curve')} |")
    return 0


if __name__ == "__main__":
    sys.exit(main())
