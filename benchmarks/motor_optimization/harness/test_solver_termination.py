"""Pinned SciPy exits and their separation from validated target credit."""
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import numpy as np
from scipy.optimize import minimize, OptimizeResult
import driver
from ledger import BudgetExhausted
from test_harness import make_run, X0


class TerminationTests(unittest.TestCase):
    def actual_exit(self, solver, options, scope="optimizer"):
        run = SimpleNamespace(solver_calls=[])
        result = driver.recorded_minimize(run, scope, lambda x: float(np.dot(x, x)),
            np.array([.8, .6]), method=solver, options=options)
        return result, run.solver_calls[0]

    def test_actual_pinned_slsqp_iteration_cap(self):
        result, row = self.actual_exit("SLSQP", {"maxiter": 1, "ftol": 1e-15})
        self.assertEqual(result.status, 9)
        self.assertEqual(row["cap_kind"], "maxiter")
        self.assertTrue(row["cap_hit"])
        self.assertFalse(row["success"])
        self.assertEqual(row["nit"], result.nit)
        self.assertEqual(row["nfev"], result.nfev)

    def test_actual_pinned_cobyqa_caps_are_distinct(self):
        for options, expected_status, kind in [({"maxfev": 6, "maxiter": 1000}, 5, "maxfev"),
                                               ({"maxfev": 1000, "maxiter": 1}, 6, "maxiter")]:
            with self.subTest(kind=kind):
                result, row = self.actual_exit("COBYQA", options)
                self.assertEqual(result.status, expected_status)
                self.assertEqual(row["cap_kind"], kind)
                self.assertTrue(row["cap_hit"])

    def test_initial_cobyqa_eval_cap_retains_upstream_status_quirk(self):
        # Pinned COBYQA main.py catches initial MaxEvalError but returns
        # MAX_ITER_WARNING. Preserve raw status/counters rather than inventing
        # corrected solver evidence; production limits are much larger.
        result, row = self.actual_exit("COBYQA", {"maxfev": 1, "maxiter": 1000})
        self.assertEqual(result.status, 6)
        self.assertTrue(row["cap_hit"])
        self.assertEqual(row["cap_kind"], "maxiter")
        self.assertEqual(row["nit"], 0)
        self.assertEqual(row["nfev"], 1)

    def test_actual_convergence_is_not_a_cap(self):
        for solver in ("SLSQP", "COBYQA"):
            result, row = self.actual_exit(solver, {"maxiter": 1000})
            self.assertTrue(result.success)
            self.assertFalse(row["cap_hit"])
            self.assertEqual(row["message"], str(result.message))

    def test_top_level_all_four_methods_keep_caps_and_failed_exits(self):
        for method in ("slsqp", "slsqp_l0", "cobyqa", "cobyqa_l0"):
            solver = driver.METHOD_SPECS[method]["optimizer"]
            for cap in (True, False):
                run, _, _, _ = make_run(method=method)
                u = run.problem.u_of(X0)
                code = (9 if solver == "slsqp" else 5) if cap else (8 if solver == "slsqp" else -1)
                result = OptimizeResult(x=u, status=code, success=False, message="fixture", nit=3, nfev=7)
                with patch.object(driver, "minimize", return_value=result):
                    reason, _, error = driver.execute(run, u)
                self.assertEqual(reason, "optimizer_cap" if cap else "solver_stopped")
                self.assertIsNone(error)
                record = run.result()
                self.assertEqual(record["optimizer_cap_hit"], cap)
                self.assertEqual(record["solver_calls"][0]["options"]["maxiter"],
                                 100000 if solver == "slsqp" else 10**9)
                self.assertFalse(any(v["attained"] for v in record["attainment"].values()))

    def test_record_survives_target_stop_during_final_validation(self):
        run, _, _, _ = make_run()
        u = run.problem.u_of(X0)
        result = OptimizeResult(x=u, status=9, success=False, message="cap", nit=100000)
        with patch.object(driver, "minimize", return_value=result), \
             patch.object(run, "accept_iterate", side_effect=driver.TargetReached):
            reason, _, _ = driver.execute(run, u)
        self.assertEqual(reason, "T99")
        self.assertTrue(run.result()["optimizer_cap_hit"])

    def test_aborted_calls_are_retained_without_invented_counts(self):
        run, _, _, _ = make_run()
        with patch.object(driver, "minimize", side_effect=BudgetExhausted):
            reason, _, _ = driver.execute(run, run.problem.u_of(X0))
        self.assertEqual(reason, "budget")
        row = run.result()["solver_calls"][0]
        self.assertEqual(row["state"], "aborted")
        self.assertEqual(row["exception_type"], "BudgetExhausted")
        self.assertIsNone(row["cap_hit"])
        self.assertIsNone(row["nit"])

    def test_surrogate_local_cap_is_not_a_whole_run_cap(self):
        run, _, _, _ = make_run(method="surrogate", budget_fe=150,
                                 tamper={"val_mean_scale": .9})
        def capped(*args, **kwargs):
            # Real surrogate loop/candidate checks, controlled inner solver exit.
            return OptimizeResult(x=np.asarray(args[1]), status=9, success=False,
                                  message="local cap", nit=200, nfev=3)
        with patch.object(driver, "minimize", side_effect=capped):
            reason, _, _ = driver.execute(run, run.problem.u_of(X0))
        record = run.result()
        self.assertGreater(record["surrogate_subproblem_cap_count"], 0)
        self.assertFalse(record["optimizer_cap_hit"])
        self.assertNotEqual(reason, "optimizer_cap")
        self.assertTrue(all(c["scope"] == "surrogate_subproblem" for c in record["solver_calls"]))
        self.assertTrue(all(c["options"] == {"maxiter": 200, "ftol": 1e-14} for c in record["solver_calls"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
