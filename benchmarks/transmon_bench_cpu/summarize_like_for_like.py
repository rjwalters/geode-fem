#!/usr/bin/env python3
"""Summarize a like_for_like_local.sh run tree into a results TOML (stdout).

usage: summarize_like_for_like.py <run-dir> <palace-eig.csv> [<950-run-dir>]
  e.g. python3 -I benchmarks/transmon_bench_cpu/summarize_like_for_like.py \
         benchmarks/transmon_bench_cpu/runs/2026-10-08_local_like_for_like \
         reference/fixtures/transmon_palace/results_p1/eig.csv \
         > benchmarks/transmon_bench_cpu/results_like_for_like_local.toml

Reads <run-dir>/cells.tsv, host.txt and raw/<cell>_run<i>.{log,time,meta}, and
the committed Palace eigenvalues. For every cell it lists the modes geode
returned, classifies each one, counts the physical ones, and reports wall
clock, CPU time and peak RSS per run. Pure stdlib; no fitting, no outlier
rejection, every run of every cell is reported.

With a third argument, the issue #950 before/after sweep in that directory
(like_for_like_local_950.sh) is appended by summarize_local_950.py.

Classification (issue #927; the rule of benchmarks/transmon_eigen):
  physical      nearest geode mode to a Palace mode, within PHYSICAL_TOL_PCT
  near_kernel   f < NEAR_KERNEL_GHZ (the image(d0) gradient cluster at ~0)
  spurious_port within PHYSICAL_TOL_PCT of the documented 3.4528 GHz mode
  unclassified  anything else
Participation is not used: the 3.45 GHz mode has participation 0.994.
"""
import csv
import math
import pathlib
import re
import statistics
import sys

# The <=1% same-mesh bar of crates/geode-core/tests/transmon_eigenmode.rs
# (PALACE_BAR_PCT), which gates benchmarks/transmon_eigen/results.toml.
PHYSICAL_TOL_PCT = 1.0
# Threshold of real_transmon_port_aware_retains_all_six_modes for a gradient
# near-kernel survivor.
NEAR_KERNEL_GHZ = 0.5
# benchmarks/transmon_eigen/results.toml [spurious_mode].f_ghz (#514).
SPURIOUS_GHZ = 3.4528
# benchmarks/transmon_eigen/results.toml [spurious_mode].tree_cotree_gradient_dofs
# = rank(d0_interior): the dimension of the gradient near-kernel at lambda ~ 0.
GRADIENT_KERNEL_DIM = 13747
BASELINE_CELL = "ungauged_s4p5_n6"

MODE = re.compile(r"mode\[(\d+)\]: .*?f = ([0-9.eE+-]+) GHz, participation p = ([0-9.]+)")


def read_oracle(path):
    with open(path) as f:
        rows = list(csv.reader(f))
    return [float(r[1]) for r in rows[1:] if len(r) >= 2]


def kv(path):
    out = {}
    if path.exists():
        for line in path.read_text().splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                out[k.strip()] = v.strip()
    return out


def kv_tokens(text):
    """{key: value} from whitespace-separated key=value tokens on one line."""
    return dict(t.split("=", 1) for t in text.split() if "=" in t)


def read_time(path):
    """(wall_s, user_s, sys_s, peak_rss_bytes) from /usr/bin/time -l or -v."""
    txt = path.read_text(errors="replace")
    m = re.search(r"^\s*([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", txt, re.M)
    if m:  # macOS: maximum resident set size is in BYTES
        rss = re.search(r"^\s*(\d+)\s+maximum resident set size", txt, re.M)
        return float(m.group(1)), float(m.group(2)), float(m.group(3)), int(rss.group(1))
    w = re.search(r"Elapsed \(wall clock\) time.*: ([0-9:.]+)", txt)
    u = re.search(r"User time \(seconds\):\s*([0-9.]+)", txt)
    s = re.search(r"System time \(seconds\):\s*([0-9.]+)", txt)
    r = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", txt)
    sec = 0.0
    for p in w.group(1).split(":"):
        sec = sec * 60 + float(p)
    return sec, float(u.group(1)), float(s.group(1)), int(r.group(1)) * 1024


