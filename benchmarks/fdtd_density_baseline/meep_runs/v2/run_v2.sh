#!/usr/bin/env bash
# #1146 harness-v2 runs (one r8g.8xlarge, us-east-1).
# Usage: run_v2.sh build | v1repro | checks | audit <tag> <geom> <res> | run <tag> <geom> <res> <ranks> [args]
set -u
OUT=$HOME/v2; mkdir -p "$OUT/logs"; cd "$HOME/geode-fem"
IMG=meep-matched:locked
CFG=reference/meep/docker/matched_initial_2g4.json
V2=reference/meep/docker/matched_antenna_v2.py
dr() { # dr <name> <cpus> <mem> cmd...
  local name=$1 cpus=$2 mem=$3; shift 3
  docker run --rm --name "$name" --cpus "$cpus" --memory "$mem" --shm-size 2g \
    -v "$PWD:/work:ro" -v "$OUT:/results" "$IMG" "$@"
}
sampler() { # sampler <container> <log>: cgroup memory of a running container every 30 s
  ( while sleep 30; do
      id=$(docker inspect -f '{{.Id}}' "$1" 2>/dev/null) || continue
      g=/sys/fs/cgroup/system.slice/docker-$id.scope
      echo "$(date -u +%FT%TZ) current=$(cat $g/memory.current 2>/dev/null) peak=$(cat $g/memory.peak 2>/dev/null)"
    done ) > "$2" 2>&1 &
  echo $!
}
case "$1" in
build)
  docker build --platform linux/arm64 -t "$IMG" -f reference/meep/docker/Dockerfile.matched \
    reference/meep/docker > "$OUT/logs/docker-build.log" 2>&1 || exit 1
  docker run --rm "$IMG" cat /opt/meep-explicit.txt > "$OUT/conda-explicit.txt"
  diff <(grep '^https' reference/meep/docker/matched-conda-linux-aarch64.txt | sort) \
       <(grep '^https' "$OUT/conda-explicit.txt" | sort) > "$OUT/logs/lock-diff.txt"
  echo "lock_diff_exit=$? urls=$(grep -c '^https' "$OUT/conda-explicit.txt")" | tee "$OUT/logs/lock-check.txt"
  docker image inspect -f '{{.Id}}' "$IMG" > "$OUT/logs/image-id.txt" ;;
v1repro)
  # The unchanged v1 harness: RC oracles at R2 and the committed R1 antenna run (8 ranks).
  for kind in volume sheet; do
    flag=(); [ $kind = sheet ] && flag=(--rc-sheet)
    dr rc-$kind 2 4g mpirun -np 2 python reference/meep/docker/matched_antenna.py --rc-oracle "${flag[@]}" \
      --resolution 2 --after-source 100 --check-interval 100 --output /results/v1_rc_${kind}_R2.json \
      > "$OUT/logs/v1_rc_${kind}_R2.log" 2>&1
  done
  dr geom 1 2g python reference/meep/docker/matched_geometry_verify.py > "$OUT/logs/v1_geometry.log" 2>&1
  echo "geometry_exit=$?" >> "$OUT/logs/v1_geometry.log"
  dr r1 8 24g mpirun -np 8 python reference/meep/docker/matched_antenna.py --config "$CFG" \
    --resolution 1 --after-source 1200 --check-interval 600 --max-checks 5 \
    --output /results/v1_initial_R1.json > "$OUT/logs/v1_initial_R1.log" 2>&1
  echo "end=$(date -u +%FT%TZ)" > "$OUT/logs/v1repro.exit" ;;
checks)
  # Probes, RC oracles (point source, point monitors) and FR-4 material oracles.
  for r in 2.5 5; do
    dr probe 1 8g python $V2 --probe --resolution $r --output /results/probe_R$r.json \
      > "$OUT/logs/probe_R$r.log" 2>&1
  done
  # RC oracles on v1's window and stopping (2.2-2.7 GHz, 100/100, one check).
  W=(--fmin 2.2 --fmax 2.7 --df 0.01 --after-source 100 --check-interval 100 --max-checks 1)
  for r in 2 2.5 5; do
    dr rc 2 8g mpirun -np 2 python $V2 --rc-oracle --resolution $r "${W[@]}" \
      --output /results/rc_point_R$r.json > "$OUT/logs/rc_point_R$r.log" 2>&1
  done
  dr rc 2 8g mpirun -np 2 python $V2 --rc-oracle --rc-sheet --resolution 2 "${W[@]}" \
    --output /results/rc_sheetpoint_R2.json > "$OUT/logs/rc_sheetpoint_R2.log" 2>&1
  for r in 2.5 5; do
    dr mat 2 8g mpirun -np 2 python $V2 --material-oracle --resolution $r --after-source 3000 \
      --check-interval 1000 --max-checks 6 --temporal-tolerance 1e-7 \
      --output /results/material_R$r.json > "$OUT/logs/material_R$r.log" 2>&1
  done
  dr mat 2 8g mpirun -np 2 python $V2 --material-oracle --loss-model conductivity --resolution 5 \
    --after-source 3000 --check-interval 1000 --max-checks 6 --temporal-tolerance 1e-7 \
    --output /results/material_conductivity_R5.json > "$OUT/logs/material_conductivity_R5.log" 2>&1
  echo "end=$(date -u +%FT%TZ)" > "$OUT/logs/checks.exit" ;;
audit)
  tag=$2; geom=$3; res=$4; shift 4
  np=${AUDIT_RANKS:-32}
  dr audit-$tag $np 120g mpirun -np $np python $V2 --config "$CFG" --geometry "$geom" --resolution "$res" \
    --audit-only --max-estimated-gib 240 "$@" --output "/results/audit_${tag}.json" \
    > "$OUT/logs/audit_${tag}.log" 2>&1
  echo "exit=$?" >> "$OUT/logs/audit_${tag}.log" ;;
run)
  # run <tag> <geom> <res> <ranks> [extra args]; memory limit scales with ranks
  tag=$2; geom=$3; res=$4; np=$5; shift 5
  mem=$(( np * 7 + 16 ))g
  date -u +%FT%TZ > "$OUT/logs/${tag}.start"
  s=$(sampler "v2-$tag" "$OUT/logs/${tag}.cgroup_memory.log")
  dr "v2-$tag" "$np" "$mem" timeout "${RUN_TIMEOUT:-72000}" mpirun -np "$np" python $V2 --config "$CFG" --geometry "$geom" \
    --resolution "$res" --max-estimated-gib 240 "$@" --output "/results/${tag}.json" \
    > "$OUT/logs/${tag}.log" 2>&1
  echo "exit=$? end=$(date -u +%FT%TZ)" > "$OUT/logs/${tag}.exit"
  kill "$s" ;;
esac
