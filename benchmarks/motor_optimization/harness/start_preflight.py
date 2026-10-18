"""Recheck the frozen starts on moved L0/L1 meshes in both spaces, without FEM."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import sys
import subprocess
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from evaluator import Evaluator, to_bits
from main_schedule import STARTS
from make_starts import verdict
from run import thread_env
import interruption


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def check_reply(reply, x):
    if reply.get("x_bits") != to_bits(x):
        return "wrong or missing design bits"
    reason = verdict(reply)
    if reason is not None:
        return reason
    q = reply.get("quality") or {}
    angle = q.get("min_angle_deg")
    if (q.get("inverted") != 0 or not isinstance(angle, (int, float))
            or not math.isfinite(angle) or angle < 5.0):
        return "moved mesh did not demonstrate the registered quality floor"
    charges = reply.get("charges")
    if not isinstance(charges, list) or not charges:
        return "missing preflight cost record"
    if any(c.get("forward_solves") != 0 or c.get("adjoint_solves") != 0 for c in charges):
        return "preflight unexpectedly performed FEM solves"
    return None


def verify(binary, starts_path, meshes, out, evaluator_factory=Evaluator, stop=None):
    """Return durable evidence. Failures do not select replacement starts."""
    out = Path(out)
    starts_path = Path(starts_path)
    starts = json.loads(starts_path.read_text())
    if set(starts["starts"]) != set(STARTS) or set(meshes) != {0, 1}:
        raise ValueError("preflight requires exactly the frozen five starts and levels0/1")
    binary = Path(binary).resolve()
    record = {"issue": 1202, "label": "setup_preflight", "status": "running",
              "rule": "all frozen starts, spaces A/B, moved L0/L1 meshes; no remesh, no replacement",
              "starts_sha256": sha(starts_path), "binary_sha256": sha(binary),
              "mesh_sha256": {str(level): sha(path) for level, path in meshes.items()},
              "counted_in_run_budgets": False, "fem_solves": 0, "sessions": [],
              "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=HERE, text=True).strip(),
              "source_sha256": {name: sha(HERE / name) for name in
                                ("start_preflight.py", "make_starts.py", "evaluator.py", "run.py", "resource_usage.py")}}
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("x") as f:
        json.dump(record, f, indent=2)
    def save():
        temporary = out.with_suffix(out.suffix + ".tmp")
        temporary.write_text(json.dumps(record, indent=2, allow_nan=False) + "\n")
        temporary.replace(out)
    started = time.monotonic()
    try:
        for space in "ab":
            for level in (0, 1):
                entry = {"space": space, "level": level, "status": "running", "checks": []}
                record["sessions"].append(entry)
                ev = None
                t0 = time.monotonic()
                try:
                    interruption.check(stop)
                    mesh = str(Path(meshes[level]).resolve())
                    command = [str(binary), "--space", space, "--train-mesh", mesh,
                               "--val-mesh", mesh, "--train-level", str(level), "--val-level", str(level)]
                    entry["command"] = command
                    ev = evaluator_factory(command, env=thread_env(), timeout=120,
                                           stderr_path=out.with_suffix(".stderr.log"), stop=stop)
                    entry["ready"] = ev.ready
                    if ev.ready.get("train_msh_sha256") != record["mesh_sha256"][str(level)]:
                        raise ValueError("preflight evaluator loaded a different training mesh")
                    for field in ("free", "lower", "upper", "x0"):
                        if ev.ready["space"][field] != starts["spaces"][space][field]:
                            raise ValueError(f"space {space} {field} differs from frozen starts")
                    for name in STARTS:
                        x = starts["starts"][name][space]
                        result = {"start": name, "x_bits": to_bits(x), "status": "running"}
                        entry["checks"].append(result)
                        save()
                        reply = ev.request("check", timeout=120, x_bits=to_bits(x))
                        charges = reply.get("charges")
                        if (not isinstance(charges, list) or not charges or
                                any(type(c.get(k)) is not int or c[k] < 0 for c in charges
                                    for k in ("forward_solves", "adjoint_solves"))):
                            record["fem_solves"] = None
                        elif record["fem_solves"] is not None:
                            record["fem_solves"] += sum(c[k] for c in charges
                                for k in ("forward_solves", "adjoint_solves"))
                        reason = check_reply(reply, x)
                        result.update(reply=reply, reason=reason, status="PASS" if reason is None else "failed")
                        save()
                    entry["status"] = "PASS" if all(c["status"] == "PASS" for c in entry["checks"]) else "failed"
                except interruption.Interrupted as error:
                    entry.update(status="interrupted", error=str(error))
                    if getattr(error, "evaluator_resources", None):
                        entry["startup_failure_resources"] = error.evaluator_resources
                    raise
                except Exception as error:
                    entry.update(status="failed", error=f"{type(error).__name__}: {error}")
                    if getattr(error, "evaluator_resources", None):
                        entry["startup_failure_resources"] = error.evaluator_resources
                finally:
                    if ev is not None:
                        ev.close()
                        entry["resources"] = ev.resources()
                    entry["wall_seconds"] = time.monotonic() - t0
                    save()
        if sha(starts_path) != record["starts_sha256"]:
            raise ValueError("frozen starts changed during preflight")
        if any(sha(path) != record["mesh_sha256"][str(level)] for level, path in meshes.items()):
            raise ValueError("mesh changed during preflight")
        if any(sha(HERE / name) != digest for name, digest in record["source_sha256"].items()):
            raise ValueError("preflight source changed during execution")
        record["status"] = "PASS" if all(s["status"] == "PASS" for s in record["sessions"]) else "failed"
    except BaseException as error:
        record.update(status="interrupted_or_failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        record["wall_seconds"] = time.monotonic() - started
        save()
    return record


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", type=Path, required=True)
    ap.add_argument("--starts", type=Path, default=HERE / "starts.json")
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    meshroot = HERE.parents[2] / "benchmarks/motor_reference/geometry/meshes"
    result = verify(a.binary, a.starts, {k: meshroot / f"icem_initial_sector_L{k}.msh" for k in (0, 1)}, a.out)
    print(json.dumps({"status": result["status"], "checks": sum(len(e["checks"]) for e in result["sessions"]),
                      "fem_solves": result["fem_solves"], "wall_seconds": result["wall_seconds"]}))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