def classify(freqs, oracle):
    """(classes, matches): one class per geode mode; one match row per Palace mode."""
    classes = [None] * len(freqs)
    matches = []
    for pf in oracle:
        if not freqs:
            matches.append((pf, None, None))
            continue
        i = min(range(len(freqs)), key=lambda k: abs(freqs[k] - pf))
        rel = abs(freqs[i] - pf) / pf * 100.0
        if rel <= PHYSICAL_TOL_PCT:
            classes[i] = "physical"
            matches.append((pf, freqs[i], rel))
        else:
            matches.append((pf, None, None))
    for i, f in enumerate(freqs):
        if classes[i]:
            continue
        if f < NEAR_KERNEL_GHZ:
            classes[i] = "near_kernel"
        elif abs(f - SPURIOUS_GHZ) / SPURIOUS_GHZ * 100.0 <= PHYSICAL_TOL_PCT:
            classes[i] = "spurious_port"
        else:
            classes[i] = "unclassified"
    return classes, matches


def fl(xs, nd):
    return "[" + ", ".join(f"{x:.{nd}f}" for x in xs) + "]"


def mean_sd(xs):
    if len(xs) < 2:
        return xs[0], None
    return statistics.mean(xs), statistics.stdev(xs)


# Everything below runs only as a script, so summarize_like_for_like_m6i.py can
# load this file for the shared classification rule and parsers.
if __name__ == "__main__":
    run_dir = pathlib.Path(sys.argv[1])
    oracle_csv = pathlib.Path(sys.argv[2])
    raw = run_dir / "raw"
    oracle = read_oracle(oracle_csv)
    host = kv(run_dir / "host.txt")
    cells = []
    for line in (run_dir / "cells.tsv").read_text().splitlines():
        if not line.strip():
            continue
        name, gauge, sigma, nmodes, runs = line.split("\t")
        c = {"name": name, "gauge": gauge, "sigma": float(sigma), "n": int(nmodes), "runs": []}
        for i in range(1, int(runs) + 1):
            stem = raw / f"{name}_run{i}"
            meta = kv(stem.with_suffix(".meta"))
            log = stem.with_suffix(".log").read_text(errors="replace")
            modes = [(float(m.group(2)), float(m.group(3))) for m in MODE.finditer(log)]
            wall, user, sys_s, rss = read_time(stem.with_suffix(".time"))
            solve = re.search(r"^\s*solve \(.*\) : ([0-9.]+) s", log, re.M)
            c["runs"].append(
                {
                    "meta": meta,
                    "modes": modes,
                    "wall": wall,
                    "user": user,
                    "sys": sys_s,
                    "rss": rss,
                    "solve": float(solve.group(1)) if solve else float("nan"),
                    "exit": int(meta.get("exit", "-1")),
                }
            )
        r0 = c["runs"][0]
        c["freqs"] = [f for f, _ in r0["modes"]]
        c["part"] = [p for _, p in r0["modes"]]
        c["classes"], c["matches"] = classify(c["freqs"], oracle)
        c["n_physical"] = sum(1 for k in c["classes"] if k == "physical")
        c["six"] = all(g is not None for _, g, _ in c["matches"]) and len(oracle) == 6
        per_run_phys = []
        for r in c["runs"]:
            fr = [f for f, _ in r["modes"]]
            cl, _ = classify(fr, oracle)
            per_run_phys.append([round(f, 6) for f, k in zip(fr, cl) if k == "physical"])
        c["phys_identical"] = all(p == per_run_phys[0] for p in per_run_phys)
        c["all_identical"] = all(
            [round(f, 6) for f, _ in r["modes"]] == [round(f, 6) for f in c["freqs"]] for r in c["runs"]
        )
        cells.append(c)

    by_name = {c["name"]: c for c in cells}
    base = by_name[BASELINE_CELL]
    base_wall = statistics.mean(r["wall"] for r in base["runs"])
    base_solve = statistics.mean(r["solve"] for r in base["runs"])
    base_rss = max(r["rss"] for r in base["runs"])
    base_cpu = statistics.mean(r["user"] + r["sys"] for r in base["runs"])

    loads = []
    starts = []
    for c in cells:
        for r in c["runs"]:
            for k in ("loadavg_before", "loadavg_after"):
                if k in r["meta"]:
                    loads.append(float(r["meta"][k].split()[0]))
            starts.append(r["meta"].get("start_utc", ""))
            starts.append(r["meta"].get("end_utc", ""))
    starts = sorted(s for s in starts if s)

    out = []
    w = out.append
    w("# geode-only cost of returning the SAME six physical modes as Palace on the")
    w("# 133k transmon fixture (issue #927). GENERATED by")
    w("# benchmarks/transmon_bench_cpu/summarize_like_for_like.py from")
    w(f"# {run_dir.as_posix()}/ (do not edit by hand; rerun the script).")
    w("#")
    w("# ---------------------------------------------------------------------------")
    w("# READ FIRST")
    w("#   * LOCAL DEV-MACHINE, GEODE ONLY. This is NOT a benchmark host and there is")
    w("#     NO Palace run here. The developer Mac was running other builds at the")
    w("#     same time (load average and CPU% are recorded per cell). Every run is")
    w("#     single-threaded (see [meta].threads), so these are ONE-CORE times.")
    w("#   * THE LOAD-INDEPENDENT RESULTS are the mode lists, the number of physical")
    w("#     modes each request returns, and the frequency agreement with Palace.")
    w("#   * TIMES ARE INDICATIVE ONLY. Wall clocks carry scheduling noise; CPU")
    w("#     seconds (cpu_s) are the steadier cost. Do NOT place any time or RSS")
    w("#     from this file next to the m6i cells of results.toml or the Lambda A100 cells of")
    w("#     ../transmon_bench_gpu/results.toml: different host, different load,")
    w("#     different core count. The only time comparison this file supports is a")
    w("#     RATIO between two of its own cells, measured minutes apart on one host.")
    w("#   * The same-host re-timing against Palace is NOT done. See [deferred].")
    w("# ---------------------------------------------------------------------------")
    w("")
    w("[meta]")
    w("issue = 927")
    w(f'measured_date = "{starts[0][:10]}"  # UTC; runs between {starts[0]} and {starts[-1]}')
    w(f'geode_commit = "{host.get("geode_commit", "unknown")}"')
    w('scope = "LOCAL, GEODE-ONLY, loaded developer machine; not a benchmark-host measurement and not a head-to-head with Palace"')
    w('fixture = "crates/geode-core/tests/fixtures/transmon_smoke.msh"')
    w(f'fixture_sha256 = "{host.get("fixture_sha256", "unknown")}"')
    w("n_interior_dofs = 133108")
    w('binary = "cargo build --release -p geode-core --example transmon_bench; GEODE_INNER=direct (faer COLAMD sparse LU shift-invert Lanczos, f64)"')
    w(f'geode_num_threads = {host.get("geode_num_threads", "0")}  # caps faer\'s sparse LU pool only (see ../transmon_bench_gpu/results.toml [notes].geode_threads)')
    w(f'rayon_num_threads = {host.get("rayon_num_threads", "0")}  # caps the rest of the process')
    w('threads = "single-threaded, so that a run\'s CPU seconds are a usable cost on an oversubscribed host; cpu_percent per cell shows what each run actually used. No multi-threaded run is recorded here"')
    w('timing = "/usr/bin/time wall clock over the full transmon_bench process (mesh load + assembly + solve); solve_s is the solver phase the binary prints"')
    w('memory = "peak resident set size of the process, from /usr/bin/time"')
    w('statistic = "every run is listed; mean and sample stddev where a cell has 3 runs, a single sample otherwise"')
    w('generator = "benchmarks/transmon_bench_cpu/like_for_like_local.sh, then summarize_like_for_like.py"')
    w("")
    w("[hardware]")
    w(f'model = "{host.get("model", "unknown")}"')
    w(f'cpu = "{host.get("cpu", "unknown")}"')
    w(f'os = "{host.get("os", "unknown")}"')
    w(f'rustc = "{host.get("rustc", "unknown")}"')
    w(f'logical_cpus = {host.get("logical_cpus", "0")}')
    w(f'ram_gib = {host.get("ram_gib", "0")}')
    w(f"loadavg_1min_range = [{min(loads):.2f}, {max(loads):.2f}]  # over all run starts and ends")
    w('label = "dev machine, loaded; timings indicative only"')
    w("")
    w("[oracle]")
    w(f'source = "{oracle_csv.as_posix()} (Palace fba6a5b, Order 1, same mesh; the 6 modes Palace returns for N = 6, Target = 4.5 GHz)"')
    w(f"palace_f_ghz = {fl(oracle, 6)}")
    w(f"physical_tol_pct = {PHYSICAL_TOL_PCT}  # a geode mode is physical when it is the nearest mode to a Palace mode and within this")
    w(f"near_kernel_below_ghz = {NEAR_KERNEL_GHZ}")
    w(f"spurious_port_f_ghz = {SPURIOUS_GHZ}  # ../transmon_eigen/results.toml [spurious_mode], #514")
    w('rule = "frequency match against the Palace oracle, as benchmarks/transmon_eigen does; participation cannot identify physical modes (the 3.45 GHz mode has p = 0.994)"')
    w("")

    for c in cells:
        rs = c["runs"]
        walls = [r["wall"] for r in rs]
        wm, wsd = mean_sd(walls)
        sm, ssd = mean_sd([r["solve"] for r in rs])
        counts = {k: c["classes"].count(k) for k in ("physical", "near_kernel", "spurious_port", "unclassified")}
        w("[[cell]]")
        w(f'name = "{c["name"]}"')
        w(f'gauge = "{c["gauge"]}"')
        w(f"sigma_ghz = {c['sigma']}")
        w(f"n_modes_requested = {c['n']}")
        w(f"n_runs = {len(rs)}")
        w(f"exit_codes = [{', '.join(str(r['exit']) for r in rs)}]")
        w(f"modes_f_ghz = {fl(c['freqs'], 6)}")
        w(f"modes_participation = {fl(c['part'], 4)}")
        w("modes_class = [" + ", ".join(f'"{k}"' for k in c["classes"]) + "]")
        w(f"n_physical = {c['n_physical']}  # of Palace's {len(oracle)}")
        w(f"n_near_kernel = {counts['near_kernel']}")
        w(f"n_spurious_port = {counts['spurious_port']}")
        w(f"n_unclassified = {counts['unclassified']}" + ("  # no oracle for these and no residual check on this solver path; not counted as physical" if counts["unclassified"] else ""))
        w(f"six_physical_recovered = {'true' if c['six'] else 'false'}")
        matched = [(p, g, e) for p, g, e in c["matches"] if g is not None]
        if matched:
            w("physical_matches = [")
            for p, g, e in matched:
                w(f"  {{ palace_f_ghz = {p:.6f}, geode_f_ghz = {g:.6f}, rel_err_pct = {e:.4f} }},")
            w("]")
            w(f"physical_max_rel_err_pct = {max(e for _, _, e in matched):.4f}")
        w(f"modes_identical_across_runs = {'true' if c['all_identical'] else 'false'}")
        w(f"physical_modes_identical_across_runs = {'true' if c['phys_identical'] else 'false'}")
        w(f"wall_s = {fl(walls, 2)}")
        w(f"wall_s_mean = {wm:.2f}")
        if wsd is not None:
            w(f"wall_s_stddev = {wsd:.2f}")
        w(f"solve_s = {fl([r['solve'] for r in rs], 2)}")
        w(f"solve_s_mean = {sm:.2f}")
        w(f"user_s = {fl([r['user'] for r in rs], 2)}")
        w(f"sys_s = {fl([r['sys'] for r in rs], 2)}")
        cpus = [r["user"] + r["sys"] for r in rs]
        w(f"cpu_s = {fl(cpus, 2)}  # user + sys: the load-robust cost of a single-threaded run")
        w(f"cpu_s_mean = {statistics.mean(cpus):.2f}")
        w("cpu_percent = [" + ", ".join(f"{(r['user'] + r['sys']) / r['wall'] * 100:.0f}" for r in rs) + "]  # below 100: the run was descheduled, so its wall clock is inflated")
        w("peak_rss_bytes = [" + ", ".join(str(r["rss"]) for r in rs) + "]")
        w(f"peak_rss_gb = {max(r['rss'] for r in rs) / 1e9:.2f}  # max over runs, decimal GB")
        w('loadavg_1min_before = [' + ", ".join(r["meta"].get("loadavg_before", "nan").split()[0] for r in rs) + "]")
        w(f"cpu_ratio_vs_{BASELINE_CELL} = {statistics.mean(cpus) / base_cpu:.2f}  # same host, same sweep; the ratio to quote")
        w(f"wall_ratio_vs_{BASELINE_CELL} = {wm / base_wall:.2f}  # indicative only: both wall clocks carry scheduling noise")
        w(f"solve_ratio_vs_{BASELINE_CELL} = {sm / base_solve:.2f}")
        w(f"peak_rss_ratio_vs_{BASELINE_CELL} = {max(r['rss'] for r in rs) / base_rss:.2f}")
        w("")


    def strategy(gauge, sigma):
        return sorted((c for c in cells if c["gauge"] == gauge and c["sigma"] == sigma), key=lambda c: c["n"])


    def min_six(cs):
        ok = [c for c in cs if c["six"]]
        return ok[0] if ok else None


    def nearer_than_kernel(sigma):
        """Palace modes nearer the shift than lambda = 0 is, in lambda ~ f^2."""
        return sum(1 for f in oracle if abs(f * f - sigma * sigma) < sigma * sigma)


    w("# ---------------------------------------------------------------------------")
    w("# FINDINGS. Every number below is computed from the [[cell]] tables above and")
    w("# the oracle; the sentences scope it to what was measured.")
    w("# ---------------------------------------------------------------------------")
    for key, gauge, sigma, what in (
        ("same_target_ungauged", "none", 4.5, "option 2 as written: keep the 4.5 GHz shift, raise the mode count"),
        ("moved_shift_ungauged", "none", 20.0, "option 2 with the shift moved to 20 GHz (the transmon_eigen gate's shift)"),
        ("same_target_port_aware", "port_aware", 4.5, "option 1: the #514 port-aware divergence-free projection at the 4.5 GHz shift"),
    ):
        cs = strategy(gauge, sigma)
        w(f"[finding.{key}]")
        w(f'what = "{what}"')
        w(f"sigma_ghz = {sigma}")
        w("n_modes_tried = [" + ", ".join(str(c["n"]) for c in cs) + "]")
        w("n_physical_returned = [" + ", ".join(str(c["n_physical"]) for c in cs) + "]")
        w(f"palace_modes_nearer_shift_than_gradient_kernel = {nearer_than_kernel(sigma)}  # |f^2 - sigma^2| < sigma^2, from the oracle alone")
        m = min_six(cs)
        if m is None:
            w("six_physical_recovered = false")
            w(f"max_n_modes_tried = {cs[-1]['n']}")
        else:
            wm = statistics.mean(r["wall"] for r in m["runs"])
            sm = statistics.mean(r["solve"] for r in m["runs"])
            w("six_physical_recovered = true")
            w(f"min_n_modes_for_six_physical = {m['n']}  # smallest request tried that returns all six")
            below = [c for c in cs if c["n"] < m["n"]]
            if below:
                w(f"largest_n_modes_tried_that_falls_short = {below[-1]['n']}  # returns {below[-1]['n_physical']} physical")
            w(f'cell = "{m["name"]}"')
            w(f"non_physical_modes_returned_alongside = {m['n'] - m['n_physical']}")
            w(f"physical_max_rel_err_pct = {max(e for _, g, e in m['matches'] if g is not None):.4f}")
            w(f"wall_s_mean = {wm:.2f}  # dev machine, loaded; n = {len(m['runs'])}")
            w(f"solve_s_mean = {sm:.2f}")
            w(f"peak_rss_gb = {max(r['rss'] for r in m['runs']) / 1e9:.2f}")
            cm = statistics.mean(r["user"] + r["sys"] for r in m["runs"])
            w(f"cpu_s_mean = {cm:.2f}  # user + sys, single thread")
            w(f"cpu_ratio_vs_{BASELINE_CELL} = {cm / base_cpu:.2f}  # the ratio to quote")
            w(f"wall_ratio_vs_{BASELINE_CELL} = {wm / base_wall:.2f}  # indicative only")
            w(f"solve_ratio_vs_{BASELINE_CELL} = {sm / base_solve:.2f}")
            w(f"peak_rss_ratio_vs_{BASELINE_CELL} = {max(r['rss'] for r in m['runs']) / base_rss:.2f}")
        w("")

    top = max(oracle)
    w("[finding]")
    w(f"baseline_cell = \"{BASELINE_CELL}\"")
    w(f"baseline_n_physical = {base['n_physical']}  # the timed configuration returns this many of Palace's {len(oracle)}")
    w(f"baseline_wall_s_mean = {base_wall:.2f}  # dev machine, loaded, one thread")
    w(f"baseline_cpu_s_mean = {base_cpu:.2f}  # user + sys")
    w(f"gradient_kernel_dim = {GRADIENT_KERNEL_DIM}  # rank(d0_interior), ../transmon_eigen/results.toml [spurious_mode].tree_cotree_gradient_dofs")
    w(f"min_shift_ghz_for_six_before_kernel = {top / math.sqrt(2):.2f}  # top Palace mode / sqrt(2): below this shift the kernel is nearer than the top mode")
    st = strategy("none", 4.5)
    st_min = min_six(st)
    if st_min is not None:
        nk = st_min["classes"].count("near_kernel")
        w(f"same_target_near_kernel_ritz_values = {nk}  # in the smallest 4.5 GHz request that returns all six ({st_min['name']})")
    w('same_target_mechanism = "Shift-invert orders eigenvalues by |lambda - sigma|, lambda ~ f^2. At sigma = 4.5 GHz only the 5.15 GHz mode is nearer the shift than lambda = 0, where the gradient near-kernel (dimension gradient_kernel_dim) sits; the other five physical modes are farther than every kernel mode. A converged enumeration would therefore need more than gradient_kernel_dim modes. This solver path (SparseShiftInvertLanczos::smallest_eigenpairs) does not enumerate: it builds a Krylov space of max(96, n_modes + 2) steps from a deterministic start vector and returns the n_modes Ritz values nearest the shift, with no per-pair residual check. In that space the kernel shows up as only same_target_near_kernel_ritz_values Ritz values, so a request large enough to hold them, the 3.45 GHz mode and the six physical modes returns the six. That count belongs to this Krylov size, start vector and mesh. It is not a property of the eigenproblem and was not tested on any other mesh."')
    w('scope = "One mesh (133,108 interior DOFs, first order), one host, geode only, direct LU. The mode counts and the frequency agreement do not depend on load. The wall clocks do (cpu_percent below 100 marks a descheduled run), so the cost ratios to read are the cpu_ratio_* fields: CPU seconds of single-threaded runs of this file against each other. Nothing here is a geode-vs-Palace time, and a one-thread CPU-second ratio on this machine is not a prediction of the multi-threaded wall-clock ratio on a benchmark host."')
    w('moved_shift_caveat = "Moving the shift to 20 GHz is a different request from Palace\'s (N = 6, Target = 4.5 GHz). It returns the six physical modes only because 20 GHz was chosen knowing where they are (it must exceed min_shift_ghz_for_six_before_kernel). It is not a general recipe."')
    w('same_target_caveat = "Raising the mode count at 4.5 GHz returns the six physical modes only buried among near-kernel Ritz values (and, for larger requests, unclassified ones above 26 GHz). The request size that works was found by trial against the oracle; nothing in the solver output says when it is large enough."')
    w('port_aware_caveat = "The port-aware solve keeps the 4.5 GHz shift and needs no reference-solver number (the junction extract is placed at the analytic f_LC). It still returns two non-physical modes ahead of the top physical ones: one near-zero survivor and the solenoidal 3.45 GHz port mode, which no divergence-free projection removes (#514). A caller needs a rule to drop them; here that rule is the Palace oracle."')
    w("")
    w("[deferred]")
    w("# NOT done in this file. Operator-run, paid hosts (issue #927).")
    w("not_done = [")
    w('  "same-box, same-session re-timing of geode (moved shift or port-aware) against Palace on the m6i benchmark host (AWS geode-fem-bench)",')
    w('  "the Palace-GPU / Palace-CPU re-time on the Lambda A100 host of ../transmon_bench_gpu",')
    w('  "the 1.16M-DOF cell: no Palace oracle for the refined mesh is committed, so its modes cannot be classified here",')
    w('  "revising the efficiency claim of papers/transmon-benchmark (see #763, #593)",')
    w("]")
    w('follow_up = "#950: a timed geode configuration that returns the six physical modes at the 4.5 GHz shift with no oracle filter (drop the near-zero survivor, remove or identify the 3.45 GHz port mode, cut the port-aware cost)"')
    if len(sys.argv) > 3:
        import importlib.util

        _spec = importlib.util.spec_from_file_location(
            "summarize_local_950", pathlib.Path(__file__).resolve().parent / "summarize_local_950.py"
        )
        _s950 = importlib.util.module_from_spec(_spec)
        _spec.loader.exec_module(_s950)
        w("")
        out.extend(_s950.section(sys.argv[3], oracle, sys.modules[__name__]))
    print("\n".join(out))
