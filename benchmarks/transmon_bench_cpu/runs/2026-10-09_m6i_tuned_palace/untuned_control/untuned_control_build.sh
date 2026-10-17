#!/bin/bash
# Same-box untuned control (issue #927): build reference/palace/docker/Dockerfile
# unchanged as palace:cpu on the box that ran the tuned sweep, record the same
# provenance as the tuned image, then the driver re-times the four fixture-request
# Palace cells with IMAGE=palace:cpu into run-untuned/.
set -uo pipefail
DOCKER=docker; docker info >/dev/null 2>&1 || DOCKER="sudo docker"
cd "$HOME/tuned-palace"
P=provenance-untuned
t0=$(date +%s); rc=0
$DOCKER build -t palace:cpu -f ctx-untuned/Dockerfile ctx-untuned > "$P/palace-docker-build.out" 2>&1 || rc=$?
echo "palace docker build: rc=$rc wall_s=$(( $(date +%s) - t0 ))" > "$P/palace-build-time.txt"
$DOCKER run --rm --entrypoint bash palace:cpu -c "cat /opt/palace/palace-build-info.txt; echo; mpichversion 2>/dev/null | head -3; cat /etc/redhat-release; rpm -qa | grep -i blas" > "$P/palace-image-provenance.txt" 2>&1
$DOCKER image inspect palace:cpu --format "image_id={{.Id}} created={{.Created}} size={{.Size}}" > "$P/palace-image-id.txt" 2>&1
$DOCKER run --rm --entrypoint bash -v "$PWD:/w" palace:cpu -c "dnf install -y -q binutils findutils >/dev/null 2>&1; /w/isa_check.sh" > "$P/palace-image-isa.txt" 2>&1
echo DONE > "$P/BUILD_DONE"
