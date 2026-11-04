#!/usr/bin/env bash
# Experiment-2 prerequisites (issue #1193, of #1185): the θ0 anchor (F0 and
# the θ0 validation mean), the reproduction of the #1182 h93 records by the
# validation mode, and the finite-difference checks of the frozen design
# spaces. From the repository root:
#
#   benchmarks/motor_optimization/run_all.sh [meshes|anchor|reproduce|fd|all]
#
# Needs the upstream cache (benchmarks/motor_reference/extract_reference.py
# --fetch) and gmsh 4.15.2 through uv. The meshes stage rebuilds the
# gitignored meshes of #1182 (the NURBS-multipatch family L1-L3, the Opt120
# export L2-L3, the construction meshes L2-L3) and checks each SHA-256
# against the committed reports. Every run is single-threaded
# (RAYON_NUM_THREADS = GEODE_NUM_THREADS = 1, faer Par::Seq); the values do
# not depend on the host load, the wall times do (recorded). JOBS runs are
# started concurrently (default 8). Set CARGO_TARGET_DIR to a private
# directory.
set -euo pipefail
cd "$(dirname "$0")/../.."
O=benchmarks/motor_optimization
V=benchmarks/motor_validation
GEO=benchmarks/motor_reference/geometry
DES=benchmarks/motor_shape_gradient/design
STAGE="${1:-all}"
JOBS="${JOBS:-8}"
mkdir -p "$O/logs" "$O/runs"
cargo build -p geode-core --release --example icem_experiment2
EX="${CARGO_TARGET_DIR:-target}/release/examples/icem_experiment2"
GMSH=(uv run -q --with numpy==2.5.4 --with gmsh==4.15.2)
export GEODE_COMMIT="$(git rev-parse HEAD)"
export GEODE_TREE_CLEAN="$(if git diff --quiet HEAD -- crates; then echo yes; else echo no; fi)"
export GEODE_RUSTC="$(rustc -V)"
export RAYON_NUM_THREADS=1 GEODE_NUM_THREADS=1

if [[ "$STAGE" == meshes || "$STAGE" == all ]]; then
    "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --levels 1 2 3 --out "$GEO/meshes" \
        >"$O/logs/build_initial_L1-L3.log" 2>&1
    for L in 2 3; do
        "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --design optimized --levels "$L" \
            --out "$V/meshes" >"$O/logs/build_export_L$L.log" 2>&1
        "${GMSH[@]}" python -I "$DES/build_design_mesh.py" --plan "$V/meshes/plan.json" \
            --levels "$L" --out "$V/meshes" >"$O/logs/build_construction_L$L.log" 2>&1
    done
    python3 -I "$O/check_meshes.py" | tee "$O/logs/check_meshes.log"
fi

run() { # name, args...
    local n=$1
    shift
    env GEODE_HOST_LOAD="$(uptime | sed 's/.*load averages*: //')" \
        GEODE_STARTED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        "$EX" "$@" >"$O/logs/$n.log" 2>&1 || echo "FAILED (exit $?)" >>"$O/logs/$n.log"
    echo "$n: $(tail -1 "$O/logs/$n.log")"
}
export -f run
export EX O

if [[ "$STAGE" == anchor || "$STAGE" == all ]]; then
    run anchor_L2 anchor --mesh "$GEO/meshes/icem_initial_sector_L2.msh" --out "$O/anchor.json"
fi

