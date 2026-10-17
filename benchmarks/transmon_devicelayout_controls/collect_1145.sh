#!/usr/bin/env bash
# Issue #1145 T7: copy the Palace adaptive-refinement run on the published
# final design back from the worker into the scratch directory, then lay out
# the committed cell in reference/fixtures/transmon_palace/results_controls_1145/
# as collect_1135.sh does for #1135 (Palace's postpro CSVs for the last solve
# and every iterationNN/, the config it ran, its log and the /usr/bin/time
# record). Host paths are scrubbed to ~, and IPv4 addresses and EC2 host
# names are blanked.
#
#   benchmarks/transmon_devicelayout_controls/collect_1145.sh <scratch> [sync]
#
# <scratch> holds amr/ (the final design's h mesh final_o1_v22.msh and
# palace_config_amr_final_p2.json). With "sync", first rsync the worker's
# work directory into <scratch>/worker_amr (scripts/aws/geode-worker.sh
# sync-out; the worker is w1145 in us-east-2); without it, only re-lay out.
set -euo pipefail
scr=$1
W=scripts/aws/geode-worker.sh
export AWS_PROFILE=geode-fem-bench AWS_REGION=us-east-2
dst=reference/fixtures/transmon_palace/results_controls_1145
if [[ "${2:-}" == sync ]]; then
  "$W" sync-out w1145 work1145amr/ "$scr/worker_amr/"
fi
scrub() {
  sed -E -e "s#$HOME#~#g" -e "s#/home/ubuntu#~#g" \
    -e 's#ip-[0-9]+-[0-9]+-[0-9]+-[0-9]+#ip-x-x-x-x#g' \
    -e 's#(^|[^0-9.])[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}([^0-9.]|$)#\1x.x.x.x\2#g' "$1"
}
mkdir -p "$dst/logs"
d=$scr/worker_amr
c=amr_final_p2
o=$dst/$c
rm -rf "$o"
mkdir -p "$o"
(cd "$d/postpro/$c" && find . -name '*.csv' -o -name 'palace.json' | while read -r f; do
   mkdir -p "$OLDPWD/$o/$(dirname "$f")"; cp "$f" "$OLDPWD/$o/$f"; done)
cp "$d/palace_config_$c.json" "$o/palace_config.json"
scrub "$d/palace_run_$c.log" > "$o/palace_run.log"
scrub "$d/time_$c.txt" > "$o/time.txt"
scrub "$d/orchestrate.log" > "$dst/logs/orchestrate_amr1145.log"
scrub "$d/mem.log" > "$dst/logs/mem_amr1145.log"
(cd "$scr/amr" && shasum -a 256 final_o1_v22.msh) > "$dst/logs/final_mesh_sha256.txt"
echo "iterations in $o: $(find "$o" -mindepth 1 -maxdepth 1 -type d -name 'iteration*' | wc -l | tr -d ' ')"
