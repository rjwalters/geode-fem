#!/usr/bin/env bash
# Shape-adjoint validation study for the ICEM initial design (issue #1174).
# Runs the study example once (mesh L0, all modes and currents, Newton
# rel_tol 1e-14) and the CI test, sequentially so the wall times in the
# cost section are not perturbed by concurrent runs. From the repository
# root:
#
#   benchmarks/motor_shape_gradient/run_all.sh
#
# Set CARGO_TARGET_DIR to a private directory if other builds share the
# default one.
set -euo pipefail
cd "$(dirname "$0")/../.."
SG=benchmarks/motor_shape_gradient
mkdir -p "$SG/logs"
cargo build -p geode-core --release --example icem_shape_gradient
BIN="${CARGO_TARGET_DIR:-target}/release/examples/icem_shape_gradient"
"$BIN" --rel-tol 1e-14 --out "$SG/study.json" >"$SG/tables.md" 2>"$SG/logs/study.log"
tail -1 "$SG/logs/study.log"
cargo test -p geode-core --release --test motor_shape_gradient -- --nocapture --test-threads 1 \
    >"$SG/logs/test_motor_shape_gradient.log" 2>&1
tail -1 "$SG/logs/test_motor_shape_gradient.log"
cargo test -p geode-core --release --lib magnetostatic_shape -- --nocapture \
    >"$SG/logs/test_magnetostatic_shape_unit.log" 2>&1
tail -1 "$SG/logs/test_magnetostatic_shape_unit.log"
python3 -I "$SG/analyze.py" >"$SG/logs/analyze.log" 2>&1
grep "windows span" "$SG/logs/analyze.log"
