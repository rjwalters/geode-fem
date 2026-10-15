#!/usr/bin/env python3
"""Gate G2 for the manufacturable copper-only patch (issue #1120).

Reads the copper optimizations `copper/L*_<label>.toml` and the fresh forward
verifications `copper/verify/<design>_on_<level>[_p2][_dense<k>of<n>].toml`
(written by `examples/patch_conformal_nested_dof.rs --copper`), merges the
dense-sweep shards of each (design, level, order), and reports per
verification:

* worst / best of the 9 in-band points (dB);
* from the dense sweep: the dip (parabolic interpolation in dB around the
  sampled minimum), the -10 dB bandwidth (the contiguous run of the dense grid
  around the dip with |S11| <= -10 dB, crossings linearly interpolated in dB),
  its coverage of the 2.4-2.4835 GHz ISM band (MHz and %), the dense-sweep
  worst point inside the band, the centring (band centre minus the centre
  of the -10 dB interval, MHz) and the margins of the -10 dB interval beyond
  each band edge (MHz; negative = the band edge is not covered);
* the as-built copper change against the design as optimized (patch length at
  the feed, mm, from the `[copper]` sections): non-zero where a feed re-base
  between two re-based fixtures dropped the feed DOFs' drag of the +y edge, so
  the verified copper is a related design, not the optimum itself.

`--write` regenerates `copper/g2_summary.toml` (consumed by
`tests/patch_conformal_copper.rs`, which recomputes every number from the
shards). Usage:

    python3 benchmarks/patch_antenna_conformal/scripts/copper_g2.py [--write]
"""
import glob
import math
import os
import re
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))
COPPER = os.path.join(ROOT, "copper")
BAND = (2.4, 2.4835)
TARGET = -10.0
SHARD = re.compile(r"^(?P<stem>.+?)_dense(?P<k>\d+)of(?P<n>\d+)\.toml$")


NOTE = (
    "G2 is met on the finest computed model (P2/L1, rbD_L1_copper_38_sm45_on_rbD_L1_p2: "
    "margins 6.2 MHz at 2.400 GHz and 5.2 MHz at 2.4835 GHz). #1107 extrapolates the converged "
    "dip a further 12.7-48.4 MHz above P2/L1, more than the 6.2 MHz lower-edge margin, so on the "
    "converged estimate the design likely misses at the lower band edge. Rows with a non-zero "
    "copper_shorter_than_design_mm verify a related design (feed re-base drift), not the design "
    "as optimized: the L2 chain on rbE (0.47 mm, lower-edge margin 0.7 MHz on P2/L1) and the "
    "final design on the sibling rbF (0.09 mm). The feed is an idealized lumped-port strip."
)


def load(p):
    with open(p, "rb") as f:
        return tomllib.load(f)


def db(mag):
    return 20.0 * math.log10(mag)


def dense_points(files):
    """Merge shards: sorted (f, dB) with a consistency check of the grid."""
    pts, grid = {}, None
    for p in files:
        d = load(p)
        v = d["verify"]
        g = (tuple(v["dense_grid_ghz"]), v["dense_grid_n"])
        assert grid is None or grid == g, f"{p}: dense grid {g} != {grid}"
        grid = g
        for q in d.get("s11_dense", {}).get("point", []):
            f = round(q["f_ghz"], 6)
            assert f not in pts, f"{p}: duplicate dense point {f}"
            pts[f] = db(math.hypot(q["s11_re"], q["s11_im"]))
    (lo, hi), n = grid
    want = {round(lo + (hi - lo) * i / (n - 1), 6) for i in range(n)}
    missing = sorted(want - set(pts))
    return sorted(pts.items()), grid, missing


