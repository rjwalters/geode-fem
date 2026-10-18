"""Tests of the amendment-7 safeguards of the experiment-2 driver (issue
#1201; #1185 amendment 7 with its adoption corrections) and of the two
#1197 gaps (K2, K4). The evaluator is `test_harness.FakeEvaluator` (an
analytic model with a fake clock) with a geometry domain: a coupled guard
`wall - u_0 - u_3 > 0`, a *trap* where the construction refuses (not
buildable) and a region where every remesh attempt fails (mesh-infeasible).
The fake raises if a point that is not buildable reaches `train`,
`validate` or `mesh_check`: such a request would be a refused point
reaching the mesher or the FEM.

    benchmarks/motor_optimization/harness/pyrun.sh \\
        benchmarks/motor_optimization/harness/test_safeguards.py
"""
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import numpy as np  # noqa: E402

import driver  # noqa: E402
import session as session_mod  # noqa: E402
import test_harness as th  # noqa: E402
from acceptance import AcceptanceMismatch  # noqa: E402
from driver import Problem, Refused, Run, WiringError  # noqa: E402
from evaluator import EvaluatorFailure, from_bits, to_bits  # noqa: E402
from ledger import CATEGORIES, Ledger  # noqa: E402
from run import run_one  # noqa: E402

U0 = (np.array(th.X0) - np.array(th.LOWER)) / (np.array(th.UPPER) - np.array(th.LOWER))


class TrapEvaluator(th.FakeEvaluator):
    """The fake with a region where the construction refuses (`trap(u)`):
    not buildable, whatever the smooth guards say."""

    def __init__(self, clock, trap=None, fail_ops=(), transport_fail=(), **kw):
        super().__init__(clock, **kw)
        self.trap = trap or (lambda u: False)
        self.fail_ops = set(fail_ops)
        self.transport_fail = set(transport_fail)

    def domain(self, u):
        d = super().domain(u)
        if self.trap(u):
            d = dict(d, buildable=False, admissible=False, credit_admissible=False,
                     construction_error="slot flank degenerates (trap)")
        return d

    #: A runaway guard: no test needs more than ~200 requests, so a method
    #: that loops (a mutant that never shrinks its step) fails fast.
    MAX_REQUESTS = 5000

    def request(self, op, timeout=None, **f):
        if len(self.requests) >= self.MAX_REQUESTS:
            raise AssertionError(f"runaway: more than {self.MAX_REQUESTS} requests")
        if op in self.transport_fail:
            raise EvaluatorFailure(op, "evaluator gone: broken pipe")
        if op in self.fail_ops:
            self.requests.append((op, f))
            self.clock.advance(th.IO)
            return {"ok": False, "op": op, "error": "solve: Newton diverged",
                    "x_bits": f.get("x_bits"), "charges": []}
        return super().request(op, timeout=timeout, **f)


class GuardedRun(Run):
    """A Run whose method fails fast if it loops: no test asks for more
    than a few hundred points, so a method that keeps asking for the same
    (refused, cached) point forever is an assertion, not a hang."""

    MAX_TRAIN_CALLS = 20000

    def train(self, *a, **kw):
        self._calls = getattr(self, "_calls", 0) + 1
        if self._calls > self.MAX_TRAIN_CALLS:
            raise AssertionError(f"runaway: {self.MAX_TRAIN_CALLS} training calls")
        return super().train(*a, **kw)


def make_run(method="slsqp", budget_fe=1000.0, **kw):
    clock = th.FakeClock()
    led = Ledger(1.0, budget_fe, weights=th.WEIGHTS, clock=clock)
    fake = TrapEvaluator(clock, **kw)
    prob = Problem(fake.ready["space"], th.theta0_train(fake))
    run = GuardedRun(fake, led, th.Anchor(th.F0, th.MEAN0, 0, th.SHA), prob, method,
                     clock=clock)
    return run, fake, led, clock


def bits_of(run, u):
    return to_bits(run.problem.x_of(u))


def fem_requests(fake):
    """Every request that reaches the mesher or the FEM."""
    return [(op, f) for op, f in fake.requests if op in ("train", "validate", "mesh_check")]


class CreditTests(unittest.TestCase):
    def test_an_inadmissible_candidate_with_low_F_and_high_mean_earns_nothing(self):
        # wall = 1.27: the optimum (u_0 + u_3 = 1.25) is buildable (m = 0.02)
        # but inadmissible (c = 0.02 / 0.97 < 0.05). Its F is far below T99
        # and its mean above the floor.
        run, fake, led, clock = make_run(wall=1.27)
        g = run.geometry(th.CENTRE_U)
        self.assertTrue(g["buildable"] and not g["admissible"])
        v = run.validate(th.CENTRE_U)  # no TargetReached
        self.assertTrue(v["F_upper"] < 0.01 * th.F0 and v["mean_ok"])
        self.assertFalse(any(v["attained"].values()))
        self.assertEqual(run.attained, {})
        res = run.result()
        self.assertIsNone(res["best_validated_feasible"])
        self.assertTrue(all(not a["attained"] for a in res["attainment"].values()))
        self.assertTrue(all(not t["attained"] for t in res["checkpoint_60"]["targets"].values()))
        # The record carries the evidence: raw margins, scales, h, discrete.
        det = v["geometry"]
        self.assertFalse(det["admissible"])
        self.assertEqual([gd["name"] for gd in det["guards"]], ["g0", "g1"])
        self.assertLess(det["guards"][0]["h"], 0.0)
        self.assertAlmostEqual(det["guards"][0]["margin"] / det["guards"][0]["scale"],
                               det["guards"][0]["c"])
        self.assertEqual(det["discrete"], [{"name": "components_finite", "pass": True}])

    def test_an_evaluator_crediting_an_inadmissible_candidate_is_refused(self):
        run, fake, led, clock = make_run(wall=1.27, tamper={"admissible": True})
        with self.assertRaises((WiringError, AcceptanceMismatch)):
            run.validate(th.CENTRE_U)

    def test_the_admissible_optimum_is_credited(self):
        run, fake, led, clock = make_run()
        with self.assertRaises(driver.TargetReached):
            run.validate(th.CENTRE_U)


