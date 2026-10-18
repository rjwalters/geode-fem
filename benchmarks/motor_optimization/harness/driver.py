"""The experiment-2 optimizer driver (issue #1195, of #1185 with amendments
1-3 and the proposed amendment 4).

Three methods on one frozen space from one start, with matched budgets:

- A `slsqp`: scipy.optimize.minimize(method="SLSQP") with the adjoint
  gradient of F_train and of the full-current training mean.
- B `cobyqa`: scipy.optimize.minimize(method="COBYQA") (scipy >= 1.14),
  derivative-free, the same constraint and bounds.
- C `surrogate`: the calibrated RBF trust-region method of the proposed
  amendment 4 (README.md; `run_surrogate`).

Common to all three:

- Variables are the free components scaled to the unit box,
  u = (x - lower) / (upper - lower).
- Objective f(u) = F_train(x) / F_train(theta0); constraint
  g(u) = mean_train(x) / mean_train(theta0) - (1 + 0.25%) >= 0, with the
  theta0 training constants measured once per session (`calibrate_unit`).
- Validation policy: every new accepted iterate is validated on the
  validation model and scored against the anchor (SLSQP and COBYQA: the
  iterate each iteration's callback reports; C: each trust-region
  acceptance), charged to the method. A point already validated in the run
  is not validated again (a validation depends on the candidate only,
  amendment 3 §2). The start is not an iterate and is not validated.
- The run stops at T99 attainment, when the wall-time budget is spent
  (the request in flight is aborted at the budget), when the method stops
  on its own, or on an evaluator failure. Every outcome is recorded.
"""
import math
import time

import numpy as np
from scipy.optimize import Bounds, NonlinearConstraint, minimize

from acceptance import MEAN_FLOOR_MARGIN, TARGET_NAMES, Anchor, check_acceptance
from evaluator import Aborted, EvaluatorFailure, to_bits
from ledger import BudgetExhausted
from surrogate import Models

METHODS = ("slsqp", "cobyqa", "surrogate")
#: Method letters of the ordering rule (amendment 3 §4: ABC, then CBA).
LETTERS = {"A": "slsqp", "B": "cobyqa", "C": "surrogate"}


class TargetReached(Exception):
    """T99 was attained (the run's stopping target)."""


class WiringError(Exception):
    """A reply that the protocol forbids (wrong mesh, wrong anchor)."""


def method_order(repeat):
    """Amendment 3 §4: ABC on the first repeat, CBA on the second."""
    letters = "ABC" if repeat % 2 == 1 else "CBA"
    return [LETTERS[c] for c in letters]


class Problem:
    """The scaled problem of one space."""

    def __init__(self, space, theta0_train):
        self.names = list(space["free"])
        self.lower = np.array(space["lower"], dtype=float)
        self.upper = np.array(space["upper"], dtype=float)
        self.x0 = np.array(space["x0"], dtype=float)
        self.n = len(self.names)
        self.width = self.upper - self.lower
        self.f_ref = float(theta0_train["F"])
        self.mean_ref = float(theta0_train["mean_full"])
        self.floor_ratio = 1.0 + MEAN_FLOOR_MARGIN

    def x_of(self, u):
        x = self.lower + np.asarray(u, dtype=float) * self.width
        return np.minimum(np.maximum(x, self.lower), self.upper)

    def u_of(self, x):
        return (np.asarray(x, dtype=float) - self.lower) / self.width

    def objective(self, rec):
        return rec["F"] / self.f_ref

    def constraint(self, rec):
        return rec["mean_full"] / self.mean_ref - self.floor_ratio


