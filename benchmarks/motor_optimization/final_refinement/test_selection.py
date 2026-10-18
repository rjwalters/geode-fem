"""Selection-contract tests; synthetic records, never paper evidence."""
from dataclasses import replace
import copy
import json
import math
from pathlib import Path
import random
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from selection import Design, Validation, select  # noqa: E402


def design(x=0.0, **settings):
    return Design.from_values({"x": x}, {"current": 9.6777, "level": 3, **settings})


def record(**changes):
    base = Validation("run-0", "a" * 64, "A", "a", 0, 1, 0, design(1.0),
                      10.0, 120.0, 0.1, 1.0025, eligible_run=True,
                      host_contaminated=False, completed=True,
                      geometry_admissible=True, discrete_pass=True, mesh_pass=True,
                      stop_reason="budget")
    return replace(base, **changes)


def selection(rows, **changes):
    return select(rows, **dict(f0=1.0, mean0=1.0, theta0=design(), **changes))


def cell(out, role="A/a/representative"):
    return next(c for c in out["cells"] if c["role"] == role)


class SelectionTests(unittest.TestCase):
    def test_every_eligibility_gate_rejects_an_apparently_better_candidate(self):
        cases = [
            ({"mean_full": 1.0024}, "mean_floor_not_met"),
            ({"geometry_admissible": False}, "geometry_not_admissible"),
            ({"discrete_pass": False}, "discrete_guard_not_passed"),
            ({"mesh_pass": False}, "mesh_policy_not_passed"),
            ({"completed": False}, "validation_not_completed"),
            ({"elapsed_seconds": math.nextafter(120.0, math.inf)}, "validation_not_within_budget"),
            ({"host_contaminated": True}, "host_contaminated_or_unknown"),
            ({"eligible_run": False}, "run_not_eligible"),
            ({"f_val": -1.0}, "invalid_objective"),
            ({"f_val": math.nan}, "invalid_objective"),
            ({"mean_full": math.inf}, "mean_floor_not_met"),
            ({"geometry_admissible": 1}, "geometry_not_admissible"),
        ]
        for changes, reason in cases:
            with self.subTest(changes=changes):
                bad = replace(record(run_id="bad", ordinal=1, f_val=0.001), **changes)
                out = selection([bad, record()])
                self.assertEqual(cell(out)["record_id"], "run-0:0")
                row = next(r for r in out["records"] if r["record"]["run_id"] == "bad")
                self.assertIn(reason, row["rejection_reasons"])
                self.assertEqual(row["decision"], "rejected")
                self.assertFalse(any(row["attains"].values()))

    def test_all_rejection_reasons_are_retained(self):
        out = selection([record(completed=False, mean_full=None, f_val=None, mesh_pass=False)])
        self.assertEqual(set(out["records"][0]["rejection_reasons"]),
                         {"validation_not_completed", "mean_floor_not_met", "invalid_objective",
                          "mesh_policy_not_passed"})

    def test_rejected_nonfinite_observations_have_valid_json_provenance(self):
        out = selection([record(f_val=math.nan, mean_full=math.inf,
                                elapsed_seconds=math.nan)])
        saved = json.loads(json.dumps(out, allow_nan=False))
        self.assertEqual(saved["records"][0]["record"]["f_val"], {"nonfinite_float": "nan"})

    def test_mean_and_budget_boundaries_are_inclusive_and_failed_runs_are_retained(self):
        out = selection([record(elapsed_seconds=120.0, stop_reason="failed")])
        self.assertEqual(cell(out)["status"], "selected")
        self.assertEqual(out["records"][0]["record"]["stop_reason"], "failed")

    def test_target_credit_includes_relative_and_absolute_error_margins(self):
        out = selection([record(f_val=0.48)])
        self.assertEqual(cell(out, "A/a/T50")["status"], "unavailable")
        out = select([record(f_val=1e-9)], f0=1e-6, mean0=1.0, theta0=design())
        self.assertEqual(cell(out, "A/a/T50")["status"], "selected")
        self.assertEqual(cell(out, "A/a/T90")["status"], "unavailable")
        self.assertEqual(cell(out, "A/a/T99")["status"], "unavailable")

    def test_repeat2_can_be_representative_but_never_substitutes_for_crossing(self):
        out = selection([record(repeat=2, f_val=0.001)])
        self.assertEqual(cell(out)["status"], "selected")
        for target in ("T50", "T90", "T99"):
            c = cell(out, f"A/a/{target}")
            self.assertEqual(c["reason"], "no_repeat1_crossing")

    def test_representative_uses_objective_crossing_uses_elapsed_time(self):
        best = record(run_id="best", start_index=1, f_val=0.001, elapsed_seconds=80.0)
        fast = record(run_id="fast", f_val=0.005, elapsed_seconds=20.0, design=design(2.0))
        out = selection([best, fast])
        self.assertEqual(cell(out)["record_id"], "best:0")
        self.assertEqual(cell(out, "A/a/T99")["record_id"], "fast:0")

    def test_exact_ties_use_start_then_repeat_then_ordinal_not_input_order(self):
        rows = [record(run_id="start1", start_index=1, ordinal=0),
                record(run_id="repeat2", repeat=2, ordinal=0),
                record(run_id="later", ordinal=2), record(run_id="winner", ordinal=1)]
        expected = selection(rows)
        for seed in range(5):
            shuffled = list(rows)
            random.Random(seed).shuffle(shuffled)
            self.assertEqual(selection(shuffled), expected)
        self.assertEqual(cell(expected)["record_id"], "winner:1")
        self.assertEqual(cell(expected, "A/a/T50")["record_id"], "winner:1")

    def test_complete_cell_inventory_and_deduplicated_roles(self):
        rows = [record(run_id=f"{m}{s}", method=m, space=s, f_val=0.001,
                       design=design(float(1 + i * 2 + j)))
                for i, m in enumerate("ABCDE") for j, s in enumerate(("a", "b"))]
        out = selection(rows)
        self.assertEqual(len(out["cells"]), 40)
        self.assertEqual(len({c["role"] for c in out["cells"]}), 40)
        self.assertTrue(all(c["status"] == "selected" for c in out["cells"]))
        self.assertEqual(len(out["jobs"]), 11)  # theta0 + ten designs, 4 roles each
        self.assertEqual(sum(len(j["roles"]) for j in out["jobs"]), 41)
        self.assertEqual(len(out["records"]), 10)

    def test_start_precedes_repeat_in_representative_tie(self):
        out = selection([record(run_id="start0repeat2", repeat=2),
                         record(run_id="start1repeat1", start_index=1)])
        self.assertEqual(cell(out)["record_id"], "start0repeat2:0")

    def test_empty_cohort_has_all_cells_and_theta0_only(self):
        out = selection([])
        self.assertEqual(len(out["cells"]), 40)
        self.assertEqual(len(out["jobs"]), 1)
        self.assertEqual(out["jobs"][0]["roles"], ["theta0"])
        self.assertTrue(all(c["status"] == "unavailable" for c in out["cells"]))

    def test_duplicates_across_methods_preserve_every_source(self):
        out = selection([record(), record(run_id="other", method="B")])
        job = next(j for j in out["jobs"] if "A/a/representative" in j["roles"])
        self.assertEqual(job["source_records"], ["run-0:0", "other:0"])
        self.assertIn("B/a/representative", job["roles"])

    def test_nearby_geometry_changed_settings_and_signed_zero_never_alias(self):
        designs = [design(1.0), design(math.nextafter(1.0, math.inf)),
                   design(1.0, level=2), design(1.0, current=9.6778),
                   design(0.0), design(-0.0)]
        self.assertEqual(len({d.key for d in designs}), len(designs))
        self.assertEqual(Design.from_values({"a": 1.0, "b": 2}, {"x": 3}).key,
                         Design.from_values({"b": 2, "a": 1.0}, {"x": 3}).key)
        with self.assertRaises(ValueError):
            design(math.nan)

    def test_duplicate_identity_and_ambiguous_clean_reruns_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "duplicate validation"):
            selection([record(), record()])
        with self.assertRaisesRegex(ValueError, "ambiguous eligible reruns"):
            selection([record(), record(run_id="rerun")])
        out = selection([record(host_contaminated=True), record(run_id="rerun")])
        self.assertEqual(cell(out)["record_id"], "rerun:0")

    def test_source_records_and_output_cannot_change_each_other(self):
        rows = [record(), record(run_id="runnerup", ordinal=1, f_val=0.2)]
        saved = copy.deepcopy(rows)
        out = selection(rows)
        out["records"][0]["record"]["f_val"] = -9
        out["jobs"][0]["refinement_status"] = "failed"
        self.assertEqual(rows, saved)
        self.assertEqual(cell(selection(rows))["record_id"], "run-0:0")
        self.assertEqual(selection(rows)["records"][1]["decision"], "eligible_not_selected")

    def test_malformed_metadata_is_not_silently_relabelled(self):
        for changes in ({"source_sha256": ""}, {"method": "Z"}, {"start_index": 5},
                        {"repeat": True}, {"budget_seconds": 0}, {"ordinal": -1}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                selection([record(**changes)])


if __name__ == "__main__":
    unittest.main()