class ConstraintOrderingTests(unittest.TestCase):
    def test_c_feasible_first_uses_the_geometry_violation_too(self):
        # Codex's case: candidate F .01, mean constraint .01, min_h -.5;
        # incumbent F .1, mean constraint .01, min_h .2.
        P = Problem({"free": ["p"], "lower": [0.0], "upper": [1.0], "x0": [0.0]},
                    {"F": 1.0, "mean_full": 1.0})
        mean = 1.0 + 0.0025 + 0.01
        cand = {"F": 0.01, "mean_full": mean, "h": [-0.5], "discrete_pass": True}
        inc = {"F": 0.1, "mean_full": mean, "h": [0.2], "discrete_pass": True}
        self.assertAlmostEqual(P.constraint(cand), 0.01)
        self.assertEqual(driver.violation(P, cand), 0.5)
        self.assertEqual(driver.violation(P, inc), 0.0)
        self.assertFalse(driver.better(P, cand, inc))
        self.assertTrue(driver.better(P, inc, cand))
        # A discrete-guard failure is infeasible whatever else holds.
        bad = dict(inc, discrete_pass=False)
        self.assertEqual(driver.violation(P, bad), math.inf)
        self.assertFalse(driver.better(P, bad, cand))


class MeanFloorOrderingTests(unittest.TestCase):
    def test_c_feasible_first_uses_the_mean_floor_violation_too(self):
        # Equal geometry; only the mean differs: the candidate, below the
        # floor with a lower F, must not beat the feasible incumbent.
        P = Problem({"free": ["p"], "lower": [0.0], "upper": [1.0], "x0": [0.0]},
                    {"F": 1.0, "mean_full": 1.0})
        floor = 1.0 + 0.0025
        cand = {"F": 0.01, "mean_full": floor - 0.01, "h": [0.2], "discrete_pass": True}
        inc = {"F": 0.1, "mean_full": floor + 0.01, "h": [0.2], "discrete_pass": True}
        self.assertAlmostEqual(driver.violation(P, cand), 0.01)
        self.assertEqual(driver.violation(P, inc), 0.0)
        self.assertFalse(driver.better(P, cand, inc))
        self.assertTrue(driver.better(P, inc, cand))