class Run:
    """One charged optimizer run: training and validation requests, the
    validation policy, attainment, and the trace."""

    def __init__(self, evaluator, ledger, anchor, problem, method, clock=time.monotonic):
        self.ev = evaluator
        self.ledger = ledger
        self.anchor = anchor
        self.problem = problem
        self.method = method
        self.clock = clock
        self.events = []
        self.validations = []
        self.attained = {}
        self.assessment = {"held_out_training": [], "validation": []}
        self._train = {}
        self._val = {}
        self.check_ready(evaluator.ready)

    def check_ready(self, ready):
        """The evaluator validates on the anchor's mesh, against the same
        anchor, in the same space."""
        if ready.get("validation_msh_sha256") != self.anchor.mesh_sha256:
            raise WiringError(f"validation mesh {ready.get('validation_msh_sha256')} "
                              f"is not the anchor's {self.anchor.mesh_sha256}")
        # The evaluator parses the anchor file with serde_json, which can be
        # an ulp off the decimal; within 4 ulp it is the same anchor, and the
        # driver then scores with the evaluator's exact values (its replies
        # print them exactly), so both sides compare the same targets.
        a = ready.get("anchor") or {}
        f0, m0 = a.get("F0"), a.get("mean0_full_current")
        if not (ulp_close(f0, self.anchor.f0) and ulp_close(m0, self.anchor.mean0)
                and a.get("level") == self.anchor.level
                and a.get("msh_sha256") == self.anchor.mesh_sha256):
            raise WiringError("evaluator anchor is not the driver's")
        self.anchor_file_values = {"F0": self.anchor.f0, "mean0": self.anchor.mean0}
        self.anchor = Anchor(f0, m0, self.anchor.level, self.anchor.mesh_sha256,
                             source=self.anchor.source, file_sha256=self.anchor.file_sha256)
        if list(ready["space"]["free"]) != self.problem.names:
            raise WiringError("evaluator space is not the driver's")

    # ── requests ────────────────────────────────────────────────────────

    def _request(self, op, category, **fields):
        led = self.ledger
        led.sync("driver")
        if led.exhausted():
            raise BudgetExhausted()
        try:
            reply = self.ev.request(op, timeout=led.remaining_seconds(), **fields)
        except Aborted:
            led.charge_to_budget(category, note=f"{op} aborted at the budget")
            raise BudgetExhausted() from None
        for c in reply.get("charges", []):
            led.charge(c["category"], c["seconds"], c["forward_solves"], c["adjoint_solves"],
                       c.get("note") or op)
        led.sync(f"io {op}")
        if not reply.get("ok"):
            raise EvaluatorFailure(op, reply.get("error"))
        if "x_bits" in fields and reply.get("x_bits") != fields["x_bits"]:
            raise WiringError(f"{op}: the evaluator evaluated another point")
        return reply

    def _stamp(self):
        led = self.ledger
        return {"wall_fe": led.total_fe(), "solve_fe": led.solve_fe(),
                "seconds": led.charged_seconds()}

    def train(self, u, gradient, purpose="iterate", note=""):
        x = self.problem.x_of(u)
        key = x.tobytes()
        hit = self._train.get(key)
        if hit is not None and (hit["dF"] is not None or not gradient):
            return hit
        category = "line_search" if purpose == "line_search" else "forward"
        r = self._request("train", category, x_bits=to_bits(x), gradient=gradient,
                          purpose=purpose, note=note)
        rec = {"x": x, "u": self.problem.u_of(x), "F": r["F"], "mean_full": r["mean_full"],
               "stds": r["stds"], "means": r["means"], "fractions": r["fractions"],
               "dF": None if r["dF"] is None else np.array(r["dF"]),
               "dmean_full": None if r["dmean_full"] is None else np.array(r["dmean_full"])}
        self._train[key] = rec
        self.events.append(dict(self._stamp(), kind="train", x=x.tolist(), F=r["F"],
                                mean_full=r["mean_full"], stds=r["stds"], gradient=gradient,
                                purpose=purpose, note=note, generation=r["generation"],
                                remesh=r["event"] is not None, quality=r["quality"],
                                newton_iterations=r["newton_iterations"]))
        return rec

    def validate(self, u, note="", prediction=None):
        x = self.problem.x_of(u)
        key = x.tobytes()
        if key in self._val:
            return self._val[key]
        r = self._request("validate", "validation", x_bits=to_bits(x), note=note)
        if r["validator_msh_sha256"] != self.anchor.mesh_sha256 \
                or r["anchor_msh_sha256"] != self.anchor.mesh_sha256:
            raise WiringError("validation was not on the anchor's θ0 mesh")
        flags, values = check_acceptance(self.anchor, r["F_val"], r["mean_full"],
                                         r["acceptance"])
        stamp = self._stamp()
        credited = self.ledger.total_fe() <= self.ledger.budget_fe
        rec = dict(stamp, index=len(self.validations), x=x.tolist(), note=note,
                   F_val=r["F_val"], mean_full=r["mean_full"], stds=r["stds"],
                   fractions=r["fractions"], attained=flags, credited=credited,
                   mesh=r["mesh"], msh_sha256=r["msh_sha256"], **values)
        train = self._train.get(key)
        if train is not None:
            rec["F_train"] = train["F"]
            rec["mean_train"] = train["mean_full"]
        self.validations.append(rec)
        self._val[key] = rec
        self.events.append(dict(stamp, kind="validate", x=x.tolist(), F_val=r["F_val"],
                                mean_full=r["mean_full"], attained=flags, credited=credited))
        if prediction is not None:
            k = r["fractions"].index(0.5)
            s_pred, m_pred, f_pred = prediction
            self.assessment["validation"].append({
                "validation": rec["index"], "std_half_pred": s_pred[k],
                "std_half_val": r["stds"][k],
                "rel_std_half": rel(s_pred[k], r["stds"][k]),
                "F_pred": f_pred, "F_val": r["F_val"], "rel_F": rel(f_pred, r["F_val"]),
                "mean_pred": m_pred, "mean_val": r["mean_full"],
                "rel_mean": rel(m_pred, r["mean_full"])})
        if credited:
            for name in TARGET_NAMES:
                if name not in self.attained and flags[name]:
                    self.attained[name] = dict(stamp, validation=rec["index"], x=x.tolist(),
                                               F_val=r["F_val"], mean_full=r["mean_full"])
        if "T99" in self.attained:
            raise TargetReached()
        if not credited:
            raise BudgetExhausted()
        return rec

    def accept_iterate(self, u, note, prediction=None):
        """The validation policy: validate a newly accepted iterate."""
        return self.validate(u, note=note, prediction=prediction)

    # ── record ──────────────────────────────────────────────────────────

    def result(self):
        credited = [v for v in self.validations if v["credited"]]
        feasible = [v for v in credited if v["mean_ok"]]
        best = min(feasible, key=lambda v: v["F_val"], default=None)
        return {
            "attainment": {
                name: (dict(self.attained[name], attained=True, censored=False)
                       if name in self.attained else {"attained": False, "censored": True})
                for name in TARGET_NAMES},
            "validations": self.validations,
            "best_validated_feasible": best,
            "feasible_validated": bool(feasible),
            "infeasible": not feasible,
            "events": self.events,
            "assessment": self.assessment if self.method == "surrogate" else None,
            "ledger": self.ledger.to_json(),
            "anchor": dict(self.anchor.to_json(), file_values=self.anchor_file_values),
        }


