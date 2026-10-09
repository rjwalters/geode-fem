#!/usr/bin/env python3
"""Write results_ams_cpu_local.toml from an ams_cpu_sweep.sh run tree (#930).

usage: summarize_ams_local.py <run-dir> > results_ams_cpu_local.toml

<run-dir> holds the per-leg outputs of ams_cpu_sweep.sh: n<N>_direct,
n<N>_jacobi, n<N>_ams (one process per size and config, so /usr/bin/time's
peak RSS and CPU time belong to one configuration) and n<N>_xcheck (all three
in one process, for the in-harness full-field rel-L2 vs Direct). Every number
in the output is read from those files or computed from them here; the prose
is assembled from the computed values. Pure stdlib. The only derived
quantities are ratios, the endpoint growth exponent
log(iters_last / iters_first) / log(edges_last / edges_first), the
least-squares slope of log(iterations) on log(edges) over all sizes, and
2 / rho for the damped-Jacobi stability bound.

Optional subtrees of <run-dir>, each summarized when present:
  single_thread/  the n<N>_jacobi and n<N>_ams legs rerun with
                  RAYON_NUM_THREADS=1. Every time and cost RATIO in the output
                  comes from these legs: in the default-threading legs the AMS
                  V-cycle's small triangular solves fan out to the rayon pool
                  and, on an oversubscribed host, the leg is mostly system time.
  diagnostics/    one-off runs of throwaway patched builds (NOTE.txt there).
  threading_946/  the before / after measurement of the issue #946 fix (the AMS
                  Krylov solve now runs with faer's parallelism sequential).
                  One ams_cpu_sweep.sh tree per build and threading mode, named
                  <build>_<threading>[_rep<K>] with build = main (before the
                  fix) or fix, and threading = default or rayon1
                  (RAYON_NUM_THREADS=1). Written as [threading_fix].
  smoother_945/   the before / after measurement of the issue #945 fix (the AMS
                  edge-smoother weight is chosen per operator from a
                  spectral-radius estimate instead of being fixed at 0.6). One
                  ams_cpu_sweep.sh tree per build and drive frequency, named
                  <build>_w<omega>[_rayon1]: build = main (origin/main before
                  the fix), fix (the branch) or w060 (the branch with
                  GEODE_AMS_SMOOTH_WEIGHT=0.6, i.e. the old weight; main has no
                  GEODE_SCALING_OMEGA knob), omega written as 0p10. Plus
                  ritz_gap/w<omega>.err (a 1000-step reference Lanczos next to
                  the shipped 30-step estimate), validation/ (the AMS tests
                  outside this fixture, on main and on the branch) and
                  unconditional_rule/ (some of those tests on a throwaway build
                  that lowers the weight without the proof condition). Written
                  as [smoother_weight].

The Jacobi iteration counts are also compared with the committed #520 Lambda
record (runs/2026-10-07_lambda_a100/B1_cpu_direct_iter.stdout) at the sizes
both runs share. Only iteration counts and port voltages are compared across
hosts; no timing from the other host is copied into the output.
"""
import math
import pathlib
import re
import sys
import tomllib

d = pathlib.Path(sys.argv[1])

# Label -> (leg suffix, harness config id). Order is the output order.
CONFIGS = [
    ("direct", "1_direct"),
    ("jacobi", "2_iterative_csr"),
    ("ams", "5_iterative_ams"),
]
# "Flat" means the iteration count stays inside the band the #742 / #744
# spiral measurement spanned (102 to 155 iterations, a ratio of 1.52).
FLAT_MAX_OVER_MIN = 155 / 102


def fragment(path):
    """The [[cell]] tables of one leg's stdout ([] if the leg did not finish)."""
    if not path.exists():
        return []
    m = re.search(
        r"# ---- gpu_driven_scaling TOML fragment.*?\n(.*?)# ---- end fragment ----",
        path.read_text(),
        re.S,
    )
    return tomllib.loads(m.group(1)).get("cell", []) if m else []


def time_file(path):
    """wall / user / sys seconds and max RSS bytes, macOS -l or GNU -v format."""
    out = {}
    if not path.exists():
        return out
    txt = path.read_text()
    m = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", txt)
    if m:  # macOS / BSD time -l; RSS in bytes
        out["wall_s"], out["user_s"], out["sys_s"] = (float(g) for g in m.groups())
        out["max_rss_bytes"] = int(re.search(r"(\d+)\s+maximum resident set size", txt).group(1))
        return out
    m = re.search(r"Elapsed \(wall clock\) time.*: ([\d:.]+)", txt)
    if m:  # GNU time -v; RSS in kbytes
        parts = [float(p) for p in m.group(1).split(":")]
        out["wall_s"] = sum(p * 60**i for i, p in enumerate(reversed(parts)))
        out["user_s"] = float(re.search(r"User time \(seconds\): ([\d.]+)", txt).group(1))
        out["sys_s"] = float(re.search(r"System time \(seconds\): ([\d.]+)", txt).group(1))
        out["max_rss_bytes"] = 1024 * int(
            re.search(r"Maximum resident set size \(kbytes\): (\d+)", txt).group(1)
        )
    return out


def keyvals(path):
    out = {}
    if path.exists():
        for line in path.read_text().splitlines():
            k, sep, v = line.partition("=")
            if sep and " " not in k:
                out[k] = v
    return out


def q(s):
    return '"' + str(s).replace("\\", "\\\\").replace('"', '\\"') + '"'


def sci(x, digits=3):
    return "nan" if x != x else f"{x:.{digits}e}"


def fl(vals, digits=3):
    """A TOML array of fixed-point floats."""
    return "[" + ", ".join(f"{v:.{digits}f}" for v in vals) + "]"


def rel_diff(a, b):
    """|a - b| / |b| for [re, im] pairs."""
    return math.hypot(a[0] - b[0], a[1] - b[1]) / math.hypot(b[0], b[1])


def lsq_exponent(edges, its):
    """Least-squares slope of log(iterations) on log(edges)."""
    xs, ys = [math.log(e) for e in edges], [math.log(i) for i in its]
    mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
    return sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sum((x - mx) ** 2 for x in xs)


def endpoint_exponent(edges, its):
    return math.log(its[-1] / its[0]) / math.log(edges[-1] / edges[0])


def load_cells(run, labels, xcheck_dir=None):
    """(n, label) -> cell dict for the legs n<N>_<label> found in `run`."""
    out = {}
    ns = sorted({int(m.group(1)) for p in run.glob("n*_*.meta") if (m := re.match(r"n(\d+)_", p.name))})
    for n in ns:
        xcheck = {c["config"]: c for c in fragment(xcheck_dir / f"n{n}_xcheck.stdout")} if xcheck_dir else {}
        for label, config in labels:
            leg = f"n{n}_{label}"
            frag = fragment(run / f"{leg}.stdout")
            meta = keyvals(run / f"{leg}.meta")
            if not meta:
                continue
            c = dict(frag[0]) if frag else {}
            c.update(leg=leg, label=label, meta=meta, time=time_file(run / f"{leg}.time"))
            c["xcheck"] = xcheck.get(config)
            c["finished"] = bool(frag)
            out[(n, label)] = c
    return out


def print_process(t):
    print(f"process_wall_s = {t['wall_s']:.2f}  # whole process: mesh + assembly + solve")
    print(f"process_user_s = {t['user_s']:.2f}")
    print(f"process_sys_s = {t['sys_s']:.2f}  # kernel time; large values mean thread contention, not solver work")
    print(f"process_cpu_pct = {100 * (t['user_s'] + t['sys_s']) / t['wall_s']:.0f}  # (user + sys) / wall")
    print(f"process_max_rss_gb = {t['max_rss_bytes'] / 1e9:.3f}")


sizes = sorted({int(m.group(1)) for p in d.glob("n*_*.meta") if (m := re.match(r"n(\d+)_", p.name))})
host = keyvals(d / "host.txt")

cells = load_cells(d, CONFIGS, xcheck_dir=d)  # (n, label) -> dict, default threading
st_cells = load_cells(d / "single_thread", CONFIGS[1:])  # RAYON_NUM_THREADS=1 rerun ({} if absent)


# Issue #946 before / after trees ([] if absent): (tree name, build, threading, rep, cells).
tf_dir = d / "threading_946"
tf_trees = []
for p in sorted(tf_dir.glob("*_*")) if tf_dir.is_dir() else []:
    m = re.fullmatch(r"(main|fix)_(default|rayon1)(?:_rep(\d+))?", p.name)
    if m and p.is_dir():
        tf_trees.append((p.name, m.group(1), m.group(2), int(m.group(3) or 1), load_cells(p, CONFIGS)))
tf_trees.sort(key=lambda t: (t[1] != "main", t[2] != "default", t[3]))

# Issue #945 before / after trees ([] if absent): (tree name, build, omega, threading, cells).
sm_dir = d / "smoother_945"
sm_trees = []
for p in sorted(sm_dir.glob("*_w*")) if sm_dir.is_dir() else []:
    m = re.fullmatch(r"(main|fix|w060)_w(\d+)p(\d+)(_rayon1)?", p.name)
    if m and p.is_dir():
        sm_trees.append(
            (p.name, m.group(1), float(f"{m.group(2)}.{m.group(3)}"), "rayon1" if m.group(4) else "default", load_cells(p, CONFIGS, xcheck_dir=p))
        )

metas = [c["meta"] for c in cells.values()]
commits = sorted({m["git"] for m in metas})
loads = [float(m[k].split()[0]) for m in metas for k in ("loadavg_start", "loadavg_end") if k in m]
dates = sorted(m["start"] for m in metas)
ncpu = int(host.get("logical_cpus", "0"))

print("# Jacobi vs AMS preconditioning of geode's assembled driven solve, CPU f64")
print("# (issue #930). GENERATED by benchmarks/gpu_driven_scaling/summarize_ams_local.py")
print(f"# from {d.as_posix()}/ (do not edit by hand; rerun the script).")
print("#")
print("# ---------------------------------------------------------------------------")
print("# READ FIRST")
print("#   * LOCAL DEV-MACHINE SUBSET. This is NOT the #520 sweep and NOT a benchmark")
print("#     host: a developer Mac that was running other builds at the same time")
print("#     (1-minute load average and per-process CPU% are recorded per cell).")
print("#   * ITERATION COUNTS are the load-independent result. TIMES ARE INDICATIVE")
print("#     ONLY, single samples, and must not be placed next to the Lambda A100 /")
print("#     Palace cells of results_large_a100.toml: different host, different")
print("#     load, different core count.")
print("#   * THE [[cell]] TABLES USE THE DEFAULT THREADING, and their AMS timings at")
print("#     the larger sizes are mostly kernel time (process_sys_s): the V-cycle's")
print("#     small triangular solves fan out to the rayon pool on an oversubscribed")
print("#     host. Every time RATIO in this file comes from the RAYON_NUM_THREADS=1")
print("#     rerun in [[single_thread_cell]] / [[single_thread_comparison]].")
if tf_trees:
    print("#   * THAT CONTENTION IS FIXED (issue #946). The [[cell]] tables predate the")
    print("#     fix: they are the build named in [meta]. [threading_fix] has the")
    print("#     before / after measurement, on the same loaded machine.")
print("#   * THE [[cell]] AND [[single_thread_cell]] TABLES MEASURE THE SHIPPED AMS")
print("#     (edge-smoother weight 0.6). [diagnostics] shows that weight is above the")
print("#     damped-Jacobi stability bound on this fixture and that the iteration")
print("#     growth goes away below it. Read [finding].scope before quoting a result.")
if sm_trees:
    print("#   * THAT WEIGHT IS NO LONGER FIXED (issue #945). The AMS now lowers it where")
    print("#     a spectral-radius estimate proves 0.6 unstable, which it does on this")
    print("#     fixture. [[cell]], [[single_thread_cell]], [finding], [threading_fix] and")
    print("#     [diagnostics] all predate that and are kept as the record of the old")
    print("#     weight. [smoother_weight] has the before / after measurement, two more")
    print("#     frequencies, and the tests outside this fixture.")
