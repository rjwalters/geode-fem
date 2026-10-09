#!/usr/bin/env bash
# On-box benchmark driver for the TUNED-Palace re-timing on m6i (issue #927,
# follow-up to PR #964). Run after m6i_tuned_box_setup.sh succeeded, on an
# otherwise idle box. One host, one session, one geode commit, the committed
# 133,108-DOF transmon fixture.
#
# This is m6i_box_bench.sh (the PR #964 driver) with these changes only:
#   * the Palace image is palace:tuned (reference/palace/docker/Dockerfile.tuned);
#   * Solver.Linear.Type is written EXPLICITLY into every Palace config (column
#     `linear` of cells.tsv). The fixture config says "Default", which at this
#     Palace commit resolves to SuperLU for an eigenmode problem when Palace is
#     built with SuperLU (palace/utils/iodata.cpp, "Prefer sparse direct solver
#     for frequency domain problems"), as both images are. So "SuperLU" is the
#     solver PR #964 ran, now named. One control cell keeps "Default";
#   * the cell list is the PR #964 fixture-request Palace cells (Save = 6 and
#     Save = 0, 1 and 8 ranks) and the four PR #964 geode requests, with
#     PALACE_RUNS (default 3) and GEODE_RUNS (default 2) repeats.
#
# geode cells (transmon_bench example, GEODE_INNER=direct, f64):
#   cell                   gauge       sigma  modes  what
#   geode_s4p5_n6          none        4.5    6      the historical timed request (returns 1 physical mode)
#   geode_s4p5_n30         none        4.5    30     route "more modes"
#   geode_s20_n6           none        20     6      route "shift"
#   geode_pa_s4p5_n8       port_aware  4.5    8      route "port-aware"
# each at 1 thread (_t1) and 8 threads (_t8).
#
# Route "port-aware": GEODE_GAUGE=port_aware selects, since PR #1002 (#950),
# the single-factorization port-subspace projection (one factorization of
# K - sigma M, no junction extract; six physical modes from a 7-mode request,
# plus the 3.45 GHz port mode, #1003). The #927 tuned sweep ran this driver at
# geode 8367ee7b, before PR #1002, when port_aware selected the issue #514 route
# (junction extract + projected band, two factorizations; 8 modes for all six
# physical, plus two non-physical). That route is now
# GEODE_GAUGE=port_aware_extract. The numbers in results_tuned_palace_m6i.toml
# stay attributable through that recorded commit; a re-run of this driver at a
# later commit times the port-subspace route.
#
# Thread enforcement (#763), unchanged from PR #964. "1 thread" =
# GEODE_NUM_THREADS=1 and RAYON_NUM_THREADS=1 and `taskset -c <first cpu>`.
# "8 threads" = both knobs at 8 and taskset to ONE hardware thread of each of
# the 8 physical cores (SMT siblings idle). Every run's .meta records the
# affinity mask the process actually had and the largest task count seen.
#
# Palace cells (palace:tuned, OMP_NUM_THREADS=1, Linear.Type explicit):
#   palace_s4p5_n6_np1 / _np8                 fixture config: N = 6, Target = 4.5 GHz, Save = 6
#   palace_s4p5_n6_np1_nosave / _np8_nosave   same with Eigenmode.Save = 0 (no ParaView output)
#   palace_s4p5_n6_np8_nosave_lindefault      control: Save = 0, Linear.Type = "Default" (1 run)
# The container is confined with `docker run --cpuset-cpus` to the same CPUs the
# geode cell of that width uses, and hydra binds each rank with `-bind-to core`.
#
# Runs are sequential, never two at once, in round-robin order (repeat 1 of
# every cell, then repeat 2, ...) so slow drift of the box spreads over cells.
#
# Timing = /usr/bin/time -v around the full solver invocation (mesh load +
# assembly + solve + output). For Palace that wraps `palace -np N` INSIDE the
# container, so container start-up is excluded; GNU time's user/sys are the sum
# over mpirun and every rank it waited for. Peak RSS for Palace is the LARGEST
# SINGLE PROCESS (not the sum over ranks); Palace's own "Peak Memory ... Total"
# is in each log.
#
# Each run writes into $OUT/raw:
#   <cell>_run<i>.log    solver stdout+stderr (config banner, modes, phase times)
#   <cell>_run<i>.time   /usr/bin/time -v
#   <cell>_run<i>.meta   start/end (UTC), load average, env, affinity evidence
#   <cell>_run<i>_eig.csv   Palace cells: the eigenvalue table Palace wrote
# and once per sweep: $OUT/cells.tsv, $OUT/pinning.txt, $OUT/configs/.
#
# A run whose .meta already says exit=0 is skipped, so the driver can be
# re-invoked on the same $OUT to add cells or resume.
#
# usage: ./m6i_tuned_box_bench.sh   (env: WORK, OUT, GEODE_RUNS, PALACE_RUNS, RUN_TIMEOUT_S, CELL_FILTER)
# Then:  summarize_tuned_palace_m6i.py <run-dir> <palace eig.csv> <PR #964 results TOML>
set -euo pipefail
WORK="${WORK:-$HOME/tuned-palace}"
OUT="${OUT:-$WORK/run}"
GEODE_RUNS="${GEODE_RUNS:-2}"
PALACE_RUNS="${PALACE_RUNS:-3}"
IMAGE="${IMAGE:-palace:tuned}"
RUN_TIMEOUT_S="${RUN_TIMEOUT_S:-2400}"
CELL_FILTER="${CELL_FILTER:-.}"
RAW="$OUT/raw"
mkdir -p "$RAW" "$OUT/configs"
cd "$WORK"
DOCKER=docker
docker info >/dev/null 2>&1 || DOCKER="sudo docker"
BIN="$HOME/geode-fem/target/release/examples/transmon_bench"
[ -x "$BIN" ] || { echo "missing $BIN" >&2; exit 1; }