def ulp_close(a, b, ulps=4):
    """|a - b| <= ulps * eps * |b| (serde_json's float parse is within an
    ulp or so of the correctly rounded value)."""
    return isinstance(a, float) and abs(a - b) <= ulps * 2.220446049250313e-16 * abs(b)


def rel(pred, actual):
    return abs(pred - actual) / abs(actual) if actual != 0 else math.inf


# ── A: SLSQP with the adjoint gradient ──────────────────────────────────

def slsqp_callables(run):
    """The SLSQP objective, constraint, their gradients and the callback.

    Every function evaluation requests the gradient (a line-search trial
    is usually accepted, and then needs it). The first new point after each
    gradient use is an `iterate`; later new points before the next gradient
    use are `line_search` trials."""
    P = run.problem
    state = {"after_gradient": True}

    def rec(u):
        purpose = "iterate" if state["after_gradient"] else "line_search"
        if P.x_of(u).tobytes() not in run._train:
            state["after_gradient"] = False
        return run.train(u, gradient=True, purpose=purpose, note="slsqp")

    def fun(u):
        return P.objective(rec(u))

    def jac(u):
        r = rec(u)
        state["after_gradient"] = True
        return r["dF"] * P.width / P.f_ref

    def con(u):
        return P.constraint(rec(u))

    def con_jac(u):
        r = rec(u)
        state["after_gradient"] = True
        return r["dmean_full"] * P.width / P.mean_ref

    def callback(intermediate_result):
        run.accept_iterate(intermediate_result.x, note="slsqp iterate")

    return fun, jac, con, con_jac, callback


