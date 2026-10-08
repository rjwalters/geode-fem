#!/usr/bin/env bash
# #520 chain 2: per-omega setup isolation for the capped legs, then Palace driven.
set -u
while [ ! -f ~/sweep/CHAIN_DONE ]; do sleep 10; done
C="GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1"
# ITER_MAX=1 => attempt time = fixture-independent prepare_at setup + 1 iteration
~/box_sweep.sh S1_gpu_mf_cap1 cuda GEODE_SCALING_SIZES=30,40 GEODE_SCALING_CONFIGS=matrix_free GEODE_SCALING_REPS=0 GEODE_SCALING_ITER_MAX=1 $C
~/box_sweep.sh S2_cpu_mf_cap1 cpu GEODE_SCALING_SIZES=30,40 GEODE_SCALING_CONFIGS=matrix_free GEODE_SCALING_REPS=0 GEODE_SCALING_ITER_MAX=1 $C
# Palace driven (Palace fba6a5b CUDA image from #519): S11 agreement sweep + timing
export OUT=$HOME/sweep/palace
for n in 15 24 30; do ~/pd/box_palace_driven.sh $n GPU 1 1; ~/pd/box_palace_driven.sh $n CPU 8 1; done
for n in 20 40; do ~/pd/box_palace_driven.sh $n GPU 1 3; ~/pd/box_palace_driven.sh $n CPU 8 3; done
echo DONE > ~/sweep/CHAIN2_DONE
