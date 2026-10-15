#!/usr/bin/env python3
"""PRIMA (derivative-free trust region) baseline on the 38-DOF copper basis (issue #1130).

Drives `patch_conformal_nested_dof --eval-server` (optionally with
`--palace-eval`, so every band evaluation is a Palace solve) with PRIMA's
NEWUOA, the model-based trust-region family DeviceLayout.jl uses. The
problem is #1126 X4's: the soft-max |S11|^2 band objective, the 0.25 volume
guard, the basis and fixture, from x = 0.

Counting follows `--gradient-free`: one solve is one frequency of the band;
the x = 0 evaluation (done first, outside the optimizer) and the final
verification points are excluded from the trajectory counts. NEWUOA is
unconstrained and needs a value at every point it asks for, so a design that
fails the guard is answered without a solve with the penalty
`GUARD_PENALTY + (0.25 - min_vol_ratio)` (above any feasible objective,
which is at most 1 + ln(5)/25); each such evaluation is logged as infeasible.

Usage:
  palace_loop_prima.py --exe EXAMPLE_BINARY --out OUT.toml --budget 190 \\
      [--method NEWUOA] [--rhobeg 0.5] [--rhoend 1e-3] [--label L] \\
      [--host "..."] -- <patch_conformal_nested_dof flags, incl. --palace-eval>
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
import time

import numpy as np

GUARD_PENALTY = 2.0
MIN_VOL_RATIO = 0.25


class Server:
    def __init__(self, exe: str, flags: list[str], log_path: str):
        self.log = open(log_path, "w")
        self.p = subprocess.Popen(
            [exe, *flags, "--eval-server"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.log,
            text=True,
            bufsize=1,
        )
        self.hello = json.loads(self.p.stdout.readline())
        assert self.hello.get("ready"), self.hello

    def ask(self, kind: str, x) -> dict:
        self.p.stdin.write(kind + " " + " ".join(f"{v:.17e}" for v in x) + "\n")
        self.p.stdin.flush()
        line = self.p.stdout.readline()
        if not line:
            raise RuntimeError("eval-server exited; see its log")
        return json.loads(line)

    def close(self):
        try:
            self.p.stdin.write("q\n")
            self.p.stdin.flush()
        except BrokenPipeError:
            pass
        self.p.wait(timeout=600)
        self.log.close()


def worst_db(g):
    return max(10.0 * math.log10(v) for v in g)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--exe", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--budget", type=int, default=190)
    ap.add_argument("--method", default="NEWUOA")
    ap.add_argument("--rhobeg", type=float, default=0.5)
    ap.add_argument("--rhoend", type=float, default=1e-3)
    ap.add_argument("--label", default="copper_38_newuoa190")
    ap.add_argument("--host", default="")
    ap.add_argument("--server-log", default="eval_server.log")
    ap.add_argument("flags", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    flags = a.flags[1:] if a.flags and a.flags[0] == "--" else a.flags

    import prima  # noqa: PLC0415  (only needed to run, not to read)

    srv = Server(a.exe, flags, a.server_log)
    n = srv.hello["n_dofs"]
    band = srv.hello["band_ghz"]
    t_start = time.monotonic()
    e0 = srv.ask("f", np.zeros(n))
    solves_x0 = e0["solves"]
    log = []
    best = {"worst_db": math.inf, "x": np.zeros(n), "objective": math.nan}
    t0 = time.monotonic()

    def fun(x):
        if len(log) >= a.budget:
            # PRIMA respects maxfev; this is a guard, not a code path.
            raise RuntimeError("budget exceeded")
        r = srv.ask("f", x)
        rec = {
            "eval": len(log) + 1,
            "feasible": r["feasible"],
            "min_vol_ratio": r["min_vol_ratio"],
            "solves_so_far": r["solves"],
        }
        if r["feasible"]:
            val = r["objective"]
            w = r["worst_db"]
            rec["objective"] = val
            rec["worst_db"] = w
            if w < best["worst_db"]:
                best.update(worst_db=w, x=np.array(x, dtype=float), objective=val)
        else:
            val = GUARD_PENALTY + (MIN_VOL_RATIO - r["min_vol_ratio"])
        rec["best_so_far_worst_db"] = best["worst_db"]
        rec["wall_s"] = time.monotonic() - t0
        log.append(rec)
        print(
            f"    {a.method} eval {rec['eval']}: "
            + (f"G = {val:.6e} (worst {rec['worst_db']:.2f} dB)" if r["feasible"]
               else f"guard {r['min_vol_ratio']:.3f}")
            + f", best worst {best['worst_db']:.2f} dB, {rec['wall_s']:.0f} s",
            file=sys.stderr, flush=True,
        )
        return val

    res = prima.minimize(
        fun,
        np.zeros(n),
        method=a.method,
        options={"rhobeg": a.rhobeg, "rhoend": a.rhoend, "maxfev": a.budget},
    )
    opt_wall = time.monotonic() - t0
    ver = srv.ask("v", best["x"])
    srv.close()
    total_wall = time.monotonic() - t_start
    n_feas = sum(1 for r in log if r["feasible"])
    ver_db = [10.0 * math.log10(g) for g in ver["g"]]

    def num(v):
        return f"{v:.9e}"

    t = []
    t.append("# Auto-generated by benchmarks/patch_antenna_conformal/scripts/palace_loop_prima.py")
    t.append("# (issue #1130); see [meta].command. Do NOT edit by hand.\n")
    t.append("[meta]")
    t.append(
        'description = "Derivative-free trust-region baseline (issue #1130): PRIMA '
        f"{a.method} on the same soft-max |S11|^2 band objective, basis, volume guard and "
        "fixture as the adjoint L-BFGS descent and #1126 X4's sep-CMA-ES, from x = 0, at a "
        "fixed budget of band evaluations; guard failures are answered without a solve "
        f'with the penalty {GUARD_PENALTY} + (0.25 - min_vol_ratio)."'
    )
    t.append('mode = "gradient_free"')
    ext = srv.hello.get("external")
    t.append(f'forward_solver = "{"palace" if ext else "geode"}"')
    shown = " ".join(
        ("benchmarks/patch_antenna_conformal/scripts/palace_band_eval.py"
         if i > 0 and flags[i - 1] == "--palace-eval" else
         ("<dir>/" + f.rsplit("/", 1)[-1] if i > 0 and flags[i - 1] in ("--fixture", "--palace-workdir") else f))
        for i, f in enumerate(flags)
    )
    t.append(
        'command = "python3 benchmarks/patch_antenna_conformal/scripts/palace_loop_prima.py '
        f"--exe <target>/release/examples/patch_conformal_nested_dof --budget {a.budget} "
        f"--method {a.method} --rhobeg {a.rhobeg} --rhoend {a.rhoend} --label {a.label} "
        f'--out <out> -- {shown}"'
    )
    t.append(f'label = "{a.label}"')
    if a.host:
        t.append(f'host = "{a.host}"')
    t.append(f"prima_version = \"{getattr(prima, '__version__', 'unknown')}\"")
    t.append("band_points_ghz = [" + ", ".join(f"{f:.6f}" for f in band) + "]")
    t.append("")
    t.append("[gradient_free]")
    t.append(f'method = "PRIMA {a.method}"')
    t.append(f"rhobeg = {a.rhobeg}")
    t.append(f"rhoend = {a.rhoend}")
    t.append(f"n_dofs = {n}")
    t.append(f"budget_evals = {a.budget}")
    t.append(f"n_evals = {len(log)}")
    t.append(f"n_feasible = {n_feas}")
    t.append(f"prima_message = {json.dumps(str(getattr(res, 'message', '')))}")
    t.append(f"worst_s11_db_initial = {num(e0['worst_db'])}")
    t.append(f"worst_s11_db_final = {num(best['worst_db'])}")
    t.append(f"objective_final = {num(best['objective'])}")
    t.append(f"verify_worst_s11_db = {num(max(ver_db))}")
    t.append(f"n_driven_solves = {ver['solves']}")
    t.append(f"n_solves_initial_eval = {solves_x0}")
    t.append(f"optimizer_wall_s = {opt_wall:.1f}")
    t.append(f"total_wall_s = {total_wall:.1f}")
    t.append("")
    t.append("[shape]")
    t.append("design_vector = [" + ", ".join(num(v) for v in best["x"]) + "]")
    t.append("")
    for r in log:
        t.append("[[trajectory.eval]]")
        t.append(f"eval = {r['eval']}")
        t.append(f"feasible = {'true' if r['feasible'] else 'false'}")
        t.append(f"min_vol_ratio = {r['min_vol_ratio']:.6f}")
        if r["feasible"]:
            t.append(f"objective = {num(r['objective'])}")
            t.append(f"worst_db = {r['worst_db']:.6f}")
        if math.isfinite(r["best_so_far_worst_db"]):
            t.append(f"best_so_far_worst_db = {r['best_so_far_worst_db']:.6f}")
        t.append(f"solves_so_far = {r['solves_so_far']}")
        t.append(f"wall_s = {r['wall_s']:.1f}")
        t.append("")
    for f, g, s in zip(ver["f_ghz"], ver["g"], ver["s11"]):
        t.append("[[s11_verify.point]]")
        t.append(f"f_ghz = {f:.6f}")
        t.append(f"s11_re = {num(s[0])}")
        t.append(f"s11_im = {num(s[1])}")
        t.append(f"s11_db = {10.0 * math.log10(g):.6f}")
        t.append("")
    with open(a.out, "w") as fh:
        fh.write("\n".join(t))
    print(f"palace_loop_prima: {a.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
