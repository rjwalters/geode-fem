#!/usr/bin/env python3
"""Write results_ams_cpu_r6i.toml from the #930 at-scale run tree.

usage: summarize_ams_r6i.py <run-dir>             > results_ams_cpu_r6i.toml
       summarize_ams_r6i.py <run-dir> --markdown  > the README tables

<run-dir> (runs/2026-10-08_r6i_ams_scale/) holds:

  t<T>_rep<K>/      ams_cpu_sweep.sh trees: one process per (size, config),
                    pinned with taskset to T cores (T = 1 or 8), repeat K.
                    Legs n<N>_{jacobi,ams,direct,xcheck}.{stdout,err,time,meta}
  direct_large/     optional: the Direct legs at the sizes where one LU is
                    expensive (same leg naming, one tree per thread count,
                    t<T>/)
  omega/t<T>_w<W>/  optional: the same legs at another drive frequency, W
                    written as 0p05
  palace/           box_palace_cpu.sh output with Palace's Solver.Linear.Type
                    "Default": per run a .log, .time, .cgroup_peak,
                    _port-V.csv, plus runs.txt and build_provenance.txt
  palace_ams/       the same with Solver.Linear.Type "AMS"
  host_extra.txt    instance type, kernel, OS, CPU topology
  session.txt       launch / terminate times and the cost estimate

Every number in the output is read from those files or computed from them
here. The derived quantities are: sums (setup + Krylov, user + sys), min /
median / max over repeats, ratios, |a - b| / |b| of port voltages, and the
least-squares slope of log(iterations) on log(edges). Pure stdlib.
"""
import math
import pathlib
import re
import statistics
import sys
import tomllib

d = pathlib.Path(sys.argv[1])
MARKDOWN = "--markdown" in sys.argv[2:]

CONFIG_OF = {"direct": "1_direct", "jacobi": "2_iterative_csr", "ams": "5_iterative_ams"}
LABELS = ["jacobi", "ams", "direct"]


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
    """wall / user / sys seconds and max RSS bytes from GNU time -v."""
    if not path.exists():
        return {}
    txt = path.read_text()
    m = re.search(r"Elapsed \(wall clock\) time.*: ([\d:.]+)", txt)
    if not m:
        return {}
    parts = [float(p) for p in m.group(1).split(":")]
    return {
        "wall_s": sum(p * 60**i for i, p in enumerate(reversed(parts))),
        "user_s": float(re.search(r"User time \(seconds\): ([\d.]+)", txt).group(1)),
        "sys_s": float(re.search(r"System time \(seconds\): ([\d.]+)", txt).group(1)),
        "max_rss_bytes": 1024
        * int(re.search(r"Maximum resident set size \(kbytes\): (\d+)", txt).group(1)),
    }


def keyvals(path):
    out = {}
    if path.exists():
        for line in path.read_text().splitlines():
            k, sep, v = line.partition("=")
            if sep and " " not in k and not k.startswith("#"):
                out[k] = v
    return out


def smoother(path):
    """The '# ams_smoother ...' line of an AMS leg's stderr, as a dict."""
    if not path.exists():
        return {}
    m = re.search(r"^# ams_smoother (.*)$", path.read_text(), re.M)
    return dict(kv.split("=", 1) for kv in m.group(1).split()) if m else {}


def q(s):
    return '"' + str(s).replace("\\", "\\\\").replace('"', '\\"') + '"'


def fl(vals, digits=3):
    return "[" + ", ".join(f"{v:.{digits}f}" for v in vals) + "]"


def sci(x, digits=2):
    return "nan" if x != x else f"{x:.{digits}e}"


def rel_diff(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1]) / math.hypot(b[0], b[1])


def lsq_exponent(edges, its):
    xs, ys = [math.log(e) for e in edges], [math.log(i) for i in its]
    mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
    return sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sum((x - mx) ** 2 for x in xs)


def mmm(vals):
    return min(vals), statistics.median(vals), max(vals)


def load_leg(tree, stem, label):
    """One leg -> a dict, or a failure record when it has no converged cell."""
    meta = keyvals(tree / f"{stem}.meta")
    if not meta:
        return None
    cells = [c for c in fragment(tree / f"{stem}.stdout") if c["config"] == CONFIG_OF[label]]
    t = time_file(tree / f"{stem}.time")
    rec = {"meta": meta, "time": t, "rc": int(meta.get("rc", "-1")), "tree": tree, "stem": stem}
    if not cells or rec["rc"] != 0 or "dnf_attempt_s" in cells[0] or not t:
        rec["failed"] = True
        rec["cell"] = cells[0] if cells else None
        return rec
    rec["failed"] = False
    rec["cell"] = cells[0]
    rec["smoother"] = smoother(tree / f"{stem}.err") if label == "ams" else {}
    return rec


