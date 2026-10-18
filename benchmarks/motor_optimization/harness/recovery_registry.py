"""One durable, exclusively owned recovery chain at the original frozen path.

The registry records generations before their launch, seals prior journal hashes,
and never edits the original session. Missing reservations or torn history require
resolution; a different output directory cannot silently reset replacement limits.
This module does not dispatch processes or establish their absence.
"""
import fcntl
import json
from pathlib import Path
import time

from recovery_inventory import owner_active
from session_journal import digest, freeze, read_events, sync_directory


def session_evidence(path):
    path = Path(path)
    if owner_active(path):
        raise ValueError("session owner is still active")
    events = read_events(path / "events.jsonl")
    ready = [event for event in events["events"] if event["kind"] == "session_ready"]
    if events["incomplete_tail_bytes"] or len(ready) != 1:
        raise ValueError("session has no unique, complete readiness binding")
    bound = digest(path / "bindings.json")
    if ready[0]["bindings_sha256"] != bound:
        raise ValueError("session bindings changed")
    return {"journal_sha256": events["sha256"], "bindings_sha256": bound}


class Registry:
    def __init__(self, source):
        self.source = Path(source).resolve()
        self.lock = self.stream = None
        original = session_evidence(self.source)
        bindings = json.loads((self.source / "bindings.json").read_text())
        expected_root = self.source.with_name(self.source.name + ".recoveries")
        if (bindings.get("session_directory") != str(self.source)
                or bindings.get("recovery_registry") != str(expected_root)):
            raise ValueError("original session has no matching frozen registry location")
        self.root = expected_root
        if self.root.resolve() != self.root:
            raise ValueError("recovery registry location was redirected")
        origin = {"schema": 1, "source": str(self.source), **original}
        try:
            try:
                self.root.mkdir()
                created = True
                sync_directory(self.root.parent)
            except FileExistsError:
                created = False
            self.lock = (self.root / "owner.lock").open("x+" if created else "r+")
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if created:
                freeze(self.root / "origin.json", origin)
                self.stream = (self.root / "events.jsonl").open("x")
                self.sequence = 0
                self.emit("registry_created", origin_sha256=digest(self.root / "origin.json"))
                sync_directory(self.root)
            else:
                if json.loads((self.root / "origin.json").read_text()) != origin:
                    raise ValueError("original session changed since recovery registration")
                record = read_events(self.root / "events.jsonl")
                if record["incomplete_tail_bytes"] or not record["events"]:
                    raise ValueError("registry history is torn or empty")
                first = record["events"][0]
                if (first["kind"] != "registry_created"
                        or first["origin_sha256"] != digest(self.root / "origin.json")):
                    raise ValueError("registry origin binding changed")
                self.sequence = len(record["events"])
                self.stream = (self.root / "events.jsonl").open("a")
            self.generations()  # Refuse a missing, active or altered earlier generation.
        except BaseException:
            self.close()
            raise

    def emit(self, kind, **fields):
        import os
        event = {"sequence": self.sequence, "time": time.time(), "kind": kind, **fields}
        self.stream.write(json.dumps(event, allow_nan=False) + "\n")
        self.stream.flush()
        os.fsync(self.stream.fileno())
        self.sequence += 1
        return event

    def generations(self):
        record = read_events(self.root / "events.jsonl")
        if record["incomplete_tail_bytes"]:
            raise ValueError("registry history has a torn tail")
        entries, finished = [], {}
        for event in record["events"][1:]:
            if event["kind"] == "generation_reserved":
                index = len(entries) + 1
                if event["index"] != index or event["directory"] != f"generation-{index:04d}":
                    raise ValueError("registry generations are missing or reordered")
                entries.append(event)
            elif event["kind"] == "generation_finished":
                index = event["index"]
                if index not in range(1, len(entries) + 1) or index in finished:
                    raise ValueError("unknown or duplicate generation completion")
                finished[index] = event
            else:
                raise ValueError("unknown registry transition")
        expected_names = {event["directory"] for event in entries}
        if any(path.name not in expected_names for path in self.root.glob("generation-*")):
            raise ValueError("unregistered recovery generation exists")
        previous = self.source
        out = []
        for event in entries:
            path = self.root / event["directory"]
            if path.resolve() != path:
                raise ValueError("recovery generation location was redirected")
            previous_hash = digest(previous / "events.jsonl")
            if event["previous"] != str(previous) or event["previous_journal_sha256"] != previous_hash:
                raise ValueError("prior generation changed or recovery chain branched")
            plan = event["plan"]
            if plan["file"] != f"plan-{event['index']:04d}.json" or digest(self.root / plan["file"]) != plan["sha256"]:
                raise ValueError("reserved recovery plan changed")
            observed = session_evidence(path)
            binding = json.loads((path / "bindings.json").read_text())
            expected_link = {"registry": str(self.root), "original": str(self.source),
                             "index": event["index"], "previous": str(previous), "plan": plan}
            if binding.get("recovery") != expected_link:
                raise ValueError("generation does not bind its original session and reserved plan")
            if event["index"] in finished:
                end = finished[event["index"]]
                if any(end[key] != value for key, value in observed.items()):
                    raise ValueError("finished recovery generation changed")
            out.append({"index": event["index"], "path": str(path), "link": expected_link, **observed})
            previous = path
        return out

    def reserve(self, policy_plan):
        entries = self.generations()
        index = len(entries) + 1
        previous = Path(entries[-1]["path"]) if entries else self.source
        # Freeze before reservation; a crash between the two refuses reuse of
        # this plan filename rather than silently erasing a partial reservation.
        binding = freeze(self.root / f"plan-{index:04d}.json", policy_plan)
        name = f"generation-{index:04d}"
        self.emit("generation_reserved", index=index, directory=name, previous=str(previous),
                  previous_journal_sha256=digest(previous / "events.jsonl"), plan=binding)
        link = {"registry": str(self.root), "original": str(self.source), "index": index,
                "previous": str(previous), "plan": binding}
        return self.root / name, link

    def finish(self, index):
        entries = self.generations()
        if not entries or entries[-1]["index"] != index:
            raise ValueError("only the newest generation can finish")
        if any(e["kind"] == "generation_finished" and e["index"] == index
               for e in read_events(self.root / "events.jsonl")["events"]):
            raise ValueError("generation completion is already recorded")
        self.emit("generation_finished", index=index,
                  **{key: entries[-1][key] for key in ("journal_sha256", "bindings_sha256")})

    def close(self):
        if self.stream is not None:
            self.stream.close()
            self.stream = None
        if self.lock is not None:
            fcntl.flock(self.lock, fcntl.LOCK_UN)
            self.lock.close()
            self.lock = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
