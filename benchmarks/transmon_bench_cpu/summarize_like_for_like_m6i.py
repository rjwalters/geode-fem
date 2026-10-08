#!/usr/bin/env python3
"""Summarize an m6i_box_bench.sh run tree into a results TOML (stdout).

usage: summarize_like_for_like_m6i.py <run-dir> <palace-eig.csv>
  e.g. python3 -I benchmarks/transmon_bench_cpu/summarize_like_for_like_m6i.py \
         benchmarks/transmon_bench_cpu/runs/2026-10-08_m6i_like_for_like \
         reference/fixtures/transmon_palace/results_p1/eig.csv \
         > benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml

Reads <run-dir>/cells.tsv, pinning.txt, launch.txt, provenance/*.txt, configs/ and
raw/<cell>_run<i>.{log,time,meta} (+ _eig.csv for Palace cells), and the
committed Palace eigenvalues. For every cell it lists the modes the solver
returned, classifies each one, counts the physical ones, and reports wall
clock, core-seconds (user + sys) and peak RSS per run with min / median / max.
Pure stdlib; no fitting, no outlier rejection, every run of every cell is
reported.

The classification rule, its thresholds and the /usr/bin/time parser are the
ones of summarize_like_for_like.py (loaded from the file next to this one), so
the dev-machine record and this one cannot drift apart.
"""
import csv
import importlib.util
import json
import pathlib
import re
import statistics
import sys

sys.dont_write_bytecode = True  # do not drop a __pycache__ next to the scripts
_here = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("summarize_like_for_like", _here / "summarize_like_for_like.py")
rule = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rule)

# The 2026-07-14 m6i cells of results.toml [matched.physical_target] that this
# run re-measures (issue #763). Constants, copied from that file.
HIST_GEODE_1THREAD_WALL_S = 28.7
HIST_GEODE_8THREAD_WALL_S = 29.0
HIST_PALACE_NP1_WALL_S = 130.9
HIST_PALACE_NP8_WALL_S = 44.5
# results.toml [matched.off_target] (12 modes, 20 GHz), same session.
HIST_OFF_GEODE_1THREAD_WALL_S = 36.8
HIST_OFF_GEODE_8THREAD_WALL_S = 26.6
HIST_OFF_PALACE_NP1_WALL_S = 248.0
HIST_OFF_PALACE_NP8_WALL_S = 64.7

ROUTES = (
    # (key, geode cell stem, same-request Palace cell, what, caveat)
    (
        "historical_request",
        "geode_s4p5_n6",
        "palace_s4p5_n6_np8",
        "6 modes at 4.5 GHz, ungauged: the request every committed geode-vs-Palace transmon time used",
        "Not a route to the six modes. Kept for continuity with results.toml [matched.physical_target] and for #763.",
    ),
    (
        "more_modes",
        "geode_s4p5_n30",
        "palace_s4p5_n30_np8",
        "30 modes at 4.5 GHz, ungauged; the six physical modes are picked out of the 30 with the Palace oracle",
        "ORACLE-TUNED. 30 was found by trial against the Palace eigenvalues (29 falls short on the dev machine: results_like_for_like_local.toml). It belongs to this mesh, Krylov size and start vector. Nothing in the solver output says when the request is large enough, and the caller still needs the oracle to tell the six from the other 24.",
    ),
    (
        "shift",
        "geode_s20_n6",
        "palace_s20_n6_np8",
        "6 modes at 20 GHz, ungauged: a different request from Palace's (N = 6, Target = 4.5 GHz)",
        "ORACLE-TUNED. 20 GHz was chosen knowing where the modes are (it must exceed top mode / sqrt(2) = 18.44 GHz for all six to be nearer the shift than the gradient kernel). It is not a recipe for a device whose spectrum is unknown.",
    ),
    (
        "port_aware",
        "geode_pa_s4p5_n8",
        "palace_s4p5_n8_np8",
        "8 modes at 4.5 GHz with GEODE_GAUGE=port_aware (#514 port-aware divergence-free projection)",
        "The only route that uses no reference-solver number to choose the request. It still returns two non-physical modes ahead of the top physical ones (a near-zero survivor and the 3.45 GHz port mode), so 8 are requested to get 6, and a caller needs a rule to drop the two; here that rule is the Palace oracle (#950).",
    ),
)

run_dir = pathlib.Path(sys.argv[1])
oracle_csv = pathlib.Path(sys.argv[2])
raw = run_dir / "raw"
prov = run_dir / "provenance"

oracle = rule.read_oracle(oracle_csv)
host = rule.kv(prov / "host.txt")
gver = rule.kv(prov / "geode-version.txt")
pal = rule.kv(prov / "palace-image-provenance.txt")
pal_img = (prov / "palace-image-id.txt").read_text().strip()
pinning = rule.kv(run_dir / "pinning.txt")
launch = rule.kv(run_dir / "launch.txt")

PHASE = re.compile(r"^( *)([A-Z][A-Za-z.\- ]*?) {2,}([0-9.]+) +([0-9.]+) +([0-9.]+)\s*$")


def palace_phases(log):
    """Avg. column of Palace's 'Elapsed Time Report (s)': {row name: seconds}.

    Indented rows are sub-phases Palace lists under the row above; they are
    keyed 'Parent / Child'. Palace's Total is the sum of every row.
    """
    out = {}
    block = log.split("Elapsed Time Report (s)", 1)
    if len(block) < 2:
        return out
    parent = None
    for line in block[1].splitlines()[2:]:
        if line.startswith("---"):
            continue
        m = PHASE.match(line)
        if not m:
            if out:
                break
            continue
        name = m.group(2).strip()
        if m.group(1):
            name = f"{parent} / {name}"
        else:
            parent = name
        out[name] = float(m.group(5))
        if name == "Total":
            break
    return out


