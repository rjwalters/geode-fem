#!/bin/bash
req=$1; key=$(basename $(dirname $req)); ch=$(basename $req .json); out=/Volumes/Stripe/scratch-codex-review/refine-binary-equivalence-20261021/results/$key/$ch
mkdir -p $(dirname $out); rm -rf $out
mesh=/Volumes/Stripe/scratch-codex-review/exp2-fd-refine-direct-20261021/mesh-cache/$(python3 -I -c "import json,sys;print(json.load(open(sys.argv[1]))['reference_mesh_sha256'])" /Volumes/Stripe/scratch-codex-review/exp2-fd-refine-direct-20261021/bundle/$req)
RAYON_NUM_THREADS=1 GEODE_NUM_THREADS=1 /Volumes/Stripe/scratch-codex-review/refine-binary-equivalence-20261021/icem_final_refinement.c75fe847 --request /Volumes/Stripe/scratch-codex-review/exp2-fd-refine-direct-20261021/bundle/$req --mesh $mesh --out $out >/dev/null 2>$out.stderr
