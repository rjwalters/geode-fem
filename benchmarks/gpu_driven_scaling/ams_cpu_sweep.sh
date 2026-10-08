#!/usr/bin/env bash
# CPU driver for the Jacobi-vs-AMS driven-solve comparison (issue #930).
#
# Runs crates/geode-core/tests/gpu_driven_scaling.rs once per (size, config),
# each in its OWN process, so the peak RSS and CPU time reported by
# /usr/bin/time belong to one solver configuration at one mesh size:
#
#   n<N>_direct   Direct (faer sparse LU)            - the accuracy reference
#   n<N>_jacobi   assembled COCG + Jacobi  (config 2)
#   n<N>_ams      assembled COCG + AMS     (config 5, issue #930)
#   n<N>_xcheck   all three in one process - only for the in-harness
#                 full-field rel-L2 of each iterative solution vs Direct
#
# Legs run sequentially, never two at once. Each leg writes into <out-dir>:
#   <leg>.stdout  the test's stdout (the TOML fragment; named .stdout so the
#                 benchmark-provenance guard, which classifies every *.toml
#                 under benchmarks/, skips it)
#   <leg>.err     stderr (per-size progress, drift / DNF notes)
#   <leg>.time    /usr/bin/time (-l on macOS, -v on Linux): wall, user, sys,
#                 max RSS of the test process
#   <leg>.meta    start/end time, load average before and after, commit, env
# and once per sweep: host.txt (CPU, core count, memory, OS, rustc).
#
# usage: ams_cpu_sweep.sh <out-dir> <n> [<n> ...]
#   e.g. ams_cpu_sweep.sh benchmarks/gpu_driven_scaling/runs/2026-10-07_local_ams 15 20 24
# Env: GEODE_DIR (checkout, default: the repo this script is in),
#      CARGO_TARGET_DIR (honoured by cargo as usual),
#      LEG_TIMEOUT_S (per-leg wall-clock cap, default 5400),
#      SKIP_XCHECK=1 (skip the combined cross-check leg),
#      SKIP_DIRECT=1 (skip the Direct legs: sizes whose LU does not fit; the
#      port voltage then has no Direct reference),
#      RAYON_NUM_THREADS (inherited by every leg and recorded in <leg>.meta.
#      Before issue #946 the AMS legs needed it set to 1 for timings on a
#      shared host: the V-cycle's small sparse triangular solves fanned out to
#      the rayon pool and spent most of the leg in system time. The AMS Krylov
#      solve now runs those solves sequentially at any setting; the Direct
#      legs still use every thread for the LU factorization).
# Then: summarize_ams_local.py <out-dir>
set -euo pipefail
OUT=$1; shift
here=$(cd "$(dirname "$0")" && pwd)
GEODE_DIR="${GEODE_DIR:-$(cd "$here/../.." && pwd)}"
LEG_TIMEOUT_S="${LEG_TIMEOUT_S:-5400}"
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
cd "$GEODE_DIR"

# Build first (excluded from timing).
cargo test --release -p geode-core --test gpu_driven_scaling --no-run > "$OUT/build.log" 2>&1
bin=$(sed -nE 's/.*Executable .*\((.*gpu_driven_scaling-[0-9a-f]+)\).*/\1/p' "$OUT/build.log" | tail -1)
[ -x "$bin" ] || { echo "could not find the test binary in $OUT/build.log" >&2; exit 1; }

os=$(uname -s)
if [ "$os" = Darwin ]; then
  tflag=-l
  loadavg() { sysctl -n vm.loadavg | tr -d '{}' | xargs; }
  {
    echo "os=$(sw_vers -productName) $(sw_vers -productVersion) ($(uname -m))"
    echo "model=$(sysctl -n hw.model)"
    echo "cpu=$(sysctl -n machdep.cpu.brand_string)"
    echo "logical_cpus=$(sysctl -n hw.ncpu)"
    echo "ram_gib=$(( $(sysctl -n hw.memsize) / 1073741824 ))"
  } > "$OUT/host.txt"
else
  tflag=-v
  loadavg() { cut -d' ' -f1-3 /proc/loadavg; }
  {
    echo "os=$(. /etc/os-release && echo "$PRETTY_NAME") ($(uname -m))"
    echo "model=$(cat /sys/devices/virtual/dmi/id/product_name 2>/dev/null || echo unknown)"
    echo "cpu=$(sed -n 's/^model name[^:]*: //p' /proc/cpuinfo | head -1)"
    echo "logical_cpus=$(nproc)"
    echo "ram_gib=$(( $(sed -n 's/^MemTotal: *\([0-9]*\).*/\1/p' /proc/meminfo) / 1048576 ))"
  } > "$OUT/host.txt"
fi
echo "rustc=$(rustc --version)" >> "$OUT/host.txt"

run_leg() {
  local leg=$1 n=$2 cfgs=$3 rc=0 pid t0
  {
    # The binary's name only: its directory is this machine's cargo target.
    echo "leg=$leg bin=<cargo-target>/release/deps/$(basename "$bin")"
    echo "start=$(date -u +%FT%TZ)"
    echo "loadavg_start=$(loadavg)"
    echo "rayon_num_threads=${RAYON_NUM_THREADS:-unset}"
    echo "git=$(git rev-parse HEAD) dirty=$(git status --porcelain --untracked-files=no | wc -l | xargs)"
    echo "env: GEODE_SCALING_SIZES=$n GEODE_SCALING_CONFIGS=$cfgs GEODE_SCALING_REPS=0 GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1 GEODE_SCALING_SPLIT_SETUP=1"
  } > "$OUT/$leg.meta"
  env GEODE_SCALING_SIZES="$n" GEODE_SCALING_CONFIGS="$cfgs" GEODE_SCALING_REPS=0 \
    GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1 GEODE_SCALING_SPLIT_SETUP=1 \
    /usr/bin/time "$tflag" -o "$OUT/$leg.time" \
    "$bin" --ignored --nocapture --test-threads 1 \
    > "$OUT/$leg.stdout" 2> "$OUT/$leg.err" &
  pid=$!
  t0=$SECONDS
  # Per-leg wall-clock cap: kill only this leg's own process tree.
  while kill -0 "$pid" 2>/dev/null; do
    if [ $((SECONDS - t0)) -ge "$LEG_TIMEOUT_S" ]; then
      pkill -P "$pid" 2>/dev/null || true
      kill "$pid" 2>/dev/null || true
      echo "timeout_s=$LEG_TIMEOUT_S" >> "$OUT/$leg.meta"
      break
    fi
    sleep 1
  done
  wait "$pid" || rc=$?
  {
    echo "end=$(date -u +%FT%TZ)"
    echo "loadavg_end=$(loadavg)"
    echo "rc=$rc"
  } >> "$OUT/$leg.meta"
  echo "$leg rc=$rc"
}

for n in "$@"; do
  [ -n "${SKIP_DIRECT:-}" ] || run_leg "n${n}_direct" "$n" direct
  run_leg "n${n}_jacobi" "$n" iterative
  run_leg "n${n}_ams" "$n" iterative_ams
  if [ -z "${SKIP_XCHECK:-}" ] && [ -z "${SKIP_DIRECT:-}" ]; then
    run_leg "n${n}_xcheck" "$n" direct,iterative,iterative_ams
  fi
done
echo SWEEP_DONE
