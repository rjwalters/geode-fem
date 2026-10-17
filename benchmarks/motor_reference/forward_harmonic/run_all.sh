#!/usr/bin/env bash
# Harmonic-truncated rotor-stator coupling study for the ICEM initial design
# (issue #1170). Runs sequentially, so the per-solve wall times are not
# perturbed by concurrent runs, then builds the tables. From the repository
# root:
#
#   benchmarks/motor_reference/forward_harmonic/run_all.sh
#
# Needs the L1-L3 meshes (benchmarks/motor_reference/geometry/README.md) and
# the conforming baseline records in ../forward/runs (issue #1168; the
# conforming code path is unchanged, and an L0 rerun is bit-identical).
# Set CARGO_TARGET_DIR to a private directory if other builds share the
# default one.
set -euo pipefail
cd "$(dirname "$0")/../../.."
HF=benchmarks/motor_reference/forward_harmonic
mkdir -p "$HF/logs" "$HF/runs"
cargo build -p geode-core --release --example icem_initial_torque
BIN="${CARGO_TARGET_DIR:-target}/release/examples/icem_initial_torque"
run() { # name, args...
    local name=$1
    shift
    echo "== $name: $*"
    "$BIN" "$@" --out "$HF/runs/$name.json" >"$HF/logs/$name.log" 2>&1
    tail -2 "$HF/logs/$name.log"
}
# The reference's coupling: orders n = 3:6:45 (8 orders, 16 sin/cos modes).
for L in 0 1 2 3; do
    run "L${L}_h45" --level "$L" --coupling harmonic:3:6:45
done
# Every order below the grid's Nyquist limit: must reproduce the conforming
# band (../forward/runs) to round-off.
for L in 0 1; do
    run "L${L}_hall" --level "$L" --coupling harmonic:all
done
# Order ladder at fixed mesh level: n = 3:6:n_max.
for L in 1 2; do
    for NMAX in 21 93 189 381; do
        run "L${L}_h${NMAX}" --level "$L" --coupling "harmonic:3:6:${NMAX}"
    done
done
run L3_h93 --level 3 --coupling harmonic:3:6:93
# The reference's end treatment: coupling-circle end nodes left out of the
# antiperiodic pairs (setAntiPeriodicBoundaries excludes the coupling DOFs).
for L in 0 1 2 3; do
    run "L${L}_h45f" --level "$L" --coupling harmonic:3:6:45 --ends free
done
uv run -q --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \
    python -I "$HF/analyze.py" >"$HF/logs/analyze.log" 2>&1
tail -3 "$HF/logs/analyze.log"