def palace_modes(path):
    with open(path) as f:
        rows = list(csv.reader(f))
    return [float(r[1]) for r in rows[1:] if len(r) >= 2]


def in_common(a, b):
    """Number of modes of list a that have a mode of list b within PHYSICAL_TOL_PCT."""
    return sum(1 for x in a if any(abs(x - y) / x * 100.0 <= rule.PHYSICAL_TOL_PCT for y in b if x > 0))


def flag(mpicxx_show, prefix):
    return " ".join(t for t in mpicxx_show.split() if t.startswith(prefix)) or "none"


def fl(xs, nd):
    return "[" + ", ".join(f"{x:.{nd}f}" for x in xs) + "]"


def mmm(xs):
    return min(xs), statistics.median(xs), max(xs)


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


cells = []
for line in (run_dir / "cells.tsv").read_text().splitlines():
    if not line.strip():
        continue
    name, solver, gauge, sigma, nmodes, width, pin, runs = line.split("\t")
    c = {
        "name": name,
        "solver": solver,
        "gauge": gauge,
        "sigma": float(sigma),
        "n": int(nmodes),
        "width": int(width),
        "pin": pin,
        "runs": [],
    }
    for i in range(1, int(runs) + 1):
        stem = raw / f"{name}_run{i}"
        meta = rule.kv(stem.with_suffix(".meta"))
        log = stem.with_suffix(".log").read_text(errors="replace")
        wall, user, sys_s, rss = rule.read_time(stem.with_suffix(".time"))
        r = {"meta": meta, "wall": wall, "user": user, "sys": sys_s, "rss": rss, "exit": int(meta.get("exit", "-1"))}
        if solver == "geode":
            r["freqs"] = [float(m.group(2)) for m in rule.MODE.finditer(log)]
            r["part"] = [float(m.group(3)) for m in rule.MODE.finditer(log)]
            s = re.search(r"^\s*solve \(.*\) : ([0-9.]+) s", log, re.M)
            a = re.search(r"^\s*setup\+assembly : ([0-9.]+) s", log, re.M)
            r["solve"] = float(s.group(1)) if s else float("nan")
            r["assembly"] = float(a.group(1)) if a else float("nan")
        else:
            r["freqs"] = palace_modes(raw / f"{name}_run{i}_eig.csv")
            r["cfg"] = json.loads((run_dir / "configs" / f"cfg_{name}_run{i}.json").read_text())["Solver"]["Eigenmode"]
            r["phases"] = palace_phases(log)
            mem = re.search(r"Estimated peak per-rank memory usage is: .*Total ([0-9.]+[KMG])", log)
            r["mem_total"] = mem.group(1) if mem else "unknown"
            r["ceed"] = (re.search(r"libCEED backend: (\S+)", log) or [None, "unknown"])[1]
            r["changeset"] = (re.search(r"Git changeset ID: (\S+)", log) or [None, "unknown"])[1]
            r["mpi"] = (re.search(r"Running with (\d+) MPI process", log) or [None, "0"])[1]
            r["omp"] = (re.search(r"MPI process(?:es)?, (\d+) OpenMP thread", log) or [None, "1"])[1]
        c["runs"].append(r)
    r0 = c["runs"][0]
    c["freqs"] = r0["freqs"]
    c["classes"], c["matches"] = rule.classify(c["freqs"], oracle)
    c["n_physical"] = sum(1 for k in c["classes"] if k == "physical")
    c["six"] = all(g is not None for _, g, _ in c["matches"]) and len(oracle) == 6
    c["all_identical"] = all([round(f, 6) for f in r["freqs"]] == [round(f, 6) for f in c["freqs"]] for r in c["runs"])
    c["per_run_n_physical"] = [sum(1 for k in rule.classify(r["freqs"], oracle)[0] if k == "physical") for r in c["runs"]]
    c["walls"] = [r["wall"] for r in c["runs"]]
    c["cores"] = [r["user"] + r["sys"] for r in c["runs"]]
    cells.append(c)

by = {c["name"]: c for c in cells}
geode_rss_max = max(r["rss"] for c in cells if c["solver"] == "geode" for r in c["runs"]) / 1e9
stamps = sorted(
    s for c in cells for r in c["runs"] for s in (r["meta"].get("start_utc", ""), r["meta"].get("end_utc", "")) if s
)
loads = [float(r["meta"]["loadavg_before"].split()[0]) for c in cells for r in c["runs"] if "loadavg_before" in r["meta"]]
changesets = sorted({r["changeset"] for c in cells if c["solver"] == "palace" for r in c["runs"]})
ceeds = sorted({r["ceed"] for c in cells if c["solver"] == "palace" for r in c["runs"]})

