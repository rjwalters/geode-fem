"""Bounded L0-training/L2-validation wiring smoke; no optimizer performance data."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from evaluator import Evaluator, to_bits
from run import physical_post_run_diagnostic, thread_env


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--space", choices=("a", "b"), default="a")
    args = ap.parse_args()
    if args.out.exists():
        raise SystemExit("refusing to overwrite existing smoke evidence")
    root = HERE.parents[2]
    binary = args.binary.resolve()
    meshes = root / "benchmarks/motor_reference/geometry/meshes"
    train_mesh, val_mesh = [meshes / f"icem_initial_sector_L{k}.msh" for k in (0, 2)]
    anchor = root / "benchmarks/motor_optimization/anchor.json"
    anchor_record = json.loads(anchor.read_text())["anchor"]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    command = [str(binary), "--space", args.space, "--train-mesh", str(train_mesh),
               "--train-level", "0", "--val-mesh", str(val_mesh), "--val-level", "2",
               "--anchor", str(anchor), "--mesh-out", str(args.out.with_suffix(".meshes"))]
    # Exact source snapshot includes the harness and common evaluator modules.
    sources = list(HERE.glob("*.py")) + list((root / "crates/geode-core/tests/common").glob("icem*.rs"))
    sources += [root / "crates/geode-core/examples/icem_harness_eval.rs",
                root / "crates/geode-core/tests/common/motor_resources.rs"]
    result = {"label": "smoke", "issue": 1202, "space": args.space,
              "scope": "theta0 model separation and nonlinear diagnostics only; no optimizer speedup evidence",
              "command": command, "python": sys.version, "binary_sha256": sha(binary),
              "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "tree_clean": not subprocess.check_output(["git", "status", "--porcelain"], cwd=root),
              "input_sha256": {str(p.relative_to(root)): sha(p) for p in (train_mesh, val_mesh, anchor)},
              "source_sha256": {str(p.relative_to(root)): sha(p) for p in sources},
              "status": "running"}
    def save():
        args.out.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    save()
    started = time.monotonic()
    ev = None
    try:
        ev = Evaluator(command, env=thread_env(), timeout=120,
                       stderr_path=args.out.with_suffix(".stderr.log"))
        result["ready"] = ev.ready
        save()
        assert ev.ready["train_msh_sha256"] == sha(train_mesh)
        assert ev.ready["validation_msh_sha256"] == sha(val_mesh) == anchor_record["msh_sha256"]
        assert ev.ready["train_msh_sha256"] != ev.ready["validation_msh_sha256"]
        assert ev.ready["session"]["training"]["level"] == 0
        assert ev.ready["session"]["validation"]["level"] == 2
        for key in ("F0", "mean0_full_current"):
            assert abs(ev.ready["anchor"][key] - anchor_record[key]) <= abs(anchor_record[key]) * 1e-14
        x = ev.ready["space"]["x0"]
        result["train"] = ev.request("train", timeout=180, x_bits=to_bits(x), gradient=True,
                                     purpose="iterate")
        save()
        assert result["train"]["ok"] and result["train"]["status"] == "ok"
        assert result["train"]["dF"] is not None
        result["validate"] = ev.request("validate", timeout=180, x_bits=to_bits(x))
        save()
        assert result["validate"]["ok"] and result["validate"]["status"] == "ok"
        assert result["validate"]["validator_msh_sha256"] == anchor_record["msh_sha256"]
        assert result["validate"]["anchor_msh_sha256"] == anchor_record["msh_sha256"]
        ev.close()
        result["run_evaluator_resources"] = ev.resources()
        assert ev.receipt["cpu_complete"] and ev.receipt["returncode"] == 0
        selected = SimpleNamespace(training_model={"train_mesh_sha256": sha(train_mesh)},
                                   validations=[], training_incumbent={"x": x},
                                   anchor=SimpleNamespace(mesh_sha256=anchor_record["msh_sha256"]))
        def fresh():
            return Evaluator(command, env=thread_env(), timeout=120,
                             stderr_path=args.out.with_suffix(".stderr.log"))
        result["diagnostic"] = physical_post_run_diagnostic(selected, fresh, unit=18.)
        save()
        assert result["diagnostic"]["status"] == "ok", result["diagnostic"].get("error")
        assert result["diagnostic"]["reply"]["training"]["reference_msh_sha256"] == sha(train_mesh)
        assert result["diagnostic"]["reply"]["validation"]["msh_sha256"] == sha(val_mesh)
        assert all(sha(root / name) == value for name, value in result["source_sha256"].items())
        result["status"] = "PASS"
    except BaseException as error:
        result.update(status="failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        if ev is not None:
            ev.close()
            result["run_evaluator_resources"] = ev.resources()
        result["wall_seconds"] = time.monotonic() - started
        save()
    print(json.dumps({"status": result["status"], "label": "smoke", "space": args.space,
                      "wall_seconds": result["wall_seconds"]}))


if __name__ == "__main__":
    main()
