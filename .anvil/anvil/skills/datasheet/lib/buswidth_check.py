"""Bus-width-vs-range sanity checker for the ``anvil:datasheet`` skill.

Deterministic pre-flight (issue #418, canary recommendation 2): the studio
canary found a 5-bit field claiming a 0–79 index range — a bus that cannot
represent its own stated value set (capacity 2^5 = 32). This module makes the
check mechanical.

The drafter annotates every N-bit field whose value range the sheet claims
with a marker comment adjacent to the claim::

    % anvil-bus: name=roi_index width=5 max=79
    % anvil-bus: name=ch_sel width=3 range=0-7
    % anvil-bus: name=layer_id width=6 values=64

Claim shapes (exactly one per declaration):

- ``max=<M>``     — the field must represent values up to ``M`` (0-based),
  i.e. ``M <= 2^width - 1``.
- ``range=<lo>-<hi>`` — the field must represent ``hi - lo + 1`` distinct
  values, i.e. ``hi - lo + 1 <= 2^width``.
- ``values=<count>``  — the field must represent ``count`` distinct values,
  i.e. ``count <= 2^width``.

Graceful degradation: when no markers are present, the result has
``found=False`` and ``passed=True`` — the mechanical check is inactive.

Skill-local per the lib-promotion rule. Pure stdlib — no third-party deps.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

__all__ = [
    "BusDeclaration",
    "BusViolation",
    "BuswidthResult",
    "bus_capacity",
    "check_bus",
    "check_buswidths",
]

_DECL_RE = re.compile(r"^\s*%\s*anvil-bus:\s*(?P<attrs>[^\n]*)$", re.MULTILINE)
_ATTR_RE = re.compile(r"(?P<key>[A-Za-z_][\w-]*)=(?P<value>\S+)")
_RANGE_RE = re.compile(r"^(?P<lo>-?\d+)\s*-\s*(?P<hi>-?\d+)$")


@dataclass(frozen=True)
class BusDeclaration:
    """One parsed ``% anvil-bus:`` declaration."""

    name: str
    width: int
    claimed_max: int | None
    claimed_values: int | None
    line_no: int


@dataclass(frozen=True)
class BusViolation:
    """One bus-width sanity violation (or a malformed declaration)."""

    name: str
    message: str


@dataclass
class BuswidthResult:
    """Outcome of a bus-width sanity check.

    ``found`` is ``False`` when no declarations exist (check inactive —
    ``passed`` is ``True`` by graceful degradation).
    """

    found: bool
    declarations: list[BusDeclaration] = field(default_factory=list)
    violations: list[BusViolation] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        return not self.violations

    def to_dict(self) -> dict:
        """JSON-serializable shape for ``_gate.json`` / findings payloads."""
        return {
            "found": self.found,
            "declarations": [
                {
                    "name": d.name,
                    "width": d.width,
                    "capacity": bus_capacity(d.width),
                    "claimed_max": d.claimed_max,
                    "claimed_values": d.claimed_values,
                }
                for d in self.declarations
            ],
            "violations": [
                {"name": v.name, "message": v.message} for v in self.violations
            ],
            "passed": self.passed,
        }


def bus_capacity(width: int) -> int:
    """Distinct values an ``width``-bit field can represent (``2**width``)."""
    if width < 0:
        raise ValueError(f"bus width must be non-negative, got {width}")
    return 2**width

def check_bus(
    width: int,
    *,
    max_value: int | None = None,
    value_count: int | None = None,
) -> bool:
    """Pure predicate: can an ``width``-bit field cover the claimed set?

    ``max_value`` is 0-based (a field claiming indices 0..79 has
    ``max_value=79``); ``value_count`` is a cardinality. Either (or both) may
    be given; the field must cover all that are.
    """
    capacity = bus_capacity(width)
    if max_value is not None and max_value > capacity - 1:
        return False
    if value_count is not None and value_count > capacity:
        return False
    return True


def check_buswidths(tex_source: str) -> BuswidthResult:
    """Check every ``% anvil-bus:`` declaration in ``tex_source``.

    Returns a :class:`BuswidthResult` with ``found=False`` (inactive,
    passing) when no declarations exist. Malformed declarations (missing
    ``name``/``width``, no claim attribute, unparseable numbers) are recorded
    as violations — a malformed integrity marker is itself a defect, not a
    silent skip.
    """
    declarations: list[BusDeclaration] = []
    violations: list[BusViolation] = []
    found = False

    for match in _DECL_RE.finditer(tex_source):
        found = True
        line_no = tex_source.count("\n", 0, match.start()) + 1
        attrs = {
            m.group("key"): m.group("value")
            for m in _ATTR_RE.finditer(match.group("attrs"))
        }
        name = attrs.get("name", f"<unnamed@line{line_no}>")

        try:
            width = int(attrs["width"])
            if width < 0:
                raise ValueError
        except (KeyError, ValueError):
            violations.append(
                BusViolation(
                    name=name,
                    message=(
                        f"line {line_no}: anvil-bus declaration {name!r} has "
                        f"a missing or invalid width={attrs.get('width')!r}"
                    ),
                )
            )
            continue

        claimed_max: int | None = None
        claimed_values: int | None = None
        try:
            if "max" in attrs:
                claimed_max = int(attrs["max"])
            if "range" in attrs:
                m = _RANGE_RE.match(attrs["range"])
                if m is None:
                    raise ValueError(attrs["range"])
                lo, hi = int(m.group("lo")), int(m.group("hi"))
                if hi < lo:
                    raise ValueError(attrs["range"])
                claimed_values = max(claimed_values or 0, hi - lo + 1)
            if "values" in attrs:
                claimed_values = max(claimed_values or 0, int(attrs["values"]))
        except ValueError as exc:
            violations.append(
                BusViolation(
                    name=name,
                    message=(
                        f"line {line_no}: anvil-bus declaration {name!r} has "
                        f"an unparseable claim attribute ({exc})"
                    ),
                )
            )
            continue

        if claimed_max is None and claimed_values is None:
            violations.append(
                BusViolation(
                    name=name,
                    message=(
                        f"line {line_no}: anvil-bus declaration {name!r} "
                        "carries no claim (need max=, range=, or values=)"
                    ),
                )
            )
            continue

        declarations.append(
            BusDeclaration(
                name=name,
                width=width,
                claimed_max=claimed_max,
                claimed_values=claimed_values,
                line_no=line_no,
            )
        )

        if not check_bus(
            width, max_value=claimed_max, value_count=claimed_values
        ):
            capacity = bus_capacity(width)
            claim_bits = []
            if claimed_max is not None:
                claim_bits.append(f"max={claimed_max}")
            if claimed_values is not None:
                claim_bits.append(f"values={claimed_values}")
            violations.append(
                BusViolation(
                    name=name,
                    message=(
                        f"line {line_no}: {width}-bit field {name!r} "
                        f"(capacity {capacity}) cannot represent its claimed "
                        f"set ({', '.join(claim_bits)})"
                    ),
                )
            )

    return BuswidthResult(
        found=found, declarations=declarations, violations=violations
    )
