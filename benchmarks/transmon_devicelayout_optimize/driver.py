"""Issue #1092 part 2: two-parameter re-optimization of DeviceLayout.jl's
SingleTransmon toward the example's targets [3.0, 4.0] GHz.

Run from the repository root on the machine that has Julia and the
reference/julia/devicelayout environment (mesh generation, about 27 s per
order-1 mesh); every solve runs on an AWS worker reached through
scripts/aws/geode-worker.sh:

    python3 -I benchmarks/transmon_devicelayout_optimize/driver.py \
        <phase> --scratch <dir> --worker <name>

Phases:

  calib-L   one finite difference in total_length at the initial geometry
            (620 / 5000 -> 620 / 5250 um), geode at h: the transmission-line
            end correction delta of f_r ~ 1/(L + delta).
  dl        the comparison with DeviceLayout's PRIMA run: the objective is
            DeviceLayout's own discretization (Palace on its mesh_order 2
            mesh, solver_order 2, its configfile verbatim). Inner loop:
            Newton on S * f_h(theta) = targets, with f_h the geode A2 solve
            on DeviceLayout's order-1 mesh and its Hellmann-Feynman
            capacitor_length gradient; S = f_DL / f_h is the output
            correction measured by Palace at the last outer point (at the
            start: the committed part-1 run at the initial geometry). Outer
            loop: one Palace run per inner optimum, stopping on
            DeviceLayout's own criterion (sum((f - t)^2 / t^2) / 2 < 1e-4),
            then continued to 1e-6 to locate the discretization's optimum.
  conv      the same inner loop with S = R(c), the converged-over-h ratio
            of the part-1 ladders interpolated linearly in capacitor_length
            between the initial and final geometries (both measured); the
            optimum is then evaluated by a full resolution ladder (separate
            step, see the README).

Every evaluation is appended to <scratch>/opt_log.jsonl; records, meshes and
Palace outputs stay in <scratch>. The committed artifact is assembled from
the records by examples/transmon_devicelayout_optimize.rs.
"""

import argparse
import json
import math
import os
import subprocess
import sys
import time

TARGETS = (3.0, 4.0)
DL_RELTOL = 1e-2  # run_optimization(reltol = 1e-2): stop when err < reltol^2
WORKER_SCRIPT = "scripts/aws/geode-worker.sh"
REMOTE = "work1092opt"
BIN = "geode-fem/target/release/examples/transmon_devicelayout_optimize"
HOST_NOTE = "AWS r8g.8xlarge (Graviton4, 32 vCPU, 247 GiB), Ubuntu 24.04 arm64"

# Part-1 measurements (benchmarks/transmon_devicelayout_crosscheck/results.toml).
INITIAL = (620.0, 5000.0)
FINAL = (1217.678, 7243.955)
PART1 = {
    "initial": {"h": (3.452049, 5.177592), "dl": (4.168313, 5.613182), "conv": (4.4438, 5.7404)},
    "final": {"h": (2.392688, 3.672332), "dl": (3.016352, 3.995612), "conv": (3.2652, 4.0880)},
}


def env():
    e = dict(os.environ)
    e["AWS_PROFILE"] = "geode-fem-bench"
    return e


def worker(args, cmd, stdin=None, capture=True):
    full = [WORKER_SCRIPT, "ssh", args.worker, cmd]
    r = subprocess.run(
        full,
        stdin=stdin,
        capture_output=capture,
        text=stdin is None,
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


def gen(args, c, L, tag, order2=False):
    """DeviceLayout mesh(es) for (c, L) in the scratch dir; returns paths."""
    o1 = os.path.join(args.scratch, f"{tag}_o1.msh")
    flags = ["--order1-msh41"]
    if order2:
        flags += ["--dl-config"]
    log = os.path.join(args.scratch, f"{tag}_gen.log")
    t = time.time()
    with open(log, "w") as fh:
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
                *flags,
            ],
            stdout=fh,
            stderr=subprocess.STDOUT,
            check=True,
        )
    return o1, time.time() - t