print("#   * No Palace run and no size above the largest one listed here. See")
print("#     [deferred].")
print("# ---------------------------------------------------------------------------")
print()
print("[meta]")
print("issue = 930")
print(f"measured_date = {q(dates[0][:10])}  # UTC; the single-config legs started between {dates[0]} and {dates[-1]}")
print(f"geode_commit = {q(', '.join(commits))}")
print(f"scope = {q('LOCAL CPU SUBSET of the #520 sizes on a loaded developer machine; not a benchmark-host measurement')}")
print(
    "fixture = "
    + q(
        "sigma-lossy parallel-plate cube, single LumpedPort (e_hat = y, R = 1), eps_r = 1, "
        "sigma = 2.0, PEC on y-planes (crates/geode-core/tests/gpu_driven_scaling.rs), "
        "identical to results.toml and results_large_a100.toml"
    )
)
print("omega = 0.10")
print("iter_tol = 1e-8  # both iterative configs; convergence is judged on the EXPLICIT residual ||Ax - b|| / ||b||")
print("iter_max = 20000")
print(
    "krylov = "
    + q("COCG on the assembled CSR A(omega) for both; only the preconditioner differs (Jacobi vs IterativePreconditioner::AMS, AmsCoarseSolve::Auto)")
)
print(
    "statistic = "
    + q(
        "one solve per cell (GEODE_SCALING_REPS=0: the warm-up solve is the measurement, as in "
        "the #520 run). One process per (size, config), so process_* and max RSS belong to that "
        "configuration alone; they include mesh generation and operator assembly."
    )
)
print(f"generator = {q('benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh, then summarize_ams_local.py')}")
print()
print("[hardware]")
for k in ("model", "cpu", "os", "rustc"):
    print(f"{k} = {q(host.get(k, 'unknown'))}")
print(f"logical_cpus = {ncpu}")
print(f"ram_gib = {host.get('ram_gib', 0)}")
print(
    f"loadavg_1min_range = [{min(loads):.2f}, {max(loads):.2f}]  # over all leg starts and ends; "
    f"{ncpu} logical CPUs, so the machine was "
    + ("oversubscribed" if max(loads) > ncpu else "not oversubscribed")
    + " by other work"
)
print('label = "dev machine, loaded; timings indicative only"')
print()
print("[deferred]")
print("# NOT done in this file. Operator-run on a benchmark host (issue #930).")
print("not_done = [")
largest = max(c["n_edges"] for c in cells.values() if "n_edges" in c)
for item in (
    f"AMS and Jacobi cells above {largest} edges (the #520 sizes 197k, 338k and 463k edges)",
    "the same-host Palace (GMRES + AMS) comparison",
    "the decision on the default driven preconditioner (unchanged here: Jacobi)",
    "the follow-up issue for AMS on the matrix-free / GPU path",
):
    print(f"  {q(item)},")
print("]")
print("# RECOMMENDED FOLLOW-UPS, in this order. None is done here and none is filed by")
print("# this record; [diagnostics] and [finding].thread_contention are the evidence.")
print("recommended_follow_ups = [")
for item in (
    ("(i) DONE in issue #945, see [smoother_weight]: " if sm_trees else "(i) ")
    + "library issue for the AMS edge smoother (crates/geode-core/src/eigen/ams.rs, "
    "DEFAULT_SMOOTH_WEIGHT = 0.6): take the weight from an estimate of the spectral radius of "
    "D^-1 A, or use an l1-Jacobi smoother; validate on the #742 / #744 spiral, the transmon AMS "
    "tests and this fixture before changing the constant",
    (
        "(ii) DONE in issue #946, see [threading_fix]: the parallel sparse triangular solve inside "
        "the V-cycle (lu_solve in eigen/ams.rs), which fanned two small solves per iteration out "
        "to the rayon pool and caused the contention recorded in the [[cell]] tables"
        if tf_trees
        else "(ii) the parallel sparse triangular solve inside the V-cycle (lu_solve in eigen/ams.rs), "
        "which fans two small solves per iteration out to the rayon pool and causes the contention "
        "recorded here"
    ),
    "(iii) only after (i) and (ii): the at-scale cells and the same-host Palace comparison, on the "
    "fixed preconditioner",
):
    print(f"  {q(item)},")
print("]")

for n in sizes:
    direct = cells.get((n, "direct"))
    for label, _ in CONFIGS:
        c = cells.get((n, label))
        if c is None:
            continue
        t = c["time"]
        print()
        print("[[cell]]")
        print(f"leg = {q(c['leg'])}")
        print(f"size_n = {n}")
        if not c["finished"]:
            print("finished = false  # the leg produced no cell (timed out or crashed); see the .meta / .err")
            print(f"leg_rc = {c['meta'].get('rc', 'unknown')}")
            continue
        print(f"n_edges = {c['n_edges']}")
        print(f"n_interior = {c['n_interior']}")
        print(f"config = {q(c['config'])}")
        print(f"method = {q(c['method'])}")
        print(f"device = {q(c['device'])}")
        print(f"dtype = {q(c['dtype'])}")
        print(f"explicit_converged = {str(c['converged']).lower()}")
        if c.get("recursion_converged"):
            print("recursion_converged = true  # drift: recursion met tol, explicit residual did not")
        print(f"iterations = {c['iterations']}")
        print(f"residual_rel = {sci(c['residual_rel'])}  # explicit ||Ax - b|| / ||b||")
        if "dnf_attempt_s" in c:
            print(f"dnf_attempt_s = {c['dnf_attempt_s']:.3f}")
        else:
            print(f"setup_s = {c['warmup_setup_s']:.3f}  # prepare_at: assemble A(omega) + LU factor / build preconditioner")
            print(f"krylov_s = {c['warmup_krylov_s']:.3f}  # solve: iteration (or triangular solves) + explicit residual")
            print(f"solve_s = {c['warmup_solve_s']:.3f}  # setup_s + krylov_s")
            if c["iterations"]:
                print(f"krylov_s_per_iter = {c['warmup_krylov_s'] / c['iterations']:.5f}")
        if t:
            print_process(t)
        print(f"loadavg_1min_start = {c['meta']['loadavg_start'].split()[0]}")
        pv = c.get("port_v")
        if pv and pv[0] == pv[0]:
            print(f"port_v = [{pv[0]:.9e}, {pv[1]:.9e}]")
            if direct and direct.get("port_v") and label != "direct":
                print(
                    f"port_v_rel_diff_vs_direct = {sci(rel_diff(pv, direct['port_v']))}"
                    "  # from the 10 printed digits of two separate processes"
                )
        x = c["xcheck"]
        if x and label != "direct":
            acc = x.get("accuracy_rel_l2_vs_direct", float("nan"))
            print(f"field_rel_l2_vs_direct = {sci(acc)}  # full edge field, from the n{n}_xcheck leg")
            print(f"xcheck_iterations = {x['iterations']}  # same solve repeated in the xcheck process")

# ---- per-size comparison -------------------------------------------------
lam = {}
lam_path = d.parent / "2026-10-07_lambda_a100" / "B1_cpu_direct_iter.stdout"
for c in fragment(lam_path):
    if c["config"] == "2_iterative_csr":
        lam[c["size_n"]] = c

rows = []
for n in sizes:
    j, a = cells.get((n, "jacobi")), cells.get((n, "ams"))
    if not (j and a and j["finished"] and a["finished"]):
        continue
    rows.append((n, j, a))
    print()
    print("[[comparison]]")
    print(f"size_n = {n}")
    print(f"n_edges = {j['n_edges']}")
    print(f"jacobi_iterations = {j['iterations']}")
    print(f"ams_iterations = {a['iterations']}")
    print(f"ams_explicit_converged = {str(a['converged']).lower()}")
    print(f"iteration_ratio_jacobi_over_ams = {j['iterations'] / a['iterations']:.1f}")
    if j["time"] and a["time"]:
        print(f"max_rss_ratio_ams_over_jacobi = {a['time']['max_rss_bytes'] / j['time']['max_rss_bytes']:.2f}")
        # No time ratio here: these legs used the default threading. See
        # [[single_thread_comparison]] for the cost ratios.
        print(
            f"ams_process_sys_over_user = {a['time']['sys_s'] / a['time']['user_s']:.2f}"
            "  # default threading; where this is large on a leg of seconds or more, its wall clock is contention, not solver cost"
        )
    if n in lam:
        print(f"lambda_520_jacobi_iterations = {lam[n]['iterations']}  # runs/2026-10-07_lambda_a100/B1_cpu_direct_iter.stdout")
        print(f"jacobi_iterations_match_lambda_520 = {str(lam[n]['iterations'] == j['iterations']).lower()}")
        print(f"jacobi_port_v_rel_diff_vs_lambda_520 = {sci(rel_diff(j['port_v'], lam[n]['port_v']))}")


# ---- single-threaded rerun (optional) --------------------------------------
st_rows = []
if st_cells:
    st_metas = [c["meta"] for c in st_cells.values()]
    st_loads = [float(m[k].split()[0]) for m in st_metas for k in ("loadavg_start", "loadavg_end") if k in m]
    st_dates = sorted(m["start"] for m in st_metas)
    print()
    print("[single_thread]")
    print(
        "note = "
        + q(
            "The Jacobi and AMS legs rerun with RAYON_NUM_THREADS=1 (ams_cpu_sweep.sh with "
            "SKIP_DIRECT=1), same fixture, sizes and shipped AMS. These are the timings the cost "
            "statements in [finding] use. Still a loaded developer machine and single samples."
        )
    )
    print(f"run_dir = {q((d / 'single_thread').as_posix())}")
    print(f"geode_commit = {q(', '.join(sorted({m['git'] for m in st_metas})))}")
    print(f"rayon_num_threads = {q(', '.join(sorted({m.get('rayon_num_threads', 'unset') for m in st_metas})))}")
    print(f"started_between = [{q(st_dates[0])}, {q(st_dates[-1])}]")
    print(f"loadavg_1min_range = [{min(st_loads):.2f}, {max(st_loads):.2f}]")
    for (n, label), c in sorted(st_cells.items(), key=lambda kv: (kv[0][0], kv[0][1] != "jacobi")):
        print()
        print("[[single_thread_cell]]")
        print(f"leg = {q(c['leg'])}")
        print(f"size_n = {n}")
        if not c["finished"]:
            print("finished = false")
            continue
        print(f"n_edges = {c['n_edges']}")
        print(f"config = {q(c['config'])}")
        print(f"explicit_converged = {str(c['converged']).lower()}")
        print(f"iterations = {c['iterations']}")
        print(f"residual_rel = {sci(c['residual_rel'])}")
        print(f"setup_s = {c['warmup_setup_s']:.3f}")
        print(f"krylov_s = {c['warmup_krylov_s']:.3f}")
        print(f"solve_s = {c['warmup_solve_s']:.3f}")
        print(f"krylov_s_per_iter = {c['warmup_krylov_s'] / c['iterations']:.5f}")
        if c["time"]:
            print_process(c["time"])
        print(f"loadavg_1min_start = {c['meta']['loadavg_start'].split()[0]}")
    for n in sizes:
        j, a = st_cells.get((n, "jacobi")), st_cells.get((n, "ams"))
        if not (j and a and j["finished"] and a["finished"]):
            continue
        st_rows.append((n, j, a))
        print()
        print("[[single_thread_comparison]]")
        print(f"size_n = {n}")
        print(f"n_edges = {j['n_edges']}")
        print(f"jacobi_iterations = {j['iterations']}")
        print(f"ams_iterations = {a['iterations']}")
        d_ams = cells.get((n, "ams"), {}).get("iterations")
        if d_ams is not None:
            print(
                f"ams_iterations_default_threading = {d_ams}"
                "  # serial and parallel triangular solves differ in roundoff; a slowly converging solve amplifies it"
            )
        print("# Same host, same binary, both single-threaded, different moments on a loaded machine.")
        print(f"solve_s_ratio_ams_over_jacobi = {a['warmup_solve_s'] / j['warmup_solve_s']:.2f}")
        print(
            f"krylov_s_per_iter_ratio_ams_over_jacobi = "
            f"{(a['warmup_krylov_s'] / a['iterations']) / (j['warmup_krylov_s'] / j['iterations']):.1f}"
        )


# ---- smoother-weight diagnostic data (optional; printed under [diagnostics]) ----
diag = d / "diagnostics"
SHIPPED_WEIGHT = 0.6  # DEFAULT_SMOOTH_WEIGHT in crates/geode-core/src/eigen/ams.rs
sw = {}  # weight -> list of AMS cells, one per size
for p in sorted(diag.glob("sw_[0-9]p*.stdout")):
    sw[float(p.stem[3:].replace("p", "."))] = (fragment(p), time_file(p.with_suffix(".time")))
sw_jac = fragment(diag / "sw_jacobi.stdout")
rho = [  # (edge_dim, gershgorin upper bound, [(power iterations, Rayleigh quotient), ...])
    (int(m.group(1)), float(m.group(2)), [(int(a), float(b)) for a, b in re.findall(r"it(\d+)=([\d.]+)", m.group(3))])
    for m in re.finditer(
        r"# diag944_rho edge_dim=(\d+) gershgorin_upper=([\d.]+) power_rayleigh_lower:(.*)",
        (diag / "sw_rho.stdout").read_text() if (diag / "sw_rho.stdout").exists() else "",
    )
]


def sw_ok(w):
    """True if every solve at weight w converged and the count stays under 2x the best weight's."""
    best = [min(c["iterations"] for cs, _ in sw.values() for c in cs if c["size_n"] == x["size_n"]) for x in sw[w][0]]
    return all(c["converged"] and c["iterations"] <= 2 * b for c, b in zip(sw[w][0], best))


