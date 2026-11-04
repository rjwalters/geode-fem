#!/usr/bin/env python3
"""Frequency space-mapping fit for the copper designs (issue #1120).

A design optimized on a coarse model resonates higher on a finer one (#1103:
2.425 -> 2.45 -> 2.50 GHz from L2 -> L3 -> P2/L1). Model the finer response as
the coarse one shifted in frequency, S_fine(f) ~ S_coarse(f - delta), and fit
delta by least squares over the fine model's sampled points, with the coarse
response interpolated (complex, linear) on a dense coarse sweep:

    delta = argmin sum_i |S_fine(f_i) - S_coarse(f_i - delta)|^2

Usage:
    python3 copper_shift_fit.py COARSE_DENSE.toml [more shards...] -- FINE.toml

COARSE_DENSE: `--verify-only --dense` output(s) of the design on the coarse
model ([[s11_dense.point]], complex). FINE: a verification with complex
[[s11_verify.point]] / [[s11_dense.point]] on the finer model. Prints delta
(MHz), the rms complex misfit before and after, and the residual per point.
The fitted delta is what `--band-shift` takes (in GHz) to re-optimize on the
coarse model for the band as the finer model sees it.
"""
import math
import sys
import tomllib


def pts(path, keys):
    with open(path, "rb") as f:
        d = tomllib.load(f)
    out = []
    for k in keys:
        for p in d.get(k, {}).get("point", []):
            out.append((p["f_ghz"], complex(p["s11_re"], p["s11_im"])))
    return out


def interp(grid, f):
    for (f0, s0), (f1, s1) in zip(grid, grid[1:]):
        if f0 <= f <= f1:
            t = (f - f0) / (f1 - f0)
            return s0 + t * (s1 - s0)
    return None


def misfit(grid, fine, delta):
    acc, n = 0.0, 0
    for f, s in fine:
        c = interp(grid, f - delta)
        if c is None:
            return math.inf
        acc += abs(s - c) ** 2
        n += 1
    return math.sqrt(acc / n)


def main():
    argv = sys.argv[1:]
    k = argv.index("--")
    coarse = sorted(set(sum((pts(p, ["s11_dense"]) for p in argv[:k]), [])))
    fine = sorted(set(sum((pts(p, ["s11_verify", "s11_dense"]) for p in argv[k + 1 :]), [])))
    best = min((misfit(coarse, fine, d / 1e4), d / 1e4) for d in range(-1500, 1501))
    lo, hi = best[1] - 1e-4, best[1] + 1e-4
    for _ in range(60):  # golden-section refinement
        a, b = lo + 0.382 * (hi - lo), lo + 0.618 * (hi - lo)
        if misfit(coarse, fine, a) < misfit(coarse, fine, b):
            hi = b
        else:
            lo = a
    delta = 0.5 * (lo + hi)
    print(f"coarse points {len(coarse)} over [{coarse[0][0]}, {coarse[-1][0]}] GHz; fine points {len(fine)}")
    print(f"rms |dS11| at delta = 0: {misfit(coarse, fine, 0.0):.4f}")
    print(f"delta = {1e3 * delta:.2f} MHz: rms |dS11| = {misfit(coarse, fine, delta):.4f}")
    for f, s in fine:
        c = interp(coarse, f - delta)
        print(f"  {f:.4f} GHz: fine {20 * math.log10(abs(s)):7.2f} dB, shifted coarse "
              f"{20 * math.log10(abs(c)):7.2f} dB, |dS| {abs(s - c):.4f}")


if __name__ == "__main__":
    main()
