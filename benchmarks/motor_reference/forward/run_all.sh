#!/usr/bin/env bash
# Run the full ICEM initial-design forward study (issue #1168) sequentially,
# so the per-solve wall times are not perturbed by concurrent runs, then
# build the tables and the figure. From the repository root:
#
#   benchmarks/motor_reference/forward/run_all.sh
#
# Needs the L1-L3 meshes (benchmarks/motor_reference/geometry/README.md).
# Set CARGO_TARGET_DIR to a private directory if other builds share the
# default one.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FWD=benchmarks/motor_reference/forward
mkdir -p "$FWD/logs" "$FWD/runs"
cargo build -p geode-core --release --example icem_initial_torque
BIN="${CARGO_TARGET_DIR:-target}/release/examples/icem_initial_torque"
run() { # name, args...
    local name=$1
    shift
    echo "== $name: $*"
    "$BIN" "$@" >"$FWD/logs/$name.log" 2>&1
    tail -2 "$FWD/logs/$name.log"
}
# Baseline: M27 table law, default Newton tolerance, mesh group areas,
# with the harmonic-resolved torque diagnostic.
for L in 0 1 2 3; do
    run "L${L}_table_default_mesh" --level "$L" --spectrum
done
# Sensitivity (each against the baseline at the same level).
run L2_clamp426_default_mesh --level 2 --law clamp426
run L3_clamp426_default_mesh --level 3 --law clamp426
run L2_table_tight_mesh --level 2 --tol tight
run L3_table_tight_mesh --level 3 --tol tight --currents 2
run L2_table_default_exact --level 2 --area exact
uv run -q --with numpy==2.5.4 --with matplotlib==3.11.2 --with pillow==11.3.0 \
    python -I "$FWD/analyze.py" >"$FWD/logs/analyze.log" 2>&1
tail -3 "$FWD/logs/analyze.log"