def parse_toml_floats(text):
    """Minimal reader for the flat [run] records (numbers and arrays)."""
    out = {}
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if "=" not in line or line.startswith("["):
            continue
        k, v = (s.strip() for s in line.split("=", 1))
        if v.startswith("["):
            try:
                out[k] = [float(x) for x in v.strip("[]").split(",") if x.strip()]
            except ValueError:
                pass
        else:
            try:
                out[k] = float(v)
            except ValueError:
                out[k] = v.strip('"')
    return out


def geode_eval(args, c, L, tag, fd_h=None):
    o1 = os.path.join(args.scratch, f"{tag}_o1.msh")
    if os.path.exists(o1):
        t_gen = 0.0  # written by palace_dl_eval for the same (c, L); generation is deterministic
    else:
        o1, t_gen = gen(args, c, L, tag)
    upload(args, o1, f"{tag}_o1.msh")
    envs = (
        f"GEODE_SOLVE_MESH=$HOME/{REMOTE}/{tag}_o1.msh GEODE_SOLVE_LABEL={tag} "
        f"GEODE_CAP_UM={c!r} GEODE_TOTAL_UM={L!r} GEODE_SIGMA_GHZ=4.0 "
        f"GEODE_RUNS_DIR=$HOME/{REMOTE}/opt_runs GEODE_HOST_NOTE='{HOST_NOTE}'"
    )
    if fd_h is not None:
        envs += f" GEODE_FD_H_UM={fd_h!r}"
    t = time.time()
    worker(
        args,
        f"cd ~/{REMOTE} && {envs} /usr/bin/time -v -o time_{tag}.txt ~/{BIN} > solve_{tag}.log 2>&1; "
        f"tail -3 solve_{tag}.log",
    )
    rec = worker(args, f"cat ~/{REMOTE}/opt_runs/{tag}.toml")
    os.makedirs(os.path.join(args.scratch, "opt_runs"), exist_ok=True)
    with open(os.path.join(args.scratch, "opt_runs", f"{tag}.toml"), "w") as fh:
        fh.write(rec)
    d = parse_toml_floats(rec)
    out = {
        "kind": "geode",
        "tag": tag,
        "c": c,
        "L": L,
        "f": d["f_ghz"][:2],
        "dfdc": d["df_dc_ghz_per_um"],
        "gen_s": round(t_gen, 1),
        "solve_wall_s": round(time.time() - t, 1),
    }
    if "fd_df_dc_ghz_per_um" in d:
        out["fd_dfdc"] = d["fd_df_dc_ghz_per_um"]
    log(args, out)
    return out


def palace_dl_eval(args, c, L, tag):
    """Palace at DeviceLayout's discretization (its configfile, verbatim)."""
    _, t_gen = gen(args, c, L, tag, order2=True)
    upload(args, os.path.join(args.scratch, f"{tag}_o2.msh2"), f"{tag}_o2.msh2")
    upload(args, os.path.join(args.scratch, f"{tag}_dl_config.json"), f"palace_config_dl_{tag}.json")
    cell = f"dl_{tag}"
    t = time.time()
    worker(
        args,
        f"cd ~/{REMOTE} && WORK=$HOME/{REMOTE} IMG=palace:cpu-1043 DOCKER='sudo docker' "
        f"bash ~/geode-fem/reference/fixtures/transmon_palace/results_crosscheck_1092/run.sh {cell}:32",
    )
    eig = worker(args, f"cat ~/{REMOTE}/postpro/{cell}/eig.csv")
    epr = worker(args, f"cat ~/{REMOTE}/postpro/{cell}/port-EPR.csv")
    dst = os.path.join(args.scratch, "palace", cell)
    os.makedirs(dst, exist_ok=True)
    for name, text in (("eig.csv", eig), ("port-EPR.csv", epr)):
        with open(os.path.join(dst, name), "w") as fh:
            fh.write(text)
    for name in ("palace_run.log", "time.txt"):
        remote = f"palace_run_{cell}.log" if name == "palace_run.log" else f"time_{cell}.txt"
        with open(os.path.join(dst, name), "w") as fh:
            fh.write(worker(args, f"cat ~/{REMOTE}/{remote}"))
    f = [float(l.split(",")[1]) for l in eig.splitlines()[1:]]
    p = [abs(float(l.split(",")[-1])) for l in epr.splitlines()[1:]]
    modes = [fi for fi, pi in zip(f, p) if pi >= 1e-6]
    out = {
        "kind": "palace_dl",
        "tag": tag,
        "c": c,
        "L": L,
        "f": modes[:2],
        "gen_s": round(t_gen, 1),
        "palace_wall_s": round(time.time() - t, 1),
    }
    log(args, out)
    return out


