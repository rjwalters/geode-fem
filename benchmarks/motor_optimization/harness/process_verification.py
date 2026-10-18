"""Read-only, time-bound verification that a session's tracked processes stopped.

Never signals a PID, never launches recovery, and never treats PID reuse or an
unpublished spawn intent as proof of absence. The recovery launcher must perform
this check again immediately before dispatch, after verifying frozen bindings.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import process_identity
from recovery_inventory import owner_active
from session_journal import freeze, read_events


def verify(root, *, observe=process_identity.table, host=process_identity.host):
    root = Path(root).resolve()
    issues, observed, journals = [], [], {}
    try:
        current_host, processes = host(), observe()
    except Exception as error:
        return {"source": str(root), "observed_time": time.time(), "processes_stopped": False,
                "can_launch": False, "issues": [f"process observation unavailable: {error}"]}
    by_pid = {p["pid"]: p for p in processes}
    if len(by_pid) != len(processes):
        raise ValueError("duplicate observed PID")

    def check(identity, *, group, label):
        if not identity or identity.get("host") != current_host:
            issues.append(f"{label}: missing identity or changed host/boot")
            return
        if (type(identity.get("pid")) is not int or identity["pid"] <= 0
                or type(identity.get("pgid")) is not int or identity["pgid"] <= 0
                or not isinstance(identity.get("start"), str) or not identity["start"]):
            issues.append(f"{label}: malformed process identity")
            return
        current = by_pid.get(identity.get("pid"))
        if current is None:
            state = "absent"
        elif not process_identity.same(identity, current):
            state = "ambiguous_pid_identity"
            issues.append(f"{label}: PID identity differs; never act on a reused PID")
        elif current["state"].startswith("Z"):
            state = "terminated_zombie"
        else:
            state = "live"
            issues.append(f"{label}: tracked process is still live")
        members = [p for p in processes if p["pgid"] == identity.get("pgid")
                   and not p["state"].startswith("Z")] if group else []
        if members:
            issues.append(f"{label}: tracked process group still has live members")
        observed.append({"label": label, "recorded": identity, "current": current,
                         "state": state, "live_group_members": members})

    def visit(directory):
        directory = directory.resolve()
        if directory != root and root not in directory.parents:
            issues.append("child journal escapes the source session")
            return
        key = str(directory.relative_to(root))
        if key in journals:
            issues.append(f"{key}: repeated/cyclic child journal")
            return
        try:
            active = owner_active(directory)
            record = read_events(directory / "events.jsonl")
        except (OSError, ValueError) as error:
            issues.append(f"{key}: journal unavailable or malformed: {error}")
            return
        journals[key] = {"sha256": record["sha256"], "owner_active": active,
                         "incomplete_tail_bytes": record["incomplete_tail_bytes"]}
        if active:
            issues.append(f"{key}: journal owner lock is still held")
        if record["incomplete_tail_bytes"]:
            issues.append(f"{key}: torn journal tail leaves process state unresolved")
        events = record["events"]
        if (not events or events[0].get("kind") != "session_created"
                or events[0].get("process_tracking_schema") != 1):
            issues.append(f"{key}: no supported process-tracking schema")
            return
        check(events[0].get("owner_identity"), group=False, label=f"{key}/owner")
        spawns = {}
        for event in events:
            kind = event["kind"]
            if kind == "process_spawn_intent":
                spawns[event["sequence"]] = {"intent": event}
            elif kind in ("process_spawn_started", "process_spawn_failed", "process_spawn_reaped"):
                spawn = spawns.get(event.get("spawn_id"))
                step = kind.removeprefix("process_spawn_")
                if spawn is None or step in spawn:
                    issues.append(f"{key}: missing or duplicate spawn transition")
                    continue
                if step == "reaped" and "started" not in spawn:
                    issues.append(f"{key}: exit receipt precedes process identity")
                spawn[step] = event
        tracked_pids = {s.get("started", {}).get("identity", {}).get("pid") for s in spawns.values()}
        if any(e["kind"] == "owned_process_started" and e.get("pid") not in tracked_pids for e in events):
            issues.append(f"{key}: owned run has no matching tracked spawn")
        for index, spawn in spawns.items():
            label = f"{key}/spawn{index}"
            if "failed" in spawn:
                if not isinstance(spawn["failed"].get("error"), str) or not spawn["failed"]["error"]:
                    issues.append(f"{label}: spawn failure has no retained error evidence")
                if "started" in spawn or "reaped" in spawn:
                    issues.append(f"{label}: contradictory spawn failure")
                continue
            if "started" not in spawn:
                issues.append(f"{label}: spawn intent has no published process identity")
                continue
            identity = spawn["started"].get("identity")
            if not identity or identity.get("pid") != identity.get("pgid"):
                issues.append(f"{label}: spawned child did not establish its own process group")
                continue
            check(identity, group=True, label=label)
            receipt = spawn.get("reaped", {}).get("receipt")
            if receipt is not None and (receipt.get("pid") != identity["pid"]
                                        or receipt.get("state") != "reaped"
                                        or type(receipt.get("returncode")) is not int):
                issues.append(f"{label}: inconsistent exit receipt")
            intent = spawn["intent"]
            child = intent.get("child_journal")
            if intent.get("role") not in ("run", "evaluator", "direct"):
                issues.append(f"{label}: unknown spawned-process role")
            if intent.get("role") == "run":
                if not child or Path(child).is_absolute():
                    issues.append(f"{label}: run lacks a relative evaluator-journal binding")
                else:
                    visit(directory / child)
            elif child is not None:
                issues.append(f"{label}: unexpected child journal for non-run process")

    visit(root)
    return {"source": str(root), "observed_time": time.time(), "host": current_host,
            "processes_stopped": not issues, "can_launch": False, "issues": issues,
            "journals": journals, "observations": observed,
            "process_table_sha256": hashlib.sha256(json.dumps(processes, sort_keys=True).encode()).hexdigest(),
            "scope": "time-bound process absence only; no source/calibration/retry eligibility verdict"}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--source", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    result = verify(args.source)
    freeze(args.out, result)
    print(json.dumps({"processes_stopped": result["processes_stopped"],
                      "issues": result["issues"], "can_launch": False}))
    return 0 if result["processes_stopped"] else 1


if __name__ == "__main__":
    sys.exit(main())
