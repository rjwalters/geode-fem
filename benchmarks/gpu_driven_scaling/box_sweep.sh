#!/usr/bin/env bash
# On-box driver for the #520 larger-mesh GPU-vs-CPU driven-solve sweep.
#
# Runs ONE leg of crates/geode-core/tests/gpu_driven_scaling.rs per call, on
# the same host, sequentially (never two legs at once: the GPU leg is
# host/launch-bound, so a concurrent CPU leg would contaminate it). Each leg
# writes:
#   $OUT/<leg>.stdout   the test's stdout (contains the TOML fragment; named
#                       .stdout so the benchmark-provenance guard, which
#                       classifies every *.toml under benchmarks/, skips it)
#   $OUT/<leg>.err      stderr (per-size progress, drift/DNF notes)
#   $OUT/<leg>.time     /usr/bin/time -v (wall, max RSS of the test process)
#   $OUT/<leg>.gpumem   nvidia-smi memory.used every 200 ms (GPU legs)
#
# usage: box_sweep.sh <leg-name> <cpu|cuda> [extra VAR=value ...]
#   e.g. box_sweep.sh gpu_mf cuda GEODE_SCALING_SIZES=15,24,30 \
#          GEODE_SCALING_CONFIGS=matrix_free GEODE_SCALING_REPS=1 \
#          GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1
# Env: GEODE_DIR (checkout, default ~/geode-520), OUT (default ~/sweep),
#      CUDA_PATH (default /usr — Lambda Stack ships CUDA headers in
#      /usr/include, not /usr/local/cuda/include).
set -euo pipefail
leg=$1; kind=$2; shift 2
GEODE_DIR="${GEODE_DIR:-$HOME/geode-520}"
OUT="${OUT:-$HOME/sweep}"
mkdir -p "$OUT"
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
cd "$GEODE_DIR"

feat=(); tdir=target
if [ "$kind" = cuda ]; then feat=(--features cuda); tdir=target-cuda; fi
# Build first (excluded from timing).
cargo test --release -p geode-core "${feat[@]}" --target-dir "$tdir" \
  --test gpu_driven_scaling --no-run > "$OUT/$leg.build" 2>&1
bin=$(grep -oE "$tdir/release/deps/gpu_driven_scaling-[0-9a-f]+" "$OUT/$leg.build" | tail -1)

poll=""
if [ "$kind" = cuda ]; then
  nvidia-smi --query-gpu=memory.used,utilization.gpu --format=csv,noheader,nounits -lms 200 \
    > "$OUT/$leg.gpumem" 2>/dev/null &
  poll=$!
fi
{
  echo "leg=$leg kind=$kind bin=$bin"
  echo "start=$(date -u +%FT%TZ)"
  echo "git=$(git rev-parse HEAD) dirty=$(git status --porcelain | wc -l)"
  echo "env: $*"
} > "$OUT/$leg.meta"
rc=0
env CUDA_PATH="${CUDA_PATH:-/usr}" "$@" /usr/bin/time -v -o "$OUT/$leg.time" \
  "$bin" --ignored --nocapture --test-threads 1 \
  > "$OUT/$leg.stdout" 2> "$OUT/$leg.err" || rc=$?
[ -n "$poll" ] && { kill "$poll" 2>/dev/null || true; wait "$poll" 2>/dev/null || true; }
{ echo "end=$(date -u +%FT%TZ)"; echo "rc=$rc"; } >> "$OUT/$leg.meta"
echo "$leg rc=$rc"