def tree_legs(tree):
    """{(size_n, label): leg} for one ams_cpu_sweep.sh tree."""
    out = {}
    for meta in sorted(tree.glob("n*_*.meta")):
        m = re.fullmatch(r"n(\d+)_(jacobi|ams|direct)", meta.stem)
        if m:
            leg = load_leg(tree, meta.stem, m.group(2))
            if leg:
                out[(int(m.group(1)), m.group(2))] = leg
    return out


def collect(trees):
    """{(size_n, label, threads): [leg, ...]} over (threads, tree) pairs, plus failures."""
    ok, failed = {}, []
    for threads, tree in trees:
        for (n, label), leg in tree_legs(tree).items():
            if leg["failed"]:
                failed.append((threads, n, label, leg))
            else:
                ok.setdefault((n, label, threads), []).append(leg)
    return ok, failed


def rep_trees(parent, pattern):
    out = []
    for p in sorted(parent.glob(pattern)):
        m = re.match(r"t(\d+)", p.name)
        if p.is_dir() and m:
            out.append((int(m.group(1)), p))
    return out


main_trees = rep_trees(d, "t*_rep*")
large_trees = rep_trees(d / "direct_large", "t*")
cells, failures = collect(main_trees + large_trees)

host = keyvals(d / "host_extra.txt")
session = keyvals(d / "session.txt")
sizes = sorted({n for (n, _, _) in cells})
threads_run = sorted({t for (_, _, t) in cells})
edges_of = {n: legs[0]["cell"]["n_edges"] for (n, _, _), legs in cells.items()}
first_meta = next(iter(cells.values()))[0]["meta"]
commit = first_meta["git"].split()[0]
assert all(
    leg["meta"]["git"].split()[0] == commit and leg["meta"]["git"].endswith("dirty=0")
    for legs in cells.values()
    for leg in legs
), "legs come from more than one commit or a dirty checkout"


def stat(legs):
    """The per-cell numbers of one (size, config, threads) over its repeats."""
    c0 = legs[0]["cell"]
    out = {
        "reps": len(legs),
        "iterations": sorted({l["cell"]["iterations"] for l in legs}),
        "residual": max(l["cell"]["residual_rel"] for l in legs),
        "setup": [l["cell"]["warmup_setup_s"] for l in legs],
        "krylov": [l["cell"]["warmup_krylov_s"] for l in legs],
        "solve": [l["cell"]["warmup_setup_s"] + l["cell"]["warmup_krylov_s"] for l in legs],
        "wall": [l["time"]["wall_s"] for l in legs],
        "user": [l["time"]["user_s"] for l in legs],
        "sys": [l["time"]["sys_s"] for l in legs],
        "core": [l["time"]["user_s"] + l["time"]["sys_s"] for l in legs],
        "rss": [l["time"]["max_rss_bytes"] / 1e9 for l in legs],
        "port_v": c0["port_v"],
        "port_v_all": [l["cell"]["port_v"] for l in legs],
        "converged": all(l["cell"]["converged"] for l in legs),
        "cpus": sorted({l["meta"].get("cpus_allowed_list", "unpinned") for l in legs}),
        "rayon": sorted({l["meta"].get("rayon_num_threads", "unset") for l in legs}),
        "load_start": max(float(l["meta"]["loadavg_start"].split()[0]) for l in legs),
        "smoother": legs[0].get("smoother", {}),
        "smoother_all": [l.get("smoother", {}) for l in legs],
    }
    return out


S = {k: stat(v) for k, v in cells.items()}


def reference_v(n):
    """(label, port_v) of the reference at one size: Direct if any, else Jacobi."""
    for label in ("direct", "jacobi"):
        for t in sorted(threads_run, reverse=True):
            if (n, label, t) in S:
                return label, S[(n, label, t)]["port_v"]
    return None, None


# In-harness full-field rel-L2 vs Direct, from the combined cross-check legs.
xcheck = {}
for threads, tree in main_trees + large_trees:
    for p in sorted(tree.glob("n*_xcheck.stdout")):
        n = int(re.match(r"n(\d+)_", p.name).group(1))
        if keyvals(tree / f"{p.stem}.meta").get("rc") != "0":
            continue
        for c in fragment(p):
            for label in ("jacobi", "ams"):
                if c["config"] == CONFIG_OF[label]:
                    xcheck[(n, label)] = c["accuracy_rel_l2_vs_direct"]

