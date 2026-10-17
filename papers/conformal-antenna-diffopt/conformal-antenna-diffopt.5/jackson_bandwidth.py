#!/usr/bin/env python3
"""Closed-form bandwidth estimate for the section 6 "Bandwidth in context" paragraph.

Jackson and Alexopoulos (1991) CAD formulas for a resonant rectangular patch, as
restated in D. R. Jackson's University of Houston ECE 6345 course notes
(Notes 15 "CAD for Qsp" and Notes 16 "CAD for Gain and Efficiency",
http://courses.egr.uh.edu/ECE/ECE6345/Class%20Notes/):

    n   = sqrt(eps_r),  c1 = 1 - 1/n^2 + 2/(5 n^4)
    p   = 1 + (a2/10)(k0 W)^2 + (a2^2 + 2 a4)(3/560)(k0 W)^4
            + (c2/5)(k0 L)^2 + (a2 c2/70)(k0 W)^2 (k0 L)^2
          a2 = -0.16605, a4 = 0.00761, c2 = -0.0914153
    Q_sp = (3 pi/8) eps_r (L/W) / (k0 h c1 p)                 (space wave)
    e_r  = 1 / [1 + (3 pi/4)(k0 h)(1/c1)(1 - 1/n^2)^3]       (HED radiation efficiency)
    Q_sw = Q_sp e_r / (1 - e_r)                               (surface wave)
    Q_d  = 1 / tan_delta                                      (dielectric)
    1/Q  = 1/Q_sp + 1/Q_sw + 1/Q_d                            (Q_c omitted: the FEM model is PEC)
    BW   = 1 / (sqrt(2) Q)                                    (VSWR <= 2)

Substrate and patch values are those of the fixture (benchmarks/patch_antenna_conformal/
baseline_2g4_L1.toml [meta]) and of the final design as built
(copper/verify/rbD_L1_copper_38_sm45_on_rbD_L1_p2_dense0of3.toml [copper]); f = 2.44 GHz,
near the band centre. W is the radiating-edge width (along s), L the resonant length
(along y). Prints one line per patch; the paper quotes the last column rounded.
"""
import math

EPS_R, TAN_D, H_MM, F_GHZ = 4.4, 0.02, 1.6, 2.44
A2, A4, C2 = -0.16605, 0.00761, -0.0914153
PATCHES = [("initial", 38.0, 29.0), ("final", 49.752266, 28.803414)]


def bandwidth_mhz(w, l, surface_wave=True):
    n = math.sqrt(EPS_R)
    c1 = 1 - 1 / n**2 + 2 / (5 * n**4)
    k0 = 2 * math.pi * F_GHZ / 299.792458   # rad/mm
    kw, kl = k0 * w, k0 * l
    p = (1 + A2 / 10 * kw**2 + (A2**2 + 2 * A4) * 3 / 560 * kw**4
         + C2 / 5 * kl**2 + A2 * C2 / 70 * kw**2 * kl**2)
    q_sp = 3 * math.pi / 8 * EPS_R * (l / w) / (k0 * H_MM * c1 * p)
    e_r = 1 / (1 + 3 * math.pi / 4 * k0 * H_MM / c1 * (1 - 1 / n**2) ** 3)
    inv_q = 1 / q_sp + TAN_D
    if surface_wave:
        inv_q += (1 - e_r) / (q_sp * e_r)
    return 1e3 * F_GHZ * inv_q / math.sqrt(2), p, q_sp, e_r


if __name__ == "__main__":
    for name, w, l in PATCHES:
        bw, p, q_sp, e_r = bandwidth_mhz(w, l)
        bw0 = bandwidth_mhz(w, l, surface_wave=False)[0]
        print(f"{name}: W x L = {w:.2f} x {l:.2f} mm  p = {p:.3f}  Q_sp = {q_sp:.1f}  "
              f"e_r = {e_r:.3f}  BW without Q_sw = {bw0:.1f} MHz  BW = {bw:.1f} MHz")
