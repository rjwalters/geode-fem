"""The experiment-2 optimizer driver (issues #1195 and #1201; #1185 with
amendments 1-7).

Three methods on one frozen space from one start, with matched budgets:

- A `slsqp`: safeguarded SLSQP with restarts (scipy SLSQP with the
  adjoint gradients of F_train and of the full-current training mean, and
  the geometry constraints h >= 0);
- B `cobyqa`: COBYQA (scipy 1.18.1), derivative-free, the same
  constraints, with the pinned NaN extreme barrier at a refused point;
- C `surrogate`: the calibrated RBF trust-region method of amendment 4.

Common to all three:

- Variables are the free components scaled to the unit box,
  u = (x - lower) / (upper - lower).
- Objective f(u) = F_train(x) / F_train(theta0); constraints
  g(u) = mean_train(x) / mean_train(theta0) - (1 + 0.25%) >= 0 and the
  geometry constraints h_i(x) = m_i(x) / s_i - 0.05 >= 0 (amendment 7 §1),
  with the theta0 training constants measured once per session.
- **Buildable and admissible (amendment 7 §1).** The driver asks the
  evaluator for the geometry domain of every point before anything else
  (`geometry`, charged `geometry_check`). Only a buildable point is ever
  sent to the mesher or the FEM solver. A buildable point with some
  h_i < 0 is evaluated normally; it is only constraint-infeasible, and it
  never earns credit.
- **Refused points (amendment 7 §§2-3).** A point that is not buildable, a
  point whose four fresh-mesh attempts all fail (mesh-infeasible), and a
  gradient request where the design derivative is not defined are
  *refused*: no FEM value or gradient exists for them, none is invented,
  and none is ever returned for another point. Each refusal is recorded
  with its design point and costs. Method by method: A aborts its SLSQP
  call, restores and restarts; B returns NaN (COBYQA's barrier); C skips the
  point.
- **Restoration (amendment 7 §2, adoption correction 1).** From the last
  admissible evaluated point x_v (h >= 0, every discrete guard; every
  evaluated point passed the training-mesh policy), toward the refused x:
  y_k = x_v + 2^-k (x - x_v), k = 1..20, the first y_k that is admissible
  (and, for a mesh-infeasible x, whose mesh passes the policy, with its
  repair attempts charged) is the restored point. None in 20:
  `restoration_failed`.
- Validation policy: every new accepted iterate is validated on the
  validation model and scored against the anchor; never the start, never a
  point twice, never a point that was not evaluated.
- The run stops at T99 attainment, when the wall-time budget is spent
  (the request in flight is aborted at the budget), when the method stops
  on its own, on `restoration_failed`, or on an evaluator failure. Every
  outcome is recorded.
"""
import math
import time

import numpy as np
from scipy.optimize import Bounds, NonlinearConstraint, minimize

from acceptance import (CREDIT_H_TOLERANCE, MEAN_FLOOR_MARGIN, TARGET_NAMES, Anchor,
                        check_acceptance)
from evaluator import Aborted, EvaluatorFailure, to_bits
from ledger import BudgetExhausted
from surrogate import Models

#: The (optimizer, resolution) method specs. `resolution` names the
#: training model a method's evaluator runs (`None`: the session's primary
#: training model, `session["primary_model"]`). Amendment 8 adds D
#: (COBYQA on L0) and E (safeguarded SLSQP on L0) here, as
#: {"letter": "D", "optimizer": "cobyqa", "resolution": "L0"}, with an
#: "L0" record in the session's `models`.
METHOD_SPECS = {
    "slsqp": {"letter": "A", "optimizer": "slsqp", "resolution": None,
              "label": "safeguarded SLSQP with restarts (adjoint)"},
    "cobyqa": {"letter": "B", "optimizer": "cobyqa", "resolution": None,
               "label": "derivative-free trust region (COBYQA)"},
    "surrogate": {"letter": "C", "optimizer": "surrogate", "resolution": None,
                  "label": "empirical RBF surrogate (trust region)"},
}
METHODS = tuple(METHOD_SPECS)
#: How each method is described in every record (#1185 amendment 6 §1:
#: C is an empirical surrogate, not a reduced physical or waveform model).
METHOD_LABELS = {m: s["label"] for m, s in METHOD_SPECS.items()}
#: Held-out currents of C's post-run diagnostic (amendment 6 §3).
DIAGNOSTIC_FRACTIONS = (0.25, 0.75)
#: Method letters of the ordering rule (amendment 3 §4: ABC, then CBA).
LETTERS = {s["letter"]: m for m, s in METHOD_SPECS.items()}
#: Restoration (amendment 7 §2): at most 20 bisections per restoration and
#: 20 restarts per run.
MAX_BISECTIONS = 20
MAX_RESTARTS = 20
#: The interim checkpoint (amendment 7 §4): validations completed by 60 FE.
CHECKPOINT_FE = 60.0


