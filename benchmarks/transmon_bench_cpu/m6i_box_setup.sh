#!/usr/bin/env bash
# On-box setup for the same-box like-for-like transmon re-timing (issues #927,
# #763): geode's three "return Palace's six modes" routes and Palace itself on
# one CPU host. Run ONCE on a fresh Ubuntu 22.04 x86_64 box.
#
# Expects $WORK to contain (scp'd from the repo checkout):
#   Dockerfile            reference/palace/docker/Dockerfile
#   transmon_smoke.msh    crates/geode-core/tests/fixtures/transmon_smoke.msh
#   palace_config.json    reference/fixtures/transmon_palace/palace_config.json
#
# Does, in parallel:
#   (a) docker build of palace:cpu (Palace pinned to the Dockerfile's PALACE_REF)
#   (b) rustup + release build of geode's transmon_bench example at GEODE_REF
#       (niced, so the Palace build keeps priority)
# then converts the mesh to MSH 2.2 (MFEM rejects the MSH 4.1 fixture) and
# writes host and build provenance into $WORK/provenance/.
#
# usage: GEODE_REF=<commit> ./m6i_box_setup.sh
# Then:  ./m6i_box_bench.sh
set -euo pipefail
WORK="${WORK:-$HOME/like-for-like}"
GEODE_REF="${GEODE_REF:?set GEODE_REF to the geode-fem commit to benchmark}"
PROV="$WORK/provenance"
mkdir -p "$PROV"
cd "$WORK"

EXPECTED_MESH_SHA=5b3ff4c357a4dc905e7a2e42abc8178778b626e47582c7bbe610f95d332b33dd
echo "$EXPECTED_MESH_SHA  transmon_smoke.msh" | sha256sum -c - | tee "$PROV/mesh-sha-check.txt"

# Keep the box idle during the timed runs: no background package work.
sudo systemctl stop unattended-upgrades apt-daily.timer apt-daily-upgrade.timer 2>/dev/null || true

sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
  docker.io gmsh build-essential pkg-config time git curl util-linux > "$PROV/apt-install.log" 2>&1
DOCKER=docker
docker info >/dev/null 2>&1 || DOCKER="sudo docker"

# ---- host provenance --------------------------------------------------------
# No `uname -n`, no IP, no instance id: cloud hostnames encode the address.
imds() {
  local tok
  tok=$(curl -s -m 2 -X PUT http://169.254.169.254/latest/api/token \
        -H 'X-aws-ec2-metadata-token-ttl-seconds: 60' 2>/dev/null || true)
  curl -s -m 2 -H "X-aws-ec2-metadata-token: $tok" "http://169.254.169.254/latest/meta-data/$1" 2>/dev/null || true
}
{
  echo "date_utc=$(date -u +%FT%TZ)"
  echo "instance_type=$(imds instance-type)"
  echo "region=$(imds placement/region)"
  echo "os=$(. /etc/os-release && echo "$PRETTY_NAME") ($(uname -m))"
  echo "kernel=$(uname -sr)"
  echo "cpu=$(sed -n 's/^model name[^:]*: //p' /proc/cpuinfo | head -1)"
  echo "logical_cpus=$(nproc)"
  echo "physical_cores=$(lscpu -p=CORE,SOCKET | grep -v '^#' | sort -u | wc -l)"
  echo "ram_gib=$(( $(sed -n 's/^MemTotal: *\([0-9]*\).*/\1/p' /proc/meminfo) / 1048576 ))"
  echo "docker=$($DOCKER --version)"
  echo "fixture_sha256=$EXPECTED_MESH_SHA"
} > "$PROV/host.txt"
lscpu > "$PROV/lscpu.txt"
lscpu -e > "$PROV/lscpu-e.txt"
free -g > "$PROV/free-g.txt"

# ---- (a) Palace CPU docker build -------------------------------------------
(
  t0=$(date +%s); rc=0
  mkdir -p "$WORK/docker-ctx" && cp "$WORK/Dockerfile" "$WORK/docker-ctx/"
  $DOCKER build -t palace:cpu -f "$WORK/docker-ctx/Dockerfile" "$WORK/docker-ctx" > "$PROV/palace-docker-build.out" 2>&1 || rc=$?
  echo "palace docker build: rc=$rc wall_s=$(( $(date +%s) - t0 ))" > "$PROV/palace-build-time.txt"
  exit "$rc"
) &
PALACE_PID=$!

# ---- (b) geode release build -------------------------------------------------
(
  t0=$(date +%s)
  if ! command -v cargo >/dev/null && [ ! -f "$HOME/.cargo/env" ]; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal > "$PROV/rustup.out" 2>&1
  fi
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
  cd "$HOME"
  [ -d geode-fem ] || git clone -q https://github.com/rjwalters/geode-fem.git geode-fem
  cd geode-fem && git fetch -q origin && git checkout -q --detach "$GEODE_REF"
  nice -n 19 cargo build --release -p geode-core --example transmon_bench > "$PROV/geode-cargo-build.out" 2>&1
  {
    echo "geode_commit=$(git rev-parse HEAD) dirty=$(git status --porcelain --untracked-files=no | wc -l | xargs)"
    echo "geode_commit_date=$(git log -1 --format=%cI)"
    echo "rustc=$(rustc --version)"
    echo "cargo=$(cargo --version)"
    echo "geode_build=cargo build --release -p geode-core --example transmon_bench"
    echo "geode_build_wall_s=$(( $(date +%s) - t0 ))"
  } > "$PROV/geode-version.txt"
) > "$PROV/geode-setup.out" 2>&1 &
GEODE_PID=$!

# ---- mesh -> MSH 2.2 -------------------------------------------------------
gmsh --version > "$PROV/gmsh-version.txt" 2>&1 || true
[ -f transmon_smoke_v22.msh ] || gmsh transmon_smoke.msh -0 -save -format msh2 -o transmon_smoke_v22.msh > "$PROV/gmsh-convert.out" 2>&1
sha256sum transmon_smoke.msh transmon_smoke_v22.msh > "$PROV/mesh-sha.txt"

wait "$GEODE_PID" || echo "geode build FAILED (see geode-cargo-build.out)" >> "$PROV/geode-version.txt"
wait "$PALACE_PID" || echo "palace build FAILED (see palace-docker-build.out)" >> "$PROV/palace-build-time.txt"

# ---- Palace build provenance from inside the image ------------------------
$DOCKER run --rm --entrypoint bash palace:cpu -c \
  'cat /opt/palace/palace-build-info.txt; echo; mpichversion 2>/dev/null | head -3 || mpirun --version | head -3; gcc --version | head -1; cat /etc/redhat-release' \
  > "$PROV/palace-image-provenance.txt" 2>&1 || true
$DOCKER image inspect palace:cpu --format 'image_id={{.Id}} created={{.Created}} size={{.Size}}' > "$PROV/palace-image-id.txt" 2>&1 || true
echo SETUP_DONE > "$PROV/SETUP_DONE"