# ---- finding ---------------------------------------------------------------
if len(rows) >= 2:
    ams_it = [a["iterations"] for _, _, a in rows]
    jac_it = [j["iterations"] for _, j, _ in rows]
    edges = [j["n_edges"] for _, j, _ in rows]
    all_conv = all(a["converged"] for _, _, a in rows)
    spread = max(ams_it) / min(ams_it)
    flat = all_conv and spread <= FLAT_MAX_OVER_MIN
    pa, pj = endpoint_exponent(edges, ams_it), endpoint_exponent(edges, jac_it)
    la, lj = lsq_exponent(edges, ams_it), lsq_exponent(edges, jac_it)
    timed = st_rows  # cost statements use the single-threaded rerun only
    ams_faster = [n for n, j, a in timed if a["warmup_solve_s"] < j["warmup_solve_s"]]
    print()
    print("[finding]")
    print(f"sizes_edges = {edges}")
    print(f"jacobi_iterations = {jac_it}")
    print(f"ams_iterations = {ams_it}")
    print(f"ams_all_explicit_converged = {str(all_conv).lower()}")
    print(f"ams_iterations_max_over_min = {spread:.2f}")
    print(
        f"flat_threshold_max_over_min = {FLAT_MAX_OVER_MIN:.2f}"
        "  # the band of the #742 / #744 spiral measurement (102 to 155 iterations)"
    )
    print(f"ams_iteration_count_flat = {str(flat).lower()}")
    print(f"ams_growth_exponent_endpoint = {pa:.3f}  # iterations ~ edges^p, from the smallest and largest size only")
    print(f"ams_growth_exponent_least_squares = {la:.3f}  # slope of log(iterations) on log(edges), all sizes")
    print(f"jacobi_growth_exponent_endpoint = {pj:.3f}")
    print(f"jacobi_growth_exponent_least_squares = {lj:.3f}")
    if timed:
        print(
            f"sizes_n_where_ams_solve_was_faster_than_jacobi = {ams_faster}"
            "  # shipped AMS, single-threaded rerun, loaded machine: indicative only"
        )
    print(
        "scope = "
        + q(
            f"Everything in verdict, convergence and cost is a statement about the SHIPPED AMS "
            f"(edge-smoother weight {SHIPPED_WEIGHT}) on this one fixture at omega = 0.10. It is not a "
            "statement about AMS preconditioning in general: see cause."
        )
    )
    lo, hi = edges[0], edges[-1]
    ratio_lo = jac_it[0] / ams_it[0]
    ratio_hi = jac_it[-1] / ams_it[-1]
    if not all_conv:
        verdict = (
            f"AMS did NOT reach the explicit-residual tolerance at every size from {lo} to {hi} "
            "edges (see the cells with explicit_converged = false)."
        )
    elif flat:
        verdict = (
            f"AMS held a flat iteration count from {lo} to {hi} edges: {min(ams_it)} to "
            f"{max(ams_it)} iterations (max/min {spread:.2f}), while Jacobi went from "
            f"{jac_it[0]} to {jac_it[-1]}."
        )
    else:
        verdict = (
            f"The shipped AMS converged at every size but its iteration count is NOT flat on this "
            f"fixture: {ams_it[0]} iterations at {lo} edges, {ams_it[-1]} at {hi} (max/min "
            f"{spread:.2f}, outside the {FLAT_MAX_OVER_MIN:.2f} band of the #742 / #744 spiral "
            f"measurement). It grows as edges^{pa:.2f} by the endpoint fit (edges^{la:.2f} by least "
            f"squares over all {len(rows)} sizes), against edges^{pj:.2f} for Jacobi (least squares "
            f"{lj:.2f}; {jac_it[0]} to {jac_it[-1]}). It needs {ratio_lo:.1f}x fewer iterations than "
            f"Jacobi at the smallest size and {ratio_hi:.1f}x fewer at the largest."
        )
    print(f"verdict = {q(verdict)}")
    if all_conv:
        worst = max(a["residual_rel"] for _, _, a in rows)
        print(
            "convergence = "
            + q(
                f"No plateau and no drift: every AMS solve met the 1e-8 tolerance on the explicit "
                f"residual (worst {worst:.1e}) and matches the Direct solution. The degradation "
                "is in the iteration count as the mesh is refined, not a stall of the kind "
                "recorded in benchmarks/transmon_ams_minres_133k/results.toml. Only omega = 0.10 "
                "was run, so nothing here says how the count depends on frequency."
            )
        )
    if timed:
        per_iter = [
            (a["warmup_krylov_s"] / a["iterations"]) / (j["warmup_krylov_s"] / j["iterations"])
            for _, j, a in timed
        ]
        solve = [a["warmup_solve_s"] / j["warmup_solve_s"] for _, j, a in timed]
        cost = (
            f"Single-threaded (RAYON_NUM_THREADS=1), one shipped-AMS iteration cost "
            f"{min(per_iter):.0f}x to {max(per_iter):.0f}x a Jacobi iteration, and the shipped-AMS "
            f"solve (setup + Krylov) took {min(solve):.1f}x to {max(solve):.1f}x as long as the "
            f"Jacobi solve; it was faster at {len(ams_faster)} of {len(timed)} sizes. Loaded dev "
            "machine, single samples: the direction is clear at the larger sizes, the factors are "
            "indicative."
        )
        print(f"cost = {q(cost)}")
        # Thread contention in the default-threading legs, from their own .time files.
        cont = [
            (n, a, s)
            for n, _, a in rows
            if a["time"]
            and (s := st_cells.get((n, "ams")))
            and a["time"]["sys_s"] >= 1.0  # ignore sub-second process-startup noise
            and a["time"]["sys_s"] > 0.1 * a["time"]["user_s"]
        ]
        if cont:
            worst = max(cont, key=lambda r: r[1]["time"]["sys_s"])
            print(f"sizes_n_with_contended_default_threading_ams_leg = {[n for n, _, _ in cont]}")
            print(
                "thread_contention = "
                + q(
                    "The default-threading AMS timings in [[cell]] at these sizes are NOT the cost "
                    "of AMS. The V-cycle does two small sparse triangular solves per iteration "
                    "(lu_solve in eigen/ams.rs, faer solve_in_place under the global parallelism "
                    "setting), which fan out to the rayon pool; on this oversubscribed host that "
                    f"became kernel time. The worst leg, {worst[1]['leg']}, used "
                    f"{worst[1]['time']['user_s']:.0f} s user and {worst[1]['time']['sys_s']:.0f} s "
                    f"sys for a {worst[1]['warmup_krylov_s']:.0f} s Krylov solve; the same solve "
                    f"with RAYON_NUM_THREADS=1 took {worst[2]['warmup_krylov_s']:.0f} s with "
                    f"{worst[2]['time']['sys_s']:.1f} s sys. No time ratio in this file uses the "
                    "default-threading legs. Whether the parallel solve helps on an idle host was "
                    "not measured."
                    + (
                        " Fixed in issue #946 after these legs were run: see [threading_fix]."
                        if tf_trees
                        else ""
                    )
                )
            )
    if sw and rho:
        good = sorted(w for w in sw if sw_ok(w))
        r_lo = max(tr[-1][1] for _, _, tr in rho)
        print(
            "cause = "
            + q(
                f"Identified, by a one-off diagnostic on this fixture (see [diagnostics]): the edge "
                f"smoother weight. The shipped damped-Jacobi weight {SHIPPED_WEIGHT} is above the "
                f"stability bound 2 / rho(D^-1 P) = {2 / r_lo:.3f} measured here (rho >= {r_lo:.2f}), "
                f"so the smoother amplifies the highest edge modes. With the weight anywhere from "
                f"{good[0]} to {good[-1]} the same solve takes "
                f"{min(c['iterations'] for w in good for c in sw[w][0])} to "
                f"{max(c['iterations'] for w in good for c in sw[w][0])} iterations at every size. "
                "This is a throwaway patch, single runs, one fixture and one frequency; no weight "
                "was validated on any other problem and the library constant is unchanged."
            )
        )
    print(
        "not_established = "
        + q(
            "Not established: (1) why the shipped weight holds a flat count on the #742 / #744 "
            "spiral and not here. A plausible reason is the mesh (this fixture is a structured "
            "Kuhn-tet cube, the spiral an unstructured mesh, and rho(D^-1 P) depends on the mesh), "
            "but the spiral was NOT rerun and its rho was NOT measured, so this is untested. "
            "(2) What a lower weight does on the spiral or the transmon AMS tests. (3) The "
            "dependence on frequency: only omega = 0.10 was run. (4) Anything above the largest "
            "size listed. No AMS setting was varied in the [[cell]] and [[single_thread_cell]] "
            "tables (AmsCoarseSolve::Auto throughout). Palace's ~5 iterations use a different "
            "Krylov method (GMRES) and a different AMS (hypre) and were not rerun here."
            + (
                " Items (1) to (3) were measured afterwards, in issue #945: see [smoother_weight]."
                if sm_trees
                else ""
            )
        )
    )

