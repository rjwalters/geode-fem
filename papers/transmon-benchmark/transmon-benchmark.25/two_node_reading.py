#!/usr/bin/env python3
"""The two-node circuit of Section 3.3, both ways (Python >= 3.11, run as python3.14 -I two_node_reading.py --write).

Bare qubit 1/(2 pi sqrt((L_J + L_s) C_Sigma)), C_Sigma = C_isl + C_J, Maxwell-form coupling k to the resonator
(as geode-core's coupled_lc_dressed_pair_hz / coupled_lc_bare_pair_hz). Forward: the grounded- and natural-box
C_isl brackets to the dressed qubit (the static route). Backward: DeviceLayout's dressed pair [4.14, 5.591] GHz to
C_Sigma (Table 6, "Published, two-node"). L_s and k bands from benchmarks/transmon_loop_inductance/results.toml
[anchor]; that artifact's own C_Sigma band uses another topology (C_J across L_J alone) and is not used here.
Writes two_node_reading.toml beside this script; verify_evidence.py checks the file against a fresh run.
"""
import math
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
L_J, C_J = 14.860e-9, 5.5e-15
E, H = 1.602176634e-19, 6.62607015e-34


def dressed_lo(fq, fr, k):
    x, y, q = fq * fq, fr * fr, 1 - k * k
    return math.sqrt((x + y - math.sqrt((x + y) ** 2 - 4 * q * x * y)) / (2 * q))


def bare_lo(flo, fhi, k):
    q = 1 - k * k
    x1, x2 = flo * flo, fhi * fhi
    s, p = q * (x1 + x2), q * x1 * x2
    return math.sqrt((s - math.sqrt(s * s - 4 * p)) / 2)


def compute():
    anc = tomllib.loads((REPO / "benchmarks/transmon_loop_inductance/results.toml").read_text())["anchor"]
    ls_band = [x * 1e-9 for x in anc["l_s_band_nh"]]
    k_band = anc["k_band"]
    sr = tomllib.loads((REPO / "benchmarks/transmon_diffopt/c_sigma_bracket_grounded_box.toml").read_text())
    fr = sr["static_route"]["resonator_f_r_ghz"] * 1e9
    out = {"l_s_band_nh": anc["l_s_band_nh"], "k_band": k_band, "resonator_f_r_ghz": fr / 1e9}
    for name, (lo, hi) in [("grounded", (79.56, 79.77)), ("natural", (79.56, 79.76))]:
        v = [dressed_lo(1 / (2 * math.pi * math.sqrt((L_J + ls) * (c + 5.5) * 1e-15)), fr, k) / 1e9
             for c in (lo, hi) for ls in ls_band for k in k_band]
        out[f"static_route_{name}_ghz"] = [round(min(v), 6), round(max(v), 6)]
    c = [1 / ((L_J + ls) * (2 * math.pi * bare_lo(4.14e9, 5.591e9, k)) ** 2) * 1e15 for k in k_band for ls in ls_band]
    out["published_c_sigma_ff"] = [round(min(c), 6), round(max(c), 6)]
    out["published_e_c_ghz"] = [round(E * E / (2 * H * max(c) * 1e-15) / 1e9, 6),
                                round(E * E / (2 * H * min(c) * 1e-15) / 1e9, 6)]
    return out


def render(d):
    lines = ["# Written by two_node_reading.py --write. Do not edit by hand.",
             "# Section 3.3's two-node circuit, forward (static route) and backward (published pair)."]
    for k, v in d.items():
        lines.append(f"{k} = {v!r}".replace("'", '"'))
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    text = render(compute())
    if "--write" in sys.argv:
        (HERE / "two_node_reading.toml").write_text(text)
    print(text, end="")
