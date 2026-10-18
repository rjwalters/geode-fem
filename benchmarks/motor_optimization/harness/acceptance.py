"""The acceptance rule of #1185 amendment 2, section 2, on the driver side.

The evaluator applies the rule (`Validator::accept`, in Rust) and replies
every value behind it. The driver applies it again, independently, from
the reply's F_val and full-current mean and the anchor it loaded itself, and
refuses a reply whose flags disagree. A first attainment is recorded only
from the driver's own decision, so a wiring error on either side stops the
run instead of producing a number.

- delta_F = max(0.10 F_val, 2.5e-7 N^2 m^2), at the candidate;
- target T in {T50, T90, T99} = {0.5, 0.1, 0.01} F0 is attained when
  F_val + delta_F <= T and mean_full >= (1 + 0.25%) mean0.
"""
import hashlib
import json
import math

DELTA_F_REL = 0.10
DELTA_F_ABS = 2.5e-7
MEAN_FLOOR_MARGIN = 0.0025
TARGETS = (("T50", 0.5), ("T90", 0.1), ("T99", 0.01))
TARGET_NAMES = tuple(n for n, _ in TARGETS)


class AcceptanceMismatch(Exception):
    """The evaluator's acceptance disagrees with the driver's."""


class Anchor:
    def __init__(self, f0, mean0, level, mesh_sha256, source=None, file_sha256=None):
        if not (math.isfinite(f0) and f0 > 0 and math.isfinite(mean0) and mean0 > 0):
            raise ValueError(f"anchor needs F0 > 0 and mean0 > 0, got {f0}, {mean0}")
        self.f0 = f0
        self.mean0 = mean0
        self.level = level
        self.mesh_sha256 = mesh_sha256
        self.source = source
        self.file_sha256 = file_sha256

    @classmethod
    def from_file(cls, path):
        with open(path, "rb") as f:
            raw = f.read()
        a = json.loads(raw)["anchor"]
        out = cls(a["F0"], a["mean0_full_current"], a["level"], a["msh_sha256"],
                  source=str(path), file_sha256=hashlib.sha256(raw).hexdigest())
        # The record's targets and floor must follow from F0 and mean0.
        for name, t in out.targets():
            if abs(a["targets"][name] - t) > 4 * 2.220446049250313e-16 * abs(t):
                raise ValueError(f"anchor targets.{name} = {a['targets'][name]} != {t}")
        if abs(a["mean_floor"] - out.mean_floor()) > 4 * 2.220446049250313e-16 * out.mean_floor():
            raise ValueError("anchor mean_floor disagrees with the rule")
        return out

    def targets(self):
        return [(name, frac * self.f0) for name, frac in TARGETS]

    def mean_floor(self):
        return self.mean0 * (1.0 + MEAN_FLOOR_MARGIN)

    def to_json(self):
        return {"F0": self.f0, "mean0_full_current": self.mean0, "level": self.level,
                "msh_sha256": self.mesh_sha256, "targets": dict(self.targets()),
                "mean_floor": self.mean_floor(), "source": self.source,
                "file_sha256": self.file_sha256}


def delta_f(f_val):
    if math.isnan(f_val):
        return math.nan
    return max(DELTA_F_REL * f_val, DELTA_F_ABS)


def rule(anchor, f_val, mean_full):
    """The rule on validation values: {name: attained}, and the values."""
    d = delta_f(f_val)
    upper = f_val + d
    mean_ok = mean_full >= anchor.mean_floor()
    flags = {}
    for name, t in anchor.targets():
        flags[name] = (upper <= t) and mean_ok
    return flags, {"delta_F": d, "F_upper": upper, "mean_ok": mean_ok,
                   "mean_floor": anchor.mean_floor()}


def check_acceptance(anchor, f_val, mean_full, reply):
    """The driver's decision, after checking it against the evaluator's
    acceptance `reply` (Acceptance::to_json). Raises AcceptanceMismatch."""
    flags, values = rule(anchor, f_val, mean_full)
    if reply.get("F_val") != f_val or reply.get("mean_full") != mean_full:
        raise AcceptanceMismatch("acceptance values are not the validation's")
    got = {t["name"]: t for t in reply.get("targets", [])}
    if set(got) != set(TARGET_NAMES):
        raise AcceptanceMismatch(f"targets {sorted(got)}")
    for name, t in anchor.targets():
        if got[name]["attained"] != flags[name]:
            raise AcceptanceMismatch(
                f"{name}: evaluator attained={got[name]['attained']}, driver {flags[name]} "
                f"(F_val {f_val!r}, mean {mean_full!r})")
        if got[name]["T"] != t:
            raise AcceptanceMismatch(f"{name}: evaluator T={got[name]['T']!r}, driver {t!r}")
    if reply.get("mean_ok") != values["mean_ok"]:
        raise AcceptanceMismatch("mean_ok disagrees")
    return flags, values
