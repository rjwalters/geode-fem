"""Tests of the experiment-2 driver (issue #1195): charging, the budget
stop, the acceptance and mesh wiring, the methods end to end, the ordering
rule and the committed starts. The evaluator is a fake with an analytic
model and a fake clock, so every charge and every stop is exact.

    benchmarks/motor_optimization/harness/pyrun.sh \\
        benchmarks/motor_optimization/harness/test_harness.py
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

import acceptance  # noqa: E402
import driver  # noqa: E402
import session as session_mod  # noqa: E402
from acceptance import AcceptanceMismatch, Anchor, rule  # noqa: E402
from driver import Problem, Run, WiringError, method_order  # noqa: E402
from evaluator import Aborted, from_bits, to_bits  # noqa: E402
from ledger import CATEGORIES, BudgetExhausted, Ledger  # noqa: E402
from lhs import latin_hypercube, to_box  # noqa: E402
from run import run_one  # noqa: E402
from surrogate import CubicRBF  # noqa: E402

SHA = "a" * 64
F0 = 1.0e-2
MEAN0 = 1.44
N = 4
LOWER = [0.0, 1.0, -2.0, 5.0]
UPPER = [1.0, 3.0, 2.0, 6.0]
X0 = [0.2, 1.4, -1.0, 5.1]
CENTRE_U = np.array([0.7, 0.6, 0.4, 0.55])
FAKE_FRACTIONS = {"T50": 0.5, "T90": 0.1, "T99": 0.01}
MOTION = 1 / 64
IO = 1 / 512
GEOM = 1 / 1024
TAU = 0.05
TRAIN = MOTION + 1.0
VALIDATE = MOTION + 2.0
WEIGHTS = {"validation_solve": 1 / 60, "train_forward_solve": 0.4 / 60,
           "adjoint_solve": 0.1 / 60, "validation_eval": 1.0, "train_forward_eval": 0.4,
           "adjoint_eval": 0.1}


class FakeClock:
    def __init__(self):
        self.t = 100.0

    def __call__(self):
        return self.t

    def advance(self, dt):
        self.t += dt


class TickingClock(FakeClock):
    """A clock that moves on every read (driver work takes time)."""

    def __call__(self):
        self.t += 1 / 4096
        return self.t


def anchor_json():
    a = Anchor(F0, MEAN0, 0, SHA)
    return {"F0": a.f0, "mean0_full_current": a.mean0, "level": 0, "msh_sha256": SHA,
            "targets": dict(a.targets()), "mean_floor": a.mean_floor()}


class FakeEvaluator:
    """The evaluator protocol on an analytic model.

    q(u) = 0.002 + sum_i (u_i - c_i)^2 / q(u0) normalised so that q(u0) = 1;
    validation F = F0 q(u), mean = MEAN0 (1.004 + 0.02 (u_1 - c_1)); the
    training model is F_train = 0.9 F_val, mean_train = 0.99 mean_val.
    Costs (s, dyadic so that every sum is exact): train motion 1/64,
    forward 1, adjoint 3/8, remesh 1/2 when u_0 > 0.95; validation motion
    1/64, solves 2, fresh mesh 1/4 when u_0 > 0.95; io (outside the
    charges) 1/512 per request."""

    def __init__(self, clock, io=IO, validator_sha=SHA, tamper=None, post=0.0, wall=None,
                 mesh_bad=None):
        """`wall`: the coupled geometry guard m = wall - u_0 - u_3 (the
        analogue of SW2-min with SW1/SW4-max; default 1.9, outside the
        box); a second guard m = u_2 + 0.5 never binds. Buildable when
        every m > 0, admissible when every h = m / m(u_0) - 0.05 >= 0.
        `mesh_bad(u)`: designs whose every remesh attempt fails."""
        self.clock = clock
        self.io = io
        # Driver-side time after the reply has arrived (parsing it): not
        # bounded by the request's deadline.
        self.post = post
        self.tamper = tamper or {}
        self.lower = np.array(LOWER)
        self.width = np.array(UPPER) - self.lower
        self.u0 = (np.array(X0) - self.lower) / self.width
        self.scale = 1.0 / (0.002 + float(np.sum((self.u0 - CENTRE_U) ** 2)))
        self.wall = 1.9 if wall is None else wall
        self.mesh_bad = mesh_bad or (lambda u: False)
        self.requests = []
        self.emitted = {c: 0.0 for c in CATEGORIES}
        self.ready = {"op": "ready", "ok": True,
                      "space": {"free": [f"p{i}" for i in range(N)], "lower": LOWER,
                                "upper": UPPER, "x0": X0},
                      "validation_msh_sha256": validator_sha, "anchor": anchor_json()}
        self.closed = False

    def q(self, u):
        return self.scale * (0.002 + float(np.sum((u - CENTRE_U) ** 2)))

    def dq_dx(self, u):
        return self.scale * 2.0 * (u - CENTRE_U) / self.width

    def mean(self, u):
        return MEAN0 * (1.004 + 0.02 * (u[1] - CENTRE_U[1]))

    def margins(self, u):
        return np.array([self.wall - u[0] - u[3], u[2] + 0.5])

    def domain(self, u):
        m = self.margins(u)
        s = np.abs(self.margins(self.u0))
        h = m / s - TAU
        dm_du = np.array([[-1.0, 0.0, 0.0, -1.0], [0.0, 0.0, 1.0, 0.0]])
        dh_dx = dm_du / s[:, None] / self.width
        buildable = bool(np.all(m > 0))
        return {"buildable": buildable, "admissible": buildable and bool(np.all(h >= 0)),
                "credit_admissible": buildable and bool(np.all(h >= -1e-12)),
                "h": h.tolist(), "min_h": float(h.min()), "jacobian_defined": True,
                "inadmissible_guards": [f"g{i}" for i in range(2) if h[i] < 0],
                "construction_error": None if buildable else "slot flank degenerates",
                "guards": [{"name": f"g{i}", "unit": "m", "margin": float(m[i]),
                            "scale": float(s[i]), "c": float(m[i] / s[i]), "h": float(h[i]),
                            "pass": bool(m[i] > 0), "dh_dx": dh_dx[i].tolist()}
                           for i in range(2)],
                "discrete": [{"name": "components_finite", "pass": True}]}

    def request(self, op, timeout=None, **f):
        self.requests.append((op, f))
        x = np.array(from_bits(f["x_bits"]))
        u = (x - self.lower) / self.width
        dom = self.domain(u)
        if op == "geometry":
            charges = [("geometry_check", GEOM, 0, 0)]
        elif op in ("train", "validate", "mesh_check") and not dom["buildable"]:
            raise AssertionError(f"{op} of a point that is not buildable reached the evaluator")
        if op == "geometry":
            pass
        elif op == "mesh_check":
            charges = [("geometry_check", GEOM, 0, 0), ("mesh_motion", MOTION, 0, 0)]
            if self.mesh_bad(u):
                charges.append(("remesh", 4 * 0.5, 0, 0))
        elif op == "train" and self.mesh_bad(u):
            charges = [("geometry_check", GEOM, 0, 0), ("mesh_motion", MOTION, 0, 0),
                       ("remesh", 4 * 0.5, 0, 0)]
        elif op == "train":
            charges = [("mesh_motion", MOTION, 0, 0)]
            if u[0] > 0.95:
                charges.append(("remesh", 0.5, 0, 0))
            cat = "line_search" if f.get("purpose") == "line_search" else "forward"
            charges.append((cat, 1.0, 60, 0))
            if f.get("gradient"):
                charges.append(("adjoint", 0.375, 0, 60))
        elif op == "diagnose":
            charges = [("mesh_motion", MOTION, 0, 0), ("forward", 1.0, 40, 0),
                       ("validation", 2.0, 40, 0)]
        elif op == "validate":
            charges = [("mesh_motion", MOTION, 0, 0)]
            if u[0] > 0.95:
                charges.append(("remesh", 0.25, 0, 0))
            charges.append(("validation", 2.0, 60, 0))
        else:
            raise AssertionError(op)
        cost = sum(c[1] for c in charges) + self.io
        if timeout is not None and cost > timeout:
            self.clock.advance(max(timeout, 0.0))
            raise Aborted()
        self.clock.advance(cost + self.post)
        for c in charges:
            self.emitted[c[0]] += c[1]
        echo = list(f["x_bits"])
        if "echo_bits" in self.tamper:
            echo = to_bits([v + 1e-9 for v in from_bits(echo)])
        reply = {"ok": True, "op": op, "x_bits": echo,
                 "charges": [{"category": c, "seconds": s, "forward_solves": nf,
                              "adjoint_solves": na, "note": ""} for c, s, nf, na in charges]}
        if op == "geometry":
            reply["geometry"] = dom
            return reply
        if op == "mesh_check":
            reply["status"] = "mesh_infeasible" if self.mesh_bad(u) else "ok"
            return reply
        if op == "train" and self.mesh_bad(u):
            reply.update(status="mesh_infeasible", attempts=[{"attempt": k} for k in range(4)])
            return reply
        fval, mval = F0 * self.q(u), self.mean(u)
        if op == "diagnose":
            fr = f["fractions"]
            reply.update(fractions=fr,
                         training={"fractions": fr, "means": [0.99 * g * mval for g in fr],
                                   "stds": [g * math.sqrt(0.9 * fval) for g in fr]},
                         validation={"fractions": fr, "means": [g * mval for g in fr],
                                     "stds": [g * math.sqrt(fval) for g in fr],
                                     "mesh": {"kind": "moved"}})
            for model in ("training", "validation"):
                m = reply[model]
                m["angles_deg"] = list(range(20))
                m["torques"] = [[mean + math.sqrt(2) * std * math.sin(2 * math.pi * k / 20)
                                  for k in range(20)]
                                 for mean, std in zip(m["means"], m["stds"])]
            reply["training"]["reference_msh_sha256"] = SHA
            reply["validation"]["msh_sha256"] = SHA
            return reply
        if op == "train":
            g = f.get("gradient")
            dmean = np.zeros(N)
            dmean[1] = 0.99 * MEAN0 * 0.02 / self.width[1]
            reply.update(status="ok", F=0.9 * fval, mean_full=0.99 * mval,
                         fractions=[0.0, 0.5, 1.0],
                         remesh_meshes=([{"name": "remesh000", "attempt": 0,
                                          "msh_sha256": "f" * 64}] if u[0] > 0.95 else []),
                         stds=[0.0, math.sqrt(0.3 * 0.9 * fval), math.sqrt(0.7 * 0.9 * fval)],
                         means=[0.0, 0.5 * mval, 0.99 * mval],
                         dF=(0.9 * F0 * self.dq_dx(u)).tolist() if g else None,
                         dmean_full=dmean.tolist() if g else None,
                         generation=1 if u[0] > 0.95 else 0,
                         event={} if u[0] > 0.95 else None, quality={}, newton_iterations=1)
        else:
            # The evaluator's rule, written out here independently of
            # acceptance.py (as the Rust side is).
            fr = dict(FAKE_FRACTIONS, **self.tamper.get("target_fractions", {}))
            mval *= self.tamper.get("val_mean_scale", 1.0)
            d = max(0.10 * fval, 2.5e-7)
            mean_ok = mval >= MEAN0 * (1.0 + 0.0025)
            flip = self.tamper.get("flip_attained", ())
            adm = dom["credit_admissible"] if "admissible" not in self.tamper \
                else self.tamper["admissible"]
            targets = [{"name": n, "T": fr[n] * F0,
                        "attained": ((fval + d <= fr[n] * F0) and mean_ok and adm)
                        != (n in flip),
                        "F_ok": True, "slack": 0.0}
                       for n in ("T50", "T90", "T99")]
            reply.update(status="ok", F_val=fval, mean_full=mval, fractions=[0.0, 0.5, 1.0],
                         admissibility={"buildable": True, "admissible": dom["admissible"],
                                        "credit_admissible": adm,
                                        "min_h": dom["min_h"]},
                         stds=[0.0, math.sqrt(0.3 * fval), math.sqrt(0.7 * fval)],
                         means=[0.0, 0.5 * mval, mval],
                         acceptance={"F_val": fval, "mean_full": mval, "admissible": adm,
                                     "mean_ok": mean_ok, "targets": targets},
                         mesh={"kind": "fresh" if u[0] > 0.95 else "moved"},
                         msh_sha256=SHA,
                         validator_msh_sha256=self.tamper.get("validator_sha", SHA),
                         anchor_msh_sha256=SHA)
        return reply

    def close(self):
        self.closed = True


def theta0_train(fake):
    u0 = fake.u0
    return {"F": 0.9 * F0 * fake.q(u0), "mean_full": 0.99 * fake.mean(u0)}


def make_run(budget_fe=1000.0, method="slsqp", io=IO, **kw):
    clock = FakeClock()
    led = Ledger(1.0, budget_fe, weights=WEIGHTS, clock=clock)
    fake = FakeEvaluator(clock, io=io, **kw)
    anchor = Anchor(F0, MEAN0, 0, SHA)
    prob = Problem(fake.ready["space"], theta0_train(fake))
    return Run(fake, led, anchor, prob, method, clock=clock), fake, led, clock


def to_x(u):
    return (np.array(LOWER) + np.asarray(u) * (np.array(UPPER) - np.array(LOWER))).tolist()


def near_centre(run):
    """A point that attains T50 and T90 but not T99, above the mean floor."""
    return 0.9 * CENTRE_U + 0.1 * run.problem.u_of(X0)


class LedgerTests(unittest.TestCase):
    def test_sync_charges_every_second_of_wall_time(self):
        clock = FakeClock()
        led = Ledger(2.0, 10.0, clock=clock)
        clock.advance(0.5)
        led.charge("forward", 0.3)
        led.sync("io")
        self.assertAlmostEqual(led.charged_seconds(), 0.5, places=15)
        self.assertEqual(led.entries[-1]["category"], "overhead")
        self.assertAlmostEqual(led.total_fe(), 0.25, places=15)
        led.time("surrogate_update", "fit", lambda: clock.advance(1.25))
        self.assertAlmostEqual(led.totals()["surrogate_update"]["seconds"], 1.25, places=15)
        self.assertAlmostEqual(led.charged_seconds(), led.elapsed(), places=12)

    def test_budget_edge_is_inclusive_and_charge_to_budget_is_exact(self):
        clock = FakeClock()
        led = Ledger(3.0, 2.0, clock=clock)
        led.charge("forward", 5.0)
        self.assertFalse(led.exhausted())
        led.charge_to_budget("validation", "abort")
        self.assertTrue(led.exhausted())
        self.assertEqual(led.charged_seconds(), 6.0)
        self.assertEqual(led.total_fe(), 2.0)

    def test_refuses_bad_charges(self):
        led = Ledger(1.0, 1.0, clock=FakeClock())
        for bad in (lambda: led.charge("nope", 1.0), lambda: led.charge("forward", -1.0),
                    lambda: led.charge("forward", math.nan)):
            self.assertRaises(ValueError, bad)

    def test_solve_fe_uses_the_weights(self):
        led = Ledger(1.0, 100.0, weights=WEIGHTS, clock=FakeClock())
        led.charge("forward", 1.0, 60, 0)
        led.charge("line_search", 1.0, 60, 0)
        led.charge("adjoint", 1.0, 0, 60)
        led.charge("validation", 1.0, 60, 0)
        led.charge("remesh", 9.0)
        self.assertAlmostEqual(led.solve_fe(), 0.4 + 0.4 + 0.1 + 1.0, places=14)


class ChargingTests(unittest.TestCase):
    def test_every_request_category_reaches_the_ledger(self):
        run, fake, led, clock = make_run()
        u0 = run.problem.u_of(X0)
        run.train(u0, gradient=True, purpose="iterate")
        u1 = u0.copy()
        u1[1] += 0.01
        run.train(u1, gradient=False, purpose="line_search")
        u2 = u0.copy()
        u2[0] = 0.99  # remesh
        run.train(u2, gradient=False)
        run.validate(u2)  # fresh mesh
        run.validate(u1)
        led.time("setup_calibration", "fit", lambda: clock.advance(0.0625))
        led.time("surrogate_update", "refit", lambda: clock.advance(0.078125))
        led.sync("end")
        totals = led.totals()
        for c in CATEGORIES:
            self.assertGreater(totals[c]["seconds"], 0.0, c)
        # Each reply charge reaches its own category, in full.
        for c in ("forward", "adjoint", "line_search", "mesh_motion", "remesh", "validation",
                  "geometry_check"):
            self.assertEqual(totals[c]["seconds"], fake.emitted[c], c)
        self.assertEqual(totals["setup_calibration"]["seconds"], 0.0625)
        self.assertEqual(totals["surrogate_update"]["seconds"], 0.078125)
        self.assertEqual(totals["validation"]["forward_solves"], 120)
        self.assertEqual(totals["adjoint"]["adjoint_solves"], 60)
        self.assertEqual(totals["line_search"]["forward_solves"], 60)
        # The io outside the charges is overhead, and nothing is excluded.
        self.assertEqual(totals["overhead"]["seconds"], len(fake.requests) * IO)
        self.assertAlmostEqual(led.charged_seconds(), led.elapsed(), places=12)

    def test_validation_is_charged_before_attainment_is_stamped(self):
        run, fake, led, clock = make_run()
        u = near_centre(run)
        run.geometry(u)
        before = led.total_fe()
        v = run.validate(u)
        self.assertEqual(v["wall_fe"] - before, VALIDATE + IO)
        self.assertEqual(led.totals()["validation"]["entries"], 1)
        self.assertEqual(run.attained["T50"]["wall_fe"], v["wall_fe"])
        self.assertAlmostEqual(run.attained["T50"]["solve_fe"], 1.0, places=12)

    def test_repeated_points_are_not_charged_twice(self):
        run, fake, led, clock = make_run()

        def n(op):
            return sum(1 for o, _ in fake.requests if o == op)
        u = run.problem.u_of(X0)
        run.train(u, gradient=False)
        run.train(u, gradient=False)
        self.assertEqual(n("train"), 1)
        run.train(u, gradient=True)  # needs the gradient: a new request
        self.assertEqual(n("train"), 2)
        run.validate(u)
        run.validate(u)
        self.assertEqual(n("validate"), 1)
        # The geometry of a point is requested once.
        self.assertEqual(n("geometry"), 1)


class BudgetTests(unittest.TestCase):
    def test_run_stops_exactly_at_the_budget(self):
        for method in driver.METHODS:
            run, fake, led, clock = make_run(budget_fe=7.3, method=method)
            reason, msg, err = driver.execute(run, run.problem.u_of(X0))
            self.assertEqual(reason, "budget", method)
            self.assertAlmostEqual(led.total_fe(), 7.3, places=12, msg=method)
            self.assertTrue(led.exhausted())
            self.assertIn("aborted at the budget", led.entries[-1]["note"])
            # No request after the aborted one.
            n = len(fake.requests)
            self.assertRaises(BudgetExhausted, run.train, np.full(N, 0.123), False)
            self.assertEqual(len(fake.requests), n)
            self.assertAlmostEqual(clock() - led.t0, 7.3, places=12)

    def test_validation_ending_exactly_at_the_budget_is_credited(self):
        run, fake, led, clock = make_run(budget_fe=GEOM + TRAIN + VALIDATE, io=0.0)
        u = near_centre(run)
        run.train(u, gradient=False)
        with self.assertRaises(BudgetExhausted):
            run.validate(u)
            run.train(np.full(N, 0.3), False)
        self.assertTrue(run.validations[0]["credited"])
        self.assertIn("T50", run.attained)
        self.assertNotIn("T99", run.attained)
        self.assertEqual(led.total_fe(), GEOM + TRAIN + VALIDATE)
        self.assertEqual([o for o, _ in fake.requests], ["geometry", "train", "validate"])

    def test_validation_beyond_the_budget_is_aborted_and_not_credited(self):
        run, fake, led, clock = make_run(budget_fe=GEOM + TRAIN + VALIDATE - 1 / 64, io=0.0)
        u = near_centre(run)
        run.train(u, gradient=False)
        with self.assertRaises(BudgetExhausted):
            run.validate(u)
        self.assertEqual(run.validations, [])
        self.assertEqual(run.attained, {})
        self.assertEqual(led.total_fe(), GEOM + TRAIN + VALIDATE - 1 / 64)
        self.assertEqual(led.entries[-1]["category"], "validation")

    def test_validation_whose_reply_lands_after_the_budget_is_not_credited(self):
        # The reply arrives before the deadline, but the driver's handling
        # of it carries the total past the budget: recorded, not credited.
        run, fake, led, clock = make_run(budget_fe=GEOM + TRAIN + VALIDATE + 1 / 1024, io=0.0)
        u = near_centre(run)
        run.train(u, gradient=False)
        fake.post = 1 / 256
        with self.assertRaises(BudgetExhausted):
            run.validate(u)
        self.assertEqual(len(run.validations), 1)
        self.assertFalse(run.validations[0]["credited"])
        self.assertTrue(run.validations[0]["attained"]["T50"])
        self.assertEqual(run.attained, {})
        self.assertTrue(run.result()["attainment"]["T50"]["censored"])

    def test_t99_stops_the_run(self):
        run, fake, led, clock = make_run()
        with self.assertRaises(driver.TargetReached):
            run.validate(CENTRE_U)
        self.assertEqual(set(run.attained), {"T50", "T90", "T99"})


class AcceptanceWiringTests(unittest.TestCase):
    def test_first_attainment_of_each_target(self):
        run, fake, led, clock = make_run()
        # q from far to near the centre: F_val decreases monotonically.
        u0 = run.problem.u_of(X0)
        stamps = []
        for t in (0.0, 0.3, 0.6, 0.8, 0.9):
            u = u0 + t * (CENTRE_U - u0)
            v = run.validate(u)
            stamps.append(v)
        a = Anchor(F0, MEAN0, 0, SHA)
        for name, T in a.targets():
            first = next((v for v in stamps
                          if v["F_val"] + acceptance.delta_f(v["F_val"]) <= T
                          and v["mean_full"] >= a.mean_floor()), None)
            if first is None:
                self.assertNotIn(name, run.attained, name)
            else:
                self.assertEqual(run.attained[name]["validation"], first["index"], name)
                self.assertEqual(run.attained[name]["wall_fe"], first["wall_fe"], name)
        self.assertIn("T50", run.attained)
        self.assertNotIn("T99", run.attained)
        res = run.result()
        self.assertTrue(res["attainment"]["T99"]["censored"])
        self.assertFalse(res["attainment"]["T50"]["censored"])

    def test_rule_edges(self):
        a = Anchor(F0, MEAN0, 0, SHA)
        floor = a.mean_floor()
        # 10% regime: F_val = T / 1.1 attains exactly; the next float up does not.
        t50 = 0.5 * F0
        f = t50 / 1.1
        while f + 0.1 * f > t50:
            f = math.nextafter(f, 0.0)
        self.assertTrue(rule(a, f, floor, True)[0]["T50"])
        g = math.nextafter(f, 1.0)
        while g + 0.1 * g <= t50:
            g = math.nextafter(g, 1.0)
        self.assertFalse(rule(a, g, floor, True)[0]["T50"])
        # Absolute regime: delta_F = 2.5e-7 at small F_val.
        self.assertEqual(acceptance.delta_f(1e-7), 2.5e-7)
        self.assertEqual(acceptance.delta_f(1e-3), 1e-4)
        # At F0 = 1e-6, T50 = 5e-7 is attained at F_val = 2.5e-7 exactly
        # (delta_F = 2.5e-7, absolute), and not one float above.
        small = Anchor(1e-6, MEAN0, 0, SHA)
        self.assertTrue(rule(small, 2.5e-7, floor, True)[0]["T50"])
        self.assertFalse(rule(small, math.nextafter(2.5e-7, 1.0), floor, True)[0]["T50"])
        self.assertFalse(any(rule(small, 0.0, floor, True)[0][n] for n in ("T90", "T99")))
        # The floor is inclusive; below it nothing is attained.
        self.assertTrue(rule(a, 1e-9, floor, True)[0]["T99"])
        self.assertFalse(rule(a, 1e-9, math.nextafter(floor, 0.0), True)[0]["T99"])
        self.assertFalse(any(rule(a, math.nan, floor, True)[0].values()))

    def test_wrong_target_in_the_evaluator_is_refused(self):
        run, fake, led, clock = make_run(tamper={"target_fractions": {"T90": 0.2}})
        with self.assertRaises(AcceptanceMismatch):
            run.validate(near_centre(run))

    def test_evaluator_anchor_within_an_ulp_is_adopted_otherwise_refused(self):
        clock = FakeClock()
        fake = FakeEvaluator(clock)
        f0_parsed = math.nextafter(F0, 1.0)  # the evaluator's parse, an ulp up
        fake.ready["anchor"] = dict(fake.ready["anchor"], F0=f0_parsed)
        led = Ledger(1.0, 100.0, weights=WEIGHTS, clock=clock)
        prob = Problem(fake.ready["space"], theta0_train(fake))
        run = Run(fake, led, Anchor(F0, MEAN0, 0, SHA), prob, "slsqp", clock=clock)
        self.assertEqual(run.anchor.f0, f0_parsed)
        self.assertEqual(run.result()["anchor"]["file_values"]["F0"], F0)
        for bad in (dict(F0=F0 * (1 + 1e-9)), dict(mean0_full_current=MEAN0 * 1.001),
                    dict(level=2), dict(msh_sha256="d" * 64)):
            fake.ready["anchor"] = dict(anchor_json(), **bad)
            with self.assertRaises(WiringError, msg=str(bad)):
                Run(fake, led, Anchor(F0, MEAN0, 0, SHA), prob, "slsqp", clock=clock)

    def test_an_evaluator_that_evaluated_other_bits_is_refused(self):
        run, fake, led, clock = make_run(tamper={"echo_bits": True})
        with self.assertRaises(WiringError):
            run.train(run.problem.u_of(X0), gradient=False)

    def test_a_flipped_attained_flag_stops_the_run(self):
        for name in ("T50", "T90", "T99"):
            run, fake, led, clock = make_run(tamper={"flip_attained": (name,)})
            with self.assertRaises(AcceptanceMismatch, msg=name):
                run.validate(near_centre(run))

    def test_validation_on_another_mesh_is_refused(self):
        with self.assertRaises(WiringError):
            make_run(validator_sha="b" * 64)
        run, fake, led, clock = make_run(tamper={"validator_sha": "c" * 64})
        with self.assertRaises(WiringError):
            run.validate(CENTRE_U)

    def test_rule_constants_are_the_registered_ones(self):
        self.assertEqual(acceptance.TARGETS, (("T50", 0.5), ("T90", 0.1), ("T99", 0.01)))
        self.assertEqual((acceptance.DELTA_F_REL, acceptance.DELTA_F_ABS,
                          acceptance.MEAN_FLOOR_MARGIN), (0.10, 2.5e-7, 0.0025))
        # The registered anchor (written by the Rust rule) loads: its targets
        # and floor follow from F0 and mean0 under the driver's rule too.
        reg = Anchor.from_file(HERE.parent / "anchor.json")
        self.assertEqual(reg.level, 2)
        self.assertEqual(reg.f0, 9.603404147653403e-3)

    def test_anchor_file_is_checked(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "anchor.json"
            good = anchor_json()
            p.write_text(json.dumps({"anchor": good}))
            self.assertEqual(Anchor.from_file(p).f0, F0)
            bad = dict(good, targets=dict(good["targets"], T90=0.2 * F0))
            p.write_text(json.dumps({"anchor": bad}))
            self.assertRaises(ValueError, Anchor.from_file, p)


class MethodTests(unittest.TestCase):
    def test_slsqp_labels_line_search_trials(self):
        run, fake, led, clock = make_run()
        fun, jac, con, con_jac, geo, geo_jac, cb = driver.slsqp_callables(run)
        u0 = run.problem.u_of(X0)
        fun(u0)
        jac(u0)
        fun(u0 + 0.1)  # first trial after the gradient: an iterate
        fun(u0 + 0.05)  # a second trial: line search
        con(u0 + 0.05)
        con_jac(u0 + 0.05)
        fun(u0 + 0.2)  # after the gradient again
        purposes = [f["purpose"] for op, f in fake.requests if op == "train"]
        self.assertEqual(purposes, ["iterate", "iterate", "line_search", "iterate"])
        self.assertTrue(all(f["gradient"] for op, f in fake.requests if op == "train"))

    def test_methods_end_to_end_write_a_trace(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "anchor.json").write_text(json.dumps({"anchor": anchor_json()}))
            starts = {"starts": {"theta0": {"a": X0}}}
            (d / "starts.json").write_text(json.dumps(starts))
            probe = FakeEvaluator(FakeClock())
            session = {"label": "test", "session_id": "t", "budget_fe": 400.0,
                       "unit_seconds": 1.0, "weights": WEIGHTS,
                       "theta0_train": theta0_train(probe),
                       "evaluator": {"anchor": str(d / "anchor.json")},
                       "starts": str(d / "starts.json")}
            session["primary_model"] = "L1"
            session["models"] = {f"L{level}": {"train_mesh": f"fixture_L{level}.msh",
                "train_level": level, "theta0_train": session["theta0_train"],
                "weights": WEIGHTS} for level in (0, 1)}
            for method in driver.METHODS:
                clock = FakeClock()
                holder = {}

                def factory(space, ledger, clock=clock, holder=holder):
                    holder["ev"] = FakeEvaluator(clock)
                    return holder["ev"]
                out = d / f"{method}.json"
                t = run_one(session, method, "a", "theta0", 1, out, position=0, clock=clock,
                            evaluator_factory=factory)
                back = json.loads(out.read_text())
                self.assertEqual(back["method"], method)
                self.assertIn(t["stop_reason"], ("T99", "budget", "converged"), t.get("error"))
                # Short budget: the same run stops at the budget exactly.
                clock2 = TickingClock()
                short = dict(session, budget_fe=9.5)
                t2 = run_one(short, method, "a", "theta0", 1, d / f"{method}_short.json",
                             clock=clock2, evaluator_factory=lambda sp, led, c=clock2:
                             FakeEvaluator(c))
                self.assertEqual(t2["stop_reason"], "budget", method)
                self.assertAlmostEqual(t2["ledger"]["total_fe"], 9.5, places=12, msg=method)
                self.assertTrue(holder["ev"].closed)
                self.assertGreater(len(back["validations"]), 0, method)
                self.assertIn("T50", back["attainment"])
                self.assertAlmostEqual(back["ledger"]["total_seconds"],
                                       back["ledger"]["elapsed_seconds"], places=9)
                if method == "surrogate":
                    self.assertGreater(len(back["assessment"]["held_out_training"]), 0)
                    self.assertGreater(
                        len(back["assessment"]["new_design_validation_model_checks"]), 0)
                    diag = back["post_run_diagnostic"]
                    self.assertEqual(diag["status"], "ok")
                    self.assertFalse(diag["counted_in_run_budget"])
                    self.assertEqual([r["fraction"] for r in diag["rows"]], [0.25, 0.75])
                    self.assertEqual(back["method_label"], "empirical RBF surrogate (trust region)")
                    self.assertGreater(back["ledger"]["totals"]["setup_calibration"]["entries"],
                                       0)
                    self.assertGreater(back["ledger"]["totals"]["surrogate_update"]["entries"],
                                       0)
                elif method in ("cobyqa_l0", "slsqp_l0"):
                    diag = back["post_run_diagnostic"]
                    self.assertEqual(diag["status"], "ok", diag)
                    self.assertFalse(diag["counted_in_run_budget"])
                    self.assertEqual(diag["kind"], "nonlinear_physical_model")
                else:
                    self.assertNotIn("post_run_diagnostic", back)
                # Amendment 5 §1: the start is never validated.
                self.assertNotIn(X0, [v["x"] for v in back["validations"]], method)
                print(f"{method}: {t['stop_reason']} at {t['ledger']['total_fe']:.2f} FE, "
                      f"{len(back['validations'])} validations, "
                      f"attained {[k for k, v in back['attainment'].items() if v['attained']]}")

    def test_no_method_validates_its_start_or_inside_its_initial_set(self):
        # θ0 (infeasible on the mean floor) and a start next to the optimum,
        # whose start may well be the best point of the initial set.
        for method in driver.METHODS:
            for start in (X0, to_x(0.9 * CENTRE_U + 0.1 * np.full(N, 0.5))):
                run, fake, led, clock = make_run(budget_fe=300.0, method=method)
                u0 = run.problem.u_of(start)
                reason, msg, err = driver.execute(run, u0)
                self.assertIsNone(err, method)
                xs = run.problem.x_of(u0).tolist()
                self.assertNotIn(xs, [v["x"] for v in run.validations], method)
                keys = [tuple(v["x"]) for v in run.validations]
                self.assertEqual(len(keys), len(set(keys)), f"{method}: a repeat")
                if not run.validations:
                    continue
                # Nothing is validated before the initial set is complete.
                n_init = {"slsqp": 1, "cobyqa": 2 * N + 1, "surrogate": N + 1}[driver.METHOD_SPECS[method]["optimizer"]]
                first_val = next(i for i, e in enumerate(run.events) if e["kind"] == "validate")
                n_train = sum(1 for e in run.events[:first_val] if e["kind"] == "train")
                self.assertGreaterEqual(n_train, n_init, method)

    def test_surrogate_counts_a_nonimproving_geometry_substitute_as_a_rejection(self):
        # Every subproblem solution counts as a near-duplicate: only geometry
        # points are evaluated. Non-improving ones must halve Δ, so the run
        # ends on Δ < 1e-6 long before the budget (C1), and its final
        # incumbent is validated under amendment 5 §1 (C2).
        # The validation mean is held below the floor: no target stops it.
        run, fake, led, clock = make_run(budget_fe=1e6, method="surrogate",
                                         tamper={"val_mean_scale": 0.9})
        opts = dict(driver.SURROGATE, duplicate_tol=1e12)
        u0 = run.problem.u_of(X0)
        run.start_key = run.problem.x_of(u0).tobytes()
        msg = driver.run_surrogate(run, u0, opts)
        self.assertIn("trust radius", msg)
        self.assertLess(led.total_fe(), 1000.0)
        steps = run.c_steps
        rejected = [k for k, st in enumerate(steps[:-1]) if st["geometry"]
                    and not st["accepted"]]
        self.assertGreater(len(rejected), 0)
        for k in rejected:
            self.assertEqual(steps[k + 1]["delta"], steps[k]["delta"] / 2, k)
        final = run.c_incumbent["x"].tobytes()
        if final != run.start_key:
            self.assertIn(final, run._val)

    def test_rbf_interpolates_and_its_gradient_is_right(self):
        rng = np.random.default_rng(0)
        X = rng.random((9, 3))
        y = np.sin(X @ np.array([1.0, 2.0, -1.0]))
        s = CubicRBF(X, y)
        self.assertTrue(s.exact)
        for xi, yi in zip(X, y):
            self.assertAlmostEqual(s(xi), yi, places=10)
        u = np.array([0.3, 0.4, 0.5])
        h = 1e-6
        fd = np.array([(s(u + h * e) - s(u - h * e)) / (2 * h) for e in np.eye(3)])
        np.testing.assert_allclose(s.grad(u), fd, rtol=1e-6, atol=1e-8)
        # n + 1 points: the linear interpolant.
        L = CubicRBF(np.vstack([np.zeros(3), np.eye(3)]), np.array([1.0, 2.0, 3.0, 4.0]))
        self.assertAlmostEqual(L(np.array([0.5, 0.5, 0.5])), 1.0 + 0.5 * (1 + 2 + 3), places=10)


class DiagnosticTests(unittest.TestCase):
    def test_post_run_diagnostic_changes_no_budget_or_attainment(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "anchor.json").write_text(json.dumps({"anchor": anchor_json()}))
            (d / "starts.json").write_text(json.dumps({"starts": {"theta0": {"a": X0}}}))
            probe = FakeEvaluator(FakeClock())
            for budget in (400.0, 9.5):
                session = {"label": "test", "session_id": "t", "budget_fe": budget,
                           "unit_seconds": 1.0, "weights": WEIGHTS,
                           "theta0_train": theta0_train(probe),
                           "evaluator": {"anchor": str(d / "anchor.json")},
                           "starts": str(d / "starts.json")}
                traces = []
                for diag in (False, True):
                    clock = FakeClock()
                    t = run_one(session, "surrogate", "a", "theta0", 1, d / f"c{diag}.json",
                                clock=clock, diagnostic=diag,
                                evaluator_factory=lambda sp, led, c=clock: FakeEvaluator(c))
                    traces.append(json.loads((d / f"c{diag}.json").read_text()))
                off, on = traces
                for k in ("ledger", "attainment", "validations", "stop_reason", "events"):
                    self.assertEqual(off[k], on[k], (budget, k))
                pd = on["post_run_diagnostic"]
                self.assertEqual(pd["status"], "ok", pd)
                self.assertGreater(sum(c["seconds"] for c in pd["charges"]), 0)
                self.assertGreater(pd["seconds"], 0)
                for r in pd["rows"]:
                    self.assertIn(r["neighbours"], ([0.0, 0.5], [0.5, 1.0]))
                    self.assertIsNotNone(r["rel_std_val"])

    def test_unit_is_the_median_of_all_3n_concurrent_timings(self):
        def cal(vals):
            return {"ok": True, "validation_seconds": vals, "train_forward_seconds": [1.0] * 3,
                    "train_adjoint_seconds": [0.5] * 3,
                    "weights": {"validation_solves_per_eval": 60, "train_solves_per_eval": 60},
                    "theta0_train": {"F": 1.0, "mean_full": 2.0},
                    "theta0_validation": {}, "anchor_agrees_bitwise": True,
                    "anchor_agrees_4ulp": True, "deterministic": True}
        # Per-process medians 1, 2, 9 (median 2); all nine timings: median 3.
        c = session_mod.combine_calibrations([cal([1.0, 1.0, 3.0]), cal([2.0, 2.0, 4.0]),
                                              cal([9.0, 9.0, 3.0])])
        self.assertEqual(c["unit_seconds"], 3.0)
        self.assertEqual(len(c["validation_seconds"]), 9)
        self.assertEqual(c["weights"]["train_forward_eval"], 1.0 / 3.0)
        bad = cal([1.0, 1.0, 1.0])
        bad["theta0_train"] = {"F": 1.5, "mean_full": 2.0}
        with self.assertRaises(SystemExit):
            session_mod.combine_calibrations([cal([1.0] * 3), bad])


class ProtocolTests(unittest.TestCase):
    def test_method_order_alternates(self):
        self.assertEqual(method_order(1), ["slsqp", "cobyqa", "surrogate"])
        self.assertEqual(method_order(2), ["surrogate", "cobyqa", "slsqp"])

    def test_pilot_jobs(self):
        jobs = session_mod.build_jobs(["theta0", "lhs1", "lhs2"], ["a", "b"], 2)
        self.assertEqual(len(jobs), 36)
        self.assertEqual(len({(j["method"], j["space"], j["start"], j["repeat"])
                              for j in jobs}), 36)
        for rep in (1, 2):
            for sp in "ab":
                for st in ("theta0", "lhs1", "lhs2"):
                    seq = [j["method"] for j in jobs
                           if (j["repeat"], j["space"], j["start"]) == (rep, sp, st)]
                    self.assertEqual(seq, method_order(rep))

    def test_committed_starts_are_the_generator_output(self):
        st = json.loads((HERE / "starts.json").read_text())
        g = st["generator"]
        sb = st["spaces"]["b"]
        first = {1: 1, 2: 4, 3: 0, 4: 0}
        for name, seed in (("lhs1", 1), ("lhs2", 2), ("lhs3", 3), ("lhs4", 4)):
            e = st["starts"][name]
            u = latin_hypercube(seed, g["points_per_seed"], g["dimensions"])[e["index"]]
            self.assertEqual(u, e["u"])
            xb = to_box(u, sb["lower"], sb["upper"])
            self.assertEqual(xb, e["b"])
            self.assertEqual(xb[:8], e["a"])
            self.assertEqual(to_bits(xb), e["b_bits"])
            self.assertEqual(to_bits(xb[:8]), e["a_bits"])
            self.assertEqual(from_bits(e["b_bits"]), e["b"])
            self.assertTrue(all(lo <= v <= hi for v, lo, hi in zip(xb, sb["lower"], sb["upper"])))
            self.assertEqual([r["index"] for r in e["rejected"]],
                             list(range(first[seed], e["index"])))
            for c in e["checks"].values():
                self.assertIsNone(c["verdict"], name)
                self.assertGreaterEqual(c["min_h"], 0.0)
            self.assertEqual(set(e["checks"]), {"a_L0", "a_L1", "b_L0", "b_L1"})


if __name__ == "__main__":
    unittest.main(verbosity=2)
