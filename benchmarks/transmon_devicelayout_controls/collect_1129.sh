#!/usr/bin/env bash
# Issue #1129: copy the T1/T2/T3 Palace outputs back from the worker into
# the scratch directory, then lay out the committed cells in
# reference/fixtures/transmon_palace/results_controls_1129/ (one directory
# per cell: Palace's postpro CSVs, the config it ran, its log, the
# /usr/bin/time record and, for DeviceLayout-discretization designs, the
# generator report), as collect.sh does for #1123. Host paths are scrubbed
# to ~, and IPv4 addresses and EC2 host names are blanked.
#
#   benchmarks/transmon_devicelayout_controls/collect_1129.sh <scratch> [sync]
#
# <scratch> holds sp/ (driver.py scaling-palace), pr/ (prima_driver.jl) and
# fam_cv2/ (the cv2 h mesh and family-b configs). With "sync", first rsync
# the worker's work directories into <scratch>/worker_ctl and
# <scratch>/worker_fam (scripts/aws/geode-worker.sh sync-out; the worker is
# in us-east-2); without it, only re-lay out.
set -euo pipefail
scr=$1
W=scripts/aws/geode-worker.sh
export AWS_PROFILE=geode-fem-bench AWS_REGION=us-east-2
dst=reference/fixtures/transmon_palace/results_controls_1129
if [[ "${2:-}" == sync ]]; then
  "$W" sync-out w1129 work1129ctl/ "$scr/worker_ctl/"
  "$W" sync-out w1129 work1129fam/ "$scr/worker_fam/"
fi
scrub() {
  sed -E -e "s#$HOME#~#g" -e "s#/home/ubuntu#~#g" \
    -e 's#ip-[0-9]+-[0-9]+-[0-9]+-[0-9]+#ip-x-x-x-x#g' \
    -e 's#(^|[^0-9.])[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}([^0-9.]|$)#\1x.x.x.x\2#g' "$1"
}
mkdir -p "$dst/logs"
# cell <worker dir> <cell> [generator report]
cell() {
  local wd=$1 c=$2 rep=${3:-}
  local o=$dst/$c
  rm -rf "$o"
  mkdir -p "$o"
  (cd "$wd/postpro/$c" && find . -name '*.csv' -o -name 'palace.json' | while read -r f; do
     mkdir -p "$OLDPWD/$o/$(dirname "$f")"; cp "$f" "$OLDPWD/$o/$f"; done)
  cp "$wd/palace_config_$c.json" "$o/palace_config.json"
  scrub "$wd/palace_run_$c.log" > "$o/palace_run.log"
  scrub "$wd/time_$c.txt" > "$o/time.txt"
  if [[ -n "$rep" ]]; then scrub "$rep" > "$o/generator_report.txt"; fi
}
d=$scr/worker_ctl
for c in $(cd "$d/postpro" && ls -d dl_sp* dl_pr* 2>/dev/null); do
  tag=${c#dl_}
  case $tag in
    sp*) rep=$scr/sp/${tag}_report.txt ;;
    pr*) rep=$scr/pr/${tag}_report.txt ;;
  esac
  cell "$d" "$c" "$rep"
done
d=$scr/worker_fam
for c in famcv2_u1_p1 famcv2_u2_p1 famcv2_u1_p2; do
  if [[ -d "$d/postpro/$c" ]]; then cell "$d" "$c"; fi
done
scrub "$d/orchestrate.log" > "$dst/logs/orchestrate_fam.log"
scrub "$d/mem.log" > "$dst/logs/mem_fam.log"
scrub "$scr/fam_cv2/cv2_report.txt" > "$dst/logs/cv2_generator_report.txt"
(cd "$scr/fam_cv2" && shasum -a 256 cv2_o1.msh cv2_o1_v22.msh) > "$dst/logs/cv2_mesh_sha256.txt"
cp "$scr/sp/controls_log.jsonl" "$dst/logs/scaling_palace_log.jsonl"
cp "$scr/pr/prima_log.jsonl" "$dst/logs/prima_log.jsonl"
cp "$scr/pr/prima_summary.json" "$dst/logs/prima_summary.json"
cp "$scr/pr/controls_log.jsonl" "$dst/logs/prima_evals_log.jsonl"
echo "cells in $dst: $(find "$dst" -mindepth 1 -maxdepth 1 -type d ! -name logs | wc -l | tr -d ' ')"
