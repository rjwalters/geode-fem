"""Pure all-mode final-check schedule; no solver execution or target credit.

Consumes the selector's manifest, not refinement results. The runner must still
verify the raw provenance, full physical settings, actual mesh/solver options,
and conforming-interface eligibility. See #1203 and #1185 comment 6248492318.
"""
import copy
import hashlib
import json
from selection import Design


def _digest(value):
    return hashlib.sha256(json.dumps(
        value, sort_keys=True, separators=(",", ":"), allow_nan=False,
    ).encode()).hexdigest()


def _spec(name, level, angles, *, tolerance=1e-8, currents=(0.0, 0.5, 1.0)):
    return {
        "name": name,
        "level": level,
        "harmonic_max": 93,
        "angles_deg": list(angles),
        "current_fractions": list(currents),
        "newton_rel_tol": tolerance,
        "current_sweep_initialization": "cold",
        "successive_angle_initialization": "previous_angle",
        "requires_complete_sweep": True,
        "retain_raw_waveforms": True,
    }


def make_plan(selection):
    """Expand one frozen cohort into mandatory all-mode checks and references.

    Primary and withheld checks are included so comparisons have explicit
    dependencies. The runner may reuse a record only after proving exact
    geometry/settings/source equivalence; a g40 slice is not a shifted solve.
    This plan does not assert that settings in the selector are complete or
    that any check has run. Conforming checks need a separate fresh-mesh gate.
    """
    if selection.get("schema_version") != 1:
        raise ValueError("unsupported selection schema")
    cohort = selection.get("jobs", [])
    if not cohort:
        raise ValueError("selection cohort is empty")
    keys, anchors = set(), []
    for entry in cohort:
        canonical = entry["canonical_design"]
        key = entry["design_key"]
        if hashlib.sha256(canonical.encode()).hexdigest() != key:
            raise ValueError("design identity does not match canonical geometry/settings")
        if key in keys:
            raise ValueError("duplicate design in deduplicated cohort")
        keys.add(key)
        if not entry["roles"]:
            raise ValueError("cohort entry has no selection role")
        if "theta0" in entry["roles"]:
            anchors.append(key)
    if len(anchors) != 1:
        raise ValueError("exactly one theta0 reference is required")
    anchor = anchors[0]
    entries = {entry["design_key"]: entry for entry in cohort}
    initial_parameters = Design(entries[anchor]["canonical_design"]).identity_parts()[0]
    references = set()
    for entry in cohort:
        reference_key = entry.get("theta0_design_key")
        reference = entries.get(reference_key)
        if reference is None or not set(reference['roles']) & {'theta0', 'theta0_reference'}:
            raise ValueError("missing matching-settings theta0 reference")
        design = Design(entry['canonical_design'])
        initial = Design(reference['canonical_design'])
        if (initial.identity_parts()[0] != initial_parameters
                or initial.identity_parts()[1] != design.identity_parts()[1]
                or reference.get('theta0_design_key') != reference_key):
            raise ValueError("theta0 reference changes geometry or comparison settings")
        references.add(reference_key)
    g20 = [float(i) for i in range(20)]
    specs = [
        _spec("primary", 2, g20),
        _spec("L3", 3, g20),
        _spec("g40", 2, [i * 0.5 for i in range(40)]),
        _spec("tight", 2, g20, tolerance=1e-12),
        _spec("shifted", 2, [i + 0.5 for i in range(20)]),
        _spec("withheld", 2, g20, currents=(0.25, 0.75)),
    ]
    # Build the reference first, then deterministic design/spec ordering.
    jobs = []
    for key in sorted(keys, key=lambda k: (k not in references, k != anchor, k)):
        for spec in specs:
            name = spec["name"]
            actual_spec = copy.deepcopy(spec)
            actual_spec['base_settings_sha256'] = Design(entries[key]['canonical_design']).settings_key
            jobs.append({
                "job_id": f"{key}/{name}",
                "design_key": key,
                "spec": actual_spec,
                "theta0_job_id": f"{entries[key]['theta0_design_key']}/{name}",
                "primary_job_id": f"{key}/primary" if name != "withheld" else None,
                "status": "not_run",
                "cost_scope": "post_run",
            })
    return {
        "schema_version": 1,
        "selection_manifest_sha256": _digest(selection),
        "cohort": copy.deepcopy(cohort),
        "specs": specs,
        "jobs": jobs,
        "reference_design_keys": sorted(references),
        "target_credit_policy": "preserve_original_common_validation",
        "replacement_policy": "never_replace_selected_design",
        "conforming_checks": "pending_separate_fresh_mesh_and_supported_rotation_gate",
        "scope": "All-mode schedule only; no execution, outcomes, or conforming verdict.",
    }
