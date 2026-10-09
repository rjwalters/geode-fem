#!/usr/bin/env python3
"""Summarize the tuned-Palace m6i run tree, beside the untuned PR #964 tree, into a TOML (stdout).

usage: summarize_tuned_palace_m6i.py <tuned-run-dir> <palace-eig.csv> <untuned-run-dir>
  e.g. python3 -I benchmarks/transmon_bench_cpu/summarize_tuned_palace_m6i.py \
         benchmarks/transmon_bench_cpu/runs/2026-10-09_m6i_tuned_palace \
         reference/fixtures/transmon_palace/results_p1/eig.csv \
         benchmarks/transmon_bench_cpu/runs/2026-10-08_m6i_like_for_like \
         > benchmarks/transmon_bench_cpu/results_tuned_palace_m6i.toml

<tuned-run-dir> is written by m6i_tuned_box_setup.sh and m6i_tuned_box_bench.sh
(issue #927): Palace from reference/palace/docker/Dockerfile.tuned and geode at
one commit, on one AWS m6i.4xlarge. <tuned-run-dir>/untuned_control/ is the
same-box control: Palace from the unchanged reference/palace/docker/Dockerfile,
built and timed on that same instance after the tuned sweep, same driver, same
four fixture-request cells. <untuned-run-dir> is the committed PR #964 tree
(the same untuned recipe, same instance type, an earlier session on a
different instance). All three are read from their raw /usr/bin/time, log,
meta and eig.csv files, never from a generated TOML.

The mode-classification rule, its thresholds and the /usr/bin/time parser are
those of summarize_like_for_like.py (loaded from the file next to this one).
Pure stdlib; no fitting, no outlier rejection, every run of every cell is
reported, with min / median / max.
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

# The six-mode geode routes of PR #964 plus its historical request.
ROUTES = (
    # (key, geode cell stem, what, caveat)
    (
        "historical_request",
        "geode_s4p5_n6",
        "6 modes at 4.5 GHz, ungauged: the request every committed geode-vs-Palace transmon time used",
        "Not a route to the six modes (returns 1 of Palace's 6). Kept for continuity with PR #964.",
    ),
    (
        "more_modes",
        "geode_s4p5_n30",
        "30 modes at 4.5 GHz, ungauged; the six physical modes are picked out of the 30 with the Palace oracle",
        "ORACLE-TUNED: 30 was found by trial against the Palace eigenvalues; the caller needs the oracle to tell the six from the other 24.",
    ),
    (
        "shift",
        "geode_s20_n6",
        "6 modes at 20 GHz, ungauged: a different request from Palace's (N = 6, Target = 4.5 GHz)",
        "ORACLE-TUNED: 20 GHz was chosen knowing where the modes are.",
    ),
    (
        "port_aware",
        "geode_pa_s4p5_n8",
        "8 modes at 4.5 GHz with GEODE_GAUGE=port_aware",
        "The only route not tuned against the oracle; it still returns two non-physical modes, dropped here with the oracle (#950).",
    ),
)
PALACE_CELLS = (
    # (key, cell name, what)
    ("save6_np1", "palace_s4p5_n6_np1", "fixture config (Save = 6), 1 rank"),
    ("save6_np8", "palace_s4p5_n6_np8", "fixture config (Save = 6), 8 ranks"),
    ("save0_np1", "palace_s4p5_n6_np1_nosave", "Save = 0 (no ParaView output), 1 rank"),
    ("save0_np8", "palace_s4p5_n6_np8_nosave", "Save = 0 (no ParaView output), 8 ranks"),
)

tuned_dir = pathlib.Path(sys.argv[1])
oracle_csv = pathlib.Path(sys.argv[2])
untuned_dir = pathlib.Path(sys.argv[3])
oracle = rule.read_oracle(oracle_csv)

PHASE = re.compile(r"^( *)([A-Z][A-Za-z.\- ]*?) {2,}([0-9.]+) +([0-9.]+) +([0-9.]+)\s*$")


def palace_phases(log):
    """Avg. column of Palace's 'Elapsed Time Report (s)' (same parser as summarize_like_for_like_m6i.py)."""
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


def estimator_s(run):
    return sum(run["phases"].get(k, 0.0) for k in ("Estimation", "Estimation / Construction", "Estimation / Solve"))


def fl(xs, nd):
    return "[" + ", ".join(f"{x:.{nd}f}" for x in xs) + "]"


