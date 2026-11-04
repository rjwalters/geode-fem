"""Finite-difference check of the evaluator's gradient replies (issue
#1195), through the evaluator process and the JSON protocol, at an interior
point of a frozen space.

    pyrun.sh fd_check.py --binary EVALUATOR --space a|b --level L
                         [--newton default|tight] --out RECORD.json

The training model (h45 coupling torque, g20) at level L on the θ0 mesh of
that level. At the interior point x0 + 0.25 (centre - x0) the `train`
reply with `gradient: true` gives dF and dmean_full; central differences of
the value replies are taken along the fixed direction of
tests/motor_experiment2_harness.rs (steps 1e-3 and 1e-4 in t) and along
single components (step 1e-6 in the component's unit). Every stencil point
must stay on the θ0 mesh (generation 0), so that the differences are on one
mesh. CI runs the same check at L0 with 4 angles (the
`motor_experiment2_harness` target).
"""
import argparse
import json
import math
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from evaluator import Evaluator, to_bits  # noqa: E402
from run import thread_env  # noqa: E402

ROOT = HERE.parents[2]
MESHES = ROOT / "benchmarks/motor_reference/geometry/meshes"
COMPONENTS = {"a": ["MAG", "SW2", "Sr1"], "b": ["MAG", "SW2", "CPR1", "CPS1"]}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--space", required=True, choices=["a", "b"])
    ap.add_argument("--level", type=int, required=True)
    ap.add_argument("--newton", default="default", choices=["default", "tight"])
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    mesh = MESHES / f"icem_initial_sector_L{a.level}.msh"
    argv_ev = [a.binary, "--space", a.space, "--train-mesh", str(mesh), "--val-mesh", str(mesh),
               "--train-level", str(a.level), "--val-level", str(a.level),
               "--newton", a.newton, "--mesh-out", str(a.out.with_suffix(".meshes"))]
    t0 = time.monotonic()
    ev = Evaluator(argv_ev, env=thread_env())
    try:
        sp = ev.ready["space"]
        lo, hi, x0 = sp["lower"], sp["upper"], sp["x0"]
        n = len(x0)
        x = [v + 0.25 * (0.5 * (l + h) - v) for v, l, h in zip(x0, lo, hi)]
        d = [(((k + 1.0) * 0.618_033_988_75) % 1.0 * 2.0 - 1.0) * (hi[k] - lo[k])
             for k in range(n)]

        def train(xs, gradient=False):
            r = ev.request("train", x_bits=to_bits(xs), gradient=gradient,
                           purpose="iterate" if gradient else "line_search")
            if not r.get("ok"):
                raise SystemExit(f"train failed: {r.get('error')}")
            if r["generation"] != 0:
                raise SystemExit("a stencil point left the θ0 mesh (remesh)")
            return r

        base = train(x, True)
        g, dm = base["dF"], base["dmean_full"]
        dot = lambda u: sum(p * q for p, q in zip(u, d))  # noqa: E731
        ad_f, ad_m = dot(g), dot(dm)
        rows = []
        for h in (1e-3, 1e-4):
            p = train([v + h * w for v, w in zip(x, d)])
            m = train([v - h * w for v, w in zip(x, d)])
            fd_f = (p["F"] - m["F"]) / (2 * h)
            fd_m = (p["mean_full"] - m["mean_full"]) / (2 * h)
            row = {"h": h, "reply_dF": ad_f, "fd_dF": fd_f,
                   "rel_dF": abs(fd_f - ad_f) / abs(ad_f),
                   "reply_dmean_full": ad_m, "fd_dmean_full": fd_m,
                   "rel_dmean_full": abs(fd_m - ad_m) / abs(ad_m)}
            rows.append(row)
            print(f"space {a.space} L{a.level} {a.newton} interior dir h {h:e}: "
                  f"dF/dt reply {ad_f:.9e} FD {fd_f:.9e} rel {row['rel_dF']:.2e}; "
                  f"dmean(1)/dt reply {ad_m:.9e} FD {fd_m:.9e} rel {row['rel_dmean_full']:.2e}",
                  flush=True)
        comps = []
        for name in COMPONENTS[a.space]:
            k = sp["free"].index(name)
            h = 1e-6
            xp, xm = list(x), list(x)
            xp[k] += h
            xm[k] -= h
            p, m = train(xp), train(xm)
            fd = (p["F"] - m["F"]) / (2 * h)
            fdm = (p["mean_full"] - m["mean_full"]) / (2 * h)
            row = {"name": name, "h": h, "reply_dF": g[k], "fd_dF": fd,
                   "rel_dF": abs(fd - g[k]) / abs(g[k]) if g[k] else math.inf,
                   "reply_dmean_full": dm[k], "fd_dmean_full": fdm,
                   "rel_dmean_full": abs(fdm - dm[k]) / abs(dm[k]) if dm[k] else math.inf}
            comps.append(row)
            print(f"space {a.space} L{a.level} {a.newton} {name} h {h:e}: dF reply {g[k]:.9e} "
                  f"FD {fd:.9e} rel {row['rel_dF']:.2e}; dmean(1) reply {dm[k]:.9e} FD "
                  f"{fdm:.9e} rel {row['rel_dmean_full']:.2e}", flush=True)
        record = {"issue": 1195, "check": "evaluator gradient replies vs central differences",
                  "space": a.space, "level": a.level, "newton": a.newton,
                  "session": ev.ready["session"], "train_msh_sha256": ev.ready["train_msh_sha256"],
                  "point": "interior x0 + 0.25 (centre - x0)", "x": x, "x_bits": to_bits(x),
                  "direction": d, "F": base["F"], "mean_full": base["mean_full"],
                  "dF": g, "dmean_full": dm, "quality": base["quality"],
                  "directional": rows, "components": comps,
                  "wall_s": time.monotonic() - t0, "load": os.getloadavg(),
                  "threads": {"RAYON_NUM_THREADS": "1", "GEODE_NUM_THREADS": "1"}}
        a.out.parent.mkdir(parents=True, exist_ok=True)
        a.out.write_text(json.dumps(record, indent=1) + "\n")
    finally:
        ev.close()


if __name__ == "__main__":
    main()
