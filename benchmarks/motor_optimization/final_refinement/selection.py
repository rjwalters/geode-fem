"""Pure final-check selection under #1185 amendment 8 addendum (#1203).

No trace loading, solver calls or result-dependent replacement happens here.
The future harness adapter must verify raw provenance and provide these records.
"""
from dataclasses import asdict, dataclass
import hashlib
import json
import math
import struct

METHODS = tuple("ABCDE")
SPACES = ("a", "b")
TARGETS = (("T50", 0.5), ("T90", 0.1), ("T99", 0.01))


def _exact(value):
    """Typed encoding: preserve float bits, including signed zero."""
    if value is None:
        return ["none"]
    if type(value) is bool:
        return ["bool", value]
    if type(value) is int:
        return ["int", str(value)]
    if type(value) is float:
        if not math.isfinite(value):
            raise ValueError("nonfinite design/setting")
        return ["f64", struct.pack(">d", value).hex()]
    if type(value) is str:
        return ["str", value]
    if isinstance(value, (list, tuple)):
        return ["array", [_exact(x) for x in value]]
    if isinstance(value, dict) and all(type(k) is str for k in value):
        return ["map", [[k, _exact(value[k])] for k in sorted(value)]]
    raise ValueError(f"unsupported identity value: {type(value).__name__}")


@dataclass(frozen=True)
class Design:
    """Complete geometry and check settings, frozen before deduplication.

    Include all fixed/free physical parameters and source/material/current/
    numerical settings; exclude optimizer identity and training resolution when
    they do not change the final check. Completeness is the adapter's obligation.
    """
    canonical: str

    @classmethod
    def from_values(cls, parameters, settings):
        if not parameters or not settings:
            raise ValueError("geometry and check settings are required")
        return cls(json.dumps(_exact([parameters, settings]), separators=(",", ":")))

    @property
    def key(self):
        return hashlib.sha256(self.canonical.encode()).hexdigest()


@dataclass(frozen=True)
class Validation:
    run_id: str
    source_sha256: str
    method: str
    space: str
    start_index: int
    repeat: int
    ordinal: int
    design: Design
    elapsed_seconds: float
    budget_seconds: float
    f_val: float | None
    mean_full: float | None
    # Fail closed. The adapter must affirm each condition from raw evidence.
    eligible_run: bool = False
    host_contaminated: bool = True
    completed: bool = False
    geometry_admissible: bool = False
    discrete_pass: bool = False
    mesh_pass: bool = False
    stop_reason: str = "unknown"

    @property
    def record_id(self):
        return f"{self.run_id}:{self.ordinal}"

    @property
    def tie(self):
        return self.start_index, self.repeat, self.ordinal


def _finite(value):
    return type(value) in (int, float) and math.isfinite(value)


def _metadata(r):
    if not r.run_id or r.method not in METHODS or r.space not in SPACES:
        raise ValueError("invalid run/method/space identity")
    if (type(r.start_index) is not int or not 0 <= r.start_index < 5
            or type(r.repeat) is not int or r.repeat not in (1, 2)
            or type(r.ordinal) is not int or r.ordinal < 0):
        raise ValueError("invalid start/repeat/validation identity")
    if (len(r.source_sha256) != 64
            or any(c not in "0123456789abcdef" for c in r.source_sha256)):
        raise ValueError("raw trace SHA-256 required")
    if not _finite(r.budget_seconds) or r.budget_seconds <= 0:
        raise ValueError("positive finite run budget required")


def _reasons(r, mean_floor):
    reasons = []
    checks = (
        (r.eligible_run is True, "run_not_eligible"),
        (r.host_contaminated is False, "host_contaminated_or_unknown"),
        (r.completed is True, "validation_not_completed"),
        (_finite(r.elapsed_seconds) and 0 <= r.elapsed_seconds <= r.budget_seconds,
         "validation_not_within_budget"),
        (r.geometry_admissible is True, "geometry_not_admissible"),
        (r.discrete_pass is True, "discrete_guard_not_passed"),
        (r.mesh_pass is True, "mesh_policy_not_passed"),
        (_finite(r.f_val) and r.f_val >= 0, "invalid_objective"),
        (_finite(r.mean_full) and r.mean_full >= mean_floor, "mean_floor_not_met"),
    )
    for passed, reason in checks:
        if not passed:
            reasons.append(reason)
    return reasons