def mmm(xs):
    return min(xs), statistics.median(xs), max(xs)


def spread_pct(xs):
    return (max(xs) - min(xs)) / statistics.median(xs) * 100.0


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


FILTERED = []


def load(run_dir):
    raw = run_dir / "raw"
    cells = {}
    for line in (run_dir / "cells.tsv").read_text().splitlines():
        if not line.strip():
            continue
        f = line.split("\t")
        name, solver, gauge, sigma, nmodes, width, pin, runs = f[:8]
        c = {
            "name": name,
            "solver": solver,
            "gauge": gauge,
            "sigma": float(sigma),
            "n": int(nmodes),
            "width": int(width),
            "pin": pin,
            "linear_requested": f[8] if len(f) > 8 else "",
            "runs": [],
        }
        n_runs = int(runs)
        if n_runs and not (raw / f"{name}_run1.meta").exists():
            # Listed in cells.tsv but excluded by the driver's CELL_FILTER (the
            # control sweep times only the fixture-request Palace cells).
            FILTERED.append(f"{run_dir.name}/{name}")
            continue
        for i in range(1, n_runs + 1):
            stem = raw / f"{name}_run{i}"
            meta = rule.kv(stem.with_suffix(".meta"))
            log = stem.with_suffix(".log").read_text(errors="replace")
            wall, user, sys_s, rss = rule.read_time(stem.with_suffix(".time"))
            r = {"meta": meta, "wall": wall, "user": user, "sys": sys_s, "rss": rss, "exit": int(meta.get("exit", "-1"))}
            if solver == "geode":
                r["freqs"] = [float(m.group(2)) for m in rule.MODE.finditer(log)]
                s = re.search(r"^\s*solve \(.*\) : ([0-9.]+) s", log, re.M)
                r["solve"] = float(s.group(1)) if s else float("nan")
            else:
                with open(raw / f"{name}_run{i}_eig.csv") as fh:
                    rows = list(csv.reader(fh))
                r["freqs"] = [float(x[1]) for x in rows[1:] if len(x) >= 2]
                cfg = json.loads((run_dir / "configs" / f"cfg_{name}_run{i}.json").read_text())["Solver"]
                r["save"] = cfg["Eigenmode"]["Save"]
                r["linear_type"] = cfg["Linear"]["Type"]
                r["phases"] = palace_phases(log)
                r["ceed"] = (re.search(r"libCEED backend: (\S+)", log) or [None, "unknown"])[1]
                r["changeset"] = (re.search(r"Git changeset ID: (\S+)", log) or [None, "unknown"])[1]
                r["mpi"] = (re.search(r"Running with (\d+) MPI process", log) or [None, "0"])[1]
                r["omp"] = (re.search(r"MPI process(?:es)?, (\d+) OpenMP thread", log) or [None, "1"])[1]
                r["gmres_iters"] = [int(x) for x in re.findall(r"GMRES solver converged in (\d+) iteration", log)]
            c["runs"].append(r)
        if not c["runs"]:
            continue  # a cell the driver was told to skip (runs = 0)
        c["freqs"] = c["runs"][0]["freqs"]
        c["classes"], c["matches"] = rule.classify(c["freqs"], oracle)
        c["n_physical"] = sum(1 for k in c["classes"] if k == "physical")
        c["six"] = all(g is not None for _, g, _ in c["matches"]) and len(oracle) == 6
        c["walls"] = [r["wall"] for r in c["runs"]]
        c["cores"] = [r["user"] + r["sys"] for r in c["runs"]]
        cells[name] = c
    return cells


prov = tuned_dir / "provenance"
T = load(tuned_dir)
ctl_dir = tuned_dir / "untuned_control"
C = load(ctl_dir)
U = load(untuned_dir)
c_pal = rule.kv(ctl_dir / "provenance" / "palace-image-provenance.txt")
c_img = (ctl_dir / "provenance" / "palace-image-id.txt").read_text().strip()
c_ceeds = sorted({r["ceed"] for c in C.values() if c["solver"] == "palace" for r in c["runs"]})


def isa(path):
    """{file: {ymm, evex_regs, opmask, zmm}} from an isa_check.sh output."""
    out = {}
    for line in path.read_text().splitlines():
        f = line.split()
        if len(f) == 5 and all("=" in x for x in f[1:]):
            out[f[0]] = {k: int(v) for k, v in (x.split("=") for x in f[1:])}
    return out


