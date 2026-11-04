#!/usr/bin/env python3
"""Markdown table of the nested-DOF ladder (issue #1087).

Reads ``benchmarks/patch_antenna_conformal/nested_dof/<level>_<label>.toml``
(written by ``examples/patch_conformal_nested_dof.rs``) and prints one row per
run: DOF counts, the worst-of-ISM-band |S11| before and after (on the band
points and on the fresh 9-point verification sweep), the best in-band point,
the |S11| dip of the final design on the wide sweep, the terminal condition,
the FD gate, the mesh-distortion budget use, and the cost.

Usage::

    python3 benchmarks/patch_antenna_conformal/scripts/nested_dof_table.py [L0 L1 ...]

Only the standard library is needed (Python >= 3.11 for ``tomllib``).
"""
from __future__ import annotations

import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
DIR = HERE.parent / "nested_dof"
LABELS = ["gap_1", "gap_4", "gap_16", "gap_48", "outline_16", "combined_64"]


def main() -> None:
    levels = sys.argv[1:] or ["L0", "L1"]
    print(
        "| level | run | radial | outline | worst-of-band initial (dB) | worst final, band pts (dB) | "
        "worst final, 9-pt verify (dB) | best in-band final (dB) | final dip (GHz, dB) | reached -10 dB | "
        "steps / terminal | FD max rel | worst vol ratio | max disp (mm) | sigma_0 | wall (min) |"
    )
    print("|" + "---|" * 16)
    for level in levels:
        for label in LABELS:
            path = DIR / f"{level}_{label}.toml"
            if not path.exists():
                print(f"| {level} | {label} | (missing) |" + " |" * 13)
                continue
            d = tomllib.loads(path.read_text())
            m, b, o, fd = d["meta"], d["basis"], d["optimization"], d["fd_validation"]
            print(
                f"| {level} | {label} | {b['n_radial_dofs']} | {b['n_outline_dofs']} | "
                f"{o['worst_s11_db_initial']:.2f} | {o['worst_s11_db_final']:.2f} | "
                f"{o['verify_worst_s11_db']:.2f} | {o['verify_best_s11_db']:.2f} | "
                f"{o['dip_f_ghz_final']:.3f}, {o['dip_db_final']:.1f} | "
                f"{'yes' if o['reached_target'] else 'no'} | {o['n_steps']} / {o['terminal_condition']} | "
                f"{fd['max_rel_err']:.1e} | {o['worst_vol_ratio']:.3f} | {o['max_nodal_disp_mm']:.3f} | "
                f"{m['upml_sigma_0']} | {o['total_wall_s'] / 60:.0f} |"
            )


if __name__ == "__main__":
    main()
