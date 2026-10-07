#!/usr/bin/env bash
# On-box setup for the #519 Palace-GPU transmon head-to-head (1 NVIDIA GPU).
#
# Run ONCE on a fresh GPU box with the NVIDIA driver, docker and
# nvidia-container-toolkit (AWS "Deep Learning Base OSS Nvidia Driver GPU AMI
# (Ubuntu 22.04)", or a Lambda Cloud Ubuntu 22.04 image — on Lambda, first run
# `sudo nvidia-ctk runtime configure --runtime=docker && sudo systemctl
# restart docker` so `docker run --gpus` works). Set CUDA_ARCH to the GPU's
# sm (default 89 = L40S; #519 ran with CUDA_ARCH=80 on an A100).
# Expects $WORK to contain (scp'd from the repo checkout):
#   Dockerfile.cuda            reference/palace/docker/Dockerfile.cuda
#   transmon_smoke.msh         crates/geode-core/tests/fixtures/transmon_smoke.msh
#   palace_config.json         reference/fixtures/transmon_palace/palace_config.json
#   palace_config_gpu.json     reference/fixtures/transmon_palace/palace_config_gpu.json
#
# Does, in parallel:
#   (a) docker build of palace:cuda (Palace pinned to PALACE_REF, CUDA sm_$CUDA_ARCH)
#   (b) rustup + release build of geode's transmon_bench example at GEODE_REF
#       (niced, so the Palace build keeps priority)
# then converts the mesh to MSH 2.2 (MFEM rejects the MSH 4.1 fixture) and
# writes host/GPU/build provenance into $WORK/provenance/.
set -euo pipefail
WORK="${WORK:-$HOME/palace-run}"
GEODE_REF="${GEODE_REF:?set GEODE_REF to the geode-fem commit to benchmark}"
PROV="$WORK/provenance"
mkdir -p "$PROV"
cd "$WORK"

DOCKER=docker
docker info >/dev/null 2>&1 || DOCKER="sudo docker"

EXPECTED_MESH_SHA=5b3ff4c357a4dc905e7a2e42abc8178778b626e47582c7bbe610f95d332b33dd
echo "$EXPECTED_MESH_SHA  transmon_smoke.msh" | sha256sum -c - | tee "$PROV/mesh-sha-check.txt"

# ---- host + GPU provenance ------------------------------------------------
{
  date -u +%FT%TZ
  uname -srvmo   # no -n: cloud hostnames encode the public IPv4 (see #925 review)
  cat /etc/os-release | head -4
  lscpu | grep -E 'Model name|^CPU\(s\)|Thread|Core|Socket'
  free -g
  $DOCKER --version
  nvidia-container-cli --version 2>/dev/null | head -2 || true
  curl -s -m 2 http://169.254.169.254/latest/meta-data/instance-type 2>/dev/null; echo
} > "$PROV/host.txt" 2>&1 || true
nvidia-smi > "$PROV/nvidia-smi.txt"
nvidia-smi --query-gpu=name,driver_version,memory.total,compute_cap --format=csv > "$PROV/gpu-info.csv"

# ---- (a) Palace CUDA docker build ----------------------------------------
(
  t0=$(date +%s); rc=0
  mkdir -p "$WORK/docker-ctx" && cp "$WORK/Dockerfile.cuda" "$WORK/docker-ctx/"
  $DOCKER build --build-arg CUDA_ARCH="${CUDA_ARCH:-89}" -t palace:cuda -f "$WORK/docker-ctx/Dockerfile.cuda" "$WORK/docker-ctx" > "$PROV/palace-docker-build.log" 2>&1 || rc=$?
  echo "palace docker build: rc=$rc wall_s=$(( $(date +%s) - t0 ))" > "$PROV/palace-build-time.txt"
  exit "$rc"
) &
PALACE_PID=$!

# ---- (b) geode release build ----------------------------------------------
(
  t0=$(date +%s)
  if ! command -v cargo >/dev/null; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal > "$PROV/rustup.log" 2>&1
  fi
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
  cd "$HOME"
  [ -d geode-fem ] || git clone -q https://github.com/rjwalters/geode-fem.git geode-fem
  cd geode-fem && git checkout -q --detach "$GEODE_REF"
  nice -n 19 cargo build --release -p geode-core --example transmon_bench > "$PROV/geode-cargo-build.log" 2>&1
  { git log -1 --format='%H %cI %s'; rustc --version; cargo --version; } > "$PROV/geode-version.txt"
  echo "geode cargo build: wall_s=$(( $(date +%s) - t0 ))" > "$PROV/geode-build-time.txt"
) > "$PROV/geode-setup.out" 2>&1 &
GEODE_PID=$!

# ---- mesh -> MSH 2.2 -------------------------------------------------------
if ! command -v gmsh >/dev/null; then
  sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq gmsh > "$PROV/gmsh-install.log" 2>&1
fi
gmsh --version > "$PROV/gmsh-version.txt" 2>&1 || true
[ -f transmon_smoke_v22.msh ] || gmsh transmon_smoke.msh -0 -save -format msh2 -o transmon_smoke_v22.msh > "$PROV/gmsh-convert.log" 2>&1
sha256sum transmon_smoke.msh transmon_smoke_v22.msh > "$PROV/mesh-sha.txt"

wait "$GEODE_PID" || echo "geode build FAILED (see geode-cargo-build.log)" >> "$PROV/geode-build-time.txt"
wait "$PALACE_PID" || echo "palace build FAILED (see palace-docker-build.log)" >> "$PROV/palace-build-time.txt"

# ---- Palace build provenance from inside the image ------------------------
$DOCKER run --rm --entrypoint bash palace:cuda -c \
  'cd /opt/palace-src && git log -1 --format="%H %cI %s"; echo; cat /opt/palace-build-flags.txt; echo; echo "mpicxx -show (LTO stripped):"; cat /opt/mpicxx-show.txt; echo; nvcc --version | tail -2; mpichversion 2>/dev/null | head -3 || mpirun --version | head -3' \
  > "$PROV/palace-image-provenance.txt" 2>&1 || true
$DOCKER image inspect palace:cuda --format '{{.Id}} {{.Created}} size={{.Size}}' > "$PROV/palace-image-id.txt" 2>&1 || true
echo SETUP_DONE > "$PROV/SETUP_DONE"