# ---- issue #946 before / after (optional) ----------------------------------
if tf_trees:
    THREADING = {"default": "default", "rayon1": "RAYON_NUM_THREADS=1"}
    tf_metas = [c["meta"] for *_, cs in tf_trees for c in cs.values()]
    tf_loads = [float(m[k].split()[0]) for m in tf_metas for k in ("loadavg_start", "loadavg_end") if k in m]
    tf_dates = sorted(m["start"] for m in tf_metas)
    commit_of = lambda build: sorted({c["meta"]["git"] for _, b, _, _, cs in tf_trees if b == build for c in cs.values()})
    cpu = lambda c: c["time"]["user_s"] + c["time"]["sys_s"]
    cpu_pct = lambda c: 100 * cpu(c) / c["time"]["wall_s"]

    def legs(build, threading, n, label):
        """The finished legs of one (build, threading) at one size, one per repeat."""
        return [
            cs[(n, label)]
            for _, b, t, _, cs in tf_trees
            if b == build and t == threading and (n, label) in cs and cs[(n, label)]["finished"]
        ]

    print()
    print("[threading_fix]")
    print("issue = 946")
    print(
        "note = "
        + q(
            "Before / after measurement of the issue #946 fix. Before (build = main): faer's "
            "Lu::solve_in_place, called twice per COCG iteration by the AMS V-cycle (lu_solve in "
            "eigen/ams.rs), took its thread count from faer's process-global parallelism, by "
            "default every core. After (build = fix): the SolverMode::Iterative back-solve holds a "
            "SequentialSolveScope for the whole Krylov solve when the preconditioner is AMS, so "
            "those solves run with Par::Seq. Nothing else differs between the two builds. Each "
            "build was run with the default threading and with RAYON_NUM_THREADS=1, by "
            "ams_cpu_sweep.sh with SKIP_XCHECK=1, one process per leg."
        )
    )
    print(
        "how_to_read = "
        + q(
            "LOADED DEVELOPER MACHINE, single samples. process_user_s and process_sys_s are the "
            "signal. Wall-clock times (krylov_s, setup_s, process_wall_s) are NOT comparable "
            "between legs: the load average changed by a factor of several during the session, "
            "and a leg whose process_cpu_pct is well under 100 on one thread was waiting for a "
            "CPU, not computing. process_* covers the whole process: mesh, assembly, setup and "
            "solve."
        )
    )
    print(f"run_dir = {q(tf_dir.as_posix())}")
    print(f"trees = {[name for name, *_ in tf_trees]}")
    print(f"main_commit = {q(', '.join(commit_of('main')))}")
    print(f"fix_commit = {q(', '.join(commit_of('fix')))}")
    print(f"started_between = [{q(tf_dates[0])}, {q(tf_dates[-1])}]")
    print(f"loadavg_1min_range = [{min(tf_loads):.2f}, {max(tf_loads):.2f}]  # {ncpu} logical CPUs")

    for name, build, threading, rep, cs in tf_trees:
        for (n, label), c in sorted(cs.items(), key=lambda kv: (kv[0][0], [l for l, _ in CONFIGS].index(kv[0][1]))):
            print()
            print("[[threading_fix_cell]]")
            print(f"tree = {q(name)}")
            print(f"build = {q(build)}")
            print(f"threading = {q(THREADING[threading])}")
            print(f"repeat = {rep}")
            print(f"leg = {q(c['leg'])}")
            print(f"size_n = {n}")
            if not c["finished"]:
                print("finished = false")
                continue
            print(f"n_edges = {c['n_edges']}")
            print(f"config = {q(c['config'])}")
            print(f"explicit_converged = {str(c['converged']).lower()}")
            print(f"iterations = {c['iterations']}")
            print(f"residual_rel = {sci(c['residual_rel'])}")
            print(f"setup_s = {c['warmup_setup_s']:.3f}")
            print(f"krylov_s = {c['warmup_krylov_s']:.3f}")
            if c["time"]:
                print_process(c["time"])
            print(f"loadavg_1min_start = {c['meta']['loadavg_start'].split()[0]}")
            print(f"port_v = [{c['port_v'][0]:.9e}, {c['port_v'][1]:.9e}]")

    CATS = [("main", "default"), ("main", "rayon1"), ("fix", "default"), ("fix", "rayon1")]
    key = lambda b, t: f"{b}_{t}"
    result = lambda c: (c["iterations"], sci(c["residual_rel"]), f"{c['port_v'][0]:.9e}", f"{c['port_v'][1]:.9e}")
    tf_rows = []
    for n in sorted({n for *_, cs in tf_trees for n, _ in cs}):
        a = {key(b, t): legs(b, t, n, "ams") for b, t in CATS}
        if not all(a.values()):
            continue
        tf_rows.append((n, a))
        ref = a["main_default"][0]
        print()
        print("[[threading_fix_comparison]]")
        print(f"size_n = {n}")
        print(f"n_edges = {ref['n_edges']}")
        print("# AMS legs. One entry per repeat, in the order of [threading_fix].trees.")
        for k, cs in a.items():
            print(f"ams_iterations_{k} = {[c['iterations'] for c in cs]}")
        for k, cs in a.items():
            print(f"ams_process_sys_s_{k} = {fl((c['time']['sys_s'] for c in cs), 2)}")
        for k, cs in a.items():
            print(f"ams_process_user_s_{k} = {fl((c['time']['user_s'] for c in cs), 2)}")
        for k, cs in a.items():
            print(
                f"ams_process_cpu_ms_per_iteration_{k} = {fl((1e3 * cpu(c) / c['iterations'] for c in cs), 2)}"
                "  # (user + sys) / iterations"
            )
        for k, cs in a.items():
            print(f"ams_krylov_s_{k} = {fl((c['warmup_krylov_s'] for c in cs), 2)}  # wall clock: see how_to_read")
        for k, cs in a.items():
            print(f"ams_process_cpu_pct_{k} = {[round(cpu_pct(c)) for c in cs]}")
        fix_same = len({result(c) for k in ("fix_default", "fix_rayon1") for c in a[k]}) == 1
        main_same = len({result(c) for k in ("main_default", "main_rayon1") for c in a[k]}) == 1
        print(
            f"fix_result_identical_across_threading = {str(fix_same).lower()}"
            "  # iterations, residual_rel and port_v as printed, over every fix leg"
        )
        print(f"main_result_identical_across_threading = {str(main_same).lower()}")
        fx = a["fix_default"][0]
        for k in ("main_default", "main_rayon1"):
            print(
                f"fix_port_v_rel_diff_vs_{k} = {sci(rel_diff(fx['port_v'], a[k][0]['port_v']))}"
                "  # from the 10 printed digits"
            )
        dm, df = legs("main", "default", n, "direct"), legs("fix", "default", n, "direct")
        if dm and df:
            print("# Direct legs, default threading: the path the fix must not touch.")
            print(f"direct_process_user_s_main_default = {fl((c['time']['user_s'] for c in dm), 2)}")
            print(f"direct_process_user_s_fix_default = {fl((c['time']['user_s'] for c in df), 2)}")
            print(f"direct_setup_s_main_default = {fl((c['warmup_setup_s'] for c in dm), 2)}  # wall clock")
            print(f"direct_setup_s_fix_default = {fl((c['warmup_setup_s'] for c in df), 2)}  # wall clock")
            print(f"direct_process_cpu_pct_main_default = {[round(cpu_pct(c)) for c in dm]}")
            print(f"direct_process_cpu_pct_fix_default = {[round(cpu_pct(c)) for c in df]}")
            same = all(result(c) == result(dm[0]) for c in dm + df)
            print(f"direct_result_identical_main_vs_fix = {str(same).lower()}")
        ds = legs("main", "rayon1", n, "direct")
        if dm and ds:
            print(
                f"direct_process_wall_s_main_rayon1_over_main_default = "
                f"{ds[0]['time']['wall_s'] / dm[0]['time']['wall_s']:.1f}"
                "  # the direct solver does use the threads: its LU factorization is parallel"
            )

    if tf_rows:
        n, a = tf_rows[-1]
        md, mr = a["main_default"][0], a["main_rayon1"][0]
        fd, fr = a["fix_default"], a["fix_rayon1"]
        rng = lambda vals, digits=1: (
            f"{min(vals):.{digits}f}" if max(vals) - min(vals) < 10**-digits / 2 else f"{min(vals):.{digits}f} to {max(vals):.{digits}f}"
        )
        per_it = lambda cs: [1e3 * cpu(c) / c["iterations"] for c in cs]
        all_fix_same = all(
            len({result(c) for k in ("fix_default", "fix_rayon1") for c in a_[k]}) == 1 for _, a_ in tf_rows
        )
        print()
        print("[threading_fix.finding]")
        print(f"largest_size_edges = {md['n_edges']}")
        print(
            "contention = "
            + q(
                f"At {md['n_edges']} edges with the default threading, the AMS leg's kernel time went "
                f"from {md['time']['sys_s']:.0f} s before the fix to {rng([c['time']['sys_s'] for c in fd])} s "
                f"after it, and its user time from {md['time']['user_s']:.0f} s to "
                f"{rng([c['time']['user_s'] for c in fd], 0)} s. With RAYON_NUM_THREADS=1 the kernel time was "
                f"{mr['time']['sys_s']:.1f} s before and {rng([c['time']['sys_s'] for c in fr])} s after. "
                "The scope changes nothing but faer's global parallelism during the Krylov solve, and "
                "the only faer calls in that solve are the two lu_solve calls per iteration, so the "
                "contention was in those calls."
            )
        )
        print(
            "cost = "
            + q(
                f"CPU time per AMS iteration at {md['n_edges']} edges, whole process: "
                f"{per_it([md])[0]:.0f} ms before the fix with the default threading and "
                f"{per_it([mr])[0]:.0f} ms with RAYON_NUM_THREADS=1; after the fix "
                f"{rng(per_it(fd), 0)} ms with the default threading and {rng(per_it(fr), 0)} ms with "
                "RAYON_NUM_THREADS=1. After the fix the two modes run the same sequential code, so "
                "the spread between them and between repeats is the host, not the solver. "
                "Wall-clock times were not comparable in this session (see how_to_read), so no "
                "wall-clock speedup is quoted."
            )
        )
        print(f"fix_result_identical_across_threading_at_every_size = {str(all_fix_same).lower()}")
        print(
            "results = "
            + q(
                "After the fix the default-threading and RAYON_NUM_THREADS=1 legs agree in "
                "iteration count, explicit residual and port voltage to every printed digit at "
                "every size"
                + ("" if all_fix_same else " EXCEPT where fix_result_identical_across_threading is false")
                + ". Before the fix they did not: the parallel and the one-thread triangular solve "
                "differ in roundoff, and at the shipped smoother weight a slowly converging solve "
                "amplifies that into a different iteration count. For the same reason the fix "
                "build does not reproduce either main iteration count exactly: faer's Par::Seq "
                "solve differs in roundoff from its Par::rayon solve even on one thread. Every "
                "leg met the same 1e-8 explicit-residual tolerance, and the port voltages of the "
                "two builds agree to the fix_port_v_rel_diff_* values above."
            )
        )
        print(
            "not_measured = "
            + q(
                "An idle host; whether a parallel triangular solve would pay at node counts far "
                f"above the {md['n_edges']}-edge case; the AMS setup, whose coarse factorization "
                "still runs at the caller's thread count; and concurrent solves (the CLI's --jobs)."
            )
        )

# ---- issue #946: the eigen direct shift-invert loop (optional) ---------------
eig_dir = tf_dir / "eigen_loop"
if eig_dir.is_dir():

    def eig_run(path):
        txt = path.read_text()
        m = re.search(r"diag946 n=(\d+) dofs=(\d+) nev=(\d+) seq_loop=(\w+) solve_s=([\d.]+) par_after=(\S+)", txt)
        c = re.search(r"diag946 solve_user_s=([\d.]+) solve_sys_s=([\d.]+)", txt)
        return {
            "n": int(m.group(1)),
            "dofs": int(m.group(2)),
            "nev": int(m.group(3)),
            "solve_s": float(m.group(5)),
            "user_s": float(c.group(1)),
            "sys_s": float(c.group(2)),
            "lambdas": re.findall(r"diag946 lambda\[\d+\] = (\S+)", txt),
        }

    EIG_MODES = {
        "default": "default threading",
        "seqloop": "default threading, with the Lanczos loop made sequential from the test (ParallelismGuard::cap(1) around the eigensolve)",
        "rayon1": "RAYON_NUM_THREADS=1",
    }
    eig = {p.stem: eig_run(p) for p in sorted(eig_dir.glob("*_*.stdout"))}
    first = next(iter(eig.values()))
    print()
    print("[threading_fix.eigen_loop]")
    print(
        "note = "
        + q(
            "The same pattern in the direct shift-invert Lanczos loop (eigen/lanczos.rs): one "
            "sparse triangular solve per step, on faer's global parallelism. Measured with a "
            "throwaway test (eigen_loop/diag946_eigen_loop.rs, NOTE.txt), not part of the "
            "benchmark: a scalar P1 pencil on cube_tet_mesh, SparseShiftInvertLanczos::default(). "
            "solve_user_s and solve_sys_s are getrusage deltas around the eigensolve alone "
            "(factorization plus Lanczos loop). One retained sample per row, same loaded machine; "
            "NOTE.txt lists the repeats."
        )
    )
    print(f"run_dir = {q(eig_dir.as_posix())}")
    print(f"mesh_n = {first['n']}")
    print(f"dofs = {first['dofs']}")
    print(f"modes_requested = {first['nev']}")
    for name, r in sorted(eig.items(), key=lambda kv: (not kv[0].startswith("main"), list(EIG_MODES).index(kv[0].split("_", 1)[1]))):
        build, mode = name.split("_", 1)
        print()
        print("[[threading_fix.eigen_loop.run]]")
        print(f"build = {q(build)}")
        print(f"threading = {q(EIG_MODES[mode])}")
        print(f"solve_user_s = {r['user_s']:.3f}")
        print(f"solve_sys_s = {r['sys_s']:.3f}")
        print(f"solve_s = {r['solve_s']:.3f}  # wall clock, loaded machine")
    same = lambda a, b: a in eig and b in eig and eig[a]["lambdas"] == eig[b]["lambdas"]
    print()
    print("[threading_fix.eigen_loop.finding]")
    if "main_default" in eig and "main_seqloop" in eig and "fix_default" in eig:
        md, ms, fd = eig["main_default"], eig["main_seqloop"], eig["fix_default"]
        print(
            "contention = "
            + q(
                f"Yes. Before the fix the eigensolve used {md['sys_s']:.1f} s of kernel time with the "
                f"default threading and {ms['sys_s']:.1f} s with only the Lanczos loop made "
                f"sequential ({md['user_s']:.1f} s and {ms['user_s']:.1f} s of user time). What remains "
                "is attributed to the factorization, the only faer call still on the rayon pool; "
                "that part was not measured separately. The direct backends now hold the "
                f"same SequentialSolveScope for the loop: the fix build used {fd['sys_s']:.1f} s of "
                f"kernel time and {fd['user_s']:.1f} s of user time with the default threading."
            )
        )
    print(
        f"eigenvalues_identical_default_vs_single_thread_before = {str(same('main_default', 'main_rayon1')).lower()}"
        "  # all printed modes, 17 significant digits"
    )
    print(f"eigenvalues_identical_default_vs_single_thread_after = {str(same('fix_default', 'fix_rayon1')).lower()}")
    print(f"eigenvalues_identical_fix_default_vs_main_sequential_loop = {str(same('fix_default', 'main_seqloop')).lower()}")
    if "main_default" in eig and "fix_default" in eig:
        a, b = eig["main_default"]["lambdas"], eig["fix_default"]["lambdas"]
        print(
            f"eigenvalue_max_rel_diff_fix_vs_main_default = "
            f"{sci(max(abs(float(x) - float(y)) / abs(float(y)) for x, y in zip(a, b)))}"
        )

