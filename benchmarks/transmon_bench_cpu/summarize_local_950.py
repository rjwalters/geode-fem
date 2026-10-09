#!/usr/bin/env python3
"""The issue #950 section of results_like_for_like_local.toml.

Not run on its own: summarize_like_for_like.py loads this file and appends
section(<run-dir>, <oracle>) when it is given the #950 run directory as a third
argument, so the whole TOML stays one generated file:

  python3 -I benchmarks/transmon_bench_cpu/summarize_like_for_like.py \
      benchmarks/transmon_bench_cpu/runs/2026-10-08_local_like_for_like \
      reference/fixtures/transmon_palace/results_p1/eig.csv \
      benchmarks/transmon_bench_cpu/runs/2026-10-09_local_950 \
      > benchmarks/transmon_bench_cpu/results_like_for_like_local.toml

<run-dir> comes from like_for_like_local_950.sh (cells.tsv has a sixth column,
the binary: main = origin/main before #950, head = this change). The Palace
oracle is used only to LABEL the modes in this file, exactly as in the #927
cells above; the solver never sees it.
"""
import pathlib
import re
import statistics

FACT = re.compile(r"factorizations of K − σM: (\d+); re-admitted gradient directions: (\d+)")
NULL = re.compile(r"null-mode filter: dropped (\d+) \(\|λ\| ≤ ([0-9.eE+-]+); largest dropped \|λ\| = ([0-9.eE+-]+)\)")
LAMBDA = re.compile(r"mode\[\d+\]: λ = ([0-9.eE+-]+),")
RESID = re.compile(r"mode\[\d+\]: .*residual = ([0-9.eE+-]+)")
BASELINE = "ungauged_s4p5_n6"