# ---- Palace -----------------------------------------------------------------
PAL_ROWS = ["Operator Construction", "Linear Solve", "Setup", "Preconditioner", "Total"]


def palace_run(log):
    txt = log.read_text()
    stem = log.with_suffix("")
    m = re.fullmatch(r"palace_cpu_np(\d+)_n(\d+)_run(\d+)", log.stem)
    out = {"ranks": int(m.group(1)), "n": int(m.group(2)), "run": int(m.group(3))}
    it = re.search(r"(?:GMRES|CG) solver converged in (\d+) iterations", txt)
    out["ok"] = bool(it) and "Elapsed Time Report" in txt
    if not out["ok"]:
        return out
    out["iterations"] = int(it.group(1))
    out["changeset"] = re.search(r"Git changeset ID: (\S+)", txt).group(1)
    out["backend"] = re.search(r"libCEED backend: (\S+)", txt).group(1)
    out["mpi"] = re.search(r"Running with (.*)", txt).group(1).strip()
    report = txt[txt.index("Elapsed Time Report") : txt.index("Peak Memory")]
    for row in PAL_ROWS:
        r = re.search(rf"^\s*{row}\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*$", report, re.M)
        out[row] = float(r.group(3))
    mem = txt[txt.index("Peak Memory") :]
    out["hwm"] = re.search(r"^Total\s+\S+\s+\S+\s+(\S+)\s*$", mem, re.M).group(1)
    out["time"] = time_file(pathlib.Path(f"{stem}.time"))
    peak = pathlib.Path(f"{stem}.cgroup_peak")
    out["cgroup_peak_gb"] = int(peak.read_text().strip()) / 1e9 if peak.exists() else math.nan
    rows = pathlib.Path(f"{stem}_port-V.csv").read_text().strip().splitlines()
    _, vinc, re_v, im_v = (float(x) for x in rows[-1].split(","))
    out["port_v"] = [re_v / vinc, im_v / vinc]
    return out


# kind -> (directory, Solver.Linear.Type, what it is, table label)
PALACE_KINDS = {
    "default": ("palace", "Default", "SuperLU", "Palace GMRES + SuperLU (its default)"),
    "ams": ("palace_ams", "AMS", "AMS", "Palace GMRES + AMS"),
}
def palace_solve_s(r):
    """Palace's whole linear solve: its Setup + Preconditioner + Linear Solve rows."""
    return r["Setup"] + r["Preconditioner"] + r["Linear Solve"]


palace = {}
palace_failed = []
for kind, (sub, _, _, _) in PALACE_KINDS.items():
    for log in sorted((d / sub).glob("palace_cpu_np*_n*_run*.log")):
        r = palace_run(log)
        if r["ok"]:
            palace.setdefault((kind, r["n"], r["ranks"]), []).append(r)
        else:
            palace_failed.append(f"{sub}/{log.name}")
palace_build = keyvals(d / "palace" / "build_provenance.txt")

# ---- extra drive frequencies ------------------------------------------------
omega = {}
omega_failures = []
for p in sorted((d / "omega").glob("t*_w*")) if (d / "omega").exists() else []:
    m = re.fullmatch(r"t(\d+)_w(\d+)p(\d+)", p.name)
    w = float(f"{m.group(2)}.{m.group(3)}")
    for (n, label), leg in tree_legs(p).items():
        if leg["failed"]:
            omega_failures.append((int(m.group(1)), n, label, leg))
        else:
            omega.setdefault((w, n, label, int(m.group(1))), []).append(leg)
OS = {k: stat(v) for k, v in omega.items()}


def its(n, label):
    for t in threads_run:
        if (n, label, t) in S:
            return S[(n, label, t)]["iterations"]
    return None