if [[ "$STAGE" == reproduce || "$STAGE" == all ]]; then
    R="$V/runs"
    JOBFILE="$O/logs/jobs.txt"
    : >"$JOBFILE"
    job() { echo "$*" >>"$JOBFILE"; }
    for L in 3 2; do
        C=primary
        [[ $L == 3 ]] && C=L3
        N="$GEO/meshes/icem_initial_sector_L$L.msh"
        job "theta0_nurbs_L${L}_$C" validate --space b --design initial --mesh "$N" --config $C \
            --policy off --compare "$R/initial_nurbs_L${L}_h93_fixed.json" --out "$O/runs/theta0_nurbs_L${L}_$C.json"
        job "theta0_nurbs_L${L}_${C}_space_a" validate --space a --design initial --mesh "$N" --config $C \
            --policy off --compare "$R/initial_nurbs_L${L}_h93_fixed.json" --out "$O/runs/theta0_nurbs_L${L}_${C}_space_a.json"
        job "theta0_construction_L${L}_$C" validate --space b --design initial \
            --mesh "$V/meshes/icem_remesh_theta0_L$L.msh" --config $C --policy off \
            --compare "$R/initial_construction_L${L}_h93_fixed.json" --out "$O/runs/theta0_construction_L${L}_$C.json"
        job "opt120_export_L${L}_$C" validate --space b --design opt120 --mesh-design opt120 \
            --mesh "$V/meshes/icem_opt120_sector_L$L.msh" --config $C --policy off \
            --compare "$R/opt120_export_L${L}_h93_fixed.json" --out "$O/runs/opt120_export_L${L}_$C.json"
        job "opt120_construction_L${L}_$C" validate --space b --design opt120 --mesh-design opt120 \
            --mesh "$V/meshes/icem_remesh_opt120_L$L.msh" --config $C --policy off \
            --compare "$R/opt120_construction_L${L}_h93_fixed.json" --out "$O/runs/opt120_construction_L${L}_$C.json"
    done
    N2="$GEO/meshes/icem_initial_sector_L2.msh"
    # Opt120 on the moved θ0 mesh at L2 (the policy would fire: min angle 1.3°).
    job opt120_moved_L2_primary validate --space b --design opt120 --mesh "$N2" --config primary \
        --policy off --compare "$R/opt120_moved_L2_h93_fixed.json" --out "$O/runs/opt120_moved_L2_primary.json"
    # g40 spot check on the same meshes.
    job theta0_nurbs_L2_g40 validate --space b --design initial --mesh "$N2" --config g40 \
        --policy off --compare "$R/initial_nurbs_L2_h93_fixed.json" --out "$O/runs/theta0_nurbs_L2_g40.json"
    job opt120_moved_L2_g40 validate --space b --design opt120 --mesh "$N2" --config g40 \
        --policy off --compare "$R/opt120_moved_L2_h93_fixed.json" --out "$O/runs/opt120_moved_L2_g40.json"
    # Conforming outer-band spot check: θ0 (own fresh meshes).
    job theta0_nurbs_L2_conforming validate --space b --design initial --mesh "$N2" --config conforming \
        --compare "$R/initial_nurbs_L2_conforming_fixed.json" --out "$O/runs/theta0_nurbs_L2_conforming.json"
    job theta0_construction_L2_conforming validate --space b --design initial \
        --mesh "$V/meshes/icem_remesh_theta0_L2.msh" --config conforming \
        --compare "$R/initial_construction_L2_conforming_fixed.json" --out "$O/runs/theta0_construction_L2_conforming.json"
    # Opt120 has nonzero stator offsets: the conforming check must refuse it.
    job opt120_conforming_refused validate --space b --design opt120 --mesh "$N2" --config conforming \
        --out "$O/runs/opt120_conforming_refused.json"
    # The policy path: Opt120 from the θ0 L2 mesh fires the policy and is
    # validated on a fresh construction mesh (gmsh), against the #1182
    # construction record.
    job opt120_policy_L2_primary validate --space b --design opt120 --mesh "$N2" --config primary \
        --compare "$R/opt120_construction_L2_h93_fixed.json" --out "$O/runs/opt120_policy_L2_primary.json"
    # shellcheck disable=SC2016
    xargs -P "$JOBS" -L 1 bash -c 'run "$@"' _ <"$JOBFILE" | tee "$O/logs/reproduce.log"
fi

if [[ "$STAGE" == fd || "$STAGE" == all ]]; then
    for S in a b; do
        run "fd_L1_space_$S" fd --space $S --level 1 --mesh "$GEO/meshes/icem_initial_sector_L1.msh" \
            --out "$O/runs/fd_L1_space_$S.json" &
    done
    wait
fi
