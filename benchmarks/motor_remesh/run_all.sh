#!/usr/bin/env bash
# Remesh-and-transfer validation of the motor design loop (issue #1184).
# From the repository root:
#
#   benchmarks/motor_remesh/run_all.sh [meshes|fixture|identity|path|all]
#
# Needs the upstream cache (benchmarks/motor_reference/extract_reference.py
# --fetch) for the initial L1/L2 meshes, and gmsh 4.15.2 through uv for the
# fresh meshes. Every run is single-threaded (RAYON_NUM_THREADS =
# GEODE_NUM_THREADS = 1, faer Par::Seq). The runs of one stage are started
# concurrently, so wall times are load-perturbed. Set CARGO_TARGET_DIR to a
# private directory.
set -euo pipefail
cd "$(dirname "$0")/../.."
R=benchmarks/motor_remesh
GEO=benchmarks/motor_reference/geometry
STAGE="${1:-all}"
mkdir -p "$R/logs" "$R/runs" "$R/meshes"
cargo build -p geode-core --release --example icem_remesh_path
EX="${CARGO_TARGET_DIR:-target}/release/examples/icem_remesh_path"
COMMIT="$(git rev-parse HEAD)"
RUN=(env RAYON_NUM_THREADS=1 GEODE_NUM_THREADS=1 GEODE_COMMIT="$COMMIT")

if [[ "$STAGE" == meshes || "$STAGE" == all ]]; then
    # Initial NURBS-multipatch family L1-L2 (gitignored; same bytes as #1162).
    uv run -q --with numpy==2.5.4 --with gmsh==4.15.2 python -I "$GEO/build_sector_mesh.py" \
        --levels 1 2 --out "$GEO/meshes" >"$R/logs/build_initial_L1-L2.log" 2>&1
fi

if [[ "$STAGE" == fixture || "$STAGE" == all ]]; then
    # The committed CI fixture: a fresh construction mesh of θ0 at L0.
    "$EX" fresh --level 0 --design initial --out "$R/fixtures" >"$R/logs/fixture_L0.log" 2>&1
fi

if [[ "$STAGE" == identity || "$STAGE" == all ]]; then
    "${RUN[@]}" "$EX" identity --level 1 --out "$R/runs/identity_L1.json" >"$R/logs/identity_L1.log" 2>&1
fi

if [[ "$STAGE" == path || "$STAGE" == all ]]; then
    for L in 1 2; do
        "${RUN[@]}" "$EX" path --level "$L" --steps 10 --mesh-dir "$R/meshes/L$L" \
            --out "$R/runs/path_L$L.json" >"$R/logs/path_L$L.log" 2>&1 &
    done
    "${RUN[@]}" "$EX" path --level 1 --steps 10 --warm cold --mesh-dir "$R/meshes/L1_cold" \
        --out "$R/runs/path_L1_cold.json" >"$R/logs/path_L1_cold.log" 2>&1 &
    wait
fi
