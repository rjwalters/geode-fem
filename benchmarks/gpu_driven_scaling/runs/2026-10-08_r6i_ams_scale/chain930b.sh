#!/usr/bin/env bash
# #930 at-scale chain, part B: Palace on the same box, after part A.
# The image must already exist (palace_build.sh). Meshes are exported by the
# geode harness in a separate, untimed process first.
set -u
S="${S:-$HOME/bench}"
R="${R:-$HOME/run930}"
export OUT="$R/palace" MESHES="$R/meshes"
ALL="15 20 24 36 40"
# 8 ranks first (the #520 configuration), then 1 rank; three runs of each.
for n in $ALL; do "$S/box_palace_cpu.sh" "$n" 8 0-7 3; done
for n in $ALL; do "$S/box_palace_cpu.sh" "$n" 1 0 3; done
echo CHAIN_B_DONE
