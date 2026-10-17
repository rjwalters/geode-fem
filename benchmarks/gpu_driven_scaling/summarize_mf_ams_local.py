#!/usr/bin/env python3
"""Write results_mf_ams_cpu_local.toml from an ams_cpu_sweep.sh run tree (#966).

usage: summarize_mf_ams_local.py <run-dir> [<r6i-results>] > results_mf_ams_cpu_local.toml

<run-dir> holds the per-leg outputs of ams_cpu_sweep.sh, every leg run with
RAYON_NUM_THREADS=1, one process per (size, config):

  n<N>_jacobi    assembled COCG + Jacobi              (harness config 2)
  n<N>_ams       assembled COCG + AMS                 (config 5)
  n<N>_mfjacobi  matrix-free COCG + on-device Jacobi  (config 3)
  n<N>_mfams     matrix-free COCG + host-side AMS     (config 6, issue #966);
                 its .err carries the "# mf_ams_profile" line
  n<N>_mfxcheck  Direct + configs 5 and 6 in one process, for the in-harness
                 full-field rel-L2 vs Direct

plus two optional subtrees of capped matrix-free Jacobi legs, for sizes where
the full matrix-free Jacobi solve (thousands of iterations) was not run:

  mfjacobi_cap1/n<N>_mfjacobi    GEODE_SCALING_ITER_MAX=1
  mfjacobi_cap300/n<N>_mfjacobi  GEODE_SCALING_ITER_MAX=300

Their per-iteration cost is (t_300 - t_1) / 299, the setup-subtracted method
of the #520 record. A size with both a full and a capped run reports both, as
a check of the method.

<r6i-results> (default: results_ams_cpu_r6i.toml next to this script) is read
for the single-thread assembled-AMS and Jacobi ITERATION COUNTS only, to check
them against this host's. No timing from that host is copied.

Every number in the output is read from those files or computed here. Pure
stdlib.
"""
import pathlib
import re
import sys
import tomllib

d = pathlib.Path(sys.argv[1])
here = pathlib.Path(__file__).resolve().parent
r6i_path = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else here / "results_ams_cpu_r6i.toml"

# label -> (leg suffix, harness config id, description)
CONFIGS = [
    ("jacobi", "jacobi", "2_iterative_csr", "assembled CSR COCG + Jacobi"),
    ("ams", "ams", "5_iterative_ams", "assembled CSR COCG + AMS"),
    ("mf_jacobi", "mfjacobi", "3_matrix_free", "matrix-free (Burn ndarray f64) COCG + on-device Jacobi"),
    ("mf_ams", "mfams", "6_matrix_free_ams", "matrix-free (Burn ndarray f64) COCG + host-side AMS"),
]
ACCEPT_FACTOR = 1.5  # issue #966: matrix-free AMS iterations within 1.5x of assembled AMS


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
    if m:
        out["wall_s"], out["user_s"], out["sys_s"] = (float(g) for g in m.groups())
        out["max_rss_bytes"] = int(re.search(r"(\d+)\s+maximum resident set size", txt).group(1))
        return out
    m = re.search(r"Elapsed \(wall clock\) time.*: ([\d:.]+)", txt)
    if m:
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


def profile(path):
    """The '# mf_ams_profile' line of a matrix-free AMS leg's stderr."""
    if not path.exists():
        return None
    m = re.search(
        r"# mf_ams_profile applies=(\d+) krylov_s=([\d.]+) vcycle_s=([\d.]+) transfer_s=([\d.]+)",
        path.read_text(),
    )
    if not m:
        return None
    return {
        "applies": int(m.group(1)),
        "krylov_s": float(m.group(2)),
        "vcycle_s": float(m.group(3)),
        "transfer_s": float(m.group(4)),
    }


def q(s):
    return '"' + str(s).replace("\\", "\\\\").replace('"', '\\"') + '"'


def sci(x, digits=3):
    return "nan" if x != x else f"{x:.{digits}e}"


def f3(x):
    return "nan" if x != x else f"{x:.3f}"


def one_cell(path, config):
    cells = [c for c in fragment(path) if c.get("config") == config]
    return cells[0] if cells else None


# ---- gather -----------------------------------------------------------------
sizes = sorted(
    {int(m.group(1)) for p in d.glob("n*_*.stdout") if (m := re.match(r"n(\d+)_", p.name))}
)
metas = [keyvals(p) for p in sorted(d.rglob("n*_*.meta"))]
commits = sorted({m.get("git", "?") for m in metas})
starts = sorted(m["start"] for m in metas if "start" in m)
rayon = sorted({m.get("rayon_num_threads", "?") for m in metas})
loads = []
for m in metas:
    for k in ("loadavg_start", "loadavg_end"):
        if k in m:
            loads.append(float(m[k].split()[0]))