ISA_T = isa(prov / "palace-image-isa.txt")
ISA_C = isa(ctl_dir / "provenance" / "palace-image-isa.txt")
host = rule.kv(prov / "host.txt")
gver = rule.kv(prov / "geode-version.txt")
pal = rule.kv(prov / "palace-image-provenance.txt")
pal_img = (prov / "palace-image-id.txt").read_text().strip()
libs_txt = (prov / "palace-image-libs.txt").read_text()
pinning = rule.kv(tuned_dir / "pinning.txt")
c_pinning = rule.kv(ctl_dir / "pinning.txt")
launch = rule.kv(tuned_dir / "launch.txt")
u_gver = rule.kv(untuned_dir / "provenance" / "geode-version.txt")
u_pal = rule.kv(untuned_dir / "provenance" / "palace-image-provenance.txt")
u_launch = rule.kv(untuned_dir / "launch.txt")

ALL = list(T.values()) + list(C.values())
stamps = sorted(s for c in ALL for r in c["runs"] for s in (r["meta"].get("start_utc", ""), r["meta"].get("end_utc", "")) if s)
loads = [float(r["meta"]["loadavg_before"].split()[0]) for c in ALL for r in c["runs"] if "loadavg_before" in r["meta"]]
t_pal = [c for c in T.values() if c["solver"] == "palace"]
ceeds = sorted({r["ceed"] for c in t_pal for r in c["runs"]})
u_ceeds = sorted({r["ceed"] for c in U.values() if c["solver"] == "palace" for r in c["runs"]})
changesets = sorted({r["changeset"] for c in t_pal + [x for x in C.values() if x["solver"] == "palace"] for r in c["runs"]})
blas_libs = sorted({p for _, p in re.findall(r"(lib\w*(?:blas|lapack)\w*\.so[.0-9]*) => (\S+)", libs_txt)})
distro_blas = "no distribution BLAS package installed" in (prov / "palace-image-provenance.txt").read_text()
t_sweep = sorted(s for c in T.values() for r in c["runs"] for s in (r["meta"]["start_utc"], r["meta"]["end_utc"]))
c_sweep = sorted(s for c in C.values() for r in c["runs"] for s in (r["meta"]["start_utc"], r["meta"]["end_utc"]))


def med(xs):
    return statistics.median(xs)


def isa_table(d):
    return "{ " + ", ".join(f"{q(k)} = {{ ymm = {v['ymm']}, evex_regs = {v['evex_regs']}, opmask = {v['opmask']}, zmm = {v['zmm']} }}" for k, v in sorted(d.items())) + " }"


