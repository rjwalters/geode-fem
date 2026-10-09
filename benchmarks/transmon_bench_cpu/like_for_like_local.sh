#!/usr/bin/env bash
# geode-only "cost to return the same six physical modes as Palace" sweep on
# the committed 133k transmon fixture (issue #927).
#
# The timed transmon benchmarks ask both solvers for "6 eigenmodes near
# 4.5 GHz". Palace returns six physical modes; geode's ungauged shift-invert
# returns the six eigenvalues nearest the shift, of which one is physical. This
# script runs crates/geode-core/examples/transmon_bench.rs in the
# configurations below, each run in its OWN process under /usr/bin/time, so the
# wall clock, CPU time and peak RSS belong to that configuration alone.
#
#   cell                gauge       sigma   modes  runs   purpose
#   ungauged_s4p5_n6    none        4.5     6      3      the timed configuration (audit)
#   ungauged_s4p5_n12   none        4.5     12     1      option 2, same target
#   ungauged_s4p5_n24   none        4.5     24     1      option 2, same target
#   ungauged_s4p5_n29   none        4.5     29     1      option 2, same target
#   ungauged_s4p5_n30   none        4.5     30     3      option 2, same target
#   ungauged_s4p5_n48   none        4.5     48     1      option 2, same target
#   ungauged_s20_n6     none        20      6      3      option 2, shift moved
#   ungauged_s20_n7     none        20      7      1      option 2, shift moved
#   ungauged_s20_n12    none        20      12     1      the transmon_eigen gate's request
#   portaware_s4p5_n6   port_aware  4.5     6      1      option 1, same target
#   portaware_s4p5_n7   port_aware  4.5     7      1      option 1, same target
#   portaware_s4p5_n8   port_aware  4.5     8      3      option 1, same target
#
# GEODE_GAUGE=port_aware selects, since PR #1002 (#950), the single-factorization
# port-subspace projection (solve_transmon_eigenmodes_port_subspace): one
# factorization of K - sigma M, no junction extract. It returns the six physical
# modes from a 7-mode request, plus one non-physical mode, the 3.45 GHz port mode
# (#1003). Before PR #1002 the same value selected the issue #514 route (junction
# extract + projected band, two factorizations), which is now
# GEODE_GAUGE=port_aware_extract. The portaware_* rows of the #927 sweep in
# results_like_for_like_local.toml ran that #514 route; they stay attributable
# through the geode commit the sweep recorded (7563b1b6). A re-run of this script times
# the port-subspace route instead. like_for_like_local_950.sh runs both routes
# side by side.
#
# Every run is SINGLE-THREADED by default (GEODE_NUM_THREADS=1 caps faer's LU
# pool, RAYON_NUM_THREADS=1 caps the rest). On a shared, oversubscribed host
# multi-threaded runs contend for cores; one thread makes a run's CPU seconds
# (user + sys) a repeatable cost. It also means these are one-core times.
#
# Runs are sequential, never two at once. Each run writes into <out-dir>/raw:
#   <cell>_run<i>.log   transmon_bench stdout (config banner, modes, phase times)
#   <cell>_run<i>.time  stderr: /usr/bin/time (-l on macOS, -v on Linux)
#   <cell>_run<i>.meta  start/end time (UTC), load average before/after, env
# and once per sweep: <out-dir>/host.txt, <out-dir>/cells.tsv, <out-dir>/build.log.
#
# usage: like_for_like_local.sh <out-dir>
#   e.g. like_for_like_local.sh benchmarks/transmon_bench_cpu/runs/2026-10-08_local_like_for_like
# Env: GEODE_DIR (checkout, default: the repo this script is in),
#      CARGO_TARGET_DIR (honoured by cargo as usual),
#      GEODE_NUM_THREADS and RAYON_NUM_THREADS (default 1 each; recorded in
#      host.txt and every <cell>_run<i>.meta),
#      RUN_TIMEOUT_S (per-run wall-clock cap, default 1500).
# Then: summarize_like_for_like.py <out-dir> reference/fixtures/transmon_palace/results_p1/eig.csv
set -euo pipefail
OUT=$1
here=$(cd "$(dirname "$0")" && pwd)
GEODE_DIR="${GEODE_DIR:-$(cd "$here/../.." && pwd)}"
RUN_TIMEOUT_S="${RUN_TIMEOUT_S:-1500}"
THREADS="${GEODE_NUM_THREADS:-1}"
RAYON="${RAYON_NUM_THREADS:-1}"
mkdir -p "$OUT/raw"
OUT=$(cd "$OUT" && pwd)
cd "$GEODE_DIR"