rcs = sorted({m.get("rc", "?") for m in metas})
host = keyvals(d / "host.txt")

rows = {}  # (n, label) -> dict
for n in sizes:
    for label, leg, config, _ in CONFIGS:
        c = one_cell(d / f"n{n}_{leg}.stdout", config)
        if c is None:
            continue
        t = time_file(d / f"n{n}_{leg}.time")
        meta = keyvals(d / f"n{n}_{leg}.meta")
        r = {
            "n_edges": c["n_edges"],
            "n_interior": c["n_interior"],
            "converged": c["converged"],
            "iterations": c["iterations"],
            "residual_rel": c["residual_rel"],
            "setup_s": c.get("warmup_setup_s", float("nan")),
            "krylov_s": c.get("warmup_krylov_s", float("nan")),
            "process_wall_s": t.get("wall_s", float("nan")),
            "max_rss_gb": t.get("max_rss_bytes", float("nan")) / 1e9,
            "loadavg_start": float(meta.get("loadavg_start", "nan").split()[0]),
            "port_v": c.get("port_v"),
        }
        r["krylov_ms_per_iteration"] = 1e3 * r["krylov_s"] / r["iterations"]
        if label == "mf_ams":
            r["profile"] = profile(d / f"n{n}_{leg}.err")
        rows[(n, label)] = r

capped = {}  # n -> per-iteration ms of matrix-free Jacobi from the capped legs
for n in sizes:
    c1 = one_cell(d / "mfjacobi_cap1" / f"n{n}_mfjacobi.stdout", "3_matrix_free")
    c300 = one_cell(d / "mfjacobi_cap300" / f"n{n}_mfjacobi.stdout", "3_matrix_free")
    if c1 and c300:
        t1, t300 = c1["dnf_attempt_s"], c300["dnf_attempt_s"]
        i1, i300 = c1["iterations"], c300["iterations"]
        capped[n] = {
            "n_edges": c300["n_edges"],
            "t_cap1_s": t1,
            "t_cap300_s": t300,
            "iters": (i1, i300),
            "ms_per_iteration": 1e3 * (t300 - t1) / (i300 - i1),
        }

xcheck = {}
for n in sizes:
    cells = {c["config"]: c for c in fragment(d / f"n{n}_mfxcheck.stdout")}
    if "1_direct" in cells:
        xcheck[n] = cells

r6i = {}
if r6i_path.exists():
    for c in tomllib.loads(r6i_path.read_text()).get("cell", []):
        if c.get("threads") == 1 and c.get("preconditioner") in ("ams", "jacobi"):
            r6i[(c["size_n"], c["preconditioner"])] = c["iterations"]