# =============================================================================
if MARKDOWN:
    def med(k, key):
        return statistics.median(S[k][key]) if k in S else None

    def f(x, digits=1):
        return "–" if x is None else f"{x:.{digits}f}"

    print("| edges | solver | threads / ranks | iterations | setup s | Krylov s | solve s | process wall s | core-s | peak RSS GB | n |")
    print("|---|---|---|---|---|---|---|---|---|---|---|")
    for n in sizes:
        e = f"{edges_of[n]:,}".replace(",", " ")
        for label, name in (("jacobi", "COCG + Jacobi"), ("ams", "COCG + AMS"), ("direct", "Direct LU")):
            for t in threads_run:
                k = (n, label, t)
                if k not in S:
                    continue
                s = S[k]
                it = "–" if label == "direct" else " / ".join(str(i) for i in s["iterations"])
                lo, mid, hi = mmm(s["wall"])
                wall = f"{mid:.1f}" if s["reps"] == 1 else f"{mid:.1f} ({lo:.1f} … {hi:.1f})"
                print(
                    f"| {e} | {name} | {t} | {it} | {f(med(k, 'setup'), 2)} | {f(med(k, 'krylov'), 2)} "
                    f"| {f(med(k, 'solve'), 2)} | {wall} | {f(med(k, 'core'))} | {f(max(s['rss']), 2)} | {s['reps']} |"
                )
        for kind, ranks in sorted((k, r) for (k, pn, r) in palace if pn == n):
            runs = palace[(kind, n, ranks)]
            lo, mid, hi = mmm([r["time"]["wall_s"] for r in runs])
            wall = f"{mid:.1f}" if len(runs) == 1 else f"{mid:.1f} ({lo:.1f} … {hi:.1f})"
            core = statistics.median(r["time"]["user_s"] + r["time"]["sys_s"] for r in runs)
            solve = statistics.median(palace_solve_s(r) for r in runs)
            print(
                f"| {e} | {PALACE_KINDS[kind][3]} | {ranks} | {runs[0]['iterations']} | – | – "
                f"| {solve:.2f} | {wall} | {core:.1f} "
                f"| {max(r['cgroup_peak_gb'] for r in runs):.2f} | {len(runs)} |"
            )
    sys.exit(0)

# =============================================================================
p = print
p("# AMS vs Jacobi vs Palace on the #520 driven cube at scale (issue #930).")
p("# GENERATED by benchmarks/gpu_driven_scaling/summarize_ams_r6i.py over")
p(f"# benchmarks/gpu_driven_scaling/runs/{d.name}/ . Do not edit by hand: rerun the")
p("# summarizer. Every number is read from the run tree or computed from it.")
p("#")
p("# One rented box, one geode commit, nothing else running. Each geode cell is")
p("# its own process (ams_cpu_sweep.sh), pinned with taskset; each Palace run is")
p("# its own container pinned with --cpuset-cpus and mpirun -bind-to core.")
p("#")
p("# TIMES. setup_s is prepare_at (assemble A(omega), then build the")
p("# preconditioner or factor), krylov_s is solve (the iteration plus the")
p("# explicit-residual check), solve_s is their sum. process_wall_s, user_s,")
p("# sys_s and max_rss_gb are /usr/bin/time -v of the whole process, which also")
p("# builds the mesh and assembles the operator. core_s = user_s + sys_s.")
p("# Arrays hold one value per repeat, in tree order; *_median etc. are over them.")
p()
p("[meta]")
p("issue = 930")
p(f"measured_date = {q(first_meta['start'][:10])}")
p(f"geode_commit = {q(commit)}")
p(
    'scope = "CPU f64 only, one fixture (the #520 sigma-lossy driven cube, structured Kuhn tets, '
    "first-order edge elements), one host. Assembled COCG with the Jacobi preconditioner against the "
    "same COCG with geode's AMS preconditioner, Direct where it was run, and Palace on the same box. "
    'Nothing here is a matrix-free, GPU or p=2 measurement."'
)
p('fixture = "sigma-lossy parallel-plate cube, single LumpedPort (crates/geode-core/tests/gpu_driven_scaling.rs), identical to results_large_a100.toml"')
p("omega = 0.10")
p("iter_tol = 1e-8")
p(f"sizes_n = {sizes}")
p(f"n_edges = {[edges_of[n] for n in sizes]}")
p(f"threads = {threads_run}")
reps_by_size = {n: sorted({len(v) for (m, _, _), v in cells.items() if m == n}) for n in sizes}
thr_by_size = {n: sorted({t for (m, _, t) in cells if m == n}) for n in sizes}
p(f"reps_per_size = {[max(reps_by_size[n]) for n in sizes]}")
p(f"threads_per_size = {[thr_by_size[n] for n in sizes]}")
p(
    'statistic = "one solve per process (GEODE_SCALING_REPS=0: the warm-up solve is the measurement); '
    'the repeats are separate processes. reps on each cell says how many."'
)
p(f"default_preconditioner_changed = false")
p()
p("[hardware]")
p(f"label = {q(host.get('instance_type', 'unknown') + ', dedicated to this run, otherwise idle')}")
for k in ("kernel", "os", "ami_name", "rustc", "docker", "governor", "thp"):
    if k in host:
        p(f"{k} = {q(host[k])}")