out = []
w = out.append
w("# Same-box like-for-like transmon eigenmode timing: geode's three routes to")
w("# Palace's six physical modes, and Palace, on one idle AWS m6i.4xlarge in one")
w("# session (issues #927, #763). GENERATED by")
w("# benchmarks/transmon_bench_cpu/summarize_like_for_like_m6i.py from")
w(f"# {run_dir.as_posix()}/ (do not edit by hand; rerun the script).")
w("#")
w("# ---------------------------------------------------------------------------")
w("# READ FIRST")
w("#   * NO WINNER IS DECLARED HERE. Three geode routes return Palace's six modes;")
w("#     each carries a caveat ([route.*].caveat) and they are reported side by")
w("#     side. Two of the three were tuned against the Palace answer. Which one, if")
w("#     any, a publication may set beside Palace is an operator decision (#927).")
w("#   * A geode cell is like_for_like ONLY in the sense that the six Palace modes")
w("#     are among the modes it returned (each within physical_tol_pct). A route")
w("#     may return non-physical modes beside them ([route.*].non_physical_modes_")
w("#     returned); the six are then picked out with the Palace oracle. Palace's")
w("#     fixture request returns the six and nothing else.")
w("#   * PALACE'S FIXTURE CONFIG WRITES SIX PARAVIEW FIELDS (Save = 6) AND GEODE")
w("#     WRITES NONE. The *_nosave Palace cells (Save = 0) remove that output; both")
w("#     are reported, and every [route.*] gives ratios against both.")
w("#   * PALACE'S TARGET IS A LOWER BOUND, GEODE'S SHIFT IS A CENTRE. For a request")
w("#     at 20 GHz the two solvers return different mode sets ([off_target_request]).")
w("#   * WALL CLOCK AND CORE-SECONDS ARE BOTH GIVEN. core_s = user + sys CPU time")
w("#     of the whole process tree from /usr/bin/time. MPICH ranks busy-wait, so a")
w("#     Palace core_s is close to ranks x wall by construction.")
w("#   * '1 thread' is enforced by the OS (taskset to one CPU), not only requested:")
w("#     see observed_cpus_allowed / observed_tasks_max per cell and [pinning].")
w("#   * ONE MESH (133,108 interior DOFs, first order), ONE HOST, ONE SESSION, and")
w("#     geode's direct LU against Palace's default iterative solver. Nothing here")
w("#     extrapolates to another mesh size; results.toml records that geode's direct")
w("#     path runs out of memory near 1.16M DOFs on this instance type.")
w("# ---------------------------------------------------------------------------")
w("")
w("[meta]")
w("issues = [927, 763]")
w(f'measured_date = "{stamps[0][:10]}"  # UTC; timed runs between {stamps[0]} and {stamps[-1]}')
w(f'geode_commit = "{gver.get("geode_commit", "unknown")}"')
w('scope = "one AWS m6i.4xlarge with no other workload, one session; geode (direct LU) and Palace (iterative) on the committed 133k transmon fixture; not a scaling study"')
w('fixture = "crates/geode-core/tests/fixtures/transmon_smoke.msh"')
w(f'fixture_sha256 = "{host.get("fixture_sha256", "unknown")}"')
w("n_interior_dofs = 133108")
w('geode_binary = "cargo build --release -p geode-core --example transmon_bench; GEODE_INNER=direct (faer COLAMD sparse LU shift-invert Lanczos, f64)"')
w('palace_binary = "palace:cpu image built on the box from reference/palace/docker/Dockerfile; config = reference/fixtures/transmon_palace/palace_config.json with only Model.Mesh, Problem.Output and, for the non-fixture requests, Solver.Eigenmode.N / Target changed (Save = 6, Tol = 1e-8, Order 1 kept)"')
w('timing = "/usr/bin/time -v over the full solver process (mesh load + assembly + solve + output). For Palace it wraps `palace -np N` inside the container, so container start-up is excluded"')
w('core_seconds = "user + sys CPU seconds from the same /usr/bin/time: summed over all threads (geode) or over mpirun and all ranks (Palace)"')
w('memory = "peak resident set size from /usr/bin/time. geode: the one process. Palace: the LARGEST SINGLE process, not the sum over ranks (Palace\'s own total is in each raw log)"')
w('statistic = "every run is listed; min, median and max over the runs of a cell. No mean, no outlier rejection"')
w('order = "runs are sequential, never two at once, round-robin over cells (repeat 1 of every cell, then repeat 2, ...)"')
w('generator = "benchmarks/transmon_bench_cpu/m6i_box_setup.sh and m6i_box_bench.sh on the box, then summarize_like_for_like_m6i.py"')
w("")
w("[hardware]")
w(f'instance = "{host.get("instance_type", "unknown")}"')
w(f'region = "{host.get("region", "unknown")}"')
w(f'cpu = "{host.get("cpu", "unknown")}"')
w(f'os = "{host.get("os", "unknown")}"')
w(f'ami_name = "{launch.get("ami_name", "unknown")}"')
w(f'kernel = "{host.get("kernel", "unknown")}"')
w(f'logical_cpus = {host.get("logical_cpus", "0")}')
w(f'physical_cores = {host.get("physical_cores", "0")}')
w(f'ram_gib = {host.get("ram_gib", "0")}')
w(f"loadavg_1min_before_range = [{min(loads):.2f}, {max(loads):.2f}]  # over all run starts; see loadavg_note")
w('loadavg_note = "Each run starts 5 s after the previous one ends, so the 1-minute load average before a run is the tail of the previous timed run (highest after an 8-rank Palace run), not a second workload. cpu_percent = 100 on every pinned one-thread run is the evidence that the pinned CPU was not shared."')
w(f'purchase = "{launch.get("purchase", "unknown")}"')
w(f'session_utc = "{launch.get("launch_utc", "unknown")} to {launch.get("terminate_utc", "unknown")}"  # instance launch to termination')
w('label = "AWS m6i.4xlarge, on-demand (shared tenancy), launched for this run and used for nothing else"')
w("")
w("[software]")
w(f'rustc = "{gver.get("rustc", "unknown")}"')
w(f'cargo = "{gver.get("cargo", "unknown")}"')
w(f'palace_commit = "{pal.get("palace_commit", "unknown").split()[0]}"')
w(f'palace_changeset_in_logs = {"[" + ", ".join(q(x) for x in changesets) + "]"}  # "Git changeset ID" printed by every Palace run')
w(f'palace_image_id = "{rule.kv_tokens(pal_img).get("image_id", "unknown")}"  # local build on the box; not pushed to a registry, so there is no registry digest')
w(f'palace_image_created = "{rule.kv_tokens(pal_img).get("created", "unknown")}"')
w(f'docker = "{host.get("docker", "unknown")}"')
w('palace_threads = "OMP_NUM_THREADS=1; one MPI rank per physical core"')
w(f'palace_libceed_backend_in_logs = {"[" + ", ".join(q(x) for x in ceeds) + "]"}')
w(f'palace_with_libxsmm = "{pal.get("PALACE_WITH_LIBXSMM:BOOL", "unknown")}"  # the committed baseline log (reference/fixtures/transmon_palace/results_p1/palace_run_v22.log) shows /cpu/self/xsmm/blocked: that Palace was built WITH libxsmm, this image without')
w(f'palace_build_type = "{pal.get("CMAKE_BUILD_TYPE:STRING", "unknown")}"')
w(f'palace_cmake_cxx_flags = "{pal.get("CMAKE_CXX_FLAGS:STRING", "unknown")}"')
w(f'palace_mpi_wrapper_arch_flags = "{flag(pal.get("mpicxx_show", ""), "-march")} {flag(pal.get("mpicxx_show", ""), "-mtune")} {flag(pal.get("mpicxx_show", ""), "-flto")} {flag(pal.get("mpicxx_show", ""), "-O")}"  # Rocky 9 MPICH wrapper flags, applied to every Palace target; full line in provenance/palace-image-provenance.txt')
w('geode_rustflags = "no RUSTFLAGS set: cargo --release with the default x86-64 target CPU of rustc"')
w('build_tuning_note = "Neither solver was compiled with -march=native. Palace was built exactly as reference/palace/docker/Dockerfile builds it, which is without libxsmm; a build tuned for this CPU, with libxsmm, or with a different BLAS could be faster. That was not tried, so these Palace times are an upper bound on what a tuned Palace needs, not a best case."')
w("")
w("[pinning]")
w("# Issue #763: thread and core counts are OS limits here, and the evidence is recorded.")
w(f'one_cpu_per_physical_core = "{pinning.get("one_cpu_per_physical_core", "")}"  # lowest-numbered CPU of each core, from lscpu')
w(f'cpuset_1 = "{pinning.get("cpuset_1", "")}"')
w(f'cpuset_8 = "{pinning.get("cpuset_8", "")}"  # one hardware thread on each of the 8 physical cores')
w(f'smt_siblings_of_cpuset_8 = "{pinning.get("smt_siblings_of_cpuset_8", "")}"  # each core\'s two hardware threads; the second of each pair was left idle')
w(f'taskset_check_1 = "{pinning.get("taskset_check_1", "")}"  # Cpus_allowed_list from /proc/self/status under taskset')
w(f'taskset_check_8 = "{pinning.get("taskset_check_8", "")}"')
w(f'palace_np1_rank_masks = "{pinning.get("palace_np1_rank_masks", "")}"  # Cpus_allowed_list of each rank under --cpuset-cpus + mpirun -bind-to core')
w(f'palace_np8_rank_masks = "{pinning.get("palace_np8_rank_masks", "")}"')
w('geode_1_thread = "GEODE_NUM_THREADS=1 RAYON_NUM_THREADS=1 taskset -c <cpuset_1>"')
w('geode_8_threads = "GEODE_NUM_THREADS=8 RAYON_NUM_THREADS=8 taskset -c <cpuset_8>"')
w('geode_knob_only = "GEODE_NUM_THREADS=1, RAYON_NUM_THREADS unset, no taskset: how the 2026-07-14 1-thread row was requested"')
w('palace = "docker run --cpuset-cpus <cpuset_1 or cpuset_8>; palace -np N -launcher-args \'-bind-to core\'"')
w('per_run_evidence = "observed_cpus_allowed = union of Cpus_allowed_list over every task of the live solver process(es), sampled from /proc every 2 s; observed_tasks_max = largest task count seen"')
w("")
w("[oracle]")
w(f'source = "{oracle_csv.as_posix()} (Palace fba6a5b, Order 1, same mesh; the 6 modes Palace returns for N = 6, Target = 4.5 GHz)"')
w(f"palace_f_ghz = {fl(oracle, 6)}")
w(f"physical_tol_pct = {rule.PHYSICAL_TOL_PCT}  # a returned mode is physical when it is the nearest returned mode to a Palace mode and within this")
w(f"near_kernel_below_ghz = {rule.NEAR_KERNEL_GHZ}")
w(f"spurious_port_f_ghz = {rule.SPURIOUS_GHZ}  # ../transmon_eigen/results.toml [spurious_mode], #514")
w('rule = "frequency match against the Palace oracle, as benchmarks/transmon_eigen and summarize_like_for_like.py do; participation cannot identify physical modes (the 3.45 GHz mode has p = 0.994)"')
w('unclassified_note = "unclassified = not within tolerance of one of the six oracle modes, not near zero, not the 3.45 GHz mode. For a Palace cell that asked for more than six modes these are Palace\'s further modes, for which no second solver is committed"')
w("")

