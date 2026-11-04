#!/usr/bin/env bash
# ICEM design parametrization as fixed-topology node motion, and the forward
# check of the optimized design (issue #1173). From the repository root:
#
#   benchmarks/motor_reference/parametric/run_all.sh [geometry|meshes|torque|scaling|analyze|all]
#
# Needs the upstream cache (../extract_reference.py --fetch) for the boundary
# check and the meshes, and gmsh 4.15.2 through uv for the meshes. Runs are
# sequential so the per-solve wall times are not perturbed. Set
# CARGO_TARGET_DIR to a private directory if other builds share the default
# one.
set -euo pipefail
cd "$(dirname "$0")/../../.."
PM=benchmarks/motor_reference/parametric
GEO=benchmarks/motor_reference/geometry
STAGE="${1:-all}"
mkdir -p "$PM/logs" "$PM/runs" "$PM/meshes"
cargo build -p geode-core --release --example icem_parametric --example icem_initial_torque
EX="${CARGO_TARGET_DIR:-target}/release/examples"
PY=(uv run -q --with numpy==2.5.4)
GMSH=(uv run -q --with numpy==2.5.4 --with gmsh==4.15.2)

# The optimized design (reference_data.json designs.optimized): unscaled peak
# current history_unscaled_current_amplitudes_A[2] = 11.57569753783308 / ScaleR,
# load angle OPERATING_ANGLE (degrees). Torques are converted to the saved
# arrays' ScaleR / LENGTH in analyze.py.
I_UNSCALED=11.048494105125824
I_PHYSICAL=11.57569753783308
PHI0=-11.646272734037339
SCALE_R=1.0477172207986847

if [[ "$STAGE" == geometry || "$STAGE" == all ]]; then
    "$EX/icem_parametric" curves --out "$PM/curves.json" >"$PM/logs/curves.log" 2>&1
    "${PY[@]}" python -I "$PM/check_boundaries.py" --curves "$PM/curves.json" \
        --out "$PM/boundary_check.json" --fixture "$PM/boundary_samples.json" \
        >"$PM/logs/check_boundaries.log" 2>&1
    tail -1 "$PM/logs/check_boundaries.log"
fi

if [[ "$STAGE" == meshes || "$STAGE" == all ]]; then
    # Initial family L1-L3 (gitignored; byte-identical to the #1162 reports).
    "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --levels 1 2 3 --out "$GEO/meshes" \
        >"$PM/logs/build_initial_L1-L3.log" 2>&1
    # Fresh meshes of the optimized design: full export, and scalar-only.
    for D in optimized optimized-scalar; do
        "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --design "$D" --levels 0 1 2 3 \
            --out "$PM/meshes" >"$PM/logs/build_$D.log" 2>&1
    done
    # Quality sweeps over the parameter box and Jacobian FD checks.
    for L in 0 1; do
        "$EX/icem_parametric" sweep --level "$L" --out "$PM/runs/sweep_L$L.json" \
            >"$PM/logs/sweep_L$L.log" 2>&1
        "$EX/icem_parametric" fd --level "$L" --out "$PM/runs/fd_L$L.json" \
            >"$PM/logs/fd_L$L.log" 2>&1
    done
    # Optimized-design quality per extension variant; then the moved meshes.
    : >"$PM/logs/quality_variants.log"
    for L in 0 1 2 3; do
        for V in "stretch 0" "slide 0" "slide 1"; do
            set -- $V
            "$EX/icem_parametric" quality --level "$L" --edges "$1" --stiffening "$2" \
                --out "$PM/runs/quality_L${L}_$1_chi$2.json" >>"$PM/logs/quality_variants.log" 2>&1
        done
        "$EX/icem_parametric" move --level "$L" --design optimized \
            --out "$PM/meshes/icem_opt120moved_sector_L$L.msh" >>"$PM/logs/move.log" 2>&1
    done
fi

run() { # name, args...
    local name=$1
    shift
    echo "== $name: $*"
    "$EX/icem_initial_torque" "$@" --out "$PM/runs/$name.json" >"$PM/logs/$name.log" 2>&1
    tail -2 "$PM/logs/$name.log"
}

if [[ "$STAGE" == torque || "$STAGE" == all ]]; then
    for L in 0 1 2 3; do
        for M in opt120moved opt120scalar opt120; do
            run "${M}_L${L}_h45" --level "$L" --mesh "$PM/meshes/icem_${M}_sector_L$L.msh" \
                --coupling harmonic:3:6:45 --i-peak "$I_UNSCALED" --phi0-deg "$PHI0"
        done
    done
fi

if [[ "$STAGE" == torque || "$STAGE" == scaling || "$STAGE" == all ]]; then
    # ScaleR similarity check: the moved L0 mesh scaled by ScaleR at the
    # physical current gives ScaleR^2 times the unscaled coupling torque.
    run opt120moved_L0_h45_scaled --level 0 --mesh "$PM/meshes/icem_opt120moved_sector_L0.msh" \
        --coupling harmonic:3:6:45 --i-peak "$I_PHYSICAL" --phi0-deg "$PHI0" --scale-mesh "$SCALE_R"
fi

if [[ "$STAGE" == analyze || "$STAGE" == all ]]; then
    uv run -q --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \
        python -I "$PM/analyze.py" >"$PM/logs/analyze.log" 2>&1
    tail -3 "$PM/logs/analyze.log"
fi