h1 = keyvals(main_trees[0][1] / "host.txt")
p(f"cpu = {q(h1.get('cpu', 'unknown'))}")
p(f"logical_cpus = {h1.get('logical_cpus', 0)}")
p(f"ram_gib = {h1.get('ram_gib', 0)}")
p('pinning = "1 thread: taskset -c 0. 8 threads: taskset -c 0-7, the eight physical cores, one hyperthread each (lscpu -e in host_extra.txt). RAYON_NUM_THREADS set to the same count."')
p(f"max_loadavg_1min_at_leg_start = {max(s['load_start'] for s in S.values()):.2f}")
if session:
    p()
    p("[session]")
    for k, v in session.items():
        p(f"{k} = {q(v)}")

# ---- cells ------------------------------------------------------------------
p()
p("# ---------------------------------------------------------------------------")
p("# geode cells: one per (size, preconditioner, threads).")
p("# ---------------------------------------------------------------------------")
for n in sizes:
    ref_label, ref_v = reference_v(n)
    for label in LABELS:
        for t in threads_run:
            k = (n, label, t)
            if k not in S:
                continue
            s = S[k]
            p()
            p("[[cell]]")
            p(f"size_n = {n}")
            p(f"n_edges = {edges_of[n]}")
            p(f"preconditioner = {q(label)}")
            p(f"threads = {t}")
            p(f"cpus_allowed_list = {q(' / '.join(s['cpus']))}")
            p(f"rayon_num_threads = {q(' / '.join(s['rayon']))}")
            p(f"reps = {s['reps']}")
            p(f"converged = {str(s['converged']).lower()}")
            if label != "direct":
                if len(s["iterations"]) == 1:
                    p(f"iterations = {s['iterations'][0]}")
                else:
                    p(f"iterations_per_rep_distinct = {s['iterations']}")
            p(f"residual_rel_max = {sci(s['residual'], 3)}")
            p(f"setup_s = {fl(s['setup'])}")
            p(f"krylov_s = {fl(s['krylov'])}")
            p(f"solve_s = {fl(s['solve'])}")
            lo, mid, hi = mmm(s["solve"])
            p(f"solve_s_min_median_max = {fl([lo, mid, hi])}")
            if label != "direct":
                p(f"krylov_s_per_iteration = {statistics.median(s['krylov']) / s['iterations'][-1]:.5f}")
                p(f"setup_share_of_solve = {statistics.median(s['setup']) / mid:.3f}")
            p(f"process_wall_s = {fl(s['wall'], 2)}")
            p(f"process_wall_s_min_median_max = {fl(list(mmm(s['wall'])), 2)}")
            p(f"user_s = {fl(s['user'], 2)}")
            p(f"sys_s = {fl(s['sys'], 2)}")
            p(f"core_s_median = {statistics.median(s['core']):.2f}")
            p(f"max_rss_gb = {fl(s['rss'])}")
            p(f"port_v = [{s['port_v'][0]:.9e}, {s['port_v'][1]:.9e}]")
            same = all(v == s["port_v"] for v in s["port_v_all"])
            p(f"port_v_identical_across_reps = {str(same).lower()}")
            if ref_label and label != ref_label:
                p(f"port_v_rel_diff_vs_{ref_label} = {sci(rel_diff(s['port_v'], ref_v))}")
            if (n, label) in xcheck:
                p(f"field_rel_l2_vs_direct = {sci(xcheck[(n, label)])}")
            if s["smoother"]:
                sm = s["smoother"]
                p(f"ams_ritz_theta = {float(sm['theta_max']):.6f}")
                p(f"ams_gershgorin = {float(sm['gershgorin']):.6f}")
                p(f"ams_smoother_weight = {float(sm['weight']):.6f}")
                p(f"ams_weight_source = {q(sm['source'])}")
                p(f"ams_estimate_ms = {fl([float(x['estimate_ms']) for x in s['smoother_all'] if x], 1)}")

for threads, n, label, leg in failures:
    p()
    p("[[failed_leg]]")
    p(f"size_n = {n}")
    p(f"preconditioner = {q(label)}")
    p(f"threads = {threads}")
    p(f"leg = {q(str(leg['tree'].relative_to(d)) + '/' + leg['stem'])}")
    p(f"rc = {leg['rc']}")
    if leg["time"]:
        p(f"process_wall_s = {leg['time']['wall_s']:.2f}")
        p(f"max_rss_gb = {leg['time']['max_rss_bytes'] / 1e9:.3f}")
    if "timeout_s" in leg["meta"]:
        p(f"timeout_s = {leg['meta']['timeout_s']}")