def run_slsqp(run, u0):
    """SLSQP with the adjoint gradients (`slsqp_callables`)."""
    fun, jac, con, con_jac, callback = slsqp_callables(run)
    res = minimize(fun, u0, jac=jac, method="SLSQP", bounds=Bounds(0.0, 1.0),
                   constraints=[{"type": "ineq", "fun": con, "jac": con_jac}],
                   callback=callback, options={"ftol": 1e-15, "maxiter": 100000})
    run.accept_iterate(res.x, note="slsqp final")
    return f"slsqp: {res.message}"


# ── B: COBYQA ────────────────────────────────────────────────────────────

#: Initial trust radius in the unit box (matched with method C's Δ0).
COBYQA_RADIUS0 = 0.1


def run_cobyqa(run, u0):
    """COBYQA (scipy >= 1.14), forward values only. Its default
    interpolation set has 2n + 1 points."""
    P = run.problem

    def fun(u):
        return P.objective(run.train(u, gradient=False, note="cobyqa"))

    def con(u):
        return P.constraint(run.train(u, gradient=False, note="cobyqa"))

    def callback(intermediate_result):
        run.accept_iterate(intermediate_result.x, note="cobyqa iterate")

    res = minimize(fun, u0, method="COBYQA", bounds=Bounds(0.0, 1.0),
                   constraints=NonlinearConstraint(con, 0.0, np.inf), callback=callback,
                   options={"initial_tr_radius": COBYQA_RADIUS0, "final_tr_radius": 1e-6,
                            "maxfev": 10 ** 9, "maxiter": 10 ** 9, "scale": False})
    run.accept_iterate(res.x, note="cobyqa final")
    return f"cobyqa: {res.message}"


# ── C: the calibrated RBF trust-region surrogate (proposed amendment 4) ─

SURROGATE = {"delta0": 0.1, "delta_max": 0.5, "delta_min": 1e-6, "eta_good": 0.75,
             "expand_at": 0.9, "duplicate_tol": 1e-3}


def violation(P, rec):
    return max(0.0, -P.constraint(rec))


def better(P, a, b):
    """Feasible-first ordering: lower violation, then lower F."""
    va, vb = violation(P, a), violation(P, b)
    if va == 0.0 and vb == 0.0:
        return a["F"] < b["F"]
    return va < vb