# ---- emit -------------------------------------------------------------------
out = []
p = out.append
p("# Matrix-free driven solve with the AMS preconditioner, CPU f64 (issue #966).")
p("# GENERATED by benchmarks/gpu_driven_scaling/summarize_mf_ams_local.py")
p(f"# from benchmarks/gpu_driven_scaling/runs/{d.name}/ (do not edit by hand; rerun the script).")
p("#")
p("# ---------------------------------------------------------------------------")
p("# READ FIRST")
p("#   * LOCAL DEV-MACHINE RUN, NOT A BENCHMARK HOST: a developer Mac running")
p("#     other builds at the same time (load average per leg is recorded).")
p("#   * ITERATION COUNTS are the load-independent result. TIMES ARE INDICATIVE,")
p("#     single samples; the per-iteration RATIOS are same-host, same-session,")
p("#     single-threaded (RAYON_NUM_THREADS=1 on every leg).")
p("#   * \"Matrix-free AMS\" = matrix-free Krylov OPERATOR + host-side AMS")
p("#     PRECONDITIONER. The AMS is the assembled path's (assembled real SPD")
p("#     proxy, exact sparse LU of the nodal coarse operator). It is a CPU")
p("#     stopgap, not a GPU-ready preconditioner; see [cost_model] and [deferred].")
p("#   * Matrix-free Jacobi was not run to convergence above 102k edges (about")
p("#     40 min at 463k edges here). Its cost there is MEASURED per iteration from")
p("#     capped runs and its time-to-solution is PROJECTED with the Jacobi")
p("#     iteration count; every projected value is named *_projected.")
p("#   * No GPU, no Palace. Nothing here is a GPU performance claim.")
p("# ---------------------------------------------------------------------------")
p("")
p("[meta]")
p("issue = 966")
p(f"measured_date = {q(starts[0][:10] if starts else '?')}  # UTC; legs started {starts[0] if starts else '?'} to {starts[-1] if starts else '?'}")
p(f"geode_commit = {q(', '.join(commits))}")
p(f"scope = {q('LOCAL CPU run of the #520 sizes 25.7k to 463k edges on a loaded developer machine; iteration counts are the result, times indicative')}")
p(f"fixture = {q('sigma-lossy parallel-plate cube, single LumpedPort (e_hat = y, R = 1), eps_r = 1, sigma = 2.0, PEC on y-planes (crates/geode-core/tests/gpu_driven_scaling.rs), identical to results_ams_cpu_r6i.toml')}")
p("omega = 0.10")
p("iter_tol = 1e-8  # every iterative config; convergence is judged on the EXPLICIT residual ||Ax - b|| / ||b||")
p("iter_max = 20000")
p("max_replacements = 0  # the harness's first-crossing check (issue #987)")
p(f"rayon_num_threads = {q(', '.join(rayon))}")
p(f"leg_exit_codes = {q(', '.join(rcs))}")
p(f"statistic = {q('one solve per cell (GEODE_SCALING_REPS=0: the warm-up solve is the measurement). setup_s = prepare_at (assemble A(omega), build the preconditioner and, for matrix-free, upload the mesh and build the Burn operator); krylov_s = solve (COCG plus the explicit-residual check). One process per (size, config).')}")
p(f"generator = {q('benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh with LEGS=jacobi,ams,mfjacobi,mfams,mfxcheck (and capped mfjacobi legs), then summarize_mf_ams_local.py')}")
p("")
p("[hardware]")
for k in ("model", "cpu", "os", "rustc", "logical_cpus", "ram_gib"):
    if k in host:
        v = host[k]
        p(f"{k} = {v}" if k in ("logical_cpus", "ram_gib") else f"{k} = {q(v)}")
if loads:
    p(f"loadavg_1min_range = [{min(loads):.2f}, {max(loads):.2f}]  # over all leg starts and ends")
p(f"label = {q('dev machine, loaded; timings indicative only')}")
p("")

for n in sizes:
    for label, leg, config, desc in CONFIGS:
        r = rows.get((n, label))
        if r is None:
            continue
        p("[[cell]]")
        p(f"size_n = {n}")
        p(f"n_edges = {r['n_edges']}")
        p(f"n_interior = {r['n_interior']}")
        p(f"config = {q(label)}  # {desc} (harness {config})")
        p(f"converged = {str(r['converged']).lower()}")
        p(f"iterations = {r['iterations']}")
        p(f"residual_rel = {sci(r['residual_rel'])}")
        p(f"setup_s = {f3(r['setup_s'])}")
        p(f"krylov_s = {f3(r['krylov_s'])}")
        p(f"krylov_ms_per_iteration = {f3(r['krylov_ms_per_iteration'])}")
        p(f"process_wall_s = {f3(r['process_wall_s'])}")
        p(f"max_rss_gb = {r['max_rss_gb']:.2f}")
        p(f"loadavg_start = {r['loadavg_start']:.2f}")
        pv = r.get("port_v")
        if pv:
            p(f"port_v = [{sci(pv[0], 9)}, {sci(pv[1], 9)}]")
        prof = r.get("profile")
        if prof:
            p(f"ams_applies = {prof['applies']}")
            p(f"ams_vcycle_s = {f3(prof['vcycle_s'])}  # host V-cycles (real + imaginary part)")
            p(f"ams_transfer_s = {f3(prof['transfer_s'])}  # download + gather, scatter + upload")
        p("")

for n, c in sorted(capped.items()):
    p("[[mf_jacobi_capped]]")
    p(f"size_n = {n}")
    p(f"n_edges = {c['n_edges']}")
    p(f"iterations = {list(c['iters'])}")
    p(f"attempt_s = [{c['t_cap1_s']:.3f}, {c['t_cap300_s']:.3f}]  # prepare_at + capped solve")
    p(f"ms_per_iteration = {c['ms_per_iteration']:.3f}  # (t_300 - t_1) / 299")
    full = rows.get((n, "mf_jacobi"))
    if full:
        p(f"full_run_ms_per_iteration = {full['krylov_ms_per_iteration']:.3f}  # same size, run to convergence: a check of the capped method")
    p("")

