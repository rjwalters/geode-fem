#!/usr/bin/env bash
# Issue #1092: run the Palace cells of the DeviceLayout cross-check.
#
#   WORK=<dir> ./run.sh <cell>[:<np>] [<cell>[:<np>] ...]
#
# $WORK holds the meshes and the configs palace_config_<cell>.json (from
# mkcfg.py and generate_transmon_param.jl --dl-config, the latter copied to
# palace_config_dl_<tag>.json). <np> MPI ranks (default 16). One cell at a
# time; the log, /usr/bin/time record and Palace's postpro/<cell>/ outputs
# land in $WORK. Image: palace:cpu-1043, built on the worker from
# reference/palace/docker/Dockerfile (see provenance.txt).
set -u
: "${WORK:?set WORK to the directory holding the meshes and the generated configs}"
IMG=${IMG:-palace:cpu-1043}
DOCKER=${DOCKER:-docker}
for spec in "$@"; do
  tag=${spec%%:*}
  np=16
  [[ "$spec" == *:* ]] && np=${spec#*:}
  rm -rf "$WORK/postpro/$tag"
  mkdir -p "$WORK/postpro"
  echo "start $tag np=$np $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  $DOCKER run --rm --ipc=host -e OMP_NUM_THREADS=1 \
    -v "$WORK:/work" -w /work --entrypoint /usr/bin/time "$IMG" -v -o "/work/time_$tag.txt" \
    palace -np "$np" "/work/palace_config_$tag.json" > "$WORK/palace_run_$tag.log" 2>&1 < /dev/null
  echo "end $tag exit=$? $(date -u +%Y-%m-%dT%H:%M:%SZ)"
done
