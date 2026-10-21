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
import main_schedule  # noqa: E402
import session  # noqa: E402
from driver import FD_STEP, Refused, WiringError  # noqa: E402
from evaluator import from_bits  # noqa: E402
from test_harness import N, X0, make_run  # noqa: E402

#: The registered main queue and its method specs before method F existed
#: (canonical JSON SHA-256 as `export_queue`, origin/main c08ce9e6): F must
#: leave both unchanged (#1218 acceptance 2).
MAIN_JOBS_SHA256 = "eedac7dbbc7f53516986417f69161eb9915f9b487cc1b9f6fb0af8d2b4d5e0ac"
MAIN_SPECS_SHA256 = "afc6921dc06bfcd2b149bfd5f13fbdcd3be04acc7230844598f5253de35f9374"
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


if __name__ == "__main__":
    unittest.main(verbosity=2)