# ---- CPU sets: one hardware thread per physical core -------------------------
# lscpu -p=CPU,CORE,SOCKET: keep the lowest-numbered CPU of every (socket, core).
ONE_PER_CORE=$(lscpu -p=CPU,CORE,SOCKET | grep -v '^#' | sort -t, -k3,3n -k2,2n -k1,1n \
  | awk -F, '!seen[$3","$2]++ {print $1}' | sort -n | paste -sd, -)
CPUS1=$(echo "$ONE_PER_CORE" | cut -d, -f1)
CPUS8=$(echo "$ONE_PER_CORE" | cut -d, -f1-8)
[ "$(echo "$CPUS8" | tr , '\n' | wc -l)" -eq 8 ] || { echo "need 8 physical cores, have: $ONE_PER_CORE" >&2; exit 1; }
{
  echo "one_cpu_per_physical_core=$ONE_PER_CORE"
  echo "cpuset_1=$CPUS1"
  echo "cpuset_8=$CPUS8"
  echo "smt_siblings_of_cpuset_8=$(for c in $(echo "$CPUS8" | tr , ' '); do cat "/sys/devices/system/cpu/cpu$c/topology/thread_siblings_list"; done | paste -sd' ' -)"
  echo "taskset_check_1=$(taskset -c "$CPUS1" grep Cpus_allowed_list /proc/self/status | awk '{print $2}')"
  echo "taskset_check_8=$(taskset -c "$CPUS8" grep Cpus_allowed_list /proc/self/status | awk '{print $2}')"
  # What hydra's -bind-to core gives each rank inside the confined container.
  echo "palace_np8_rank_masks=$($DOCKER run --rm --cpuset-cpus "$CPUS8" --entrypoint mpirun "$IMAGE" -np 8 -bind-to core grep Cpus_allowed_list /proc/self/status 2>&1 | awk '{print $2}' | sort -n | paste -sd' ' -)"
  echo "palace_np1_rank_masks=$($DOCKER run --rm --cpuset-cpus "$CPUS1" --entrypoint mpirun "$IMAGE" -np 1 -bind-to core grep Cpus_allowed_list /proc/self/status 2>&1 | awk '{print $2}' | paste -sd' ' -)"
} > "$OUT/pinning.txt"

# cell <tab> solver <tab> gauge <tab> sigma_ghz <tab> n_modes <tab> width <tab> pin <tab> runs <tab> linear
#   width = geode threads or Palace MPI ranks; pin = pinned | knob_only;
#   linear = Palace Solver.Linear.Type written into the config ("-" for geode)
cat > "$OUT/cells.tsv" <<CELLS
geode_s4p5_n6_t1	geode	none	4.5	6	1	pinned	$GEODE_RUNS	-
geode_s4p5_n6_t8	geode	none	4.5	6	8	pinned	$GEODE_RUNS	-
geode_s4p5_n30_t1	geode	none	4.5	30	1	pinned	$GEODE_RUNS	-
geode_s4p5_n30_t8	geode	none	4.5	30	8	pinned	$GEODE_RUNS	-
geode_s20_n6_t1	geode	none	20	6	1	pinned	$GEODE_RUNS	-
geode_s20_n6_t8	geode	none	20	6	8	pinned	$GEODE_RUNS	-
geode_pa_s4p5_n8_t1	geode	port_aware	4.5	8	1	pinned	$GEODE_RUNS	-
geode_pa_s4p5_n8_t8	geode	port_aware	4.5	8	8	pinned	$GEODE_RUNS	-
palace_s4p5_n6_np1	palace	divfree	4.5	6	1	pinned	$PALACE_RUNS	SuperLU
palace_s4p5_n6_np8	palace	divfree	4.5	6	8	pinned	$PALACE_RUNS	SuperLU
palace_s4p5_n6_np1_nosave	palace	divfree_nosave	4.5	6	1	pinned	$PALACE_RUNS	SuperLU
palace_s4p5_n6_np8_nosave	palace	divfree_nosave	4.5	6	8	pinned	$PALACE_RUNS	SuperLU
palace_s4p5_n6_np8_nosave_lindefault	palace	divfree_nosave	4.5	6	8	pinned	1	Default
CELLS