class RestorationTests(unittest.TestCase):
    def test_slsqp_refusal_aborts_restores_and_restarts_at_the_actual_y(self):
        trap = lambda u: u[0] > U0[0] + 0.25  # noqa: E731
        run, fake, led, clock = make_run(trap=trap)
        reason, msg, err = driver.execute(run, U0)
        self.assertIn(reason, ("T99", "converged", "budget", "restoration_failed"), err)
        self.assertGreater(len(run.restarts), 0)
        # Nothing refused reached the evaluator's mesher or FEM.
        for op, f in fem_requests(fake):
            self.assertFalse(trap((np.array(from_bits(f["x_bits"])) - fake.lower) / fake.width))
        for k, rs in enumerate(run.restarts):
            res = run.restorations[rs["restoration"]]
            self.assertEqual(res["rejected_x"], rs["rejected_x"])
            self.assertEqual(res["restored_x"], rs["restored_x"])
            self.assertLessEqual(res["bisections"], driver.MAX_BISECTIONS)
            self.assertTrue(res["restored_geometry"]["admissible"])
            self.assertFalse(res["rejected_geometry"]["buildable"])
            # The restart point is the actual y: the next training request
            # after the restart event is at y, bit for bit.
            i = next(i for i, e in enumerate(run.events)
                     if e["kind"] == "restart" and e["restart"] == k + 1)
            nxt = next(e for e in run.events[i + 1:] if e["kind"] == "train")
            self.assertEqual(to_bits(nxt["x"]), to_bits(rs["restored_x"]))
            self.assertIn("cost_fe", res)
        # Every refusal is recorded with its point and its geometry.
        self.assertTrue(all(r["kind"] == "not_buildable" and r["geometry"] is not None
                            for r in run.refusals))

    def test_the_restoration_anchor_is_the_last_admissible_point_not_the_last_buildable(self):
        # wall 1.0: m = 1 - u_0 - u_3, s = 0.7; admissible while u_0 + u_3
        # <= 0.965; buildable below 1.
        run, fake, led, clock = make_run(wall=1.0)
        u_adm = U0.copy()
        u_bld = U0.copy()
        u_bld[0], u_bld[3] = 0.6, 0.38  # u_0 + u_3 = 0.98: buildable, h < 0
        u_bad = U0.copy()
        u_bad[0], u_bad[3] = 0.8, 0.5
        run.train(u_adm, gradient=False)
        g = run.geometry(u_bld)
        self.assertTrue(g["buildable"] and not g["admissible"])
        run.train(u_bld, gradient=False)
        with self.assertRaises(Refused) as cm:
            run.train(u_bad, gradient=False)
        y = run.restore(cm.exception)
        res = run.restorations[-1]
        self.assertEqual(res["anchor_x"], run.problem.x_of(u_adm).tolist())
        np.testing.assert_allclose(y, u_adm + 0.5 * (u_bad - u_adm), rtol=0, atol=1e-15)

    def test_restoration_passes_a_buildable_but_inadmissible_probe(self):
        # wall 1.0: buildable while u_0 + u_3 < 1, admissible to 0.965. From
        # the anchor (sum 0.3) toward x (sum 1.66, not buildable): y_1 has
        # sum 0.98, buildable but inadmissible, so the bisection goes on;
        # y_2 (sum 0.64) is admissible and is the restored point.
        run, fake, led, clock = make_run(wall=1.0)
        run.train(U0, gradient=False)
        bad = U0.copy()
        bad[0], bad[3] = 0.9, 0.76
        with self.assertRaises(Refused) as cm:
            run.train(bad, gradient=False)
        y = run.restore(cm.exception)
        res = run.restorations[-1]
        self.assertEqual(res["bisections"], 2)
        y1 = U0 + 0.5 * (bad - U0)
        g1 = run.geometry(y1)
        self.assertTrue(g1["buildable"] and not g1["admissible"])
        self.assertFalse(res["probes"][0]["admissible"])
        np.testing.assert_allclose(y, U0 + 0.25 * (bad - U0), rtol=0, atol=1e-15)
        self.assertTrue(res["restored_geometry"]["admissible"])

    def test_a_restoration_with_no_admissible_probe_fails(self):
        trap = lambda u: u[0] > U0[0] + 1e-12  # noqa: E731
        run, fake, led, clock = make_run(trap=trap)
        run.train(U0, gradient=False)
        bad = U0.copy()
        bad[0] = 0.9
        with self.assertRaises(Refused) as cm:
            run.train(bad, gradient=False)
        with self.assertRaises(driver.RestorationFailed):
            run.restore(cm.exception)
        self.assertEqual(len(run.restorations[-1]["probes"]), driver.MAX_BISECTIONS)

    def test_more_than_max_restarts_ends_as_restoration_failed(self):
        trap = lambda u: u[0] > U0[0] + 0.25  # noqa: E731
        old = driver.MAX_RESTARTS
        driver.MAX_RESTARTS = 1
        try:
            run, fake, led, clock = make_run(trap=trap)
            reason, msg, err = driver.execute(run, U0)
        finally:
            driver.MAX_RESTARTS = old
        self.assertGreater(len(run.refusals), 1)
        self.assertEqual(reason, "restoration_failed")
        self.assertIn("restarts", err)
        self.assertEqual(len(run.restarts), 1)

    def test_a_mesh_infeasible_request_follows_the_same_restoration_path(self):
        bad = lambda u: u[0] > U0[0] + 0.25  # noqa: E731
        run, fake, led, clock = make_run(mesh_bad=bad)
        reason, msg, err = driver.execute(run, U0)
        self.assertGreater(len(run.restarts), 0, (reason, err))
        r0 = run.restorations[run.restarts[0]["restoration"]]
        self.assertEqual(r0["kind"], "mesh_infeasible")
        # The restored point passed the mesh policy (a mesh_check), and its
        # repair attempts were charged (remesh at a refused point).
        self.assertTrue(r0["probes"][-1]["mesh_ok"])
        self.assertTrue(any(e["kind"] == "mesh_check" for e in run.events))
        self.assertGreaterEqual(led.totals()["remesh"]["seconds"], 2.0)
        mi = [r for r in run.refusals if r["kind"] == "mesh_infeasible"]
        self.assertEqual(len(mi[0]["attempts"]), 4)
        # A refused point is never requested twice.
        xs = [tuple(f["x_bits"]) for op, f in fake.requests if op == "train"]
        refused = {tuple(to_bits(r["x"])) for r in mi}
        for b in refused:
            self.assertEqual(xs.count(b), 1)


class CobyqaTests(unittest.TestCase):
    def test_a_refused_x_never_reaches_the_fem(self):
        trap = lambda u: u[0] > U0[0] + 0.25  # noqa: E731
        run, fake, led, clock = make_run(method="cobyqa", budget_fe=150.0, trap=trap)
        reason, msg, err = driver.execute(run, U0)
        self.assertIsNone(err)
        self.assertGreater(len(run.refusals), 0)
        for op, f in fem_requests(fake):
            u = (np.array(from_bits(f["x_bits"])) - fake.lower) / fake.width
            self.assertFalse(trap(u), op)
        # The message is recorded but never read for credit.
        self.assertIn("not used for credit", msg if reason == "converged" else "not used for credit")

    def test_the_barrier_is_nan_and_no_fem_value_is_made_up(self):
        trap = lambda u: u[0] > 0.9  # noqa: E731
        run, fake, led, clock = make_run(method="cobyqa", trap=trap)
        fun, cons, cb, seen = driver.cobyqa_callables(run)
        u = U0.copy()
        u[0] = 0.95
        self.assertTrue(math.isnan(fun(u)))
        c = cons(u)
        self.assertTrue(math.isnan(c[0]))
        np.testing.assert_array_equal(c[1:], run.geometry(u)["h"])
        self.assertEqual(fem_requests(fake), [])
        self.assertEqual(len(seen), 1)

    def test_refusals_in_the_initial_set(self):
        # u_0 + σ is refused: one of COBYQA's 2n + 1 initial points. It
        # counts in the set, gets the barrier, and the run continues; the
        # first validation still comes right after the set is complete.
        trap = lambda u: u[0] > U0[0] + 0.05  # noqa: E731
        run, fake, led, clock = make_run(method="cobyqa", budget_fe=150.0, trap=trap)
        reason, msg, err = driver.execute(run, U0)
        self.assertIsNone(err)
        n_init = driver.COBYQA_INITIAL(th.N)
        points = [e for e in run.events if e["kind"] in ("train", "refused", "validate")]
        first_val = next(i for i, e in enumerate(points) if e["kind"] == "validate")
        self.assertEqual(first_val, n_init, "validated right after the 2n + 1 set")
        self.assertIn("refused", [e["kind"] for e in points[:n_init]])