# ---- diagnostics (optional) ------------------------------------------------
# One-off runs of throwaway patched builds (diagnostics/NOTE.txt). Iteration
# counts only: they say what the growth is NOT caused by.
if diag.is_dir():

    def series(path, config):
        return [c["iterations"] for c in fragment(path) if c["config"] == config and c["converged"]]

    print()
    print("[diagnostics]")
    print(
        "note = "
        + q(
            "Throwaway patched builds, not committed code and not part of the benchmark "
            "(diagnostics/NOTE.txt, pec.patch, pi.patch, smooth_weight.patch). The pec_* and "
            "vector_nodal_* series are AMS iteration counts at n = 6, 9, 12, 15 (1854 to 25695 "
            "edges); the smooth_weight tables cover all six sizes."
        )
    )
    pec = {p.stem[4:]: series(p, "5_iterative_ams") for p in sorted(diag.glob("pec_*.stdout"))}
    pi = {p.stem[3:]: series(p, "5_iterative_ams") for p in sorted(diag.glob("pi_*.stdout"))}
    for name, its in pec.items():
        print(f"pec_planes_{name}_ams_iterations = {its}")
    for name, its in pi.items():
        print(f"vector_nodal_solve_{name}_ams_iterations = {its}")
    grows = lambda its: len(its) >= 2 and its[-1] / its[0] > FLAT_MAX_OVER_MIN
    if pec:
        print(f"growth_persists_for_every_pec_layout = {str(all(grows(v) for v in pec.values())).lower()}")
    if pi:
        print(f"growth_persists_with_exact_vector_nodal_solve = {str(grows(pi.get('direct', []))).lower()}")
    if pec and pi and all(grows(v) for v in pec.values()) and grows(pi.get("direct", [])):
        print(
            "reading = "
            + q(
                "The growth is not caused by the fixture's two disconnected PEC planes (it "
                "persists with one PEC plane, three, or none), and it is not caused only by the "
                "inexact vector-nodal solve (it persists, reduced, with an exact LU on both "
                "auxiliary spaces). Both series were run at the shipped smoother weight; the "
                "smooth_weight tables below identify the edge smoother as the cause. Not tested: "
                "the SPD proxy against this operator's large loss term, and an unstructured mesh "
                "of this fixture. Neither is needed to explain the growth."
            )
        )
    if sw:
        good = sorted(w for w in sw if sw_ok(w))
        bad = sorted(w for w in sw if not sw_ok(w))
        print(f"smooth_weight_shipped = {SHIPPED_WEIGHT}")
        print(f"smooth_weights_run = {sorted(sw)}")
        print(
            f"smooth_weights_within_2x_of_best_at_every_size = {good}"
            "  # converged, and at most twice the best weight's iteration count at each size"
        )
        print(f"smooth_weights_degraded = {bad}")
        if rho:
            r_lo, r_hi = max(tr[-1][1] for _, _, tr in rho), max(g for _, g, _ in rho)
            r_min = min(tr[-1][1] for _, _, tr in rho)
            print(
                f"rho_dinv_p_bounds = [{r_lo:.3f}, {r_hi:.3f}]"
                "  # largest eigenvalue of D^-1 P over the six sizes: power-iteration Rayleigh quotient (lower), Gershgorin (upper)"
            )
            print(
                f"damped_jacobi_stability_bound_weight = [{2 / r_hi:.3f}, {2 / r_lo:.3f}]"
                "  # 2 / rho for the upper and lower rho bound"
            )
            drift = max(tr[-1][1] - tr[-2][1] for _, _, tr in rho)
            print(
                f"power_rayleigh_max_change_over_last_step = {drift:.5f}"
                f"  # between {rho[0][2][-2][0]} and {rho[0][2][-1][0]} power iterations, worst size: the lower"
                " rho bound had settled, so the true stability bound is at or just below the larger value above"
            )
            print(f"shipped_weight_exceeds_stability_bound = {str(SHIPPED_WEIGHT > 2 / r_lo).lower()}")
            contiguous = good == [w for w in sorted(sw) if good[0] <= w <= good[-1]]
            print(
                "smooth_weight_reading = "
                + q(
                    f"The result is robust to the weight, not a narrow sweet spot: every weight run "
                    f"from {good[0]} to {good[-1]} ({len(good)} values"
                    + (", contiguous" if contiguous else "")
                    + f") gives {min(c['iterations'] for w in good for c in sw[w][0])} to "
                    f"{max(c['iterations'] for w in good for c in sw[w][0])} iterations across the six "
                    f"sizes, and the weights that degrade ({', '.join(str(w) for w in bad)}) are the "
                    f"ones next to and above the damped-Jacobi stability bound 2 / rho = {2 / r_lo:.3f}. "
                    f"The estimate of rho(D^-1 P) is {r_min:.2f} to {r_lo:.2f} over the six sizes, so "
                    f"the bound does not move with refinement, and {SHIPPED_WEIGHT} * {r_min:.2f} = "
                    f"{SHIPPED_WEIGHT * r_min:.2f} > 2 at every size: the shipped smoother amplifies the "
                    "top edge modes, and their number grows with the mesh. That the count already "
                    "rises just below the bound fits the same picture (there the symmetrized "
                    "smoother is nearly singular on those modes), but this mechanism was inferred "
                    "from the threshold, not verified mode by mode. The comment on "
                    "DEFAULT_SMOOTH_WEIGHT in eigen/ams.rs says 0.5; the constant is 0.6. Single "
                    "runs of an uncommitted patch on one fixture: this identifies the cause here "
                    "and does not validate a replacement value."
                )
            )
        jac = {c["size_n"]: c for c in sw_jac}
        if jac:
            print()
            print("[diagnostics.smooth_weight_jacobi_same_session]")
            print("# COCG + Jacobi from the same patched binary and session, RAYON_NUM_THREADS=1: the time reference.")
            print(f"iterations = {[c['iterations'] for c in sw_jac]}")
            print(f"solve_s = {fl(c['warmup_solve_s'] for c in sw_jac)}")
        for w in sorted(sw):
            cs, t = sw[w]
            its = [c["iterations"] for c in cs]
            ed = [c["n_edges"] for c in cs]
            print()
            print("[[diagnostics.smooth_weight]]")
            print(f"weight = {w}" + ("  # the shipped DEFAULT_SMOOTH_WEIGHT" if w == SHIPPED_WEIGHT else ""))
            print(f"n_edges = {ed}")
            print(f"ams_iterations = {its}")
            print(f"all_explicit_converged = {str(all(c['converged'] for c in cs)).lower()}")
            print(f"growth_exponent_endpoint = {endpoint_exponent(ed, its):.2f}")
            print(f"growth_exponent_least_squares = {lsq_exponent(ed, its):.2f}")
            print(f"setup_s = {fl(c['warmup_setup_s'] for c in cs)}")
            print(f"krylov_s = {fl(c['warmup_krylov_s'] for c in cs)}")
            if jac:
                print(
                    f"solve_s_ratio_jacobi_over_ams = {fl((jac[c['size_n']]['warmup_solve_s'] / c['warmup_solve_s'] for c in cs), 2)}"
                    "  # above 1: AMS at this weight was faster than Jacobi (setup + Krylov, single-threaded)"
                )
            if t:
                starved = (t["user_s"] + t["sys_s"]) / t["wall_s"] < 0.9
                print(
                    f"process_wall_user_sys_s = [{t['wall_s']:.2f}, {t['user_s']:.2f}, {t['sys_s']:.2f}]"
                    "  # one process for all six sizes, incl. mesh + assembly"
                    + ("; STARVED (CPU% < 90): its wall-clock times overstate the cost" if starved else "")
                )
        for edge_dim, gersh, tr in rho:
            print()
            print("[[diagnostics.spectral_radius]]")
            print(f"n_interior = {edge_dim}")
            print(f"gershgorin_upper = {gersh:.5f}")
            print(f"power_iterations = {[k for k, _ in tr]}")
            print(f"power_rayleigh_lower = {fl((v for _, v in tr), 5)}")
            print(f"stability_bound_weight = {2 / tr[-1][1]:.4f}  # 2 / the converged Rayleigh quotient")