def bandwidth(pts):
    """Dip, -10 dB interval around it, ISM coverage, centring, worst in band."""
    fs = [f for f, _ in pts]
    ds = [d for _, d in pts]
    i = min(range(len(ds)), key=lambda k: ds[k])
    if 0 < i < len(ds) - 1:
        a, b, c = ds[i - 1], ds[i], ds[i + 1]
        den = a - 2 * b + c
        t = 0.5 * (a - c) / den if den > 0 else 0.0
        h = fs[i + 1] - fs[i]
        dip_f, dip_db = fs[i] + t * h, b - 0.25 * (a - c) * t
    else:
        dip_f, dip_db = fs[i], ds[i]
    out = {"dip_f_ghz": dip_f, "dip_db": dip_db}
    in_band = [d for f, d in pts if BAND[0] - 1e-9 <= f <= BAND[1] + 1e-9]
    out["dense_worst_in_band_db"] = max(in_band)
    out["dense_n_in_band"] = len(in_band)
    if ds[i] > TARGET:
        out.update(bw_lo_ghz=None, bw_hi_ghz=None, bw_mhz=0.0, ism_cover_mhz=0.0)
    else:
        lo = i
        while lo > 0 and ds[lo - 1] <= TARGET:
            lo -= 1
        hi = i
        while hi < len(ds) - 1 and ds[hi + 1] <= TARGET:
            hi += 1

        def cross(j, k):  # TARGET crossing between grid points j (above) and k (below)
            t = (ds[j] - TARGET) / (ds[j] - ds[k])
            return fs[j] + t * (fs[k] - fs[j])

        f_lo = cross(lo - 1, lo) if lo > 0 else None
        f_hi = cross(hi + 1, hi) if hi < len(ds) - 1 else None
        a = f_lo if f_lo is not None else fs[0]
        b = f_hi if f_hi is not None else fs[-1]
        cover = max(0.0, min(b, BAND[1]) - max(a, BAND[0]))
        out.update(
            bw_lo_ghz=f_lo,
            bw_hi_ghz=f_hi,
            bw_open_ended=f_lo is None or f_hi is None,
            bw_mhz=1e3 * (b - a),
            ism_cover_mhz=1e3 * cover,
            centring_mhz=1e3 * (0.5 * (BAND[0] + BAND[1]) - 0.5 * (a + b)),
            margin_lo_mhz=1e3 * (BAND[0] - a),
            margin_hi_mhz=1e3 * (b - BAND[1]),
        )
    out["ism_cover_pct"] = 100.0 * out["ism_cover_mhz"] / (1e3 * (BAND[1] - BAND[0]))
    return out


def verifications():
    """{name: {"verify": path or None, "dense": [paths]}} over copper/verify."""
    groups = {}
    for p in sorted(glob.glob(os.path.join(COPPER, "verify", "*.toml"))):
        base = os.path.basename(p)
        m = SHARD.match(base)
        key = m.group("stem") if m else base[:-5]
        g = groups.setdefault(key, {"verify": None, "dense": []})
        d = load(p)
        if d.get("s11_dense"):
            g["dense"].append(p)
        if d["verify"]["verify_n_points"] > 0:
            assert g["verify"] is None, f"two in-band verifications for {key}"
            g["verify"] = p
    return groups


def summarize():
    rows = []
    for key, g in sorted(verifications().items()):
        row = {"name": key}
        first = load(g["verify"] or g["dense"][0])
        src = load(os.path.join(COPPER, first["design"]["source"].replace(".design.toml", ".toml")))
        drift = src["copper"]["patch_length_mm"] - first["copper"]["patch_length_mm"]
        # The as-built length is read off the mesh to ~3e-3 mm.
        row["copper_shorter_than_design_mm"] = round(drift, 3) if abs(drift) >= 1e-2 else 0.0
        if g["verify"]:
            v = load(g["verify"])["verify"]
            row["verify_toml"] = os.path.relpath(g["verify"], ROOT)
            row["verify_worst_db"] = v["verify_worst_s11_db"]
            row["verify_best_db"] = v["verify_best_s11_db"]
            row["min_vol_ratio"] = v["min_vol_ratio"]
        if g["dense"]:
            pts, grid, missing = dense_points(g["dense"])
            row["dense_shards"] = [os.path.relpath(p, ROOT) for p in g["dense"]]
            row["dense_grid"] = [grid[0][0], grid[0][1], grid[1]]
            row["dense_missing"] = missing
            row.update(bandwidth(pts))
        rows.append(row)
    return rows


def fmt(v):
    if v is None:
        return "nan"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return f"{v:.6g}"
    if isinstance(v, list):
        return "[" + ", ".join(fmt(x) if not isinstance(x, str) else f'"{x}"' for x in v) + "]"
    return f'"{v}"' if isinstance(v, str) else str(v)


def main():
    rows = summarize()
    for r in rows:
        print(r["name"])
        for k, v in r.items():
            if k != "name":
                print(f"    {k} = {fmt(v)}")
    if "--write" in sys.argv:
        out = os.path.join(COPPER, "g2_summary.toml")
        with open(out, "w") as f:
            f.write("# Generated by benchmarks/patch_antenna_conformal/scripts/copper_g2.py --write\n")
            f.write("# from copper/verify/*.toml (issue #1120). Do NOT edit by hand.\n\n")
            f.write("[meta]\n")
            f.write(f"band_f_ghz = [{BAND[0]}, {BAND[1]}]\ntarget_db = {TARGET}\n")
            f.write(
                'bandwidth_rule = "contiguous run of the dense grid around the dip with |S11| <= '
                '-10 dB; crossings linearly interpolated in dB; open-ended if the run reaches the '
                'grid edge"\n'
            )
            f.write(f'note = "{NOTE}"\n\n')
            for r in rows:
                f.write("[[verification]]\n")
                for k, v in r.items():
                    if v is None:
                        continue
                    f.write(f"{k} = {fmt(v) if not isinstance(v, str) else chr(34) + v + chr(34)}\n")
                f.write("\n")
        print("wrote", os.path.relpath(out, os.getcwd()))


if __name__ == "__main__":
    main()
