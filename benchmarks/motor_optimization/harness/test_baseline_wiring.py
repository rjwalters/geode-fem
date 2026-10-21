"""Amendment-8 model and common-unit wiring; no performance claims."""
import copy
import json
import tempfile
from unittest import mock
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import driver
import main_schedule
import run
import session
from test_harness import make_run


def fixture():
    return {"primary_model": "L1", "unit_seconds": 13.0,
            "models": {f"L{k}": {"train_mesh": f"L{k}.msh", "train_level": k,
                        "theta0_train": {"F": 3.0 + k, "mean_full": 8.0 + k},
                        "weights": {"train_forward_eval": (k + 1) / 13.0}}
                       for k in (0, 1)},
            "evaluator": {"binary": "eval", "val_mesh": "L2.msh", "val_level": 2,
                          "anchor": "registered.json"}}


def calibration(vals, forward, adjoint, f):
    return {"ok": True, "validation_seconds": vals,
            "train_forward_seconds": [forward] * 3,
            "train_adjoint_seconds": [adjoint] * 3,
            "weights": {"validation_solves_per_eval": 60, "train_solves_per_eval": 60},
            "theta0_train": {"F": f, "mean_full": f + 1}, "theta0_validation": {},
            "anchor_agrees_bitwise": True, "anchor_agrees_4ulp": True,
            "deterministic": True}