def log(args, rec):
    rec["utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    with open(os.path.join(args.scratch, "opt_log.jsonl"), "a") as fh:
        fh.write(json.dumps(rec) + "\n")
    print(json.dumps(rec), flush=True)


def dl_error(f):
    return sum((fi - ti) ** 2 / ti**2 for fi, ti in zip(f, TARGETS)) / 2


def newton(args, theta, scale, dscale, delta, prefix, tol=1e-4, max_it=8):
    """Newton on scale(c) * f_h(theta) = targets.

    Jacobian: d/dc from the Hellmann-Feynman gradient (and dscale/dc);
    d f_r / dL = -f_r / (L + delta) (transmission line), d f_q / dL = 0.
    """
    c, L = theta
    evals = []
    for k in range(max_it):
        g = geode_eval(args, c, L, f"{prefix}{k}")
        evals.append(g)
        s = scale(c)
        ds = dscale(c)
        F = [s[i] * g["f"][i] for i in range(2)]
        r = [(F[i] - TARGETS[i]) / TARGETS[i] for i in range(2)]
        print(f"  {prefix}{k}: c={c:.3f} L={L:.3f} F={F} rel={r}", flush=True)
        if max(abs(x) for x in r) < tol:
            return (c, L), evals, F
        a = s[0] * g["dfdc"][0] + ds[0] * g["f"][0]  # dFq/dc
        b = s[1] * g["dfdc"][1] + ds[1] * g["f"][1]  # dFr/dc
        e = -F[1] / (L + delta)  # dFr/dL
        res = [F[0] - TARGETS[0], F[1] - TARGETS[1]]
        dc = -res[0] / a
        dL = -(res[1] + b * dc) / e
        # Keep the step inside DeviceLayout's PRIMA box (x in [0.6, 1.4]).
        c = min(max(c + dc, 620.0 / 1.4**2), 620.0 / 0.6**2)
        L = min(max(L + dL, 5000.0 / 1.4), 5000.0 / 0.6)
    return (c, L), evals, F


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("phase", choices=["calib-L", "fdcheck", "dl", "conv"])
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--worker", default="tq1092")
    ap.add_argument("--delta", type=float, default=None)
    ap.add_argument("--step", type=int, default=1)
    ap.add_argument("--start", type=float, nargs=2, default=None)
    ap.add_argument("--start-conv", type=float, nargs=2, default=None)
    a = ap.parse_args()
    os.makedirs(a.scratch, exist_ok=True)

    if a.phase == "fdcheck":
        geode_eval(a, *INITIAL, "fd_initial", fd_h=0.5)
        return
    if a.phase == "calib-L":
        g0 = geode_eval(a, *INITIAL, "calibL_5000")
        g1 = geode_eval(a, 620.0, 5250.0, "calibL_5250")
        # f_r (L + delta) constant: delta = (f1 L1 - f0 L0) / (f0 - f1).
        f0, f1 = g0["f"][1], g1["f"][1]
        delta = (f1 * 5250.0 - f0 * 5000.0) / (f0 - f1)
        log(a, {"kind": "calib_L", "delta_um": delta, "dfq_dL": (g1["f"][0] - g0["f"][0]) / 250.0})
        # Remesh check of the shape gradient: DeviceLayout's own
        # parametrization, c = 620 -> 650 um, against the mean of the
        # Hellmann-Feynman gradients at the two ends.
        g2 = geode_eval(a, 650.0, 5000.0, "calibC_650")
        fd = [(g2["f"][i] - g0["f"][i]) / 30.0 for i in range(2)]
        hf = [0.5 * (g0["dfdc"][i] + g2["dfdc"][i]) for i in range(2)]
        log(a, {"kind": "remesh_check_c", "fd_remesh": fd, "hf_mean": hf})
        return
    assert a.delta is not None, "--delta from calib-L"

    if a.phase == "dl":
        # Newton on DeviceLayout's own discretization F(theta) (Palace on its
        # mesh_order 2 mesh, solver_order 2). The Jacobian's c column is
        # geode's Hellmann-Feynman gradient at h scaled by the measured
        # fidelity ratio S = F / f_h at the same point; its L column is the
        # transmission line, dF_r/dL = -F_r / (L + delta), dF_q/dL = 0.
        # One geode solve and one Palace run per step; point 0 reuses the
        # committed part-1 Palace run at the initial geometry (DeviceLayout's
        # count includes its initial run too).
        theta = INITIAL
        F = list(PART1["initial"]["dl"])
        n_palace = 1
        g = geode_eval(a, *theta, "dl0")
        for k in range(1, 9):
            err = dl_error(F)
            log(a, {"kind": "newton_dl", "k": k - 1, "theta": theta, "F": F, "f_h": g["f"],
                    "dfdc_h": g["dfdc"], "err": err, "n_palace": n_palace})
            if err < 1e-6:
                break
            c, L = theta
            # Newton in DeviceLayout's own optimizer variables,
            # x1 = sqrt(620 / c), x2 = 5000 / L, in which F is close to
            # linear (the example's comment), with its box [0.6, 1.4].
            x1, x2 = math.sqrt(620.0 / c), 5000.0 / L
            dc_dx1 = -2.0 * 620.0 / x1**3
            dL_dx2 = -5000.0 / x2**2
            S = [F[i] / g["f"][i] for i in range(2)]
            jq_1 = S[0] * g["dfdc"][0] * dc_dx1
            jr_1 = S[1] * g["dfdc"][1] * dc_dx1
            jr_2 = -F[1] / (L + a.delta) * dL_dx2
            dx1 = -(F[0] - TARGETS[0]) / jq_1
            dx2 = -((F[1] - TARGETS[1]) + jr_1 * dx1) / jr_2
            x1 = min(max(x1 + dx1, 0.6), 1.4)
            x2 = min(max(x2 + dx2, 0.6), 1.4)
            theta = (round(620.0 / x1**2, 3), round(5000.0 / x2, 3))
            p = palace_dl_eval(a, *theta, f"dl{k}")
            n_palace += 1
            F = p["f"]
            g = geode_eval(a, *theta, f"dl{k}")
        return

    if a.phase == "conv":
        # One Newton step toward the targets on the CONVERGED frequencies,
        # from the published final geometry, whose converged pair the part-1
        # ladder measured. Same Jacobian model as the dl phase, scaled by
        # R = F_conv / f_h at that point. The step's prediction is evaluated
        # by a full resolution ladder (ladder.sh), which also gives the
        # converged pair for a second step if one is needed.
        start = FINAL if a.start is None else tuple(a.start)
        F = list(PART1["final"]["conv"]) if a.start is None else list(a.start_conv)
        g = geode_eval(a, *start, f"cv{a.step}_at")
        c, L = start
        x1, x2 = math.sqrt(620.0 / c), 5000.0 / L
        dc_dx1 = -2.0 * 620.0 / x1**3
        dL_dx2 = -5000.0 / x2**2
        S = [F[i] / g["f"][i] for i in range(2)]
        jq_1 = S[0] * g["dfdc"][0] * dc_dx1
        jr_1 = S[1] * g["dfdc"][1] * dc_dx1
        jr_2 = -F[1] / (L + a.delta) * dL_dx2
        dx1 = -(F[0] - TARGETS[0]) / jq_1
        dx2 = -((F[1] - TARGETS[1]) + jr_1 * dx1) / jr_2
        theta = (round(620.0 / (x1 + dx1) ** 2, 3), round(5000.0 / (x2 + dx2), 3))
        log(a, {"kind": "conv_step", "step": a.step, "from": start, "F_conv_from": F, "f_h_from": g["f"],
                "dfdc_h": g["dfdc"], "S": S, "theta": theta})
        return

if __name__ == "__main__":
    sys.exit(main())
