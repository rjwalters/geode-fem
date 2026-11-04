#!/usr/bin/env bash
# Run every request in the bundle, one single-threaded process each, P in parallel.
# Each check writes a new results/<design12>/<check>/ directory; existing ones are skipped (rerun-safe).
set -euo pipefail
ROOT=/Volumes/Stripe/scratch-codex-review/exp2-fd-refine-direct-20261021
BIN=$1 P=${2:-8}
export ROOT BIN RAYON_NUM_THREADS=1 GEODE_NUM_THREADS=1
mkdir -p "$ROOT/results"
shasum -a 256 "$BIN" > "$ROOT/results/binary.sha256"
# L3 first (longest), then the rest
python3 -I -c '
import json,sys
j=json.load(open(sys.argv[1]+"/bundle/cohort.json"))["jobs"]
for r in sorted(j,key=lambda r:(r["check"]!="L3",r["check"]!="g40",r["design"])):print(r["request"],r["level"])
' "$ROOT" | xargs -P "$P" -L 1 bash -c '
req=$0 level=$1; d=$(dirname "$req"); key=$(basename "$d"); check=$(basename "$req" .json)
out="$ROOT/results/$key/$check"; [ -e "$out/result.json" ] && exit 0; rm -rf "$out"; mkdir -p "$(dirname "$out")"
mesh=$ROOT/mesh-cache/$(python3 -I -c "import json,sys;print(json.load(open(sys.argv[1]))[\"reference_mesh_sha256\"])" "$ROOT/bundle/$req")
s=$(date +%s); "$BIN" --request "$ROOT/bundle/$req" --mesh "$mesh" --out "$out" > /dev/null 2> "$out.stderr" || true
echo "$(date -u +%FT%TZ) $key $check $(( $(date +%s)-s ))s $(python3 -I -c "import json,sys;r=json.load(open(sys.argv[1]));print(r.get(\"ok\"),r.get(\"status\"),r.get(\"error\",\"\"))" "$out/result.json" 2>/dev/null || echo NO_RESULT)" >> "$ROOT/results/progress.log"
'
echo done >> "$ROOT/results/progress.log"
