#!/usr/bin/env bash
# Issue #956, step 1: per-call (pool vs sequential) and end-to-end legs of the
# diag956_threading harness, one process per leg.
#
# usage: sweep956.sh <main-binary> <ungated-binary> <run-dir>
#
# Binaries: the harness (diag956_threading.rs in this directory, copied into
# crates/geode-core/tests/) built with
#   cargo test --release -p geode-core --test diag956_threading --no-run
# on main (main_commit in meta.txt) and on the first fix commit, which took the
# sequential scope at every size ("ungated", ungated_commit in meta.txt).
#
# Per-call legs run the MAIN build only: the harness times each solve on the
# ambient parallelism (pool) and under a SequentialSolveScope (seq), which is
# what the fix does at each site.
set -euo pipefail
main_bin=$1
ungated_bin=$2
out=$3
mkdir -p "$out/percall" "$out/e2e"

{
  echo "cpu=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m)"
  echo "logical_cpus=$(sysctl -n hw.ncpu 2>/dev/null || nproc)"
  echo "os=$(uname -sr)"
} > "$out/host.txt"

leg() { # <bin> <file> <env...>
  local bin=$1 file=$2
  shift 2
  env "$@" "$bin" --ignored --nocapture --test-threads=1 > "$file" 2>&1
}

# cube_tet_mesh(35) and above panic inside the P1 assembly (ndarray index out
# of bounds), so 34 is the largest P1 size.
for threads in 1 8; do
  for n in 20 30 34; do
    leg "$main_bin" "$out/percall/complex_n${n}_t${threads}.stdout" \
      DIAG956_MODE=percall DIAG956_SITE=complex DIAG956_N=$n DIAG956_REPS=400 RAYON_NUM_THREADS=$threads
  done
  for n in 14 30 34; do
    leg "$main_bin" "$out/percall/ams_n${n}_t${threads}.stdout" \
      DIAG956_MODE=percall DIAG956_SITE=ams DIAG956_N=$n DIAG956_REPS=400 RAYON_NUM_THREADS=$threads
  done
  for n in 10 16 24; do
    leg "$main_bin" "$out/percall/transient_n${n}_t${threads}.stdout" \
      DIAG956_MODE=percall DIAG956_SITE=transient DIAG956_N=$n DIAG956_STEPS=200 RAYON_NUM_THREADS=$threads
  done
done

for rep in 1 2; do
  for threads in 1 8; do
    for build in main ungated; do
      bin=$main_bin
      [ "$build" = ungated ] && bin=$ungated_bin
      leg "$bin" "$out/e2e/complex_n30_${build}_t${threads}_rep${rep}.stdout" \
        DIAG956_SITE=complex DIAG956_N=30 DIAG956_NEV=20 RAYON_NUM_THREADS=$threads
      leg "$bin" "$out/e2e/ams_n12_${build}_t${threads}_rep${rep}.stdout" \
        DIAG956_SITE=ams DIAG956_N=12 DIAG956_NEV=4 GEODE_COARSE=direct RAYON_NUM_THREADS=$threads
      leg "$bin" "$out/e2e/transient_n16_${build}_t${threads}_rep${rep}.stdout" \
        DIAG956_SITE=transient DIAG956_N=16 DIAG956_STEPS=400 RAYON_NUM_THREADS=$threads
    done
  done
done
