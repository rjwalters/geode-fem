#!/usr/bin/env bash
# #1088 diagnosis runs (one r8g.8xlarge, us-east-2). Usage: run_diag.sh build|grid|run <tag> <geometry> <res>
set -u
OUT=$HOME/diag; mkdir -p "$OUT/logs"; cd "$HOME/geode-fem"
IMG=meep-matched:locked
DR=(docker run --rm --cpus 32 --memory 240g --shm-size 2g -v "$PWD:/work:ro" -v "$OUT:/results" "$IMG")
case "$1" in
build)
  docker build --platform linux/arm64 -t "$IMG" -f reference/meep/docker/Dockerfile.matched \
    reference/meep/docker > "$OUT/logs/docker-build.log" 2>&1 || exit 1
  docker run --rm "$IMG" cat /opt/meep-explicit.txt > "$OUT/conda-explicit.txt"
  diff <(grep '^https' reference/meep/docker/matched-conda-linux-aarch64.txt | sort) \
       <(grep '^https' "$OUT/conda-explicit.txt" | sort) > "$OUT/logs/lock-diff.txt"
  echo "lock_diff_exit=$? urls=$(grep -c '^https' "$OUT/conda-explicit.txt")" | tee "$OUT/logs/lock-check.txt"
  docker image inspect -f '{{.Id}}' "$IMG" > "$OUT/logs/image-id.txt"
  # RC oracle replay at R2 against the committed us-east-1 spectra.
  for kind in volume sheet; do
    flag=(); [ $kind = sheet ] && flag=(--rc-sheet)
    "${DR[@]}" mpirun -np 2 python reference/meep/docker/matched_antenna.py --rc-oracle "${flag[@]}" \
      --resolution 2 --after-source 100 --check-interval 100 --output /results/rc_${kind}_R2.json \
      > "$OUT/logs/rc_${kind}_R2.log" 2>&1
  done
  "${DR[@]}" python reference/meep/docker/matched_geometry_verify.py > "$OUT/logs/geometry.log" 2>&1
  echo "geometry_exit=$?" >> "$OUT/logs/geometry.log" ;;
grid)
  "${DR[@]}" mpirun -np 32 python reference/meep/docker/diag_antenna.py \
    --config reference/meep/docker/matched_initial_2g4.json --geometry "$3" --resolution "$4" \
    --dump-grid --max-estimated-gib 120 --output "/results/grid_${2}.json" > "$OUT/logs/grid_${2}.log" 2>&1
  echo "exit=$?" >> "$OUT/logs/grid_${2}.log" ;;
run)
  # run <tag> <geometry> <res> [extra diag_antenna.py args]
  tag=$2; geom=$3; res=$4; shift 4
  date -u +%FT%TZ > "$OUT/logs/${tag}.start"
  "${DR[@]}" timeout 14400 mpirun -np 32 python reference/meep/docker/diag_antenna.py \
    --config reference/meep/docker/matched_initial_2g4.json --geometry "$geom" --resolution "$res" \
    --max-estimated-gib 120 "$@" --output "/results/${tag}.json" > "$OUT/logs/${tag}.log" 2>&1
  echo "exit=$? end=$(date -u +%FT%TZ)" > "$OUT/logs/${tag}.exit" ;;
esac
