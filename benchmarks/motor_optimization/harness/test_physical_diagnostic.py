"""Diagnostic selection, failure retention, waveform and budget isolation."""
import copy
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import physical_diagnostic as physical
from evaluator import Aborted
from run import physical_post_run_diagnostic, run_one
from test_harness import (FakeClock, FakeEvaluator, WEIGHTS, X0, anchor_json,
                          make_run, near_centre, theta0_train)


class DiagnosticTests(unittest.TestCase):
    def test_best_common_feasible_and_earliest_tie(self):
        base = {"status": "ok", "credited": True, "admissible": True, "mean_ok": True}
        vs = [dict(base, index=i, x=[i], F_val=f) for i, f in enumerate((4., 2., 2., 1., .5))]
        vs[3]["mean_ok"] = False
        vs[4]["credited"] = False
        r = SimpleNamespace(validations=vs, training_incumbent={"x": [99]})
        picked = physical.select(r)
        self.assertEqual(picked["x"], [1])
        self.assertEqual(picked["validation"], 1)
        self.assertTrue(picked["common_feasibility_established"])

    def test_fallback_and_unavailable_do_not_claim_feasibility(self):
        r = SimpleNamespace(validations=[], training_incumbent=None)
        self.assertIsNone(physical.select(r))
        r.training_incumbent = {"x": [1.]}
        self.assertEqual(physical.select(r)["source"], "last_buildable_training_incumbent")
        self.assertFalse(physical.select(r)["common_feasibility_established"])

    def test_trial_does_not_replace_reported_incumbent(self):
        r, _, _, _ = make_run(method="slsqp_l0")
        start = r.problem.u_of(X0)
        r.train(start, False)
        r.train(near_centre(r), False)
        self.assertEqual(physical.select(r)["x"], X0)
        # A reported incumbent is retained before a possibly interrupted validation.
        r.validate = lambda *a, **k: (_ for _ in ()).throw(Aborted("fixture"))
        with self.assertRaises(Aborted):
            r.accept_iterate(near_centre(r), "fixture")
        self.assertEqual(physical.select(r)["x"], r.problem.x_of(near_centre(r)).tolist())

    def test_paired_records_survive_a_failed_acceptance(self):
        r, _, _, _ = make_run(method="cobyqa_l0", tamper={"val_mean_scale": .5})
        u = near_centre(r)
        t = r.train(u, False)
        v = r.validate(u)
        self.assertFalse(v["mean_ok"])
        self.assertIsNotNone(v["paired_training"])
        self.assertEqual(v["paired_training"]["stds"], t["stds"])
        self.assertEqual(v["paired_training"]["means"], t["means"])
        self.assertEqual(v["paired_training"]["F"], t["F"])
        self.assertEqual(len(v["means"]), 3)

    def test_mesh_infeasible_validation_retains_training_pair(self):
        r, ev, _, _ = make_run(method="cobyqa_l0")
        u = near_centre(r)
        t = r.train(u, False)
        request = ev.request
        def failed(op, **kw):
            reply = request(op, **kw)
            if op == "validate":
                reply["status"] = "mesh_infeasible"
            return reply
        ev.request = failed
        v = r.validate(u)
        self.assertEqual(v["status"], "mesh_infeasible")
        self.assertEqual(v["paired_training"]["F"], t["F"])
        self.assertFalse(any(v["attained"].values()))

    def test_interrupted_validation_retains_pair_without_credit(self):
        from ledger import BudgetExhausted
        r, ev, _, _ = make_run(method="cobyqa_l0")
        u = near_centre(r)
        trained = r.train(u, False)
        request = ev.request
        def interrupted(op, **kw):
            if op == "validate":
                raise Aborted("fixture interruption")
            return request(op, **kw)
        ev.request = interrupted
        with self.assertRaises(BudgetExhausted):
            r.validate(u)
        self.assertEqual(r.validations, [])
        self.assertEqual(r.attained, {})
        event = r.events[-1]
        self.assertEqual(event["kind"], "validation_unavailable")
        self.assertEqual(event["paired_training"]["F"], trained["F"])

    def test_waveforms_reconstruct_and_corrupt_statistics_fail(self):
        ev = FakeEvaluator(FakeClock())
        from evaluator import to_bits
        reply = ev.request("diagnose", x_bits=to_bits(X0), fractions=[.25, .75])
        self.assertEqual(len(physical.rows(reply)), 2)
        for field in ("means", "stds"):
            bad = copy.deepcopy(reply)
            bad["training"][field][0] += 1.
            with self.assertRaises(ValueError):
                physical.rows(bad)
        for change in (lambda d: d["validation"]["torques"][0].pop(),
                       lambda d: d["training"].update(reference_msh_sha256=""),
                       lambda d: d["validation"].update(angles_deg=list(range(1, 21)))):
            bad = copy.deepcopy(reply)
            change(bad)
            with self.assertRaises(ValueError):
                physical.rows(bad)

    def test_failure_retains_raw_reply_cost_and_closes_process(self):
        r, _, _, clock = make_run(method="cobyqa_l0")
        r.train(r.problem.u_of(X0), False)
        ev = FakeEvaluator(clock)
        request = ev.request
        def malformed(op, **kw):
            reply = request(op, **kw)
            reply["training"]["stds"][0] += 1
            return reply
        ev.request = malformed
        before = copy.deepcopy(r.result())
        out = physical_post_run_diagnostic(r, lambda: ev, 1., clock)
        self.assertEqual(out["status"], "failed")
        self.assertIn("reply", out)
        self.assertGreater(out["seconds"], 0)
        self.assertTrue(ev.closed)
        after = r.result()
        for key in ("events", "validations", "attainment"):
            self.assertEqual(before[key], after[key])
        self.assertEqual(before["ledger"]["entries"], after["ledger"]["entries"])

    def test_unavailable_and_startup_failure_remain_explicit(self):
        r, _, _, clock = make_run(method="slsqp_l0")
        def unavailable_factory():
            self.fail("must not launch without an incumbent")
        out = physical_post_run_diagnostic(r, unavailable_factory, 1., clock)
        self.assertEqual(out["status"], "unavailable")
        r.train(r.problem.u_of(X0), False)
        def failed_factory():
            error = ValueError("malformed startup fixture")
            error.evaluator_resources = {"child_receipt": {"cpu_complete": False}}
            raise error
        out = physical_post_run_diagnostic(r, failed_factory, 1., clock)
        self.assertEqual(out["status"], "failed")
        self.assertIn("malformed startup", out["error"])
        self.assertFalse(out["resources"]["evaluator"]["child_receipt"]["cpu_complete"])

    def test_diagnostics_do_not_change_stopped_run_including_budget_abort(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            (root / "anchor.json").write_text(json.dumps({"anchor": anchor_json()}))
            (root / "starts.json").write_text(json.dumps({"starts": {"theta0": {"a": X0}}}))
            train = theta0_train(FakeEvaluator(FakeClock()))
            base = {"label": "test", "session_id": "fixture", "unit_seconds": 1.,
                    "models": {"L0": {"train_level": 0, "train_mesh": "fixture.msh",
                                       "weights": WEIGHTS, "theta0_train": train}},
                    "primary_model": "L0", "starts": str(root / "starts.json"),
                    "evaluator": {"anchor": str(root / "anchor.json")}}
            for method in ("cobyqa_l0", "slsqp_l0"):
                for budget in (400., 9.5):
                    outputs = []
                    for enabled in (False, True):
                        clock = FakeClock()
                        outputs.append(run_one(dict(base, budget_fe=budget), method, "a", "theta0",
                            1, root / f"{method}-{budget}-{enabled}.json", clock=clock,
                            diagnostic=enabled,
                            evaluator_factory=lambda sp, led, c=clock: FakeEvaluator(c)))
                    for key in ("ledger", "attainment", "validations", "events", "stop_reason",
                                "best_validated_feasible", "checkpoint_60"):
                        self.assertEqual(outputs[0][key], outputs[1][key], (method, budget, key))
                    self.assertEqual(outputs[1]["post_run_diagnostic"]["status"], "ok")


if __name__ == "__main__":
    unittest.main(verbosity=2)