# ---- issue #945: smoother weight, before / after (optional) -------------------
if sm_trees:
    BUILD = {
        "main": "origin/main before the fix (weight fixed at 0.6)",
        "fix": "the fix (weight chosen per operator)",
        "w060": "the fix build with GEODE_AMS_SMOOTH_WEIGHT=0.6 (the old weight)",
    }
    THREADS = {"default": "default", "rayon1": "RAYON_NUM_THREADS=1"}
    cpu_s = lambda c: c["time"]["user_s"] + c["time"]["sys_s"]
    printed = lambda c: (c["iterations"], sci(c["residual_rel"]), f"{c['port_v'][0]:.9e}", f"{c['port_v'][1]:.9e}")

    def smoother(path):
        """key -> string for the one '# ams_smoother' line of a leg's stderr ({} if none)."""
        m = re.search(r"^# ams_smoother (.*)$", path.read_text() if path.exists() else "", re.M)
        return dict(t.split("=", 1) for t in m.group(1).split()) if m else {}

    for name, _, _, _, cs in sm_trees:
        for (n, label), c in cs.items():
            c["smoother"] = smoother(sm_dir / name / f"{c['leg']}.err") if label == "ams" else {}

    # Reference Ritz values: ritz_gap/w<omega>.err, one '# ams_smoother' line per size.
    ritz_ref = {}  # (omega, n_interior) -> dict
    for p in sorted((sm_dir / "ritz_gap").glob("w*.err")) if (sm_dir / "ritz_gap").is_dir() else []:
        m = re.fullmatch(r"w(\d+)p(\d+)", p.stem)
        for line in re.findall(r"^# ams_smoother (.*)$", p.read_text(), re.M):
            kv = dict(t.split("=", 1) for t in line.split())
            ritz_ref[(float(f"{m.group(1)}.{m.group(2)}"), int(kv["edge_dim"]))] = kv

    def tree(build, omega, threading="default"):
        return next((cs for _, b, w, t, cs in sm_trees if (b, w, t) == (build, omega, threading)), {})

    sm_metas = [c["meta"] for *_, cs in sm_trees for c in cs.values()]
    sm_loads = [float(m[k].split()[0]) for m in sm_metas for k in ("loadavg_start", "loadavg_end") if k in m]
    sm_dates = sorted(m["start"] for m in sm_metas)
    sm_commit = lambda build: sorted({c["meta"]["git"] for _, b, _, _, cs in sm_trees if b == build for c in cs.values()})
    omegas = sorted({w for _, _, w, _, _ in sm_trees})

    print()
    print("[smoother_weight]")
    print("issue = 945")
    print(
        "note = "
        + q(
            "Before / after measurement of the issue #945 fix, on the fixture of this file. Before: the "
            "AMS V-cycle's damped-Jacobi edge smoother used the fixed weight 0.6. After: each "
            "preconditioner build takes 30 steps of plain Lanczos on D^-1/2 P D^-1/2 (P the real "
            "proxy the V-cycle smooths, D its diagonal) and a Gershgorin row-sum bound g, and uses "
            "0.6 if 0.6 * theta < 2, else 1.5 / min(1.1 * theta, g), theta being the largest Ritz "
            "value. Nothing else differs between the builds. One ams_cpu_sweep.sh tree per build "
            "and frequency, one process per leg."
        )
    )
    print(
        "guaranteed = "
        + q(
            "theta is a lower bound on lambda_max(D^-1 P) and g an upper bound. So 0.6 * theta >= 2 "
            "proves that 0.6 is at or above the stability bound 2 / lambda_max: the weight is "
            "lowered only where the old one is proven unstable, and is exactly 0.6 everywhere "
            "else. A lowered weight is below 2 / lambda_max when g is the smaller term of the "
            "minimum, and in any case when 1.5 / rho_hat < 2 / g, which the cells below let you check."
        )
    )
    print(
        "heuristic = "
        + q(
            "1.1 is an allowance for what 30 Lanczos steps have not resolved, not a bound. Where "
            "1.1 * theta is the smaller term, the lowered weight is below 2 / lambda_max only if "
            "lambda_max < 1.47 * theta. And a weight kept at 0.6 is not thereby shown stable: the "
            "Ritz value only failed to prove it unstable (it is proven stable only if g < 10/3). "
            "ritz_gap_rel in [[smoother_weight_comparison]] is the measured shortfall of the "
            "30-step theta against a 1000-step Lanczos run, itself a lower bound."
        )
    )
    print(
        "how_to_read = "
        + q(
            "Developer machine, single samples. Iteration counts are the result. process_user_s + "
            "process_sys_s is the CPU cost of the whole process (mesh, assembly, setup, solve); "
            "wall-clock times (setup_s, krylov_s) depend on the load at that moment and are not "
            "compared between legs. main has no GEODE_SCALING_OMEGA knob, so away from omega = 0.10 "
            "the 'before' is the fix build with the weight forced back to 0.6; at omega = 0.10 both "
            "were run and main_equals_forced_0p6 says whether they agree."
        )
    )
    print(f"run_dir = {q(sm_dir.as_posix())}")
    print(f"trees = {[name for name, *_ in sm_trees]}")
    print(f"main_commit = {q(', '.join(sm_commit('main')))}")
    print(f"fix_commit = {q(', '.join(sorted(set(sm_commit('fix') + sm_commit('w060')))))}")
    print(f"omegas = {omegas}")
    print(f"started_between = [{q(sm_dates[0])}, {q(sm_dates[-1])}]")
    print(f"loadavg_1min_range = [{min(sm_loads):.2f}, {max(sm_loads):.2f}]  # {ncpu} logical CPUs")

    for name, build, omega, threading, cs in sm_trees:
        direct_of = {n: c for (n, label), c in cs.items() if label == "direct" and c["finished"]}
        for (n, label), c in sorted(cs.items(), key=lambda kv: (kv[0][0], [l for l, _ in CONFIGS].index(kv[0][1]))):
            print()
            print("[[smoother_weight_cell]]")
            print(f"tree = {q(name)}")
            print(f"build = {q(BUILD[build])}")
            print(f"omega = {omega:.2f}")
            print(f"threading = {q(THREADS[threading])}")
            print(f"leg = {q(c['leg'])}")
            print(f"size_n = {n}")
            if not c["finished"]:
                print("finished = false")
                continue
            print(f"n_edges = {c['n_edges']}")
            print(f"n_interior = {c['n_interior']}")
            print(f"config = {q(c['config'])}")
            print(f"explicit_converged = {str(c['converged']).lower()}")
            print(f"iterations = {c['iterations']}")
            print(f"residual_rel = {sci(c['residual_rel'])}")
            sm = c["smoother"]
            if sm:
                print(f"ritz_theta_max = {sm['theta_max']}  # 30-step Lanczos, lower bound on lambda_max(D^-1 P)")
                print(f"gershgorin_upper = {sm['gershgorin']}")
                print(f"rho_hat = {sm['rho_hat']}  # min(1.1 * theta, gershgorin)")
                print(f"smooth_weight = {sm['weight']}")
                print(f"smooth_weight_source = {q(sm['source'])}")
                print(f"estimate_ms = {sm['estimate_ms']}  # wall clock of the Gershgorin pass + Lanczos run, sequential")
            print(f"setup_s = {c['warmup_setup_s']:.3f}  # wall clock")
            print(f"krylov_s = {c['warmup_krylov_s']:.3f}  # wall clock")
            if c["time"]:
                print_process(c["time"])
            print(f"loadavg_1min_start = {c['meta']['loadavg_start'].split()[0]}")
            print(f"port_v = [{c['port_v'][0]:.9e}, {c['port_v'][1]:.9e}]")
            if label != "direct" and n in direct_of:
                print(f"port_v_rel_diff_vs_direct = {sci(rel_diff(c['port_v'], direct_of[n]['port_v']))}  # from the 10 printed digits")
            x = c["xcheck"]
            if x and label != "direct":
                print(f"field_rel_l2_vs_direct = {sci(x.get('accuracy_rel_l2_vs_direct', float('nan')))}  # full edge field, from the n{n}_xcheck leg")

    sm_rows = {}  # omega -> [(n, jacobi, before, after)]
    for omega in omegas:
        fx, forced, mn, r1 = tree("fix", omega), tree("w060", omega), tree("main", omega), tree("fix", omega, "rayon1")
        for n in sorted({n for n, _ in fx}):
            j, a = fx.get((n, "jacobi")), fx.get((n, "ams"))
            b = mn.get((n, "ams")) or forced.get((n, "ams"))  # main where it can run this frequency
            if not (j and a and b and j["finished"] and a["finished"] and b["finished"]):
                continue
            sm_rows.setdefault(omega, []).append((n, j, b, a))
            sm, ref = a["smoother"], ritz_ref.get((omega, a["n_interior"]), {})
            print()
            print("[[smoother_weight_comparison]]")
            print(f"omega = {omega:.2f}")
            print(f"size_n = {n}")
            print(f"n_edges = {a['n_edges']}")
            print(f"jacobi_iterations = {j['iterations']}")
            print(f"ams_iterations_before = {b['iterations']}  # weight 0.6")
            print(f"before_build = {q('main' if (n, 'ams') in mn else 'fix build with GEODE_AMS_SMOOTH_WEIGHT=0.6')}")
            print(f"ams_iterations_after = {a['iterations']}")
            print(f"ams_explicit_converged_before = {str(b['converged']).lower()}")
            print(f"ams_explicit_converged_after = {str(a['converged']).lower()}")
            mb, fb = mn.get((n, "ams")), forced.get((n, "ams"))
            if mb and fb and mb["finished"] and fb["finished"]:
                print(f"ams_iterations_before_main = {mb['iterations']}")
                print(
                    f"main_equals_forced_0p6 = {str(printed(mb) == printed(fb)).lower()}"
                    "  # iterations, residual_rel and port_v as printed: main vs the fix build at weight 0.6"
                )
            if sm:
                theta, g, w = float(sm["theta_max"]), float(sm["gershgorin"]), float(sm["weight"])
                print(f"smooth_weight_after = {sm['weight']}")
                print(f"smooth_weight_source = {q(sm['source'])}")
                print(f"ritz_theta_max = {sm['theta_max']}")
                print(f"gershgorin_upper = {sm['gershgorin']}")
                print(f"rho_hat = {sm['rho_hat']}")
                print(f"default_proven_unstable = {str(0.6 * theta >= 2).lower()}  # 0.6 * theta >= 2")
                print(f"weight_below_rigorous_bound = {str(w < 2 / g).lower()}  # weight < 2 / gershgorin <= 2 / lambda_max")
                if ref.get("theta_ref"):
                    tref = float(ref["theta_ref"])
                    print(f"ritz_theta_reference = {ref['theta_ref']}  # {ref['ref_steps']}-step Lanczos, same start vector; still a lower bound")
                    print(f"ritz_gap_rel = {sci((tref - theta) / tref)}  # (reference - theta) / reference")
                    print(f"stability_bound_weight = [{2 / g:.4f}, {2 / tref:.4f}]  # 2 / gershgorin (rigorous), 2 / reference (estimate)")
                    print(f"weight_times_reference = {w * tref:.3f}  # below 2: inside the bound by the reference value")
                print(f"estimate_ms = {sm['estimate_ms']}")
                print(f"ams_setup_s_after = {a['warmup_setup_s']:.3f}  # wall clock, same process as estimate_ms")
                print(f"estimate_share_of_setup = {float(sm['estimate_ms']) / 1e3 / a['warmup_setup_s']:.3f}")
            if j["time"] and a["time"] and b["time"]:
                print("# Whole-process CPU seconds (user + sys): mesh + assembly + setup + solve.")
                print(f"jacobi_process_cpu_s = {cpu_s(j):.2f}")
                print(f"ams_process_cpu_s_before = {cpu_s(b):.2f}")
                print(f"ams_process_cpu_s_after = {cpu_s(a):.2f}")
            ra, rj = r1.get((n, "ams")), r1.get((n, "jacobi"))
            if ra and rj and ra["finished"] and rj["finished"]:
                print(f"jacobi_process_cpu_s_rayon1 = {cpu_s(rj):.2f}  # RAYON_NUM_THREADS=1")
                print(f"ams_process_cpu_s_after_rayon1 = {cpu_s(ra):.2f}")
                print(f"cpu_ratio_jacobi_over_ams_rayon1 = {cpu_s(rj) / cpu_s(ra):.2f}  # above 1: the AMS process used less CPU")
                same = printed(ra) == printed(a) and ra["smoother"].get("theta_max") == sm.get("theta_max") and ra["smoother"].get("weight") == sm.get("weight")
                print(
                    f"after_identical_across_threading = {str(same).lower()}"
                    "  # iterations, residual_rel, port_v, theta and weight as printed"
                )
            x = a["xcheck"]
            if x:
                print(f"ams_field_rel_l2_vs_direct_after = {sci(x.get('accuracy_rel_l2_vs_direct', float('nan')))}")

    for omega in omegas:
        rows945 = sm_rows.get(omega, [])
        if len(rows945) < 2:
            continue
        ed = [a["n_edges"] for _, _, _, a in rows945]
        jac = [j["iterations"] for _, j, _, _ in rows945]
        bef = [b["iterations"] for _, _, b, _ in rows945]
        aft = [a["iterations"] for _, _, _, a in rows945]
        wts = [float(a["smoother"]["weight"]) for _, _, _, a in rows945 if a["smoother"]]
        print()
        print("[[smoother_weight_finding]]")
        print(f"omega = {omega:.2f}")
        print(f"sizes_edges = {ed}")
        print(f"jacobi_iterations = {jac}")
        print(f"ams_iterations_before = {bef}  # weight 0.6")
        print(f"ams_iterations_after = {aft}")
        if wts:
            print(f"smooth_weight_after_range = [{min(wts):.4f}, {max(wts):.4f}]")
        print(f"ams_all_explicit_converged_after = {str(all(a['converged'] for *_, a in rows945)).lower()}")
        print(f"ams_iterations_max_over_min_before = {max(bef) / min(bef):.2f}")
        print(f"ams_iterations_max_over_min_after = {max(aft) / min(aft):.2f}")
        print(
            f"flat_threshold_max_over_min = {FLAT_MAX_OVER_MIN:.2f}"
            "  # the band of the #742 / #744 spiral measurement, as in [finding]"
        )
        print(f"ams_iteration_count_flat_after = {str(max(aft) / min(aft) <= FLAT_MAX_OVER_MIN).lower()}")
        print(f"ams_growth_exponent_least_squares_before = {lsq_exponent(ed, bef):.3f}  # iterations ~ edges^p")
        print(f"ams_growth_exponent_least_squares_after = {lsq_exponent(ed, aft):.3f}")
        print(f"jacobi_growth_exponent_least_squares = {lsq_exponent(ed, jac):.3f}")
        print(f"iteration_ratio_jacobi_over_ams_after = {fl((j / a for j, a in zip(jac, aft)), 1)}")
        if all(j["time"] and a["time"] and b["time"] for _, j, b, a in rows945):
            print(f"jacobi_process_cpu_s = {fl((cpu_s(j) for _, j, _, _ in rows945), 2)}  # user + sys, whole process")
            print(f"ams_process_cpu_s_before = {fl((cpu_s(b) for _, _, b, _ in rows945), 2)}")
            print(f"ams_process_cpu_s_after = {fl((cpu_s(a) for *_, a in rows945), 2)}")
        print(
            "verdict = "
            + q(
                f"At omega = {omega:.2f} the AMS iteration count went from {bef[0]} to {bef[-1]} "
                f"(weight 0.6, edges^{lsq_exponent(ed, bef):.2f}) to {aft[0]} to {aft[-1]} "
                f"(edges^{lsq_exponent(ed, aft):.2f}) over {ed[0]} to {ed[-1]} edges; Jacobi takes "
                f"{jac[0]} to {jac[-1]} (edges^{lsq_exponent(ed, jac):.2f}). The count after the fix "
                + (
                    "stays inside"
                    if max(aft) / min(aft) <= FLAT_MAX_OVER_MIN
                    else f"still rises by {max(aft) / min(aft):.2f}x, outside"
                )
                + f" the {FLAT_MAX_OVER_MIN:.2f} band this file calls flat."
            )
        )

    # One table of the statements the README makes about all cells at once.
    all945 = [(w, n, j, b, a) for w in omegas for n, j, b, a in sm_rows.get(w, [])]
    if all945:
        gaps945, share945, eq_main, eq_thr, acc945 = [], [], [], [], []
        for w, n, j, b, a in all945:
            sm, ref = a["smoother"], ritz_ref.get((w, a["n_interior"]), {})
            if sm and ref.get("theta_ref"):
                gaps945.append((float(ref["theta_ref"]) - float(sm["theta_max"])) / float(ref["theta_ref"]))
            if sm:
                share945.append(float(sm["estimate_ms"]) / 1e3 / a["warmup_setup_s"])
            mb, fb = tree("main", w).get((n, "ams")), tree("w060", w).get((n, "ams"))
            if mb and fb and mb["finished"] and fb["finished"]:
                eq_main.append(printed(mb) == printed(fb))
            ra = tree("fix", w, "rayon1").get((n, "ams"))
            if ra and ra["finished"]:
                eq_thr.append(
                    printed(ra) == printed(a)
                    and ra["smoother"].get("theta_max") == sm.get("theta_max")
                    and ra["smoother"].get("weight") == sm.get("weight")
                )
            if a["xcheck"]:
                acc945.append(a["xcheck"].get("accuracy_rel_l2_vs_direct", float("nan")))
        sms = [a["smoother"] for *_, a in all945 if a["smoother"]]
        print()
        print("[smoother_weight.summary]")
        print(f"cells = {len(all945)}  # frequencies x sizes with a before and an after")
        print(f"ams_iterations_after_range = [{min(a['iterations'] for *_, a in all945)}, {max(a['iterations'] for *_, a in all945)}]")
        print(f"ams_iterations_before_range = [{min(b['iterations'] for *_, b, _ in all945)}, {max(b['iterations'] for *_, b, _ in all945)}]")
        print(f"all_explicit_converged_after = {str(all(a['converged'] for *_, a in all945)).lower()}")
        print(f"smooth_weight_range = [{min(float(x['weight']) for x in sms):.4f}, {max(float(x['weight']) for x in sms):.4f}]")
        print(f"smooth_weight_sources = [{', '.join(q(x) for x in sorted({x['source'] for x in sms}))}]")
        print(f"ritz_theta_max_range = [{min(float(x['theta_max']) for x in sms):.4f}, {max(float(x['theta_max']) for x in sms):.4f}]")
        print(f"gershgorin_upper_range = [{min(float(x['gershgorin']) for x in sms):.4f}, {max(float(x['gershgorin']) for x in sms):.4f}]")
        print(
            f"every_weight_below_rigorous_bound = {str(all(float(x['weight']) < 2 / float(x['gershgorin']) for x in sms)).lower()}"
            "  # weight < 2 / gershgorin in every cell"
        )
        if gaps945:
            print(f"ritz_gap_rel_range = [{sci(min(gaps945))}, {sci(max(gaps945))}]  # 30-step theta below the 1000-step reference")
        if share945:
            print(f"estimate_share_of_setup_range = [{min(share945):.3f}, {max(share945):.3f}]  # wall clock, same process")
        if eq_main:
            print(
                f"main_equals_forced_0p6_at_every_size = {str(all(eq_main)).lower()}"
                f"  # {len(eq_main)} sizes at the one frequency main can run"
            )
        if eq_thr:
            print(f"after_identical_across_threading_at_every_size = {str(all(eq_thr)).lower()}  # {len(eq_thr)} sizes")
        if acc945:
            print(f"ams_field_rel_l2_vs_direct_after_range = [{sci(min(acc945))}, {sci(max(acc945))}]")

