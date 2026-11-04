"""Issue #1123: gradient-free controls for the #1092 re-optimization.

#1111 met DeviceLayout's stopping criterion in 3 Palace runs with a Newton
iteration whose Jacobian took its capacitor_length column from geode's
shape derivative. This driver runs the same problem (Palace at
DeviceLayout's own discretization: its mesh_order 2 mesh, solver_order 2,
its configfile verbatim; targets [3.0, 4.0] GHz; criterion
sum((f - t)^2 / t^2) / 2 < 1e-4) with two Jacobians that use no shape
derivative (the scaling variant keeps #1111's transmission-line L column, whose
delta came from #1111's one-time geode finite difference; Broyden uses no geode
input at all):

  scaling   Newton in DeviceLayout's variables x1 = sqrt(620 / c),
            x2 = 5000 / L, with the scaling-law Jacobian
                dF_q/dx1 = F_q / x1          (f_q ~ 1/sqrt(C) ~ x1)
                dF_r/dx2 = -F_r/(L + delta) * dL/dx2   (#1111's line model)
                dF_q/dx2 = dF_r/dx1 = 0,
            re-evaluated from the Palace values at every point.
  broyden   a secant method: from the initial design, one forward
            finite-difference probe per variable (dx = PROBE in x) gives the
            full 2x2 Jacobian for the first step; Broyden's ("good") rank-one
            update after every later run.
  scaling-palace
            (#1129) the scaling-law Newton with the L column's end correction
            delta taken from the Palace-only Broyden probe dl_bp2 (by the
            same f_r (L + delta) = const model, palace_delta()) instead of
            #1111's geode calibration: no geode input of any kind.
  eval      (#1129) one Palace run at DeviceLayout's discretization of
            --c / --L (exact floats) with tag --tag, printed as one JSON line
            on stdout: the evaluation prima_driver.jl calls for every point
            PRIMA asks for.
  conv-step one scaling-law Newton step from DeviceLayout's published final
            design toward the targets on the CONVERGED frequencies (the
            part-1 estimate there); prints the design for ladder.sh.

Runs are counted as #1111 counts them: the initial-geometry run is run 1
(the committed part-1 cell dl_initial, reused, as #1111 reused it), and
every later Palace evaluation adds one. A method "meets the criterion at
run n" when the n-th run's err < 1e-4.

Run from the repository root on the host with Julia and the
reference/julia/devicelayout environment; every Palace run goes to an AWS
worker through scripts/aws/geode-worker.sh:

    python3 -I benchmarks/transmon_devicelayout_controls/driver.py \
        <scaling|scaling-palace|broyden|conv-step> --scratch <dir> --worker <name> \
        [--np 16] [--remote work1123ctl]

Every evaluation is appended to <scratch>/controls_log.jsonl; meshes and
Palace outputs stay in <scratch> (the cells are then copied to
reference/fixtures/transmon_palace/results_controls_1123/). The committed
artifact is assembled from the raw cells by assemble.py.
"""

import argparse
import json
import math
import os
import subprocess
import sys
import time

TARGETS = (3.0, 4.0)
CRITERION = 1e-4  # run_optimization(reltol = 1e-2): err < reltol^2
DELTA_UM = 215.954  # #1111 [transmission_line].delta_um
PROBE = -0.05  # Broyden's first-step finite-difference probe, in x1 and x2
BOX = (0.6, 1.4)  # DeviceLayout's PRIMA box on x1, x2
WORKER_SCRIPT = "scripts/aws/geode-worker.sh"
REMOTE = "work1123ctl"  # the worker directory; --remote overrides it
CELLS_1123 = "reference/fixtures/transmon_palace/results_controls_1123"
RUN_SH = "geode-fem/reference/fixtures/transmon_palace/results_crosscheck_1092/run.sh"

INITIAL = (620.0, 5000.0)
FINAL = (1217.678, 7243.955)
# The committed part-1 Palace run at DeviceLayout's discretization of the
# initial geometry (results_crosscheck_1092/dl_initial), run 1 of every method.
DL_INITIAL = (4.168313, 5.613182)
# Part-1 converged estimate at the published final design
# (benchmarks/transmon_devicelayout_crosscheck/results.toml [convergence.final]).
CONV_FINAL = (3.2652, 4.0880)


def env():
    e = dict(os.environ)
    e["AWS_PROFILE"] = "geode-fem-bench"
    return e


def worker(args, cmd):
    r = subprocess.run(
        [WORKER_SCRIPT, "ssh", args.worker, cmd],
        capture_output=True,
        text=True,
        env=env(),
        check=False,
    )
    if r.returncode != 0:
        raise RuntimeError(f"worker command failed ({r.returncode}): {cmd}\n{r.stderr}")
    return r.stdout


