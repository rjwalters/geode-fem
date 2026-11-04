#!/usr/bin/env python3
"""Gate G2 tables of the nested-DOF study (issue #1103, conformal v5 E4b).

Reads the L2 optimizations ``nested_dof/L2_<label>.toml`` and the fresh
forward verifications ``nested_dof/verify/<design>_on_<level>[_p2].toml``
(both written by ``examples/patch_conformal_nested_dof.rs``) and prints three
Markdown tables:

1. the initial patch (x = 0), worst/best of the 9 in-band points per level;
2. the L2 optimizations (start, steps, terminal, guard, crater, thickness);
3. the cross-level transfer: each design's worst-of-band |S11| on every level
   it was verified on, and the loss against its own level.

Usage::

    python3 benchmarks/patch_antenna_conformal/scripts/nested_dof_g2.py

Only the standard library is needed (Python >= 3.11 for ``tomllib``).
"""
from __future__ import annotations

import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ND = HERE.parent / "nested_dof"
VERIFY = ND / "verify"
# Verification targets in order of increasing fidelity (#1085: the P2-on-L1
# dip lies above the L3 one, closest to the converged dip).
TARGETS = ["L0", "L1", "L2", "L3", "L1_p2"]
TARGET_NAME = {"L1_p2": "P2/L1"}


def load(p: Path) -> dict:
    return tomllib.loads(p.read_text())


def worst(d: dict) -> float:
    return max(p["s11_db"] for p in d["s11_verify"]["point"])


def best(d: dict) -> float:
    return min(p["s11_db"] for p in d["s11_verify"]["point"])


def own_level_worst(design: str) -> tuple[str, float] | None:
    """The design's worst in-band |S11| on the level it was optimized on."""
    level, label = design.split("_", 1)
    p = ND / f"{level}_{label}.toml"
    if not p.exists():
        return None
    return level, load(p)["optimization"]["verify_worst_s11_db"]


def main() -> None:
    print("### Initial patch (x = 0), 9 in-band points\n")
    print("| level | order | worst (dB) | best (dB) | at 2.40 GHz (dB) | at 2.4835 GHz (dB) |")
    print("|---|---|---|---|---|---|")
    for t in TARGETS:
        p = VERIFY / f"initial_on_{t}.toml"
        if not p.exists():
            continue
        d = load(p)
        pts = d["s11_verify"]["point"]
        print(
            f"| {t.split('_')[0]} | {d['meta']['element_order']} | {worst(d):.2f} | {best(d):.2f} | "
            f"{pts[0]['s11_db']:.2f} | {pts[-1]['s11_db']:.2f} |"
        )

    print("\n### L2 optimizations\n")
    print(
        "| run | start | DOFs | worst initial (dB) | worst final, 9-pt (dB) | best (dB) | dip (GHz, dB) | "
        "steps / terminal | guard events | final worst tet | FD max rel | mean / max thickness (mm) | "
        "steepest slope (at mm from feed) | wall (h) |"
    )
    print("|" + "---|" * 14)
    for p in sorted(ND.glob("L2_*.toml")):
        d = load(p)
        m, o, s, g, c = d["meta"], d["optimization"], d["start"], d["guard"], d["crater"]
        fw = g["final_worst"]
        start = s["kind"] if s["kind"] == "cold" else f"warm x{s['scale']} ({s['design_level']})"
        print(
            f"| {m['label']} | {start} | {d['basis']['n_dofs']} | {o['worst_s11_db_initial']:.2f} | "
            f"{o['verify_worst_s11_db']:.2f} | {o['verify_best_s11_db']:.2f} | "
            f"{o['dip_f_ghz_final']:.3f}, {o['dip_db_final']:.1f} | {o['n_steps']} / {o['terminal_condition']} | "
            f"{g['n_events']} | {fw['ratio']:.3f} {fw['region']}, {fw['feed_dist_mm']:.1f} mm feed / "
            f"{fw['corner_dist_mm']:.1f} mm corner | {d['fd_validation']['max_rel_err']:.1e} | "
            f"{c['mean_thickness_mm']:.2f} / {c['max_thickness_mm']:.2f} | "
            f"{c['max_slope']:.2f} ({c['max_slope_feed_dist_mm']:.1f}) | {o['total_wall_s'] / 3600:.1f} |"
        )

    print("\n### Cross-level transfer (worst of the 9 in-band points, dB)\n")
    designs = sorted(
        {p.stem.split("_on_")[0] for p in VERIFY.glob("*_on_*.toml")} - {"initial"}
    )
    print("| design | own level | " + " | ".join(TARGET_NAME.get(t, t) for t in TARGETS) + " | worst loss vs own (dB) |")
    print("|---|---|" + "---|" * (len(TARGETS) + 1))
    for des in designs:
        own = own_level_worst(des)
        cells, losses = [], []
        for t in TARGETS:
            p = VERIFY / f"{des}_on_{t}.toml"
            if own and t == own[0]:
                cells.append(f"**{own[1]:.2f}** (own)")
            elif p.exists():
                d = load(p)
                w = worst(d)
                cells.append(f"{w:.2f} (vol {d['verify']['min_vol_ratio']:.3f})")
                if own:
                    losses.append(w - own[1])
            else:
                cells.append("")
        loss = f"{max(losses):+.2f}" if losses else ""
        print(f"| {des} | {own[0] if own else '?'} | " + " | ".join(cells) + f" | {loss} |")


if __name__ == "__main__":
    main()
