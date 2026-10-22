"""Tests of method F `slsqp_fd` (#1218 protocol §1) and of its {A', F}
session queue (protocol §§2-3). The evaluator is test_harness's analytic
fake, whose train replies carry the exact gradient (the fake's "adjoint"),
with a fake clock, so every charge is exact. With the evaluator binary
built (MOTOR_HARNESS_EVALUATOR, else target/release/examples), F's
gradient is also checked against the adjoint on the L0 CI model.

    benchmarks/motor_optimization/harness/pyrun.sh \\
        benchmarks/motor_optimization/harness/test_fd_gradient.py
"""
import contextlib
import hashlib
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import numpy as np  # noqa: E402

import driver  # noqa: E402
import fd_ablation_schedule  # noqa: E402
import fd_warm_schedule  # noqa: E402
import main_schedule  # noqa: E402
import session  # noqa: E402
from driver import FD_STEP, Refused, WiringError  # noqa: E402
from evaluator import from_bits  # noqa: E402
from test_harness import CENTRE_U, F0, N, X0, make_run  # noqa: E402

#: The registered main queue and its method specs before method F existed
#: (canonical JSON SHA-256 as `export_queue`, origin/main c08ce9e6): F must
#: leave both unchanged (#1218 acceptance 2).
MAIN_JOBS_SHA256 = "eedac7dbbc7f53516986417f69161eb9915f9b487cc1b9f6fb0af8d2b4d5e0ac"
MAIN_SPECS_SHA256 = "afc6921dc06bfcd2b149bfd5f13fbdcd3be04acc7230844598f5253de35f9374"
#: The #1218 {A', F} queue and its method specs as merged (origin/main
#: f94acad3, before F_w): F_w must leave both unchanged (#1224).
FD_ABLATION_JOBS_SHA256 = "b0620ac5672cdcc2346da840adf4f331b277199322eac31c886d5ac47c3c36c0"
FD_ABLATION_SPECS_SHA256 = "fdad5d3a3de6e71e9664a0d324d9f5107300ffb88539dac421a656d6541b9360"
#: The #1224 {A', F_w} queue as registered (pinned before any F_w run).
FD_WARM_JOBS_SHA256 = "f1a7b045c3e71f9d3f3a29df59673e6630dcfc891a51e1a7e1038f6c1a4991b8"
#: The FD-vs-adjoint bounds of `fd_gradient_check` on the L0 CI model.
L0_NORM_TOL = 5e-3
L0_COMPONENT_TOL = 1e-1