out = []
w = out.append
w("# Tuned-Palace re-timing of the transmon eigenmode benchmark on one AWS m6i.4xlarge,")
w("# with a same-box untuned control and the untuned PR #964 run beside it (issue #927).")
w("# GENERATED by benchmarks/transmon_bench_cpu/summarize_tuned_palace_m6i.py from")
w(f"# {tuned_dir.as_posix()}/ (tuned + untuned_control/)")
w(f"# and {untuned_dir.as_posix()}/ (PR #964); do not edit by hand, rerun the script.")
w("#")
w("# ---------------------------------------------------------------------------")
w("# READ FIRST")
w("#   * NO WINNER IS DECLARED HERE. Ratios are given with the size and config")
w("#     they belong to so nobody has to recompute them; they are not claims.")
w("#   * THE TUNING EFFECT IS [palace.*].untuned_same_box_over_tuned_*: tuned and")
w("#     untuned Palace built and timed on ONE instance, one session, same driver.")
w("#     The PR #964 numbers are another instance of the same type; between the")
w("#     two instances geode itself moves by 6-9% ([geode_session_check]), which")
w("#     is as large as the tuning effect, so a PR #964 / tuned ratio mixes")
w("#     instance and build and is given only for reference.")
w("#   * Same fixture, same configs (Save = 6 and Save = 0), same rank counts (1,")
w("#     8), same pinning, same geode requests as PR #964. Linear.Type is written")
w("#     EXPLICITLY as SuperLU in every Palace cell of this run, tuned and control;")
w("#     PR #964 left the fixture's \"Default\", which resolves to SuperLU at this")
w("#     Palace commit ([palace_linear_solver]).")
w("#   * MPICH ranks busy-wait, so a Palace core_s is close to ranks x wall by")
w("#     construction; core-seconds overstate the CPU work Palace needs.")
w("#   * Save = 0 removes the ParaView field output only; Palace still runs its")
w("#     error estimator, which geode has no counterpart for.")
w("#   * geode is built for the default x86-64 target CPU (no -C target-cpu), as")
w("#     in PR #964: only Palace was tuned for the host.")
w("#   * ONE MESH (133,108 interior DOFs, first order). Nothing here extrapolates")
w("#     to another mesh size.")
w("# ---------------------------------------------------------------------------")
w("")
w("[meta]")
w("issues = [927]")
w(f'measured_date = "{stamps[0][:10]}"  # UTC date the timed runs started; they ran between {stamps[0]} and {stamps[-1]}')
w(f'tuned_sweep_utc = "{t_sweep[0]} to {t_sweep[-1]}"  # geode and tuned Palace, interleaved round-robin')
w(f'untuned_control_sweep_utc = "{c_sweep[0]} to {c_sweep[-1]}"  # after the untuned image was built on the same box')
w(f'geode_commit = "{gver.get("geode_commit", "unknown").split()[0]}"  # main HEAD when the run started')
w(f'pr964_reference = "{untuned_dir.as_posix()} (geode {u_gver.get("geode_commit", "unknown")[:8]}, Palace from reference/palace/docker/Dockerfile, a different m6i.4xlarge instance)"')
w('scope = "one AWS m6i.4xlarge with no other workload, one session; tuned Palace, untuned Palace (same-box control) and geode (direct LU) on the committed 133k transmon fixture"')
w('fixture = "crates/geode-core/tests/fixtures/transmon_smoke.msh"')
w(f'fixture_sha256 = "{host.get("fixture_sha256", "unknown")}"')
w("n_interior_dofs = 133108")
w('geode_binary = "cargo build --release -p geode-core --example transmon_bench; GEODE_INNER=direct (faer sparse LU shift-invert Lanczos, f64); no RUSTFLAGS (default x86-64 target CPU), as in PR #964"')
w('palace_config = "reference/fixtures/transmon_palace/palace_config.json with Model.Mesh, Problem.Output, Solver.Linear.Type (explicit) and, for the Save = 0 cells, Solver.Eigenmode.Save changed; N = 6, Target = 4.5 GHz, Tol, MaxIts, Order 1 as committed"')
w('timing = "/usr/bin/time -v over the full solver process (mesh load + assembly + solve + output). For Palace it wraps `palace -np N` inside the container, so container start-up is excluded"')
w('core_seconds = "user + sys CPU seconds from the same /usr/bin/time: summed over all threads (geode) or over mpirun and all ranks (Palace)"')
w('statistic = "every run is listed; min, median and max over the runs of a cell; spread_pct = (max - min) / median. No mean, no outlier rejection"')
w('order = "runs are sequential, never two at once; within a sweep round-robin over cells (repeat 1 of every cell, then repeat 2, ...)"')
w('generator = "m6i_tuned_box_setup.sh and m6i_tuned_box_bench.sh on the box (the control: untuned_control/untuned_control_build.sh, then the same driver with IMAGE=palace:cpu), then summarize_tuned_palace_m6i.py"')
w("filtered_cells = [" + ", ".join(q(x) for x in FILTERED) + "]  # listed in a cells.tsv but not run: excluded by that sweep's CELL_FILTER")
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
w(f"loadavg_1min_before_range = [{min(loads):.2f}, {max(loads):.2f}]  # each run starts 5 s after the previous one ends, so this is mostly the previous run's tail")
w(f'purchase = "{launch.get("purchase", "unknown")}"')
w('label = "AWS m6i.4xlarge, on-demand (shared tenancy), launched for this run and used for nothing else"')
w(f'session_utc = "{launch.get("launch_utc", "unknown")} to {launch.get("terminate_utc", "unknown")}"  # instance launch to termination')
w(f'pr964_session_utc = "{u_launch.get("launch_utc", "unknown")} to {u_launch.get("terminate_utc", "unknown")}"  # a different m6i.4xlarge instance')
w("")
w("[palace_build]")
w(f'palace_commit = "{pal.get("palace_commit", "unknown").split()[0]}"  # tuned image')
w(f'untuned_control_palace_commit = "{c_pal.get("palace_commit", "unknown").split()[0]}"')
w(f'pr964_palace_commit = "{u_pal.get("palace_commit", "unknown").split()[0]}"')
w(f'palace_changeset_in_logs = {"[" + ", ".join(q(x) for x in changesets) + "]"}  # every Palace run of this session, tuned and control')
w(f'tuned_image_id = "{rule.kv_tokens(pal_img).get("image_id", "unknown")}"  # local builds on the box; not pushed to a registry')
w(f'untuned_control_image_id = "{rule.kv_tokens(c_img).get("image_id", "unknown")}"')
w('tuned_recipe = "reference/palace/docker/Dockerfile.tuned"')
w('untuned_recipe = "reference/palace/docker/Dockerfile (unchanged; the PR #964 recipe; built again on this box for the control)"')
w(f'march = "{pal.get("march", "unknown")}"  # tuned: -march / -mtune in CMAKE_C/CXX/Fortran_FLAGS, which Palace passes to every dependency it builds')
w(f'tuned_cmake_cxx_flags = "{pal.get("CMAKE_CXX_FLAGS:STRING", "unknown")}"')
w(f'untuned_cmake_cxx_flags = "{c_pal.get("CMAKE_CXX_FLAGS:STRING", "unknown")}"  # no -march: compiler default (x86-64) for code compiled by g++ directly, -march=x86-64-v2 where the Rocky MPICH wrapper compiles')
w(f'mpi_wrapper_flags = "{" ".join(t for t in pal.get("mpicxx_show", "").split() if t.startswith(("-march", "-mtune", "-O", "-flto")))}"  # Rocky 9 MPICH wrapper, identical in both images; the wrapper puts these BEFORE the caller\'s flags and GCC keeps the last -march, so the tuned MARCH wins where the wrapper compiles')
w(f'tuned_build_type = "{pal.get("CMAKE_BUILD_TYPE:STRING", "unknown")}"')
w(f'untuned_build_type = "{c_pal.get("CMAKE_BUILD_TYPE:STRING", "unknown")}"')
w(f'tuned_palace_with_libxsmm = "{pal.get("PALACE_WITH_LIBXSMM:BOOL", "unknown")}"')
w(f'untuned_palace_with_libxsmm = "{c_pal.get("PALACE_WITH_LIBXSMM:BOOL", "unknown")}"')
w(f'tuned_libceed_backend_in_logs = {"[" + ", ".join(q(x) for x in ceeds) + "]"}')
w(f'untuned_control_libceed_backend_in_logs = {"[" + ", ".join(q(x) for x in c_ceeds) + "]"}')
w(f'pr964_libceed_backend_in_logs = {"[" + ", ".join(q(x) for x in u_ceeds) + "]"}')
w(f'tuned_blas = "OpenBLAS {pal.get("openblas", "unknown")}"  # built in the builder stage from the pinned release tarball; single-threaded')
w('untuned_blas = "Rocky 9 openblas-serial 0.3.29 (the distribution build: DYNAMIC_ARCH, serial)"')
w("tuned_blas_lapack_resolved = [" + ", ".join(q(p) for p in blas_libs) + "]  # every BLAS / LAPACK path `ldd` resolves for the installed binary and libraries")
w(f"tuned_distribution_blas_in_image = {'false' if distro_blas else 'true'}")
w("# isa: per file, instructions using a ymm register (AVX; none at x86-64 / x86-64-v2),")
w("# using an EVEX-only register (xmm/ymm16-31, zmm) and using an opmask k1-k7 (both")
w("# AVX-512 only). GCC at -march=icelake-server prefers 256-bit vectors, so zmm stays")
w("# low there. OpenBLAS and libxsmm pick kernels at run time, so their counts say")
w("# little about the compile flags.")
w(f"tuned_isa = {isa_table(ISA_T)}")
w(f"untuned_isa = {isa_table(ISA_C)}")
w("")
w("[palace_linear_solver]")
w('fixture_value = "Default"')
w('resolution = "At Palace fba6a5b, palace/utils/iodata.cpp resolves Linear.Type = Default for an eigenmode problem to SuperLU when Palace is built with SuperLU (MFEM_USE_SUPERLU: \\"Prefer sparse direct solver for frequency domain problems if available\\"), else STRUMPACK, MUMPS, AMS. Both recipes build SuperLU, so PR #964 ran SuperLU too: a sparse direct LU (SuperLU_DIST) applied as the preconditioner of GMRES, which therefore converges in one or two iterations per solve. PR #964\'s generated TOML calls this Palace\'s \\"default iterative solver\\"; the Krylov wrapper is iterative but the work is a sparse direct factorization, as geode\'s is."')
w(f"this_run_linear_type = {'[' + ', '.join(sorted({q(r['linear_type']) for c in t_pal + [x for x in C.values() if x["solver"] == "palace"] for r in c['runs']})) + ']'}  # read from each run's generated config; Default only in the control cell below")
w(f"pr964_linear_type = {'[' + ', '.join(sorted({q(r['linear_type']) for c in U.values() if c['solver'] == 'palace' for r in c['runs']})) + ']'}")
ctl = T.get("palace_s4p5_n6_np8_nosave_lindefault")
ref = T.get("palace_s4p5_n6_np8_nosave")
if ctl and ref:
    same_iters = all(r["gmres_iters"] == ref["runs"][0]["gmres_iters"] for r in ctl["runs"] + ref["runs"])
    w(f'default_check_cell = "{ctl["name"]}"  # tuned image, Save = 0, 8 ranks, Linear.Type = Default, n = {len(ctl["runs"])}')
    w(f"default_check_wall_s = {fl(ctl['walls'], 2)}")
    w(f"explicit_superlu_wall_s = {fl(ref['walls'], 2)}")
    w(f"default_check_max_rel_dev_eigenvalues = {max(abs(a - b) / b for r in ctl['runs'] for a, b in zip(r['freqs'], ref['runs'][0]['freqs'])):.1e}  # against explicit-SuperLU run 1")
    w(f"default_check_gmres_iterations_identical = {'true' if same_iters else 'false'}  # per-solve GMRES iteration counts, every Default and explicit-SuperLU run")
    w(f"default_check_gmres_solves = {len(ctl['runs'][0]['gmres_iters'])}")
