#!/usr/bin/env bash
# On-box Palace CPU driven runs for the #930 same-host Jacobi / AMS / Palace
# comparison. The CPU-image counterpart of box_palace_driven.sh (#520).
#
# Solves the gpu_driven_scaling cube fixture (exported by the geode harness
# with GEODE_SCALING_EXPORT_DIR as Gmsh MSH 2.2) with Palace at the harness's
# drive frequency (omega_nat = 0.1), config from palace_driven_cfg.py: Order 1,
# Linear Type Default, Tol 1e-8, the same as the #520 run. Image: palace:cpu
# from reference/palace/docker/Dockerfile.
#
# PALACE_LINEAR_TYPE (default "Default") sets Solver.Linear.Type. "Default"
# is a SuperLU-preconditioned GMRES in this image, not AMS (see
# palace_driven_cfg.py); PALACE_LINEAR_TYPE=AMS runs Palace's AMS. Give each
# type its own $OUT.
#
# Pinning: the container gets --cpuset-cpus <cpu-list> (one hyperthread per
# physical core) and MPICH binds each rank to a core (-bind-to core).
# OMP_NUM_THREADS=1.
#
# usage: box_palace_cpu.sh <n> <ranks> <cpu-list> [runs]
# Writes, per run, into $OUT:
#   <tag>.log              Palace's stdout + stderr
#   <tag>.time             /usr/bin/time -v around `palace -np N` inside the
#                          container (container start-up excluded). Its max RSS
#                          is the largest single process, i.e. one rank
#   <tag>.cgroup_peak      the container cgroup's memory.peak in bytes, all
#                          ranks together, page cache included
#   <tag>_config.json, <tag>_port-{S,V,I}.csv, <tag>.units
# and appends one summary line to $OUT/runs.txt.
set -euo pipefail
n=$1; np=$2; cpus=$3; runs=${4:-1}
OUT="${OUT:-$HOME/run930/palace}"
MESHES="${MESHES:-$HOME/run930/meshes}"
IMAGE="${IMAGE:-palace:cpu}"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$OUT"
DOCKER=docker
docker info >/dev/null 2>&1 || DOCKER="sudo docker"
for i in $(seq 1 "$runs"); do
  tag="palace_cpu_np${np}_n${n}_run${i}"
  work="$OUT/$tag.work"
  mkdir -p "$work"
  cp "$MESHES/cube_n${n}.msh" "$work/mesh.msh"
  python3 -I "$HERE/palace_driven_cfg.py" /work/mesh.msh "$work/config.json" /work/postpro CPU \
    1e-8 2000 "${PALACE_LINEAR_TYPE:-Default}" > "$OUT/$tag.units"
  rc=0
  $DOCKER run --rm --ipc=host --cpuset-cpus "$cpus" -e OMP_NUM_THREADS=1 \
    -v "$work:/work" -w /work --entrypoint /bin/sh "$IMAGE" -c \
    "/usr/bin/time -v -o /work/time.txt palace -np $np -launcher-args '-bind-to core' /work/config.json; rc=\$?; cat /sys/fs/cgroup/memory.peak > /work/cgroup_peak.txt 2>/dev/null; chown -R $(id -u):$(id -g) /work; exit \$rc" \
    > "$OUT/$tag.log" 2>&1 || rc=$?
  cp "$work/time.txt" "$OUT/$tag.time" 2>/dev/null || true
  cp "$work/cgroup_peak.txt" "$OUT/$tag.cgroup_peak" 2>/dev/null || true
  for f in port-S.csv port-V.csv port-I.csv; do
    cp "$work/postpro/$f" "$OUT/${tag}_$f" 2>/dev/null || true
  done
  cp "$work/config.json" "$OUT/${tag}_config.json"
  wall=$(grep -F 'Elapsed (wall clock)' "$OUT/$tag.time" 2>/dev/null | awk '{print $NF}' || echo NA)
  rss=$(grep -F 'Maximum resident set size' "$OUT/$tag.time" 2>/dev/null | awk '{print $NF}' || echo NA)
  echo "$tag,rc=$rc,cpus=$cpus,wall=$wall,max_rss_kb_one_rank=$rss,cgroup_peak_bytes=$(cat "$OUT/$tag.cgroup_peak" 2>/dev/null || echo NA),end=$(date -u +%FT%TZ)" \
    | tee -a "$OUT/runs.txt"
  # Keep only the copied evidence; the mesh and Palace's field output are large.
  find "$work" -mindepth 1 -delete; rmdir "$work"
done
