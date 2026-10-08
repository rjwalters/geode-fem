#!/usr/bin/env bash
# On-box Palace driven runs for the #520 geode-vs-Palace head-to-head.
#
# Solves the gpu_driven_scaling cube fixture (exported by the geode harness
# with GEODE_SCALING_EXPORT_DIR as Gmsh MSH 2.2, physical tags volume 1 /
# PEC 2 / port 3) with Palace at the single drive frequency of the harness
# (omega_nat = 0.1), using the palace:cuda image built for #519
# (reference/palace/docker/Dockerfile.cuda, Palace fba6a5b). Unit mapping
# geode-natural -> Palace SI is in palace_driven_cfg.py.
#
# usage: box_palace_driven.sh <n> <GPU|CPU> <ranks> [runs]
# Writes $OUT/palace_<dev>_np<ranks>_n<n>_run<i>.{log,time,gpumem} and copies
# Palace's port-S.csv / port-V.csv / port-I.csv next to them.
set -euo pipefail
n=$1; dev=$2; np=$3; runs=${4:-1}
OUT="${OUT:-$HOME/sweep/palace}"
MESHES="${MESHES:-$HOME/sweep/meshes}"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$OUT"
DOCKER=docker
docker info >/dev/null 2>&1 || DOCKER="sudo docker"
devl=$(echo "$dev" | tr '[:upper:]' '[:lower:]')
for i in $(seq 1 "$runs"); do
  tag="palace_${devl}_np${np}_n${n}_run${i}"
  work="$OUT/$tag"
  rm -rf "$work"; mkdir -p "$work"
  cp "$MESHES/cube_n${n}.msh" "$work/mesh.msh"
  python3 -I "$HERE/palace_driven_cfg.py" /work/mesh.msh "$work/config.json" /work/postpro "$dev" \
    > "$work/units.txt"
  nvidia-smi --query-gpu=memory.used,utilization.gpu --format=csv,noheader,nounits -lms 100 \
    > "$OUT/$tag.gpumem" 2>/dev/null &
  poll=$!
  rc=0
  # --gpus all even for CPU: the CUDA build links libcuda.so.1 (see #519).
  $DOCKER run --rm --gpus all --ipc=host -e OMP_NUM_THREADS=1 -v "$work:/work" -w /work \
    --entrypoint /usr/bin/time palace:cuda -v -o /work/time.txt \
    palace -np "$np" /work/config.json > "$OUT/$tag.log" 2>&1 || rc=$?
  kill "$poll" 2>/dev/null || true; wait "$poll" 2>/dev/null || true
  $DOCKER run --rm -v "$work:/work" --entrypoint chown palace:cuda -R "$(id -u):$(id -g)" /work >/dev/null 2>&1 || true
  cp "$work/time.txt" "$OUT/$tag.time" 2>/dev/null || true
  for f in port-S.csv port-V.csv port-I.csv; do
    cp "$work/postpro/$f" "$OUT/${tag}_$f" 2>/dev/null || true
  done
  cp "$work/config.json" "$OUT/${tag}_config.json"
  wall=$(grep -F 'Elapsed (wall clock)' "$OUT/$tag.time" 2>/dev/null | awk '{print $NF}' || echo NA)
  rss=$(grep -F 'Maximum resident set size' "$OUT/$tag.time" 2>/dev/null | awk '{print $NF}' || echo NA)
  peak=$(cut -d, -f1 "$OUT/$tag.gpumem" | sort -n | tail -1)
  echo "$tag,rc=$rc,wall=$wall,max_rss_kb=$rss,gpu_peak_mib=$peak" | tee -a "$OUT/runs.txt"
  rm -rf "$work/postpro/paraview" "$work/mesh.msh"
done
