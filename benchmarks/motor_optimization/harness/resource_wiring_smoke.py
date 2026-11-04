"""Labelled L0 resource-wiring smoke, never main-study performance evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from evaluator import Evaluator, EvaluatorFailure, to_bits  # noqa: E402
import resource_usage  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        raise SystemExit("refusing to overwrite an existing smoke artifact")
    root = HERE.parents[2]
    mesh = root / "benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L0.msh"
    binary = args.binary.resolve()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    command = [str(binary), "--space", "a", "--train-mesh", str(mesh),
               "--val-mesh", str(mesh), "--train-level", "0", "--val-level", "0",
               "--degrees", "0:4", "--mesh-out", str(args.out.with_suffix(".meshes"))]
    env = dict(os.environ, GEODE_NUM_THREADS="1", RAYON_NUM_THREADS="1")
    before = resource_usage.snapshot()
    wall = time.monotonic()
    ev = Evaluator(command, env=env, timeout=60)
    try:
        x = to_bits(ev.ready["space"]["x0"])
        trained = ev.request("train", timeout=60, x_bits=x, gradient=False, purpose="iterate")
        failed = ev.request("validate", timeout=60, x_bits=x)  # deliberately no anchor
    finally:
        ev.close()
    assert trained["ok"] is True and failed["ok"] is False
    receipt = ev.receipt
    assert receipt["returncode"] == 0 and receipt["cpu_complete"] is True
    measured = ev.resources()
    request_cpu = sum(r["measurement"]["observed_cpu_seconds"] for r in measured["requests"])
    assert all(r["measurement"]["cpu_complete"] is True for r in measured["requests"])
    assert receipt["cpu"]["total_seconds"] + 1e-5 >= request_cpu
    phase = resource_usage.interval(before, resource_usage.snapshot(),
                                    label="resource_wiring_smoke", children_complete=True)
    normal_wall = time.monotonic() - wall
    # Exercise the compiled executable's startup-failure reply and cleanup.
    try:
        Evaluator([str(binary), "--unknown", "value"], env=env, timeout=10)
    except EvaluatorFailure as error:
        startup_failure = error.evaluator_resources
    else:
        raise AssertionError("invalid arguments unexpectedly started an evaluator")
    assert startup_failure["child_receipt"]["state"] == "reaped"
    assert startup_failure["child_receipt"]["returncode"] == 2
    paths = [HERE / name for name in ("evaluator.py", "resource_usage.py", "resource_wiring_smoke.py")]
    paths += [root / name for name in (
        "crates/geode-core/examples/icem_harness_eval.rs",
        "crates/geode-core/tests/common/icem_harness.rs",
        "crates/geode-core/tests/common/motor_resources.rs")]
    result = {
        "label": "smoke", "issue": 1202,
        "scope": "All-L0, four training angles, resource wiring only; no main-performance evidence.",
        "command": command, "python": sys.version, "platform": sys.platform,
        "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "tree_clean": not subprocess.check_output(["git", "status", "--porcelain"], cwd=root),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "mesh_sha256": hashlib.sha256(mesh.read_bytes()).hexdigest(),
        "source_sha256": {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
        "resources": measured, "phase": phase, "observed_wall_seconds": normal_wall,
        "request_cpu_sum_for_crosscheck": request_cpu,
        "startup_failure": startup_failure,
        "train_ok": trained["ok"], "expected_validation_failure": failed["error"],
    }
    args.out.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(json.dumps({"status": "PASS", "label": "smoke", "requests": len(measured["requests"]),
                      "normal_exit": receipt["returncode"], "startup_failure_exit": 2}))


if __name__ == "__main__":
    main()