w("")
w("[pinning]")
w(f'cpuset_1 = "{pinning.get("cpuset_1", "")}"')
w(f'cpuset_8 = "{pinning.get("cpuset_8", "")}"  # one hardware thread on each of the 8 physical cores')
w(f'smt_siblings_of_cpuset_8 = "{pinning.get("smt_siblings_of_cpuset_8", "")}"')
w(f'palace_np1_rank_masks = "{pinning.get("palace_np1_rank_masks", "")}"  # tuned image')
w(f'palace_np8_rank_masks = "{pinning.get("palace_np8_rank_masks", "")}"')
w(f'control_palace_np8_rank_masks = "{c_pinning.get("palace_np8_rank_masks", "")}"  # untuned image')
w('geode_1_thread = "GEODE_NUM_THREADS=1 RAYON_NUM_THREADS=1 taskset -c <cpuset_1>"')
w('geode_8_threads = "GEODE_NUM_THREADS=8 RAYON_NUM_THREADS=8 taskset -c <cpuset_8>"')
w('palace = "docker run --cpuset-cpus <cpuset_1 or cpuset_8>; palace -np N -launcher-args \'-bind-to core\'; OMP_NUM_THREADS=1"')
w("")
w("[oracle]")
w(f'source = "{oracle_csv.as_posix()}"')
w(f"palace_f_ghz = {fl(oracle, 6)}")
w(f"physical_tol_pct = {rule.PHYSICAL_TOL_PCT}")
w("")