for c in cells:
    rs = c["runs"]
    counts = {k: c["classes"].count(k) for k in ("physical", "near_kernel", "spurious_port", "unclassified")}
    wmin, wmed, wmax = mmm(c["walls"])
    cmin, cmed, cmax = mmm(c["cores"])
    w("[[cell]]")
    w(f'name = "{c["name"]}"')
    w(f'solver = "{c["solver"]}"')
    w(f'gauge = "{c["gauge"]}"' + ("  # Palace's divergence-free projection" if c["solver"] == "palace" else ""))
    w(f"sigma_ghz = {c['sigma']}")
    w(f"n_modes_requested = {c['n']}")
    w(f"{'threads' if c['solver'] == 'geode' else 'mpi_ranks'} = {c['width']}")
    w(f'pin = "{rs[0]["meta"].get("pin", "")}"')
    w(f'env = "{rs[0]["meta"].get("env", "")}"')
    w("observed_cpus_allowed = [" + ", ".join(q(r["meta"].get("observed_cpus_allowed", "")) for r in rs) + "]")
    w("observed_tasks_max = [" + ", ".join(r["meta"].get("observed_tasks_max", "0") for r in rs) + "]")
    w("observed_processes_max = [" + ", ".join(r["meta"].get("observed_processes_max", "0") for r in rs) + "]")
    w(f"n_runs = {len(rs)}")
    w(f"exit_codes = [{', '.join(str(r['exit']) for r in rs)}]")
    w(f"modes_f_ghz = {fl(c['freqs'], 6)}  # run 1")
    if c["solver"] == "geode":
        w(f"modes_participation = {fl(rs[0]['part'], 4)}")
    w("modes_class = [" + ", ".join(f'"{k}"' for k in c["classes"]) + "]")
    w(f"n_physical = {c['n_physical']}  # of Palace's {len(oracle)}")
    w(f"n_physical_per_run = [{', '.join(str(n) for n in c['per_run_n_physical'])}]")
    w(f"n_near_kernel = {counts['near_kernel']}")
    w(f"n_spurious_port = {counts['spurious_port']}")
    w(f"n_unclassified = {counts['unclassified']}")
    w(f"six_physical_recovered = {'true' if c['six'] else 'false'}")
    w(f"like_for_like = {'true' if c['six'] else 'false'}  # the six Palace modes are all among the returned modes")
    matched = [(p, g, e) for p, g, e in c["matches"] if g is not None]
    if matched:
        w("physical_matches = [")
        for p, g, e in matched:
            w(f"  {{ palace_f_ghz = {p:.6f}, returned_f_ghz = {g:.6f}, rel_err_pct = {e:.4f} }},")
        w("]")
        w(f"physical_max_rel_err_pct = {max(e for _, _, e in matched):.4f}")
    w(f"modes_identical_across_runs = {'true' if c['all_identical'] else 'false'}")
    w(f"wall_s = {fl(c['walls'], 2)}")
    w(f"wall_s_min = {wmin:.2f}")
    w(f"wall_s_median = {wmed:.2f}")
    w(f"wall_s_max = {wmax:.2f}")
    w(f"user_s = {fl([r['user'] for r in rs], 2)}")
    w(f"sys_s = {fl([r['sys'] for r in rs], 2)}")
    w(f"core_s = {fl(c['cores'], 2)}  # user + sys")
    w(f"core_s_min = {cmin:.2f}")
    w(f"core_s_median = {cmed:.2f}")
    w(f"core_s_max = {cmax:.2f}")
    w("cpu_percent = [" + ", ".join(f"{(r['user'] + r['sys']) / r['wall'] * 100:.0f}" for r in rs) + "]  # core_s / wall_s")
    w("peak_rss_bytes = [" + ", ".join(str(r["rss"]) for r in rs) + "]")
    w(f"peak_rss_gb = {max(r['rss'] for r in rs) / 1e9:.2f}  # max over runs, decimal GB" + ("; largest single rank" if c["solver"] == "palace" else ""))
    w("loadavg_1min_before = [" + ", ".join(r["meta"].get("loadavg_before", "nan").split()[0] for r in rs) + "]")
    if c["solver"] == "geode":
        w(f"assembly_s = {fl([r['assembly'] for r in rs], 2)}  # setup+assembly phase printed by the binary")
        w(f"solve_s = {fl([r['solve'] for r in rs], 2)}  # solve phase printed by the binary")
        w(f"solve_s_median = {statistics.median(r['solve'] for r in rs):.2f}")
    else:
        w("mpi_processes_in_log = [" + ", ".join(r["mpi"] for r in rs) + "]")
        w("openmp_threads_in_log = [" + ", ".join(r["omp"] for r in rs) + "]")
        w(f"eigenmode_save = {rs[0]['cfg']['Save']}  # Solver.Eigenmode.Save in the run's config: modes written as ParaView fields (the fixture config has 6)")
        w(f"n_modes_returned = {len(c['freqs'])}  # Palace may return more than N")
        w("host_wall_s = [" + ", ".join(r["meta"].get("host_wall_s", "nan") for r in rs) + "]  # including docker run start-up and teardown")
        w("palace_estimated_peak_memory_all_ranks = [" + ", ".join(q(r["mem_total"]) for r in rs) + "]  # 'Total' of Palace's own 'Estimated peak per-rank memory usage' line: the sum over ranks")
        w(f"palace_total_s = {fl([r['phases'].get('Total', float('nan')) for r in rs], 2)}  # 'Total' of Palace's Elapsed Time Report (Avg. over ranks)")
        w("")
        w("[cell.palace_phase_s]")
        w("# Avg.-over-ranks column of Palace's own Elapsed Time Report, one entry per run.")
        for ph in rs[0]["phases"]:
            w(f"{q(ph)} = {fl([r['phases'].get(ph, float('nan')) for r in rs], 2)}")
    w("")


