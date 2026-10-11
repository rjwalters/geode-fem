#!/usr/bin/env bash
# Issue #1043: reproduce the three Palace cells in results_p1_target2/.
#
#   WORK=<scratch dir> ./run.sh
#
# Prerequisites (see provenance.txt):
#   docker build --cpuset-cpus 0-11 -t palace:cpu-1043 \
#     -f reference/palace/docker/Dockerfile reference/palace/docker   # DOCKER_BUILDKIT=0
#   gmsh crates/geode-core/tests/fixtures/transmon_smoke.msh -0 -save -format msh2 \
#     -o "$WORK/transmon_smoke_v22.msh"
#   python3 -I mkcfg.py reference/fixtures/transmon_palace/palace_config.json "$WORK"
set -u
: "${WORK:?set WORK to the directory holding the mesh and the generated configs}"
IMG=${IMG:-palace:cpu-1043}
for tag in control_target4p5 target2_L1 target2_L2; do
  rm -rf "$WORK/postpro/$tag"
  mkdir -p "$WORK/postpro"
  echo "start $tag $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  docker run --rm --cpuset-cpus 0-7 --ipc=host -e OMP_NUM_THREADS=1 \
    -v "$WORK:/work" -w /work --entrypoint /usr/bin/time "$IMG" -v -o "/work/time_$tag.txt" \
    palace -np 8 "/work/palace_config_$tag.json" > "$WORK/palace_run_$tag.log" 2>&1 < /dev/null
  echo "end $tag exit=$? $(date -u +%Y-%m-%dT%H:%M:%SZ)"
done
