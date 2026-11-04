#!/usr/bin/env bash
# Issue #1123: the Palace half of #1111's evaluation ladder at one design
# (c, L): Palace order 1 on h, h/2, h/4 and order 2 on h, h/2, PEC box,
# sapphire tensor. Same meshes and configs as
# benchmarks/transmon_devicelayout_optimize/ladder.sh; the geode h, h/2
# same-mesh cells are not repeated (the estimate and band use Palace cells
# only).
#
#   benchmarks/transmon_devicelayout_controls/ladder_palace.sh <tag> <c_um> <L_um> <scratch> <worker> [np]
#
# Run from the repository root on the host with Julia and gmsh 4.15.2;
# the cells run detached on the worker, one at a time, with run.sh.
set -euo pipefail
tag=$1; c=$2; L=$3; scr=$4; w=$5; np=${6:-16}
W=scripts/aws/geode-worker.sh
export AWS_PROFILE=geode-fem-bench
d=$scr/ladder_$tag
mkdir -p "$d"
julia --project=reference/julia/devicelayout reference/julia/generate_transmon_param.jl \
  --cap-length "$c" --total-length "$L" --out-dir "$d" --tag "$tag" --order1-msh41 > "$d/${tag}_gen.log" 2>&1
glog=$d/${tag}_gmsh.log
: > "$glog"
gmsh "$d/${tag}_o1.msh" -0 -save -format msh2 -o "$d/${tag}_o1_v22.msh" >> "$glog" 2>&1
gmsh "$d/${tag}_o1.msh" -refine -format msh41 -o "$d/${tag}_r1.msh" >> "$glog" 2>&1
gmsh "$d/${tag}_r1.msh" -0 -save -format msh2 -o "$d/${tag}_r1_v22.msh" >> "$glog" 2>&1
gmsh "$d/${tag}_r1.msh" -refine -format msh2 -o "$d/${tag}_r2_v22.msh" >> "$glog" 2>&1
: > "$d/cells.txt"
python3 -I reference/fixtures/transmon_palace/results_crosscheck_1092/mkcfg.py \
  reference/fixtures/transmon_palace/palace_config.json "$d" \
  "${tag}_h_p1:${tag}_o1_v22.msh:1:2.0:3:Default:$np" \
  "${tag}_h_p2:${tag}_o1_v22.msh:2:2.0:3:AMS:$np" \
  "${tag}_r1_p1:${tag}_r1_v22.msh:1:2.0:3:AMS:$np" \
  "${tag}_r1_p2:${tag}_r1_v22.msh:2:2.0:3:AMS:$np" \
  "${tag}_r2_p1:${tag}_r2_v22.msh:1:2.0:3:AMS:$np"
(cd "$d" && shasum -a 256 ./*.msh > sha256.txt)
rdir=work1123lad_$tag
"$W" ssh "$w" "mkdir -p ~/$rdir"
for f in "$d"/*_v22.msh "$d"/palace_config_*.json "$d/cells.txt"; do
  "$W" ssh "$w" "cat > ~/$rdir/$(basename "$f")" < "$f"
done
# shellcheck disable=SC2029  # $rdir and $tag expand locally on purpose
"$W" ssh "$w" "cd ~/$rdir && setsid -f bash -c 'cells=(); while read -r cell n; do [ -n \"\$cell\" ] && cells+=(\"\$cell:\$n\"); done < cells.txt; echo \"ladder $tag start \$(date -u +%FT%TZ)\"; WORK=\$HOME/$rdir IMG=palace:cpu-1043 DOCKER=\"sudo docker\" bash ~/geode-fem/reference/fixtures/transmon_palace/results_crosscheck_1092/run.sh \"\${cells[@]}\"; echo \"ladder $tag end \$(date -u +%FT%TZ)\"' > ~/$rdir/orchestrate.log 2>&1 < /dev/null"
echo "ladder $tag started on $w"
