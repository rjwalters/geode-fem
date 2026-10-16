#!/usr/bin/env python3
"""Full-covariance CMA-ES baseline on the copper basis (issue #1147, B3).

Drives `patch_conformal_nested_dof --eval-server` with pycma's
`CMAEvolutionStrategy` (full covariance, pycma's defaults otherwise) on the
problem of #1126 X4: the soft-max |S11|^2 band objective, the 0.25 volume
guard, the basis and fixture, from x = 0.

The rules are those of the in-house sep-CMA-ES (`--gradient-free`), so the
records are comparable row for row:

* lambda = 4 + floor(3 ln n) (pycma's default too), the same sigma0 and a
  fixed budget of evaluations; the budget may end inside a generation; that
  partial generation is evaluated, and it updates the sep-CMA-ES state only
  if it holds at least mu samples (pycma is not told).
* A design that fails the guard costs an evaluation index but no solve. It
  is ranked after every feasible design by its guard violation (fitness
  1e6 + (0.25 - min_vol_ratio)), exactly as `sep_cma_es` ranks it.
* Solves are the server's running total on the `--gradient-free` rule (one
  per band point and factorization); the x = 0 evaluation, made first and
  outside the optimizer, is `n_solves_initial_eval` and is subtracted by the
  summary. The verification points come after the trajectory.

pycma's own stopping criteria are left on except `maxfevals` (= the budget)
and the flat-fitness stop; its stop message is recorded.

Usage:
  cma_loop.py --exe EXAMPLE_BINARY --out OUT.toml --budget 190 --sigma0 1.5 \\
      --seed 11 --label L [--host "..."] -- <patch_conformal_nested_dof flags>
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
import time

import numpy as np

MIN_VOL_RATIO = 0.25
GUARD_RANK = 1e6


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


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--exe", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--budget", type=int, default=190)
    ap.add_argument("--sigma0", type=float, default=1.5)
    ap.add_argument("--seed", type=int, required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument("--host", default="")
    ap.add_argument("--server-log", default="eval_server.log")
    ap.add_argument("flags", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    flags = a.flags[1:] if a.flags and a.flags[0] == "--" else a.flags

    import cma  # noqa: PLC0415  (only needed to run, not to read)

    srv = Server(a.exe, flags, a.server_log)
    n = srv.hello["n_dofs"]
    band = srv.hello["band_ghz"]
    lam = 4 + int(math.floor(3.0 * math.log(n)))
    t_start = time.monotonic()
    e0 = srv.ask("f", np.zeros(n))
    solves_x0 = e0["solves"]
    opts = {
        "seed": a.seed,
        "popsize": lam,
        "CMA_diagonal": False,
        "maxfevals": a.budget,
        "tolflatfitness": 10**9,
        "verbose": -9,
        "verb_log": 0,
        "verb_disp": 0,
    }
    es = cma.CMAEvolutionStrategy(np.zeros(n), a.sigma0, opts)
    log = []
    best = {"worst_db": math.inf, "x": np.zeros(n), "objective": math.nan}
    t0 = time.monotonic()
    generation = 0
    while len(log) < a.budget and not es.stop():
        xs = es.ask()
        fit = []
        for x in xs:
            if len(log) >= a.budget:
                break
            r = srv.ask("f", x)
            rec = {
                "eval": len(log) + 1,
                "generation": generation,
                "feasible": r["feasible"],
                "min_vol_ratio": r["min_vol_ratio"],
                "solves_so_far": r["solves"],
            }
            if r["feasible"]:
                rec["objective"] = r["objective"]
                rec["worst_db"] = r["worst_db"]
                if r["worst_db"] < best["worst_db"]:
                    best.update(worst_db=r["worst_db"], x=np.array(x, dtype=float),
                                objective=r["objective"])
                fit.append(r["objective"])
            else:
                fit.append(GUARD_RANK + (MIN_VOL_RATIO - r["min_vol_ratio"]))
            rec["best_so_far_worst_db"] = best["worst_db"]
            rec["wall_s"] = time.monotonic() - t0
            log.append(rec)
            print(
                f"    cma gen {generation} eval {rec['eval']}: "
                + (f"G = {rec['objective']:.6e} (worst {rec['worst_db']:.2f} dB)"
                   if r["feasible"] else f"guard {r['min_vol_ratio']:.3f}")
                + f", sigma {es.sigma:.3e}, best worst {best['worst_db']:.2f} dB",
                file=sys.stderr, flush=True,
            )
        if len(fit) < len(xs):
            break
        es.tell(xs, fit)
        generation += 1
    stop = {k: (v if isinstance(v, (int, float, str)) else str(v)) for k, v in es.stop().items()}
    message = (
        "pycma stop: " + json.dumps(stop) if stop
        else f"stopped by the driver at its budget of {a.budget} evaluations"
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
    t.append("# Auto-generated by benchmarks/patch_antenna_conformal/scripts/cma_loop.py")
    t.append("# (issue #1147); see [meta].command. Do NOT edit by hand.\n")
    t.append("[meta]")
    t.append(
        'description = "Full-covariance CMA-ES baseline (issue #1147, B3): pycma on the same '
        "soft-max |S11|^2 band objective, basis, volume guard and fixture as the adjoint L-BFGS "
        "descent and #1126 X4's sep-CMA-ES, from x = 0, at a fixed budget of band evaluations; "
        'guard failures are ranked last by their violation, without a solve."'
    )
    t.append('mode = "gradient_free"')
    t.append(f'forward_solver = "{"palace" if srv.hello.get("external") else "geode"}"')
    shown = " ".join(
        ("<dir>/" + f.rsplit("/", 1)[-1] if i > 0 and flags[i - 1] == "--fixture" else f)
        for i, f in enumerate(flags)
    )
    t.append(
        'command = "python3 benchmarks/patch_antenna_conformal/scripts/cma_loop.py '
        "--exe <target>/release/examples/patch_conformal_nested_dof "
        f"--budget {a.budget} --sigma0 {a.sigma0} --seed {a.seed} --label {a.label} "
        f'--out <out> -- {shown}"'
    )
    t.append(f'label = "{a.label}"')
    for k in ("backend", "float_dtype", "generated_at_commit"):
        if k in srv.hello:
            t.append(f'{k} = "{srv.hello[k]}"')
    if a.host:
        t.append(f'host = "{a.host}"')
    t.append(f'pycma_version = "{cma.__version__}"')
    t.append("band_points_ghz = [" + ", ".join(f"{f:.6f}" for f in band) + "]")
    t.append("")
    t.append("[gradient_free]")
    t.append('method = "CMA-ES (full covariance, pycma)"')
    t.append(f"seed = {a.seed}")
    t.append(f"sigma0 = {a.sigma0}")
    t.append(f"n_dofs = {n}")
    t.append(f"lambda = {lam}")
    t.append(f"cma_active = {'true' if es.opts['CMA_active'] else 'false'}")
    t.append(f"budget_evals = {a.budget}")
    t.append(f"n_evals = {len(log)}")
    t.append(f"n_generations_told = {generation}")
    t.append(f"n_feasible = {n_feas}")
    t.append(f"stop_message = {json.dumps(message)}")
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
        t.append(f"generation = {r['generation']}")
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
    print(f"cma_loop: {a.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
