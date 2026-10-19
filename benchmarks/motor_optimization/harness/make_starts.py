"""Generate the committed starts of experiment 2 (issues #1195 and #1201;
#1185 pre-registration, amendment 7 §5 with amendment 8's start rule).

    pyrun.sh make_starts.py --binary EVALUATOR [--out starts.json]

Rule (deterministic, no numpy):

- For seed s in 1..4: a Latin hypercube of M = 10 points in the
  18-dimensional unit box of space B (`lhs.latin_hypercube(s, 10, 18)`).
- Space B's box is space A's in its first 8 components (the 8 scalars), so
  each point gives a space-B start x_B = lower + u (upper - lower) and a
  space-A start x_A = the first 8 components of x_B: the two spaces start
  from the same scalars.
- A point is a **valid start** when, in both spaces and on both the L0 and
  the L1 θ0 mesh (amendment 8: eligible at L0 and L1), it is admissible
  (every h_i >= 0, amendment 7 §1), buildable, the θ0 mesh moves to it, and
  the moved mesh passes the remesh policy (no inverted triangle, minimum
  angle at least 5°): a start never begins with a remesh.
- Seeds 1 and 2: the frozen pilot starts (indices 1 and 4) are kept if
  valid; a kept seed that is not valid is replaced by the next valid index
  of its seed. Seeds 3 and 4: the first valid index. Every rejected index
  and its reasons, and every replacement, are recorded.
- θ0 is the registered initial design in both spaces (checked by the same
  rule).
"""
import argparse
import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from evaluator import Evaluator, to_bits  # noqa: E402
from lhs import latin_hypercube, to_box  # noqa: E402

ROOT = HERE.parents[2]
MESHES = ROOT / "benchmarks/motor_reference/geometry/meshes"
LEVELS = (0, 1)
POINTS_PER_SEED = 10
SEEDS = (1, 2, 3, 4)
#: The pilot starts (#1198), kept if still valid (amendment 7 §5).
PILOT_INDEX = {1: 1, 2: 4}


def mesh(level):
    return MESHES / f"icem_initial_sector_L{level}.msh"


def verdict(c):
    """Why a check rejects a point (None: it does not)."""
    if not c.get("ok"):
        return {"error": c.get("error")}
    g = c.get("geometry") or {}
    out = {}
    if not g.get("buildable"):
        out["not_buildable"] = g.get("construction_error") or [
            x["name"] for x in g.get("guards", []) if not x["pass"]]
    elif not g.get("admissible"):
        out["inadmissible"] = g.get("inadmissible_guards")
    if not c["geometry_ok"]:
        out["geometry"] = c.get("curves_error") or c.get("motion_error")
    if c["trigger"] is not None:
        out["trigger"] = c["trigger"]
    return out or None


REQUESTS = [0]


def check_point(ev, xa, xb):
    """Checks of one point in both spaces at both levels: {(space, L):
    reply}."""
    REQUESTS[0] += len(ev)
    out = {}
    for (sp, lv), e in ev.items():
        x = xa if sp == "a" else xb
        c = e.request("check", x_bits=to_bits(x))
        assert c.get("x_bits", to_bits(x)) == to_bits(x)
        out[(sp, lv)] = c
    return out


