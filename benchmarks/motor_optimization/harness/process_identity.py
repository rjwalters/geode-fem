"""Local PID/group observations without collecting command lines or environments.

A differing PID identity is ambiguous, never permission to signal its new owner.
The verifier is read-only and treats missing observations as unresolved.
"""
from functools import lru_cache
import os
from pathlib import Path
import platform
from subprocess import Popen as _Popen, PIPE
import sys


def output(argv):
    env = dict(os.environ, LC_ALL="C")
    proc = _Popen(argv, stdout=PIPE, stderr=PIPE, env=env)
    try:
        stdout, stderr = proc.communicate(timeout=10)
    except BaseException:
        proc.kill()
        proc.communicate()
        raise
    if proc.returncode:
        raise RuntimeError(f"process observation failed: {argv[0]} exited {proc.returncode}")
    return stdout.decode().strip()


@lru_cache(maxsize=1)
def host():
    if sys.platform.startswith("linux"):
        boot = Path("/proc/sys/kernel/random/boot_id").read_text().strip()
    elif sys.platform == "darwin":
        boot = output(["sysctl", "-n", "kern.boottime"])
    else:
        raise RuntimeError("process identity observation supports Linux and macOS only")
    return {"host": platform.node(), "platform": sys.platform, "boot": boot}


def table():
    raw = output(["ps", "-ax", "-o", "pid=", "-o", "ppid=", "-o", "pgid=",
                  "-o", "lstart=", "-o", "stat="])
    records = []
    for line in raw.splitlines():
        parts = line.split()
        if len(parts) != 9:
            raise ValueError("unexpected process observation format")
        records.append({"pid": int(parts[0]), "ppid": int(parts[1]), "pgid": int(parts[2]),
                        "start": " ".join(parts[3:8]), "state": parts[8]})
    if len({p["pid"] for p in records}) != len(records):
        raise ValueError("duplicate PID in process observation")
    return records


def identify(pid):
    record = next((r for r in table() if r["pid"] == pid), None)
    if record is None:
        raise ValueError(f"cannot establish identity for PID {pid}")
    return dict(record, host=host())


def same(recorded, current):
    # PPID may change after the original parent exits. It is evidence, not a
    # stable identity component. Do not infer safety from a differing start.
    return all(recorded.get(k) == current.get(k) for k in ("pid", "pgid", "start"))
