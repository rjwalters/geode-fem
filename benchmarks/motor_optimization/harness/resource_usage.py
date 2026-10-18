"""Observational process accounting for #1202.

Call inside one isolated optimizer/calibration process, never use the shared
session launcher's RUSAGE_CHILDREN to attribute concurrent jobs. CPU counters
are cumulative seconds; request/phase CPU is their difference. Children appear
only after they have terminated and been waited for. A caller must explicitly
establish completeness; killed, unreaped descendants can be missing.

RSS is an observation of a lifetime high-water statistic, not a phase delta or
simultaneous process-tree peak. Raw platform units are always retained.
"""
import math
import os
import sys
import time

try:
    import resource
except ImportError:  # Explicit unavailable observations on unsupported hosts.
    resource = None


def rss_bytes(raw, platform):
    """Normalize only platforms whose getrusage units we have established."""
    if type(raw) is not int or raw < 0:
        raise ValueError("RSS counter must be a nonnegative integer")
    if platform == "darwin":
        return raw, "bytes"
    if platform.startswith("linux"):
        return raw * 1024, "KiB"
    return None, "unknown"


def _read(who):
    if resource is None:
        return {"available": False, "error": "Python resource module unavailable"}
    try:
        r = resource.getrusage(who)
        user, system = float(r.ru_utime), float(r.ru_stime)
        if not all(math.isfinite(v) and v >= 0 for v in (user, system)):
            raise ValueError("invalid CPU counter")
        peak, unit = rss_bytes(r.ru_maxrss, sys.platform)
        return {"available": True, "user_seconds": user, "system_seconds": system,
                "maxrss_raw": r.ru_maxrss, "maxrss_raw_unit": unit,
                "maxrss_bytes": peak}
    except (OSError, ValueError) as e:
        return {"available": False, "error": f"{type(e).__name__}: {e}"}


def snapshot():
    """Record self and reaped-child counters without resetting either."""
    return {"pid": os.getpid(), "platform": sys.platform,
            "monotonic_seconds": time.monotonic(), "source": "getrusage",
            "self": _read(resource.RUSAGE_SELF if resource is not None else None),
            "reaped_children": _read(resource.RUSAGE_CHILDREN if resource is not None else None)}


def interval(before, after, *, label, children_complete=False, incomplete_reason=None):
    """Describe a phase; never add request CPU again to these inclusive totals.

    Errors produce unavailable metrics rather than changing optimizer behavior.
    A complete child total requires caller evidence that all owned descendants
    were reaped. Otherwise the observed sum is explicitly incomplete/unverified.
    RSS values remain separate self/child lifetime observations at the end.
    """
    out = {"label": label, "before": before, "after": after,
           "children_complete": children_complete is True,
           "incomplete_reason": (None if children_complete is True else
                                 incomplete_reason or "descendant accounting not established"),
           "cpu": {}, "rss": {}, "errors": []}
    if before["pid"] != after["pid"] or before["platform"] != after["platform"]:
        out["errors"].append("snapshots do not belong to the same process and platform")
    if after["monotonic_seconds"] < before["monotonic_seconds"]:
        out["errors"].append("snapshot time decreased")
    identity_ok = not out["errors"]
    for scope in ("self", "reaped_children"):
        a, b = before[scope], after[scope]
        cpu = None
        if identity_ok and a["available"] and b["available"]:
            user = b["user_seconds"] - a["user_seconds"]
            system = b["system_seconds"] - a["system_seconds"]
            if all(math.isfinite(v) and v >= 0 for v in (user, system)):
                cpu = {"user_seconds": user, "system_seconds": system,
                       "total_seconds": user + system}
            else:
                out["errors"].append(f"{scope}: CPU counter decreased or is nonfinite")
        else:
            out["errors"].append(f"{scope}: counter unavailable or identity invalid")
        out["cpu"][scope] = cpu
        out["rss"][scope] = {
            "available": b["available"] and b.get("maxrss_bytes") is not None,
            "bytes": b.get("maxrss_bytes"), "raw": b.get("maxrss_raw"),
            "raw_unit": b.get("maxrss_raw_unit"),
            "semantics": ("self lifetime high-water mark observed at phase end" if scope == "self"
                          else "OS statistic for reaped children at phase end; not a process-tree peak")}
    parts = list(out["cpu"].values())
    out["observed_cpu_seconds"] = (sum(p["total_seconds"] for p in parts)
                                   if all(p is not None for p in parts) else None)
    out["cpu_complete"] = children_complete is True and not out["errors"]
    out["aggregation"] = "self plus reaped children once; do not add nested request totals"
    return out


def reap_usage(pid, *, block=False, descendants_complete=False):
    """Reap one owned child and retain its wait4 receipt.

    The caller must be this PID's sole reaper: do not call Popen.poll/wait or
    communicate first. Retain the receipt after reaping, and synchronize the
    process handle's returncode before any later cleanup. This helper neither
    kills processes nor drains pipes; drain output before a blocking wait.
    Nonblocking calls return pending without inventing zero resource usage.

    CPU includes descendants the child itself waited for, according to the
    OS. It is an independent cross-check, not an extra summand for run CPU.
    A signal death conservatively leaves descendant accounting incomplete.
    """
    if type(pid) is not int or pid <= 0:
        raise ValueError("an explicit positive owned-child PID is required")
    out = dict(pid=pid, source="wait4", platform=sys.platform,
               state="unavailable", available=False, returncode=None,
               cpu_complete=False, error=None)
    if not hasattr(os, "wait4"):
        out["error"] = "wait4 unavailable on this platform"
        return out
    try:
        observed_pid, status, r = os.wait4(pid, 0 if block else os.WNOHANG)
    except OSError as e:
        out["error"] = f"{type(e).__name__}: {e}"
        return out
    if observed_pid == 0:
        out["state"] = "pending"
        return out
    if observed_pid != pid:
        out["error"] = "wait4 returned an unexpected PID"
        return out
    out.update(state="reaped", wait_status=status,
               returncode=os.waitstatus_to_exitcode(status))
    try:
        user, system = float(r.ru_utime), float(r.ru_stime)
        total = user + system
        if not all(math.isfinite(v) and v >= 0 for v in (user, system, total)):
            raise ValueError("invalid CPU counter")
        peak, unit = rss_bytes(r.ru_maxrss, sys.platform)
    except (ValueError, OverflowError) as e:
        out["error"] = f"{type(e).__name__}: {e}"
        return out
    out.update(available=True, cpu=dict(user_seconds=user, system_seconds=system,
                                      total_seconds=total),
               rss=dict(raw=r.ru_maxrss, raw_unit=unit, bytes=peak,
                        semantics="OS wait4 statistic for this child; not a summed process-tree peak"),
               cpu_complete=descendants_complete is True and out["returncode"] >= 0,
               aggregation="cross-check only; already included in reaped-child run CPU")
    return out