def med(name, key):
    return statistics.median(by[name][key])


w("# ---------------------------------------------------------------------------")
w("# SIDE BY SIDE. Every number below is computed from the [[cell]] tables above.")
w("# Ratios are Palace / geode of the two medians named; above 1 means the geode")
w("# cell took less. They are given so nobody has to recompute them, not as a")
w("# claim: read each route's caveat, and note what each comparison does not show.")
w("# ---------------------------------------------------------------------------")
for key, stem, pal_same, what, caveat in ROUTES:
    w(f"[route.{key}]")
    w(f"what = {q(what)}")
    w(f"caveat = {q(caveat)}")
    g1, g8 = by[f"{stem}_t1"], by[f"{stem}_t8"]
    w(f'geode_cells = ["{g1["name"]}", "{g8["name"]}"]')
    w(f"n_modes_requested = {g1['n']}")
    w(f"n_physical_returned = {g1['n_physical']}  # of {len(oracle)}; 8-thread cell: {g8['n_physical']}")
    w(f"non_physical_modes_returned = {g1['n'] - g1['n_physical']}")
    w(f"like_for_like = {'true' if g1['six'] and g8['six'] else 'false'}")
    if g1["six"]:
        w(f"physical_max_rel_err_pct = {max(e for _, g, e in g1['matches'] if g is not None):.4f}")
    for tag, g in (("1_thread", g1), ("8_threads", g8)):
        w(f"geode_{tag}_wall_s_min_median_max = {fl(mmm(g['walls']), 2)}")
        w(f"geode_{tag}_core_s_min_median_max = {fl(mmm(g['cores']), 2)}")
        w(f"geode_{tag}_peak_rss_gb = {max(r['rss'] for r in g['runs']) / 1e9:.2f}")
    # Against Palace's fixture request (N = 6, Target = 4.5 GHz), same width.
    for tag, g, p in (("1_thread_vs_palace_np1", g1, by["palace_s4p5_n6_np1"]), ("8_threads_vs_palace_np8", g8, by["palace_s4p5_n6_np8"])):
        w(f"palace_fixture_over_geode_{tag}_wall = {statistics.median(p['walls']) / statistics.median(g['walls']):.2f}")
        w(f"palace_fixture_over_geode_{tag}_core_s = {statistics.median(p['cores']) / statistics.median(g['cores']):.2f}")
    p8 = by["palace_s4p5_n6_np8"]
    w(f"palace_fixture_np8_over_geode_1_thread_wall = {statistics.median(p8['walls']) / statistics.median(g1['walls']):.2f}  # the form of the old '1 core vs 8 ranks' reading")
    w(f"palace_fixture_np8_over_geode_1_thread_core_s = {statistics.median(p8['cores']) / statistics.median(g1['cores']):.2f}")
    ps = by.get(pal_same)
    if ps is not None:
        w(f'palace_same_request_cell = "{ps["name"]}"  # Palace given this route\'s N and Target, 8 ranks, n = {len(ps["runs"])}')
        w(f"palace_same_request_n_of_six = {ps['n_physical']}  # how many of the six oracle modes Palace returns for that request")
        w(f"palace_same_request_wall_s = {fl(ps['walls'], 2)}")
        w(f"palace_same_request_core_s = {fl(ps['cores'], 2)}")
        w(f"palace_same_request_over_geode_8_threads_wall = {statistics.median(ps['walls']) / statistics.median(g8['walls']):.2f}")
        w(f"palace_same_request_modes_in_common_with_geode = {in_common(ps['freqs'], g8['freqs'])}  # of {len(ps['freqs'])} Palace modes; geode returned {len(g8['freqs'])}")
    pn = by.get("palace_s4p5_n6_np8_nosave")
    if pn is not None:
        w(f"palace_fixture_nosave_np8_over_geode_8_threads_wall = {statistics.median(pn['walls']) / statistics.median(g8['walls']):.2f}  # Palace without ParaView output; geode writes no fields either")
        w(f"palace_fixture_nosave_np8_over_geode_1_thread_wall = {statistics.median(pn['walls']) / statistics.median(g1['walls']):.2f}")
        w(f"palace_fixture_nosave_np8_over_geode_1_thread_core_s = {statistics.median(pn['cores']) / statistics.median(g1['cores']):.2f}")
        w(f"palace_fixture_nosave_np8_over_geode_8_threads_core_s = {statistics.median(pn['cores']) / statistics.median(g8['cores']):.2f}")
    w("")

