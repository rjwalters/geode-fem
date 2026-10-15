#!/usr/bin/env bash
# R4 production run for #1088 (same stopping checks as R2: 1200/600/4/0.001)
set -u
OUT=$HOME/r4; cd $HOME/geode-fem
date -u +%FT%TZ > $OUT/logs/production.start
( while sleep 30; do
    id=$(docker inspect -f '{{.Id}}' meep-production 2>/dev/null) || continue
    peak=$(cat /sys/fs/cgroup/system.slice/docker-$id.scope/memory.peak 2>/dev/null)
    cur=$(cat /sys/fs/cgroup/system.slice/docker-$id.scope/memory.current 2>/dev/null)
    echo "$(date -u +%FT%TZ) current=$cur peak=$peak"
  done ) > $OUT/logs/cgroup_memory.log 2>&1 &
SAMPLER=$!
docker run --rm --name meep-production --cpus 32 --memory 240g --shm-size 2g \
  -v "$PWD:/work:ro" -v "$OUT:/results" meep-matched:locked \
  timeout 41400 mpirun -np 32 python reference/meep/docker/matched_antenna.py \
  --config reference/meep/docker/matched_initial_2g4.json \
  --resolution 4 --after-source 1200 --check-interval 600 --max-checks 4 \
  --max-estimated-gib 120 \
  --output /results/initial_R4.json > $OUT/logs/initial_R4.log 2>&1
echo "exit=$?" > $OUT/logs/production.exit
date -u +%FT%TZ >> $OUT/logs/production.exit
kill $SAMPLER
