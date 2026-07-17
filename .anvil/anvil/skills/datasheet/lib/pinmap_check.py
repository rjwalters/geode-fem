"""Pin-map integrity checker for the ``anvil:datasheet`` skill.

Deterministic pre-flight (issue #418, canary recommendation 2): the studio
canary found two pins double-assigned (power AND a MIPI differential pair)
while two others sat unassigned — errors invisible to a prose read of the
pinout table. This module makes the check mechanical.

The drafter wraps the pinout table rows in marker comments::

    % anvil-pinmap-begin package=QFN48 pins=48
    1 & VDD\\_CORE & P & Core supply \\\\
    2 & VSS & G & Ground \\\\
    ...
    % anvil-pinmap-end

``check_pinmap(tex_source)`` parses the rows between the markers and asserts:

- **every pin designator is assigned exactly once** (a duplicate designator is
  a ``double-assigned`` violation);
- when the begin-marker declares ``pins=<N>`` and the designators are numeric,
  **every designator 1..N is assigned** (a missing designator is an
  ``unassigned`` violation); for non-numeric designators (BGA balls), the
  unique-designator count is compared against ``N`` (``count-mismatch``).

Graceful degradation: when no markers are present, the result has
``found=False`` and ``passed=True`` — the mechanical check is inactive and the
human critic reviews the pinout manually. (The drafter is REQUIRED to emit the
markers, so marker absence on a skill-authored sheet is itself a review
finding — but not this module's concern.)

Skill-local per the lib-promotion rule ("wait for the second consumer before
generalizing"). Pure stdlib — no third-party deps.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

__all__ = [
    "PinRow",
    "PinmapViolation",
    "PinmapResult",
    "check_pinmap",
]

_BEGIN_RE = re.compile(
    r"^\s*%\s*anvil-pinmap-begin\b(?P<attrs>[^\n]*)$", re.MULTILINE
)
_END_RE = re.compile(r"^\s*%\s*anvil-pinmap-end\b", re.MULTILINE)
_ATTR_RE = re.compile(r"(?P<key>[A-Za-z_][\w-]*)=(?P<value>\S+)")


@dataclass(frozen=True)
class PinRow:
    """One parsed pinout-table row."""

    pin: str
    signal: str
    line_no: int


@dataclass(frozen=True)
class PinmapViolation:
    """One pin-map integrity violation.

    ``kind`` is one of ``"double-assigned"``, ``"unassigned"``,
    ``"count-mismatch"``.
    """

    kind: str
    message: str


@dataclass
class PinmapResult:
    """Outcome of a pin-map integrity check.

    ``found`` is ``False`` when no marker block exists (check inactive —
    ``passed`` is ``True`` by graceful degradation). ``passed`` is ``True``
    iff no violations were recorded.
    """

    found: bool
    package: str | None = None
    declared_pins: int | None = None
    rows: list[PinRow] = field(default_factory=list)
    violations: list[PinmapViolation] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        return not self.violations

    def to_dict(self) -> dict:
        """JSON-serializable shape for ``_gate.json`` / findings payloads."""
        return {
            "found": self.found,
            "package": self.package,
            "declared_pins": self.declared_pins,
            "assigned_pins": len({r.pin for r in self.rows}),
            "rows": len(self.rows),
            "violations": [
                {"kind": v.kind, "message": v.message} for v in self.violations
            ],
            "passed": self.passed,
        }


def _parse_attrs(attr_text: str) -> dict[str, str]:
    return {
        m.group("key"): m.group("value") for m in _ATTR_RE.finditer(attr_text)
    }


def _strip_tex(cell: str) -> str:
    """Normalize a table cell: trim, drop trailing ``\\\\`` row terminators."""
    return cell.replace("\\\\", "").strip()


def check_pinmap(tex_source: str) -> PinmapResult:
    """Check pin-map integrity of the marker-delimited pinout block.

    Parameters
    ----------
    tex_source:
        Full text of ``datasheet.tex``.

    Returns
    -------
    PinmapResult
        With ``found=False`` (inactive, passing) when no marker block exists.
    """
    begin = _BEGIN_RE.search(tex_source)
    if begin is None:
        return PinmapResult(found=False)

    end = _END_RE.search(tex_source, begin.end())
    if end is None:
        return PinmapResult(
            found=True,
            violations=[
                PinmapViolation(
                    kind="count-mismatch",
                    message=(
                        "anvil-pinmap-begin marker has no matching "
                        "anvil-pinmap-end marker"
                    ),
                )
            ],
        )

    attrs = _parse_attrs(begin.group("attrs"))
    package = attrs.get("package")
    declared_pins: int | None = None
    if "pins" in attrs:
        try:
            declared_pins = int(attrs["pins"])
        except ValueError:
            declared_pins = None

    block = tex_source[begin.end() : end.start()]
    base_line = tex_source.count("\n", 0, begin.end()) + 1

    rows: list[PinRow] = []
    for offset, raw_line in enumerate(block.splitlines()):
        line = raw_line.strip()
        if not line or line.startswith("%") or line.startswith("\\"):
            # Blank lines, comments, and LaTeX commands (\midrule etc.) are
            # not pin rows.
            continue
        cells = line.split("&")
        if len(cells) < 2:
            continue
        pin = _strip_tex(cells[0])
        signal = _strip_tex(cells[1])
        if not pin:
            continue
        rows.append(PinRow(pin=pin, signal=signal, line_no=base_line + offset))

    violations: list[PinmapViolation] = []

    # (a) Every pin designator assigned exactly once.
    seen: dict[str, PinRow] = {}
    for row in rows:
        if row.pin in seen:
            first = seen[row.pin]
            violations.append(
                PinmapViolation(
                    kind="double-assigned",
                    message=(
                        f"pin {row.pin} assigned more than once: "
                        f"{first.signal!r} (line {first.line_no}) and "
                        f"{row.signal!r} (line {row.line_no})"
                    ),
                )
            )
        else:
            seen[row.pin] = row

    # (b) Declared pin count coverage.
    if declared_pins is not None:
        unique = set(seen)
        if all(p.isdigit() for p in unique) and unique:
            expected = {str(i) for i in range(1, declared_pins + 1)}
            missing = sorted(expected - unique, key=int)
            if missing:
                violations.append(
                    PinmapViolation(
                        kind="unassigned",
                        message=(
                            f"{len(missing)} of {declared_pins} pins "
                            f"unassigned: {', '.join(missing)}"
                        ),
                    )
                )
            extra = sorted(
                (p for p in unique if int(p) > declared_pins or int(p) < 1),
                key=int,
            )
            if extra:
                violations.append(
                    PinmapViolation(
                        kind="count-mismatch",
                        message=(
                            f"pin designators outside 1..{declared_pins}: "
                            f"{', '.join(extra)}"
                        ),
                    )
                )
        elif len(unique) != declared_pins:
            # Non-numeric designators (BGA balls): compare counts only.
            violations.append(
                PinmapViolation(
                    kind="count-mismatch",
                    message=(
                        f"declared pins={declared_pins} but "
                        f"{len(unique)} unique designators assigned"
                    ),
                )
            )

    return PinmapResult(
        found=True,
        package=package,
        declared_pins=declared_pins,
        rows=rows,
        violations=violations,
    )