loadavg() { cut -d' ' -f1-3 /proc/loadavg; }

# sample_affinity <pgrep-args> <stem>: while the solver runs, record the union
# of Cpus_allowed_list over every task of every matching process, the largest
# task count, and the number of such processes. (Matches the kernel's 15-char
# comm, so the time/timeout/taskset/mpirun wrappers are not counted.)
sample_affinity() {
  local name=$1 stem=$2 masks="" maxthr=0 maxproc=0 pids p n t s
  sleep 3
  while [ -e "$stem.running" ]; do
    pids=$(pgrep $name || true)
    n=0; t=0
    for p in $pids; do
      n=$((n + 1))
      for s in /proc/"$p"/task/*/status; do
        [ -r "$s" ] || continue
        t=$((t + 1))
        masks="$masks $(sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' "$s" 2>/dev/null || true)"
      done
    done
    [ "$t" -gt "$maxthr" ] && maxthr=$t
    [ "$n" -gt "$maxproc" ] && maxproc=$n
    sleep 2
  done
  {
    echo "observed_processes_max=$maxproc"
    echo "observed_tasks_max=$maxthr"
    echo "observed_cpus_allowed=$(echo "$masks" | tr ' ' '\n' | grep -v '^$' | sort -u | paste -sd' ' -)"
  } >> "$stem.meta"
}

geode_run() {  # cell gauge sigma nmodes width pin i
  local cell=$1 gauge=$2 sigma=$3 nmodes=$4 width=$5 pin=$6 i=$7
  local stem="$RAW/${cell}_run${i}" rc=0 cpus
  if [ "$width" = 1 ]; then cpus=$CPUS1; else cpus=$CPUS8; fi
  local -a envv=(GEODE_INNER=direct GEODE_GAUGE="$gauge" GEODE_SIGMA_GHZ="$sigma" GEODE_NMODES="$nmodes" GEODE_NUM_THREADS="$width")
  local -a pre=()
  if [ "$pin" = pinned ]; then
    envv+=(RAYON_NUM_THREADS="$width")
    pre=(taskset -c "$cpus")
  fi
  {
    echo "cell=$cell"; echo "run=$i"; echo "solver=geode"
    echo "env=${envv[*]}"
    if [ "$pin" = pinned ]; then echo "pin=taskset -c $cpus"; else echo "pin=none (RAYON_NUM_THREADS unset, no taskset)"; fi
    echo "start_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "loadavg_before=$(loadavg)"
  } > "$stem.meta"
  : > "$stem.running"
  sample_affinity "-x transmon_bench" "$stem" &
  local sp=$!
  env -u RAYON_NUM_THREADS "${envv[@]}" timeout "$RUN_TIMEOUT_S" \
    /usr/bin/time -v -o "$stem.time" "${pre[@]}" "$BIN" > "$stem.log" 2>&1 < /dev/null || rc=$?
  rm -f "$stem.running"; wait "$sp" || true
  {
    echo "end_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "loadavg_after=$(loadavg)"
    echo "exit=$rc"
  } >> "$stem.meta"
  echo "$cell run $i: exit $rc $(grep -F 'Elapsed (wall clock)' "$stem.time" | awk '{print $NF}')" >&2
}

mkcfg() {  # mkcfg <out.json> <output-dir> <n> <target> <save:keep|0> <linear type>
  python3 -I - "$WORK/palace_config.json" "$1" "$2" "$3" "$4" "$5" "$6" <<'EOF'
import json, sys
src, out, outdir, n, target, save, linear = sys.argv[1:8]
c = json.load(open(src))
c["Model"]["Mesh"] = "/work/transmon_smoke_v22.msh"
c["Problem"]["Output"] = outdir
# Only N, Target, Save and the explicit Linear.Type are written; Tol, MaxIts,
# Order and the linear Tol / MaxIts stay as the committed fixture config has them.
c["Solver"]["Eigenmode"]["N"] = int(n)
c["Solver"]["Eigenmode"]["Target"] = float(target)
if save == "0":  # *_nosave cells only: identical solve, no ParaView output
    c["Solver"]["Eigenmode"]["Save"] = 0
c["Solver"]["Linear"]["Type"] = linear
json.dump(c, open(out, "w"), indent=2)
EOF
}

palace_run() {  # cell sigma nmodes np i save linear
  local cell=$1 sigma=$2 nmodes=$3 np=$4 i=$5 save=$6 linear=$7
  local tag="${cell}_run${i}" stem="$RAW/${cell}_run${i}" rc=0 cpus
  if [ "$np" = 1 ]; then cpus=$CPUS1; else cpus=$CPUS8; fi
  local outrel="postpro/$tag"
  $DOCKER run --rm -v "$WORK:/work" --entrypoint rm "$IMAGE" -rf "/work/$outrel" >/dev/null 2>&1 || true
  mkdir -p "$WORK/postpro"
  mkcfg "$OUT/configs/cfg_${tag}.json" "/work/$outrel" "$nmodes" "$sigma" "$save" "$linear"
  cp "$OUT/configs/cfg_${tag}.json" "$WORK/cfg_${tag}.json"
  {
    echo "cell=$cell"; echo "run=$i"; echo "solver=palace"
    echo "env=OMP_NUM_THREADS=1"
    echo "image=$IMAGE"
    echo "linear_type=$linear"
    echo "pin=docker --cpuset-cpus $cpus; mpirun -bind-to core"
    echo "command=palace -np $np -launcher-args '-bind-to core' cfg_${tag}.json"
    echo "start_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "loadavg_before=$(loadavg)"
  } > "$stem.meta"
  : > "$stem.running"
  sample_affinity "palace-x86" "$stem" &
  local sp=$! t0 t1
  t0=$(date +%s.%N)
  timeout "$RUN_TIMEOUT_S" $DOCKER run --rm --cpuset-cpus "$cpus" --ipc=host -e OMP_NUM_THREADS=1 \
    -v "$WORK:/work" -w /work --entrypoint /usr/bin/time "$IMAGE" -v -o "/work/run-time-$tag.txt" \
    palace -np "$np" -launcher-args "-bind-to core" "/work/cfg_${tag}.json" > "$stem.log" 2>&1 < /dev/null || rc=$?
  t1=$(date +%s.%N)
  rm -f "$stem.running"; wait "$sp" || true
  $DOCKER run --rm -v "$WORK:/work" --entrypoint chown "$IMAGE" -R "$(id -u):$(id -g)" /work/postpro "/work/run-time-$tag.txt" >/dev/null 2>&1 || true
  mv "$WORK/run-time-$tag.txt" "$stem.time" 2>/dev/null || echo "no time file" > "$stem.time"
  cp "$WORK/$outrel/eig.csv" "${stem}_eig.csv" 2>/dev/null || echo "no eig.csv for $tag" >> "$RAW/missing.txt"
  rm -f "$WORK/cfg_${tag}.json"
  {
    echo "end_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "loadavg_after=$(loadavg)"
    echo "host_wall_s=$(python3 -c "print(f'{$t1-$t0:.3f}')")"
    echo "exit=$rc"
  } >> "$stem.meta"
  echo "$cell run $i: exit $rc $(grep -F 'Elapsed (wall clock)' "$stem.time" | awk '{print $NF}')" >&2
}

MAXRUNS=$GEODE_RUNS; [ "$PALACE_RUNS" -gt "$MAXRUNS" ] && MAXRUNS=$PALACE_RUNS
for i in $(seq 1 "$MAXRUNS"); do
  while IFS=$'\t' read -r -u 3 cell solver gauge sigma nmodes width pin runs linear; do
    [ "$i" -le "$runs" ] || continue
    echo "$cell" | grep -Eq "$CELL_FILTER" || continue
    [ -f "$RAW/${cell}_run${i}.meta" ] && grep -q '^exit=0$' "$RAW/${cell}_run${i}.meta" && continue
    sleep 5   # let the previous run's page cache / container teardown settle
    if [ "$solver" = geode ]; then
      geode_run "$cell" "$gauge" "$sigma" "$nmodes" "$width" "$pin" "$i"
    else
      save=keep; [ "$gauge" = divfree_nosave ] && save=0
      palace_run "$cell" "$sigma" "$nmodes" "$width" "$i" "$save" "$linear"
    fi
  done 3< "$OUT/cells.tsv"
done
echo BENCH_DONE > "$OUT/BENCH_DONE"
echo "done: $OUT" >&2