class SurrogateTests(unittest.TestCase):
    def test_the_initial_set_keeps_full_rank_with_the_minus_sigma_retry(self):
        trap = lambda u: u[2] > U0[2] + 0.05  # noqa: E731  (refuses +σ along u_2)
        run, fake, led, clock = make_run(method="surrogate", budget_fe=60.0, trap=trap)
        reason, msg, err = driver.execute(run, U0)
        self.assertIsNone(err)
        c2 = run.c_initial[2]["tried"]
        self.assertEqual([t["step"] for t in c2[:2]], [0.1, -0.1])
        self.assertEqual([t["evaluated"] for t in c2[:2]], [False, True])
        self.assertEqual(c2[0]["refused"], "not_buildable")
        U = np.array([p["u"] for p in run.c_points[:th.N + 1]])
        A = np.hstack([np.ones((th.N + 1, 1)), U])
        self.assertEqual(np.linalg.matrix_rank(A), th.N + 1)

    def test_a_coordinate_with_no_buildable_step_is_restoration_failed(self):
        trap = lambda u: abs(u[1] - U0[1]) > 1e-12  # noqa: E731
        run, fake, led, clock = make_run(method="surrogate", trap=trap)
        reason, msg, err = driver.execute(run, U0)
        self.assertEqual(reason, "restoration_failed")
        self.assertIn("coordinate 1", err)
        # +σ, -σ, then three halvings each way: 8 tries, none evaluated.
        self.assertEqual(len(run.c_initial[1]["tried"]), 8)

    def test_a_refused_candidate_counts_as_a_rejection(self):
        trap = lambda u: u[0] > U0[0] + 0.25  # noqa: E731
        run, fake, led, clock = make_run(method="surrogate", budget_fe=120.0, trap=trap)
        driver.execute(run, U0)
        refused = [k for k, st in enumerate(run.c_steps[:-1]) if st.get("refused")]
        self.assertGreater(len(refused), 0)
        for k in refused:
            self.assertEqual(run.c_steps[k + 1]["delta"], run.c_steps[k]["delta"] / 2)

    def test_two_consecutive_refusals_trigger_a_geometry_point(self):
        # The #1185 ruling: a refusal is a rejection in every respect but the
        # fit, so it counts toward the every-second-rejection geometry point,
        # which is itself checked for buildability.
        trap = lambda u: u[0] > U0[0] + 0.25  # noqa: E731
        run, fake, led, clock = make_run(method="surrogate", budget_fe=120.0, trap=trap)
        driver.execute(run, U0)
        steps = run.c_steps
        hit = [k for k in range(1, len(steps))
               if steps[k].get("refused") and "geometry_point_after" in steps[k]]
        self.assertGreater(len(hit), 0, "a refusal reached the geometry-point step")
        for k in hit:
            g = steps[k]["geometry_point_after"]
            if g is not None:
                self.assertTrue(run.geometry(run.problem.u_of(g))["buildable"])
        # Two refusals in a row reach it: the second of a consecutive pair.
        pairs = [k for k in range(1, len(steps)) if steps[k].get("refused")
                 and steps[k - 1].get("refused")
                 and "geometry_point_after" in steps[k]]
        self.assertGreater(len(pairs), 0)
        for p in run.c_points:
            self.assertFalse(trap(p["u"]))

    def test_initial_step_order(self):
        self.assertEqual(driver.c_initial_steps(0.5, 0.1),
                         [0.1, -0.1, 0.05, -0.05, 0.025, -0.025, 0.0125, -0.0125])
        # + would leave the box: - first; a step outside the box is skipped.
        self.assertEqual(driver.c_initial_steps(0.95, 0.1)[:3], [-0.1, 0.05, -0.05])

    def test_refused_candidates_are_skipped_and_never_fitted(self):
        trap = lambda u: u[0] > U0[0] + 0.25  # noqa: E731
        run, fake, led, clock = make_run(method="surrogate", budget_fe=120.0, trap=trap)
        reason, msg, err = driver.execute(run, U0)
        self.assertIsNone(err)
        for p in run.c_points:
            self.assertFalse(trap(p["u"]))


class CheckpointTests(unittest.TestCase):
    def test_the_60_fe_checkpoint_gives_no_credit_to_work_in_flight(self):
        run, fake, led, clock = make_run(budget_fe=120.0, io=0.0)
        u = th.near_centre(run)
        run.geometry(u)
        # The validation starts at 59 FE and completes after 60.
        clock.advance(59.0 - led.elapsed())
        led.sync("idle")
        v = run.validate(u)
        self.assertGreater(v["wall_fe"], 60.0)
        self.assertLess(v["wall_fe"] - th.VALIDATE, 60.0)
        res = run.result()
        self.assertTrue(res["attainment"]["T50"]["attained"])
        self.assertFalse(res["checkpoint_60"]["targets"]["T50"]["attained"])
        # One that completes by 60 counts.
        run2, fake2, led2, clock2 = make_run(budget_fe=120.0, io=0.0)
        run2.geometry(u)
        clock2.advance(57.0 - led2.elapsed())
        led2.sync("idle")
        v2 = run2.validate(u)
        self.assertLessEqual(v2["wall_fe"], 60.0)
        self.assertTrue(run2.result()["checkpoint_60"]["targets"]["T50"]["attained"])