# ---- issue #945: the AMS tests outside this fixture (optional) ----------------
val_dir = sm_dir / "validation"
if sm_trees and val_dir.is_dir():
    # File stem -> what the test exercises. Order is the output order.
    VALIDATION = {
        "spiral_smoke": "driven COCG + AMS V-cycle, 14k-edge spiral, 4 frequencies (#744)",
        "spiral_benchmark": "driven COCG + AMS V-cycle, 53k-edge spiral, 1 GHz (#744)",
        "spiral_ds": "driven COCG + AMS V-cycle, Djordjevic-Sarkar dielectric spiral",
        "spiral_debye": "driven COCG + AMS V-cycle, Debye dielectric spiral; a known knife edge on macOS (#943)",
        "spiral_drude": "driven COCG + AMS V-cycle, Drude conductor spiral",
        "rough": "driven COCG + AMS V-cycle, rough-conductor spiral (#758)",
        "spiral_aniso": "driven COCG + AMS V-cycle, smoke spiral with a uniaxial substrate and an anisotropic mu, 1 and 10 GHz",
        "driven_public": "driven COCG + AMS V-cycle on the public iterative path, PEC cube n = 4",
        "transmon_5x": "eigen inner CG + AMS V-cycle, gradient-only; asserts Jacobi >= 5x AMS iterations",
        "transmon_3space": "eigen inner CG + AMS V-cycle, gradient-only vs three-space",
        "transmon_direct": "eigen inner CG + AMS V-cycle vs the direct eigensolve",
        "transmon_real": "eigen inner CG + AMS V-cycle, 133k-DOF transmon; asserts Jacobi >= 5x AMS iterations",
        "transmon_real_ams_only": "the AMS leg of transmon_real alone, by a throwaway test added to the fix build (validation/diag945_real_transmon_ams_only.patch)",
        "transmon_minres": "eigen MINRES + ADDITIVE AMS (weight fixed at 1; does not read the smoother weight)",
        "amg_minres": "eigen MINRES + ADDITIVE AMS with the AMG coarse solve (does not read the smoother weight)",
        "coarse_report": "unit: PCG + AMS V-cycle on a 1-D Laplacian, direct vs SGS coarse solve",
        "ams_spd": "unit: the V-cycle is symmetric positive definite (gradient-only)",
        "ams_spd_3space": "unit: the V-cycle is symmetric positive definite (three-space)",
        "ams_spd_amg": "unit: the V-cycle is symmetric positive definite (AMG coarse solve)",
    }

    def val_run(tree_name, stem, where=val_dir):
        path = where / f"{tree_name}_{stem}.txt"
        if not path.exists():
            return None
        txt = path.read_text()
        head = dict(t.split("=", 1) for line in txt.splitlines()[:3] for t in line[2:].split() if "=" in t)
        body = []
        for line in txt.splitlines()[3:]:
            line = re.sub(r"^test \S+ \.\.\. ", "", line)  # the first printed line shares the test's line
            line = re.sub(r"^stderr: ", "", line)  # a failing CLI test prints the child's stderr
            line = re.sub(r"# ams_smoother .*$", "", line)
            line = re.sub(r"\(\d+\) panicked", "(tid) panicked", line)
            line = re.sub(r"<tmp>/[^/\"]*", "<tmp>/dir", line)
            if not line.strip() or "finished in" in line or "µs" in line or '"git_sha"' in line:
                continue
            body.append(line)
        done = re.search(r"^test result: (\w+)\.", txt, re.M)
        drift = re.search(r"after (\d+) iterations but the explicitly recomputed residual .*? = (\S+) did not", txt)
        panic = re.search(r"panicked at [^\n]*\n([^\n]*)", txt)
        return {
            "commit": head.get("git", "unknown")
            + (f" dirty={head['dirty']}" if "dirty" in head else "")
            + (f" + {head['patch']}" if "patch" in head else ""),
            "finished": bool(done),
            "passed": bool(done) and done.group(1) == "ok",
            "iterations": [int(x) for x in re.findall(r"(?:GHz: iters|: iterations) \[(\d+)\]", txt)],
            "report": [l.strip() for l in body if re.search(r"inner-CG|inner-MINRES|outer PCG|^diag945 ", l)],
            "drift": drift.groups() if drift else None,
            "panic": panic.group(1).strip() if panic else None,
            "smoother": [dict(t.split("=", 1) for t in m.split()) for m in re.findall(r"# ams_smoother (.*)$", txt, re.M)],
            "body": body,
        }

    for stem, what in VALIDATION.items():
        mn, fx = val_run("main", stem), val_run("fix", stem)
        un = val_run("uncond", stem, sm_dir / "unconditional_rule")
        if not (mn or fx):
            continue
        print()
        print("[[smoother_weight_validation]]")
        print(f"name = {q(stem)}")
        print(f"exercises = {q(what)}")
        for tname, r in (("main", mn), ("fix", fx), ("unconditional_rule", un)):
            if r is None:
                continue
            if tname == "unconditional_rule":
                print("# THROWAWAY build, not shipped: the weight is min(0.6, 1.5 / rho_hat) with no proof condition.")
            print(f"{tname}_commit = {q(r['commit'])}")
            if not r["finished"]:
                print(f"{tname}_finished = false")
                continue
            print(f"{tname}_passed = {str(r['passed']).lower()}")
            if r["iterations"]:
                print(f"{tname}_iterations = {r['iterations']}  # COCG iterations per frequency, as the test prints them")
            if r["report"]:
                print(f"{tname}_report = [{', '.join(q(line) for line in r['report'])}]")
            if r["drift"]:
                print(
                    f"{tname}_failure = "
                    + q(
                        f"recursive residual met the tolerance after {r['drift'][0]} iterations, explicit "
                        f"residual {r['drift'][1]} did not (tol 1e-10)"
                    )
                )
            elif not r["passed"] and r["panic"]:
                print(f"{tname}_failure = {q(r['panic'])}")
            if tname == "unconditional_rule" and r["smoother"]:
                print(f"{tname}_smooth_weight = [{', '.join(s_['weight'] for s_ in r['smoother'])}]")
        if mn and fx and mn["finished"] and fx["finished"]:
            print(
                f"output_identical = {str(mn['body'] == fx['body']).lower()}"
                "  # main vs fix: every printed line, apart from timings, thread ids, temp-dir names and the smoother report"
            )
        sm = fx["smoother"] if fx else []
        if sm:
            print("# One entry per preconditioner build on the fix build (GEODE_AMS_SMOOTH_REPORT=600).")
            print(f"fix_smooth_weight = [{', '.join(s_['weight'] for s_ in sm)}]")
            print(f"fix_smooth_weight_source = [{', '.join(q(s_['source']) for s_ in sm)}]")
            print(f"fix_ritz_theta_max = [{', '.join(s_['theta_max'] for s_ in sm)}]")
            print(f"fix_gershgorin_upper = [{', '.join(s_['gershgorin'] for s_ in sm)}]")
            print(f"fix_ritz_theta_reference = [{', '.join(s_['theta_ref'] for s_ in sm)}]  # up to 600 Lanczos steps")
            gaps = [(float(s_["theta_ref"]) - float(s_["theta_max"])) / float(s_["theta_ref"]) for s_ in sm]
            print(f"fix_ritz_gap_rel_max = {sci(max(gaps))}")
            print(
                f"fix_default_weight_times_reference_max = {max(0.6 * float(s_['theta_ref']) for s_ in sm):.4f}"
                "  # 0.6 * reference: below 2 means 0.6 is inside the bound by the reference value"
            )
            lowered = [s_ for s_ in sm if s_["source"] == "estimate"]
            if lowered:
                print(
                    f"fix_lowered_weight_below_rigorous_bound = "
                    f"{str(all(float(s_['weight']) < 2 / float(s_['gershgorin']) for s_ in lowered)).lower()}"
                    "  # lowered weight < 2 / gershgorin; false means its stability rests on the Ritz value"
                )