class TargetReached(Exception):
    """T99 was attained (the run's stopping target)."""


class WiringError(Exception):
    """A reply that the protocol forbids (wrong mesh, wrong anchor)."""


class Refused(Exception):
    """The method asked for a point that may not be evaluated (module
    docs): `kind` is `not_buildable`, `mesh_infeasible` or
    `derivative_undefined`."""

    def __init__(self, u, x, kind):
        super().__init__(f"{kind} at {list(x)}")
        self.u = np.asarray(u, dtype=float)
        self.x = np.asarray(x, dtype=float)
        self.kind = kind


class RestorationFailed(Exception):
    """No admissible restored point (amendment 7 §2), too many restarts, or
    a method's initial set with no buildable step."""


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
    """One charged optimizer run: geometry, training and validation
    requests, the validation policy, refusals and restorations, attainment,
    and the trace."""

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
        # Method C only. "new_design_validation_model_checks" compare C at
        # its own training currents on new designs and the finer model;
        # they are not held-out-current tests (amendment 6 §2).
        self.assessment = {"held_out_training": [], "new_design_validation_model_checks": []}
        #: Method C's training records and final incumbent (for the
        #: post-run diagnostic).
        self.c_points = []
        self.c_incumbent = None
        #: Method C's trust-region steps: Δ before the step, whether the
        #: point was a geometry point, whether it was accepted.
        self.c_steps = []
        self._train = {}
        self._val = {}
        self._geo = {}
        #: Refused x bytes -> kind (a refused point is never requested again).
        self._refused = {}
        #: Every refusal and every restoration (amendment 7 §2).
        self.refusals = []
        self.restorations = []
        #: Method A's restarts: the rejected x, the restored y, the costs.
        self.restarts = []
        #: Method C's initial-set steps (amendment 7 §2: -σ and halvings).
        self.c_initial = []
        #: The last admissible evaluated point (u), the restoration anchor.
        self.anchor_u = None
        #: x bytes of the start (set by `execute`): never validated.
        self.start_key = None
        #: Category and op of the request in flight at a budget abort.
        self.aborted_request = None
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
        x_req = self._x_from_bits(fields.get("x_bits"))
        try:
            reply = self.ev.request(op, timeout=led.remaining_seconds(), **fields)
        except Aborted:
            led.charge_to_budget(category, note=f"{op} aborted at the budget")
            self.aborted_request = {"category": category, "op": op, "x": x_req}
            raise BudgetExhausted() from None
        except EvaluatorFailure as e:
            # A transport failure (the process died, a broken pipe, a reply
            # out of order): the request's design point is still recorded.
            led.sync(f"io {op} (transport failure)")
            self.events.append(dict(self._stamp(), kind="failed", op=op, x=x_req,
                                    error=str(e), detail={"transport": True}))
            raise
        for c in reply.get("charges", []):
            led.charge(c["category"], c["seconds"], c["forward_solves"], c["adjoint_solves"],
                       c.get("note") or op)
        led.sync(f"io {op}")
        if not reply.get("ok"):
            # The design point of every failed request is recorded (the
            # #1200 Judge's finding 2), with the evaluator's detail.
            self.events.append(dict(self._stamp(), kind="failed", op=op,
                                    x=self._x_from_bits(fields.get("x_bits")),
                                    error=reply.get("error"), detail=reply.get("detail")))
            raise EvaluatorFailure(op, reply.get("error"))
        if "x_bits" in fields and reply.get("x_bits") != fields["x_bits"]:
            raise WiringError(f"{op}: the evaluator evaluated another point")
        return reply

    @staticmethod
    def _x_from_bits(bits):
        if bits is None:
            return None
        from evaluator import from_bits
        return from_bits(bits)

    def _stamp(self):
        led = self.ledger
        return {"wall_fe": led.total_fe(), "solve_fe": led.solve_fe(),
                "seconds": led.charged_seconds()}

    def geometry(self, u):
        """The geometry domain at u (cached per point): buildable,
        admissible, h and dh/du (None where the derivative is not
        defined)."""
        x = self.problem.x_of(u)
        key = x.tobytes()
        hit = self._geo.get(key)
        if hit is not None:
            return hit
        r = self._request("geometry", "geometry_check", x_bits=to_bits(x))
        g = r["geometry"]
        rows = [gd["dh_dx"] for gd in g["guards"]]
        dh_du = None
        if all(row is not None for row in rows):
            dh_du = np.array(rows, dtype=float) * self.problem.width
        rec = {"buildable": g["buildable"], "admissible": g["admissible"],
               # The full evidence (amendment 7 §1): raw margins, scales, h
               # and the discrete guards, kept in every record that needs
               # admissibility to be checkable from the record alone.
               "detail": {"buildable": g["buildable"], "admissible": g["admissible"],
                          "construction_error": g["construction_error"],
                          "guards": [{k: gd.get(k) for k in ("name", "unit", "margin",
                                                               "scale", "c", "h", "pass")}
                                     for gd in g["guards"]],
                          "discrete": [{"name": gd["name"], "pass": gd["pass"]}
                                       for gd in g["discrete"]]},
               "h": np.array(g["h"], dtype=float), "dh_du": dh_du,
               "jacobian_defined": g["jacobian_defined"], "min_h": g["min_h"],
               "inadmissible_guards": g["inadmissible_guards"],
               "failed_guards": [gd["name"] for gd in g["guards"] if not gd["pass"]]
               + [gd["name"] for gd in g["discrete"] if not gd["pass"]],
               "construction_error": g["construction_error"]}
        if rec["admissible"] and not rec["buildable"]:
            raise WiringError("an admissible point that is not buildable")
        # Credit admissibility (the ruled tolerance), computed here from the
        # raw h and cross-checked against the evaluator's.
        rec["credit_admissible"] = credit_admissible(rec)
        if g.get("credit_admissible") is not rec["credit_admissible"]:
            raise WiringError("the evaluator's credit admissibility is not the driver's")
        rec["detail"]["credit_admissible"] = rec["credit_admissible"]
        self._geo[key] = rec
        return rec

    def _refuse(self, u, x, kind, **detail):
        key = x.tobytes()
        self._refused[key] = kind
        geo = self._geo.get(key)
        self.refusals.append(dict(self._stamp(), kind=kind, x=x.tolist(), method=self.method,
                                  geometry=geo["detail"] if geo else None, **detail))
        self.events.append(dict(self._stamp(), kind="refused", reason=kind, x=x.tolist()))
        raise Refused(u, x, kind)

    def train(self, u, gradient, purpose="iterate", note=""):
        x = self.problem.x_of(u)
        key = x.tobytes()
        hit = self._train.get(key)
        if hit is not None and (hit["dF"] is not None or not gradient):
            return hit
        if key in self._refused:
            raise Refused(u, x, self._refused[key])
        g = self.geometry(u)
        if not g["buildable"]:
            self._refuse(u, x, "not_buildable", failed_guards=g["failed_guards"],
                         construction_error=g["construction_error"], min_h=g["min_h"])
        if gradient and not g["jacobian_defined"]:
            self._refuse(u, x, "derivative_undefined", min_h=g["min_h"])
        category = "line_search" if purpose == "line_search" else "forward"
        r = self._request("train", category, x_bits=to_bits(x), gradient=gradient,
                          purpose=purpose, note=note)
        status = r.get("status", "ok")
        if status == "mesh_infeasible":
            self._refuse(u, x, "mesh_infeasible", attempts=r.get("attempts"),
                         remesh_meshes=r.get("remesh_meshes"), min_h=g["min_h"])
        if status == "derivative_undefined":
            self._refuse(u, x, "derivative_undefined", min_h=g["min_h"])
        if status != "ok":
            raise WiringError(f"train: unknown status {status!r}")
        rec = {"x": x, "u": self.problem.u_of(x), "F": r["F"], "mean_full": r["mean_full"],
               "stds": r["stds"], "means": r["means"], "fractions": r["fractions"],
               "dF": None if r["dF"] is None else np.array(r["dF"]),
               "dmean_full": None if r["dmean_full"] is None else np.array(r["dmean_full"]),
               "admissible": g["admissible"], "min_h": g["min_h"], "h": g["h"],
               "discrete_pass": all(d["pass"] for d in g["detail"]["discrete"])}
        self._train[key] = rec
        if g["admissible"]:
            # The restoration anchor: the last admissible evaluated point
            # (adoption correction 1). Its training mesh passed the policy.
            self.anchor_u = rec["u"].copy()
        self.events.append(dict(self._stamp(), kind="train", x=x.tolist(), F=r["F"],
                                mean_full=r["mean_full"], stds=r["stds"], gradient=gradient,
                                purpose=purpose, note=note, generation=r["generation"],
                                remesh=r["event"] is not None, quality=r["quality"],
                                newton_iterations=r["newton_iterations"],
                                admissible=g["admissible"], min_h=g["min_h"],
                                remesh_meshes=r.get("remesh_meshes") or [],
                                attempts=r.get("attempts")))
        return rec

    def mesh_check(self, u):
        """The training-mesh step alone at u (a restoration probe): True if
        the moved mesh, or a repaired fresh mesh, passes the policy."""
        x = self.problem.x_of(u)
        r = self._request("mesh_check", "remesh", x_bits=to_bits(x))
        self.events.append(dict(self._stamp(), kind="mesh_check", x=x.tolist(),
                                status=r["status"], remesh_meshes=r.get("remesh_meshes") or [],
                                attempts=r.get("attempts")))
        return r["status"] == "ok"

    def restore(self, refused, need_jacobian=False):
        """Restoration (module docs): the first admissible point y_k on the
        segment from the last admissible evaluated point toward the refused
        x, k = 1..20."""
        before = self._stamp()
        if self.anchor_u is None:
            raise RestorationFailed("no admissible evaluated point to restore from")
        xv = self.anchor_u
        ux = refused.u
        probes = []
        for k in range(1, MAX_BISECTIONS + 1):
            y = xv + 0.5 ** k * (ux - xv)
            y = self.problem.u_of(self.problem.x_of(y))
            g = self.geometry(y)
            ok = g["admissible"] and (g["jacobian_defined"] or not need_jacobian)
            mesh_ok = None
            if ok and refused.kind == "mesh_infeasible":
                mesh_ok = self.mesh_check(y)
                ok = mesh_ok
            probes.append({"k": k, "x": self.problem.x_of(y).tolist(),
                           "admissible": g["admissible"], "min_h": g["min_h"],
                           "mesh_ok": mesh_ok})
            if ok:
                rec = {"kind": refused.kind, "rejected_x": refused.x.tolist(),
                       "rejected_geometry": self.geometry(ux)["detail"],
                       "anchor_x": self.problem.x_of(xv).tolist(),
                       "anchor_geometry": self.geometry(xv)["detail"],
                       "restored_x": self.problem.x_of(y).tolist(),
                       "restored_geometry": g["detail"], "bisections": k,
                       "probes": probes, "before": before, "after": self._stamp(),
                       "cost_fe": self.ledger.total_fe() - before["wall_fe"]}
                self.restorations.append(rec)
                return y
        self.restorations.append({"kind": refused.kind, "rejected_x": refused.x.tolist(),
                                  "rejected_geometry": self.geometry(ux)["detail"],
                                  "anchor_x": self.problem.x_of(xv).tolist(),
                                  "anchor_geometry": self.geometry(xv)["detail"],
                                  "restored_x": None, "bisections": MAX_BISECTIONS,
                                  "probes": probes, "before": before, "after": self._stamp(),
                                  "cost_fe": self.ledger.total_fe() - before["wall_fe"]})
        raise RestorationFailed(f"no admissible point in {MAX_BISECTIONS} bisections toward "
                                f"the {refused.kind} x")

    def validate(self, u, note="", prediction=None):
        x = self.problem.x_of(u)
        key = x.tobytes()
        if key in self._val:
            return self._val[key]
        g = self.geometry(u)
        if not g["buildable"]:
            raise WiringError("validation of a point that is not buildable")
        r = self._request("validate", "validation", x_bits=to_bits(x), note=note)
        if r["validator_msh_sha256"] != self.anchor.mesh_sha256 \
                or r["anchor_msh_sha256"] != self.anchor.mesh_sha256:
            raise WiringError("validation was not on the anchor's θ0 mesh")
        stamp = self._stamp()
        credited = self.ledger.total_fe() <= self.ledger.budget_fe
        if r.get("status") == "mesh_infeasible":
            # No value on the validation mesh: no credit (admissibility
            # includes the mesh policy there).
            rec = dict(stamp, index=len(self.validations), x=x.tolist(), note=note,
                       status="mesh_infeasible", F_val=None, mean_full=None,
                       attained={n: False for n in TARGET_NAMES}, credited=credited,
                       admissible=False, mean_ok=False, attempts=r.get("attempts"),
                       geometry=g["detail"])
            self.validations.append(rec)
            self._val[key] = rec
            self.events.append(dict(stamp, kind="validate", x=x.tolist(),
                                    status="mesh_infeasible", credited=credited))
            if not credited:
                raise BudgetExhausted()
            return rec
        adm = r.get("admissibility", {}).get("credit_admissible")
        if adm is not g["credit_admissible"]:
            raise WiringError("the evaluator's admissibility is not the driver's")
        flags, values = check_acceptance(self.anchor, r["F_val"], r["mean_full"],
                                         r["acceptance"], g["credit_admissible"])
        rec = dict(stamp, index=len(self.validations), x=x.tolist(), note=note, status="ok",
                   F_val=r["F_val"], mean_full=r["mean_full"], stds=r["stds"],
                   fractions=r["fractions"], attained=flags, credited=credited,
                   mesh=r["mesh"], msh_sha256=r["msh_sha256"], min_h=g["min_h"],
                   h_nonnegative=g["admissible"], geometry=g["detail"], **values)
        train = self._train.get(key)
        if train is not None:
            rec["F_train"] = train["F"]
            rec["mean_train"] = train["mean_full"]
        self.validations.append(rec)
        self._val[key] = rec
        self.events.append(dict(stamp, kind="validate", x=x.tolist(), F_val=r["F_val"],
                                mean_full=r["mean_full"], attained=flags, credited=credited,
                                admissible=g["credit_admissible"]))
        if prediction is not None:
            k = r["fractions"].index(0.5)
            s_pred, m_pred, f_pred = prediction
            self.assessment["new_design_validation_model_checks"].append({
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
        """The validation policy (amendment 5 §1): validate a new incumbent,
        never the start, never a point twice, never a point that was not
        evaluated (a refused point has no value). Returns the validation
        record, or None when the point is not validated."""
        key = self.problem.x_of(u).tobytes()
        if key == self.start_key or key in self._val or key not in self._train:
            return None
        return self.validate(u, note=note, prediction=prediction)

    # ── record ──────────────────────────────────────────────────────────

    def checkpoint(self, fe=CHECKPOINT_FE):
        """Attainment at `fe` FE (amendment 7 §4): only validations that
        *completed* by `fe` count; work in flight at `fe` that finishes
        later earns nothing here."""
        out = {}
        for name in TARGET_NAMES:
            first = next((v for v in self.validations
                          if v["credited"] and v["wall_fe"] <= fe and v["attained"][name]),
                         None)
            out[name] = ({"attained": True, "wall_fe": first["wall_fe"],
                          "validation": first["index"]} if first is not None
                         else {"attained": False, "censored": True})
        return {"fe": fe, "rule": "validations completed by the checkpoint", "targets": out}

    def result(self):
        credited = [v for v in self.validations if v["credited"] and v["status"] == "ok"]
        # Best feasible requires admissibility (amendment 7, correction 2).
        feasible = [v for v in credited if v["mean_ok"] and v["admissible"]]
        best = min(feasible, key=lambda v: v["F_val"], default=None)
        return {
            "attainment": {
                name: (dict(self.attained[name], attained=True, censored=False)
                       if name in self.attained else {"attained": False, "censored": True})
                for name in TARGET_NAMES},
            "checkpoint_60": self.checkpoint(),
            "validations": self.validations,
            "best_validated_feasible": best,
            "feasible_validated": bool(feasible),
            "infeasible": not feasible,
            "events": self.events,
            "refusals": self.refusals,
            "restorations": self.restorations,
            "restarts": self.restarts if self.method == "slsqp" else None,
            "surrogate_initial_set": self.c_initial if self.method == "surrogate" else None,
            "method_label": METHOD_LABELS[self.method],
            "method_spec": METHOD_SPECS[self.method],
            "surrogate_steps": self.c_steps if self.method == "surrogate" else None,
            "assessment": self.assessment if self.method == "surrogate" else None,
            "aborted_request": self.aborted_request,
            "ledger": self.ledger.to_json(),
            "anchor": dict(self.anchor.to_json(), file_values=self.anchor_file_values),
        }


def credit_admissible(g):
    """Buildable, every discrete guard, and every h_i >= -CREDIT_H_TOLERANCE
    (the admissibility of credit and of the best feasible point)."""
    return bool(g["buildable"] and all(d["pass"] for d in g["detail"]["discrete"])
                and all(h >= -CREDIT_H_TOLERANCE for h in g["h"]))


def ulp_close(a, b, ulps=4):
    """|a - b| <= ulps * eps * |b| (serde_json's float parse is within an
    ulp or so of the correctly rounded value)."""
    return isinstance(a, float) and abs(a - b) <= ulps * 2.220446049250313e-16 * abs(b)


def rel(pred, actual):
    return abs(pred - actual) / abs(actual) if actual != 0 else math.inf


# ── A: safeguarded SLSQP with restarts ──────────────────────────────────

def slsqp_callables(run):
    """The SLSQP objective, constraints (mean floor and geometry), their
    gradients and the callback.

    Every function evaluation requests the gradient (a line-search trial
    is usually accepted, and then needs it). The first new point after each
    gradient use is an `iterate`; later new points before the next gradient
    use are `line_search` trials. A refused point raises `Refused`, which
    aborts the SLSQP call: SLSQP never sees a value there."""
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

    def geo(u):
        g = run.geometry(u)
        if not g["buildable"]:
            # The same refusal as a training request there (no FEM).
            rec(u)
        return g["h"]

    def geo_jac(u):
        g = run.geometry(u)
        if g["dh_du"] is None:
            rec(u)  # refused as derivative_undefined
            raise AssertionError("unreachable")
        return g["dh_du"]

    def callback(intermediate_result):
        run.accept_iterate(intermediate_result.x, note="slsqp iterate")

    return fun, jac, con, con_jac, geo, geo_jac, callback


def run_slsqp(run, u0):
    """Safeguarded SLSQP with restarts (amendment 7 §2 A): a refused x
    aborts the `minimize` call; restoration finds y; SLSQP restarts at the
    actual y with fresh state. More than 20 restarts: `restoration_failed`."""
    u = np.asarray(u0, dtype=float)
    while True:
        fun, jac, con, con_jac, geo, geo_jac, callback = slsqp_callables(run)
        try:
            res = minimize(fun, u, jac=jac, method="SLSQP", bounds=Bounds(0.0, 1.0),
                           constraints=[{"type": "ineq", "fun": con, "jac": con_jac},
                                        {"type": "ineq", "fun": geo, "jac": geo_jac}],
                           callback=callback, options={"ftol": 1e-15, "maxiter": 100000})
        except Refused as r:
            if len(run.restarts) >= MAX_RESTARTS:
                raise RestorationFailed(f"more than {MAX_RESTARTS} restarts") from None
            before = run._stamp()
            y = run.restore(r, need_jacobian=True)
            run.restarts.append({"restart": len(run.restarts) + 1, "kind": r.kind,
                                 "rejected_x": r.x.tolist(),
                                 "restored_x": run.problem.x_of(y).tolist(),
                                 "restoration": len(run.restorations) - 1,
                                 "before": before, "after": run._stamp()})
            run.events.append(dict(run._stamp(), kind="restart", restart=len(run.restarts),
                                   x=run.problem.x_of(y).tolist()))
            u = y
            continue
        run.accept_iterate(res.x, note="slsqp final")
        return f"slsqp: {res.message} after {len(run.restarts)} restarts"


# ── B: COBYQA with the NaN barrier ──────────────────────────────────────

#: Initial trust radius in the unit box (matched with method C's Δ0).
COBYQA_RADIUS0 = 0.1


def COBYQA_INITIAL(n):  # noqa: N802 - a constant of the method
    """COBYQA's default initial interpolation set: 2n + 1 points."""
    return 2 * n + 1


def cobyqa_callables(run):
    """COBYQA's objective, constraints and callback (`run_cobyqa`), and
    the set of points it has asked for."""
    P = run.problem
    seen = set()

    def evaluate(u):
        seen.add(P.x_of(u).tobytes())
        try:
            return run.train(u, gradient=False, note="cobyqa")
        except Refused:
            return None

    def fun(u):
        r = evaluate(u)
        return math.nan if r is None else P.objective(r)

    def cons(u):
        r = evaluate(u)
        m = math.nan if r is None else P.constraint(r)
        return np.concatenate([[m], run.geometry(u)["h"]])

    def callback(intermediate_result):
        # COBYQA calls this after every evaluation with its best point so
        # far (scipy 1.18.1). Amendment 5 §1: nothing during the 2n + 1
        # initial interpolation set (refused points included); then each
        # change of the best point.
        if len(seen) < COBYQA_INITIAL(P.n):
            return
        run.accept_iterate(intermediate_result.x, note="cobyqa iterate")

    return fun, cons, callback, seen


def run_cobyqa(run, u0):
    """COBYQA (scipy 1.18.1), forward values only. A refused point is never
    evaluated: the objective and the mean constraint are NaN there, which
    the pinned scipy turns into its extreme barrier (cobyqa `problem.py`);
    the geometry constraints are their real values (geometry only, no FEM).
    The run's result is read from validated attainment only, never from
    the solver's success flag: with NaN barriers COBYQA can report success
    at a non-optimal point.

    Initial set (frozen behaviour): its 2n + 1 points are COBYQA's own; a
    refused one counts as an evaluation of the set, gets the barrier, and
    COBYQA proceeds with the rest."""
    fun, cons, callback, _ = cobyqa_callables(run)
    res = minimize(fun, u0, method="COBYQA", bounds=Bounds(0.0, 1.0),
                   constraints=NonlinearConstraint(cons, 0.0, np.inf), callback=callback,
                   options={"initial_tr_radius": COBYQA_RADIUS0, "final_tr_radius": 1e-6,
                            "maxfev": 10 ** 9, "maxiter": 10 ** 9, "scale": False})
    run.accept_iterate(res.x, note="cobyqa final")
    return f"cobyqa: {res.message} (not used for credit)"


# ── C: the calibrated RBF trust-region surrogate (amendment 4) ──────────

SURROGATE = {"delta0": 0.1, "delta_max": 0.5, "delta_min": 1e-6, "eta_good": 0.75,
             "expand_at": 0.9, "duplicate_tol": 1e-3}
#: Initial-set step halvings per direction (amendment 7 §2 C).
C_HALVINGS = 3


def violation(P, rec):
    """The total constraint violation of a training record (C's
    feasible-first ordering): the mean-floor violation plus
    sum_i max(0, -h_i) over the geometry guards; a failed discrete guard
    is infinitely infeasible. A buildable but inadmissible point is
    therefore never "feasible" here."""
    if not rec.get("discrete_pass", True):
        return math.inf
    h = np.asarray(rec["h"], dtype=float)
    return max(0.0, -P.constraint(rec)) + float(np.sum(np.maximum(0.0, -h)))


def better(P, a, b):
    """Feasible-first ordering: lower violation, then lower F."""
    va, vb = violation(P, a), violation(P, b)
    if va == 0.0 and vb == 0.0:
        return a["F"] < b["F"]
    return va < vb


def c_initial_steps(ui, d0):
    """The frozen order of C's initial-set steps along one coordinate
    (amendment 7 §2 C): for σ, σ/2, σ/4, σ/8 the primary sign (+ unless
    that leaves the box), then the other; steps outside the box are
    skipped."""
    out = []
    for h in range(C_HALVINGS + 1):
        s = d0 * 0.5 ** h
        primary = s if ui + s <= 1.0 else -s
        for step in (primary, -primary):
            if 0.0 <= ui + step <= 1.0:
                out.append(step)
    return out


def run_surrogate(run, u0, opts=SURROGATE):
    P = run.problem
    n = P.n
    pts = []  # training records, in evaluation order (no duplicates)
    seen = set()
    sq = math.sqrt(P.f_ref)

    def add(u, note):
        """Evaluate u (forward only); a refused point is skipped and never
        fitted: None."""
        try:
            r = run.train(u, gradient=False, note=note)
        except Refused:
            return None
        k = r["x"].tobytes()
        if k not in seen:
            seen.add(k)
            pts.append(r)
            run.c_points = pts
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

        def geo_jac(u):
            d = run.geometry(u)["dh_du"]
            if d is None:
                raise Refused(u, P.x_of(u), "derivative_undefined")
            return d

        try:
            res = minimize(models.f, uk, jac=models.f_grad, method="SLSQP",
                           bounds=Bounds(lb, ub),
                           constraints=[{"type": "ineq",
                                         "fun": lambda u: models.mean(u) - P.floor_ratio,
                                         "jac": models.mean.grad},
                                        {"type": "ineq", "fun": lambda u: run.geometry(u)["h"],
                                         "jac": geo_jac}],
                           options={"maxiter": 200, "ftol": 1e-14})
        except Refused:
            # The geometry derivative is undefined on the subproblem's path:
            # no step (the near-duplicate rule then takes a geometry point).
            return uk.copy()
        return np.minimum(np.maximum(res.x, lb), ub)

    def geometry_point(uk, delta, j):
        """u_k ± Δ e_j, j cycling, skipping evaluated and non-buildable
        points (the same buildability test as every candidate)."""
        for t in range(n):
            i = (j + t) % n
            u = uk.copy()
            u[i] = uk[i] + delta if uk[i] + delta <= 1.0 else uk[i] - delta
            key = P.x_of(u).tobytes()
            if key in seen or key in run._refused:
                continue
            if not run.geometry(u)["buildable"]:
                continue
            return u, i + 1
        return None, j

    # Calibration: the start and one buildable coordinate step per
    # coordinate (+σ, -σ, then halvings; amendment 7 §2 C), then the fit.
    d0 = opts["delta0"]
    inc = add(np.asarray(u0, dtype=float), "doe start")
    if inc is None:
        raise RestorationFailed("C: the start is refused")
    run.c_incumbent = inc
    for i in range(n):
        tried = []
        for step in c_initial_steps(inc["u"][i], d0):
            u = inc["u"].copy()
            u[i] = u[i] + step
            r = add(u, f"doe {i}")
            tried.append({"step": step, "evaluated": r is not None,
                          "refused": run._refused.get(P.x_of(u).tobytes())})
            if r is not None:
                break
        run.c_initial.append({"coordinate": i, "tried": tried})
        if not any(t["evaluated"] for t in tried):
            raise RestorationFailed(f"C initial set: coordinate {i} has no buildable step")
    models = run.ledger.time("setup_calibration", "surrogate doe fit", fit)
    for p in pts:
        if better(P, p, inc):
            inc = p
    run.c_incumbent = inc
    if inc is not pts[0]:
        run.accept_iterate(inc["u"], note="surrogate doe incumbent",
                           prediction=physical(models, inc["u"]))
    delta = d0
    rejections = 0
    gj = 0
    while True:
        if delta < opts["delta_min"]:
            # Amendment 5 §2 C2: the final incumbent under §1.
            run.accept_iterate(inc["u"], note="surrogate final",
                               prediction=physical(models, inc["u"]))
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
        if r is None:
            # A refused candidate is a rejection in every respect but the
            # fit (the #1185 ruling): never fitted; Δ halved, the counter
            # advanced, and it counts toward the geometry point below.
            run.c_steps.append({"delta": delta, "geometry": geometry, "accepted": False,
                                "refused": run._refused.get(P.x_of(cand).tobytes())})
            accepted = False
        else:
            held_out(models, cand, r, "geometry" if geometry else "candidate")
            models = run.ledger.time("surrogate_update", "surrogate refit", fit)
            accepted = bool(better(P, r, inc))
            run.c_steps.append({"delta": delta, "geometry": geometry, "accepted": accepted})
        if accepted:
            step = float(np.max(np.abs(cand - uk)))
            pred = f_hat_k - f_hat_c
            rho = (inc["F"] - r["F"]) / P.f_ref / pred if pred > 0 else -math.inf
            if (rho >= opts["eta_good"] or violation(P, inc) > 0) \
                    and step >= opts["expand_at"] * delta:
                delta = min(2.0 * delta, opts["delta_max"])
            inc = r
            run.c_incumbent = inc
            rejections = 0
            run.accept_iterate(inc["u"], note="surrogate acceptance",
                               prediction=physical(models, inc["u"]))
        else:
            # A rejection, including a non-improving geometry point that
            # replaced a near-duplicate (amendment 5 §2 C1) and a refused
            # candidate.
            delta *= 0.5
            rejections += 1
            if rejections % 2 == 0:
                g, gj = geometry_point(inc["u"], delta, gj)
                run.c_steps[-1]["geometry_point_after"] = None if g is None else \
                    P.x_of(g).tolist()
                if g is not None:
                    # geometry_point checked buildability; a mesh-infeasible
                    # one is skipped (never fitted).
                    rg = add(g, "surrogate geometry")
                    if rg is None:
                        continue
                    held_out(models, g, rg, "geometry")
                    models = run.ledger.time("surrogate_update", "surrogate refit", fit)
                    if better(P, rg, inc):
                        inc = rg
                        run.c_incumbent = inc
                        run.accept_iterate(inc["u"], note="surrogate acceptance",
                                           prediction=physical(models, inc["u"]))


def c_current_predictions(run, fractions=DIAGNOSTIC_FRACTIONS):
    """Method C's prediction at currents it was not trained on, at its final
    incumbent (amendment 6 §3). C interpolates per training current only
    (fractions 0, 0.5, 1), so its prediction at an intermediate fraction is
    the linear interpolation, in the fraction, of its per-current RBF
    predictions at the two neighbouring training fractions. The stds use
    the method's own std interpolants. The means at 0 and 0.5, which C
    does not model, are RBFs fitted post-run to the same training records;
    they never enter the optimization. Returns None without an incumbent."""
    from surrogate import CubicRBF
    inc, pts = run.c_incumbent, run.c_points
    if inc is None or not pts:
        return None
    U = np.array([p["u"] for p in pts])
    fr = list(pts[0]["fractions"])
    std_m = [CubicRBF(U, np.array([p["stds"][c] for p in pts])) for c in range(len(fr))]
    mean_m = [CubicRBF(U, np.array([p["means"][c] for p in pts])) for c in range(len(fr))]
    u = inc["u"]
    out = []
    for f in fractions:
        hi = next(i for i, g in enumerate(fr) if g >= f)
        lo = max(hi - 1, 0)
        w = 0.0 if fr[hi] == fr[lo] else (f - fr[lo]) / (fr[hi] - fr[lo])
        out.append({"fraction": f, "neighbours": [fr[lo], fr[hi]],
                    "std_pred": (1 - w) * std_m[lo](u) + w * std_m[hi](u),
                    "mean_pred": (1 - w) * mean_m[lo](u) + w * mean_m[hi](u)})
    return {"x": inc["x"].tolist(), "points": len(pts),
            "exact_fit": all(m.exact for m in std_m + mean_m), "predictions": out,
            "rule": "linear in the current fraction between C's per-current RBF "
                    "predictions at the neighbouring training fractions (0, 0.5, 1); "
                    "the means at 0 and 0.5 are post-run fits"}


RUNNERS = {"slsqp": run_slsqp, "cobyqa": run_cobyqa, "surrogate": run_surrogate}


def execute(run, u0):
    """Run the method; return (stop_reason, message, error). A budget
    abort is `budget` (with `run.aborted_request`); `failed` is only an
    error reply (amendment 5 §3); `restoration_failed` is amendment 7 §2.
    The start must be admissible (amendment 7 §5); a start that is not is
    a fault, and ends the run as `restoration_failed`."""
    run.start_key = run.problem.x_of(u0).tobytes()
    try:
        g = run.geometry(u0)
        if not g["admissible"]:
            raise RestorationFailed("the start is not admissible")
        msg = RUNNERS[METHOD_SPECS[run.method]["optimizer"]](run, np.asarray(u0, dtype=float))
        return "converged", msg, None
    except TargetReached:
        return "T99", "T99 attained", None
    except BudgetExhausted:
        return "budget", "wall-time budget spent", None
    except RestorationFailed as e:
        return "restoration_failed", "restoration failed", str(e)
    except Refused as e:
        # A refused start (the first request of every method is the start).
        return "restoration_failed", "the start was refused", str(e)
    except EvaluatorFailure as e:
        return "failed", "evaluator failure", str(e)
