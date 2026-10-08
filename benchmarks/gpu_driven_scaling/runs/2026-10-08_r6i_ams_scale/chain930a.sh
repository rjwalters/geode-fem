#!/usr/bin/env bash
# #930 at-scale chain, part A: geode CPU f64, Jacobi vs AMS (vs Direct at the
# sizes where one LU is cheap), on an otherwise idle r6i.4xlarge.
#
# One process per (size, config), sequential, pinned with taskset:
#   t1_*  CPU 0 only          (RAYON_NUM_THREADS=1)
#   t8_*  CPUs 0-7            (RAYON_NUM_THREADS=8; the 8 physical cores, one
#                              hyperthread each: lscpu -e in host_extra.txt)
# Order follows the budget priority: (1) Jacobi + AMS at all five sizes on one
# thread, (2) the same on eight, then the Direct references and the repeats.
# S = directory holding ams_cpu_sweep.sh (this branch's copy); the measured
# code is the clean checkout in GEODE_DIR.
set -u
S="${S:-$HOME/bench}"
R="${R:-$HOME/run930}"
export GEODE_DIR="${GEODE_DIR:-$HOME/geode}" LEG_TIMEOUT_S=3600
ALL="15 20 24 36 40"
SMALL="15 20 24"
sweep() { # <tree> <cpu-list> <threads> <legs> <sizes...>
  local tree=$1 cpus=$2 thr=$3 legs=$4; shift 4
  TASKSET_CPUS=$cpus RAYON_NUM_THREADS=$thr LEGS=$legs \
    "$S/ams_cpu_sweep.sh" "$R/$tree" "$@" 2>&1 | sed "s|^|$tree: |"
}
sweep t1_rep1 0   1 "jacobi ams" $ALL
sweep t8_rep1 0-7 8 "jacobi ams" $ALL
echo PRIORITY_1_2_DONE
sweep t1_rep1 0   1 "direct" $SMALL
sweep t8_rep1 0-7 8 "direct xcheck" $SMALL
sweep t1_rep2 0   1 "jacobi ams" $ALL
sweep t8_rep2 0-7 8 "jacobi ams" $ALL
sweep t1_rep2 0   1 "direct" $SMALL
sweep t8_rep2 0-7 8 "direct" $SMALL
echo CHAIN_A_DONE
