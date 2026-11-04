#!/usr/bin/env bash
# Issue #1092 part 2: the worker side of ladder.sh. geode A2 on h and h/2
# (the part-1 cross-check example in solve mode), then the five Palace
# cells, one at a time. Progress in ~/work1092lad_<tag>/orchestrate.log.
set -u
tag=$1
WORK=$HOME/work1092lad_$tag
REPO=$HOME/geode-fem
BIN=$REPO/target/release/examples/transmon_devicelayout_crosscheck
cd "$WORK" || exit 1
exec >> "$WORK/orchestrate.log" 2>&1
echo "ladder $tag start $(date -u +%FT%TZ) commit $(git -C "$REPO" rev-parse --short=8 HEAD)"
export GEODE_RUNS_DIR=$WORK/geode_runs
export GEODE_HOST_NOTE="AWS r8g.8xlarge (Graviton4, 32 vCPU, 247 GiB), Ubuntu 24.04 arm64"
mkdir -p "$GEODE_RUNS_DIR"
run_geode() { # label mesh
  echo "geode $1 start $(date -u +%FT%TZ)"
  GEODE_SOLVE_MESH=$WORK/$2 GEODE_SOLVE_LABEL=$1 GEODE_SIGMA_GHZ=4.0 GEODE_NMODES=3 \
    /usr/bin/time -v -o "$WORK/time_geode_$1.txt" "$BIN" > "$WORK/geode_run_$1.log" 2>&1
  echo "geode $1 end exit=$? $(date -u +%FT%TZ)"
}
run_geode "${tag}_h" "${tag}_o1.msh"
run_geode "${tag}_h2" "${tag}_r1.msh"
export WORK IMG=palace:cpu-1043 DOCKER="sudo docker"
cells=()
while read -r cell np; do [ -n "$cell" ] && cells+=("$cell:$np"); done < "$WORK/cells.txt"
bash "$REPO/reference/fixtures/transmon_palace/results_crosscheck_1092/run.sh" "${cells[@]}"
echo "ladder $tag end $(date -u +%FT%TZ)"