def canonical(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def fd_run(**kw):
    return make_run(method="slsqp_fd", **kw)


def trains(fake):
    return [f for op, f in fake.requests if op == "train"]


def exact_gradients(run, u):
    """The fake's analytic gradients at u (x units), from a separate fake."""
    other, fake, _, _ = make_run()
    r = other.train(u, gradient=True)
    return r["dF"], r["dmean_full"]


class FdGradientTests(unittest.TestCase):
    def test_interior_gradient_matches_the_adjoint_reply(self):
        run, fake, led, _ = fd_run()
        u = np.array([0.5, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        g = driver.fd_gradient(run, u)
        dF, dm = exact_gradients(run, u)
        # The fake is quadratic in x: forward differences are off by
        # (1/2) f'' h, relative 1e-5 at most here.
        np.testing.assert_allclose(g["dF"], dF, rtol=1e-4, atol=1e-12)
        np.testing.assert_allclose(g["dmean_full"], dm, rtol=1e-6, atol=1e-12)
        rec = run.fd_gradients[-1]
        self.assertFalse(rec["refused"])
        self.assertEqual([c["side"] for c in rec["components"]], ["forward"] * N)
        x = run.problem.x_of(u)
        for k, c in enumerate(rec["components"]):
            self.assertAlmostEqual(c["h"], FD_STEP, delta=1e-15 * max(1.0, abs(x[k])) * 4)

    def test_one_gradient_is_n_plus_1_solves_every_one_charged(self):
        """[#1218] Every FD solve is an ordinary charged request: the base
        and the n stencil solves are `forward` with their solve counts, the
        `check` probes are `mesh_motion`, nothing is adjoint, and the
        objective and constraint gradients share one stencil."""
        run, fake, led, _ = fd_run()
        P = run.problem
        u = np.array([0.5, 0.5, 0.5, 0.5])
        fun, jac, con, con_jac, geo, geo_jac, callback = driver.slsqp_callables(run)
        fun(u)
        jac(u)
        con_jac(u)
        jac(u)
        reqs = trains(fake)
        self.assertEqual(len(reqs), N + 1)
        self.assertTrue(all(f["gradient"] is False for f in reqs))
        base = P.x_of(u)
        # The frozen step (#1218 protocol §1): 1e-6 in each component's unit.
        self.assertEqual(FD_STEP, 1e-6)
        for k, f in enumerate(reqs[1:]):
            xs = np.array(from_bits(f["x_bits"]))
            d = xs - base
            self.assertEqual(np.count_nonzero(d), 1)
            self.assertAlmostEqual(d[k], 1e-6, delta=1e-15)
        # Stencil points are not optimizer evaluations: never cached as
        # one, never the incumbent, never a restoration anchor.
        self.assertEqual(list(run._train), [base.tobytes()])
        self.assertIs(run.training_incumbent, run._train[base.tobytes()])
        np.testing.assert_array_equal(run.anchor_u, P.u_of(base))
        self.assertEqual(len(run._anchor_trail), 1)
        tot = led.totals()
        self.assertEqual(tot["forward"]["entries"], N + 1)
        self.assertEqual(tot["forward"]["forward_solves"], 60 * (N + 1))
        self.assertEqual(tot["forward"]["seconds"], float(N + 1))
        self.assertEqual(tot["adjoint"]["entries"], 0)
        self.assertEqual(tot["adjoint"]["adjoint_solves"], 0)
        checks = [op for op, _ in fake.requests if op == "check"]
        self.assertEqual(len(checks), N)
        # train motion (N + 1) and the N checks, all at the fake's 1/64 s.
        self.assertEqual(tot["mesh_motion"]["entries"], 2 * N + 1)
        self.assertEqual(tot["mesh_motion"]["seconds"], (2 * N + 1) / 64)
        self.assertEqual(led.charged_seconds(), led.elapsed())
        self.assertEqual(sum(fake.emitted.values()), led.charged_seconds()
                         - tot["overhead"]["seconds"])
        self.assertEqual(run.fd_gradients[-1]["solves"], N)
        self.assertEqual(len(run.fd_gradients), 1)

    def test_backward_step_at_the_upper_bound(self):
        run, fake, _, _ = fd_run()
        u = np.array([0.5, 1.0, 0.5, 0.5])
        run.train(u, gradient=False)
        g = driver.fd_gradient(run, u)
        rec = run.fd_gradients[-1]
        c = rec["components"][1]
        self.assertEqual(c["side"], "backward")
        self.assertEqual(c["attempts"][0]["unusable"], "outside_box")
        self.assertLess(c["h"], 0.0)
        # No request ever left the box.
        for op, f in fake.requests:
            x = np.array(from_bits(f["x_bits"]))
            self.assertTrue(np.all(x <= run.problem.upper), op)
        dF, dm = exact_gradients(run, u)
        np.testing.assert_allclose(g["dF"], dF, rtol=1e-4, atol=1e-12)

    def test_backward_step_when_the_forward_step_would_remesh(self):
        run, fake, _, _ = fd_run()
        # x_0 + h crosses the fake's remesh boundary u_0 = 0.95 (width 1).
        u = np.array([0.95 - 0.5 * FD_STEP, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        g = driver.fd_gradient(run, u)
        c = run.fd_gradients[-1]["components"][0]
        self.assertEqual(c["attempts"][0]["unusable"], "would_remesh")
        self.assertEqual(c["side"], "backward")
        # The remeshing point was probed (check) but never solved, and
        # every solve is on the base's generation 0.
        self.assertTrue(all(from_bits(f["x_bits"])[0] <= 0.95 for f in trains(fake)))
        self.assertEqual({e["generation"] for e in run.events if e["kind"] == "fd_train"}, {0})
        dF, _ = exact_gradients(run, u)
        np.testing.assert_allclose(g["dF"], dF, rtol=1e-4, atol=1e-12)

    def test_an_inadmissible_but_buildable_step_is_used(self):
        """Freeze ruling 1: a stencil point need only be buildable, as A's
        `train` requires. Guard 1: h_1 = (wall - u_0 - u_3) / s_1 - 0.05,
        s_1 = wall - 0.3; at wall 1.5 the base has h_1 = 1e-7 and +h in u_0
        or u_3 gives h_1 < 0, still buildable: the forward step is used."""
        run, fake, _, _ = fd_run(wall=1.5)
        u3 = 1.5 - 0.5 - (0.05 + 1e-7) * 1.2
        u = np.array([0.5, 0.5, 0.5, u3])
        self.assertTrue(run.geometry(u)["admissible"])
        run.train(u, gradient=False)
        g = driver.fd_gradient(run, u)
        rec = run.fd_gradients[-1]
        self.assertEqual([c["side"] for c in rec["components"]], ["forward"] * N)
        x = run.problem.x_of(u)
        for k in (0, 3):
            xs = x.copy()
            xs[k] += FD_STEP
            geo = run.geometry_at(xs)
            self.assertTrue(geo["buildable"])
            self.assertFalse(geo["admissible"])
        dF, dm = exact_gradients(run, u)
        np.testing.assert_allclose(g["dF"], dF, rtol=1e-4, atol=1e-12)

    def test_an_inadmissible_base_gets_a_gradient_where_a_does(self):
        """Freeze ruling 1: at a buildable base with h < 0 (SLSQP iterates
        can reach one) A gets an adjoint gradient, and so must F: no
        refusal, every stencil point buildable but inadmissible."""
        run, fake, _, _ = fd_run(wall=1.5)
        u = np.array([0.5, 0.5, 0.5, 1.5 - 0.5 - 0.04 * 1.2])
        g0 = run.geometry(u)
        self.assertTrue(g0["buildable"])
        self.assertFalse(g0["admissible"])
        fun, jac, con, con_jac, geo, geo_jac, callback = driver.slsqp_callables(run)
        fun(u)
        grad = jac(u) / (run.problem.width / run.problem.f_ref)
        dF, dm = exact_gradients(run, u)
        np.testing.assert_allclose(grad, dF, rtol=1e-4, atol=1e-12)
        np.testing.assert_allclose(con_jac(u) / (run.problem.width / run.problem.mean_ref), dm,
                                   rtol=1e-6, atol=1e-12)
        rec = run.fd_gradients[-1]
        self.assertFalse(rec["refused"])
        self.assertEqual(rec["solves"], N)
        self.assertEqual(run.refusals, [])
        stencil = [np.array(e["x"]) for e in run.events if e["kind"] == "fd_train"]
        self.assertEqual(len(stencil), N)
        self.assertFalse(any(run.geometry_at(x)["admissible"] for x in stencil))
        # A at the same point: an adjoint gradient, no refusal either.
        a, _, _, _ = make_run(wall=1.5)
        self.assertIsNotNone(a.train(u, gradient=True)["dF"])

    def test_backward_step_when_the_forward_step_is_not_buildable(self):
        # Guard 1 at wall 1.0: m_1 = 1 - u_0 - u_3; the base has m_1 = 5e-7
        # (buildable, h < 0), so +h in u_3 makes m_1 < 0.
        run, fake, _, _ = fd_run(wall=1.0)
        u = np.array([0.4, 0.5, 0.5, 0.6 - 5e-7])
        run.train(u, gradient=False)
        driver.fd_gradient(run, u)
        c = run.fd_gradients[-1]["components"]
        for k in (0, 3):
            self.assertEqual(c[k]["side"], "backward")
            self.assertEqual(c[k]["attempts"][0]["unusable"], "not_buildable")
        for f in trains(fake):
            self.assertTrue(run.geometry_at(np.array(from_bits(f["x_bits"])))["buildable"])

    def test_both_steps_unusable_is_refused_as_derivative_undefined(self):
        """[#1218] Component 3 on its lower bound (backward leaves the box)
        with m_1 ~ 5e-7 (forward is not buildable): refused on A's
        `derivative_undefined` path, and that stencil point is never
        solved."""
        run, fake, _, _ = fd_run(wall=0.9 + 5e-7)
        good = np.array([0.3, 0.5, 0.5, 0.1])
        run.train(good, gradient=False)
        u = np.array([0.9, 0.5, 0.5, 0.0])
        self.assertTrue(run.geometry(u)["buildable"])
        fun, jac, con, con_jac, geo, geo_jac, callback = driver.slsqp_callables(run)
        fun(u)
        n = len(trains(fake))
        with self.assertRaises(Refused) as cm:
            jac(u)
        self.assertEqual(cm.exception.kind, "derivative_undefined")
        # Components 0-2 were differenced; 3 was not solved either way.
        self.assertEqual(len(trains(fake)), n + 3)
        attempts = run.refusals[-1]["fd"]["attempts"]
        self.assertEqual(run.refusals[-1]["kind"], "derivative_undefined")
        self.assertEqual(run.refusals[-1]["fd"]["component"], "p3")
        self.assertEqual([a["unusable"] for a in attempts], ["not_buildable", "outside_box"])
        self.assertTrue(run.fd_gradients[-1]["refused"])
        # Nothing more is returned at the refused point.
        for f in (fun, con, jac, con_jac):
            with self.assertRaises(Refused):
                f(u)
        self.assertEqual(len(trains(fake)), n + 3)
        # Restoration starts from the last admissible point before it.
        np.testing.assert_allclose(run.anchor_u, good, rtol=0, atol=1e-15)
        y = run.restore(cm.exception, need_jacobian=True)
        self.assertNotEqual(run.problem.x_of(y).tobytes(), run.problem.x_of(u).tobytes())

    def test_refused_base_is_not_its_own_restoration_anchor(self):
        run, fake, _, _ = fd_run(wall=1.5)
        good = np.array([0.3, 0.5, 0.5, 0.3])
        run.train(good, gradient=False)
        # An admissible base whose components are all unusable both ways.
        u = np.array([0.5, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        np.testing.assert_allclose(run.anchor_u, u, rtol=0, atol=1e-15)
        with patch.object(driver.Run, "fd_step_unusable", return_value="would_remesh"):
            with self.assertRaises(Refused) as cm:
                driver.fd_gradient(run, u)
        np.testing.assert_allclose(run.anchor_u, good, rtol=0, atol=1e-15)
        y = run.restore(cm.exception, need_jacobian=True)
        self.assertNotEqual(run.problem.x_of(y).tobytes(), run.problem.x_of(u).tobytes())

    def test_a_stencil_reply_on_another_generation_is_a_wiring_error(self):
        run, fake, _, _ = fd_run()
        u = np.array([0.5, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        real = fake.request

        def request(op, timeout=None, **f):
            r = real(op, timeout=timeout, **f)
            if op == "train":
                r["generation"] = 7
            return r
        fake.request = request
        with self.assertRaises(WiringError):
            driver.fd_gradient(run, u)

    def test_a_base_on_a_retired_generation_is_resolved_on_the_current_one(self):
        run, fake, _, _ = fd_run()
        u = np.array([0.5, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        run._train[run.problem.x_of(u).tobytes()]["generation"] = 3
        driver.fd_gradient(run, u)
        rec = run.fd_gradients[-1]
        self.assertTrue(rec["base_resolved"])
        self.assertEqual(rec["base_generation"], 0)
        self.assertEqual(rec["solves"], N + 1)
        self.assertEqual(len(trains(fake)), N + 2)
        # A retired base that would remesh on the current reference: refused.
        run2, fake2, _, _ = fd_run()
        v = np.array([0.97, 0.5, 0.5, 0.5])
        run2.train(v, gradient=False)
        run2.train(u, gradient=False)  # the reference is back on generation 0
        with self.assertRaises(Refused) as cm:
            driver.fd_gradient(run2, v)
        self.assertEqual(cm.exception.kind, "derivative_undefined")
        self.assertEqual(run2.refusals[-1]["fd"], {"base": "would_remesh"})

    def test_f_refuses_wherever_a_refuses_before_any_solve(self):
        """[#1218 review] A/F parity at a buildable point whose design
        derivative is undefined (a zero slot-top discriminant): A's gradient
        request is refused before any solve, so F's value request must be
        too. Both refuse at `fun`, with zero train requests."""
        u = np.array([0.5, 0.5, 0.5, 0.5])
        for method in ("slsqp", "slsqp_fd"):
            with self.subTest(method=method):
                run, fake, led, _ = make_run(method=method)
                g = run.geometry(u)
                self.assertTrue(g["buildable"])
                g["jacobian_defined"] = False
                g["dh_du"] = None
                fun, jac, con, con_jac, geo, geo_jac, callback = driver.slsqp_callables(run)
                with self.assertRaises(Refused) as cm:
                    fun(u)
                self.assertEqual(cm.exception.kind, "derivative_undefined")
                self.assertEqual(trains(fake), [])
                self.assertEqual(led.totals()["forward"]["entries"], 0)
                self.assertEqual([r["kind"] for r in run.refusals], ["derivative_undefined"])
                for f in (fun, con, jac, con_jac, geo_jac):
                    with self.assertRaises(Refused):
                        f(u)
                self.assertEqual(trains(fake), [])
                self.assertIsNone(run.anchor_u)

    def test_a_not_buildable_point_is_refused_as_not_buildable_by_both(self):
        u = np.array([0.5, 0.5, 0.5, 0.5])
        for method in ("slsqp", "slsqp_fd"):
            with self.subTest(method=method):
                run, fake, _, _ = make_run(method=method)
                g = run.geometry(u)
                g.update(buildable=False, admissible=False, jacobian_defined=False, dh_du=None)
                fun = driver.slsqp_callables(run)[0]
                with self.assertRaises(Refused) as cm:
                    fun(u)
                self.assertEqual(cm.exception.kind, "not_buildable")
                self.assertEqual(trains(fake), [])

    def test_a_mesh_bad_forward_step_takes_the_backward_step(self):
        # The fake's `check` fires on a mesh-bad design (its remesh fails),
        # as `train` would remesh there: the stencil never solves it.
        run, fake, _, _ = fd_run(mesh_bad=lambda u: u[0] > 0.6)
        u = np.array([0.6 - 5e-7, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        try:
            g = driver.fd_gradient(run, u)
        except WiringError as error:  # a probe that passed a failing remesh
            self.fail(f"the check probe did not see the mesh-bad step: {error}")
        c = run.fd_gradients[-1]["components"][0]
        self.assertEqual(c["attempts"][0]["unusable"], "would_remesh")
        self.assertEqual(c["side"], "backward")
        self.assertTrue(all(from_bits(f["x_bits"])[0] <= 0.6 for f in trains(fake)))
        dF, _ = exact_gradients(run, u)
        np.testing.assert_allclose(g["dF"], dF, rtol=1e-4, atol=1e-12)

    def test_end_to_end_f_run_uses_values_only_and_records_its_gradients(self):
        run, fake, led, _ = fd_run(budget_fe=400.0)
        reason, msg, err = driver.execute(run, run.problem.u_of(X0))
        self.assertIn(reason, ("T99", "converged", "budget"), err)
        self.assertTrue(all(f["gradient"] is False for f in trains(fake)))
        self.assertEqual(led.totals()["adjoint"]["entries"], 0)
        res = run.result()
        self.assertEqual(res["method_spec"]["letter"], "F")
        self.assertEqual(res["fd_step"], FD_STEP)
        self.assertGreater(len(res["fd_gradients"]), 0)
        solves = sum(g["solves"] for g in res["fd_gradients"])
        stencil = sum(1 for e in res["events"] if e["kind"] == "fd_train")
        self.assertEqual(solves, stencil)
        optimizer = sum(1 for e in res["events"] if e["kind"] == "train")
        self.assertEqual(len(trains(fake)), optimizer + stencil)
        # Stencil points are never optimizer evaluations: no incumbent, no
        # validation, no restoration anchor.
        xs = {tuple(e["x"]) for e in res["events"] if e["kind"] == "fd_train"}
        self.assertFalse(xs & {tuple(v["x"]) for v in res["validations"]})
        self.assertNotIn("fd_gradients", make_run()[0].result())

    def test_a_is_unchanged_by_f(self):
        a, fake, led, _ = make_run(budget_fe=400.0)
        driver.execute(a, a.problem.u_of(X0))
        self.assertTrue(all(f["gradient"] is True for f in trains(fake)))
        self.assertFalse([op for op, _ in fake.requests if op == "check"])
        self.assertEqual(a.fd_gradients, [])


class AblationQueueTests(unittest.TestCase):
    def test_f_spec_is_a_with_forward_differences(self):
        a, f = driver.METHOD_SPECS["slsqp"], driver.METHOD_SPECS["slsqp_fd"]
        self.assertEqual(f["letter"], "F")
        self.assertEqual((f["optimizer"], f["resolution"]), (a["optimizer"], a["resolution"]))
        self.assertEqual(f["gradient"], "forward_difference")
        self.assertNotIn("gradient", a)
        self.assertEqual(driver.FD_ABLATION_METHODS, ("slsqp", "slsqp_fd"))

    def test_main_queue_and_specs_are_unchanged(self):
        self.assertNotIn("slsqp_fd", driver.MAIN_METHOD_SPECS)
        self.assertEqual(canonical(driver.MAIN_METHOD_SPECS), MAIN_SPECS_SHA256)
        jobs = main_schedule.registered_jobs(driver.MAIN_METHOD_SPECS)
        self.assertEqual(canonical(jobs), MAIN_JOBS_SHA256)
        self.assertEqual(session.registered_queue("main"), jobs)
        with self.assertRaises(ValueError):
            main_schedule.registered_jobs(driver.METHOD_SPECS)

    def test_ablation_queue_is_40_rotated_jobs(self):
        jobs = session.registered_queue("fd_ablation")
        self.assertEqual(jobs, fd_ablation_schedule.registered_jobs(
            {m: driver.METHOD_SPECS[m] for m in driver.FD_ABLATION_METHODS}))
        self.assertEqual(len(jobs), 40)
        self.assertEqual(len({j["name"] for j in jobs}), 40)
        self.assertEqual([j["position"] for j in jobs], list(range(40)))
        self.assertEqual({j["method"] for j in jobs}, {"slsqp", "slsqp_fd"})
        self.assertEqual({(j["method"], j["arm"]) for j in jobs},
                         {("slsqp", "A_prime"), ("slsqp_fd", "F")})
        for k, start in enumerate(main_schedule.STARTS):
            for space in main_schedule.SPACES:
                r1 = [j["method_letter"] for j in jobs
                      if (j["start"], j["space"], j["repeat"]) == (start, space, 1)]
                r2 = [j["method_letter"] for j in jobs
                      if (j["start"], j["space"], j["repeat"]) == (start, space, 2)]
                self.assertEqual(r1, ["A", "F"] if k % 2 == 0 else ["F", "A"])
                self.assertEqual(r2, r1[::-1])
        self.assertEqual(sorted({j["start"] for j in jobs}), sorted(main_schedule.STARTS))
        self.assertEqual({j["repeat"] for j in jobs}, {1, 2})

    def test_ablation_queue_refuses_other_comparisons(self):
        s = driver.METHOD_SPECS
        for specs in ({"slsqp": s["slsqp"]},
                      {"slsqp": s["slsqp"], "slsqp_fd": s["slsqp_fd"], "cobyqa": s["cobyqa"]},
                      {"slsqp_l0": dict(s["slsqp_l0"], letter="A"), "slsqp_fd": s["slsqp_fd"]},
                      {"slsqp": s["slsqp"], "slsqp_fd": dict(s["slsqp_fd"], gradient="adjoint")},
                      {"cobyqa": dict(s["cobyqa"], letter="A"), "slsqp_fd": s["slsqp_fd"]}):
            with self.subTest(specs=list(specs)), self.assertRaises(ValueError):
                fd_ablation_schedule.registered_jobs(specs)
        with self.assertRaises(ValueError):
            session.registered_queue("smoke")

    def test_fd_ablation_session_freezes_its_queue_with_main_setup(self):
        from test_session_journal import good_anchor, good_cal
        from session_journal import freeze, read_events
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            out = root / "session"
            meshes = root / "meshes"
            meshes.mkdir()
            binary = root / "eval"
            binary.write_bytes(b"fixture")
            for level in (0, 1, 2):
                (meshes / f"icem_initial_sector_L{level}.msh").write_bytes(b"fixture")
            (root / "anchor.json").write_text("{}")
            (root / "starts.json").write_text("{}")
            calls = {"preflight": 0, "anchor": 0}

            def preflight(binary, starts, meshes, path, **kwargs):
                calls["preflight"] += 1
                record = {"status": "PASS"}
                freeze(path, record)
                return record

            def anchor_preflight(binary, train, val, anchor, path, **kwargs):
                calls["anchor"] += 1
                record = good_anchor()
                freeze(path, record)
                return record

            def schedule(jobs, *a, **k):
                return [dict(job, job_name=job["name"], status="ran", exit=0,
                             host_contaminated=False, optimizer_completed=True,
                             diagnostic_status="not_requested", stop_reason="budget")
                        for job in jobs]
            with patch.object(session, "host_record", return_value={}), \
                 patch.object(session, "provenance",
                              return_value={"linear_algebra": session.linalg_fingerprint()}), \
                 patch("build_provenance.verify", return_value={"verified": True}) as build, \
                 patch.object(session, "MESHES", meshes), patch.object(session, "OPT", root), \
                 patch.object(session, "HERE", root), \
                 patch("recovery_bindings.capture",
                       return_value={"schema": 1, "source": {"tree_clean": True}}) as capture, \
                 patch("recovery_bindings.validate_setup"), \
                 patch("start_preflight.verify", side_effect=preflight), \
                 patch("anchor_preflight.verify", side_effect=anchor_preflight), \
                 patch.object(session, "gated_calibration",
                              side_effect=[good_cal() for _ in range(4)]) as cal, \
                 patch.object(session, "schedule", side_effect=schedule) as dispatch, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(session.main(["--fd-ablation", "--procs", "4", "--out", str(out),
                                               "--binary", str(binary)]), 0)
                self.assertEqual(cal.call_count, 1)  # no L0 model in the ablation
                build.assert_called_once()
                self.assertEqual(calls, {"preflight": 1, "anchor": 1})
                count = dispatch.call_count
                capture.return_value = {"schema": 1, "source": {"tree_clean": False}}
                with self.assertRaisesRegex(SystemExit, "clean, frozen"):
                    session.main(["--fd-ablation", "--procs", "4", "--out", str(root / "dirty"),
                                  "--binary", str(binary)])
                self.assertEqual(dispatch.call_count, count)
                with self.assertRaisesRegex(SystemExit, "120 FE"):
                    session.main(["--fd-ablation", "--procs", "4", "--out", str(root / "short"),
                                  "--binary", str(binary), "--budget-fe", "60"])
                with self.assertRaisesRegex(SystemExit, "smoke session only"), \
                        contextlib.redirect_stderr(io.StringIO()):
                    session.main(["--fd-ablation", "--procs", "4", "--out", str(root / "nogate"),
                                  "--binary", str(binary), "--host-gate", "off"])
            jobs = json.loads((out / "queue.json").read_text())["jobs"]
            self.assertEqual(jobs, session.registered_queue("fd_ablation"))
            config = json.loads((out / "session.json").read_text())
            self.assertEqual(config["label"], "fd_ablation")
            self.assertEqual(config["budget_fe"], 120.0)
            self.assertEqual(config["primary_model"], "L1")
            self.assertEqual(list(config["models"]), ["L1"])
            self.assertEqual(config["ablation"]["issue"], 1218)
            manifest = json.loads((out / "manifest.json").read_text())
            self.assertTrue(manifest["complete"])
            self.assertEqual(manifest["label"], "fd_ablation")
            self.assertEqual(set(manifest["method_specs"]), {"slsqp", "slsqp_fd"})
            self.assertIn("#1218", manifest["preregistration"])
            self.assertIn("post-hoc", manifest["preregistration"])
            events = read_events(out / "events.jsonl")["events"]
            self.assertEqual(events[-1]["kind"], "session_finished")

    def test_recovery_accepts_the_ablation_label_and_queue(self):
        import recovery
        import recovery_bindings
        with patch.object(recovery, "recover", return_value={"complete": True}) as rec, \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(recovery.main(["--source", "/nonexistent", "--fd-ablation"]), 0)
        self.assertEqual(rec.call_args.args[1], "fd_ablation")
        session_record = {"label": "fd_ablation"}
        dirty = {"schema": 1, "source": {"tree_clean": False}}
        with patch.object(recovery_bindings, "capture", return_value=dirty), \
                self.assertRaisesRegex(ValueError, "clean sources"):
            recovery_bindings.verify(dirty, session_record)


#: The frozen θ0 L1 training-mesh hash (`icem_initial_sector_L1_report.json`,
#: the main session's training mesh).
L1_TRAIN_SHA256 = "c4526a26904c73f68ba0987f0ca8995bcc0095f0e7936399d5920e167f35d7f5"


class FdGradientCheckLevelTests(unittest.TestCase):
    """`fd_gradient_check.py --level`: L0 (default) is the L0 CI model
    unchanged; L1 is the primary session's training and validation models
    with its registered anchor, every mesh hash-checked. No FEM."""

    def test_level_0_is_the_l0_ci_model_unchanged(self):
        import fd_gradient_check as c
        l0 = str(c.MESHES / "icem_initial_sector_L0.msh")
        expect = ["bin", "--space", "a", "--train-mesh", l0, "--val-mesh", l0,
                  "--train-level", "0", "--val-level", "0", "--mesh-out", "out"]
        self.assertEqual(c.argv_of("bin", "a", "out"), expect)
        self.assertEqual(c.argv_of("bin", "a", "out", "anc.json"),
                         expect + ["--anchor", "anc.json"])

    def test_level_1_is_the_primary_session_model(self):
        import fd_gradient_check as c
        anchor = session.OPT / "anchor.json"
        self.assertEqual(c.REGISTERED_ANCHOR, anchor)
        for space in ("a", "b"):
            # session.py --main / --fd-ablation: tl, vl = 1, 2 on L1 / L2.
            self.assertEqual(
                c.argv_of("bin", space, "out", anchor, level=1),
                session.evaluator_args("bin", space, session.MESHES / "icem_initial_sector_L1.msh",
                                       session.MESHES / "icem_initial_sector_L2.msh", 1, 2,
                                       anchor, "out"))
        self.assertEqual(c.FROZEN_SHA256["icem_initial_sector_L1.msh"], L1_TRAIN_SHA256)
        self.assertEqual(sorted(c.LEVELS), [0, 1])

    def test_level_option_defaults_to_0_and_reaches_the_check(self):
        import fd_gradient_check as c
        seen = []

        def fake_check(binary, space, out_dir, level=0, warm=False):
            self.assertIs(warm, False)
            seen.append((space, level))
            return {"space": space, "level": level, "status": "PASS"}
        with tempfile.TemporaryDirectory() as td:
            for argv, level in (([], 0), (["--level", "0"], 0), (["--level", "1"], 1)):
                seen.clear()
                out = Path(td) / f"rec{len(argv)}{level}.json"
                with patch.object(c, "check", fake_check), \
                        patch.object(c, "verify_meshes") as vm, \
                        patch.object(c, "sha256", return_value="0" * 64), \
                        contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(c.main(["--binary", "bin", "--out", str(out), *argv]), 0)
                self.assertEqual(seen, [("a", level), ("b", level)])
                vm.assert_called_once_with(level)
                rec = json.loads(out.read_text())
                if level == 0:
                    self.assertNotIn("level", rec)
                    self.assertIn("L0 CI model", rec["check"])
                else:
                    self.assertEqual(rec["level"], 1)
                    self.assertIn("L1", rec["check"])
            for bad in ("2", "-1"):
                with self.assertRaises(SystemExit), \
                        contextlib.redirect_stderr(io.StringIO()):
                    c.main(["--binary", "bin", "--out", str(Path(td) / "x.json"),
                            "--level", bad])

    def test_meshes_are_hash_checked_against_frozen_and_report(self):
        import fd_gradient_check as c
        with tempfile.TemporaryDirectory() as td:
            train, val = Path(td) / "t_L1.msh", Path(td) / "v_L2.msh"
            train.write_bytes(b"train")
            val.write_bytes(b"val")
            digest = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (train, val)}
            for p in (train, val):
                p.with_name(p.stem + "_report.json").write_text(
                    json.dumps({"msh_sha256": digest[p.name]}))
            levels = {1: {"train_mesh": train, "val_mesh": val, "val_level": 2}}
            with patch.object(c, "LEVELS", levels), patch.object(c, "FROZEN_SHA256", digest):
                c.verify_meshes(1)
                for p in (train, val):
                    with self.subTest(changed=p.name):
                        good = p.read_bytes()
                        p.write_bytes(good + b" ")
                        with self.assertRaisesRegex(SystemExit, "is not the frozen"):
                            c.verify_meshes(1)
                        p.write_bytes(good)
                        rep = p.with_name(p.stem + "_report.json")
                        rep.write_text(json.dumps({"msh_sha256": "0" * 64}))
                        with self.assertRaisesRegex(SystemExit, "is not the frozen"):
                            c.verify_meshes(1)
                        rep.write_text(json.dumps({"msh_sha256": digest[p.name]}))
                c.verify_meshes(1)
                val.unlink()
                with self.assertRaisesRegex(SystemExit, "missing"):
                    c.verify_meshes(1)


def warm_run(**kw):
    return make_run(method="slsqp_fd_warm", **kw)


def stripped(requests):
    """The requests with the F_w warm-start fields removed."""
    return [(op, {k: v for k, v in f.items() if k not in ("retain_fields", "warm_from_bits")})
            for op, f in requests]


def is_stencil(f):
    note = f.get("note") or ""
    return note.startswith("slsqp_fd ") and "base" not in note


class FdWarmTests(unittest.TestCase):
    """F_w `slsqp_fd_warm` (#1224 protocol §1): F with base-started
    stencil solves, on the fake evaluator (which models the retained
    bases)."""

    def test_w_spec_is_f_with_only_the_stencil_start_changed(self):
        f, w = driver.METHOD_SPECS["slsqp_fd"], driver.METHOD_SPECS["slsqp_fd_warm"]
        self.assertEqual(w["letter"], "W")
        self.assertEqual(w["fd_stencil_start"], "base")
        self.assertNotIn("fd_stencil_start", f)
        self.assertEqual({k: v for k, v in w.items()
                          if k not in ("letter", "label", "fd_stencil_start")},
                         {k: v for k, v in f.items() if k not in ("letter", "label")})
        self.assertIn("warm-started stencils", w["label"])
        self.assertEqual(driver.LETTERS["W"], "slsqp_fd_warm")
        self.assertEqual(driver.FD_WARM_METHODS, ("slsqp", "slsqp_fd_warm"))
        self.assertNotIn("slsqp_fd_warm", driver.MAIN_METHOD_SPECS)

    def test_w_requests_are_f_requests_plus_the_warm_fields(self):
        """The parity check: end to end on the fake, F_w sends exactly F's
        requests, in F's order, with the same charges and the same result;
        the only difference is `retain_fields` on every own training solve
        and `warm_from_bits` (the base's x, never another stencil point) on
        every stencil solve."""
        f, ffake, fled, _ = fd_run(budget_fe=400.0)
        w, wfake, wled, _ = warm_run(budget_fe=400.0)
        fr = driver.execute(f, f.problem.u_of(X0))
        wr = driver.execute(w, w.problem.u_of(X0))
        self.assertEqual(fr, wr)
        self.assertEqual(stripped(ffake.requests), stripped(wfake.requests))
        self.assertEqual(fled.totals(), wled.totals())
        for op, req in ffake.requests:
            self.assertNotIn("retain_fields", req)
            self.assertNotIn("warm_from_bits", req)
        stencils = 0
        for (op, req), (_, freq) in zip(wfake.requests, ffake.requests):
            if op != "train":
                self.assertEqual(req, freq)
                continue
            if is_stencil(req):
                stencils += 1
                self.assertNotIn("retain_fields", req)
                base = next(g for g in w.fd_gradients
                            if any(e["x"] == from_bits(req["x_bits"]) for e in w.events
                                   if e["kind"] == "fd_train"))
                self.assertIsNotNone(base)
                xs, xb = np.array(from_bits(req["x_bits"])), np.array(from_bits(
                    req["warm_from_bits"]))
                # One component away from the base, by the frozen step.
                self.assertEqual(np.count_nonzero(xs - xb), 1)
                self.assertAlmostEqual(abs(float(np.sum(xs - xb))), FD_STEP, delta=1e-12)
                self.assertIn(tuple(req["warm_from_bits"]), wfake.bases)
            else:
                self.assertIs(req.get("retain_fields"), True)
                self.assertNotIn("warm_from_bits", req)
        self.assertGreater(stencils, 0)
        fres, wres = f.result(), w.result()
        self.assertEqual(wres["method_spec"]["letter"], "W")
        self.assertEqual(len(fres["fd_gradients"]), len(wres["fd_gradients"]))
        for fg, wg in zip(fres["fd_gradients"], wres["fd_gradients"]):
            self.assertEqual(wg["stencil_start"], "base")
            self.assertEqual(fg["dF"], wg["dF"])
            self.assertEqual(fg["solves"], wg["solves"])
            for c in wg["components"]:
                if "side" in c:
                    self.assertEqual(c["start"], "same_mesh")
                    self.assertIsNone(c["start_reason"])
        self.assertEqual(fres["validations"], wres["validations"])
        warm = [e for e in wres["events"] if e["kind"] == "fd_train"]
        self.assertTrue(warm and all(e["warm"]["start"] == "same_mesh" for e in warm))
        self.assertFalse(any("warm" in e for e in fres["events"]))

    def test_every_stencil_starts_from_its_own_base_not_the_previous_stencil(self):
        run, fake, led, _ = warm_run()
        u = np.array([0.5, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        driver.fd_gradient(run, u)
        base_bits = driver.to_bits(run.problem.x_of(u))
        st = [f for op, f in fake.requests if op == "train" and is_stencil(f)]
        self.assertEqual(len(st), N)
        self.assertTrue(all(f["warm_from_bits"] == base_bits for f in st))
        # A second gradient at another point: its own base.
        v = np.array([0.4, 0.5, 0.5, 0.5])
        run.train(v, gradient=False)
        driver.fd_gradient(run, v)
        st2 = [f for op, f in fake.requests if op == "train" and is_stencil(f)][N:]
        self.assertTrue(all(f["warm_from_bits"] == driver.to_bits(run.problem.x_of(v))
                            for f in st2))
        # The charges are F's: N + 1 forward entries per gradient.
        self.assertEqual(led.totals()["forward"]["entries"], 2 * (N + 1))

    def test_a_retired_base_is_resolved_retained_and_used(self):
        run, fake, _, _ = warm_run()
        u = np.array([0.5, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        run._train[run.problem.x_of(u).tobytes()]["generation"] = 3
        driver.fd_gradient(run, u)
        rec = run.fd_gradients[-1]
        self.assertTrue(rec["base_resolved"])
        tr = trains(fake)
        self.assertEqual(len(tr), N + 2)
        # The re-solved base starts as F's (no warm start) and is retained.
        self.assertIs(tr[1].get("retain_fields"), True)
        self.assertNotIn("warm_from_bits", tr[1])
        self.assertIn("base", tr[1]["note"])
        self.assertTrue(all(f["warm_from_bits"] == tr[1]["x_bits"] for f in tr[2:]))
        self.assertTrue(all(c["start"] == "same_mesh" for c in rec["components"]))

    def test_an_evicted_base_falls_back_cold_and_is_recorded(self):
        run, fake, _, _ = warm_run()
        u = np.array([0.5, 0.5, 0.5, 0.5])
        run.train(u, gradient=False)
        fake.bases.clear()
        g = driver.fd_gradient(run, u)
        rec = run.fd_gradients[-1]
        self.assertEqual({c["start"] for c in rec["components"]}, {"cold"})
        self.assertEqual({c["start_reason"] for c in rec["components"]}, {"base_not_retained"})
        dF, _ = exact_gradients(run, u)
        np.testing.assert_allclose(g["dF"], dF, rtol=1e-4, atol=1e-12)

    def test_an_evaluator_that_ignores_the_warm_fields_is_a_wiring_error(self):
        for tamper in ("drop", "other_base", "not_retained", "unrequested"):
            with self.subTest(tamper=tamper):
                run, fake, _, _ = warm_run() if tamper != "unrequested" else fd_run()
                real = fake.request

                def request(op, timeout=None, _real=real, _t=tamper, **f):
                    r = _real(op, timeout=timeout, **f)
                    if op != "train":
                        return r
                    if _t == "drop":
                        r.pop("warm", None)
                    elif _t == "other_base" and "warm_from_bits" in f:
                        r["warm"]["from_x_bits"] = [0] * len(f["warm_from_bits"])
                    elif _t == "not_retained" and f.get("retain_fields"):
                        r["warm"]["retained"] = False
                    elif _t == "unrequested":
                        r["warm"] = {"retained": True, "generation": r["generation"]}
                    return r
                fake.request = request
                u = np.array([0.5, 0.5, 0.5, 0.5])
                with self.assertRaises(WiringError):
                    run.train(u, gradient=False)
                    driver.fd_gradient(run, u)

    def test_f_and_a_are_unchanged_by_f_w(self):
        for method in ("slsqp", "slsqp_fd"):
            with self.subTest(method=method):
                run, fake, _, _ = make_run(method=method, budget_fe=400.0)
                driver.execute(run, run.problem.u_of(X0))
                self.assertFalse(run.fd_warm)
                self.assertFalse(any("retain_fields" in f or "warm_from_bits" in f
                                     for _, f in fake.requests))
                self.assertEqual(fake.bases, {})


class FdWarmQueueTests(unittest.TestCase):
    """The #1224 {A', F_w} queue and session, and the pins of the queues
    F_w must not change."""

    def test_main_and_fd_ablation_queues_and_specs_are_pinned(self):
        self.assertEqual(canonical(driver.MAIN_METHOD_SPECS), MAIN_SPECS_SHA256)
        self.assertEqual(canonical(session.registered_queue("main")), MAIN_JOBS_SHA256)
        self.assertEqual(canonical(session.FD_ABLATION_SPECS), FD_ABLATION_SPECS_SHA256)
        self.assertEqual(canonical(session.registered_queue("fd_ablation")),
                         FD_ABLATION_JOBS_SHA256)
        self.assertEqual(driver.FD_ABLATION_METHODS, ("slsqp", "slsqp_fd"))
        self.assertNotIn("slsqp_fd_warm", session.FD_ABLATION_SPECS)

    def test_warm_queue_is_20_rotated_repeat1_jobs(self):
        jobs = session.registered_queue("fd_warm_ablation")
        self.assertEqual(jobs, fd_warm_schedule.registered_jobs(
            {m: driver.METHOD_SPECS[m] for m in driver.FD_WARM_METHODS}))
        self.assertEqual(len(jobs), 20)
        self.assertEqual(len({j["name"] for j in jobs}), 20)
        self.assertEqual([j["position"] for j in jobs], list(range(20)))
        self.assertEqual({j["repeat"] for j in jobs}, {1})
        self.assertEqual({(j["method"], j["arm"], j["method_letter"]) for j in jobs},
                         {("slsqp", "A_prime", "A"), ("slsqp_fd_warm", "F_w", "W")})
        f = session.registered_queue("fd_ablation")
        for k, start in enumerate(main_schedule.STARTS):
            for space in main_schedule.SPACES:
                got = [j["method_letter"] for j in jobs if (j["start"], j["space"]) == (start, space)]
                self.assertEqual(got, ["A", "W"] if k % 2 == 0 else ["W", "A"])
                # The rotation of fd_ablation_schedule's repeat 1, F -> W.
                r1 = [j["method_letter"] for j in f
                      if (j["start"], j["space"], j["repeat"]) == (start, space, 1)]
                self.assertEqual(got, ["W" if c == "F" else c for c in r1])
        # Space and start order is the #1218 repeat-1 order.
        self.assertEqual([(j["space"], j["start"]) for j in jobs],
                         [(j["space"], j["start"]) for j in f if j["repeat"] == 1])
        self.assertEqual(canonical(jobs), FD_WARM_JOBS_SHA256)

    def test_warm_queue_refuses_other_comparisons(self):
        s = driver.METHOD_SPECS
        for specs in ({"slsqp": s["slsqp"]},
                      {"slsqp": s["slsqp"], "slsqp_fd": s["slsqp_fd"]},
                      {"slsqp": s["slsqp"], "slsqp_fd": dict(s["slsqp_fd"], letter="W")},
                      {"slsqp": s["slsqp"],
                       "slsqp_fd_warm": dict(s["slsqp_fd_warm"], fd_stencil_start="cold")},
                      {"slsqp": dict(s["slsqp"], fd_stencil_start="base"),
                       "slsqp_fd_warm": s["slsqp_fd_warm"]},
                      {"slsqp": s["slsqp"],
                       "slsqp_fd_warm": dict(s["slsqp_fd_warm"], gradient="adjoint")},
                      {"slsqp_l0": dict(s["slsqp_l0"], letter="A"),
                       "slsqp_fd_warm": s["slsqp_fd_warm"]},
                      {"slsqp": s["slsqp"], "slsqp_fd_warm": s["slsqp_fd_warm"],
                       "slsqp_fd": s["slsqp_fd"]}):
            with self.subTest(specs=list(specs)), self.assertRaises(ValueError):
                fd_warm_schedule.registered_jobs(specs)
        for bad in (-1, 5, True, 1.0):
            with self.assertRaises(ValueError):
                fd_warm_schedule.method_order(bad)

    def test_fd_warm_session_freezes_its_queue_with_main_setup(self):
        from test_session_journal import good_anchor, good_cal
        from session_journal import freeze, read_events
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            out = root / "session"
            meshes = root / "meshes"
            meshes.mkdir()
            binary = root / "eval"
            binary.write_bytes(b"fixture")
            for level in (0, 1, 2):
                (meshes / f"icem_initial_sector_L{level}.msh").write_bytes(b"fixture")
            (root / "anchor.json").write_text("{}")
            (root / "starts.json").write_text("{}")
            calls = {"preflight": 0, "anchor": 0}

            def preflight(binary, starts, meshes, path, **kwargs):
                calls["preflight"] += 1
                record = {"status": "PASS"}
                freeze(path, record)
                return record

            def anchor_preflight(binary, train, val, anchor, path, **kwargs):
                calls["anchor"] += 1
                record = good_anchor()
                freeze(path, record)
                return record

            def schedule(jobs, *a, **k):
                return [dict(job, job_name=job["name"], status="ran", exit=0,
                             host_contaminated=False, optimizer_completed=True,
                             diagnostic_status="not_requested", stop_reason="budget")
                        for job in jobs]
            with patch.object(session, "host_record", return_value={}), \
                 patch.object(session, "provenance",
                              return_value={"linear_algebra": session.linalg_fingerprint()}), \
                 patch("build_provenance.verify", return_value={"verified": True}) as build, \
                 patch.object(session, "MESHES", meshes), patch.object(session, "OPT", root), \
                 patch.object(session, "HERE", root), \
                 patch("recovery_bindings.capture",
                       return_value={"schema": 1, "source": {"tree_clean": True}}) as capture, \
                 patch("recovery_bindings.validate_setup"), \
                 patch("start_preflight.verify", side_effect=preflight), \
                 patch("anchor_preflight.verify", side_effect=anchor_preflight), \
                 patch.object(session, "gated_calibration",
                              side_effect=[good_cal() for _ in range(4)]) as cal, \
                 patch.object(session, "schedule", side_effect=schedule) as dispatch, \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(session.main(["--fd-warm-ablation", "--procs", "15", "--out",
                                               str(out), "--binary", str(binary)]), 0)
                self.assertEqual(cal.call_count, 1)  # no L0 model
                build.assert_called_once()
                self.assertEqual(calls, {"preflight": 1, "anchor": 1})
                count = dispatch.call_count
                self.assertEqual(dispatch.call_args.args[0],
                                 session.registered_queue("fd_warm_ablation"))
                capture.return_value = {"schema": 1, "source": {"tree_clean": False}}
                with self.assertRaisesRegex(SystemExit, "clean, frozen"):
                    session.main(["--fd-warm-ablation", "--procs", "4", "--out",
                                  str(root / "dirty"), "--binary", str(binary)])
                self.assertEqual(dispatch.call_count, count)
                with self.assertRaisesRegex(SystemExit, "120 FE"):
                    session.main(["--fd-warm-ablation", "--procs", "4", "--out",
                                  str(root / "short"), "--binary", str(binary),
                                  "--budget-fe", "60"])
                with self.assertRaisesRegex(SystemExit, "smoke session only"), \
                        contextlib.redirect_stderr(io.StringIO()):
                    session.main(["--fd-warm-ablation", "--procs", "4", "--out",
                                  str(root / "nogate"), "--binary", str(binary),
                                  "--host-gate", "off"])
                with self.assertRaises(SystemExit), contextlib.redirect_stderr(io.StringIO()):
                    session.main(["--fd-warm-ablation", "--fd-ablation", "--procs", "4",
                                  "--out", str(root / "both"), "--binary", str(binary)])
            jobs = json.loads((out / "queue.json").read_text())["jobs"]
            self.assertEqual(jobs, session.registered_queue("fd_warm_ablation"))
            config = json.loads((out / "session.json").read_text())
            self.assertEqual(config["label"], "fd_warm_ablation")
            self.assertEqual(config["budget_fe"], 120.0)
            self.assertEqual(config["primary_model"], "L1")
            self.assertEqual(list(config["models"]), ["L1"])
            self.assertEqual(config["procs"], 15)
            self.assertEqual(config["ablation"],
                             {"issue": 1224, "methods": ["slsqp", "slsqp_fd_warm"],
                              "arms": {"slsqp": "A_prime", "slsqp_fd_warm": "F_w"},
                              "repeats": [1], "post_hoc": True})
            manifest = json.loads((out / "manifest.json").read_text())
            self.assertTrue(manifest["complete"])
            self.assertEqual(manifest["label"], "fd_warm_ablation")
            self.assertEqual(manifest["method_specs"], session.FD_WARM_SPECS)
            self.assertIn("#1224", manifest["preregistration"])
            self.assertIn("post-hoc", manifest["preregistration"])
            events = read_events(out / "events.jsonl")["events"]
            self.assertEqual(events[-1]["kind"], "session_finished")

    def test_recovery_accepts_the_warm_label_and_queue(self):
        import recovery
        import recovery_bindings
        with patch.object(recovery, "recover", return_value={"complete": True}) as rec, \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(recovery.main(["--source", "/nonexistent", "--fd-warm-ablation"]),
                             0)
        self.assertEqual(rec.call_args.args[1], "fd_warm_ablation")
        dirty = {"schema": 1, "source": {"tree_clean": False}}
        with patch.object(recovery_bindings, "capture", return_value=dirty), \
                self.assertRaisesRegex(ValueError, "clean sources"):
            recovery_bindings.verify(dirty, {"label": "fd_warm_ablation"})


class FdWarmCheckTests(unittest.TestCase):
    """`fd_gradient_check.py --warm` and `fd_step_sweep.py --warm` (#1224
    protocol §2) on the fake: F_w's stencils, F's cold reference, the costs
    and the stop gate. No FEM."""

    def test_check_main_passes_warm_and_labels_the_record(self):
        import fd_gradient_check as c
        seen = []

        def fake_check(binary, space, out_dir, level=0, warm=False):
            seen.append((space, level, warm))
            return {"space": space, "level": level, "status": "PASS", "gate": "proceed"}
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "rec-1224.json"
            with patch.object(c, "check", fake_check), patch.object(c, "verify_meshes"), \
                    patch.object(c, "sha256", return_value="0" * 64), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(c.main(["--binary", "bin", "--out", str(out), "--level", "1",
                                         "--warm"]), 0)
            self.assertEqual(seen, [("a", 1, True), ("b", 1, True)])
            rec = json.loads(out.read_text())
            self.assertEqual((rec["issue"], rec["level"], rec["method"]),
                             (1224, 1, "slsqp_fd_warm"))
            self.assertEqual((rec["gate"], rec["gate_factor"]), ("proceed", 10.0))
            self.assertIn("F_w", rec["check"])
            self.assertEqual((rec["norm_tol"], rec["component_tol"]),
                             (L0_NORM_TOL, L0_COMPONENT_TOL))
            self.assertIn("crates/geode-core/tests/common/icem_remesh.rs",
                          rec["source_sha256"])

    def test_warm_reference_on_the_fake(self):
        """The record of one point: every F_w stencil starts from the base;
        F's cold reference is computed on the same evaluator at the same
        point with the same sides; the costs come from the stencil replies;
        and the gate compares with F's committed record."""
        import fd_gradient_check as c
        run, fake, led, _ = warm_run()
        seen = c.Recorder(fake)
        run.ev = seen
        u = np.array([0.5, 0.5, 0.5, 0.5])
        base = run.train(u, gradient=False, note="fd check base")
        g = driver.fd_gradient(run, u)
        adj = run.train(u, gradient=True, note="fd check adjoint")
        rel_f, norm_f = c.compare(g["dF"], adj["dF"])
        rel_m, norm_m = c.compare(g["dmean_full"], adj["dmean_full"])
        fd_rec = run.fd_gradients[-1]
        with patch.object(c, "committed_normwise",
                          return_value={"normwise_rel_dF": norm_f / 2,
                                        "normwise_rel_dmean_full": max(norm_m, 1e-300)}), \
                patch.object(c.Anchor, "from_file", return_value=run.anchor):
            out = c.warm_reference(fake, "anchor.json", run.problem, u,
                                   {"base": base, "gradient": g, "record": fd_rec,
                                    "norm_f": norm_f, "norm_m": norm_m,
                                    "trains": seen.trains}, adj, 1, "a", "interior")
        cr = out["cold_reference"]
        self.assertTrue(cr["base"]["same_as_warm_base"])
        self.assertEqual([r["side"] for r in cr["components"]],
                         [r["side"] for r in fd_rec["components"]])
        # The fake's values do not depend on the start.
        self.assertEqual(cr["normwise_warm_minus_cold_dF"], 0.0)
        self.assertEqual(out["cost"]["warm_stencil"]["solves"], N)
        self.assertEqual(out["cost"]["warm_stencil"]["starts"], ["same_mesh"] * N)
        self.assertEqual(out["cost"]["cold_stencil"]["starts"], ["cold"] * N)
        self.assertEqual(out["cost"]["warm_over_cold_newton_iterations"], 1.0)
        self.assertAlmostEqual(out["gate"]["ratio_dF"], 2.0)
        self.assertEqual(out["gate"]["verdict"], "proceed")
        cold_stencils = [f for op, f in fake.requests if op == "train" and is_stencil(f)
                         and "warm_from_bits" not in f]
        self.assertEqual(len(cold_stencils), N)
        with patch.object(c, "committed_normwise",
                          return_value={"normwise_rel_dF": norm_f / 11,
                                        "normwise_rel_dmean_full": max(norm_m, 1e-300)}), \
                patch.object(c.Anchor, "from_file", return_value=run.anchor):
            stop = c.warm_reference(fake, "anchor.json", run.problem, u,
                                    {"base": base, "gradient": g, "record": fd_rec,
                                     "norm_f": norm_f, "norm_m": norm_m,
                                     "trains": seen.trains}, adj, 1, "a", "interior")
        self.assertEqual(stop["gate"]["verdict"], "stop")
        # A cold F_w stencil is refused by the check (not the protocol).
        fd_rec["components"][0]["start"] = "cold"
        with self.assertRaises(WiringError), \
                patch.object(c.Anchor, "from_file", return_value=run.anchor):
            c.warm_reference(fake, "anchor.json", run.problem, u,
                             {"base": base, "gradient": g, "record": fd_rec, "norm_f": norm_f,
                              "norm_m": norm_m, "trains": seen.trains}, adj, 1, "a", "interior")

    def test_sweep_warm_starts_every_stencil_from_the_base(self):
        import fd_step_sweep as s
        run, fake, led, clock = warm_run()
        rec = s.sweep(run, CENTRE_U + 0.1, ("p0", "p1"))
        tr = trains(fake)
        base_bits = tr[0]["x_bits"]
        self.assertIs(tr[0].get("retain_fields"), True)
        st = [f for f in tr if (f.get("note") or "").startswith("fd sweep")]
        self.assertEqual(len(st), 5 * 2)
        self.assertTrue(all(f["warm_from_bits"] == base_bits for f in st))
        # The base repeat is cold, as in F.
        self.assertNotIn("warm_from_bits", tr[-1])
        for comp in rec["components"]:
            for row in comp["stencil"]:
                self.assertEqual(row["start"], "same_mesh")
        f_run, f_fake, _, _ = fd_run()
        f_rec = s.sweep(f_run, CENTRE_U + 0.1, ("p0", "p1"))
        self.assertEqual(stripped(f_fake.requests), stripped(fake.requests))
        for a, b in zip(f_rec["components"], rec["components"]):
            self.assertEqual(a["schemes"], b["schemes"])
            self.assertTrue(all("start" not in row for row in a["stencil"]))

    def test_sweep_main_passes_warm(self):
        import fd_step_sweep as s
        seen = []

        def fake_sweep(binary, level, space, point, components, out_dir, warm=False):
            seen.append(warm)
            return {"components": []}
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "rec-1224-sweep.json"
            with patch.object(s, "run_sweep", fake_sweep), \
                    patch.object(s.fdc, "verify_meshes"), \
                    patch.object(s, "sha256", return_value="0" * 64), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(s.main(["--binary", "bin", "--out", str(out), "--warm"]), 0)
            self.assertEqual(seen, [True])
            rec = json.loads(out.read_text())
            self.assertEqual((rec["issue"], rec["method"]), (1224, "slsqp_fd_warm"))


def evaluator_binary():
    path = os.environ.get("MOTOR_HARNESS_EVALUATOR")
    if path:
        return Path(path)
    root = Path(os.environ.get("CARGO_TARGET_DIR", HERE.parents[2] / "target"))
    return root / "release/examples/icem_harness_eval"


@unittest.skipUnless(evaluator_binary().exists(), "the evaluator binary is not built")
class L0AdjointTests(unittest.TestCase):
    """[#1218] F's forward-difference gradient (`driver.fd_gradient`,
    through the real evaluator and protocol) against the adjoint reply on
    the L0 CI model, at an interior point of each space."""

    def test_fd_gradient_matches_the_adjoint_on_l0(self):
        import fd_gradient_check
        with tempfile.TemporaryDirectory() as td:
            for space in ("a", "b"):
                with self.subTest(space=space):
                    rec = fd_gradient_check.check(evaluator_binary(), space, Path(td) / space)
                    self.assertEqual(rec["status"], "PASS", json.dumps(rec, indent=1))


class L0RecordTests(unittest.TestCase):
    """The committed L0 runtime record of `fd_gradient_check.py`."""

    def test_committed_l0_record_passes(self):
        data = json.loads((HERE / "fd-gradient-runtime-checks.json").read_text())
        self.assertEqual(data["issue"], 1218)
        self.assertEqual(data["status"], "PASS")
        self.assertEqual((data["norm_tol"], data["component_tol"]),
                         (L0_NORM_TOL, L0_COMPONENT_TOL))
        self.assertEqual(sorted(r["space"] for r in data["records"]), ["a", "b"])
        sides = set()
        for rec in data["records"]:
            self.assertEqual(rec["level"], 0)
            self.assertEqual(rec["step"], FD_STEP)
            self.assertEqual(rec["charged"]["adjoint"]["entries"], 2)  # the two references only
            self.assertEqual([p["point"] for p in rec["points"]], ["theta0", "interior"])
            for p in rec["points"]:
                self.assertFalse(p["fd"]["refused"])
                self.assertEqual(p["fd"]["solves"], len(p["components"]))
                self.assertLessEqual(p["normwise_rel_dF"], L0_NORM_TOL)
                self.assertLessEqual(p["normwise_rel_dmean_full"], L0_NORM_TOL)
                for row in p["components"]:
                    sides.add(row["side"])
                    self.assertAlmostEqual(abs(row["h"]), FD_STEP, delta=1e-12)
                    for key in ("rel_dF", "rel_dmean_full"):
                        if row[key] is not None:
                            self.assertLessEqual(row[key], L0_COMPONENT_TOL, row)
        # Space B's bound-active θ0 offsets exercise the backward step.
        self.assertEqual(sides, {"forward", "backward"})


class L1RecordTests(unittest.TestCase):
    """The committed L1 runtime record of `fd_gradient_check.py --level 1`
    (the primary session's training model): same step and criteria as L0.
    Its status is whatever the criteria give; this checks the record is
    consistent with them, not that it passes."""

    def test_frozen_mesh_hashes_are_the_committed_reports(self):
        import fd_gradient_check as c
        for name, digest in c.FROZEN_SHA256.items():
            report = c.MESHES / name.replace(".msh", "_report.json")
            self.assertEqual(json.loads(report.read_text())["msh_sha256"], digest, name)

    def test_committed_l1_record_is_consistent(self):
        import fd_gradient_check as c
        data = json.loads((HERE / "fd-gradient-runtime-checks-L1.json").read_text())
        self.assertEqual((data["issue"], data["level"]), (1218, 1))
        self.assertEqual((data["norm_tol"], data["component_tol"], data["negligible"]),
                         (L0_NORM_TOL, L0_COMPONENT_TOL, c.NEGLIGIBLE))
        self.assertEqual(sorted(r["space"] for r in data["records"]), ["a", "b"])
        anchor = HERE.parent / "anchor.json"
        for rec in data["records"]:
            self.assertEqual((rec["level"], rec["val_level"]), (1, 2))
            self.assertEqual(rec["step"], FD_STEP)
            self.assertEqual(rec["train_msh_sha256"], L1_TRAIN_SHA256)
            self.assertEqual(rec["mesh_sha256"], L1_TRAIN_SHA256)
            self.assertEqual(rec["val_mesh_sha256"],
                             c.FROZEN_SHA256["icem_initial_sector_L2.msh"])
            self.assertEqual(rec["anchor"]["sha256"],
                             hashlib.sha256(anchor.read_bytes()).hexdigest())
            self.assertEqual(rec["charged"]["adjoint"]["entries"], 2)
            self.assertEqual([p["point"] for p in rec["points"]], ["theta0", "interior"])
            for p in rec["points"]:
                self.assertFalse(p["fd"]["refused"])
                self.assertEqual(p["fd"]["solves"], len(p["components"]))
                for row in p["components"]:
                    self.assertAlmostEqual(abs(row["h"]), FD_STEP, delta=1e-12)
                ok = (p["normwise_rel_dF"] <= L0_NORM_TOL
                      and p["normwise_rel_dmean_full"] <= L0_NORM_TOL
                      and all(v is None or v <= L0_COMPONENT_TOL for row in p["components"]
                              for v in (row["rel_dF"], row["rel_dmean_full"])))
                self.assertEqual(p["pass"], ok)
            self.assertEqual(rec["status"],
                             "PASS" if all(p["pass"] for p in rec["points"]) else "FAIL")
        self.assertEqual(data["status"], "PASS" if all(r["status"] == "PASS"
                                                       for r in data["records"]) else "FAIL")


class FdStepSweepTests(unittest.TestCase):
    """`fd_step_sweep.py` (#1218 diagnostic) on the fake's analytic model:
    F_train is quadratic in x, so the forward error is exactly
    h F''/2 = h 0.9 F0 scale / width_k^2 and the central error is roundoff;
    the mean is linear (exact for both). No FEM."""

    def sweep(self, u=None, components=("p0", "p1", "p2")):
        import fd_step_sweep as s
        run, fake, led, clock = fd_run()
        u = CENTRE_U + 0.1 if u is None else u
        return s.sweep(run, u, components), run, fake

    def test_defaults_are_the_requested_diagnostic(self):
        import fd_step_sweep as s
        self.assertEqual(s.FORWARD_STEPS, (1e-5, 1e-6, 1e-7))
        self.assertEqual(s.CENTRAL_STEPS, (1e-5, 1e-6))
        self.assertEqual(s.COMPONENTS, ("SW3", "MH", "SW2"))
        self.assertEqual(FD_STEP, 1e-6)  # the frozen protocol step is untouched
        sp = {"x0": [1.0, 2.0], "lower": [0.0, 0.0], "upper": [4.0, 2.0]}
        np.testing.assert_array_equal(s.point_x(sp, "theta0"), [1.0, 2.0])
        # fd_gradient_check's interior point: x0 + 0.25 (centre - x0).
        np.testing.assert_array_equal(s.point_x(sp, "interior"), [1.25, 1.75])

    def test_forward_error_is_the_truncation_and_central_removes_it(self):
        rec, run, fake = self.sweep()
        u = run.problem.u_of(np.array(rec["x"]))
        exact_dF, exact_dm = exact_gradients(run, u)
        for comp in rec["components"]:
            k = comp["component"]
            curv = 0.9 * F0 * fake.scale / fake.width[k] ** 2  # F''/2
            got = {(r["scheme"], r["step"]): r for r in comp["schemes"]}
            self.assertEqual(sorted(got), [("central", 1e-6), ("central", 1e-5),
                                           ("forward", 1e-7), ("forward", 1e-6),
                                           ("forward", 1e-5)])
            for (scheme, h), r in got.items():
                self.assertTrue(r["usable"])
                self.assertAlmostEqual(r["dF"]["adjoint"], exact_dF[k], places=15)
                self.assertAlmostEqual(r["dx"], h if scheme == "forward" else 2 * h,
                                       delta=1e-15)
                if scheme == "forward":
                    # (the fake's own roundoff floor shows at 1e-7)
                    self.assertAlmostEqual(r["dF"]["signed_err"] / (curv * r["dx"]), 1.0,
                                           delta={1e-5: 1e-4, 1e-6: 1e-3, 1e-7: 5e-2}[h])
                else:
                    self.assertLess(r["dF"]["abs_err"], 1e-3 * curv * h)
                self.assertLess(abs(r["dmean_full"]["signed_err"]), 1e-8)
                self.assertAlmostEqual(r["dmean_full"]["adjoint"], exact_dm[k], places=15)

    def test_every_stencil_value_is_a_charged_value_solve_on_the_base_generation(self):
        rec, run, fake = self.sweep()
        tr = trains(fake)
        # base, 5 distinct stencil points per component, the adjoint, the repeat
        self.assertEqual(len(tr), 1 + 5 * 3 + 1 + 1)
        self.assertEqual([bool(f.get("gradient")) for f in tr],
                         [False] * 16 + [True, False])
        self.assertEqual(rec["solves"], 1 + 5 * 3)
        self.assertEqual(sum(1 for op, _ in fake.requests if op == "check"), 5 * 3)
        x = np.array(rec["x"])
        self.assertEqual(from_bits(tr[-1]["x_bits"]), x.tolist())
        self.assertEqual(rec["base_repeat"]["generation"], rec["generation"])
        self.assertEqual(rec["base_repeat"]["dF_abs"], 0.0)
        for comp in rec["components"]:
            self.assertEqual(len(comp["stencil"]), 5)
            for st in comp["stencil"]:
                self.assertIsNone(st["unusable"])
                sign = 1.0 if st["side"] == "+" else -1.0
                self.assertAlmostEqual(st["dx"], sign * st["step"], delta=1e-15)

    def test_a_stencil_point_that_would_remesh_is_never_solved(self):
        u = np.array(CENTRE_U, dtype=float)
        u[0] = 0.95 - 5e-6  # +1e-5 in p0 (width 1) crosses the fake's remesh rule
        rec, run, fake = self.sweep(u=u, components=("p0",))
        for op, f in fake.requests:
            if op == "train":
                self.assertLessEqual(from_bits(f["x_bits"])[0], 0.95)
        rows = {(r["scheme"], r["step"]): r for r in rec["components"][0]["schemes"]}
        self.assertFalse(rows[("forward", 1e-5)]["usable"])
        self.assertIsNone(rows[("forward", 1e-5)]["dF"]["fd"])
        self.assertFalse(rows[("central", 1e-5)]["usable"])
        self.assertTrue(rows[("forward", 1e-6)]["usable"])
        self.assertTrue(rows[("central", 1e-6)]["usable"])
        bad = [st for st in rec["components"][0]["stencil"] if st["unusable"]]
        self.assertEqual([(st["side"], st["step"], st["unusable"]) for st in bad],
                         [("+", 1e-5, "would_remesh")])

    def test_main_defaults_to_the_failing_l1_point_and_verifies_meshes(self):
        import fd_step_sweep as s
        seen = []

        def fake_sweep(binary, level, space, point, components, out_dir):
            seen.append((level, space, point, components))
            return {"components": []}
        with tempfile.TemporaryDirectory() as td:
            out = Path(td) / "rec-1218-sweep.json"
            with patch.object(s, "run_sweep", fake_sweep), \
                    patch.object(s.fdc, "verify_meshes") as vm, \
                    patch.object(s, "sha256", return_value="0" * 64), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(s.main(["--binary", "bin", "--out", str(out)]), 0)
            vm.assert_called_once_with(1)
            self.assertEqual(seen, [(1, "a", "interior", ("SW3", "MH", "SW2"))])
            rec = json.loads(out.read_text())
            self.assertEqual((rec["level"], rec["protocol_step"]), (1, FD_STEP))
            self.assertIn("fd_step_sweep.py", rec["source_sha256"])
            with self.assertRaisesRegex(SystemExit, "refusing to overwrite"):
                s.main(["--binary", "bin", "--out", str(out)])


class FdStepSweepRecordTests(unittest.TestCase):
    """The committed L1 record of `fd_step_sweep.py` (#1218 diagnostic)."""

    def test_committed_l1_sweep_record_is_consistent(self):
        import fd_gradient_check as c
        import fd_step_sweep as s
        data = json.loads((HERE / "fd-step-sweep-L1.json").read_text())
        self.assertEqual((data["issue"], data["level"], data["protocol_step"]),
                         (1218, 1, FD_STEP))
        self.assertEqual(data["forward_steps"], list(s.FORWARD_STEPS))
        self.assertEqual(data["central_steps"], list(s.CENTRAL_STEPS))
        self.assertEqual(data["components"], list(s.COMPONENTS))
        rec = data["record"]
        self.assertEqual((rec["space"], rec["point"], rec["level"], rec["val_level"]),
                         ("a", "interior", 1, 2))
        self.assertEqual(rec["train_msh_sha256"], L1_TRAIN_SHA256)
        self.assertEqual(rec["mesh_sha256"], L1_TRAIN_SHA256)
        self.assertEqual(rec["val_mesh_sha256"], c.FROZEN_SHA256["icem_initial_sector_L2.msh"])
        self.assertEqual(rec["anchor"]["sha256"], hashlib.sha256(
            (HERE.parent / "anchor.json").read_bytes()).hexdigest())
        self.assertEqual(rec["base_repeat"]["generation"], rec["generation"])
        self.assertEqual(rec["charged"]["adjoint"]["entries"], 1)
        self.assertEqual(rec["charged"]["remesh"]["entries"], 0)
        self.assertEqual(rec["solves"], 1 + 5 * len(s.COMPONENTS))
        self.assertEqual(rec["charged"]["forward"]["entries"], rec["solves"] + 2)
        for comp in rec["components"]:
            self.assertEqual(len(comp["schemes"]), 5)
            for r in comp["schemes"]:
                self.assertTrue(r["usable"])
                for dq in ("dF", "dmean_full"):
                    e = r[dq]
                    self.assertAlmostEqual(e["signed_err"], e["fd"] - e["adjoint"], delta=1e-15)
        # The sweep's forward 1e-6 values are the L1 check's (same point).
        self.assertEqual(data["reproduction"]["file"], "benchmarks/motor_optimization/"
                         "harness/fd-gradient-runtime-checks-L1.json")
        self.assertEqual(len(data["reproduction"]["components"]), len(s.COMPONENTS))


class FdWarmRecordTests(unittest.TestCase):
    """The committed L1 records of F_w (#1224 protocol §2):
    `fd_gradient_check.py --level 1 --warm` and `fd_step_sweep.py --warm`.
    Their status is whatever the frozen criteria give; this checks that each
    record is consistent with those criteria and with F's records, not that
    it passes."""

    def test_committed_normwise_reads_fs_record(self):
        import fd_gradient_check as c
        prior = c.committed_normwise(1, "a", "interior")
        data = json.loads((HERE / "fd-gradient-runtime-checks-L1.json").read_text())
        p = next(q for q in next(r for r in data["records"] if r["space"] == "a")["points"]
                 if q["point"] == "interior")
        self.assertEqual(prior["normwise_rel_dF"], p["normwise_rel_dF"])
        self.assertEqual(prior["normwise_rel_dmean_full"], p["normwise_rel_dmean_full"])
        self.assertEqual(c.GATE_FACTOR, 10.0)

    def test_final_refinement_binds_the_same_arm(self):
        """The selection side (final_refinement) states the same session
        block and method specs as the harness writes."""
        import ast
        fr = HERE.parent / "final_refinement"

        def literal(path, name):
            tree = ast.parse((fr / path).read_text())
            node = next(n for n in tree.body if isinstance(n, ast.Assign)
                        and any(getattr(t, "id", None) == name for t in n.targets))
            return ast.literal_eval(node.value)
        self.assertEqual(literal("setup_evidence.py", "WARM_ABLATION"),
                         {"issue": 1224, "methods": list(driver.FD_WARM_METHODS),
                          "arms": {"slsqp": "A_prime", "slsqp_fd_warm": "F_w"},
                          "repeats": [1], "post_hoc": True})
        specs = literal("ablation_selection.py", "WARM_METHOD_SPECS")
        self.assertEqual(specs, {m: {k: v for k, v in driver.METHOD_SPECS[m].items()
                                     if k != "label"} for m in driver.FD_WARM_METHODS})
        self.assertEqual(fd_warm_schedule.registered_jobs(specs),
                         session.registered_queue("fd_warm_ablation"))

    def test_committed_l1_warm_record_is_consistent(self):
        import fd_gradient_check as c
        data = json.loads((HERE / "fd-gradient-runtime-checks-L1-warm.json").read_text())
        f = json.loads((HERE / "fd-gradient-runtime-checks-L1.json").read_text())
        self.assertEqual((data["issue"], data["level"], data["method"], data["stencil_start"]),
                         (1224, 1, "slsqp_fd_warm", "base"))
        self.assertEqual((data["norm_tol"], data["component_tol"], data["negligible"]),
                         (L0_NORM_TOL, L0_COMPONENT_TOL, c.NEGLIGIBLE))
        self.assertEqual(data["gate_factor"], 10.0)
        anchor = HERE.parent / "anchor.json"
        gates = []
        for rec, frec in zip(data["records"], f["records"]):
            self.assertEqual(rec["space"], frec["space"])
            self.assertEqual((rec["level"], rec["val_level"], rec["step"]), (1, 2, FD_STEP))
            self.assertEqual(rec["train_msh_sha256"], L1_TRAIN_SHA256)
            self.assertEqual(rec["val_mesh_sha256"], c.FROZEN_SHA256["icem_initial_sector_L2.msh"])
            self.assertEqual(rec["anchor"]["sha256"],
                             hashlib.sha256(anchor.read_bytes()).hexdigest())
            self.assertEqual(rec["charged"]["adjoint"]["entries"], 2)
            self.assertEqual(rec["charged"]["remesh"]["entries"], 0)
            self.assertEqual([p["point"] for p in rec["points"]], ["theta0", "interior"])
            for p, fp in zip(rec["points"], frec["points"]):
                self.assertEqual(p["x"], fp["x"])
                # The same base value as F's record (a cold solve).
                self.assertEqual((p["F"], p["mean_full"]), (fp["F"], fp["mean_full"]))
                self.assertFalse(p["fd"]["refused"])
                self.assertEqual(p["fd"]["solves"], len(p["components"]))
                self.assertEqual([r["side"] for r in p["components"]],
                                 [r["side"] for r in fp["components"]])
                ok = (p["normwise_rel_dF"] <= L0_NORM_TOL
                      and p["normwise_rel_dmean_full"] <= L0_NORM_TOL
                      and all(v is None or v <= L0_COMPONENT_TOL for row in p["components"]
                              for v in (row["rel_dF"], row["rel_dmean_full"])))
                self.assertEqual(p["pass"], ok)
                # Every stencil solve started from the base on its mesh.
                cost = p["cost"]
                self.assertEqual(cost["warm_stencil"]["starts"],
                                 ["same_mesh"] * len(p["components"]))
                self.assertEqual(cost["cold_stencil"]["starts"], ["cold"] * len(p["components"]))
                self.assertAlmostEqual(cost["warm_over_cold_newton_iterations"],
                                       cost["warm_stencil"]["total_newton_iterations"]
                                       / cost["cold_stencil"]["total_newton_iterations"])
                # F's cold reference in the same process reproduces F's
                # committed record bit for bit.
                ref = p["cold_reference"]
                self.assertTrue(ref["base"]["same_as_warm_base"])
                self.assertEqual([(r["fd_dF"], r["fd_dmean_full"]) for r in ref["components"]],
                                 [(r["fd_dF"], r["fd_dmean_full"]) for r in fp["components"]])
                self.assertEqual((ref["normwise_rel_dF"], ref["normwise_rel_dmean_full"]),
                                 (fp["normwise_rel_dF"], fp["normwise_rel_dmean_full"]))
                g = p["gate"]
                self.assertEqual(g["committed_f"]["normwise_rel_dF"], fp["normwise_rel_dF"])
                ratio = max(p["normwise_rel_dF"] / fp["normwise_rel_dF"],
                            p["normwise_rel_dmean_full"] / fp["normwise_rel_dmean_full"])
                self.assertEqual(g["verdict"], "stop" if ratio > 10.0 else "proceed")
                gates.append(g["verdict"])
            self.assertEqual(rec["status"],
                             "PASS" if all(p["pass"] for p in rec["points"]) else "FAIL")
        self.assertEqual(data["gate"], "stop" if "stop" in gates else "proceed")
        self.assertEqual(data["status"], "PASS" if all(r["status"] == "PASS"
                                                       for r in data["records"]) else "FAIL")

    def test_committed_warm_sweep_records_are_consistent(self):
        import fd_gradient_check as c
        import fd_step_sweep as s
        for name, space, comps in (("fd-step-sweep-L1-warm.json", "a", list(s.COMPONENTS)),
                                   ("fd-step-sweep-L1-warm-b.json", "b", ["Sr1", "SW4", "SW1"])):
            with self.subTest(record=name):
                data = json.loads((HERE / name).read_text())
                self.assertEqual((data["issue"], data["level"], data["protocol_step"],
                                  data["method"]), (1224, 1, FD_STEP, "slsqp_fd_warm"))
                self.assertEqual(data["components"], comps)
                rec = data["record"]
                self.assertEqual((rec["space"], rec["point"], rec["level"], rec["val_level"]),
                                 (space, "interior", 1, 2))
                self.assertEqual(rec["train_msh_sha256"], L1_TRAIN_SHA256)
                self.assertEqual(rec["val_mesh_sha256"],
                                 c.FROZEN_SHA256["icem_initial_sector_L2.msh"])
                self.assertEqual(rec["charged"]["adjoint"]["entries"], 1)
                self.assertEqual(rec["charged"]["remesh"]["entries"], 0)
                self.assertEqual(rec["solves"], 1 + 5 * len(comps))
                for comp in rec["components"]:
                    self.assertTrue(all(st["start"] == "same_mesh" for st in comp["stencil"]))
                    for r in comp["schemes"]:
                        self.assertTrue(r["usable"])
                        for dq in ("dF", "dmean_full"):
                            e = r[dq]
                            self.assertAlmostEqual(e["signed_err"], e["fd"] - e["adjoint"],
                                                   delta=1e-15)
                # The forward 1e-6 values are the L1 warm check's, bit for bit.
                repro = data["reproduction"]
                self.assertEqual(repro["file"], "benchmarks/motor_optimization/harness/"
                                 "fd-gradient-runtime-checks-L1-warm.json")
                self.assertEqual(len(repro["components"]), len(comps))
                for row in repro["components"]:
                    self.assertEqual((row["dF_diff"], row["dmean_full_diff"]), (0.0, 0.0))


if __name__ == "__main__":
    unittest.main(verbosity=2)
