"""The run ledger on the driver side (issue #1195; #1185 amendments 2 §3
and 3 §1).

The evaluator replies its charges per request (the entries of a
request-local `CostLedger`: category, wall seconds, solve counts). The
driver owns the totals: it adds the reply charges, its own timed work
(surrogate fits and updates), and the remainder of the wall clock as
`overhead` (process start-up, JSON I/O, ledger bookkeeping, the optimizer's
own iterations). `sync` keeps the invariant

    sum of all charged seconds == wall time since the run started,

so nothing is excluded (amendment 3 §1). The primary budget is wall time:
a run is exhausted when the charged seconds reach budget_fe x unit
(`FeRule::WallTime`). A request in flight at that moment is aborted and
the exact remainder is charged to its category, so an exhausted run's
total is the budget exactly.

Solve-count FE (secondary, never used to stop a run) weights each Newton
solve by the calibrated weights: a validation evaluation is 1, a training
forward and an adjoint their measured median over the unit.
"""
import math
import time

CATEGORIES = ("setup_calibration", "forward", "adjoint", "line_search", "mesh_motion",
              "remesh", "validation", "surrogate_update", "geometry_check", "overhead")


class BudgetExhausted(Exception):
    """The wall-time budget is spent."""


class Ledger:
    def __init__(self, unit_seconds, budget_fe, weights=None, clock=time.monotonic):
        if not (unit_seconds > 0 and math.isfinite(unit_seconds)):
            raise ValueError(f"unit {unit_seconds}")
        if not budget_fe > 0:
            raise ValueError(f"budget {budget_fe}")
        self.unit = unit_seconds
        self.budget_fe = budget_fe
        self.weights = weights or {}
        self.clock = clock
        self.t0 = clock()
        self.entries = []
        self._seconds = 0.0

    @property
    def budget_seconds(self):
        return self.budget_fe * self.unit

    def charge(self, category, seconds, forward_solves=0, adjoint_solves=0, note=""):
        if category not in CATEGORIES:
            raise ValueError(f"unknown category {category!r}")
        if not (math.isfinite(seconds) and seconds >= 0):
            raise ValueError(f"invalid charge {seconds} s to {category}")
        self.entries.append({"seq": len(self.entries), "category": category,
                             "seconds": seconds, "forward_solves": forward_solves,
                             "adjoint_solves": adjoint_solves, "note": note})
        self._seconds += seconds

    def charged_seconds(self):
        return self._seconds

    def elapsed(self):
        return self.clock() - self.t0

    def sync(self, note="driver"):
        """Charge the wall time no entry covers to `overhead`."""
        gap = self.elapsed() - self._seconds
        if gap > 0:
            self.charge("overhead", gap, note=note)

    def time(self, category, note, fn):
        """Run fn, charging its wall time to category (after syncing). The
        charges fn makes itself (requests it sends, e.g. the geometry
        requests of C's subproblem) keep their own categories; only the
        rest of its wall time goes to `category`, so nothing is charged
        twice."""
        self.sync("driver")
        t = self.clock()
        inner = self._seconds
        out = fn()
        own = (self.clock() - t) - (self._seconds - inner)
        self.charge(category, max(own, 0.0), note=note)
        return out

    def remaining_seconds(self):
        return self.budget_seconds - self._seconds

    def charge_to_budget(self, category, note):
        """Charge exactly the remaining budget (an aborted request)."""
        rest = self.remaining_seconds()
        if rest > 0:
            self.charge(category, rest, note=note)

    def total_fe(self):
        return self._seconds / self.unit

    def exhausted(self):
        return self._seconds >= self.budget_seconds

    def solve_fe(self):
        w = self.weights
        if not w:
            return None
        fe = 0.0
        for e in self.entries:
            c = e["category"]
            if c in ("forward", "line_search"):
                fe += e["forward_solves"] * w["train_forward_solve"]
            elif c == "adjoint":
                fe += e["adjoint_solves"] * w["adjoint_solve"]
            elif c == "validation":
                fe += e["forward_solves"] * w["validation_solve"]
        return fe

    def totals(self):
        out = {}
        for c in CATEGORIES:
            es = [e for e in self.entries if e["category"] == c]
            s = sum(e["seconds"] for e in es)
            out[c] = {"seconds": s, "fe": s / self.unit, "entries": len(es),
                      "forward_solves": sum(e["forward_solves"] for e in es),
                      "adjoint_solves": sum(e["adjoint_solves"] for e in es)}
        return out

    def to_json(self):
        return {"fe_rule": {"rule": "wall_time", "unit_seconds": self.unit},
                "budget_fe": self.budget_fe, "budget_seconds": self.budget_seconds,
                "total_fe": self.total_fe(), "total_seconds": self._seconds,
                "elapsed_seconds": self.elapsed(), "solve_fe": self.solve_fe(),
                "weights": self.weights, "exhausted": self.exhausted(),
                "totals": self.totals(), "entries": self.entries}
