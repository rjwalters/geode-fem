#!/usr/bin/env bash
# #520 sweep chain (revised 2026-10-07 22:17Z after the n=15 GPU point showed
# ~36 ms/iter: full solves to n=24, 300-iteration capped runs at n=30/40).
set -u
C="GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1"
# GPU f32 matrix-free: full solves (warm-up only) to ~105k edges ...
~/box_sweep.sh A1_gpu_mf_full cuda GEODE_SCALING_SIZES=20,24 GEODE_SCALING_CONFIGS=matrix_free GEODE_SCALING_REPS=0 $C GEODE_SCALING_EXPORT_DIR=$HOME/sweep/meshes
# ... and per-iteration cost + memory at ~205k / ~490k edges (300-iteration cap => DNF cell)
~/box_sweep.sh A2_gpu_mf_cap300 cuda GEODE_SCALING_SIZES=30,40 GEODE_SCALING_CONFIGS=matrix_free GEODE_SCALING_REPS=0 GEODE_SCALING_ITER_MAX=300 $C GEODE_SCALING_EXPORT_DIR=$HOME/sweep/meshes
# CPU f64 matrix-free (ndarray): same split
~/box_sweep.sh C1_cpu_mf_full cpu GEODE_SCALING_SIZES=15,20,24 GEODE_SCALING_CONFIGS=matrix_free GEODE_SCALING_REPS=0 $C
~/box_sweep.sh C2_cpu_mf_cap300 cpu GEODE_SCALING_SIZES=30,40 GEODE_SCALING_CONFIGS=matrix_free GEODE_SCALING_REPS=0 GEODE_SCALING_ITER_MAX=300 $C
# CPU f64 direct + assembled COCG (accuracy reference + iteration counts at every size)
~/box_sweep.sh B1_cpu_direct_iter cpu GEODE_SCALING_SIZES=15,20,24,30 GEODE_SCALING_CONFIGS=direct,iterative GEODE_SCALING_REPS=0 $C
~/box_sweep.sh B2_cpu_iter_large cpu GEODE_SCALING_SIZES=36,40 GEODE_SCALING_CONFIGS=iterative GEODE_SCALING_REPS=0 $C
echo CHAIN_DONE > ~/sweep/CHAIN_DONE
