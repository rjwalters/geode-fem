"""Read and bind the full retained history used by the recovery launcher."""
import copy
import hashlib
import json
import os
from pathlib import Path

from recovery_inventory import inventory
from recovery_policy import external_attestation
from session_journal import digest, read_events, sync_directory


def copy_bound(source, target, expected):
    raw = Path(source).read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("bound input changed before copy")
    with Path(target).open("xb") as stream:
        stream.write(raw); stream.flush(); os.fsync(stream.fileno())
    sync_directory(Path(target).parent)


def child_path(root, name):
    path = (root / name).resolve()
    if root.resolve() not in path.parents:
        raise ValueError("recovery artifact escapes its generation")
    return path


def diagnostics(root):
    """Keep every diagnostic attempt, including unfinished parent records."""
    root = Path(root)
    events = read_events(root / "events.jsonl")["events"]
    attempts = {}
    for event in events:
        if event["kind"] == "diagnostic_attempt_created":
            attempts[event["sequence"]] = {"created": event}
        elif event["kind"] in ("diagnostic_attempt_dispatched", "diagnostic_attempt_finished", "diagnostic_attempt_failed"):
            attempt = attempts[event["attempt_id"]]
            step = event["kind"].removeprefix("diagnostic_attempt_")
            if step in attempt:
                raise ValueError("duplicate diagnostic attempt transition")
            attempt[step] = event
    rows = []
    for index, attempt in attempts.items():
        created = attempt["created"]
        action = created["action"]
        result = attempt.get("finished", {}).get("result") or {}
        row = {"optimizer_attempt": action["optimizer"]["key"], "attempt": f"{root}#diagnostic-{index}",
               "source": str(root), "status": "pending", "events": attempt}
        # A worker's immutable terminal record can survive a killed launcher.
        # It remains usable only if its own journal binds it and its frozen inputs.
        directory = child_path(root, created["directory"])
        terminal = directory / "terminal.json"
        if terminal.exists():
            if result.get("terminal") and result["terminal"]["sha256"] != digest(terminal):
                raise ValueError("diagnostic terminal result changed")
            worker_events = read_events(directory / "events.jsonl")
            finishes = [e for e in worker_events["events"] if e["kind"] == "diagnostic_finished"]
            if (worker_events["incomplete_tail_bytes"] or len(finishes) != 1
                    or finishes[0]["result"] != {"file": "terminal.json", "sha256": digest(terminal)}):
                raise ValueError("diagnostic worker has no complete terminal binding")
            raw = json.loads(terminal.read_text())
            inputs = json.loads((directory / "inputs.json").read_text())
            expected_snapshot = action["optimizer"]["optimizer_snapshot"]
            expected_session = json.loads((root / "session.json").read_text())
            if (raw.get("kind") != "diagnostic_only" or raw.get("optimizer_reexecuted") is not False
                    or raw.get("counted_in_run_budget") is not False
                    or raw.get("attempt_id") != row["attempt"]
                    or raw.get("inputs") != {"file": "inputs.json", "sha256": digest(directory / "inputs.json")}
                    or inputs.get("session") != expected_session
                    or inputs.get("snapshot", {}).get("sha256") != expected_snapshot["sha256"]
                    or inputs.get("job") != {key: action["job"][key] for key in ("method", "space", "start", "repeat")}):
                raise ValueError("diagnostic repair changed its stopped optimizer or session binding")
            row.update(status=raw["status"], terminal={"file": str(terminal), "sha256": digest(terminal)},
                       result=raw)
        elif result.get("terminal"):
            raise ValueError("bound diagnostic terminal result is missing")
        rows.append(row)
    return rows


def load(registry):
    generations = registry.generations()
    paths = [registry.source, *[Path(g["path"]) for g in generations]]
    original = json.loads((registry.source / "bindings.json").read_text())
    inventories, repairs, external = [], [], {}
    for path in paths:
        inv = inventory(path)
        if any(event["kind"] == "recovery_verification_failed"
               for event in read_events(path / "events.jsonl")["events"]):
            raise ValueError("recovery generation has a failed dispatch verification; requires disposition")
        bound = json.loads((path / "bindings.json").read_text())
        if any(bound.get(key) != value for key, value in original.items()):
            raise ValueError("recovery generation changed original frozen session bindings")
        inventories.append(inv)
        repairs.extend(diagnostics(path))
    for generation in generations:
        policy = json.loads((registry.root / generation["link"]["plan"]["file"]).read_text())
        for key, event in policy.get("external_events", {}).items():
            if key in external and external[key] != event:
                raise ValueError("external-event review changed across generations")
            external[key] = event
    return paths, inventories, repairs, external


def archive_events(registry, retained, supplied):
    """Retain reviewed event bodies and evidence bytes in the fixed registry."""
    out = copy.deepcopy(retained)
    folder = registry.root / "external-evidence"
    for key, event in supplied.items():
        external_attestation(event, key)
        record = copy.deepcopy(event)
        folder.mkdir(exist_ok=True)
        for item in record["evidence"]:
            target = folder / (item["sha256"] + ".bin")
            if target.exists():
                if digest(target) != item["sha256"]:
                    raise ValueError("archived external evidence changed")
            else:
                copy_bound(item["path"], target, item["sha256"])
            item["source_path"] = item.get("source_path", item["path"])
            item["path"] = str(target)
        if key in out and out[key] != record:
            raise ValueError("external-event review cannot be replaced")
        out[key] = record
    return out