# ---- Palace -----------------------------------------------------------------
if palace:
    any_run = next(iter(palace.values()))[0]
    p()
    p("# ---------------------------------------------------------------------------")
    p("# Palace on the SAME box, same exported meshes, same tolerance (1e-8), the")
    p("# config of palace_driven_cfg.py (Order 1), with two values of")
    p("# Solver.Linear.Type:")
    p("#   Default  what the #520 run used. For a driven problem Palace resolves it")
    p("#            to a sparse direct solver when built with one, SuperLU first")
    p("#            (palace/utils/iodata.cpp at palace_commit), and this image")
    p("#            builds SuperLU. So it is GMRES preconditioned by a SuperLU")
    p("#            factorization, NOT AMS. The Palace log does not print the")
    p("#            resolved type; this is read from the source and the build")
    p("#            flags, and the two types below differ in every measured column.")
    p("#   AMS      Palace's auxiliary-space Maxwell solver (hypre). The")
    p("#            like-for-like cell for geode's AMS.")
    p("# wall_s, user_s, sys_s are /usr/bin/time -v around `palace -np N` inside the")
    p("# container, the whole pipeline. The palace_*_s keys are the Avg. column of")
    p("# Palace's own Elapsed Time Report, whose rows are exclusive (a nested timer")
    p("# pauses its parent; palace/utils/timer.hpp). 'Setup' is the solver's")
    p("# SetOperators, 'Preconditioner' is the time inside preconditioner")
    p("# applications (ApplyB in palace/linalg/iterative.cpp) and 'Linear Solve'")
    p("# is the rest of GMRES. MFEM's hypre and SuperLU wrappers set up or factor")
    p("# on their first application, so the AMS setup and the SuperLU")
    p("# factorization are most likely inside 'Preconditioner' (not checked")
    p("# against the MFEM version Palace pins). palace_linear_solve_total_s, the")
    p("# sum of the three, is the only figure compared with geode's solve_s; the")
    p("# rows are not split into setup and Krylov here.")
    p("# max_rss_one_rank_gb is /usr/bin/time's")
    p("# figure, the largest single process. cgroup_peak_gb is the container's")
    p("# memory.peak: every rank together, page cache included.")
    p("# ---------------------------------------------------------------------------")
    p()
    p("[palace]")
    p("same_host = true")
    p(f"palace_commit = {q(palace_build.get('palace_ref', 'unknown'))}")
    p(f"changeset_in_log = {q(any_run['changeset'])}")
    p(f"libceed_backend = {q(any_run['backend'])}")
    p(f"image = {q('palace:cpu from reference/palace/docker/Dockerfile, --build-arg PALACE_REF=<palace_commit>')}")
    for k in ("dockerfile_sha256", "build_wall_s", "base_image"):
        if k in palace_build:
            p(f"{k} = {q(palace_build[k])}")
    p('pinning = "--cpuset-cpus 0 (1 rank) or 0-7 (8 ranks), mpirun -bind-to core, OMP_NUM_THREADS=1"')
    p('default_linear_type_resolves_to = "SuperLU (GMRES preconditioned by a sparse direct factorization), from palace/utils/iodata.cpp at palace_commit and PALACE_WITH_SUPERLU=ON in the Dockerfile; not printed in the Palace log"')
    for kind, n, ranks in sorted(palace):
        runs = palace[(kind, n, ranks)]
        ref_label, ref_v = reference_v(n) if n in sizes else (None, None)
        p()
        p("[[palace_cell]]")
        p(f"linear_type = {q(PALACE_KINDS[kind][1])}")
        p(f"preconditioner = {q(PALACE_KINDS[kind][2])}")
        p(f"size_n = {n}")
        if n in edges_of:
            p(f"n_edges = {edges_of[n]}")
        p(f"ranks = {ranks}")
        p(f"mpi = {q(runs[0]['mpi'])}")
        p(f"reps = {len(runs)}")
        p(f"krylov_iterations = {sorted({r['iterations'] for r in runs})}")
        wall = [r["time"]["wall_s"] for r in runs]
        p(f"wall_s = {fl(wall, 2)}")
        p(f"wall_s_min_median_max = {fl(list(mmm(wall)), 2)}")
        p(f"user_s = {fl([r['time']['user_s'] for r in runs], 2)}")
        p(f"sys_s = {fl([r['time']['sys_s'] for r in runs], 2)}")
        p(f"core_s_median = {statistics.median(r['time']['user_s'] + r['time']['sys_s'] for r in runs):.2f}")
        p(f"palace_total_s = {fl([r['Total'] for r in runs])}")
        p(f"palace_operator_construction_s = {fl([r['Operator Construction'] for r in runs])}")
        p(f"palace_linear_solve_s = {fl([r['Linear Solve'] for r in runs])}")
        p(f"palace_solver_setup_s = {fl([r['Setup'] for r in runs])}")
        p(f"palace_preconditioner_s = {fl([r['Preconditioner'] for r in runs])}")
        p(f"palace_linear_solve_total_s = {fl([palace_solve_s(r) for r in runs])}")
        p(f"max_rss_one_rank_gb = {fl([r['time']['max_rss_bytes'] / 1e9 for r in runs])}")
        p(f"cgroup_peak_gb = {fl([r['cgroup_peak_gb'] for r in runs])}")
        p(f"palace_reported_total_hwm = {q(runs[0]['hwm'])}")
        p(f"port_v = [{runs[0]['port_v'][0]:.9e}, {runs[0]['port_v'][1]:.9e}]")
        if ref_v:
            p(f"port_v_rel_diff_vs_geode_{ref_label} = {sci(rel_diff(runs[0]['port_v'], ref_v))}")
    for name in palace_failed:
        p()
        p("[[palace_failed_run]]")
        p(f"log = {q(name)}")

