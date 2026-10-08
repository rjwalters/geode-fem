#!/usr/bin/env bash
# #930 at-scale chain, part E: one size beyond the five the run was scoped
# for, n = 50 (about 900k edges), single runs. Added during the session with
# box time left over, to see whether the trends hold towards 1M edges.
set -u
S="${S:-$HOME/bench}"
R="${R:-$HOME/run930}"
export GEODE_DIR="${GEODE_DIR:-$HOME/geode}" LEG_TIMEOUT_S=3600 SKIP_XCHECK=1
TASKSET_CPUS=0 RAYON_NUM_THREADS=1 LEGS="jacobi ams" \
  "$S/ams_cpu_sweep.sh" "$R/t1_rep3_n50" 50 2>&1 | sed -u "s|^|t1_rep3_n50: |"
# Mesh export for Palace: untimed, output discarded (see export_meshes.sh).
( cd "$GEODE_DIR" && bin=$(ls target/release/deps/gpu_driven_scaling-* | grep -v '\.d$' | head -1) &&
  GEODE_SCALING_SIZES=50 GEODE_SCALING_CONFIGS=iterative_ams GEODE_SCALING_REPS=0 \
    GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1 GEODE_SCALING_ITER_MAX=1 \
    GEODE_SCALING_EXPORT_DIR="$R/meshes" "$bin" --ignored --nocapture --test-threads 1 \
    > /dev/null 2>> "$R/meshes/export.err" )
(cd "$R" && sha256sum meshes/cube_n*.msh > mesh_sha256.txt)
export MESHES="$R/meshes"
OUT="$R/palace_ams" PALACE_LINEAR_TYPE=AMS "$S/box_palace_cpu.sh" 50 8 0-7 1
OUT="$R/palace_ams" PALACE_LINEAR_TYPE=AMS "$S/box_palace_cpu.sh" 50 1 0 1
OUT="$R/palace" "$S/box_palace_cpu.sh" 50 8 0-7 1
OUT="$R/palace" "$S/box_palace_cpu.sh" 50 1 0 1
echo CHAIN_E_DONE
