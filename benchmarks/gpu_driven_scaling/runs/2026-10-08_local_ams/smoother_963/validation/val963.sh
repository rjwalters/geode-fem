#!/usr/bin/env bash
# Issue #963: the AMS tests outside the cube fixture (the #961 validation
# set), one test per process, per (build, variant).
# usage: val963.sh <tree-name> <checkout> [VAR=value ...]
#   CARGO_TARGET_DIR must be the checkout's target directory.
# Writes <tree>_<stem>.txt (3-line header, then the test output with machine
# paths replaced by <repo>, <cargo-target>, <home>, <tmp>) and .time.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
tree=$1 dir=$2; shift 2
loadavg() { sysctl -n vm.loadavg 2>/dev/null | tr -d '{}' | xargs || cut -d' ' -f1-3 /proc/loadavg; }
# stem | package | test target (--test name or --lib) | test name | ignored?
TESTS="
spiral_smoke|geode-cli|spiral_golden|spiral_smoke_ams_matches_direct_lu|
spiral_benchmark|geode-cli|spiral_golden|spiral_benchmark_ams_matches_direct_lu|ignored
spiral_aniso|geode-cli|anisotropic_golden|spiral_ams_with_anisotropic_materials_matches_direct|
spiral_ds|geode-cli|dispersive_golden|spiral_ds_ams_matches_direct|
spiral_drude|geode-cli|dispersive_golden|spiral_drude_ams_matches_direct|
spiral_debye|geode-cli|dispersive_golden|spiral_debye_ams_matches_direct|
rough|geode-cli|roughness_golden|rough_ams_matches_direct|
driven_public|geode-core|--lib|driven::solve::tests::ams_selectable_on_public_iterative_path_matches_direct|
"
cd "$dir"
echo "$TESTS" | while IFS='|' read -r stem pkg target name ign; do
  [ -n "$stem" ] || continue
  if [ "$target" = --lib ]; then sel=(--lib); else sel=(--test "$target"); fi
  bin=$(cargo test --release -p "$pkg" "${sel[@]}" --no-run --message-format=json 2>/dev/null \
    | grep '"test":true' | sed -nE 's/.*"executable":"([^"]+\/deps\/[^"]+)".*/\1/p' | tail -1)
  out="$here/${tree}_${stem}.txt"
  extra=(); [ -n "$ign" ] && extra=(--ignored)
  {
    echo "# tree=$tree test=$name"
    echo "# git=$(git rev-parse HEAD) dirty=$(git status --porcelain --untracked-files=no | wc -l | xargs) env=${*:-none}"
    echo "# start=$(date -u +%FT%TZ) loadavg_start=$(loadavg | tr ' ' ',')"
  } > "$out"
  (cd "crates/$pkg" && env "$@" GEODE_AMS_SMOOTH_REPORT=1 RAYON_NUM_THREADS=1 /usr/bin/time -l -o "$here/${tree}_${stem}.time" \
     "$bin" "$name" --exact "${extra[@]}" --nocapture --test-threads 1) 2>&1 \
    | sed -e "s#$dir#<repo>#g" -e "s#${CARGO_TARGET_DIR}#<cargo-target>#g" -e "s#$HOME#<home>#g" \
          -e "s#/private/var/folders/[^ \"]*#<tmp>#g" -e "s#/var/folders/[^ \"]*#<tmp>#g" >> "$out"
  echo "$tree $stem rc=${PIPESTATUS[0]}"
done
