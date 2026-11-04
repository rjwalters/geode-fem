#!/usr/bin/env bash
# Issue #1135: copy the T4/T5 Palace outputs back from the worker into the
# scratch directory, then lay out the committed cells in
# reference/fixtures/transmon_palace/results_controls_1135/ exactly as
# collect_1129.sh does for #1129 (one directory per cell: Palace's postpro
# CSVs, the config it ran, its log, the /usr/bin/time record and, for
# DeviceLayout-discretization designs, the generator report). Host paths are
# scrubbed to ~, and IPv4 addresses and EC2 host names are blanked.
#
#   benchmarks/transmon_devicelayout_controls/collect_1135.sh <scratch> [sync]
#
# <scratch> holds pa/ and pb/ (prima_driver.jl --rhobeg 0.1 --prefix pa and
# --rhobeg 0.4 --prefix pb) and fam_final/ (the final design's h mesh in
# MSH 2.2 and the family-b configs). With "sync", first rsync the worker's
# work directories into <scratch>/worker_ctl and <scratch>/worker_fam
# (scripts/aws/geode-worker.sh sync-out; the worker is w1135 in us-east-1);
# without it, only re-lay out.
set -euo pipefail
scr=$1
W=scripts/aws/geode-worker.sh
export AWS_PROFILE=geode-fem-bench AWS_REGION=us-east-1
dst=reference/fixtures/transmon_palace/results_controls_1135
if [[ "${2:-}" == sync ]]; then
  "$W" sync-out w1135 work1135ctl/ "$scr/worker_ctl/"
  "$W" sync-out w1135 work1135fam/ "$scr/worker_fam/"
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
for c in $(cd "$d/postpro" && ls -d dl_pa* dl_pb* 2>/dev/null); do
  tag=${c#dl_}
  cell "$d" "$c" "$scr/${tag:0:2}/${tag}_report.txt"
done
d=$scr/worker_fam
for c in famfinal_u1_p1 famfinal_u2_p1 famfinal_u1_p2; do
  if [[ -d "$d/postpro/$c" ]]; then cell "$d" "$c"; fi
done
scrub "$d/orchestrate.log" > "$dst/logs/orchestrate_fam.log"
scrub "$d/mem.log" > "$dst/logs/mem_fam.log"
(cd "$scr/fam_final" && shasum -a 256 final_o1_v22.msh) > "$dst/logs/final_mesh_sha256.txt"
cp "$scr/rhobeg_probe.txt" "$dst/logs/rhobeg_probe.txt"  # julia benchmarks/transmon_devicelayout_controls/rhobeg_probe.jl
for p in pa:prima_rhobeg0p1 pb:prima_rhobeg0p4; do
  src=$scr/${p%%:*}
  stem=${p#*:}
  cp "$src/prima_log.jsonl" "$dst/logs/${stem}_log.jsonl"
  if [[ -f "$src/prima_summary.json" ]]; then cp "$src/prima_summary.json" "$dst/logs/${stem}_summary.json"; fi
  cp "$src/controls_log.jsonl" "$dst/logs/${stem}_evals_log.jsonl"
done
echo "cells in $dst: $(find "$dst" -mindepth 1 -maxdepth 1 -type d ! -name logs | wc -l | tr -d ' ')"
