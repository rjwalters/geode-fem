"""Deterministic disposition of an original session and its recovery generations.

Consumes verified inventories and separately reviewed external-event attestations.
It does not authenticate a reviewer's judgment, observe processes, or launch work.
The linked launcher must verify the complete registry and fresh bindings first.
"""
from pathlib import Path

from session_journal import digest

REASONS = {"remaining_registered", "never_started_or_deferred", "external_interruption", "contamination"}


def attempt_key(source, attempt):
    return f"{source}#{attempt['attempt_id']}"


def external_attestation(record, key):
    """Check the supplied reviewed record and its files; never infer from SIGTERM.

    The author of the attestation is responsible for verifying the external cause.
    A signal receipt alone, or a boolean without review/evidence, is insufficient.
    """
    if (record.get("attempt") != key or record.get("verified_external_interruption") is not True
            or record.get("event_kind") not in ("operator_stop", "host_shutdown", "external_resource_loss")
            or any(not isinstance(record.get(k), str) or not record[k].strip()
                   for k in ("reason", "verified_by", "verification_reference", "event_id"))):
        raise ValueError("external interruption lacks a reviewed, attempt-bound attestation")
    evidence = record.get("evidence")
    if not isinstance(evidence, list) or not evidence:
        raise ValueError("external interruption has no bound evidence files")
    for item in evidence:
        if not item.get("path") or not item.get("sha256") or digest(Path(item["path"])) != item["sha256"]:
            raise ValueError("external interruption evidence changed or is missing")
    return record


def plan(inventories, *, external_events=None, diagnostic_attempts=()):
    if not inventories:
        raise ValueError("no original session inventory")
    external_events = external_events or {}
    jobs = [row["job"] for row in inventories[0]["jobs"]]
    if [job["position"] for job in jobs] != list(range(len(jobs))):
        raise ValueError("registered queue positions are not consecutive and ordered")
    sources, known = set(), set()
    by_job = {job["name"]: {"job": job, "attempts": [], "completed": []} for job in jobs}
    if len(by_job) != len(jobs):
        raise ValueError("duplicate logical job")
    for generation, inv in enumerate(inventories):
        if (not inv.get("ready") or inv.get("owner_active") is not False
                or inv.get("incomplete_tail_bytes") != 0 or inv["source"] in sources):
            raise ValueError("recovery chain contains active, incomplete or repeated session inventory")
        sources.add(inv["source"])
        if [row["job"] for row in inv["jobs"]] != jobs:
            raise ValueError("recovery generation changed the registered queue")
        for row in inv["jobs"]:
            merged = by_job[row["job"]["name"]]
            for attempt in row["attempts"]:
                key = attempt_key(inv["source"], attempt)
                if key in known or attempt["job"] != row["job"]:
                    raise ValueError("duplicate attempt or changed logical job")
                known.add(key)
                reason = attempt.get("recovery_reason")
                if reason is not None and reason not in REASONS:
                    raise ValueError("unknown recovery reason")
                if generation and "dispatched" in attempt and reason is None:
                    raise ValueError("recovered dispatch lacks its policy reason")
                merged["attempts"].append(dict(attempt, key=key, source=inv["source"], generation=generation))
            eligible = row.get("eligible_completion")
            if eligible:
                key = attempt_key(inv["source"], eligible)
                if not any(a["key"] == key for a in merged["attempts"]):
                    raise ValueError("completion has no known attempt")
                merged["completed"].append(dict(eligible, key=key, source=inv["source"],
                    diagnostic_pending=row["disposition"] == "completed_optimizer_diagnostic_pending"))
    for key, event in external_events.items():
        if key not in known:
            raise ValueError("external event refers to an unknown attempt")
        external_attestation(event, key)
    for repair in diagnostic_attempts:
        if repair.get("optimizer_attempt") not in known:
            raise ValueError("diagnostic repair refers to an unknown optimizer attempt")
        if repair.get("status") not in ("pending", "interrupted", "ok", "failed", "unavailable", "no incumbent"):
            raise ValueError("unknown diagnostic repair status")
    primary, tail, diagnostics, rows = [], [], [], []
    for job in jobs:
        merged = by_job[job["name"]]
        attempts = merged["attempts"]
        dispatched = [a for a in attempts if "dispatched" in a]
        external_used = sum(a.get("recovery_reason") == "external_interruption" for a in dispatched)
        contamination_used = sum(a.get("recovery_reason") == "contamination"
                                 or (a["generation"] == 0 and a.get("suffix") == "_rerun1")
                                 for a in dispatched)
        if external_used > 1 or contamination_used > 1:
            raise ValueError("recovery chain already exceeds a per-job replacement allowance")
        row = {"job": job, "attempts": attempts, "external_replacements_used": external_used,
               "contamination_reruns_used": contamination_used, "selected_completion": None}
        action = None
        if merged["completed"]:
            selected = merged["completed"][0]  # Never sort by objective, success or time to target.
            row.update(disposition="completed_preserve", selected_completion=selected)
            repairs = [r for r in diagnostic_attempts if r["optimizer_attempt"] == selected["key"]]
            terminal = next((r for r in repairs if r["status"] not in ("pending", "interrupted")), None)
            row["diagnostic_attempts"] = repairs
            row["selected_diagnostic"] = terminal
            if selected["diagnostic_pending"] and terminal is None:
                row["disposition"] = "diagnostic_only"
                diagnostics.append({"job": job, "reason": "diagnostic_only", "optimizer": selected,
                                    "previous_repairs": repairs})
        elif any(a.get("unfinalized_trace", {}).get("optimizer_completed") is True for a in attempts):
            row.update(disposition="unresolved", reason="completed optimizer lacks finalized parent/host evidence; no optimizer replacement")
        elif not dispatched:
            reason = "never_started_or_deferred" if attempts else "remaining_registered"
            row["disposition"] = reason
            action = {"job": job, "reason": reason}
            (tail if attempts else primary).append(action)
        else:
            latest = dispatched[-1]
            outcome = latest.get("outcome") or {}
            result = latest.get("finished", {}).get("result") or {}
            if outcome.get("optimizer_completed") is True:
                if result.get("host_contaminated") is True and contamination_used == 0:
                    row["disposition"] = "contamination_rerun"
                    action = {"job": job, "reason": "contamination", "replaces": latest["key"]}
                else:
                    row.update(disposition="unresolved", reason="completed attempt lacks clean selection or contamination allowance is spent")
            elif latest["key"] in external_events and external_used == 0:
                row["disposition"] = "external_replacement"
                action = {"job": job, "reason": "external_interruption", "replaces": latest["key"],
                          "external_event": external_events[latest["key"]]}
            else:
                row.update(disposition="unresolved", reason="interruption unverified or external replacement allowance is spent")
            if action:
                tail.append(action)
        rows.append(row)
    return {"can_launch": False, "remaining_registered": primary, "recovery_tail": tail,
            "diagnostics": diagnostics, "jobs": rows,
            "replacement_job_count": sum(bool(r["external_replacements_used"] or r["contamination_reruns_used"])
                                         for r in rows),
            "unresolved_job_count": sum(r["disposition"] == "unresolved" for r in rows),
            "phase_order": ["remaining_registered", "recovery_tail", "diagnostics"],
            "scope": "policy plan only; fresh bindings, registry ownership and process absence required at dispatch"}