def section(run_dir, oracle, rule):
    """TOML lines for the #950 run tree. `rule` is summarize_like_for_like."""
    run_dir = pathlib.Path(run_dir)
    raw = run_dir / "raw"
    host = rule.kv(run_dir / "host.txt")
    cells = []
    for line in (run_dir / "cells.tsv").read_text().splitlines():
        if not line.strip():
            continue
        name, gauge, sigma, nmodes, runs, which = line.split("\t")
        c = {"name": name, "gauge": gauge, "sigma": float(sigma), "n": int(nmodes), "bin": which, "runs": []}
        for i in range(1, int(runs) + 1):
            stem = raw / f"{name}_run{i}"
            meta = rule.kv(stem.with_suffix(".meta"))
            log = stem.with_suffix(".log").read_text(errors="replace")
            wall, user, sys_s, rss = rule.read_time(stem.with_suffix(".time"))
            solve = re.search(r"^\s*solve \(.*\) : ([0-9.]+) s", log, re.M)
            fact = FACT.search(log)
            null = NULL.search(log)
            c["runs"].append(
                {
                    "meta": meta,
                    "modes": [(float(m.group(2)), float(m.group(3))) for m in rule.MODE.finditer(log)],
                    "lambdas": [float(m.group(1)) for m in LAMBDA.finditer(log)],
                    "resid": [float(m.group(1)) for m in RESID.finditer(log)],
                    "fact": fact.groups() if fact else None,
                    "null": null.groups() if null else None,
                    "wall": wall,
                    "cpu": user + sys_s,
                    "rss": rss,
                    "solve": float(solve.group(1)) if solve else float("nan"),
                    "exit": int(meta.get("exit", "-1")),
                }
            )
        r0 = c["runs"][0]
        c["freqs"] = [f for f, _ in r0["modes"]]
        c["classes"], c["matches"] = rule.classify(c["freqs"], oracle)
        c["six"] = all(g is not None for _, g, _ in c["matches"]) and len(oracle) == 6
        c["identical"] = all([round(f, 6) for f, _ in r["modes"]] == [round(f, 6) for f in c["freqs"]] for r in c["runs"])
        cells.append(c)

    by = {c["name"]: c for c in cells}
    base = by[BASELINE]
    base_cpu = statistics.mean(r["cpu"] for r in base["runs"])
    base_rss = max(r["rss"] for r in base["runs"])
    loads = [float(r["meta"][k].split()[0]) for c in cells for r in c["runs"] for k in ("loadavg_before", "loadavg_after") if k in r["meta"]]
    stamps = sorted(s for c in cells for r in c["runs"] for s in (r["meta"].get("start_utc", ""), r["meta"].get("end_utc", "")) if s)

    out = []
    w = out.append
    w("# ---------------------------------------------------------------------------")
    w("# ISSUE #950 (pieces 1 and 3), APPENDED. Generated from")
    w(f"# {run_dir.as_posix()}/ by summarize_local_950.py via summarize_like_for_like.py.")
    w("# Same host and per-run discipline as the #927 cells above, but a separate")
    w("# sweep on a different day: compare #950 cells only with each other and with")
    w(f"# this sweep's own {BASELINE} cell, never with the #927 cell times above.")
    w("# The oracle only LABELS modes here; no solver run reads it.")
    w("# ---------------------------------------------------------------------------")
    w("[issue_950]")
    w(f'measured_date = "{stamps[0][:10]}"  # UTC; runs between {stamps[0]} and {stamps[-1]}')
    w(f'head_commit = "{host.get("geode_commit", "unknown")}"  # binary "head": this change')
    w(f'main_commit = "{host.get("main_commit", "unknown")}"  # binary "main": origin/main before #950')
    w(f'host = "{host.get("model", "unknown")}, {host.get("cpu", "unknown")}, {host.get("os", "unknown")}"')
    w(f'fixture_sha256 = "{host.get("fixture_sha256", "unknown")}"')
    w(f'geode_num_threads = {host.get("geode_num_threads", "0")}')
    w(f'rayon_num_threads = {host.get("rayon_num_threads", "0")}')
    w(f"loadavg_1min_range = [{min(loads):.2f}, {max(loads):.2f}]  # over all run starts and ends")
    w('generator = "benchmarks/transmon_bench_cpu/like_for_like_local_950.sh, then summarize_like_for_like.py with the run dir as third argument"')
    w("")
    for c in cells:
        rs = c["runs"]
        r0 = rs[0]
        counts = {k: c["classes"].count(k) for k in ("physical", "near_kernel", "spurious_port", "unclassified")}
        cpus = [r["cpu"] for r in rs]
        w("[[issue_950.cell]]")
        w(f'name = "{c["name"]}"')
        w(f'binary = "{c["bin"]}"')
        w(f'gauge = "{c["gauge"]}"')
        w(f"sigma_ghz = {c['sigma']}")
        w(f"n_modes_requested = {c['n']}")
        w(f"n_runs = {len(rs)}")
        w(f"exit_codes = [{', '.join(str(r['exit']) for r in rs)}]")
        w(f"n_modes_returned = {len(c['freqs'])}")
        w(f"modes_f_ghz = {rule.fl(c['freqs'], 6)}")
        w("modes_lambda = [" + ", ".join(f"{x:.6e}" for x in r0["lambdas"]) + "]  # (1/um)^2")
        w("modes_class = [" + ", ".join(f'"{k}"' for k in c["classes"]) + "]  # labels from the oracle, for this file only")
        w(f"n_physical = {counts['physical']}  # of Palace's {len(oracle)}")
        w(f"n_near_kernel = {counts['near_kernel']}")
        w(f"n_spurious_port = {counts['spurious_port']}")
        w(f"n_unclassified = {counts['unclassified']}")
        w(f"six_physical_recovered = {'true' if c['six'] else 'false'}")
        matched = [(p, g, e) for p, g, e in c["matches"] if g is not None]
        if matched:
            w(f"physical_max_rel_err_pct = {max(e for _, _, e in matched):.4f}")
        if r0["resid"]:
            w("modes_residual_rel = [" + ", ".join(f"{x:.2e}" for x in r0["resid"]) + "]  # ||Kx - lambda Mx|| / (|lambda| ||Mx||), printed by the solver")
        if r0["fact"]:
            w(f"shifted_factorizations = {r0['fact'][0]}  # printed by the solver")
            w(f"readmitted_gradient_directions = {r0['fact'][1]}")
        elif c["gauge"] == "port_aware" and c["bin"] == "main":
            w('shifted_factorizations = "not printed by this binary (predates #950); its code factors K - sigma_j M for the junction extract and K - sigma M for the band: 2"')
        if r0["null"]:
            w(f"null_modes_dropped = {r0['null'][0]}")
            w(f"null_ceiling_lambda = {float(r0['null'][1]):.3e}  # DEGENERATE_SHIFT_REL_TOL x median(K_ii/M_ii)")
            w(f"max_dropped_null_lambda = {float(r0['null'][2]):.3e}")
        w(f"modes_identical_across_runs = {'true' if c['identical'] else 'false'}")
        w(f"wall_s = {rule.fl([r['wall'] for r in rs], 2)}")
        w(f"solve_s = {rule.fl([r['solve'] for r in rs], 2)}")
        w(f"cpu_s = {rule.fl(cpus, 2)}  # user + sys, single thread")
        w(f"cpu_s_mean = {statistics.mean(cpus):.2f}")
        w("cpu_percent = [" + ", ".join(f"{r['cpu'] / r['wall'] * 100:.0f}" for r in rs) + "]")
        w("peak_rss_bytes = [" + ", ".join(str(r["rss"]) for r in rs) + "]")
        w(f"peak_rss_gb = {max(r['rss'] for r in rs) / 1e9:.2f}")
        w("loadavg_1min_before = [" + ", ".join(r["meta"].get("loadavg_before", "nan").split()[0] for r in rs) + "]")
        w(f"cpu_ratio_vs_{BASELINE} = {statistics.mean(cpus) / base_cpu:.2f}  # this sweep's own baseline")
        w(f"peak_rss_ratio_vs_{BASELINE} = {max(r['rss'] for r in rs) / base_rss:.2f}")
        w("")

    def min_six(prefix_cells):
        ok = sorted((c for c in prefix_cells if c["six"]), key=lambda c: c["n"])
        return ok[0] if ok else None

    before = min_six([c for c in cells if c["bin"] == "main" and c["gauge"] == "port_aware"])
    after = min_six([c for c in cells if c["bin"] == "head" and c["gauge"] == "port_aware"])
    w("[issue_950.finding]")
    for key, c in (("before", before), ("after", after)):
        if c is None:
            w(f"{key}_six_physical_recovered = false")
            continue
        cm = statistics.mean(r["cpu"] for r in c["runs"])
        w(f'{key}_cell = "{c["name"]}"')
        w(f"{key}_min_n_modes_for_six_physical = {c['n']}  # smallest request tried that returns all six")
        w(f"{key}_non_physical_returned_alongside = {len(c['freqs']) - c['classes'].count('physical')}")
        w(f"{key}_cpu_s_mean = {cm:.2f}")
        w(f"{key}_cpu_ratio_vs_{BASELINE} = {cm / base_cpu:.2f}")
        w(f"{key}_peak_rss_ratio_vs_{BASELINE} = {max(r['rss'] for r in c['runs']) / base_rss:.2f}")
    if before and after:
        bc = statistics.mean(r["cpu"] for r in before["runs"])
        ac = statistics.mean(r["cpu"] for r in after["runs"])
        w(f"after_over_before_cpu = {ac / bc:.2f}  # geode vs geode, same sweep")
        w(f"after_over_before_peak_rss = {max(r['rss'] for r in after['runs']) / max(r['rss'] for r in before['runs']):.2f}")
        bp = sorted(round(g, 6) for _, g, _ in before["matches"] if g is not None)
        ap = sorted(round(g, 6) for _, g, _ in after["matches"] if g is not None)
        w(f"physical_f_ghz_identical_before_after = {'true' if bp == ap else 'false'}  # to the 6 decimals the binary prints")
    w('remaining_non_physical = "The one non-physical mode left at N = 7 is the solenoidal 3.45 GHz port mode (#514). It is a genuine eigenpair of this port formulation, not a gradient artifact, so neither piece removes it; removing it needs a different port formulation (piece 2 of #950, out of scope here)."')
    w('scope = "One mesh (133,108 interior DOFs), one loaded host, geode only, direct LU, one thread. Mode lists and factorization counts do not depend on load; times do. No geode-vs-Palace time is implied."')
    return out
