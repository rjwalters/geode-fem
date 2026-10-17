#!/usr/bin/env bash
# Issue #956, step 2, run after sweep956.sh showed the capped (8-thread) pool
# beating the sequential solve per call on large systems:
#   percall_refine/  the per-call legs again (repeats 2 and 3) at 8 threads and
#                    at the default pool (RAYON_NUM_THREADS unset), then the
#                    crossover sizes (repeats 1 and 2);
#   e2e_large/       end to end at the largest sizes, main vs ungated.
#
# usage: refine956.sh <main-binary> <ungated-binary> <run-dir>
set -euo pipefail
main_bin=$1
ungated_bin=$2
out=$3
mkdir -p "$out/percall_refine" "$out/e2e_large"
leg() { # <bin> <file> <env...>
  local bin=$1 file=$2
  shift 2
  env "$@" "$bin" --ignored --nocapture --test-threads=1 > "$file" 2>&1
}
percall() { # <file> <site> <n> <env...>
  local file=$1 site=$2 n=$3
  shift 3
  leg "$main_bin" "$file" DIAG956_MODE=percall DIAG956_SITE=$site DIAG956_N=$n \
    DIAG956_REPS=400 DIAG956_STEPS=200 "$@"
}

for rep in 2 3; do
  for t in 8 default; do
    th=()
    [ "$t" = default ] || th=(RAYON_NUM_THREADS=$t)
    for n in 20 30 34; do
      percall "$out/percall_refine/complex_n${n}_t${t}_rep${rep}.stdout" complex $n ${th[@]+"${th[@]}"}
    done
    for n in 30 34; do
      percall "$out/percall_refine/ams_n${n}_t${t}_rep${rep}.stdout" ams $n ${th[@]+"${th[@]}"}
    done
    for n in 10 16 24; do
      percall "$out/percall_refine/transient_n${n}_t${t}_rep${rep}.stdout" transient $n ${th[@]+"${th[@]}"}
    done
  done
done

for rep in 1 2; do
  for t in 8 default; do
    th=()
    [ "$t" = default ] || th=(RAYON_NUM_THREADS=$t)
    for build in main ungated; do
      bin=$main_bin
      [ "$build" = ungated ] && bin=$ungated_bin
      leg "$bin" "$out/e2e_large/complex_n34_${build}_t${t}_rep${rep}.stdout" \
        DIAG956_SITE=complex DIAG956_N=34 DIAG956_NEV=20 ${th[@]+"${th[@]}"}
      leg "$bin" "$out/e2e_large/transient_n24_${build}_t${t}_rep${rep}.stdout" \
        DIAG956_SITE=transient DIAG956_N=24 DIAG956_STEPS=200 ${th[@]+"${th[@]}"}
    done
  done
done

for rep in 1 2; do
  for t in 8 default; do
    th=()
    [ "$t" = default ] || th=(RAYON_NUM_THREADS=$t)
    for n in 22 24 26 28; do
      percall "$out/percall_refine/complex_n${n}_t${t}_rep${rep}.stdout" complex $n ${th[@]+"${th[@]}"}
    done
    for n in 11 12 13 14 15; do
      percall "$out/percall_refine/transient_n${n}_t${t}_rep${rep}.stdout" transient $n ${th[@]+"${th[@]}"}
    done
  done
done