class Issue1197Tests(unittest.TestCase):
    def test_k2_the_initial_set_gate_fires_exactly_when_the_set_completes(self):
        # From θ0 (far from the optimum) the best point of each initial set
        # is not the start, so amendment 5 §1 validates it at once: right
        # after evaluation 2n + 1 (B) or n + 1 (C), not one later.
        for method, n_init in (("cobyqa", driver.COBYQA_INITIAL(th.N)),
                               ("surrogate", th.N + 1)):
            run, fake, led, clock = make_run(method=method, budget_fe=300.0)
            driver.execute(run, U0)
            first_val = next(i for i, e in enumerate(run.events) if e["kind"] == "validate")
            n_train = sum(1 for e in run.events[:first_val] if e["kind"] == "train")
            self.assertEqual(n_train, n_init, method)

    def test_k4_a_budget_stop_records_the_aborted_request(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "anchor.json").write_text(json.dumps({"anchor": th.anchor_json()}))
            (d / "starts.json").write_text(json.dumps({"starts": {"theta0": {"a": th.X0}}}))
            probe = th.FakeEvaluator(th.FakeClock())
            session = {"label": "test", "session_id": "t", "budget_fe": 9.5,
                       "unit_seconds": 1.0, "weights": th.WEIGHTS,
                       "theta0_train": th.theta0_train(probe),
                       "evaluator": {"anchor": str(d / "anchor.json"), "train_mesh_sha256": "b" * 64},
                       "starts": str(d / "starts.json")}
            ops = {"train": ("forward", "line_search"), "validate": ("validation",),
                   "geometry": ("geometry_check",), "mesh_check": ("remesh",)}
            session["primary_model"] = "L1"
            session["models"] = {f"L{level}": {"train_mesh": f"fixture_L{level}.msh",
                "train_level": level, "train_mesh_sha256": "b" * 64,
                "theta0_train": session["theta0_train"],
                "weights": session["weights"]} for level in (0, 1)}
            for method in driver.METHODS:
                clock = th.TickingClock()
                t = run_one(session, method, "a", "theta0", 1, d / f"{method}.json",
                            clock=clock, evaluator_factory=lambda sp, led, c=clock:
                            th.FakeEvaluator(c), diagnostic=False)
                self.assertEqual(t["stop_reason"], "budget", method)
                ab = json.loads((d / f"{method}.json").read_text())["aborted_request"]
                self.assertIn(ab["op"], ops, method)
                self.assertIn(ab["category"], ops[ab["op"]], method)
                self.assertIn(ab["category"], CATEGORIES)
                self.assertEqual(len(ab["x"]), th.N, "the in-flight request's x")


class ProvenanceTests(unittest.TestCase):
    def test_a_failed_request_records_its_design_point(self):
        run, fake, led, clock = make_run(fail_ops=("train",))
        u = U0.copy()
        u[1] = 0.33
        with self.assertRaises(EvaluatorFailure):
            run.train(u, gradient=False)
        e = run.events[-1]
        self.assertEqual(e["kind"], "failed")
        self.assertEqual(to_bits(e["x"]), bits_of(run, u))

    def test_a_transport_failure_records_its_design_point(self):
        run, fake, led, clock = make_run(transport_fail=("train",))
        u = U0.copy()
        u[1] = 0.44
        with self.assertRaises(EvaluatorFailure):
            run.train(u, gradient=False)
        e = run.events[-1]
        self.assertEqual((e["kind"], e["detail"]), ("failed", {"transport": True}))
        self.assertEqual(to_bits(e["x"]), bits_of(run, u))

    def test_every_training_remesh_hash_is_in_the_trace(self):
        run, fake, led, clock = make_run()
        u = U0.copy()
        u[0] = 0.99  # the fake remeshes here
        run.train(u, gradient=False)
        ev = run.events[-1]
        self.assertTrue(ev["remesh"])
        self.assertEqual([m["msh_sha256"] for m in ev["remesh_meshes"]], ["f" * 64])

    def test_trace_separates_in_run_and_post_run_wall_time(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "anchor.json").write_text(json.dumps({"anchor": th.anchor_json()}))
            (d / "starts.json").write_text(json.dumps({"starts": {"theta0": {"a": th.X0}}}))
            probe = th.FakeEvaluator(th.FakeClock())
            session = {"label": "test", "session_id": "t", "budget_fe": 9.5,
                       "unit_seconds": 1.0, "weights": th.WEIGHTS,
                       "theta0_train": th.theta0_train(probe),
                       "evaluator": {"anchor": str(d / "anchor.json"), "train_mesh_sha256": "b" * 64},
                       "starts": str(d / "starts.json")}
            clock = th.FakeClock()
            t = run_one(session, "surrogate", "a", "theta0", 1, d / "c.json", clock=clock,
                        evaluator_factory=lambda sp, led, c=clock: th.FakeEvaluator(c))
            self.assertEqual(t["in_run_wall_seconds"], t["ledger"]["total_seconds"])
            # The run's linear-algebra stack is in its record.
            self.assertIn("numpy_blas", t["linear_algebra"])
            self.assertEqual(t["linear_algebra"]["numpy"], np.__version__)
            self.assertAlmostEqual(t["in_run_wall_seconds"], 9.5, places=9)
            self.assertGreater(t["post_run_wall_seconds"], 0.0)
            self.assertEqual(t["post_run_wall_seconds"], t["post_run_diagnostic"]["seconds"])