for n, cells in sorted(xcheck.items()):
    p("[[xcheck]]")
    p(f"size_n = {n}")
    p(f"n_edges = {cells['1_direct']['n_edges']}")
    for cfg, key in (("5_iterative_ams", "ams"), ("6_matrix_free_ams", "mf_ams")):
        if cfg in cells:
            p(f"{key}_iterations = {cells[cfg]['iterations']}")
            p(f"{key}_field_rel_l2_vs_direct = {sci(cells[cfg]['accuracy_rel_l2_vs_direct'])}")
    p("")

p("# Same-host ratios at each size. iteration_ratio_mf_ams_over_ams is the")
p(f"# issue #966 acceptance (<= {ACCEPT_FACTOR}). per_iteration_* ratios divide krylov_ms_per_iteration.")
for n in sizes:
    a, m = rows.get((n, "ams")), rows.get((n, "mf_ams"))
    if not (a and m):
        continue
    j, mj = rows.get((n, "jacobi")), rows.get((n, "mf_jacobi"))
    mj_ms = mj["krylov_ms_per_iteration"] if mj else capped.get(n, {}).get("ms_per_iteration")
    p("[[comparison]]")
    p(f"size_n = {n}")
    p(f"n_edges = {m['n_edges']}")
    ratio = m["iterations"] / a["iterations"]
    p(f"iteration_ratio_mf_ams_over_ams = {ratio:.3f}")
    p(f"within_acceptance = {str(ratio <= ACCEPT_FACTOR and m['converged']).lower()}")
    p(f"per_iteration_mf_ams_over_ams = {m['krylov_ms_per_iteration'] / a['krylov_ms_per_iteration']:.3f}")
    if mj_ms:
        p(f"per_iteration_mf_ams_over_mf_jacobi = {m['krylov_ms_per_iteration'] / mj_ms:.3f}{'' if mj else '  # mf_jacobi from the capped runs'}")
    if j:
        p(f"per_iteration_ams_over_jacobi = {a['krylov_ms_per_iteration'] / j['krylov_ms_per_iteration']:.3f}")
    mf_ams_total = m["setup_s"] + m["krylov_s"]
    p(f"mf_ams_solve_s = {mf_ams_total:.3f}  # setup_s + krylov_s")
    if mj:
        mj_total = mj["setup_s"] + mj["krylov_s"]
        p(f"mf_jacobi_solve_s = {mj_total:.3f}")
        p(f"speedup_mf_ams_over_mf_jacobi = {mj_total / mf_ams_total:.2f}")
    elif mj_ms and j:
        c = capped[n]
        proj = c["t_cap1_s"] + j["iterations"] * mj_ms / 1e3
        p(f"mf_jacobi_solve_s_projected = {proj:.1f}  # t_1 + (assembled Jacobi iterations = {j['iterations']}) x ms_per_iteration")
        p(f"speedup_mf_ams_over_mf_jacobi_projected = {proj / mf_ams_total:.1f}")
    p(f"ams_solve_s = {a['setup_s'] + a['krylov_s']:.3f}")
    p("")

p("[cost_model]")
p("# Per COCG iteration, matrix-free AMS = one matrix-free operator apply and the")
p("# COCG vector updates (as for matrix-free Jacobi) + one host AMS application:")
p("# V-cycles on the real and imaginary parts + one [n] transfer each way.")
p("# remainder_ms = krylov_ms_per_iteration - (ams_vcycle_s + ams_transfer_s) / ams_applies;")
p("# it should track matrix-free Jacobi's per-iteration cost.")
sz, vc, tr, rem, mjv = [], [], [], [], []
for n in sizes:
    m = rows.get((n, "mf_ams"))
    if not (m and m.get("profile")):
        continue
    prof = m["profile"]
    v = 1e3 * prof["vcycle_s"] / prof["applies"]
    t = 1e3 * prof["transfer_s"] / prof["applies"]
    mj = rows.get((n, "mf_jacobi"))
    mj_ms = mj["krylov_ms_per_iteration"] if mj else capped.get(n, {}).get("ms_per_iteration", float("nan"))
    sz.append(m["n_edges"])
    vc.append(v)
    tr.append(t)
    rem.append(m["krylov_ms_per_iteration"] - v - t)
    mjv.append(mj_ms)
