#!/usr/bin/env bash
# Issue #1092 part 2: the full resolution ladder at one design (c, L), the
# same cells as the part-1 cross-check (Palace order 1 on h, h/2, h/4;
# order 2 on h, h/2; geode A2 on h, h/2; PEC box, sapphire tensor).
#
#   benchmarks/transmon_devicelayout_optimize/ladder.sh <tag> <c_um> <L_um> <scratch> [worker]
#
# Run from the repository root on the host with Julia and gmsh 4.15.2
# (mesh generation, MSH 2.2 conversion and uniform refinement, as in
# part 1); the solves run detached on the worker (worker_ladder.sh), with
# progress in ~/work1092lad_<tag>/orchestrate.log.
set -euo pipefail
tag=$1; c=$2; L=$3; scr=$4; w=${5:-tq1092}
W=scripts/aws/geode-worker.sh
export AWS_PROFILE=geode-fem-bench
d=$scr/ladder_$tag
mkdir -p "$d"
julia --project=reference/julia/devicelayout reference/julia/generate_transmon_param.jl \
  --cap-length "$c" --total-length "$L" --out-dir "$d" --tag "$tag" --order1-msh41 > "$d/${tag}_gen.log" 2>&1
log=$d/${tag}_gmsh.log
: > "$log"
gmsh "$d/${tag}_o1.msh" -0 -save -format msh2 -o "$d/${tag}_o1_v22.msh" >> "$log" 2>&1
gmsh "$d/${tag}_o1.msh" -refine -format msh41 -o "$d/${tag}_r1.msh" >> "$log" 2>&1
gmsh "$d/${tag}_r1.msh" -0 -save -format msh2 -o "$d/${tag}_r1_v22.msh" >> "$log" 2>&1
gmsh "$d/${tag}_r1.msh" -refine -format msh2 -o "$d/${tag}_r2_v22.msh" >> "$log" 2>&1
: > "$d/cells.txt"
python3 -I reference/fixtures/transmon_palace/results_crosscheck_1092/mkcfg.py \
  reference/fixtures/transmon_palace/palace_config.json "$d" \
  "${tag}_h_p1:${tag}_o1_v22.msh:1:2.0:3:Default:16" \
  "${tag}_h_p2:${tag}_o1_v22.msh:2:2.0:3:AMS:32" \
  "${tag}_r1_p1:${tag}_r1_v22.msh:1:2.0:3:AMS:32" \
  "${tag}_r1_p2:${tag}_r1_v22.msh:2:2.0:3:AMS:32" \
  "${tag}_r2_p1:${tag}_r2_v22.msh:1:2.0:3:AMS:32"
(cd "$d" && shasum -a 256 ./*.msh > sha256.txt)
rdir=work1092lad_$tag
"$W" ssh "$w" "mkdir -p ~/$rdir"
for f in "$d"/*.msh "$d"/palace_config_*.json "$d/cells.txt"; do
  "$W" ssh "$w" "cat > ~/$rdir/$(basename "$f")" < "$f"
done
"$W" ssh "$w" "cd ~/$rdir && nohup bash ~/geode-fem/benchmarks/transmon_devicelayout_optimize/worker_ladder.sh $tag > /dev/null 2>&1 < /dev/null &"
echo "ladder $tag started on $w"