class StartTests(unittest.TestCase):
    def test_an_inadmissible_start_is_a_fault(self):
        run, fake, led, clock = make_run(wall=0.31)  # m(u0) = 0.01: h = ...
        # At the start c = 1 by construction of the scale: admissible.
        self.assertTrue(run.geometry(U0)["admissible"])
        run, fake, led, clock = make_run(trap=lambda u: True)
        reason, msg, err = driver.execute(run, U0)
        self.assertEqual(reason, "restoration_failed")
        self.assertEqual(fem_requests(fake), [])


class StartAdmissibilityTests(unittest.TestCase):
    def test_a_buildable_but_inadmissible_start_is_refused_before_any_fem(self):
        run, fake, led, clock = make_run(wall=1.0)
        u = U0.copy()
        u[0], u[3] = 0.6, 0.38
        g = run.geometry(u)
        self.assertTrue(g["buildable"] and not g["admissible"])
        reason, msg, err = driver.execute(run, u)
        self.assertEqual((reason, err), ("restoration_failed", "the start is not admissible"))
        self.assertEqual(fem_requests(fake), [])


class HostRuleTests(unittest.TestCase):
    def test_contamination_needs_more_than_60_s_above_1_5_n(self):
        n = 6  # threshold 9
        # A stretch lasts until the load is seen at or below the threshold
        # (conservative at a 5 s sampling): above 0..55, below at 60 is
        # exactly 60 s, not more; below only at 65 is more.
        over = [(t, 9.5) for t in range(0, 56, 5)] + [(60, 8.0)]
        self.assertEqual(session_mod.contaminated(over, 0, 100, n), (False, 60))
        longer = [(t, 9.5) for t in range(0, 61, 5)] + [(65, 8.0)]
        self.assertEqual(session_mod.contaminated(longer, 0, 100, n), (True, 65))
        # Exactly 1.5 N is not above it; a stretch open at the end counts
        # to the end of the run; samples outside the run do not count.
        self.assertFalse(session_mod.contaminated([(t, 9.0) for t in range(0, 200, 5)],
                                                  0, 200, n)[0])
        self.assertTrue(session_mod.contaminated([(10, 9.1)], 0, 75, n)[0])
        self.assertFalse(session_mod.contaminated([(t, 20.0) for t in range(0, 300, 5)],
                                                  400, 500, n)[0])

    def test_pre_run_wait_is_for_load_at_most_n_plus_1(self):
        t = [0.0]
        loads = iter([9.0, 8.0, 7.0, 7.5])
        ok, waited, v = session_mod.wait_for_idle(
            6, 100.0, interval=5.0, clock=lambda: t[0], load=lambda: (next(loads), 0, 0),
            sleep=lambda s: t.__setitem__(0, t[0] + s))
        self.assertEqual((ok, waited, v), (True, 10.0, 7.0))
        ok, waited, v = session_mod.wait_for_idle(
            6, 10.0, interval=5.0, clock=lambda: t[0], load=lambda: (50.0, 0, 0),
            sleep=lambda s: t.__setitem__(0, t[0] + s))
        self.assertFalse(ok)

    def test_host_idle_needs_every_contaminated_run_re_run_cleanly(self):
        def r(name, bad):
            return {"job_name": name, "host_contaminated": bad}
        self.assertTrue(session_mod.session_host_idle([r("a", False), r("b", False)]))
        self.assertFalse(session_mod.session_host_idle([r("a", True), r("b", False)]))
        self.assertTrue(session_mod.session_host_idle([r("a", True), r("b", False),
                                                       r("a", False)]))
        self.assertFalse(session_mod.session_host_idle([r("a", True), r("a", True)]))


class _Monitor:
    def __init__(self, samples):
        self.samples = samples