def emit_cell(c, build):
    rs = c["runs"]
    w("[[cell]]")
    w(f'name = "{c["name"]}"')
    w(f'build = "{build}"')
    w(f'solver = "{c["solver"]}"')
    w(f'gauge = "{c["gauge"]}"')
    w(f"sigma_ghz = {c['sigma']}")
    w(f"n_modes_requested = {c['n']}")
    w(f"{'threads' if c['solver'] == 'geode' else 'mpi_ranks'} = {c['width']}")
    w(f'pin = "{rs[0]["meta"].get("pin", "")}"')
    w(f'env = "{rs[0]["meta"].get("env", "")}"')
    w("observed_cpus_allowed = [" + ", ".join(q(r["meta"].get("observed_cpus_allowed", "")) for r in rs) + "]")
    w("observed_tasks_max = [" + ", ".join(r["meta"].get("observed_tasks_max", "0") for r in rs) + "]")
    w(f"n_runs = {len(rs)}")
    w(f"exit_codes = [{', '.join(str(r['exit']) for r in rs)}]")
    w(f"modes_f_ghz = {fl(c['freqs'], 6)}  # run 1")
    w("modes_class = [" + ", ".join(f'"{k}"' for k in c["classes"]) + "]")
    w(f"n_physical = {c['n_physical']}  # of Palace's {len(oracle)}")
    w("n_physical_per_run = [" + ", ".join(str(sum(1 for k in rule.classify(r["freqs"], oracle)[0] if k == "physical")) for r in rs) + "]")
    w(f"six_physical_recovered = {'true' if c['six'] else 'false'}")
    matched = [e for _, g, e in c["matches"] if g is not None]
    if matched:
        w(f"physical_max_rel_err_pct = {max(matched):.4f}")
    w(f"wall_s = {fl(c['walls'], 2)}")
    w(f"wall_s_min_median_max = {fl(mmm(c['walls']), 2)}")
    w(f"wall_spread_pct = {spread_pct(c['walls']):.1f}")
    w(f"core_s = {fl(c['cores'], 2)}  # user + sys")
    w(f"core_s_min_median_max = {fl(mmm(c['cores']), 2)}")
    w("cpu_percent = [" + ", ".join(f"{(r['user'] + r['sys']) / r['wall'] * 100:.0f}" for r in rs) + "]")
    w(f"peak_rss_gb = {max(r['rss'] for r in rs) / 1e9:.2f}" + ("  # largest single rank" if c["solver"] == "palace" else ""))
    w("start_utc = [" + ", ".join(q(r["meta"].get("start_utc", "")) for r in rs) + "]")
    w("loadavg_1min_before = [" + ", ".join(r["meta"].get("loadavg_before", "nan").split()[0] for r in rs) + "]")
    if c["solver"] == "geode":
        w(f"solve_s = {fl([r['solve'] for r in rs], 2)}  # solve phase printed by the binary")
    else:
        w(f'image = "{rs[0]["meta"].get("image", "")}"')
        w(f'linear_type = "{rs[0]["linear_type"]}"  # Solver.Linear.Type in the run\'s config')
        w(f"eigenmode_save = {rs[0]['save']}")
        w(f'libceed_backend = "{rs[0]["ceed"]}"')
        w("mpi_processes_in_log = [" + ", ".join(r["mpi"] for r in rs) + "]")
        w("openmp_threads_in_log = [" + ", ".join(r["omp"] for r in rs) + "]")
        w(f"max_rel_dev_from_committed_eig_csv = {max(abs(a - b) / b for r in rs for a, b in zip(r['freqs'], oracle)):.1e}")
        w(f"palace_total_s = {fl([r['phases'].get('Total', float('nan')) for r in rs], 2)}  # Palace's own Elapsed Time Report")
        w(f"palace_error_estimator_s = {fl([estimator_s(r) for r in rs], 2)}  # 'Estimation' + its Construction and Solve rows")
        w(f"palace_paraview_s = {fl([r['phases'].get('Postprocessing / Paraview', 0.0) for r in rs], 2)}")
        w("")
        w("[cell.palace_phase_s]")
        w("# Avg.-over-ranks column of Palace's own Elapsed Time Report, one entry per run.")
        for ph in rs[0]["phases"]:
            w(f"{q(ph)} = {fl([r['phases'].get(ph, float('nan')) for r in rs], 2)}")
    w("")


