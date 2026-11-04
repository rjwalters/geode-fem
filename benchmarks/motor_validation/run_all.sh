#!/usr/bin/env bash
# shellcheck disable=SC2086 # COMMON is split into words on purpose
# Motor experiment 1 (issue #1181): validate the optimization target at θ0
# and Opt120 across level, coupling, angle grid and current. From the
# repository root:
#
#   benchmarks/motor_validation/run_all.sh [meshes|eval|slot|threads|analyze|all]
#
# Needs the upstream cache (benchmarks/motor_reference/extract_reference.py
# --fetch) for the export meshes, and gmsh 4.15.2 through uv. Every eval job
# runs single-threaded (RAYON_NUM_THREADS = GEODE_NUM_THREADS = 1, faer
# Par::Seq): on the measured host the 2-D sparse LU did not speed up with
# 4 or 8 threads (README). JOBS independent processes run concurrently
# (default 8); per-solve wall times are therefore load-perturbed and are
# reported as such. Set CARGO_TARGET_DIR to a private directory.
#
# A further design (for example the first new feasible candidate) is added
# with EXTRA_DESIGN=file:PATH.json (see icem_validation.rs for the format):
# it is evaluated on its moved mesh (L0-L3, invalid levels recorded) and on
# a fresh mesh of the GEODE construction at every level.
set -euo pipefail
cd "$(dirname "$0")/../.."
V=benchmarks/motor_validation
GEO=benchmarks/motor_reference/geometry
DES=benchmarks/motor_shape_gradient/design
STAGE="${1:-all}"
JOBS="${JOBS:-8}"
mkdir -p "$V/logs" "$V/runs" "$V/meshes"
cargo build -p geode-core --release --example icem_validation
EX="${CARGO_TARGET_DIR:-target}/release/examples"
GMSH=(uv run -q --with numpy==2.5.4 --with gmsh==4.15.2)
COMMIT="$(git rev-parse HEAD)"
DESIGNS=(initial opt120)
[[ -n "${EXTRA_DESIGN:-}" ]] && DESIGNS+=("$EXTRA_DESIGN")

name_of() { # design spec -> name
    case "$1" in
        file:*) python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["name"])' "${1#file:}" ;;
        vector:*) local r=${1#vector:}; echo "${r%%:*}" ;;
        *) echo "$1" ;;
    esac
}

timed() { # log, cmd... : append the wall time of cmd to log
    local log=$1
    shift
    local t0 t1
    t0=$(python3 -I -c 'import time; print(time.time())')
    "$@" >"$log" 2>&1
    t1=$(python3 -I -c 'import time; print(time.time())')
    python3 -I -c 'import sys; print(f"wall_s {float(sys.argv[2]) - float(sys.argv[1]):.3f}")' "$t0" "$t1" >>"$log"
}

if [[ "$STAGE" == meshes || "$STAGE" == all ]]; then
    # Initial NURBS-multipatch family L1-L3 (gitignored; byte-identical to #1162).
    timed "$V/logs/build_initial_L1-L3.log" \
        "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --levels 1 2 3 --out "$GEO/meshes"
    # Fresh meshes of the Opt120 export (the #1176 family), one level per call
    # so each remesh is timed on its own.
    for L in 0 1 2 3; do
        timed "$V/logs/build_fresh_opt120_L$L.log" \
            "${GMSH[@]}" python -I "$GEO/build_sector_mesh.py" --design optimized --levels "$L" \
            --out "$V/meshes"
    done
    # Fresh meshes of the GEODE construction (any design vector).
    ARGS=()
    for D in "${DESIGNS[@]}"; do ARGS+=(--design "$D"); done
    "$EX/icem_validation" plan "${ARGS[@]}" --out "$V/meshes/plan.json" >"$V/logs/plan.log" 2>&1
    for L in 0 1 2 3; do
        timed "$V/logs/build_construction_L$L.log" \
            "${GMSH[@]}" python -I "$DES/build_design_mesh.py" --plan "$V/meshes/plan.json" \
            --levels "$L" --out "$V/meshes" --summary "$V/runs/construction_meshes_L$L.json"
    done
fi