class SessionGateTests(unittest.TestCase):
    def ctx(self, d, samples=(), gate=True):
        (Path(d) / "logs").mkdir(exist_ok=True)
        return {"out": Path(d), "procs": 6, "max_wait": 10.0, "interval": 5.0,
                "monitor": _Monitor(list(samples)), "gate": gate}

    JOB = {"name": "slsqp_a_theta0_r1", "method": "slsqp", "space": "a", "start": "theta0",
           "repeat": 1, "position": 0}

    def test_no_process_is_spawned_when_the_idle_wait_times_out(self):
        spawned = []
        with tempfile.TemporaryDirectory() as d:
            r = session_mod.run_job(self.JOB, self.ctx(d),
                                    wait=lambda *a, **k: (False, 10.0, 30.0),
                                    spawn=lambda *a, **k: spawned.append(a))
        self.assertEqual(spawned, [])
        self.assertEqual(r["status"], "deferred")
        self.assertIsNone(r["exit"])

    def test_a_deferred_job_is_requeued_and_never_started_while_deferred(self):
        calls = []

        def fake_run(job, ctx, suffix):
            n = sum(1 for c in calls if c == job["name"])
            calls.append(job["name"])
            status = "deferred" if (job["name"] == "x" and n < 2) else "ran"
            return {"name": job["name"], "job_name": job["name"], "position": job["position"],
                    "status": status, "host_contaminated": False}
        jobs = [{"name": "x", "position": 0}, {"name": "y", "position": 1}]
        with tempfile.TemporaryDirectory() as d:
            out = session_mod.schedule(jobs, self.ctx(d), max_deferrals=3, run=fake_run)
        self.assertEqual([r["status"] for r in out if r["name"] == "x"],
                         ["deferred", "deferred", "ran"])
        self.assertEqual(calls.count("y"), 1)
        # Out of deferrals: it stays deferred, and the session is not idle.
        with tempfile.TemporaryDirectory() as d:
            out = session_mod.schedule(
                jobs, self.ctx(d), max_deferrals=2,
                run=lambda j, c, s: dict(fake_run(j, c, s), status="deferred"))
        self.assertEqual(sum(1 for r in out if r["name"] == "x"), 2)
        self.assertFalse(session_mod.session_host_idle(out))

    def test_contamination_is_judged_on_the_in_run_interval_only(self):
        # Overloaded only after the in-run phase ends (C's diagnostic).
        with tempfile.TemporaryDirectory() as d:
            samples = [(1000.0 + t, 3.0) for t in range(0, 100, 5)] \
                + [(1105.0 + t, 50.0) for t in range(0, 200, 5)]

            def spawn(cmd, **kw):
                out = Path(cmd[cmd.index("--out") + 1])
                out.write_text(json.dumps({"in_run_start_time": 1000.0,
                                           "in_run_end_time": 1100.0,
                                           "post_run_end_time": 1300.0,
                                           "ledger": {"total_fe": 1.0}}))
                return type("P", (), {"returncode": 0})()
            r = session_mod.run_job(self.JOB, self.ctx(d, samples),
                                    wait=lambda *a, **k: (True, 0.0, 1.0), spawn=spawn)
            self.assertEqual(r["status"], "ran")
            self.assertFalse(r["host_contaminated"])
            self.assertEqual(r["diagnostic_load"]["max"], 50.0)
            t = json.loads((Path(d) / "slsqp_a_theta0_r1.json").read_text())
            self.assertEqual(t["in_run_load"]["max"], 3.0)
            # The same overload inside the in-run phase contaminates.
            samples2 = [(1000.0 + t, 50.0) for t in range(0, 100, 5)]
            r2 = session_mod.run_job(self.JOB, self.ctx(d, samples2), suffix="_second",
                                     wait=lambda *a, **k: (True, 0.0, 1.0), spawn=spawn)
            self.assertTrue(r2["host_contaminated"])

    def test_the_calibration_is_gated_and_monitored(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(SystemExit):
                session_mod.gated_calibration(self.ctx(d), lambda: {}, 2,
                                              wait=lambda *a, **k: (False, 10.0, 30.0))
            ran = []
            cal = session_mod.gated_calibration(
                self.ctx(d), lambda: ran.append(1) or {}, 2,
                wait=lambda *a, **k: (True, 0.0, 2.0))
            self.assertEqual(ran, [1])
            self.assertFalse(cal["host"]["host_contaminated"])
            self.assertEqual(cal["host"]["pre_run_load"], 2.0)
            # Overloaded through a 100 s calibration: contaminated.
            busy = [(1000.0 + k, 50.0) for k in range(0, 101, 5)]
            ticks = iter([1000.0, 1100.0])
            cal = session_mod.gated_calibration(self.ctx(d, busy), lambda: {}, 2,
                                                wait=lambda *a, **k: (True, 0.0, 2.0),
                                                clock=lambda: next(ticks))
            self.assertTrue(cal["host"]["host_contaminated"])
            self.assertEqual(cal["host"]["longest_overload_s"], 100.0)


class PilotReplayTests(unittest.TestCase):
    """The pilot's failed requests (`pilot_failures.json`). Two layers:
    what holds on any linear-algebra stack, and the bit-for-bit replay,
    which holds only on the stack the pilot ran on (COBYQA's and C's paths
    depend on BLAS/LAPACK at the bit level; Accelerate on macOS, OpenBLAS
    on Linux)."""

    def record(self):
        return json.loads((HERE / "pilot_failures.json").read_text())

    def test_the_recovered_designs_on_any_platform(self):
        rec = self.record()
        self.assertEqual(len(rec["runs"]), 13)
        self.assertEqual(len(rec["designs"]), 7)
        flank = [d for d in rec["designs"] if d["kind"] == "slot_flank"]
        self.assertEqual(sum(len(d["runs"]) for d in flank), 11)
        pilot = HERE.parent / "runs/pilot"
        for r in rec["runs"]:
            # The error string is the committed trace's.
            t = json.loads((pilot / f"{r['name']}.json").read_text())
            self.assertEqual(r["trace_error"], t["error"], r["name"])
            self.assertEqual(t["stop_reason"], "failed")
            self.assertIsNone(r["divergence"], r["name"])
            self.assertEqual(to_bits(r["x"]), r["x_bits"])
            if r["mode"] == "real":
                self.assertEqual("train: " + r["error"], r["trace_error"])
                # A fresh evaluator fails the same way. For the fresh-mesh
                # failure only the fresh mesh is a function of the design:
                # the moved-mesh trigger depends on the remesh history.
                cut = lambda e: e.split(" (moved mesh:")[0]  # noqa: E731
                self.assertEqual(cut(r["probe_error"]), cut(r["trace_error"]))
        # The construction's verdicts (Rust, motor_experiment2_domain, in
        # domain_inventory.json): the slot-flank designs are not buildable,
        # the fresh-mesh one is buildable but inadmissible.
        inv = json.loads((HERE / "domain_inventory.json").read_text())
        verdict = {tuple(p["runs"]): p for p in inv["pilot_failures"]}
        for d in rec["designs"]:
            v = verdict[tuple(d["runs"])]["summary"]
            if d["kind"] == "slot_flank":
                self.assertFalse(v["buildable"], d["runs"])
            else:
                self.assertEqual(d["kind"], "fresh_mesh_policy")
                self.assertTrue(v["buildable"] and not v["admissible"], d["runs"])

    def test_the_replay_is_bit_for_bit_on_the_pilot_stack(self):
        import subprocess
        import linalg_info
        host = json.loads((HERE / "pilot_replay_host.json").read_text())
        here = linalg_info.fingerprint()
        if not linalg_info.same_stack(here, host["linear_algebra"]):
            self.skipTest("bit-for-bit replay holds only on the pilot's linear-algebra "
                          f"stack {host['linear_algebra']}; this one is {here}")
        p = subprocess.run([sys.executable, "-I", str(HERE / "recover_pilot_failures.py"),
                            "--check"], capture_output=True, text=True, timeout=600)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("mismatches: none", p.stdout)
        self.assertEqual(sum(1 for r in self.record()["runs"] if r["mode"] == "replay"), 7)


class PinnedHEvaluator(TrapEvaluator):
    """h of guard g0 pinned to `h_pin` at the optimum (CENTRE_U): buildable,
    and inadmissible under h >= 0 whenever h_pin < 0."""

    def __init__(self, clock, h_pin, **kw):
        super().__init__(clock, **kw)
        self.h_pin = h_pin

    def domain(self, u):
        d = super().domain(u)
        if not np.allclose(u, th.CENTRE_U, rtol=0, atol=1e-12):
            return d
        h = list(d["h"])
        h[0] = self.h_pin
        guards = [dict(g) for g in d["guards"]]
        guards[0]["h"] = self.h_pin
        return dict(d, h=h, guards=guards, min_h=min(h),
                    admissible=d["buildable"] and min(h) >= 0,
                    credit_admissible=d["buildable"] and min(h) >= -1e-12)


class CreditToleranceTests(unittest.TestCase):
    """The #1185 ruling: credit and the best feasible point need every
    h_i >= -1e-12; the same for every method."""

    def run_at(self, h_pin):
        clock = th.FakeClock()
        led = Ledger(1.0, 1000.0, weights=th.WEIGHTS, clock=clock)
        fake = PinnedHEvaluator(clock, h_pin)
        prob = Problem(fake.ready["space"], th.theta0_train(fake))
        return Run(fake, led, th.Anchor(th.F0, th.MEAN0, 0, th.SHA), prob, "slsqp",
                   clock=clock)

    def test_an_iterate_at_h_minus_5e_16_that_meets_the_target_earns_credit(self):
        run = self.run_at(-5e-16)
        g = run.geometry(th.CENTRE_U)
        self.assertFalse(g["admissible"])  # h >= 0 exactly: not admissible
        self.assertTrue(g["credit_admissible"])
        with self.assertRaises(driver.TargetReached):
            run.validate(th.CENTRE_U)
        self.assertEqual(set(run.attained), {"T50", "T90", "T99"})
        self.assertIsNotNone(run.result()["best_validated_feasible"])

    def test_an_iterate_at_exactly_minus_1e_12_earns_credit(self):
        run = self.run_at(-1e-12)
        g = run.geometry(th.CENTRE_U)
        self.assertTrue(g["credit_admissible"])
        with self.assertRaises(driver.TargetReached):
            run.validate(th.CENTRE_U)

    def test_an_iterate_at_h_minus_1e_9_earns_nothing(self):
        run = self.run_at(-1e-9)
        v = run.validate(th.CENTRE_U)
        self.assertFalse(any(v["attained"].values()))
        self.assertEqual(run.attained, {})
        self.assertIsNone(run.result()["best_validated_feasible"])

    def test_the_tolerance_is_for_credit_only(self):
        # The restoration anchor still needs h >= 0 exactly.
        run = self.run_at(-5e-16)
        run.train(th.CENTRE_U, gradient=False)
        self.assertIsNone(run.anchor_u)


class LinearAlgebraGateTests(unittest.TestCase):
    """The gate of the bit-for-bit pilot replay (`linalg_info.same_stack`)."""

    def test_same_stack_checks_every_key(self):
        import linalg_info
        for k in ("system", "machine", "macos_version", "numpy", "scipy", "numpy_blas",
                  "numpy_blas_version", "numpy_lapack", "numpy_lapack_version", "scipy_blas",
                  "scipy_blas_version", "scipy_lapack", "scipy_lapack_version"):
            self.assertIn(k, linalg_info.REPLAY_KEYS)
        fp = linalg_info.fingerprint()
        self.assertTrue(linalg_info.same_stack(fp, dict(fp)))
        for k in linalg_info.REPLAY_KEYS:
            other = dict(fp, **{k: f"{fp.get(k)}-other"})
            self.assertFalse(linalg_info.same_stack(fp, other), k)
            self.assertFalse(linalg_info.same_stack(other, fp), k)

    def test_a_mismatched_stack_skips_the_exact_replay(self):
        import linalg_info
        real = linalg_info.fingerprint
        host = json.loads((HERE / "pilot_replay_host.json").read_text())["linear_algebra"]
        linalg_info.fingerprint = lambda: dict(host, numpy_blas="openblas")
        try:
            t = PilotReplayTests("test_the_replay_is_bit_for_bit_on_the_pilot_stack")
            with self.assertRaises(unittest.SkipTest):
                t.test_the_replay_is_bit_for_bit_on_the_pilot_stack()
        finally:
            linalg_info.fingerprint = real

    def test_the_recorded_pilot_stack_names_its_macos_version(self):
        host = json.loads((HERE / "pilot_replay_host.json").read_text())
        self.assertEqual(host["linear_algebra"]["system"], "Darwin")
        self.assertTrue(host["linear_algebra"]["macos_version"])
        self.assertIn(host["linear_algebra"]["macos_version"], host["pilot_host"]["platform"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
