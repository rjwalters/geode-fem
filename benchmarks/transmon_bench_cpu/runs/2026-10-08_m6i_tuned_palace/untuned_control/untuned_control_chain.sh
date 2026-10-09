#!/bin/bash
# Waits for untuned_control_build.sh, then times the four fixture-request Palace
# cells with the untuned image through the same driver (geode cells off).
cd "$HOME/tuned-palace"
until [ -f provenance-untuned/BUILD_DONE ]; do sleep 20; done
grep -q "rc=0" provenance-untuned/palace-build-time.txt || { echo "untuned build failed"; exit 1; }
IMAGE=palace:cpu OUT="$HOME/tuned-palace/run-untuned" GEODE_RUNS=0 PALACE_RUNS=3 \
  CELL_FILTER="^palace_s4p5_n6_np[18](_nosave)?$" ./m6i_tuned_box_bench.sh