# Build first (excluded from timing).
cargo build --release -p geode-core --example transmon_bench > "$OUT/build.log" 2>&1
target_dir=$(cargo metadata --format-version 1 --no-deps | sed -nE 's/.*"target_directory":"([^"]*)".*/\1/p')
bin="$target_dir/release/examples/transmon_bench"
[ -x "$bin" ] || { echo "could not find $bin" >&2; exit 1; }

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
{
  echo "rustc=$(rustc --version)"
  echo "geode_commit=$(git rev-parse HEAD) dirty=$(git status --porcelain --untracked-files=no | wc -l | xargs)"
  echo "geode_num_threads=$THREADS"
  echo "rayon_num_threads=$RAYON"
  echo "fixture_sha256=$(shasum -a 256 crates/geode-core/tests/fixtures/transmon_smoke.msh | cut -d' ' -f1)"
} >> "$OUT/host.txt"

tmo=$(command -v timeout || command -v gtimeout || true)

# cell <tab> gauge <tab> sigma_ghz <tab> n_modes <tab> runs
cat > "$OUT/cells.tsv" <<'CELLS'
ungauged_s4p5_n6	none	4.5	6	3
ungauged_s4p5_n12	none	4.5	12	1
ungauged_s4p5_n24	none	4.5	24	1
ungauged_s4p5_n29	none	4.5	29	1
ungauged_s4p5_n30	none	4.5	30	3
ungauged_s4p5_n48	none	4.5	48	1
ungauged_s20_n6	none	20	6	3
ungauged_s20_n7	none	20	7	1
ungauged_s20_n12	none	20	12	1
portaware_s4p5_n6	port_aware	4.5	6	1
portaware_s4p5_n7	port_aware	4.5	7	1
portaware_s4p5_n8	port_aware	4.5	8	3
CELLS

while IFS=$'\t' read -r cell gauge sigma nmodes runs; do
  for i in $(seq 1 "$runs"); do
    stem="$OUT/raw/${cell}_run${i}"
    {
      echo "cell=$cell"
      echo "run=$i"
      echo "env=GEODE_INNER=direct GEODE_GAUGE=$gauge GEODE_SIGMA_GHZ=$sigma GEODE_NMODES=$nmodes GEODE_NUM_THREADS=$THREADS RAYON_NUM_THREADS=$RAYON"
      echo "start_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
      echo "loadavg_before=$(loadavg)"
    } > "$stem.meta"
    rc=0
    env GEODE_INNER=direct GEODE_GAUGE="$gauge" GEODE_SIGMA_GHZ="$sigma" \
        GEODE_NMODES="$nmodes" GEODE_NUM_THREADS="$THREADS" RAYON_NUM_THREADS="$RAYON" \
        ${tmo:+$tmo "$RUN_TIMEOUT_S"} /usr/bin/time "$tflag" "$bin" \
        > "$stem.log" 2> "$stem.time" < /dev/null || rc=$?
    {
      echo "end_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
      echo "loadavg_after=$(loadavg)"
      echo "exit=$rc"
    } >> "$stem.meta"
    echo "$cell run $i: exit $rc" >&2
  done
done < "$OUT/cells.tsv"
echo "done: $OUT" >&2