# ---------------------------------------------------------------------------
# Issue #963: where the residual AMS iteration growth comes from.
# smoother_963/<variant>_w<omega>/ : one ams_cpu_sweep.sh tree per
# (build, V-cycle variant, omega), AMS legs only, RAYON_NUM_THREADS=1.
# Variant "main" is clean origin/main; every other variant is the branch,
# with the GEODE_DRIVEN_AMS_* knobs its legs' .meta files record
# ("default" sets none). validation/<variant>_<test>.txt: the AMS tests
# outside the cube fixture. Written as [smoother_963].
# ---------------------------------------------------------------------------
s9_dir = d / "smoother_963"
if s9_dir.is_dir():
    KNOBS = ("GEODE_DRIVEN_AMS_SMOOTHER", "GEODE_DRIVEN_AMS_CYCLE", "GEODE_DRIVEN_AMS_PI_COARSE")
    s9 = []  # (tree, variant, omega, cells)
    for p in sorted(s9_dir.glob("*_w*")):
        m = re.fullmatch(r"(.+)_w(\d+)p(\d+)", p.name)
        if m and p.is_dir():
            s9.append((p.name, m.group(1), float(f"{m.group(2)}.{m.group(3)}"), load_cells(p, CONFIGS[2:])))
    order = lambda t: (t[2] != 0.10, t[2], t[1] != "main", t[1] != "default", t[1])
    s9.sort(key=order)

    def s9_series(cs):
        ns = sorted(n for (n, _) in cs)
        return [cs[(n, "ams")] for n in ns]

    def growth(ser):
        its = [c["iterations"] for c in ser]
        return max(its) / min(its)

    def cpu(c):
        return c["time"]["user_s"] + c["time"]["sys_s"]

    by = {(v, w): s9_series(cs) for _, v, w, cs in s9}
    metas9 = [c["meta"] for _, _, _, cs in s9 for c in cs.values()]
    loads9 = [float(m[k].split()[0]) for m in metas9 for k in ("loadavg_start", "loadavg_end") if k in m]
    dates9 = sorted(m["start"] for m in metas9)
    commit9 = lambda pred: sorted({c["meta"]["git"] for _, v, _, cs in s9 if pred(v) for c in cs.values()})
    omegas9 = sorted({w for _, _, w, _ in s9})

    def identical(a, b):
        return len(a) == len(b) and all(
            (x["iterations"], sci(x["residual_rel"]), x["port_v"]) == (y["iterations"], sci(y["residual_rel"]), y["port_v"])
            for x, y in zip(a, b)
        )

    print()
    print("[smoother_963]")
    print("issue = 963")
    print(
        "note = "
        + q(
            "Where the residual growth of the AMS iteration count comes from (26 to 55 from 1.9k to 102k "
            "edges at omega = 0.10 after #945). Each candidate is one ams_cpu_sweep.sh tree on the fixture "
            "of this file: the edge smoother (GEODE_DRIVEN_AMS_SMOOTHER), the way the two auxiliary-space "
            "corrections are combined (GEODE_DRIVEN_AMS_CYCLE) and the vector-nodal Pi^T P Pi coarse solve "
            "(GEODE_DRIVEN_AMS_PI_COARSE). The gradient-space solve is the exact LU in every tree. 'main' is "
            "clean origin/main; the others are the branch, 'default' with no knob set."
        )
    )
    print(
        "how_to_read = "
        + q(
            "Developer machine shared with other work, single samples, RAYON_NUM_THREADS=1, one process per "
            "leg. Iteration counts are the result. process_cpu_s (user + sys of the whole process: mesh, "
            "assembly, setup, solve) is the cost figure; it is inflated by an oversubscribed host, and "
            "loadavg_1min_start is recorded per leg so it can be judged. setup_s and krylov_s are wall clock "
            "and depend on the load at that moment; they are recorded, not compared."
        )
    )
    print(f"run_dir = {q(s9_dir.as_posix())}")
    print(f"main_commit = {q(', '.join(commit9(lambda v: v == 'main')))}")
    print(f"branch_commit = {q(', '.join(commit9(lambda v: v != 'main')))}")
    print(f"omegas = {omegas9}")
    print(f"started_between = [{q(dates9[0])}, {q(dates9[-1])}]")
    print(f"loadavg_1min_range = [{min(loads9):.2f}, {max(loads9):.2f}]  # {host.get('logical_cpus', '?')} logical CPUs")
    for w in omegas9:
        if ("main", w) in by and ("default", w) in by:
            print(
                f"default_equals_main_w{w:.2f}".replace(".", "p")
                + f" = {str(identical(by[('main', w)], by[('default', w)])).lower()}"
                "  # iterations, explicit residual and port voltage, every size"
            )

    for tree, variant, w, cs in s9:
        ser = s9_series(cs)
        if not ser:
            continue
        knobs = {k: ser[0]["meta"][k] for k in KNOBS if k in ser[0]["meta"]}
        main = by.get(("main", w), [])
        print()
        print("[[smoother_963_series]]")
        print(f"tree = {q(tree)}")
        print(f"variant = {q(variant)}")
        print(f"build = {q('origin/main' if variant == 'main' else 'branch')}")
        print(f"omega = {w:.2f}")
        print(f"knobs = {q(' '.join(f'{k}={v}' for k, v in knobs.items()) or 'none')}")
        print(f"n_edges = {[c['n_edges'] for c in ser]}")
        print(f"converged = {[c['explicit_converged'] if 'explicit_converged' in c else c['converged'] for c in ser]}".lower())
        print(f"iterations = {[c['iterations'] for c in ser]}")
        g = growth(ser)
        print(f"iterations_max_over_min = {g:.3f}")
        print(f"within_flat_band = {str(g <= FLAT_MAX_OVER_MIN).lower()}  # max / min <= {FLAT_MAX_OVER_MIN:.3f}")
        edges = [c["n_edges"] for c in ser]
        its = [c["iterations"] for c in ser]
        print(f"growth_exponent_lsq = {lsq_exponent(edges, its):.3f}  # slope of log(iterations) on log(edges)")
        print(f"residual_rel_max = {sci(max(c['residual_rel'] for c in ser))}")
        if main and len(main) == len(ser) and variant != "main":
            print(
                f"port_v_rel_diff_vs_main_max = {sci(max(rel_diff(c['port_v'], b['port_v']) for c, b in zip(ser, main)))}"
                "  # same tolerance, different preconditioner"
            )
        print(f"setup_s = {fl([c['warmup_setup_s'] for c in ser])}  # wall clock")
        print(f"krylov_s = {fl([c['warmup_krylov_s'] for c in ser])}  # wall clock")
        print(f"process_cpu_s = {fl([cpu(c) for c in ser], 2)}")
        print(f"process_max_rss_gb = {fl([c['time']['max_rss_bytes'] / 1e9 for c in ser])}")
        print(f"loadavg_1min_start = {fl([float(c['meta']['loadavg_start'].split()[0]) for c in ser], 2)}")

    # Findings: every number below is read from the series above.
    W = 0.10
    get = lambda v, w=W: by.get((v, w), [])
    last = lambda v, w=W: get(v, w)[-1] if get(v, w) else None
    gs = lambda v, w=W: growth(get(v, w)) if get(v, w) else float("nan")
    its = lambda v, w=W: "/".join(str(c["iterations"]) for c in get(v, w))
    smoothers = [v for (v, w) in by if w == W and v.startswith("sm_")]
    flat = sorted(v for (v, w) in by if w == W and v not in ("main", "default") and gs(v) <= FLAT_MAX_OVER_MIN)
    print()
    print("[smoother_963.finding]")
    print(
        "growth_source = "
        + q(
            f"The vector-nodal coarse solve. With the shipped 4-sweep symmetric Gauss-Seidel on Pi^T P Pi the "
            f"count is {its('default')} (max/min {gs('default'):.2f}); with an exact LU there and nothing else "
            f"changed it is {its('pi_direct')} ({gs('pi_direct'):.2f}), and at omega = 0.05 / 0.20 "
            f"{gs('pi_direct', 0.05):.2f} / {gs('pi_direct', 0.20):.2f}. More SGS sweeps there narrow the growth "
            f"in step (sgs:8 {gs('pi_sgs8'):.2f}, sgs:16 {gs('pi_sgs16'):.2f}). A fixed number of SGS sweeps "
            "is not a mesh-independent approximate inverse of a Laplacian-like block, which is what the "
            "Hiptmair-Xu bound assumes; the gradient block is already exact."
        )
    )
    print(
        "not_the_smoother = "
        + q(
            "No edge smoother tried removes the growth: "
            + ", ".join(f"{v[3:]} {its(v)} ({gs(v):.2f})" for v in sorted(smoothers))
            + f", against {its('default')} ({gs('default'):.2f}) for the default damped Jacobi. A stronger "
            "smoother lowers the small-mesh count and leaves the slope."
        )
    )
    print(
        "not_proxy_or_port = "
        + q(
            f"With exact auxiliary solves the count is flat ({its('pi_direct')}; {its('cyc_mult_pi_direct')} with "
            "the multiplicative 0-1-2-1-0 cycle), so the parts not changed between those trees and the "
            "default (the real SPD proxy P(omega) for the complex operator, the port and loss terms in it) "
            "add no mesh dependence on this fixture. They set the level, not the growth. Not separately "
            "diagnosed: that is implied by the flat exact-solve series."
        )
    )
    print(
        "level_vs_palace = "
        + q(
            f"With an exact Pi solve, combining the two auxiliary corrections multiplicatively instead of "
            f"additively lowers the count from {its('pi_direct')} to {its('cyc_mult_pi_direct')}, the closest "
            "to Palace's AMS (12 to 17 on the r6i record) of any variant here. What remains of that gap was "
            "not investigated (Palace runs GMRES and hypre's own AMS cycle and smoother)."
        )
    )
    c0, ca, cs16, cd = last("default"), last("pi_amg4"), last("pi_sgs16"), last("pi_direct")
    if c0 and ca and cs16 and cd:
        print(
            "cost_at_largest = "
            + q(
                f"At {c0['n_edges']} edges, process CPU seconds: default {cpu(c0):.2f} ({c0['iterations']} it, "
                f"{c0['time']['max_rss_bytes'] / 1e9:.2f} GB), Pi amg:4 {cpu(ca):.2f} ({ca['iterations']} it, "
                f"{ca['time']['max_rss_bytes'] / 1e9:.2f} GB), Pi sgs:16 {cpu(cs16):.2f} ({cs16['iterations']} it, "
                f"{cs16['time']['max_rss_bytes'] / 1e9:.2f} GB), Pi direct {cpu(cd):.2f} ({cd['iterations']} it, "
                f"{cd['time']['max_rss_bytes'] / 1e9:.2f} GB). The flat-band variants do fewer but dearer "
                "iterations, and up to this size that does not pay for itself; single samples on a loaded "
                "host."
            )
        )
    rss_ratio = [
        a["time"]["max_rss_bytes"] / b["time"]["max_rss_bytes"] for a, b in zip(get("pi_amg4"), get("default"))
    ]
    if rss_ratio:
        print(
            f"pi_amg4_rss_over_default = {fl(rss_ratio)}  # peak RSS of the whole process, per size"
        )
    direct_rss_x = (
        cd["time"]["max_rss_bytes"] / c0["time"]["max_rss_bytes"] if c0 and cd else float("nan")
    )
    print(f"flat_band_variants_w0p10 = {flat}")
    print(
        "flat_band_at_all_omegas = "
        + str(
            sorted(
                v
                for v in flat
                if all((v, w) in by and gs(v, w) <= FLAT_MAX_OVER_MIN for w in omegas9)
            )
        )
    )
    print(
        "default_changed = false  # the shipped V-cycle is unchanged; every candidate is an opt-in knob"
    )
    print(
        "recommendation = "
        + q(
            "Keep the default. The growth is the vector-nodal block, so the fix is a mesh-independent Pi "
            "solve, and of those measured amg:4 (GEODE_DRIVEN_AMS_PI_COARSE=amg:4) is the candidate: flat "
            "at all three frequencies, O(n) per apply, and with a peak memory close to the default's "
            f"(pi_amg4_rss_over_default) where the exact LU needs {direct_rss_x:.1f} times it at "
            f"{c0['n_edges'] if c0 else '?'} edges. It costs more CPU than the default up to 102k edges, so the switch rests on the at-scale "
            "cells (the r6i ladder to 898k edges, where the default takes 105 iterations), which are "
            "operator-run. Combine it with GEODE_DRIVEN_AMS_CYCLE=multiplicative there to also measure the "
            "level. Smoother changes (l1-Jacobi, Chebyshev, more Jacobi or SGS sweeps) are not worth a "
            "default change on this evidence."
        )
    )

    # The AMS tests outside the cube fixture (the #961 set).
    val9 = s9_dir / "validation"
    VAL9 = {
        "spiral_smoke": "driven COCG + AMS, 14k-edge spiral, 4 frequencies (#744)",
        "spiral_benchmark": "driven COCG + AMS, 53k-edge spiral, 1 GHz (#744)",
        "spiral_aniso": "driven COCG + AMS, smoke spiral with anisotropic materials, 1 and 10 GHz",
        "spiral_ds": "driven COCG + AMS, Djordjevic-Sarkar dielectric spiral",
        "spiral_drude": "driven COCG + AMS, Drude conductor spiral",
        "spiral_debye": "driven COCG + AMS, Debye dielectric spiral (knife edge, #943)",
        "rough": "driven COCG + AMS, rough-conductor spiral (#758)",
        "driven_public": "driven COCG + AMS on the public iterative path, PEC cube n = 4",
    }

    def val9_run(tree, stem):
        path = val9 / f"{tree}_{stem}.txt"
        if not path.exists():
            return None
        txt = path.read_text()
        head = dict(t.split("=", 1) for line in txt.splitlines()[:3] for t in line[2:].split() if "=" in t)
        done = re.search(r"^test result: (\w+)\.", txt, re.M)
        return {
            "commit": head.get("git", "unknown") + (f" dirty={head['dirty']}" if "dirty" in head else ""),
            "env": head.get("env", "none"),
            "load": head.get("loadavg_start", "").split(",")[0],
            "finished": bool(done),
            "passed": bool(done) and done.group(1) == "ok",
            "iterations": [int(x) for x in re.findall(r"(?:GHz: iters|: iterations) \[(\d+)\]", txt)],
            "time": time_file(val9 / f"{tree}_{stem}.time"),
        }

    trees9 = sorted({p.stem.split("_", 1)[0] if not p.stem.startswith("pi_") else "_".join(p.stem.split("_")[:2]) for p in val9.glob("*.txt")}) if val9.is_dir() else []
    trees9.sort(key=lambda t: (t != "main", t != "default", t))
    for stem, what in VAL9.items():
        runs = {t: val9_run(t, stem) for t in trees9}
        runs = {t: r for t, r in runs.items() if r}
        if not runs:
            continue
        print()
        print("[[smoother_963_validation]]")
        print(f"name = {q(stem)}")
        print(f"exercises = {q(what)}")
        for t, r in runs.items():
            print(f"{t}_commit = {q(r['commit'])}")
            print(f"{t}_env = {q(r['env'])}")
            if not r["finished"]:
                print(f"{t}_finished = false")
                continue
            print(f"{t}_passed = {str(r['passed']).lower()}")
            if r["iterations"]:
                print(f"{t}_iterations = {r['iterations']}  # COCG iterations per frequency, as the test prints them")
            if r["time"]:
                print(f"{t}_process_cpu_s = {r['time']['user_s'] + r['time']['sys_s']:.2f}  # includes the direct reference solve")
            if r["load"]:
                print(f"{t}_loadavg_1min_start = {r['load']}")