def upload(args, local, remote_name):
    with open(local, "rb") as fh:
        r = subprocess.run(
            [WORKER_SCRIPT, "ssh", args.worker, f"mkdir -p ~/{REMOTE} && cat > ~/{REMOTE}/{remote_name}"],
            stdin=fh,
            capture_output=True,
            env=env(),
            check=False,
        )
    if r.returncode != 0:
        raise RuntimeError(f"upload {local}: {r.stderr}")


def gen(args, c, L, tag):
    """DeviceLayout's order-2 mesh and configfile for (c, L)."""
    t = time.time()
    with open(os.path.join(args.scratch, f"{tag}_gen.log"), "w") as fh:
        subprocess.run(
            [
                "julia",
                "--project=reference/julia/devicelayout",
                "reference/julia/generate_transmon_param.jl",
                "--cap-length",
                repr(c),
                "--total-length",
                repr(L),
                "--out-dir",
                args.scratch,
                "--tag",
                tag,
                "--dl-config",
            ],
            stdout=fh,
            stderr=subprocess.STDOUT,
            check=True,
        )
    return time.time() - t


def read_modes(eig, epr):
    """The first two modes with junction participation |p| >= 1e-6 (the
    port-sheet filter of #1106/#1111)."""
    f = [float(l.split(",")[1]) for l in eig.splitlines()[1:] if l.strip()]
    p = [abs(float(l.split(",")[-1])) for l in epr.splitlines()[1:] if l.strip()]
    return [fi for fi, pi in zip(f, p) if pi >= 1e-6][:2]


def palace_dl(args, c, L, tag):
    """One Palace run at DeviceLayout's discretization of (c, L)."""
    t_gen = gen(args, c, L, tag)
    upload(args, os.path.join(args.scratch, f"{tag}_o2.msh2"), f"{tag}_o2.msh2")
    upload(args, os.path.join(args.scratch, f"{tag}_dl_config.json"), f"palace_config_dl_{tag}.json")
    cell = f"dl_{tag}"
    t = time.time()
    worker(
        args,
        f"cd ~/{REMOTE} && WORK=$HOME/{REMOTE} IMG=palace:cpu-1043 DOCKER='sudo docker' "
        f"bash ~/{RUN_SH} {cell}:{args.np}",
    )
    eig = worker(args, f"cat ~/{REMOTE}/postpro/{cell}/eig.csv")
    epr = worker(args, f"cat ~/{REMOTE}/postpro/{cell}/port-EPR.csv")
    f = read_modes(eig, epr)
    out = {
        "kind": "palace_dl",
        "tag": tag,
        "c": c,
        "L": L,
        "f": f,
        "err": dl_error(f),
        "gen_s": round(t_gen, 1),
        "palace_wall_s": round(time.time() - t, 1),
    }
    log(args, out)
    return out


