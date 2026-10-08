#!/usr/bin/env bash
# #930 at-scale chain, part C (after Palace): two more drive frequencies at
# 102k and 463k edges, and Direct at 338k edges for the port-voltage check.
set -u
S="${S:-$HOME/bench}"
R="${R:-$HOME/run930}"
export GEODE_DIR="${GEODE_DIR:-$HOME/geode}" LEG_TIMEOUT_S=3600 SKIP_XCHECK=1
sweep() { # <tree> <cpu-list> <threads> <legs> <sizes...>
  local tree=$1 cpus=$2 thr=$3 legs=$4; shift 4
  TASKSET_CPUS=$cpus RAYON_NUM_THREADS=$thr LEGS=$legs \
    "$S/ams_cpu_sweep.sh" "$R/$tree" "$@" 2>&1 | sed -u "s|^|$tree: |"
}
for w in 0.05 0.20; do
  tag=w${w/./p}
  GEODE_SCALING_OMEGA=$w sweep omega/t1_$tag 0   1 "jacobi ams" 24 40
  GEODE_SCALING_OMEGA=$w sweep omega/t8_$tag 0-7 8 "direct" 24
done
sweep direct_large/t8 0-7 8 "direct" 36
echo CHAIN_C_DONE
