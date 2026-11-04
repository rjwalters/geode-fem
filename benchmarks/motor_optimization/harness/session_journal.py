"""Durable session events and immutable inputs; no optimizer-state resume."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import threading
import time

import process_identity


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def write_atomic(path, value):
    """Replace one owned snapshot; readers never observe partial JSON."""
    path = Path(path)
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("x") as stream:
        json.dump(value, stream, indent=1, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)
    sync_directory(path.parent)


def freeze(path, value):
    """Create an immutable artifact once, refusing existing evidence."""
    path = Path(path)
    with path.open("x") as stream:
        json.dump(value, stream, indent=1, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    sync_directory(path.parent)
    return {"file": path.name, "sha256": digest(path)}


def read_events(path):
    """A torn final append is reported, never interpreted as a completed event."""
    raw = Path(path).read_bytes()
    lines = raw.splitlines(keepends=True)
    events, tail = [], b""
    for index, line in enumerate(lines):
        if not line.endswith(b"\n"):
            if index != len(lines) - 1:
                raise ValueError("interior journal line has no terminator")
            tail = line
            break
        event = json.loads(line)
        if event.get("sequence") != len(events):
            raise ValueError("journal sequence is missing or duplicated")
        events.append(event)
    return {"events": events, "sha256": hashlib.sha256(raw).hexdigest(), "incomplete_tail_bytes": len(tail),
            "incomplete_tail_sha256": hashlib.sha256(tail).hexdigest() if tail else None}


class Journal:
    def __init__(self, root, metadata):
        owner_identity = process_identity.identify(os.getpid())
        self.root = Path(root)
        self.root.mkdir(parents=True, exist_ok=False)
        self.lock = threading.RLock()
        self.sequence = 0
        self.closed = False
        self._owner = (self.root / "owner.lock").open("x+")
        fcntl.flock(self._owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
        self._events = (self.root / "events.jsonl").open("x")
        self.emit("session_created", metadata=metadata, owner_pid=os.getpid(),
                  owner_identity=owner_identity, process_tracking_schema=1)
        sync_directory(self.root)
        sync_directory(self.root.parent)

    def emit(self, kind, **fields):
        with self.lock:
            if self.closed:
                raise ValueError("session journal is closed")
            record = {"sequence": self.sequence, "time": time.time(),
                      "monotonic_seconds": time.monotonic(), "kind": kind, **fields}
            self._events.write(json.dumps(record, allow_nan=False) + "\n")
            self._events.flush()
            os.fsync(self._events.fileno())
            self.sequence += 1
            return record["sequence"]

    def close(self):
        with self.lock:
            if not self.closed:
                self._events.close()
                fcntl.flock(self._owner, fcntl.LOCK_UN)
                self._owner.close()
                self.closed = True
