#!/usr/bin/env bash
# Issue #956, step 4 (PR #1010 review): the 8-thread per-call crossover again
# on the tree that ships, which is on crates.io faier 0.25.2 (steps 1 to 3 ran
# on the 0.24.4 git pin, see meta.txt). Sizes bracket SEQUENTIAL_SOLVE_MAX_DIM
# for the complex LU and the transient step, plus one AMS stand-in size.
#
# usage: spotcheck956.sh <gate-off-binary> <run-dir>
#
# Binary: the harness (diag956_threading.rs in this directory, copied into
# crates/geode-core/tests/) built with
#   cargo test --release -p geode-core --test diag956_threading --no-run
# on spotcheck_faier_0_25_2/meta.txt's tree_commit with
# spotcheck_faier_0_25_2/gate_off.patch applied. The patch sets
# SEQUENTIAL_SOLVE_MAX_DIM to 0, so TransientStepper::step takes no scope of
# its own and the harness's pool mode is the ambient parallelism at every
# size, as it was on the main build of steps 1 and 2. The complex and AMS
# per-call legs call faer's LU directly and do not depend on the patch.
set -euo pipefail
bin=$1
out=$2/spotcheck_faier_0_25_2
mkdir -p "$out/percall"
percall() { # <site> <n> <rep>
  env DIAG956_MODE=percall DIAG956_SITE=$1 DIAG956_N=$2 DIAG956_REPS=400 \
    DIAG956_STEPS=200 RAYON_NUM_THREADS=8 \
    "$bin" --ignored --nocapture --test-threads=1 \
    > "$out/percall/${1}_n${2}_t8_rep${3}.stdout" 2>&1
}
for rep in 1 2 3; do
  for n in 18 20 22 24; do percall complex $n $rep; done
  for n in 10 11 12 16; do percall transient $n $rep; done
  percall ams 20 $rep
done