def log(args, rec):
    rec["utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    with open(os.path.join(args.scratch, "controls_log.jsonl"), "a") as fh:
        fh.write(json.dumps(rec) + "\n")
    print(json.dumps(rec), flush=True)


def dl_error(f):
    return sum((fi - ti) ** 2 / ti**2 for fi, ti in zip(f, TARGETS)) / 2


def to_x(theta):
    c, L = theta
    return (math.sqrt(620.0 / c), 5000.0 / L)


def to_theta(x):
    """Clip to DeviceLayout's box and round to 1e-3 um, as #1111 did."""
    x1 = min(max(x[0], BOX[0]), BOX[1])
    x2 = min(max(x[1], BOX[0]), BOX[1])
    return (round(620.0 / x1**2, 3), round(5000.0 / x2, 3))


def scaling_jacobian(theta, F, delta=DELTA_UM):
    """diag(F_q / x1, dF_r/dx2 of #1111's line model): no shape derivative in the c column."""
    x1, x2 = to_x(theta)
    L = theta[1]
    dL_dx2 = -5000.0 / x2**2
    return [[F[0] / x1, 0.0], [0.0, -F[1] / (L + delta) * dL_dx2]]


def palace_delta(root="."):
    """The line model's end correction from Palace runs only: f_r (L + delta)
    = const through the initial run (dl_initial, at DL_INITIAL) and the
    Broyden probe dl_bp2 (L = 5263.158 um, c unchanged), as
    assemble.py's [controls.delta_sensitivity] computes it (563.879 um)."""
    d = os.path.join(root, CELLS_1123, "dl_bp2")
    with open(os.path.join(d, "eig.csv")) as fh, open(os.path.join(d, "port-EPR.csv")) as gh:
        f = read_modes(fh.read(), gh.read())
    with open(os.path.join(d, "generator_report.txt")) as fh:
        rep = fh.read()
    L = float(rep.split("total_length:", 1)[1].split("um", 1)[0])
    f0 = DL_INITIAL[1]
    return (f[1] * L - f0 * INITIAL[1]) / (f0 - f[1])


def solve2(J, r):
    det = J[0][0] * J[1][1] - J[0][1] * J[1][0]
    return [(J[1][1] * r[0] - J[0][1] * r[1]) / det, (-J[1][0] * r[0] + J[0][0] * r[1]) / det]


def newton_step(theta, F, J):
    x = to_x(theta)
    dx = solve2(J, [TARGETS[0] - F[0], TARGETS[1] - F[1]])
    return to_theta((x[0] + dx[0], x[1] + dx[1]))


def run_scaling(args, delta=DELTA_UM, kind="scaling", prefix="sc"):
    theta, F, n = INITIAL, list(DL_INITIAL), 1
    log(args, {"kind": kind, "run": n, "delta_um": delta, "theta": theta, "F": F, "err": dl_error(F), "reused": "dl_initial"})
    for k in range(1, args.max_steps + 1):
        if dl_error(F) < CRITERION:
            break
        J = scaling_jacobian(theta, F, delta)
        theta = newton_step(theta, F, J)
        p = palace_dl(args, *theta, f"{prefix}{k}")
        n += 1
        F = p["f"]
        log(args, {"kind": kind, "run": n, "J": J, "theta": theta, "F": F, "err": dl_error(F)})


def run_scaling_palace(args):
    run_scaling(args, palace_delta(), "scaling_palace", "sp")


def run_broyden(args):
    x0 = to_x(INITIAL)
    F0 = list(DL_INITIAL)
    n = 1
    log(args, {"kind": "broyden", "run": n, "theta": INITIAL, "F": F0, "err": dl_error(F0), "reused": "dl_initial"})
    # First step: one forward probe per variable.
    J = [[0.0, 0.0], [0.0, 0.0]]
    for j in range(2):
        xp = list(x0)
        xp[j] += PROBE
        th = to_theta(xp)
        p = palace_dl(args, *th, f"bp{j + 1}")
        n += 1
        xq = to_x(th)  # the rounded design actually run
        for i in range(2):
            J[i][j] = (p["f"][i] - F0[i]) / (xq[j] - x0[j])
        log(args, {"kind": "broyden_probe", "run": n, "var": j + 1, "theta": th, "F": p["f"], "err": dl_error(p["f"])})
    theta, F = INITIAL, F0
    for k in range(1, args.max_steps + 1):
        if dl_error(F) < CRITERION:
            break
        new = newton_step(theta, F, J)
        p = palace_dl(args, *new, f"br{k}")
        n += 1
        x, xn = to_x(theta), to_x(new)
        dx = [xn[0] - x[0], xn[1] - x[1]]
        dF = [p["f"][0] - F[0], p["f"][1] - F[1]]
        Jdx = [J[0][0] * dx[0] + J[0][1] * dx[1], J[1][0] * dx[0] + J[1][1] * dx[1]]
        nn = dx[0] ** 2 + dx[1] ** 2
        J_used = [row[:] for row in J]
        J = [[J[i][j] + (dF[i] - Jdx[i]) * dx[j] / nn for j in range(2)] for i in range(2)]
        theta, F = new, p["f"]
        log(args, {"kind": "broyden", "run": n, "J": J_used, "theta": theta, "F": F, "err": dl_error(F)})


def run_eval(args):
    p = palace_dl(args, float(args.c), float(args.L), args.tag)
    print("EVAL " + json.dumps(p), flush=True)


def run_conv_step(args):
    J = scaling_jacobian(FINAL, CONV_FINAL)
    theta = newton_step(FINAL, CONV_FINAL, J)
    log(args, {"kind": "conv_step_scaling", "from": FINAL, "F_conv_from": CONV_FINAL, "J": J, "theta": theta})


def main():
    global REMOTE
    ap = argparse.ArgumentParser()
    ap.add_argument("phase", choices=["scaling", "scaling-palace", "broyden", "eval", "conv-step"])
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--worker", default="dl1123")
    ap.add_argument("--np", type=int, default=16)
    ap.add_argument("--max-steps", type=int, default=6)
    ap.add_argument("--remote", default=REMOTE)
    ap.add_argument("--c", help="eval: capacitor_length in um (a float literal, passed through exactly)")
    ap.add_argument("--L", help="eval: total_length in um")
    ap.add_argument("--tag", help="eval: the design tag (cell dl_<tag>)")
    a = ap.parse_args()
    REMOTE = a.remote
    os.makedirs(a.scratch, exist_ok=True)
    {
        "scaling": run_scaling,
        "scaling-palace": run_scaling_palace,
        "broyden": run_broyden,
        "eval": run_eval,
        "conv-step": run_conv_step,
    }[a.phase](a)


if __name__ == "__main__":
    sys.exit(main())
