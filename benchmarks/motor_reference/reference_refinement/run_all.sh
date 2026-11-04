#!/usr/bin/env bash
# Reference IGA refinement study (issue #1189): rerun the ICEM initial-design
# torque sweep under Octave at several per-patch subdivisions.
#
#   ICEM_REF_ROOT=/Volumes/Stripe/scratch-icem-ref \
#       benchmarks/motor_reference/reference_refinement/run_all.sh [job ...]
#
# ICEM_REF_ROOT must hold (see README.md, "Setup"):
#   icem-octave/                  output of prepare_octave_tree.py
#   geopdes-official/geopdes/     GeoPDEs at 53ae5ca1
#   nurbs-forge/nurbs-1.4.3/      Octave Forge nurbs 1.4.3
# A job is "s<N>_c<list>", e.g. s3_c123 (3x3, currents 1,2,3 in that order)
# or s24_c1 (24x24, zero current). A suffix _t<k> sets the Newton stop on
# the squared residual to 1e-<k> (default: upstream 1e-6), e.g. s6_c1_t14.
# Jobs run as parallel Octave processes;
# each process is single-threaded in practice.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${ICEM_REF_ROOT:?set ICEM_REF_ROOT}"
REF_JSON="$HERE/../reference_data.json"
mkdir -p "$HERE/runs" "$HERE/logs"
if [ "$#" -eq 0 ]; then
    set -- s3_c123 s6_c123 s12_c1 s12_c2 s12_c3 s24_c1
fi

run_job() {
    local job=$1 n idx tol=""
    n=${job#s}
    n=${n%%_*}
    idx=${job#*_c}
    case $idx in
    *_t*)
        tol="cfg.newton_residual=1e-${idx#*_t};"
        idx=${idx%%_t*}
        ;;
    esac
    idx=$(printf '%s' "$idx" | fold -w 1 | tr '\n' ' ')
    octave-cli --no-gui --no-init-file --norc --eval "
addpath('$HERE');
cfg.icem_root='$ROOT/icem-octave';
cfg.geopdes_inst='$ROOT/geopdes-official/geopdes/inst';
cfg.nurbs_inst='$ROOT/nurbs-forge/nurbs-1.4.3/inst';
cfg.shims_dir='$HERE/shims';
cfg.ref_json='$REF_JSON';
cfg.subdiv=[$n $n]; cfg.current_idx=[$idx]; $tol
cfg.out_json='$HERE/runs/$job.json';
icem_refine_run(cfg);" >"$HERE/logs/$job.log" 2>&1
    echo "done $job: $(grep -E '^(std|total wall)' "$HERE/logs/$job.log" | tr '\n' ' ')"
}

octave-cli --version | head -n 1
date -u +"start %Y-%m-%dT%H:%M:%SZ"
pids=()
for job in "$@"; do
    run_job "$job" &
    pids+=("$!")
done
status=0
for pid in "${pids[@]}"; do
    wait "$pid" || status=1
done
date -u +"end %Y-%m-%dT%H:%M:%SZ"
exit "$status"