for c in T.values():
    emit_cell(c, "tuned" if c["solver"] == "palace" else "geode")
for c in C.values():
    emit_cell(c, "untuned_same_box")

w("# ---------------------------------------------------------------------------")
w("# SIDE BY SIDE. Every number below is computed from raw runs: the [[cell]]")
w("# tables above (this session) and the PR #964 tree.")
w("# ---------------------------------------------------------------------------")
for key, name, what in PALACE_CELLS:
    t, c, u = T[name], C[name], U[name]
    w(f"[palace.{key}]")
    w(f'what = "{what}"')
    w(f"n_runs = {{ tuned = {len(t['runs'])}, untuned_same_box = {len(c['runs'])}, pr964 = {len(u['runs'])} }}")
    for tag, x in (("tuned", t), ("untuned_same_box", c), ("pr964", u)):
        w(f"{tag}_wall_s_min_median_max = {fl(mmm(x['walls']), 2)}")
    for tag, x in (("tuned", t), ("untuned_same_box", c), ("pr964", u)):
        w(f"{tag}_core_s_median = {med(x['cores']):.2f}")
    w(f"wall_spread_pct = {{ tuned = {spread_pct(t['walls']):.1f}, untuned_same_box = {spread_pct(c['walls']):.1f}, pr964 = {spread_pct(u['walls']):.1f} }}")
    w(f"untuned_same_box_over_tuned_wall = {med(c['walls']) / med(t['walls']):.3f}  # THE TUNING EFFECT: above 1, the tuned build took less; one instance, one session")
    w(f"untuned_same_box_over_tuned_core_s = {med(c['cores']) / med(t['cores']):.3f}")
    w(f"pr964_over_tuned_wall = {med(u['walls']) / med(t['walls']):.3f}  # reference only: two instances AND two builds")
    w(f"pr964_over_untuned_same_box_wall = {med(u['walls']) / med(c['walls']):.3f}  # same recipe, two instances: instance-to-instance difference for Palace")
    for tag, x in (("tuned", t), ("untuned_same_box", c), ("pr964", u)):
        w(f"{tag}_error_estimator_s_median = {med([estimator_s(r) for r in x['runs']]):.2f}")
    for tag, x in (("tuned", t), ("untuned_same_box", c), ("pr964", u)):
        w(f"{tag}_paraview_s_median = {med([r['phases'].get('Postprocessing / Paraview', 0.0) for r in x['runs']]):.2f}")
    w(f"n_physical = {{ tuned = {t['n_physical']}, untuned_same_box = {c['n_physical']}, pr964 = {u['n_physical']} }}")
    w("")

