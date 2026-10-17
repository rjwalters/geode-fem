#!/usr/bin/env bash
# #930: write the five cube meshes as Gmsh MSH 2.2 for Palace, in one untimed
# process. The AMS config with a 1-iteration cap is used only because it
# records a DNF cell instead of panicking; its output is discarded.
set -u
R="${R:-$HOME/run930}"
mkdir -p "$R/meshes"
cd "${GEODE_DIR:-$HOME/geode}"
bin=$(ls target/release/deps/gpu_driven_scaling-* | grep -v '\.d$' | head -1)
GEODE_SCALING_SIZES=15,20,24,36,40 GEODE_SCALING_CONFIGS=iterative_ams GEODE_SCALING_REPS=0 \
  GEODE_SCALING_SKIP_E2E=1 GEODE_SCALING_SKIP_SWEEP=1 GEODE_SCALING_ITER_MAX=1 \
  GEODE_SCALING_EXPORT_DIR="$R/meshes" "$bin" --ignored --nocapture --test-threads 1 \
  > /dev/null 2> "$R/meshes/export.err"
(cd "$R" && sha256sum meshes/cube_n*.msh > mesh_sha256.txt)
echo EXPORT_DONE
