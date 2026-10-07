#!/usr/bin/env bash
# On-box benchmark driver for #519 (run after box_setup.sh succeeded).
#
# Same host, same session, identical sha-pinned transmon fixture
# (6 modes @ 4.5 GHz, Order 1). Cells:
#   palace_gpu_np1   Palace (CUDA build) "Device": "GPU", 1 MPI rank, 1x L40S
#   palace_cpu_npN   the SAME Palace image with "Device": "CPU", N ranks
#                    (same-box CPU control; N = physical cores)
#   geode_cpu_tT     geode transmon_bench (faer direct LU shift-invert, f64)
#                    at GEODE_NUM_THREADS = T
# geode-CUDA has NO eigensolve path (docs/research/gpu-eigensolve-path-design.md),
# so there is no geode GPU cell for this problem.
#
# Timing = /usr/bin/time -v around the full solver invocation (mesh load +
# assembly + solve + output). For Palace that wraps `palace -np N` INSIDE the
# container, so container start-up is excluded; the host-side wall including
# `docker run` is recorded separately (host_wall_s). Peak RSS = max single
# process RSS (/usr/bin/time over mpirun = max over ranks, NOT aggregate) —
# same methodology as benchmarks/transmon_bench_cpu. GPU memory = peak of
# nvidia-smi memory.used polled every 100 ms, minus the idle baseline.
set -euo pipefail
WORK="${WORK:-$HOME/palace-run}"
RUNS="${RUNS:-3}"
CPU_RANKS="${CPU_RANKS:-4}"
RAW="$WORK/raw"
mkdir -p "$RAW"
cd "$WORK"
DOCKER=docker
docker info >/dev/null 2>&1 || DOCKER="sudo docker"

mkcfg() {  # mkcfg <src.json> <out.json> <output-dir>
  python3 -I - "$1" "$2" "$3" <<'EOF'
import json, sys
src, out, outdir = sys.argv[1:4]
c = json.load(open(src))
c["Model"]["Mesh"] = "/work/transmon_smoke_v22.msh"
c["Problem"]["Output"] = outdir
json.dump(c, open(out, "w"), indent=2)
EOF
}

gpu_idle_mib=$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits | head -1)
echo "gpu_idle_mib=$gpu_idle_mib" > "$RAW/gpu_idle.txt"
echo "cell,run,host_wall_s,time_wall,max_rss_kb,exit,gpu_peak_mib,gpu_idle_mib" > "$RAW/runs.csv"

palace_cell() {  # palace_cell <cell> <src-config> <np> <gpu:0|1>
  local cell=$1 src=$2 np=$3 gpu=$4 i
  for i in $(seq 1 "$RUNS"); do
    local tag="${cell}_run${i}" out="postpro/${cell}_run${i}"
    rm -rf "$out"
    mkcfg "$src" "cfg_${tag}.json" "/work/$out"
    local gflag=() poll=""
    [ "$gpu" = 1 ] && gflag=(--gpus all)
    nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits -lms 100 > "$RAW/${tag}.gpumem" 2>/dev/null &
    poll=$!
    local t0 t1 rc=0
    t0=$(date +%s.%N)
    $DOCKER run --rm "${gflag[@]}" --ipc=host -e OMP_NUM_THREADS=1 -v "$WORK:/work" -w /work \
      --entrypoint /usr/bin/time palace:cuda -v -o "/work/raw/${tag}.time" \
      palace -np "$np" "/work/cfg_${tag}.json" > "$RAW/${tag}.log" 2>&1 || rc=$?
    t1=$(date +%s.%N)
    kill "$poll" 2>/dev/null || true; wait "$poll" 2>/dev/null || true
    local hw wall rss peak
    hw=$(python3 -c "print(f'{$t1-$t0:.3f}')")
    wall=$(grep -F 'Elapsed (wall clock)' "$RAW/${tag}.time" | awk '{print $NF}' || echo NA)
    rss=$(grep -F 'Maximum resident set size' "$RAW/${tag}.time" | awk '{print $NF}' || echo NA)
    peak=$(sort -n "$RAW/${tag}.gpumem" | tail -1)
    echo "$cell,$i,$hw,$wall,$rss,$rc,$peak,$gpu_idle_mib" | tee -a "$RAW/runs.csv"
    cp "$out/eig.csv" "$RAW/${tag}_eig.csv" 2>/dev/null || echo "no eig.csv for $tag" >> "$RAW/missing.txt"
    $DOCKER run --rm -v "$WORK:/work" --entrypoint chown palace:cuda -R "$(id -u):$(id -g)" /work/postpro /work/raw >/dev/null 2>&1 || true
  done
}

geode_cell() {  # geode_cell <threads>
  local t=$1 i bin="$HOME/geode-fem/target/release/examples/transmon_bench"
  for i in $(seq 1 "$RUNS"); do
    local tag="geode_cpu_t${t}_run${i}" rc=0 t0 t1
    t0=$(date +%s.%N)
    GEODE_INNER=direct GEODE_SIGMA_GHZ=4.5 GEODE_NMODES=6 GEODE_NUM_THREADS=$t \
      /usr/bin/time -v -o "$RAW/${tag}.time" "$bin" > "$RAW/${tag}.log" 2>&1 || rc=$?
    t1=$(date +%s.%N)
    local hw wall rss
    hw=$(python3 -c "print(f'{$t1-$t0:.3f}')")
    wall=$(grep -F 'Elapsed (wall clock)' "$RAW/${tag}.time" | awk '{print $NF}' || echo NA)
    rss=$(grep -F 'Maximum resident set size' "$RAW/${tag}.time" | awk '{print $NF}' || echo NA)
    echo "geode_cpu_t${t},$i,$hw,$wall,$rss,$rc,NA,NA" | tee -a "$RAW/runs.csv"
  done
}

CELLS="${CELLS:-palace_gpu palace_cpu geode}"
for c in $CELLS; do
  case $c in
    palace_gpu) palace_cell palace_gpu_np1 palace_config_gpu.json 1 1 ;;
    palace_cpu) palace_cell "palace_cpu_np${CPU_RANKS}" palace_config.json "$CPU_RANKS" 0 ;;
    geode) geode_cell 1; geode_cell "$CPU_RANKS" ;;
  esac
done
grep -hiE "libCEED backend|Device configuration|Memory configuration|Git changeset|MPI processes" \
  "$RAW"/palace_*_run1.log > "$RAW/backend.txt" || true
echo BENCH_DONE > "$RAW/BENCH_DONE"