# ---- extra frequencies ------------------------------------------------------
if OS:
    p()
    p("# ---------------------------------------------------------------------------")
    p("# Other drive frequencies (single runs), same legs and pinning.")
    p("# ---------------------------------------------------------------------------")
    for (w, n, label, t), s in sorted(OS.items()):
        p()
        p("[[omega_cell]]")
        p(f"omega = {w:.2f}")
        p(f"size_n = {n}")
        p(f"n_edges = {omega[(w, n, label, t)][0]['cell']['n_edges']}")
        p(f"preconditioner = {q(label)}")
        p(f"threads = {t}")
        p(f"reps = {s['reps']}")
        p(f"converged = {str(s['converged']).lower()}")
        if label != "direct":
            p(f"iterations = {s['iterations'][0]}")
        p(f"residual_rel_max = {sci(s['residual'], 3)}")
        p(f"setup_s = {fl(s['setup'])}")
        p(f"krylov_s = {fl(s['krylov'])}")
        p(f"solve_s = {fl(s['solve'])}")
        p(f"core_s_median = {statistics.median(s['core']):.2f}")
        p(f"max_rss_gb = {fl(s['rss'])}")
        p(f"port_v = [{s['port_v'][0]:.9e}, {s['port_v'][1]:.9e}]")
        refs = [(r, tt) for r in ("direct", "jacobi") for tt in (8, 1) if (w, n, r, tt) in OS]
        if refs and refs[0][0] != label:
            r, tt = refs[0]
            p(f"port_v_rel_diff_vs_{r} = {sci(rel_diff(s['port_v'], OS[(w, n, r, tt)]['port_v']))}")
        if s["smoother"]:
            p(f"ams_ritz_theta = {float(s['smoother']['theta_max']):.6f}")
            p(f"ams_smoother_weight = {float(s['smoother']['weight']):.6f}")
    for threads, n, label, leg in omega_failures:
        p()
        p("[[omega_failed_leg]]")
        p(f"leg = {q(str(leg['tree'].relative_to(d)) + '/' + leg['stem'])}")
        p(f"rc = {leg['rc']}")