w("[geode_session_check]")
w("# geode medians in the PR #964 session and in this one, same requests. The geode")
w("# commits differ (see [meta]) and so do the instances; this measures how far the two")
w("# sessions differ, not a geode change. Palace's same-recipe counterpart is")
w("# [palace.*].pr964_over_untuned_same_box_wall.")
for key, stem, _, _ in ROUTES:
    for tw in ("t1", "t8"):
        t, u = T[f"{stem}_{tw}"], U[f"{stem}_{tw}"]
        w(f"{key}_{tw}_wall_s_median = {{ pr964 = {med(u['walls']):.2f}, this_session = {med(t['walls']):.2f}, pr964_over_this = {med(u['walls']) / med(t['walls']):.3f} }}")
w("")

for key, stem, what, caveat in ROUTES:
    g1, g8 = T[f"{stem}_t1"], T[f"{stem}_t8"]
    w(f"[route.{key}]")
    w(f"what = {q(what)}")
    w(f"caveat = {q(caveat)}")
    w(f"n_physical_returned = {g1['n_physical']}  # of {len(oracle)}; 8-thread cell: {g8['n_physical']}")
    w(f"non_physical_modes_returned = {g1['n'] - g1['n_physical']}")
    w(f"like_for_like = {'true' if g1['six'] and g8['six'] else 'false'}")
    for tag, g in (("1_thread", g1), ("8_threads", g8)):
        w(f"geode_{tag}_wall_s_min_median_max = {fl(mmm(g['walls']), 2)}")
        w(f"geode_{tag}_core_s_median = {med(g['cores']):.2f}")
    w("# Palace median / geode median at the same width (1 rank vs 1 thread, 8 vs 8); above 1: the geode cell took less.")
    for pkey, pname, _ in PALACE_CELLS:
        g = g1 if T[pname]["width"] == 1 else g8
        tag = "1_thread" if T[pname]["width"] == 1 else "8_threads"
        for btag, X in (("tuned", T), ("untuned_same_box", C)):
            p = X[pname]
            w(f"{btag}_palace_{pkey}_over_geode_{tag} = {{ wall = {med(p['walls']) / med(g['walls']):.2f}, core_s = {med(p['cores']) / med(g['cores']):.2f} }}")
    w("")

w("[not_done]")
w("items = [")
w('  "MKL: Palace supports it through MKLROOT; this run used OpenBLAS built for the host and did not try MKL",')
w('  "separating the three tuning changes (libxsmm, -march, BLAS): only the combination was built and timed",')
w('  "any Palace linear solver other than SuperLU (what Default resolves to), and any setting other than the PR #964 fixture-request cells",')
w('  "Palace without its error estimator",')
w('  "the alternative-request Palace cells and the off-target request of PR #964 (not re-run with either build)",')
w('  "geode with -C target-cpu for this host: geode stays at the default x86-64 target CPU, as in PR #964",')
w('  "any mesh other than the 133k fixture, and the Lambda Palace-GPU re-time",')
w('  "revising papers/transmon-benchmark main.tex (operator, #593 / #763)",')
w("]")
print("\n".join(out))