def run_surrogate(run, u0, opts=SURROGATE):
    P = run.problem
    n = P.n
    pts = []  # training records, in evaluation order (no duplicates)
    seen = set()
    sq = math.sqrt(P.f_ref)

    def add(u, note):
        r = run.train(u, gradient=False, note=note)
        k = r["x"].tobytes()
        if k not in seen:
            seen.add(k)
            pts.append(r)
        return r

    def fit():
        U = np.array([p["u"] for p in pts])
        S = np.array([[s / sq for s in p["stds"]] for p in pts])
        M = np.array([p["mean_full"] / P.mean_ref for p in pts])
        return Models(U, S, M)

    def physical(models, u):
        s, m = models.predict(u)
        return [v * sq for v in s], m * P.mean_ref, models.f(u) * P.f_ref

    def held_out(models, u, r, note):
        s_pred, m_pred, f_pred = physical(models, u)
        run.assessment["held_out_training"].append({
            "note": note, "F_pred": f_pred, "F_train": r["F"], "rel_F": rel(f_pred, r["F"]),
            "stds_pred": s_pred, "stds_train": r["stds"],
            "mean_pred": m_pred, "mean_train": r["mean_full"],
            "rel_mean": rel(m_pred, r["mean_full"])})

    def subproblem(models, uk, delta):
        lb = np.maximum(0.0, uk - delta)
        ub = np.minimum(1.0, uk + delta)
        res = minimize(models.f, uk, jac=models.f_grad, method="SLSQP", bounds=Bounds(lb, ub),
                       constraints=[{"type": "ineq",
                                     "fun": lambda u: models.mean(u) - P.floor_ratio,
                                     "jac": models.mean.grad}],
                       options={"maxiter": 200, "ftol": 1e-14})
        return np.minimum(np.maximum(res.x, lb), ub)

    def geometry_point(uk, delta, j):
        for t in range(n):
            i = (j + t) % n
            u = uk.copy()
            u[i] = uk[i] + delta if uk[i] + delta <= 1.0 else uk[i] - delta
            if P.x_of(u).tobytes() not in seen:
                return u, i + 1
        return None, j

    # Calibration: the start and n coordinate steps of Δ0, then the fit.
    d0 = opts["delta0"]
    inc = add(np.asarray(u0, dtype=float), "doe start")
    for i in range(n):
        u = inc["u"].copy()
        u[i] = u[i] + d0 if u[i] + d0 <= 1.0 else u[i] - d0
        add(u, f"doe {i}")
    models = run.ledger.time("setup_calibration", "surrogate doe fit", fit)
    for p in pts:
        if better(P, p, inc):
            inc = p
    if inc is not pts[0]:
        run.accept_iterate(inc["u"], note="surrogate doe incumbent",
                           prediction=physical(models, inc["u"]))
    delta = d0
    rejections = 0
    gj = 0
    while True:
        if delta < opts["delta_min"]:
            return "surrogate: trust radius below delta_min"
        uk = inc["u"]
        cand = run.ledger.time("surrogate_update", "trust-region subproblem",
                               lambda: subproblem(models, uk, delta))
        dist = min(float(np.max(np.abs(cand - p["u"]))) for p in pts)
        geometry = dist < opts["duplicate_tol"] * delta
        if geometry:
            cand, gj = geometry_point(uk, delta, gj)
            if cand is None:
                delta *= 0.5
                continue
        f_hat_k = models.f(uk)
        f_hat_c = models.f(cand)
        r = add(cand, "surrogate geometry" if geometry else "surrogate candidate")
        held_out(models, cand, r, "geometry" if geometry else "candidate")
        models = run.ledger.time("surrogate_update", "surrogate refit", fit)
        if better(P, r, inc):
            step = float(np.max(np.abs(cand - uk)))
            pred = f_hat_k - f_hat_c
            rho = (inc["F"] - r["F"]) / P.f_ref / pred if pred > 0 else -math.inf
            if (rho >= opts["eta_good"] or violation(P, inc) > 0) \
                    and step >= opts["expand_at"] * delta:
                delta = min(2.0 * delta, opts["delta_max"])
            inc = r
            rejections = 0
            run.accept_iterate(inc["u"], note="surrogate acceptance",
                               prediction=physical(models, inc["u"]))
        elif not geometry:
            delta *= 0.5
            rejections += 1
            if rejections % 2 == 0:
                g, gj = geometry_point(inc["u"], delta, gj)
                if g is not None:
                    rg = add(g, "surrogate geometry")
                    held_out(models, g, rg, "geometry")
                    models = run.ledger.time("surrogate_update", "surrogate refit", fit)
                    if better(P, rg, inc):
                        inc = rg
                        run.accept_iterate(inc["u"], note="surrogate acceptance",
                                           prediction=physical(models, inc["u"]))


RUNNERS = {"slsqp": run_slsqp, "cobyqa": run_cobyqa, "surrogate": run_surrogate}


def execute(run, u0):
    """Run the method; return (stop_reason, message, error)."""
    try:
        msg = RUNNERS[run.method](run, np.asarray(u0, dtype=float))
        return "converged", msg, None
    except TargetReached:
        return "T99", "T99 attained", None
    except BudgetExhausted:
        return "budget", "wall-time budget spent", None
    except EvaluatorFailure as e:
        return "failed", "evaluator failure", str(e)
