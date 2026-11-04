#!/usr/bin/env python3
"""Pin head_to_head.json and the README table to the committed source artifacts (#651).

Recomputes everything via build_head_to_head.build() and compares, then checks
hand-copied spot values and structural invariants. Active under python -O.
Usage: verify_head_to_head.py [--json PATH] [--readme PATH]   (defaults: committed)
"""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_head_to_head as b  # noqa: E402


def check(cond, msg):
    if not cond:
        raise AssertionError(msg)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", default=str(b.HERE / "head_to_head.json"))
    ap.add_argument("--readme", default=str(b.HERE / "README.md"))
    a = ap.parse_args()

    fresh = b.build()
    committed = json.loads(Path(a.json).read_text())
    check(committed == json.loads(json.dumps(fresh)), "head_to_head.json drifted from source artifacts; rerun build_head_to_head.py")
    check(Path(a.readme).read_text() == b.render_readme(Path(a.readme).read_text(), b.table(fresh)),
          "README table drifted from head_to_head.json; rerun build_head_to_head.py")

    # Spot values copied by hand from the source TOML/JSON (independent of the builder).
    opt = {(r["level"], r["label"]): r for r in committed["optimized_2p4ghz"]}
    check(opt[("L1", "gap_16")]["worst_s11_db"] == -11.29462, "L1 gap_16")
    check(opt[("L1", "combined_64")]["worst_s11_db"] == -14.11728, "L1 combined_64")
    check(opt[("L2", "combined_64_cold")]["worst_s11_db"] == -14.06614, "L2 combined_64_cold")
    check(committed["smoke_fixture_freeform"]["worst_s11_db"] == -12.06262, "smoke 73-DOF")
    check(committed["smoke_fixture_freeform"]["n_dofs"] == 73, "smoke DOFs")

    # Invariants.
    for r in committed["optimized_2p4ghz"]:
        check(r.get("level") in ("L0", "L1", "L2") and r.get("upml_sigma_0") is not None,
              f"{r['label']}: level/sigma0 missing")
        check(r["conditional_on_mesh_level"] is True, f"{r['label']}: must be flagged conditional")
        check(r["cost"]["wall_s"] != "", "cost cell empty")
    check(committed["smoke_fixture_freeform"]["comparable_to_2p4ghz_rows"] is False, "smoke row must stay non-comparable")
    for g in committed["gaps"]:
        same = [r for r in committed["optimized_2p4ghz"] if r["level"] == g["level"]]
        check(any(r["label"] == g["best_parametric"] for r in same), "gap mixes levels")
        check(abs(g["gap_db_parametric_minus_freeform"]
                  - (g["best_parametric_worst_s11_db"] - g["freeform_worst_s11_db"])) < 1e-9, "gap arithmetic")
    fd = committed["fdtd_density"]
    check(fd["worst_s11_db"] == b.NOT_RUN, "no FDTD-density |S11| may be reported")
    check(not any(isinstance(v, (int, float)) for k, v in fd.items() if "s11" in k), "numeric FDTD s11 found")
    print("verify_head_to_head: OK")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAIL: {e}", file=sys.stderr)
        sys.exit(1)
