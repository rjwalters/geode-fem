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

Classification (issue #927, re-baselined by #1045; the rule of
benchmarks/transmon_eigen). The reference list is the qubit mode (Palace,
Target 2 GHz, #1043) plus the six modes of the <palace-eig.csv> oracle
(Palace's fixture request, Target 4.5 GHz): five cavity modes and the
17.49 GHz port-sheet artifact. A returned mode takes the kind of the
reference mode it is nearest to, within PHYSICAL_TOL_PCT:
  qubit                the transmon qubit mode (3.4527 GHz)
  cavity               a readout-resonator or box/cavity mode
  port_sheet_artifact  the 17.49 GHz zero-port-voltage mode of the distributed
                       port term (Palace has it too; the A2 shunt removes it)
  near_kernel          f < NEAR_KERNEL_GHZ (the image(d0) gradient cluster at ~0)
  unclassified         anything else
The six PHYSICAL modes are the qubit and the five cavity modes. The six
PALACE-REQUEST modes are what Palace's fixture request returns: the five
cavity modes and the port-sheet artifact. Before #1045 the second set was
called "physical" and the qubit mode "spurious_port"; see the migration note
in the generated header. Participation is not used to classify.
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
# benchmarks/transmon_eigen/results.toml [modes.qubit].palace_f_ghz: Palace's
# qubit mode at Target 2 GHz (#1043), six decimals. Not in the Target 4.5 oracle.
QUBIT_GHZ = 3.452749
# benchmarks/transmon_eigen/results.toml [modes.port_sheet_artifact].palace_f_ghz,
# six decimals: the oracle mode that is the port-sheet artifact, not a cavity mode.
PORT_SHEET_ARTIFACT_GHZ = 17.490109
PHYSICAL_KINDS = ("qubit", "cavity")
PALACE_REQUEST_KINDS = ("cavity", "port_sheet_artifact")
CLASSES = ("qubit", "cavity", "port_sheet_artifact", "near_kernel", "unclassified")
# benchmarks/transmon_eigen/results.toml [gauge_history].tree_cotree_gradient_dofs
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


def reference(oracle):
    """[(kind, palace_f_ghz)]: the qubit mode, then each oracle mode as cavity or port_sheet_artifact."""
    ref = [("qubit", QUBIT_GHZ)]
    for pf in oracle:
        art = abs(pf - PORT_SHEET_ARTIFACT_GHZ) / PORT_SHEET_ARTIFACT_GHZ * 100.0 <= PHYSICAL_TOL_PCT
        ref.append(("port_sheet_artifact" if art else "cavity", pf))
    return ref


def classify(freqs, oracle):
    """(classes, matches): one class per returned mode; one (kind, palace_f, f, rel_pct) row per reference mode."""
    classes = [None] * len(freqs)
    matches = []
    for kind, pf in reference(oracle):
        if not freqs:
            matches.append((kind, pf, None, None))
            continue
        i = min(range(len(freqs)), key=lambda k: abs(freqs[k] - pf))
        rel = abs(freqs[i] - pf) / pf * 100.0
        if rel <= PHYSICAL_TOL_PCT:
            classes[i] = kind
            matches.append((kind, pf, freqs[i], rel))
        else:
            matches.append((kind, pf, None, None))
    for i, f in enumerate(freqs):
        if classes[i]:
            continue
        classes[i] = "near_kernel" if f < NEAR_KERNEL_GHZ else "unclassified"
    return classes, matches


def tally(classes, matches):
    """Counts of one classified mode list (see the module docstring for the two six-mode sets)."""
    phys = [m for m in matches if m[0] in PHYSICAL_KINDS]
    req = [m for m in matches if m[0] in PALACE_REQUEST_KINDS]
    out = {k: classes.count(k) for k in CLASSES}
    out["physical"] = sum(1 for k in classes if k in PHYSICAL_KINDS)
    out["n_physical_ref"] = len(phys)
    out["six"] = len(phys) == 6 and all(m[2] is not None for m in phys)
    out["palace_request"] = sum(1 for k in classes if k in PALACE_REQUEST_KINDS)
    out["n_request_ref"] = len(req)
    out["request_six"] = len(req) == 6 and all(m[2] is not None for m in req)
    found = [m[3] for m in phys if m[2] is not None]
    out["physical_max_rel_err_pct"] = max(found) if found else None
    return out


def cell_counts(w, c, counts, unclassified_note=""):
    """The per-cell count lines shared by every generated file."""
    w(f"n_physical = {counts['physical']}  # of the {counts['n_physical_ref']} physical modes: the qubit and five cavity modes")
    w(f"n_qubit = {counts['qubit']}")
    w(f"n_cavity = {counts['cavity']}")
    w(f"n_port_sheet_artifact = {counts['port_sheet_artifact']}")
    w(f"n_near_kernel = {counts['near_kernel']}")
    w(f"n_unclassified = {counts['unclassified']}" + (unclassified_note if counts["unclassified"] else ""))
    w(f"six_physical_recovered = {'true' if counts['six'] else 'false'}")
    w(f"n_palace_request_modes = {counts['palace_request']}  # of the {counts['n_request_ref']} modes Palace's fixture request returns (five cavity + the port-sheet artifact); the pre-#1045 n_physical")
    w(f"palace_request_recovered = {'true' if counts['request_six'] else 'false'}  # all {counts['n_request_ref']} returned; the pre-#1045 six_physical_recovered")


def migration_note(w):
    """The #1045 key migration, as TOML comment lines (one copy for every generated file)."""
    w("# MIGRATION NOTE (issue #1045, 2026-10-11): MODES RE-CLASSIFIED, NO MEASUREMENT")
    w("# CHANGED. The 3.4528 GHz mode is the transmon qubit mode (Palace confirms it at")
    w("# 3.452749 GHz, ../transmon_eigen/results.toml [oracles.palace_target2]); the")
    w("# 17.49 GHz mode is the port-sheet artifact Palace shares (removed by the A2")
    w("# shunt, [port_voltage_a2]). Old -> new:")
    w('#   modes_class "spurious_port"              -> "qubit"')
    w('#   modes_class "physical" (17.49 GHz)       -> "port_sheet_artifact"')
    w('#   modes_class "physical" (other modes)     -> "cavity"')
    w("#   n_spurious_port                          -> n_qubit")
    w("#   n_physical / six_physical_recovered      -> RE-BASELINED: the six physical")
    w("#                                               modes are now the qubit and the five")
    w("#                                               cavity modes (old set: Palace's six")
    w("#                                               Target-4.5 modes, with the artifact)")
    w("#   (old n_physical, old six_physical_recovered) -> n_palace_request_modes,")
    w("#                                               palace_request_recovered (unchanged")
    w("#                                               values, honest names)")
    w("#   physical_matches                         -> reference_matches (with kind)")
    w("#   [oracle] spurious_port_f_ghz             -> qubit_f_ghz, port_sheet_artifact_f_ghz")
    w("# Counts and findings that depend on the physical set were regenerated from the")
    w("# same run trees; the CHANGELOG entry for #1045 lists the ones that moved.")


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
        c["tally"] = tally(c["classes"], c["matches"])
        c["n_physical"] = c["tally"]["physical"]
        c["six"] = c["tally"]["six"]
        per_run_phys = []
        for r in c["runs"]:
            fr = [f for f, _ in r["modes"]]
            cl, _ = classify(fr, oracle)
            per_run_phys.append([round(f, 6) for f, k in zip(fr, cl) if k in PHYSICAL_KINDS])
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
    w("# geode-only cost of returning the SAME six modes as Palace's fixture request on")
    w("# the 133k transmon fixture (issue #927), with every mode re-classified by #1045")
    w("# (qubit / cavity / port-sheet artifact). GENERATED by")
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
    migration_note(w)
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
    w(f"physical_tol_pct = {PHYSICAL_TOL_PCT}  # a geode mode takes the kind of the reference mode it is nearest to, within this")
    w(f"near_kernel_below_ghz = {NEAR_KERNEL_GHZ}")
    w(f"qubit_f_ghz = {QUBIT_GHZ}  # ../transmon_eigen/results.toml [modes.qubit].palace_f_ghz (Palace, Target 2 GHz, #1043); not in the oracle file")
    w(f"port_sheet_artifact_f_ghz = {PORT_SHEET_ARTIFACT_GHZ}  # the oracle mode that is [modes.port_sheet_artifact]; the other five oracle modes are cavity modes")
    w('physical_set = "the qubit mode and the five cavity modes (six)"')
    w('palace_request_set = "the six oracle modes: five cavity modes and the port-sheet artifact (what Palace returns for N = 6, Target = 4.5 GHz)"')
    w('rule = "frequency match against the reference list (qubit + oracle), as benchmarks/transmon_eigen does; participation is not used"')
    w("")

    for c in cells:
        rs = c["runs"]
        walls = [r["wall"] for r in rs]
        wm, wsd = mean_sd(walls)
        sm, ssd = mean_sd([r["solve"] for r in rs])
        counts = c["tally"]
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
        cell_counts(w, c, counts, "  # no oracle for these and no residual check on this solver path; not counted as physical")
        matched = [m for m in c["matches"] if m[2] is not None]
        if matched:
            w("reference_matches = [")
            for kind, p, g, e in matched:
                w(f'  {{ kind = "{kind}", palace_f_ghz = {p:.6f}, geode_f_ghz = {g:.6f}, rel_err_pct = {e:.4f} }},')
            w("]")
        if counts["physical_max_rel_err_pct"] is not None:
            w(f"physical_max_rel_err_pct = {counts['physical_max_rel_err_pct']:.4f}")
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
        """Physical reference modes nearer the shift than lambda = 0 is, in lambda ~ f^2."""
        return sum(1 for k, f in reference(oracle) if k in PHYSICAL_KINDS and abs(f * f - sigma * sigma) < sigma * sigma)


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
        w(f"physical_modes_nearer_shift_than_gradient_kernel = {nearer_than_kernel(sigma)}  # |f^2 - sigma^2| < sigma^2, from the reference list alone (was palace_modes_nearer_shift_than_gradient_kernel, over the oracle)")
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
            w(f"physical_max_rel_err_pct = {m['tally']['physical_max_rel_err_pct']:.4f}")
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
    w(f"baseline_n_physical = {base['n_physical']}  # the timed configuration returns this many of the six physical modes (the qubit and five cavity modes)")
    w(f"baseline_wall_s_mean = {base_wall:.2f}  # dev machine, loaded, one thread")
    w(f"baseline_cpu_s_mean = {base_cpu:.2f}  # user + sys")
    w(f"gradient_kernel_dim = {GRADIENT_KERNEL_DIM}  # rank(d0_interior), ../transmon_eigen/results.toml [gauge_history].tree_cotree_gradient_dofs")
    w(f"min_shift_ghz_for_six_before_kernel = {top / math.sqrt(2):.2f}  # top physical mode (26.08 GHz) / sqrt(2): below this shift the kernel is nearer than the top mode")
    st = strategy("none", 4.5)
    st_min = min_six(st)
    if st_min is not None:
        nk = st_min["classes"].count("near_kernel")
        w(f"same_target_near_kernel_ritz_values = {nk}  # in the smallest 4.5 GHz request that returns all six ({st_min['name']})")
    w('same_target_mechanism = "Shift-invert orders eigenvalues by |lambda - sigma|, lambda ~ f^2. At sigma = 4.5 GHz only the 3.45 GHz qubit mode and the 5.15 GHz mode are nearer the shift than lambda = 0, where the gradient near-kernel (dimension gradient_kernel_dim) sits; the other four physical modes (and the 17.49 GHz port-sheet artifact) are farther than every kernel mode. A converged enumeration would therefore need more than gradient_kernel_dim modes. This solver path (SparseShiftInvertLanczos::smallest_eigenpairs) does not enumerate: it builds a Krylov space of max(96, n_modes + 2) steps from a deterministic start vector and returns the n_modes Ritz values nearest the shift, with no per-pair residual check. In that space the kernel shows up as only same_target_near_kernel_ritz_values Ritz values, so a request large enough to hold them, the six physical modes and the port-sheet artifact returns the six. That count belongs to this Krylov size, start vector and mesh. It is not a property of the eigenproblem and was not tested on any other mesh."')
    w('scope = "One mesh (133,108 interior DOFs, first order), one host, geode only, direct LU. The mode counts and the frequency agreement do not depend on load. The wall clocks do (cpu_percent below 100 marks a descheduled run), so the cost ratios to read are the cpu_ratio_* fields: CPU seconds of single-threaded runs of this file against each other. Nothing here is a geode-vs-Palace time, and a one-thread CPU-second ratio on this machine is not a prediction of the multi-threaded wall-clock ratio on a benchmark host. Since #1045 the physical set is the qubit and the five cavity modes; before, it was Palace\'s six Target-4.5 modes, which include the port-sheet artifact and exclude the qubit."')
    w('moved_shift_caveat = "Moving the shift to 20 GHz is a different request from Palace\'s (N = 6, Target = 4.5 GHz). Six modes at 20 GHz return Palace\'s six Target-4.5 modes (five cavity modes and the port-sheet artifact) but not the qubit mode; seven return all six physical modes and the artifact. That works only because 20 GHz was chosen knowing where the modes are (it must exceed min_shift_ghz_for_six_before_kernel). It is not a general recipe."')
    w('same_target_caveat = "Raising the mode count at 4.5 GHz returns the six physical modes only buried among near-kernel Ritz values and the port-sheet artifact (and, for larger requests, unclassified ones above 26 GHz). The request size that works was found by trial against the oracle; nothing in the solver output says when it is large enough."')
    w('port_aware_caveat = "The port-aware solve keeps the 4.5 GHz shift and needs no reference-solver number (the 17.49 GHz extract is placed at the analytic f_LC). It still returns two non-physical modes ahead of the top physical ones: one near-zero survivor and the 17.49 GHz port-sheet artifact, which the port-aware projection re-admits by construction. A caller needs a rule to drop them; here that rule is the Palace oracle. The 3.45 GHz mode it also returns is the qubit mode (#1043), physical since #1045."')
    w("")
    w("[deferred]")
    w("# NOT done in this file. Operator-run, paid hosts (issue #927).")
    w("not_done = [")
    w('  "same-box, same-session re-timing of geode (moved shift or port-aware) against Palace on the m6i benchmark host (AWS geode-fem-bench)",')
    w('  "the Palace-GPU / Palace-CPU re-time on the Lambda A100 host of ../transmon_bench_gpu",')
    w('  "the 1.16M-DOF cell: no Palace oracle for the refined mesh is committed, so its modes cannot be classified here",')
    w('  "revising the efficiency claim of papers/transmon-benchmark (see #763, #593)",')
    w("]")
    w('follow_up = "#950: a timed geode configuration that returns the six physical modes at the 4.5 GHz shift with no oracle filter (drop the near-zero survivor, remove or identify the 3.45 GHz mode, cut the port-aware cost). #1043 identified the 3.45 GHz mode as the qubit mode; #1044 (A2) removes the 17.49 GHz port-sheet artifact"')
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