if [[ "$STAGE" == eval || "$STAGE" == all ]]; then
    JOBFILE="$V/logs/jobs.txt"
    : >"$JOBFILE"
    job() { # run-name, args...
        local n=$1
        shift
        echo "$n $*" >>"$JOBFILE"
    }
    # Longest first.
    for L in 3 2 1 0; do
        for C in harmonic:3:6:45 harmonic:3:6:93 conforming; do
            case $C in harmonic:3:6:45) CN=h45 ;; harmonic:3:6:93) CN=h93 ;; *) CN=conforming ;; esac
            COMMON="--level $L --coupling $C --angles 0:0.5:40 --threads 1"
            for D in "${DESIGNS[@]}"; do
                N=$(name_of "$D")
                if [[ "$N" == initial ]]; then
                    job "initial_nurbs_L${L}_${CN}_fixed" --design "$D" --source nurbs --family nurbs-multipatch $COMMON --rule fixed
                else
                    job "${N}_moved_L${L}_${CN}_fixed" --design "$D" --source moved --family moved $COMMON --rule fixed
                fi
                MN=$N
                [[ "$N" == initial ]] && MN=theta0 # the plan's name for θ0
                job "${N}_construction_L${L}_${CN}_fixed" --design "$D" --source fresh --family fresh-construction \
                    --mesh "$V/meshes/icem_remesh_${MN}_L$L.msh" $COMMON --rule fixed
            done
            M="$V/meshes/icem_opt120_sector_L$L.msh"
            job "opt120_export_L${L}_${CN}_fixed" --design opt120 --source fresh --family fresh-export --mesh "$M" $COMMON --rule fixed
            # Controls (Opt120 only: at θ0 every rule gives the fixed current).
            for R in fixed-physical jinit reference; do
                job "opt120_export_L${L}_${CN}_$R" --design opt120 --source fresh --family fresh-export --mesh "$M" \
                    $COMMON --rule "$R" --currents 0.5,1
                if [[ $L -lt 3 ]]; then
                    job "opt120_moved_L${L}_${CN}_$R" --design opt120 --source moved --family moved $COMMON \
                        --rule "$R" --currents 0.5,1
                fi
            done
        done
    done
    export EX V COMMIT
    run_one() {
        local n=$1
        shift
        env RAYON_NUM_THREADS=1 GEODE_NUM_THREADS=1 GEODE_COMMIT="$COMMIT" \
            "$EX/icem_validation" eval "$@" --out "$V/runs/$n.json" >"$V/logs/$n.log" 2>&1 \
            || echo "FAILED (exit $?)" >>"$V/logs/$n.log"
        echo "$n: $(tail -1 "$V/logs/$n.log")"
    }
    export -f run_one
    T0=$(date +%s)
    # shellcheck disable=SC2016
    xargs -P "$JOBS" -L 1 bash -c 'run_one "$@"' _ <"$JOBFILE" | tee "$V/logs/eval.log"
    echo "eval wall $(($(date +%s) - T0)) s with $JOBS concurrent jobs" | tee -a "$V/logs/eval.log"
fi

if [[ "$STAGE" == slot || "$STAGE" == all ]]; then
    # The candidate slot: the Opt120 vector through file: and vector: input,
    # against the opt120 record (must be identical torques).
    VEC=$(python3 -I -c 'import json,sys; print(",".join(repr(x) for x in json.load(open(sys.argv[1]))["design_vector"]))' \
        "$V/runs/opt120_moved_L0_h45_fixed.json")
    for S in "file:$V/candidate_template.json" "vector:candidate_vector:$VEC"; do
        n=slotcheck_${S%%:*}_L0
        env RAYON_NUM_THREADS=1 GEODE_NUM_THREADS=1 GEODE_COMMIT="$COMMIT" "$EX/icem_validation" eval \
            --design "$S" --level 0 --source moved --family moved --angles 0:0.5:40 --threads 1 \
            --out "$V/runs/$n.json" >"$V/logs/$n.log" 2>&1
        tail -1 "$V/logs/$n.log"
    done
fi

if [[ "$STAGE" == threads || "$STAGE" == all ]]; then
    # Thread scaling of one L3 h45 solve chain (theta0, f = 1, 8 angles).
    for T in 1 4 8; do
        env RAYON_NUM_THREADS=$T GEODE_NUM_THREADS=$T GEODE_COMMIT="$COMMIT" "$EX/icem_validation" eval \
            --design initial --level 3 --source nurbs --angles 0:0.5:8 --currents 1 --threads $T \
            --out "$V/runs/threads_L3_t$T.json" >"$V/logs/threads_L3_t$T.log" 2>&1
        tail -1 "$V/logs/threads_L3_t$T.log"
    done
fi

if [[ "$STAGE" == analyze || "$STAGE" == all ]]; then
    uv run -q --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \
        python -I "$V/analyze.py" >"$V/logs/analyze.log" 2>&1
    tail -3 "$V/logs/analyze.log"
fi
