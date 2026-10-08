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
print("#   * THE [[cell]] AND [[single_thread_cell]] TABLES MEASURE THE SHIPPED AMS")
print("#     (edge-smoother weight 0.6). [diagnostics] shows that weight is above the")
print("#     damped-Jacobi stability bound on this fixture and that the iteration")
print("#     growth goes away below it. Read [finding].scope before quoting a result.")
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
    "(i) library issue for the AMS edge smoother (crates/geode-core/src/eigen/ams.rs, "
    "DEFAULT_SMOOTH_WEIGHT = 0.6): take the weight from an estimate of the spectral radius of "
    "D^-1 A, or use an l1-Jacobi smoother; validate on the #742 / #744 spiral, the transmon AMS "
    "tests and this fixture before changing the constant",
    "(ii) the parallel sparse triangular solve inside the V-cycle (lu_solve in eigen/ams.rs), "
    "which fans two small solves per iteration out to the rayon pool and causes the contention "
    "recorded here",
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
        )
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