# ---- comparison -------------------------------------------------------------
p()
p("# ---------------------------------------------------------------------------")
p("# Derived comparison, one table per size. Medians over the repeats.")
p("# jacobi_over_ams ratios above 1 mean AMS is faster. The geode-over-Palace")
p("# ratios compare like with like in core count (geode T threads against Palace")
p("# T ranks) and are whole-process wall clock and core-seconds on both sides;")
p("# above 1 means Palace is faster / cheaper. geode builds the mesh in memory")
p("# and Palace reads and partitions the exported file, and geode's Krylov")
p("# solve is serial at any thread count, so its 8-thread cells use the extra")
p("# cores only in assembly and in the AMS setup.")
p("# ---------------------------------------------------------------------------")
both = [n for n in sizes if its(n, "jacobi") and its(n, "ams")]
for n in both:
    p()
    p("[[comparison]]")
    p(f"size_n = {n}")
    p(f"n_edges = {edges_of[n]}")
    ij, ia = its(n, "jacobi")[-1], its(n, "ams")[-1]
    p(f"iterations_jacobi = {ij}")
    p(f"iterations_ams = {ia}")
    p(f"iterations_jacobi_over_ams = {ij / ia:.1f}")
    for t in threads_run:
        kj, ka = (n, "jacobi", t), (n, "ams", t)
        if kj in S and ka in S:
            sj, sa = statistics.median(S[kj]["solve"]), statistics.median(S[ka]["solve"])
            p(f"solve_s_jacobi_t{t} = {sj:.3f}")
            p(f"solve_s_ams_t{t} = {sa:.3f}")
            p(f"solve_jacobi_over_ams_t{t} = {sj / sa:.2f}")
            cj, ca = statistics.median(S[kj]["core"]), statistics.median(S[ka]["core"])
            p(f"core_s_jacobi_over_ams_t{t} = {cj / ca:.2f}")
            p(f"max_rss_ams_over_jacobi_t{t} = {max(S[ka]['rss']) / max(S[kj]['rss']):.2f}")
    for kind, ranks in sorted((k, r) for (k, pn, r) in palace if pn == n):
        runs = palace[(kind, n, ranks)]
        pw = statistics.median(r["time"]["wall_s"] for r in runs)
        pc = statistics.median(r["time"]["user_s"] + r["time"]["sys_s"] for r in runs)
        tag = f"palace_{kind}_np{ranks}"
        p(f"{tag}_iterations = {runs[0]['iterations']}")
        p(f"{tag}_wall_s = {pw:.2f}")
        p(f"{tag}_linear_solve_total_s = {statistics.median(palace_solve_s(r) for r in runs):.3f}")
        p(f"{tag}_core_s = {pc:.2f}")
        p(f"{tag}_cgroup_peak_gb = {max(r['cgroup_peak_gb'] for r in runs):.2f}")
        for label in ("jacobi", "ams"):
            if (n, label, ranks) not in S:
                continue
            gw = statistics.median(S[(n, label, ranks)]["wall"])
            gc = statistics.median(S[(n, label, ranks)]["core"])
            p(f"geode_{label}_t{ranks}_process_wall_s_over_{tag}_wall_s = {gw / pw:.2f}")
            p(f"geode_{label}_t{ranks}_core_s_over_{tag}_core_s = {gc / pc:.2f}")

if len(both) >= 3:
    e = [edges_of[n] for n in both]
    ij = [its(n, "jacobi")[-1] for n in both]
    ia = [its(n, "ams")[-1] for n in both]
    p()
    p("[scaling]")
    p(f"n_edges = {e}")
    p(f"iterations_jacobi = {ij}")
    p(f"iterations_ams = {ia}")
    p(f"iterations_jacobi_lsq_exponent = {lsq_exponent(e, ij):.3f}")
    p(f"iterations_ams_lsq_exponent = {lsq_exponent(e, ia):.3f}")
    p(f"iterations_ams_last_over_first = {ia[-1] / ia[0]:.2f}")
    p(f"iterations_jacobi_last_over_first = {ij[-1] / ij[0]:.2f}")
    t = threads_run[0]
    if all((n, "ams", t) in S and (n, "jacobi", t) in S for n in both):
        pa = [statistics.median(S[(n, "ams", t)]["krylov"]) / its(n, "ams")[-1] for n in both]
        pj = [statistics.median(S[(n, "jacobi", t)]["krylov"]) / its(n, "jacobi")[-1] for n in both]
        sa = [statistics.median(S[(n, "ams", t)]["setup"]) for n in both]
        p(f"threads_for_the_fits_below = {t}")
        p(f"ams_krylov_s_per_iteration = {fl(pa, 5)}")
        p(f"jacobi_krylov_s_per_iteration = {fl(pj, 5)}")
        p(f"ams_over_jacobi_cost_per_iteration = {fl([a / j for a, j in zip(pa, pj)], 1)}")
        p(f"ams_krylov_s_per_iteration_lsq_exponent = {lsq_exponent(e, pa):.3f}")
        p(f"jacobi_krylov_s_per_iteration_lsq_exponent = {lsq_exponent(e, pj):.3f}")
        p(f"ams_setup_s = {fl(sa)}")
        p(f"ams_setup_s_lsq_exponent = {lsq_exponent(e, sa):.3f}")
    for kind in PALACE_KINDS:
        pi = [palace[(kind, n, r)][0]["iterations"] for n in both for r in (8,) if (kind, n, r) in palace]
        if pi:
            p(f"palace_{kind}_np8_iterations = {pi}")