def select(records, *, f0, mean0, theta0):
    """Return all 10/30 cells, every input decision, and deduplicated jobs.

    f0/mean0 must come from the independently verified common anchor. Records
    must use that anchor and validator; the adapter must refuse mismatches.
    Failed runs retain earlier eligible validations. No refinement output is
    accepted by this API, so a failed check cannot cause candidate replacement.
    """
    if not all(_finite(v) and v > 0 for v in (f0, mean0)):
        raise ValueError("positive finite anchor values required")
    mean_floor = mean0 * 1.0025
    records = list(records)
    seen = set()
    eligible_slots = set()
    audit, eligible = {}, []
    for r in records:
        _metadata(r)
        if r.record_id in seen:
            raise ValueError(f"duplicate validation identity: {r.record_id}")
        seen.add(r.record_id)
        reasons = _reasons(r, mean_floor)
        if not reasons:
            slot = (r.method, r.space, *r.tie)
            if slot in eligible_slots:
                raise ValueError("ambiguous eligible reruns: resolve run eligibility first")
            eligible_slots.add(slot)
            eligible.append(r)
        upper = (r.f_val + max(0.1 * r.f_val, 2.5e-7)
                 if _finite(r.f_val) and r.f_val >= 0 else None)
        snapshot = asdict(r)
        # JSON has no nonfinite numbers. Keep rejected observations explicit
        # and retain the raw trace digest rather than emitting invalid JSON.
        for field in ("elapsed_seconds", "f_val", "mean_full"):
            value = snapshot[field]
            if type(value) is float and not math.isfinite(value):
                snapshot[field] = {"nonfinite_float": value.hex()}
        audit[r.record_id] = {
            "record": snapshot, "eligible": not reasons, "rejection_reasons": reasons,
            "f_upper": upper, "selected_for": [],
            "attains": {name: not reasons and upper <= frac * f0 for name, frac in TARGETS},
        }

    jobs = {}

    def job(design, role, record_id):
        key = design.key
        entry = jobs.setdefault(key, {"design_key": key, "canonical_design": design.canonical,
                                      "roles": [], "source_records": []})
        if entry["canonical_design"] != design.canonical:
            raise ValueError("design hash collision")
        entry["roles"].append(role)
        if record_id is not None and record_id not in entry["source_records"]:
            entry["source_records"].append(record_id)
        return key

    job(theta0, "theta0", None)
    cells = []
    for method in METHODS:
        for space in SPACES:
            group = [r for r in eligible if (r.method, r.space) == (method, space)]
            roles = [("representative", group, lambda r: (r.f_val, *r.tie))]
            for target, _ in TARGETS:
                pool = [r for r in group if r.repeat == 1 and audit[r.record_id]["attains"][target]]
                roles.append((target, pool, lambda r: (r.elapsed_seconds, *r.tie)))
            for name, pool, order in roles:
                role = f"{method}/{space}/{name}"
                cell = {"role": role, "method": method, "space": space, "kind": name,
                        "status": "unavailable", "record_id": None, "design_key": None}
                if pool:
                    chosen = min(pool, key=order)
                    cell.update(status="selected", record_id=chosen.record_id,
                                design_key=job(chosen.design, role, chosen.record_id))
                    audit[chosen.record_id]["selected_for"].append(role)
                else:
                    cell["reason"] = ("no_feasible_representative" if name == "representative"
                                      else "no_repeat1_crossing")
                cells.append(cell)
    for row in audit.values():
        row["decision"] = ("rejected" if not row["eligible"] else
                           "selected" if row["selected_for"] else "eligible_not_selected")
    return {"schema_version": 1, "f0": f0, "mean0": mean0, "mean_floor": mean_floor,
            "targets": {n: f * f0 for n, f in TARGETS}, "cells": cells,
            "records": [audit[k] for k in sorted(audit)],
            "jobs": [jobs[k] for k in sorted(jobs)],
            "scope": "Best-observed representatives and earliest repeat-1 crossings only; "
                     "not all crossings or typical-start performance. No refinement results yet."}
