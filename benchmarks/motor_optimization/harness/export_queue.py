"""Export the registered 100-job queue for inspection, without FEM or calibration.

This is intended dispatch order, not measured execution order or launch approval.
Dirty sources are recorded explicitly so development exports cannot be mistaken
for a clean main-study source binding. Re-export after the reviewed merge.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import main_schedule
import recovery_bindings
from session_journal import freeze


def canonical_sha256(value):
    """Hash UTF-8 JSON with sorted keys, compact separators and no NaN."""
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode("utf-8")).hexdigest()


def export(out):
    before = recovery_bindings.sources()
    from driver import METHOD_SPECS
    # Take a value copy, retaining all fields and the exact resolution choices.
    specs = json.loads(json.dumps(METHOD_SPECS, allow_nan=False))
    jobs = main_schedule.registered_jobs(specs)
    after = recovery_bindings.sources()
    if before != after:
        raise ValueError("experiment sources changed during queue export")
    record = {"schema": 1, "label": "main_queue_plan", "jobs": jobs,
              "method_specs": specs, "source": before,
              "jobs_sha256": canonical_sha256(jobs),
              "method_specs_sha256": canonical_sha256(specs),
              "digest_format": "sha256 of UTF-8 JSON; sorted keys; compact separators; no NaN",
              "scope": "planned dispatch order only; no FEM, calibration or main launch"}
    receipt = freeze(out, record)
    return {**receipt, "jobs": len(jobs), "source_commit": before["commit"],
            "source_tree_clean": before["tree_clean"], "jobs_sha256": record["jobs_sha256"]}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True,
                        help="new JSON file in an existing directory; never overwritten")
    args = parser.parse_args(argv)
    print(json.dumps(export(args.out), indent=2))


if __name__ == "__main__":
    main()
