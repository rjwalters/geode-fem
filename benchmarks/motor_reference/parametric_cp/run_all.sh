#!/usr/bin/env bash
# ICEM free-form control-point design space (issue #1179). From the
# repository root:
#
#   benchmarks/motor_reference/parametric_cp/run_all.sh [geometry|meshes|torque|gradient|remesh|test|analyze|all]
#
# Needs the upstream cache (../extract_reference.py --fetch) for the
# geometry stage, and gmsh 4.15.2 through uv for the meshes. The torque runs
# are sequential; the gradient and remesh runs are independent processes and
# run in parallel (their wall times are not reported). Set CARGO_TARGET_DIR
# to a private directory if other builds share the default one.
set -euo pipefail
cd "$(dirname "$0")/../../.."
D=benchmarks/motor_reference/parametric_cp
GEO=benchmarks/motor_reference/geometry
DES=benchmarks/motor_shape_gradient/design
STAGE="${1:-all}"
mkdir -p "$D/logs" "$D/runs" "$D/meshes"
cargo build -p geode-core --release --example icem_cp_parametric --example icem_cp_gradient \
    --example icem_initial_torque
EX="${CARGO_TARGET_DIR:-target}/release/examples"
PY=(uv run -q --with numpy==2.5.4)
GMSH=(uv run -q --with numpy==2.5.4 --with gmsh==4.15.2)

# The optimized design's unscaled full-load current and load angle
# (reference_data.json designs.optimized), as in ../parametric/run_all.sh.
I_UNSCALED=11.048494105125824
PHI0=-11.646272734037339

if [[ "$STAGE" == geometry || "$STAGE" == all ]]; then
    # Control points -> export fixture -> least-squares offsets -> boundary check.
    "$EX/icem_cp_parametric" table --out "$D/cp_table.json" >"$D/logs/table.log" 2>&1
    "${PY[@]}" python -I "$D/fit_offsets.py" fixture --table "$D/cp_table.json" \
        --out "$D/opt120_control_points.json" >"$D/logs/fixture.log" 2>&1
    "$EX/icem_cp_parametric" fit --fixture "$D/opt120_control_points.json" \
        --out "$D/opt120_offsets.json" >"$D/logs/fit.log" 2>&1
    "$EX/icem_cp_parametric" curves --out "$D/curves_cp.json" >"$D/logs/curves.log" 2>&1
    "${PY[@]}" python -I "$D/fit_offsets.py" boundary --curves "$D/curves_cp.json" \
        --out "$D/boundary_check_cp.json" >"$D/logs/boundary_check_cp.log" 2>&1
    tail -3 "$D/logs/fit.log"
fi

if [[ "$STAGE" == meshes || "$STAGE" == all ]]; then
    # Initial family L1-L3 (gitignored; byte-identical to the #1162 reports).
    "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --levels 1 2 3 --out "$GEO/meshes" \
        >"$D/logs/build_initial_L1-L3.log" 2>&1
    for L in 0 1; do
        "$EX/icem_cp_parametric" sweep --level "$L" --steps 100 --out "$D/runs/sweep_L$L.json" \
            >"$D/logs/sweep_L$L.log" 2>&1
        "$EX/icem_cp_parametric" fd --level "$L" --out "$D/runs/fd_L$L.json" \
            >"$D/logs/fd_L$L.log" 2>&1
    done
    # The full iteration-120 design per extension variant; then the moved
    # meshes (the move refuses a mesh with an inverted triangle).
    : >"$D/logs/quality_variants.log"
    : >"$D/logs/move.log"
    for L in 0 1 2 3; do
        for V in "slide 0" "slide 0.5" "slide 1" "stretch 0"; do
            set -- $V
            "$EX/icem_cp_parametric" quality --level "$L" --design opt120 --edges "$1" \
                --stiffening "$2" --out "$D/runs/quality_L${L}_$1_chi$2.json" \
                >>"$D/logs/quality_variants.log" 2>&1
        done
        "$EX/icem_cp_parametric" move --level "$L" --design opt120 \
            --out "$D/meshes/icem_opt120cp_sector_L$L.msh" >>"$D/logs/move.log" 2>&1 \
            || echo "L$L: move refused (inverted triangles)" >>"$D/logs/move.log"
    done
fi

if [[ "$STAGE" == torque || "$STAGE" == all ]]; then
    for L in 0 1 2 3; do
        M="$D/meshes/icem_opt120cp_sector_L$L.msh"
        if [[ ! -f "$M" ]]; then
            echo "== opt120cp_L${L}_h45: no moved mesh (see logs/move.log)"
            continue
        fi
        echo "== opt120cp_L${L}_h45"
        "$EX/icem_initial_torque" --level "$L" --mesh "$M" --coupling harmonic:3:6:45 \
            --i-peak "$I_UNSCALED" --phi0-deg "$PHI0" --out "$D/runs/opt120cp_L${L}_h45.json" \
            >"$D/logs/opt120cp_L${L}_h45.log" 2>&1
        tail -1 "$D/logs/opt120cp_L${L}_h45.log"
    done
fi

if [[ "$STAGE" == gradient || "$STAGE" == all ]]; then
    for DS in initial opt120; do
        # L0: every component; L1: the offsets; L2: the offsets, full current,
        # decade steps.
        "$EX/icem_cp_gradient" study --level 0 --design "$DS" --components all --currents 1,2 \
            --out "$D/runs/gradient_L0_$DS.json" >"$D/logs/gradient_L0_$DS.log" 2>&1 &
        "$EX/icem_cp_gradient" study --level 1 --design "$DS" --currents 1,2 \
            --out "$D/runs/gradient_L1_$DS.json" >"$D/logs/gradient_L1_$DS.log" 2>&1 &
        "$EX/icem_cp_gradient" study --level 2 --design "$DS" --currents 2 \
            --hs 1e-3,1e-4,1e-5,1e-6,1e-7,1e-8,1e-9 \
            --out "$D/runs/gradient_L2_$DS.json" >"$D/logs/gradient_L2_$DS.log" 2>&1 &
    done
    wait
fi

if [[ "$STAGE" == remesh || "$STAGE" == all ]]; then
    "$EX/icem_cp_gradient" plan --out "$D/meshes/remesh_plan.json" >"$D/logs/plan.log" 2>&1
    for L in 0 1 2; do
        "${GMSH[@]}" python -I "$DES/build_design_mesh.py" --plan "$D/meshes/remesh_plan.json" \
            --levels "$L" --out "$D/meshes" --summary "$D/runs/remesh_meshes_L$L.json" \
            >"$D/logs/build_remesh_L$L.log" 2>&1 &
    done
    wait
    for L in 0 1 2; do
        "$EX/icem_cp_gradient" remesh --level "$L" --mesh-dir "$D/meshes" --currents 1,2 \
            --out "$D/runs/remesh_L$L.json" >"$D/logs/remesh_L$L.log" 2>&1 &
    done
    wait
fi

if [[ "$STAGE" == test || "$STAGE" == all ]]; then
    cargo test -p geode-core --release --test motor_icem_controlpoints -- --nocapture \
        >"$D/logs/test_motor_icem_controlpoints.log" 2>&1
    tail -3 "$D/logs/test_motor_icem_controlpoints.log"
fi

if [[ "$STAGE" == analyze || "$STAGE" == all ]]; then
    python3 -I "$D/analyze.py" >"$D/logs/analyze.log" 2>&1
    tail -3 "$D/logs/analyze.log"
fi
