#!/usr/bin/env bash
# ICEM design gradients dJ/dtheta: FD/Taylor validation (issue #1177).
# From the repository root:
#
#   benchmarks/motor_shape_gradient/design/run_all.sh [meshes|study|remesh|cost|test|analyze|all]
#
# Needs the upstream cache (../../motor_reference/extract_reference.py --fetch)
# and gmsh 4.15.2 through uv for the L1/L2 initial meshes and the remeshed
# designs. The study and remesh runs are independent processes and run in
# parallel; the cost stage runs alone, sequentially. Set CARGO_TARGET_DIR to a
# private directory if other builds share the default one.
set -euo pipefail
cd "$(dirname "$0")/../../.."
D=benchmarks/motor_shape_gradient/design
GEO=benchmarks/motor_reference/geometry
STAGE="${1:-all}"
mkdir -p "$D/logs" "$D/runs" "$D/meshes"
cargo build -p geode-core --release --example icem_design_gradient
EX="${CARGO_TARGET_DIR:-target}/release/examples/icem_design_gradient"
GMSH=(uv run -q --with numpy==2.5.4 --with gmsh==4.15.2)

if [[ "$STAGE" == meshes || "$STAGE" == all ]]; then
    # Initial family L1-L2 (gitignored; byte-identical to the #1162 reports).
    "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --levels 1 2 --out "$GEO/meshes" \
        >"$D/logs/build_initial_L1-L2.log" 2>&1
    # Fresh meshes of theta0 and theta0 +- h e_k for the remeshed cross-check.
    "$EX" plan --out "$D/meshes/remesh_plan.json" >"$D/logs/plan.log" 2>&1
    for L in 0 1 2; do
        "${GMSH[@]}" python -I "$D/build_design_mesh.py" --plan "$D/meshes/remesh_plan.json" \
            --levels "$L" --out "$D/meshes" --summary "$D/runs/remesh_meshes_L$L.json" \
            >"$D/logs/build_remesh_L$L.log" 2>&1 &
    done
    wait
fi

if [[ "$STAGE" == study || "$STAGE" == all ]]; then
    for L in 0 1 2; do
        # L2 (36k nodes): full current, prescribed rule, decade steps only.
        if [[ "$L" == 2 ]]; then
            EXTRA=(--currents 2 --hs 1e-3,1e-4,1e-5,1e-6,1e-7,1e-8,1e-9)
        else
            EXTRA=(--currents 0,1,2 --jinit)
        fi
        for M in h45 conforming; do
            for DS in initial perturbed; do
                "$EX" study --level "$L" --mode "$M" --design "$DS" "${EXTRA[@]}" \
                    --out "$D/runs/study_L${L}_${M}_${DS}.json" \
                    >"$D/logs/study_L${L}_${M}_${DS}.log" 2>&1 &
            done
        done
        wait
    done
fi

if [[ "$STAGE" == study || "$STAGE" == all ]]; then
    # Gradient only at L3 (139k nodes), for the mesh-convergence table.
    "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --levels 3 --out "$GEO/meshes" \
        >"$D/logs/build_initial_L3.log" 2>&1
    for M in h45 conforming; do
        for DS in initial perturbed; do
            "$EX" study --level 3 --mode "$M" --design "$DS" --currents 2 --gradient-only \
                --out "$D/runs/gradient_L3_${M}_${DS}.json" \
                >"$D/logs/gradient_L3_${M}_${DS}.log" 2>&1 &
        done
    done
    wait
fi

if [[ "$STAGE" == remesh || "$STAGE" == all ]]; then
    for L in 0 1 2; do
        "$EX" remesh --level "$L" --mode h45 --mesh-dir "$D/meshes" --currents 1,2 \
            --out "$D/runs/remesh_L${L}_h45.json" >"$D/logs/remesh_L${L}_h45.log" 2>&1 &
    done
    wait
fi

if [[ "$STAGE" == cost || "$STAGE" == all ]]; then
    : >"$D/logs/cost.log"
    uptime >>"$D/logs/cost.log"
    for L in 0 1 2; do
        if [[ "$L" == 2 ]]; then R=1; else R=3; fi
        for M in h45 conforming; do
            for T in 1e-8 1e-14; do
                "$EX" cost --level "$L" --mode "$M" --rel-tol "$T" --repeats "$R" \
                    --out "$D/runs/cost_L${L}_${M}_tol$T.json" >>"$D/logs/cost.log" 2>&1
            done
        done
    done
    uptime >>"$D/logs/cost.log"
fi

if [[ "$STAGE" == test || "$STAGE" == all ]]; then
    cargo test -p geode-core --release --test motor_design_gradient -- --nocapture \
        >"$D/logs/test_motor_design_gradient.log" 2>&1
    tail -3 "$D/logs/test_motor_design_gradient.log"
fi

if [[ "$STAGE" == analyze || "$STAGE" == all ]]; then
    python3 -I "$D/analyze.py" >"$D/logs/analyze.log" 2>&1
    tail -3 "$D/logs/analyze.log"
fi
