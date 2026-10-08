#!/usr/bin/env bash
# #930 at-scale chain, part D: Palace again with Solver.Linear.Type = "AMS".
# Added during the session, after reading Palace's source showed that the
# "Default" type of part B (and of the #520 run) is a SuperLU-preconditioned
# GMRES in an image built with SuperLU, not AMS. This is the like-for-like
# Palace cell for geode's AMS.
set -u
S="${S:-$HOME/bench}"
R="${R:-$HOME/run930}"
export OUT="$R/palace_ams" MESHES="$R/meshes" PALACE_LINEAR_TYPE=AMS
ALL="15 20 24 36 40"
for n in $ALL; do "$S/box_palace_cpu.sh" "$n" 8 0-7 3; done
for n in $ALL; do "$S/box_palace_cpu.sh" "$n" 1 0 3; done
echo CHAIN_D_DONE
