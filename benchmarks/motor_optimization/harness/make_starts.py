"""Generate the committed starts of experiment 2 (issue #1195; #1185
pre-registration "θ0 plus 2 seeded Latin-hypercube points inside the valid
box, seeds 1 and 2").

    pyrun.sh make_starts.py --binary EVALUATOR [--out starts.json]

Rule (deterministic, no numpy):

- For seed s in (1, 2): a Latin hypercube of M = 10 points in the
  18-dimensional unit box of space B (`lhs.latin_hypercube(s, 10, 18)`).
- Space B's box is space A's in its first 8 components (the 8 scalars), so
  each point gives a space-B start x_B = lower + u (upper - lower) and a
  space-A start x_A = the first 8 components of x_B: the two spaces start
  from the same scalars.
- The start of seed s is the first point (in index order) that is valid in
  both spaces (`check` request on the committed L0 θ0 mesh): the
  construction curves build, the θ0 mesh moves to it, and the moved mesh
  passes the pre-registered remesh policy (no inverted triangle, minimum
  angle at least 5°). A start therefore never begins with a remesh. (A
  box point whose moved mesh inverts may have no fresh mesh at all: at the
  first candidate of seed 2, gmsh fails in space A and does not return in
  space B.) Rejected indices and their reasons are recorded.
- θ0 is the registered initial design in both spaces.
"""
import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from evaluator import Evaluator, to_bits  # noqa: E402
from lhs import latin_hypercube, to_box  # noqa: E402

ROOT = HERE.parents[2]
L0 = ROOT / "benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L0.msh"
POINTS_PER_SEED = 10
SEEDS = (1, 2)


def reason(c):
    """Why a check rejects a point (None: it does not)."""
    if not c.get("ok"):
        return c.get("error")
    if not c["geometry_ok"]:
        return c.get("curves_error") or c.get("motion_error")
    if c["trigger"] is not None:
        return {"trigger": c["trigger"]}
    return None


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--out", type=Path, default=HERE / "starts.json")
    a = ap.parse_args(argv)
    ev = {s: Evaluator([a.binary, "--space", s, "--train-mesh", str(L0), "--val-mesh", str(L0),
                        "--train-level", "0", "--val-level", "0"])
          for s in "ab"}
    try:
        sa, sb = ev["a"].ready["space"], ev["b"].ready["space"]
        assert sb["free"][:len(sa["free"])] == sa["free"]
        assert sb["lower"][:8] == sa["lower"] and sb["upper"][:8] == sa["upper"]
        d = len(sb["free"])
        starts = {"theta0": {"a": sa["x0"], "b": sb["x0"]}}
        for seed in SEEDS:
            pts = latin_hypercube(seed, POINTS_PER_SEED, d)
            rejected = []
            for k, u in enumerate(pts):
                xb = to_box(u, sb["lower"], sb["upper"])
                xa = xb[:len(sa["free"])]
                ca = ev["a"].request("check", x_bits=to_bits(xa))
                cb = ev["b"].request("check", x_bits=to_bits(xb))
                assert ca.get("x_bits", to_bits(xa)) == to_bits(xa)
                assert cb.get("x_bits", to_bits(xb)) == to_bits(xb)
                ok = all(c.get("ok") and c["geometry_ok"] and c["trigger"] is None
                         for c in (ca, cb))
                if ok:
                    starts[f"lhs{seed}"] = {
                        "seed": seed, "index": k, "u": u, "a": xa, "b": xb,
                        "a_bits": to_bits(xa), "b_bits": to_bits(xb),
                        "check_a": {k2: ca[k2] for k2 in ("quality", "trigger")},
                        "check_b": {k2: cb[k2] for k2 in ("quality", "trigger")},
                        "rejected": rejected}
                    break
                rejected.append({"index": k, "a": reason(ca), "b": reason(cb)})
            else:
                raise SystemExit(f"seed {seed}: no valid point among {POINTS_PER_SEED}")
            print(f"lhs{seed}: index {starts[f'lhs{seed}']['index']}, "
                  f"{len(rejected)} rejected; L0 moved-mesh min angle "
                  f"A {starts[f'lhs{seed}']['check_a']['quality']['min_angle_deg']:.2f}°, "
                  f"B {starts[f'lhs{seed}']['check_b']['quality']['min_angle_deg']:.2f}°")
        record = {
            "issue": 1195,
            "generator": {"routine": "benchmarks/motor_optimization/harness/lhs.py "
                                     "latin_hypercube (SplitMix64); make_starts.py",
                          "points_per_seed": POINTS_PER_SEED, "dimensions": d,
                          "seeds": list(SEEDS),
                          "rule": "first point, in index order, valid in both spaces: geometry "
                                  "exists and the moved L0 θ0 mesh passes the remesh policy "
                                  "(check request); x_A = x_B[:8]"},
            "spaces": {"a": sa, "b": sb},
            "starts": starts,
        }
        a.out.write_text(json.dumps(record, indent=1) + "\n")
    finally:
        for e in ev.values():
            e.close()


if __name__ == "__main__":
    main()