w("[palace_fixture]")
w('what = "Palace on the fixture\'s own request (N = 6, Target = 4.5 GHz): returns the six physical modes and nothing else"')
w('target_semantics = "Palace\'s Eigenmode.Target is a LOWER BOUND: it returns the N modes above the target. geode\'s shift returns the modes NEAREST the shift on either side. The two agree on a mode set only when the request is built for it; see [off_target_request] and [route.shift]."')
w('save_note = "The fixture config writes six ParaView mode fields (Save = 6); the *_nosave cells set Save = 0. geode\'s transmon_bench writes no field output in any cell, so the Save = 6 cells charge Palace for output geode does not produce and the Save = 0 cells do not."')
for tag, name in (
    ("np1", "palace_s4p5_n6_np1"),
    ("np8", "palace_s4p5_n6_np8"),
    ("np1_nosave", "palace_s4p5_n6_np1_nosave"),
    ("np8_nosave", "palace_s4p5_n6_np8_nosave"),
):
    if name not in by:
        continue
    p = by[name]
    w(f"{tag}_n_runs = {len(p['runs'])}")
    w(f"{tag}_paraview_s = {fl([r['phases'].get('Postprocessing / Paraview', 0.0) for r in p['runs']], 2)}  # Palace's own timer for the field output")
    w(f"{tag}_n_physical = {p['n_physical']}")
    w(f"{tag}_wall_s_min_median_max = {fl(mmm(p['walls']), 2)}")
    w(f"{tag}_core_s_min_median_max = {fl(mmm(p['cores']), 2)}")
    w(f"{tag}_palace_total_s = {fl([r['phases'].get('Total', float('nan')) for r in p['runs']], 2)}")
    w(f"{tag}_peak_rss_gb_largest_rank = {max(r['rss'] for r in p['runs']) / 1e9:.2f}")
    w(f"{tag}_palace_estimated_peak_memory_all_ranks = [" + ", ".join(q(r["mem_total"]) for r in p["runs"]) + f"]  # Palace's own figure, summed over ranks; geode's one process peaks at {geode_rss_max:.2f} GB in this run")
    w(f"{tag}_max_rel_dev_from_committed_eig_csv = {max(abs(a - b) / b for r in p['runs'] for a, b in zip(r['freqs'], oracle)):.1e}")