class BaselineWiringTests(unittest.TestCase):
    def test_training_selection_keeps_one_validator_and_anchor(self):
        s = fixture()
        for method, level in (("slsqp", 1), ("cobyqa", 1), ("surrogate", 1),
                              ("cobyqa_l0", 0), ("slsqp_l0", 0)):
            with self.subTest(method=method):
                m = run.model_of(s, method)
                self.assertIs(m, s["models"][f"L{level}"])
                argv = run.evaluator_argv(s, "a", Path("mesh-out"), method)
                opts = dict(zip(argv[1::2], argv[2::2]))
                self.assertEqual(opts["--train-mesh"], f"L{level}.msh")
                self.assertEqual(opts["--train-level"], str(level))
                self.assertEqual(opts["--val-mesh"], "L2.msh")
                self.assertEqual(opts["--val-level"], "2")
                self.assertEqual(opts["--anchor"], "registered.json")
                self.assertEqual(m["theta0_train"]["F"], 3.0 + level)
                self.assertEqual(m["weights"]["train_forward_eval"], (level + 1) / 13.0)

    def test_l0_cannot_silently_use_a_legacy_or_mislabelled_model(self):
        for method in ("cobyqa_l0", "slsqp_l0"):
            s = fixture()
            del s["models"]
            with self.assertRaisesRegex(ValueError, "explicit L0"):
                run.model_of(s, method)
            s = fixture()
            s["models"]["L0"]["train_level"] = 1
            with self.assertRaisesRegex(ValueError, "training level L0"):
                run.model_of(s, method)

    def test_run_refuses_wrong_observed_training_mesh_before_optimizer(self):
        from test_harness import FakeClock, FakeEvaluator
        from test_run_interruption import fixture as run_fixture
        for method in driver.METHODS:
            with self.subTest(method=method), tempfile.TemporaryDirectory() as td:
                root, clock = Path(td), FakeClock()
                s = run_fixture(root)
                for level in (0, 1):
                    s["models"][f"L{level}"]["train_mesh_sha256"] = str(level) * 64
                expected = run.model_of(s, method)["train_mesh_sha256"]
                # D/E deliberately report the L1 hash while loading from the L0 path.
                wrong = "1" * 64 if expected == "0" * 64 else "0" * 64
                ev = FakeEvaluator(clock, train_sha=wrong)
                with mock.patch.object(run, "execute") as execute:
                    result = run.run_one(s, method, "a", "theta0", 1, root / "trace.json",
                        clock=clock, evaluator_factory=lambda sp, led: ev, diagnostic=False)
                self.assertEqual(result["stop_reason"], "error")
                self.assertIn("training mesh", result["error"])
                self.assertEqual(result["evaluator_ready"]["train_msh_sha256"], wrong)
                self.assertEqual(result["training_model"]["train_mesh_sha256"], expected)
                self.assertEqual(json.loads((root / "trace.json").read_text())["evaluator_ready"], ev.ready)
                execute.assert_not_called()
                self.assertEqual(ev.requests, [])
                self.assertTrue(ev.closed)

    def test_run_accepts_and_retains_matching_training_mesh(self):
        from test_harness import FakeClock, FakeEvaluator
        from test_run_interruption import fixture as run_fixture, successful_prefix
        for method in driver.METHODS:
            with self.subTest(method=method), tempfile.TemporaryDirectory() as td:
                root, clock = Path(td), FakeClock()
                s, ev = run_fixture(root), FakeEvaluator(clock)
                with mock.patch.object(run, "execute", successful_prefix):
                    result = run.run_one(s, method, "a", "theta0", 1, root / "trace.json",
                        clock=clock, evaluator_factory=lambda sp, led: ev, diagnostic=False)
                self.assertEqual(result["stop_reason"], "converged")
                self.assertEqual(result["evaluator_ready"], ev.ready)
                self.assertEqual(result["training_model"]["train_mesh_sha256"], "b" * 64)

    def test_missing_training_hashes_fail_closed(self):
        for expected, observed in ((None, None), ("", ""), ("fixture", "fixture"),
                                   ("b" * 64, None), (None, "b" * 64)):
            with self.subTest(expected=expected, observed=observed):
                with self.assertRaises(driver.WiringError):
                    run.check_training_mesh({"train_msh_sha256": observed},
                                            {"train_mesh_sha256": expected})

    def test_both_diagnostic_paths_refuse_wrong_training_mesh_before_request(self):
        from test_harness import FakeEvaluator, X0
        for method in ("surrogate", "cobyqa_l0", "slsqp_l0"):
            with self.subTest(method=method):
                r, _, _, clock = make_run(method=method)
                r.train(r.problem.u_of(X0), False)
                ev = FakeEvaluator(clock, train_sha="1" * 64)
                before = copy.deepcopy(r.result())
                fn = run.post_run_diagnostic if method == "surrogate" else run.physical_post_run_diagnostic
                with mock.patch.object(run, "c_current_predictions", return_value={"x": X0, "predictions": []}):
                    result = fn(r, lambda: ev, 1., clock)
                self.assertEqual(result["status"], "failed")
                self.assertIn("training mesh", result["error"])
                self.assertEqual(result["evaluator_ready"], ev.ready)
                self.assertEqual(ev.requests, [])
                self.assertTrue(ev.closed)
                self.assertEqual(r.result(), before)
                self.assertFalse(result["counted_in_run_budget"])

    def test_shared_optimizer_paths_and_slsqp_restart_record(self):
        for a, b in (("slsqp", "slsqp_l0"), ("cobyqa", "cobyqa_l0")):
            self.assertEqual(driver.METHOD_SPECS[a]["optimizer"],
                             driver.METHOD_SPECS[b]["optimizer"])
        r, _, _, _ = make_run(method="slsqp_l0")
        r.restarts.append({"fixture": "a real restart record must survive E serialization"})
        self.assertEqual(r.result()["restarts"], r.restarts)

    def test_l0_weights_use_common_unit_despite_different_local_validation_timing(self):
        primary = session.combine_calibrations([
            calibration([11., 13., 15.], 4., 2., 9.)])
        coarse = session.combine_calibrations([
            calibration([20., 30., 40.], 1., .5, 7.)],
            common_unit=primary["unit_seconds"])
        self.assertEqual(coarse["unit_seconds"], 13.)
        self.assertEqual(coarse["measured_validation_median_seconds"], 30.)
        self.assertTrue(coarse["uses_common_unit"])
        self.assertEqual(coarse["weights"]["validation_eval"], 1.)
        self.assertEqual(coarse["weights"]["train_forward_eval"], 1. / 13.)
        self.assertEqual(coarse["weights"]["adjoint_solve"], .5 / 13. / 60.)
        self.assertEqual(coarse["theta0_train"]["F"], 7.)
        self.assertEqual(primary["theta0_train"]["F"], 9.)

    def test_bad_common_units_are_refused(self):
        for unit in (0., -1., float("nan"), float("inf")):
            with self.subTest(unit=unit), self.assertRaises(ValueError):
                session.combine_calibrations([calibration([1., 2., 3.], 1., .5, 7.)], unit)

    def test_main_registry_builds_all_paired_jobs_and_retains_pilot_order(self):
        jobs = main_schedule.build_jobs({s["letter"]: m for m, s in driver.MAIN_METHOD_SPECS.items()})
        self.assertEqual(len(jobs), 100)
        self.assertEqual(len({j["name"] for j in jobs}), 100)
        for k, start in enumerate(main_schedule.STARTS):
            for space in "ab":
                r1 = [j["method_letter"] for j in jobs
                      if (j["start"], j["space"], j["repeat"]) == (start, space, 1)]
                r2 = [j["method_letter"] for j in jobs
                      if (j["start"], j["space"], j["repeat"]) == (start, space, 2)]
                self.assertEqual(r1, list("ABCDE"[k:] + "ABCDE"[:k]))
                self.assertEqual(r2, r1[::-1])
        self.assertEqual(driver.method_order(1), ["slsqp", "cobyqa", "surrogate"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
