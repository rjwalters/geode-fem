"""Read-only interrupted-session inventory; never launches or resumes an optimizer."""
import argparse
import fcntl
import json
import math
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from session_journal import digest, freeze, read_events


def owner_active(root):
    with (root / "owner.lock").open() as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return True
        fcntl.flock(lock, fcntl.LOCK_UN)
        return False


def diagnostic_pending_snapshot(root, raw):
    """Verify a bound stopped result before allowing diagnostic-only disposition."""
    if raw.get("diagnostic_status") not in ("pending", "interrupted"):
        return None
    binding = raw.get("optimizer_snapshot") or {}
    name = binding.get("file")
    if not name or Path(name).name != name or not binding.get("sha256"):
        raise ValueError("unfinished diagnostic lacks a bound optimizer snapshot")
    path = root / name
    if digest(path) != binding["sha256"]:
        raise ValueError("optimizer snapshot changed")
    stopped = json.loads(path.read_text())
    if (stopped.get("optimizer_completed") is not True
            or stopped.get("stop_reason") == "interrupted"
            or stopped.get("diagnostic_status") != "pending"
            or not stopped.get("diagnostic_state")):
        raise ValueError("diagnostic repair has no completed optimizer")
    for key in ("method", "space", "start", "repeat", "unit_seconds", "budget_fe", "session",
                "optimizer_completed", "stop_reason", "ledger", "validations", "attainment",
                "checkpoint_60", "diagnostic_state", "anchor", "training_model"):
        if raw.get(key) != stopped.get(key):
            raise ValueError(f"stopped optimizer result changed: {key}")
    return binding


