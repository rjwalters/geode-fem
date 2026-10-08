#!/usr/bin/env bash
# #930: build the Palace CPU image on the box (not timed as part of any cell;
# nothing else runs meanwhile), then export the five meshes with the geode
# harness. S holds this branch's reference/palace/docker/Dockerfile.
set -u
S="${S:-$HOME/bench}"
R="${R:-$HOME/run930}"
REF=fba6a5b9d17ef09db2907e9c4c6f92d123d131c0
mkdir -p "$R/palace" "$R/meshes" "$HOME/palace-ctx"
cp "$S/Dockerfile" "$HOME/palace-ctx/Dockerfile"
t0=$SECONDS
sudo docker build -t palace:cpu --build-arg PALACE_REF=$REF -f "$HOME/palace-ctx/Dockerfile" "$HOME/palace-ctx" \
  > "$HOME/palace_build_full.log" 2>&1
rc=$?
{
  echo "palace_ref=$REF"
  echo "build_rc=$rc"
  echo "build_wall_s=$((SECONDS - t0))"
  echo "dockerfile_sha256=$(sha256sum "$S/Dockerfile" | cut -d' ' -f1)"
  echo "image_id=$(sudo docker image inspect palace:cpu --format '{{.Id}}' 2>/dev/null)"
  echo "base_image=$(sudo docker image inspect rockylinux:9 --format '{{index .RepoDigests 0}}' 2>/dev/null)"
} > "$R/palace/build_provenance.txt"
echo "PALACE_BUILD rc=$rc"
