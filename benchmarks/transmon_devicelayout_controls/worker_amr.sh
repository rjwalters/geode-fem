#!/usr/bin/env bash
# Issue #1123: the worker side of the Palace adaptive-refinement and
# refinement-scheme cells on the DeviceLayout initial mesh. Runs the cells
# named on the command line one at a time with run.sh, and samples the
# host's used memory every 60 s into mem.log (the per-rank /usr/bin/time
# record shows only the largest rank).
#
#   cd ~/work1123amr && setsid -f bash worker_amr.sh amr_initial_p2:32 fam_u1_p1:32 ...
set -u
WORK=$(pwd)
export WORK IMG=palace:cpu-1043 DOCKER="sudo docker"
exec >> "$WORK/orchestrate.log" 2>&1
( while true; do echo "$(date -u +%FT%TZ) $(free -m | awk '/^Mem:/ {print "used_mib=" $3 " total_mib=" $2}')"; sleep 60; done ) >> "$WORK/mem.log" &
sampler=$!
echo "cells start $(date -u +%FT%TZ): $*"
bash "$HOME/geode-fem/reference/fixtures/transmon_palace/results_crosscheck_1092/run.sh" "$@"
echo "cells end $(date -u +%FT%TZ)"
kill "$sampler"
