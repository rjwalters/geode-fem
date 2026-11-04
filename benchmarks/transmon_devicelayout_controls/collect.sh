#!/usr/bin/env bash
# Issue #1123: copy the controls' Palace outputs back from the workers into
# the scratch directory, then lay out the committed cells in
# reference/fixtures/transmon_palace/results_controls_1123/ (one directory
# per cell: Palace's postpro CSVs, the config it ran, its log, the
# /usr/bin/time record and, for DeviceLayout-generated designs, the
# generator report). Host paths are scrubbed to ~.
#
#   benchmarks/transmon_devicelayout_controls/collect.sh <scratch> [sync]
#
# With "sync", first rsync each worker's work directory into <scratch>
# (scripts/aws/geode-worker.sh sync-out); without it, only re-lay out.
set -euo pipefail
scr=$1
W=scripts/aws/geode-worker.sh
export AWS_PROFILE=geode-fem-bench
dst=reference/fixtures/transmon_palace/results_controls_1123
if [[ "${2:-}" == sync ]]; then
  "$W" sync-out dl1123 work1123ctl/ "$scr/worker_dl1123/"
  "$W" sync-out dl1123 work1123fam/ "$scr/worker_dl1123_fam/"
  "$W" sync-out lad1123 work1123lad_sccv1/ "$scr/worker_lad1123/"
  "$W" sync-out amr1123 work1123amr/ "$scr/worker_amr1123/"
fi
scrub() { sed -e "s#$HOME#~#g" "$1"; }
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
d=$scr/worker_dl1123
for c in $(cd "$d/postpro" && ls -d dl_*); do
  tag=${c#dl_}
  rep=$scr/dl/${tag}_report.txt
  [[ -f "$rep" ]] || rep=$scr/regen/initial_report.txt
  cell "$d" "$c" "$rep"
done
d=$scr/worker_lad1123
for c in $(cd "$d/postpro" && ls -d sccv1_*); do
  cell "$d" "$c" "$scr/ladder_sccv1/sccv1_report.txt"
done
scrub "$d/orchestrate.log" > "$dst/logs/orchestrate_lad1123.log"
cp "$scr/ladder_sccv1/sha256.txt" "$dst/logs/sccv1_mesh_sha256.txt"
# fam_u1_p2 ran on dl1123 (moved off amr1123's queue to run in parallel).
d=$scr/worker_dl1123_fam
cell "$d" fam_u1_p2
scrub "$d/orchestrate.log" > "$dst/logs/orchestrate_dl1123_fam.log"
scrub "$d/mem.log" > "$dst/logs/mem_dl1123_fam.log"
scrub "$scr/worker_dl1123/orchestrate_rerun.log" > "$dst/logs/orchestrate_dl1123_rerun.log"
d=$scr/worker_amr1123
for c in amr_initial_p2 fam_u1_p1 fam_u2_p1; do
  cell "$d" "$c"
done
scrub "$d/orchestrate.log" > "$dst/logs/orchestrate_amr1123.log"
scrub "$d/mem.log" > "$dst/logs/mem_amr1123.log"
cp "$scr/dl/controls_log.jsonl" "$dst/logs/controls_log.jsonl"
echo "cells in $dst: $(ls "$dst" | grep -vc logs)"