w("")

# ---- issue #763 ---------------------------------------------------------------
t1, t8, kn = by["geode_s4p5_n6_t1"], by["geode_s4p5_n6_t8"], by["geode_s4p5_n6_t1knob"]
w("[issue_763]")
w('question = "Was the 2026-07-14 \'1-thread\' geode row (28.7 s; 8 threads 29.0 s; results.toml [matched.physical_target]) a single-threaded measurement?"')
w('request = "6 modes at 4.5 GHz, ungauged, direct: the same request as those rows, re-run here at the current commit"')
w(f"historical_geode_1thread_wall_s = {HIST_GEODE_1THREAD_WALL_S}  # geode 3174015, us-west-2 m6i.4xlarge, GEODE_NUM_THREADS=1, no pinning recorded")
w(f"historical_geode_8thread_wall_s = {HIST_GEODE_8THREAD_WALL_S}")
w(f"historical_palace_np1_wall_s = {HIST_PALACE_NP1_WALL_S}")
w(f"historical_palace_np8_wall_s = {HIST_PALACE_NP8_WALL_S}")
for tag, c in (("pinned_1_thread", t1), ("pinned_8_threads", t8), ("knob_only_1_thread", kn)):
    w(f"{tag}_wall_s_min_median_max = {fl(mmm(c['walls']), 2)}")
    w(f"{tag}_core_s_min_median_max = {fl(mmm(c['cores']), 2)}")
    w(f"{tag}_cpu_percent = [" + ", ".join(f"{(r['user'] + r['sys']) / r['wall'] * 100:.0f}" for r in c["runs"]) + "]")
    w(f"{tag}_observed_tasks_max = [" + ", ".join(r["meta"].get("observed_tasks_max", "0") for r in c["runs"]) + "]")
    w(f"{tag}_observed_cpus_allowed = [" + ", ".join(q(r["meta"].get("observed_cpus_allowed", "")) for r in c["runs"]) + "]")
w(f"pinned_8_over_pinned_1_wall = {med(t8['name'], 'walls') / med(t1['name'], 'walls'):.2f}  # below 1: 8 threads are faster")
w(f"knob_only_over_pinned_1_wall = {med(kn['name'], 'walls') / med(t1['name'], 'walls'):.2f}")
w(f"knob_only_over_pinned_1_core_s = {med(kn['name'], 'cores') / med(t1['name'], 'cores'):.2f}")
w(f"historical_1thread_over_pinned_1_wall = {HIST_GEODE_1THREAD_WALL_S / med(t1['name'], 'walls'):.2f}")
w(f"historical_1thread_over_pinned_8_wall = {HIST_GEODE_1THREAD_WALL_S / med(t8['name'], 'walls'):.2f}")
w(f"historical_1thread_over_knob_only_wall = {HIST_GEODE_1THREAD_WALL_S / med(kn['name'], 'walls'):.2f}")
p8 = by["palace_s4p5_n6_np8"]
w(f"palace_np8_core_s_median = {med(p8['name'], 'cores'):.2f}")
w(f"palace_np8_core_s_over_pinned_1_thread_core_s = {med(p8['name'], 'cores') / med(t1['name'], 'cores'):.2f}  # the measured form of the paper's '~12x fewer core-seconds'; this geode cell returns {t1['n_physical']} of the 6 modes")
w(f"palace_np8_wall_over_pinned_1_thread_wall = {med(p8['name'], 'walls') / med(t1['name'], 'walls'):.2f}  # the measured form of 'one core beats eight ranks'; above 1 means geode's wall clock is lower")
if "palace_s4p5_n6_np8_nosave" in by:
    w(f"palace_np8_nosave_core_s_over_pinned_1_thread_core_s = {med('palace_s4p5_n6_np8_nosave', 'cores') / med(t1['name'], 'cores'):.2f}  # same, Palace without ParaView output")
    w(f"palace_np8_nosave_wall_over_pinned_1_thread_wall = {med('palace_s4p5_n6_np8_nosave', 'walls') / med(t1['name'], 'walls'):.2f}")
kn_cpu = max((r["user"] + r["sys"]) / r["wall"] * 100 for r in kn["runs"])
m1, m8, mk = med(t1["name"], "walls"), med(t8["name"], "walls"), med(kn["name"], "walls")
w(f"knob_alone_is_serial_today = {'true' if kn_cpu <= 110 else 'false'}  # GEODE_NUM_THREADS=1 with no pinning: cpu_percent at most {kn_cpu:.0f} although the process may use {kn['runs'][0]['meta'].get('observed_cpus_allowed', '?')} and has {kn['runs'][0]['meta'].get('observed_tasks_max', '?')} tasks")
hist_gap = abs(HIST_GEODE_8THREAD_WALL_S - HIST_GEODE_1THREAD_WALL_S) / HIST_GEODE_1THREAD_WALL_S * 100
now_gap = (m1 - m8) / m1 * 100
numbers = (
    f"The historical 1-thread row ({HIST_GEODE_1THREAD_WALL_S} s) is {abs(m1 - HIST_GEODE_1THREAD_WALL_S) / m1 * 100:.0f}% "
    f"{'below' if HIST_GEODE_1THREAD_WALL_S < m1 else 'above'} today's OS-enforced one-thread median ({m1:.2f} s) and "
    f"{abs(m8 - HIST_GEODE_1THREAD_WALL_S) / m8 * 100:.0f}% {'below' if HIST_GEODE_1THREAD_WALL_S < m8 else 'above'} today's 8-thread median ({m8:.2f} s). "
    f"The two historical rows differ by {hist_gap:.0f}%; today's pinned 1-thread and 8-thread rows differ by {now_gap:.0f}%."
)
if hist_gap < 3 and now_gap > 10 and abs(m8 - HIST_GEODE_1THREAD_WALL_S) < abs(m1 - HIST_GEODE_1THREAD_WALL_S):
    verdict = "That is the pattern a multithreaded LU in BOTH historical rows would give: no difference between them then, a difference now that one thread is enforced, and the old time nearer today's 8-thread time. It is consistent with #763's suspicion and it is not proof (see limits)."
