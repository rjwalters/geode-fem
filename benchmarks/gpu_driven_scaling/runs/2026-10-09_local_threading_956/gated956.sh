#!/usr/bin/env bash
# Issue #956, step 3: end to end, main vs the shipped (gated) fix, at sizes
# below SEQUENTIAL_SOLVE_MAX_DIM (above it the fix takes no scope and runs as
# main does), at 1 and 8 threads and the default pool.
#
# usage: gated956.sh <main-binary> <fix-binary> <run-dir>
set -euo pipefail
main_bin=$1
fix_bin=$2
out=$3
mkdir -p "$out/e2e_gated"
leg() { # <bin> <file> <env...>
  local bin=$1 file=$2
  shift 2
  env "$@" "$bin" --ignored --nocapture --test-threads=1 > "$file" 2>&1
}
for rep in 1 2; do
  for t in 1 8 default; do
    th=()
    [ "$t" = default ] || th=(RAYON_NUM_THREADS=$t)
    for build in main fix; do
      bin=$main_bin
      [ "$build" = fix ] && bin=$fix_bin
      leg "$bin" "$out/e2e_gated/complex_n20_${build}_t${t}_rep${rep}.stdout" \
        DIAG956_SITE=complex DIAG956_N=20 DIAG956_NEV=20 ${th[@]+"${th[@]}"}
      leg "$bin" "$out/e2e_gated/ams_n12_${build}_t${t}_rep${rep}.stdout" \
        DIAG956_SITE=ams DIAG956_N=12 DIAG956_NEV=4 GEODE_COARSE=direct ${th[@]+"${th[@]}"}
      leg "$bin" "$out/e2e_gated/transient_n10_${build}_t${t}_rep${rep}.stdout" \
        DIAG956_SITE=transient DIAG956_N=10 DIAG956_STEPS=400 ${th[@]+"${th[@]}"}
    done
  done
done