p(f"n_edges = {sz}")
p(f"vcycle_ms_per_apply = [{', '.join(f3(x) for x in vc)}]")
p(f"transfer_ms_per_apply = [{', '.join(f3(x) for x in tr)}]")
p(f"remainder_ms_per_iteration = [{', '.join(f3(x) for x in rem)}]")
p(f"mf_jacobi_ms_per_iteration = [{', '.join(f3(x) for x in mjv)}]")
asm = [rows[(n, 'ams')]['krylov_ms_per_iteration'] for n in sizes if (n, 'ams') in rows and (n, 'mf_ams') in rows]
p(f"assembled_ams_ms_per_iteration = [{', '.join(f3(x) for x in asm)}]")
mf_tot = [r + v + t for r, v, t in zip(rem, vc, tr)]
over_asm = [m / a for m, a in zip(mf_tot, asm)]
over_mfj = [m / j for m, j in zip(mf_tot, mjv)]
tr_share = [t / m for t, m in zip(tr, mf_tot)]
p(
    "note = "
    + q(
        f"Per iteration, matrix-free AMS costs {min(over_asm):.2f}x to {max(over_asm):.2f}x assembled AMS "
        f"and {min(over_mfj):.2f}x to {max(over_mfj):.2f}x matrix-free Jacobi on this host: the ndarray "
        "matrix-free operator apply (remainder) costs about as much as the whole host V-cycle. The "
        f"transfers (host memory copies on ndarray) are {100 * min(tr_share):.1f}% to {100 * max(tr_share):.1f}% "
        "of an iteration. On a GPU the transfer would be a PCIe round trip and the nodal LU triangular "
        "solves would stay serial on the host."
    )
)
p("")

if r6i:
    p("[r6i_iteration_check]")
    p("# Single-thread ITERATION COUNTS of the assembled configs in results_ams_cpu_r6i.toml")
    p("# (AWS r6i.4xlarge, issue #930) at the sizes both runs share. Counts only; no timing.")
    shared = [n for n in sizes if (n, "ams") in r6i]
    p(f"size_n = {shared}")
    p(f"r6i_ams_iterations = {[r6i[(n, 'ams')] for n in shared]}")
    p(f"local_ams_iterations = {[rows[(n, 'ams')]['iterations'] if (n, 'ams') in rows else -1 for n in shared]}")
    p(f"local_mf_ams_iterations = {[rows[(n, 'mf_ams')]['iterations'] if (n, 'mf_ams') in rows else -1 for n in shared]}")
    p(f"r6i_jacobi_iterations = {[r6i.get((n, 'jacobi'), -1) for n in shared]}")
    p(f"local_jacobi_iterations = {[rows[(n, 'jacobi')]['iterations'] if (n, 'jacobi') in rows else -1 for n in shared]}")
    p("")

p("[coarse_solve]")
p(f"decision = {q('Stopgap: keep the exact sparse LU of the nodal (gradient-space) coarse operator G^T P G, on the host, with the vector-nodal block on 4 symmetric Gauss-Seidel sweeps: AmsCoarseSolve::Auto, the assembled path default. It is O(fill) per apply and serial (two triangular solves per V-cycle), so it is not a GPU design. The coarse solve stays pluggable through IterativePreconditioner::Ams { coarse }: AMG and SGS reach the matrix-free path unchanged (crates/geode-core/tests/driven_matrix_free_ams.rs, matrix_free_ams_coarse_solve_is_pluggable, compares the two paths at each coarse solve).')}")
p(f"drift_744 = {q('Not re-investigated here. The #744 measurement (AMG / SGS gradient-space solves drift on the driven spiral) stands; an O(nnz) coarse solve that does not drift is the precondition for a GPU-resident AMS.')}")
p("")
p("[deferred]")
p(f"gpu = {q('GPU (f64 or mixed precision per #534) on the same fixture against Palace on a GPU: a separate, operator-gated step. The host-side AMS is rejected on non-ndarray-f64 backends.')}")
p(f"benchmark_host = {q('Same-host timings on a dedicated box (r6i.4xlarge, as results_ams_cpu_r6i.toml) and the 898k-edge size: not run (no cloud spend in this change).')}")
proj = {
    n: capped[n]["t_cap1_s"] + rows[(n, "jacobi")]["iterations"] * capped[n]["ms_per_iteration"] / 1e3
    for n in capped
    if (n, "mf_jacobi") not in rows and (n, "jacobi") in rows
}
if proj:
    which = ", ".join(f"{capped[n]['n_edges']} edges (about {v / 60:.0f} min projected)" for n, v in sorted(proj.items()))
    p(f"mf_jacobi_full_at_scale = {q('Matrix-free Jacobi to convergence at ' + which + ': not run; per-iteration cost measured from the capped runs instead.')}")

sys.stdout.write("\n".join(out) + "\n")