else:
    verdict = "These numbers do not point either way on whether the historical 1-thread row was multithreaded (see limits)."
w(f"reading = {q(numbers + ' ' + verdict)}")
core_ratio = med(t8["name"], "cores") / med(t1["name"], "cores")
w(
    "consequence = "
    + q(
        f"Whatever the old row ran, a one-core geode run of this request costs {m1:.1f} s on this instance type today, "
        f"not {HIST_GEODE_1THREAD_WALL_S} s, and 8 threads buy {now_gap:.0f}% of wall clock for {core_ratio:.2f}x the core-seconds."
    )
)
w('limits = "The historical binary (geode 3174015) predates the transmon_bench example and was not rebuilt, the region differs (us-west-2 then, us-east-1 now), and the code between the two commits changed more than the thread handling. The historical run recorded no user/sys time, so its thread count cannot be read back; this table can only show which of today\'s pinned and unpinned configurations its wall clock is consistent with."')
w("")
# ---- the historical off-target request ---------------------------------------
g1, g8 = by.get("geode_s20_n12_t1"), by.get("geode_s20_n12_t8")
q1, q8 = by.get("palace_s20_n12_np1"), by.get("palace_s20_n12_np8")
if g1 and g8 and q1 and q8:
    w("[off_target_request]")
    w('what = "12 modes at 20 GHz: the request of results.toml [matched.off_target], whose 2026-07-14 cells have no mode log. Re-run here so the mode lists exist."')
    w(f"historical_wall_s = {{ geode_1thread = {HIST_OFF_GEODE_1THREAD_WALL_S}, geode_8threads = {HIST_OFF_GEODE_8THREAD_WALL_S}, palace_np1 = {HIST_OFF_PALACE_NP1_WALL_S}, palace_np8 = {HIST_OFF_PALACE_NP8_WALL_S} }}")
    w(f"geode_modes_f_ghz = {fl(g8['freqs'], 4)}")
    w(f"palace_modes_f_ghz = {fl(q8['freqs'], 4)}")
    w(f"geode_n_of_six = {g8['n_physical']}  # plus {len(g8['freqs']) - g8['n_physical']} non-physical")
    w(f"palace_n_of_six = {q8['n_physical']}  # the other {len(q8['freqs']) - q8['n_physical']} are Palace modes above 26.08 GHz")
    w(f"modes_in_common = {in_common(q8['freqs'], g8['freqs'])}  # of {len(q8['freqs'])} Palace modes and {len(g8['freqs'])} geode modes")
    w("like_for_like = false  # the two solvers return different mode sets for this request")
    w(f"geode_1_thread_wall_s_min_median_max = {fl(mmm(g1['walls']), 2)}")
    w(f"geode_1_thread_core_s_min_median_max = {fl(mmm(g1['cores']), 2)}")
    w(f"geode_8_threads_wall_s_min_median_max = {fl(mmm(g8['walls']), 2)}")
    w(f"geode_8_threads_core_s_min_median_max = {fl(mmm(g8['cores']), 2)}")
    w(f"palace_np1_wall_s = {fl(q1['walls'], 2)}  # n = {len(q1['runs'])}")
    w(f"palace_np1_core_s = {fl(q1['cores'], 2)}")
    w(f"palace_np8_wall_s = {fl(q8['walls'], 2)}  # n = {len(q8['runs'])}")
    w(f"palace_np8_core_s = {fl(q8['cores'], 2)}")
    w(f"palace_np1_over_geode_1_thread_wall = {statistics.median(q1['walls']) / statistics.median(g1['walls']):.2f}  # same request, different returned modes")
    w(f"palace_np8_over_geode_8_threads_wall = {statistics.median(q8['walls']) / statistics.median(g8['walls']):.2f}  # same request, different returned modes")
    w('reading = "This pair does not measure target robustness of one eigenproblem answered twice. Palace is asked for, and returns, the 12 modes above 20 GHz (up to 51.5 GHz); geode returns the 12 Ritz values nearest 20 GHz, which are the six physical modes from 5.15 to 26.09 GHz, five near-kernel values and the 3.45 GHz port mode."')
    w("")
w("[not_done]")
w("items = [")
w('  "the Palace-GPU / Palace-CPU re-time on the Lambda A100 host of ../transmon_bench_gpu",')
w('  "the 1.16M-DOF cell: no Palace oracle for the refined mesh is committed, so its modes cannot be classified",')
w('  "any mesh other than the 133k fixture, any Palace solver setting other than the fixture config, and Palace with Eigenmode.Save = 0",')
w('  "revising papers/transmon-benchmark main.tex (operator, #593); the affected lines and replacement numbers are in papers/transmon-benchmark/BRIEF.md",')
w("]")
w('follow_up = "#950: a timed geode configuration that returns the six physical modes at the 4.5 GHz shift with no oracle filter"')
print("\n".join(out))