def inventory(root):
    root = Path(root).resolve()
    active = owner_active(root)
    journal = read_events(root / "events.jsonl")
    events = journal["events"]
    if not any(e["kind"] == "session_ready" for e in events):
        return {"source": str(root), "owner_active": active, "ready": False,
                "reason": "no durably accepted calibration and bound queue; cannot recover jobs",
                "journal_sha256": journal["sha256"],
                "incomplete_tail_bytes": journal["incomplete_tail_bytes"], "jobs": []}
    bindings = json.loads((root / "bindings.json").read_text())
    if any(e["kind"] == "session_ready" and e["bindings_sha256"] != digest(root / "bindings.json")
           for e in events):
        raise ValueError("bindings changed after session_ready")
    for name, expected in bindings["files"].items():
        if Path(name).name != name or digest(root / name) != expected:
            raise ValueError(f"missing or changed frozen input: {name}")
    session = json.loads((root / "session.json").read_text())
    unit = session["unit_seconds"]
    if type(unit) not in (int, float) or not math.isfinite(unit) or unit <= 0:
        raise ValueError("invalid frozen validation unit")
    for name in ("calibration_primary.json", "calibration_secondary.json"):
        if name not in bindings["files"]:
            if name == "calibration_primary.json":
                raise ValueError("missing frozen primary calibration")
            continue
        calibration = json.loads((root / name).read_text())
        if (calibration.get("accepted_for_dispatch") is not True
                or calibration["calibration"].get("unit_seconds") != unit):
            raise ValueError("rejected calibration or changed validation unit")
    queue = json.loads((root / "queue.json").read_text())["jobs"]
    if len({j["name"] for j in queue}) != len(queue):
        raise ValueError("duplicate logical job in frozen queue")
    attempts = {}
    for e in events:
        if e["kind"] == "attempt_created":
            attempts[e["sequence"]] = {"attempt_id": e["sequence"], "job": e["job"],
                                       "suffix": e["suffix"], "created_time": e["time"],
                                       "recovery_reason": e.get("recovery_reason")}
        elif e["kind"] in ("attempt_dispatched", "attempt_finished", "attempt_failed"):
            a = attempts[e["attempt_id"]]
            key = e["kind"].removeprefix("attempt_")
            if key in a:
                raise ValueError("duplicate attempt transition")
            a[key] = e
    if any(a["job"]["name"] not in {j["name"] for j in queue} for a in attempts.values()):
        raise ValueError("attempt is not in the frozen queue")
    rows = []
    for job in queue:
        prior = [a for a in attempts.values() if a["job"]["name"] == job["name"]]
        for a in prior:
            if a["job"] != job:
                raise ValueError("attempt changed its logical job settings")
        completed = []
        for a in prior:
            result = a.get("finished", {}).get("result")
            if result is None or result["status"] != "ran":
                # An interrupted launcher can leave a completed optimizer trace
                # without a terminal parent event. Retain it for forensic work;
                # it must never become a whole-optimizer replacement candidate.
                name = a.get("dispatched", {}).get("trace")
                if name and Path(name).name == name and (root / name).exists():
                    raw = json.loads((root / name).read_text())
                    if any(raw.get(k) != job[k] for k in ("method", "space", "start", "repeat")):
                        raise ValueError("unfinalized trace identity differs from its logical job")
                    a["unfinalized_trace"] = {"file": name, "sha256": digest(root / name),
                                              "optimizer_completed": raw.get("optimizer_completed"),
                                              "ledger": raw.get("ledger"),
                                              "diagnostic_status": raw.get("diagnostic_status")}
                continue
            trace_name = result.get("trace")
            if type(result.get("exit")) is not int:
                raise ValueError("finished run lacks a process exit code")
            if (trace_name and Path(trace_name).name == trace_name and not result.get("trace_sha256")
                    and not (root / trace_name).exists() and result.get("exit") != 0):
                a["outcome"] = {"optimizer_completed": False, "missing_trace": True,
                                "reason": "no trace published before process exit"}
                continue
            if not trace_name or Path(trace_name).name != trace_name or not result.get("trace_sha256"):
                raise ValueError("finished attempt lacks bound trace")
            trace = root / trace_name
            if digest(trace) != result["trace_sha256"]:
                raise ValueError("finished attempt trace changed")
            raw = json.loads(trace.read_text())
            if any(raw.get(k) != job[k] for k in ("method", "space", "start", "repeat")):
                raise ValueError("trace identity differs from its logical job")
            for key in ("unit_seconds", "budget_fe"):
                if raw.get(key) != session[key]:
                    raise ValueError(f"trace changed frozen {key}")
            if raw.get("session") != session.get("session_id") or raw.get("label") != session.get("label"):
                raise ValueError("trace belongs to a different frozen session")
            if type(raw.get("optimizer_completed")) is not bool:
                raise ValueError("trace lacks explicit optimizer completion state")
            if (raw["optimizer_completed"] is False) != (raw.get("stop_reason") == "interrupted"):
                raise ValueError("optimizer completion contradicts its recorded stop reason")
            if raw["optimizer_completed"] is False and result.get("exit") == 0:
                raise ValueError("normal exit contradicts incomplete optimizer")
            a["outcome"] = {key: raw.get(key) for key in (
                "optimizer_completed", "stop_reason", "diagnostic_status", "ledger", "interruption",
                "in_run_wall_seconds", "post_run_wall_seconds", "resources", "optimizer_snapshot")}
            pending_snapshot = diagnostic_pending_snapshot(root, raw)
            if result.get("host_contaminated") is False:
                if pending_snapshot is not None:
                    a["diagnostic_pending_snapshot"] = pending_snapshot
                    completed.append((a, result))
                elif raw["optimizer_completed"] is True and raw.get("stop_reason") != "interrupted":
                    completed.append((a, result))
        if completed:
            # First eligible completion, never lowest objective or fastest time.
            selected, result = completed[0]
            disposition, launch = ("completed_optimizer_diagnostic_pending"
                                   if selected.get("diagnostic_pending_snapshot") else
                                   "completed_preserve"), False
            eligible = {"attempt_id": selected["attempt_id"], "trace": result["trace"],
                        "trace_sha256": result["trace_sha256"]}
            if selected.get("diagnostic_pending_snapshot"):
                eligible["optimizer_snapshot"] = selected["diagnostic_pending_snapshot"]
        elif not any("dispatched" in a for a in prior):
            disposition, launch, eligible = "never_started_or_deferred", True, None
        elif any("dispatched" in a and ("finished" not in a
                                       or a.get("outcome", {}).get("optimizer_completed") is False)
                 for a in prior):
            disposition, launch, eligible = "interrupted_unresolved", False, None
        elif all(a.get("finished", {}).get("result", {}).get("host_contaminated") is True
                 for a in prior if "dispatched" in a):
            disposition, launch, eligible = "contaminated_unresolved", False, None
        else:
            disposition, launch, eligible = "terminal_failure_preserve", False, None
        rows.append({"job": job, "disposition": disposition, "eligible_completion": eligible,
                     "never_started_launch_candidate": launch, "attempts": prior})
    return {"source": str(root), "owner_active": active, "ready": True,
            "journal_sha256": journal["sha256"],
            "bindings_sha256": digest(root / "bindings.json"),
            "incomplete_tail_bytes": journal["incomplete_tail_bytes"], "jobs": rows,
            "can_launch": False,
            "reason": "inventory only; requires verified process cleanup, adopted retry-policy execution and recovery launcher"}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--source", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    result = inventory(a.source)
    freeze(a.out, result)
    print(json.dumps({"ready": result["ready"], "owner_active": result["owner_active"],
                      "jobs": len(result["jobs"]), "can_launch": False}))


if __name__ == "__main__":
    main()
