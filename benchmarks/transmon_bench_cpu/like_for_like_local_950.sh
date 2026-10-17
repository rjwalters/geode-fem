#!/usr/bin/env bash
# Issue #950 before/after sweep on the committed 133k transmon fixture,
# geode only, local host. Same per-run discipline as like_for_like_local.sh
# (one process per run under /usr/bin/time, sequential, single-threaded), with
# one extra column: which binary runs the cell.
#
#   bin=main  a transmon_bench built from origin/main BEFORE #950
#             (MAIN_BIN, its commit given as MAIN_COMMIT). There,
#             GEODE_GAUGE=port_aware is the #514 junction-extract route with no
#             null-mode filter.
#   bin=head  the transmon_bench built from this checkout (GEODE_DIR). There,
#             GEODE_GAUGE=port_aware is the #950 port-subspace route (one
#             factorization, null modes dropped) and port_aware_extract is the
#             #514 route with the null-mode filter added.
#
#   cell                          bin   gauge               sigma modes runs
#   ungauged_s4p5_n6              head  none                4.5   6     3   ratio baseline
#   main_portaware_s4p5_n7        main  port_aware          4.5   7     1   before: request N = 7
#   main_portaware_s4p5_n8        main  port_aware          4.5   8     3   before: smallest N with all six
#   portaware_extract_s4p5_n7     head  port_aware_extract  4.5   7     1   piece 1 alone (2 factorizations)
#   portaware_s4p5_n6             head  port_aware          4.5   6     1   after: N = 6
#   portaware_s4p5_n7             head  port_aware          4.5   7     3   after: N = 7
#
# usage: MAIN_BIN=<path> MAIN_COMMIT=<sha> like_for_like_local_950.sh <out-dir>
# Then: summarize_like_for_like.py <2026-10-08 run-dir> <palace-eig.csv> <out-dir>
set -euo pipefail
OUT=$1
: "${MAIN_BIN:?set MAIN_BIN to a transmon_bench built from origin/main before #950}"
: "${MAIN_COMMIT:?set MAIN_COMMIT to the commit MAIN_BIN was built from}"
here=$(cd "$(dirname "$0")" && pwd)
GEODE_DIR="${GEODE_DIR:-$(cd "$here/../.." && pwd)}"
RUN_TIMEOUT_S="${RUN_TIMEOUT_S:-1500}"
THREADS="${GEODE_NUM_THREADS:-1}"
RAYON="${RAYON_NUM_THREADS:-1}"
mkdir -p "$OUT/raw"
OUT=$(cd "$OUT" && pwd)
cd "$GEODE_DIR"

cargo build --release -p geode-core --example transmon_bench > "$OUT/build.log" 2>&1
target_dir=$(cargo metadata --format-version 1 --no-deps | sed -nE 's/.*"target_directory":"([^"]*)".*/\1/p')
head_bin="$target_dir/release/examples/transmon_bench"
[ -x "$head_bin" ] || { echo "could not find $head_bin" >&2; exit 1; }
[ -x "$MAIN_BIN" ] || { echo "MAIN_BIN $MAIN_BIN is not executable" >&2; exit 1; }

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
  echo "main_commit=$MAIN_COMMIT"
  echo "main_bin_sha256=$(shasum -a 256 "$MAIN_BIN" | cut -d' ' -f1)"
  echo "head_bin_sha256=$(shasum -a 256 "$head_bin" | cut -d' ' -f1)"
  echo "geode_num_threads=$THREADS"
  echo "rayon_num_threads=$RAYON"
  echo "fixture_sha256=$(shasum -a 256 crates/geode-core/tests/fixtures/transmon_smoke.msh | cut -d' ' -f1)"
} >> "$OUT/host.txt"

tmo=$(command -v timeout || command -v gtimeout || true)

# cell <tab> gauge <tab> sigma_ghz <tab> n_modes <tab> runs <tab> bin
cat > "$OUT/cells.tsv" <<'CELLS'
ungauged_s4p5_n6	none	4.5	6	3	head
main_portaware_s4p5_n7	port_aware	4.5	7	1	main
main_portaware_s4p5_n8	port_aware	4.5	8	3	main
portaware_extract_s4p5_n7	port_aware_extract	4.5	7	1	head
portaware_s4p5_n6	port_aware	4.5	6	1	head
portaware_s4p5_n7	port_aware	4.5	7	3	head
CELLS

while IFS=$'\t' read -r cell gauge sigma nmodes runs which; do
  if [ "$which" = main ]; then bin=$MAIN_BIN; else bin=$head_bin; fi
  for i in $(seq 1 "$runs"); do
    stem="$OUT/raw/${cell}_run${i}"
    {
      echo "cell=$cell"
      echo "run=$i"
      echo "bin=$which"
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