def summarise(checks):
    return {f"{sp}_L{lv}": {"verdict": verdict(c),
                            "min_h": (c.get("geometry") or {}).get("min_h"),
                            "quality": c.get("quality")}
            for (sp, lv), c in checks.items()}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--out", type=Path, default=HERE / "starts.json")
    a = ap.parse_args(argv)
    t_pre = time.monotonic()
    ev = {(sp, lv): Evaluator([a.binary, "--space", sp, "--train-mesh", str(mesh(lv)),
                               "--val-mesh", str(mesh(lv)), "--train-level", str(lv),
                               "--val-level", str(lv)])
          for sp in "ab" for lv in LEVELS}
    try:
        sa, sb = ev[("a", 0)].ready["space"], ev[("b", 0)].ready["space"]
        assert sb["free"][:len(sa["free"])] == sa["free"]
        assert sb["lower"][:8] == sa["lower"] and sb["upper"][:8] == sa["upper"]
        d = len(sb["free"])
        th = check_point(ev, sa["x0"], sb["x0"])
        if any(verdict(c) is not None for c in th.values()):
            raise SystemExit(f"θ0 is not a valid start: {summarise(th)}")
        starts = {"theta0": {"a": sa["x0"], "b": sb["x0"], "checks": summarise(th)}}
        for seed in SEEDS:
            pts = latin_hypercube(seed, POINTS_PER_SEED, d)
            first = PILOT_INDEX.get(seed, 0)
            order = list(range(first, POINTS_PER_SEED))
            rejected = []
            chosen = None
            for k in order:
                u = pts[k]
                xb = to_box(u, sb["lower"], sb["upper"])
                xa = xb[:len(sa["free"])]
                checks = check_point(ev, xa, xb)
                if all(verdict(c) is None for c in checks.values()):
                    chosen = {"seed": seed, "index": k, "u": u, "a": xa, "b": xb,
                              "a_bits": to_bits(xa), "b_bits": to_bits(xb),
                              "checks": summarise(checks)}
                    break
                rejected.append({"index": k, "checks": summarise(checks)})
            if chosen is None:
                raise SystemExit(f"seed {seed}: no valid point from index {first}")
            if seed in PILOT_INDEX:
                kept = chosen["index"] == PILOT_INDEX[seed]
                chosen["pilot_index"] = PILOT_INDEX[seed]
                chosen["kept_pilot_start"] = kept
                chosen["replacement"] = None if kept else {
                    "replaced_index": PILOT_INDEX[seed], "by_index": chosen["index"],
                    "reasons": rejected[0]["checks"]}
            chosen["rejected"] = rejected
            starts[f"lhs{seed}"] = chosen
            print(f"lhs{seed}: index {chosen['index']}"
                  + (f" (pilot index {PILOT_INDEX[seed]}, kept {chosen['kept_pilot_start']})"
                     if seed in PILOT_INDEX else "")
                  + f", {len(rejected)} rejected; min h A "
                  f"{chosen['checks']['a_L0']['min_h']:.4f}, B "
                  f"{chosen['checks']['b_L0']['min_h']:.4f}", flush=True)
            for r in rejected:
                bad = {k2: v["verdict"] for k2, v in r["checks"].items() if v["verdict"]}
                print(f"  rejected index {r['index']}: {json.dumps(bad)[:300]}", flush=True)
        record = {
            "issue": 1201,
            "generator": {"routine": "benchmarks/motor_optimization/harness/lhs.py "
                                     "latin_hypercube (SplitMix64); make_starts.py",
                          "points_per_seed": POINTS_PER_SEED, "dimensions": d,
                          "seeds": list(SEEDS), "levels": list(LEVELS),
                          "pilot_index": {str(k): v for k, v in PILOT_INDEX.items()},
                          "rule": "seeds 1 and 2 keep the pilot index if valid, else the next "
                                  "valid index; seeds 3 and 4 take the first valid index. "
                                  "Valid: in both spaces and on the L0 and L1 θ0 meshes, "
                                  "admissible (h >= 0), buildable, and the moved mesh passes "
                                  "the remesh policy (check request); x_A = x_B[:8]"},
            "spaces": {"a": sa, "b": sb},
            "starts": starts,
            # The preflight is geometry and mesh motion only (check
            # requests: no FEM solve); its cost is outside every run budget.
            "preflight": {"check_requests": REQUESTS[0], "fem_solves": 0,
                          "wall_seconds": time.monotonic() - t_pre,
                          "counted_in_run_budgets": False},
        }
        a.out.write_text(json.dumps(record, indent=1) + "\n")
    finally:
        for e in ev.values():
            e.close()


if __name__ == "__main__":
    main()
