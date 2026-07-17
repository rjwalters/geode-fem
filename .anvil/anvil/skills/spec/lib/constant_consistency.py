"""Cross-table named-constant consistency checker for the ``anvil:spec`` skill.

Deterministic pre-flight (epic #697, Phase 3 / issue #708): the botho canary
found a spec that stated the same normative quantity two different ways in two
different places — a **block-time floor of 3\\,s** in one section and **5\\,s**
in another — and a **ring-size / byte-count** figure that disagreed with itself
across sections. A prose read of either section in isolation looks fine; only a
cross-section comparison of "the same named thing" catches the drift. This
module makes that comparison mechanical, feeding the reviewer's dim-2
(*Internal consistency*) score and a review-side critical flag.

Marker convention (drafter-authored, adjacent to each constant's authoritative
statement — a one-liner in the ``% anvil-bus:`` family, not a begin/end block,
because constants are single values, not tabular spans)::

    % anvil-const: name=block_time_floor value=3 unit=s

The same marker is matched as an inline table-row-comment suffix, so the drafter
does not have to break table formatting to annotate the authoritative row::

    Block time floor & 3\\,s & \\S2.1 \\\\ % anvil-const: name=block_time_floor value=3 unit=s

``check_constant_consistency(tex_source)`` parses every ``% anvil-const:``
marker (and every ``\\newcommand`` — see below), groups declarations by
``name``, and flags:

- **value-mismatch** — two declarations of the same ``name`` with the same
  ``unit`` but different (string-normalized) ``value``. The botho block-time
  floor is exactly this shape.
- **unit-mismatch** — two declarations of the same ``name`` with different
  ``unit`` attrs (e.g. ``s`` vs ``ms``). This is a SEPARATE, lower-severity
  finding: ``3\\,s`` and ``3000\\,ms`` are not necessarily wrong, so this
  module never silently converts units or treats a unit difference as a value
  mismatch — a human resolves it.
- **malformed-declaration** — a marker missing ``name=`` or ``value=``. A broken
  integrity marker is itself a defect (mirrors ``buswidth_check.py``), not a
  silent skip.

``\\newcommand{\\X}{body}`` support (v1 scope, per #708 heuristic 2): a
``\\newcommand`` is treated as an *implicit* constant declaration
(``name=X value=<body>``). Because a macro used consistently never disagrees
with itself, the interesting case this catches is a **second, conflicting
``\\newcommand{\\X}{...}`` redefinition** of the same macro name with a
different body across the multi-file tree — a ``value-mismatch`` on the macro
name. Macro-vs-raw-literal drift (prose using ``3\\,s`` where ``\\X`` expands
to ``5\\,s``) is OUT of v1 scope: resolving what an arbitrary macro body means
numerically is a rabbit hole (see Deferred).

Graceful degradation: when no markers AND no ``\\newcommand`` definitions exist,
the result has ``found=False`` and ``passed=True`` — the mechanical check is
inactive. On a skill-authored spec the reviewer scores that as a dim-2 deduction
(the spec opted out of its own integrity check), not a hard failure.

Structural precedent
--------------------
This module deliberately mirrors ``anvil/skills/datasheet/lib/pinmap_check.py``
and ``anvil/skills/datasheet/lib/buswidth_check.py`` — pure-stdlib, marker-driven,
no file I/O inside the checker, frozen-dataclass results, a ``to_dict()`` for
``_gate.json``, and ``found``-based graceful degradation. The caller
(``spec-review.md``) owns file I/O and passes source strings in.

Adjacent-but-distinct
---------------------
``anvil/lib/numeric_consistency.py`` is a *different* check: it validates
arithmetic *claims* against candidate values in a paragraph window
("a stated 70-point spread must equal max - min"), body-internal and
prose-only. Its own docstring notes that the "same quantity stated twice
disagrees" check did NOT clear the false-positive bar with a keyword heuristic
and was left to LLM judgment (#462). This module solves precisely that gap for
spec — but sidesteps the false-positive problem by requiring **explicit
markers** rather than inferring "these two numbers are the same named thing"
from prose. It is NOT a second consumer of ``numeric_consistency.py``; do not
merge or extend that module for this. Skill-local per the lib-promotion rule
("wait for the second consumer before generalizing").

Deferred (v1 limits — mirroring the datasheet modules' scope discipline)
------------------------------------------------------------------------
- **No free-text / prose constant extraction** — markers only. A spec author
  writing "the block time floor is 3 seconds" in one paragraph and "must wait
  at least 5s between blocks" in another has no structural marker tying the two
  phrasings to one named constant; catching that is ``spec-audit`` judgment,
  not a deterministic gate.
- **No table-row auto-parsing** — a spec parameter table is heterogeneous
  prose-in-cells; the drafter annotates the authoritative row with an inline
  ``% anvil-const:`` suffix instead.
- **No cross-unit numeric conversion** — unit mismatches are flagged, never
  resolved.
- **``\\newcommand`` support covers duplicate-macro-definition conflicts only**,
  not macro-vs-raw-literal drift.
- **No semantic equivalence across differently-named constants** — the spec
  calling it ``block_time_floor`` in one marker and ``min_block_interval`` in
  another (no shared ``name=``) is invisible here; that is ``spec-audit``
  judgment.

Pure stdlib — no third-party deps.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

__all__ = [
    "ConstantDeclaration",
    "ConstantViolation",
    "ConstantConsistencyResult",
    "check_constant_consistency",
    "check_constant_consistency_multi",
    "normalize_value",
]

# A standalone-line OR inline-suffix marker: everything before the ``%`` is
# ignored (mirrors how pinmap's row parser ignores everything before ``%``), so
# the same regex matches a comment on its own line and a trailing table-row
# comment. NOT anchored to line-start-after-whitespace-only.
_MARKER_RE = re.compile(r"%\s*anvil-const:\s*(?P<attrs>[^\n]*)")
_ATTR_RE = re.compile(r"(?P<key>[A-Za-z_][\w-]*)=(?P<value>\S+)")

# \newcommand{\Name}{body}  /  \renewcommand{\Name}{body} — best-effort,
# single-brace body (nested braces in the body are not balanced here; a body
# with nested braces simply captures up to the first unmatched close, which is
# fine for the duplicate-definition-conflict use case since we compare the
# captured bodies string-for-string).
_NEWCOMMAND_RE = re.compile(
    r"\\(?:re)?newcommand\s*\{\s*\\(?P<macro>[A-Za-z@]+)\s*\}\s*"
    r"(?:\[\d+\])?\s*\{(?P<body>[^{}]*)\}"
)


@dataclass(frozen=True)
class ConstantDeclaration:
    """One parsed constant declaration (``% anvil-const:`` marker or
    ``\\newcommand``).

    ``value`` is kept as the raw token string; comparison is exact after
    :func:`normalize_value` (no numeric tolerance band — a normative constant
    either matches or is a defect). ``source`` labels the originating file for
    the multi-file spec shape (``None`` for single-source input).
    """

    name: str
    value: str
    unit: str | None
    line_no: int
    section: str | None = None
    source: str | None = None
    kind: str = "marker"  # "marker" | "newcommand"


@dataclass(frozen=True)
class ConstantViolation:
    """One consistency violation.

    ``kind`` is one of ``"value-mismatch"``, ``"unit-mismatch"``,
    ``"malformed-declaration"``.
    """

    kind: str
    name: str
    message: str


@dataclass
class ConstantConsistencyResult:
    """Outcome of a constant-consistency check.

    ``found`` is ``False`` when no markers AND no ``\\newcommand`` definitions
    exist anywhere in the supplied source(s) — the check is inactive and
    ``passed`` is ``True`` by graceful degradation.
    """

    found: bool
    declarations: list[ConstantDeclaration] = field(default_factory=list)
    violations: list[ConstantViolation] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        return not self.violations

    def to_dict(self) -> dict:
        """JSON-serializable shape for ``_gate.json`` — same ``found`` /
        ``declarations`` / ``violations`` / ``passed`` family as
        ``pinmap_check`` / ``buswidth_check``."""
        return {
            "found": self.found,
            "declarations": [
                {
                    "name": d.name,
                    "value": d.value,
                    "unit": d.unit,
                    "section": d.section,
                    "source": d.source,
                    "kind": d.kind,
                }
                for d in self.declarations
            ],
            "violations": [
                {"kind": v.kind, "name": v.name, "message": v.message}
                for v in self.violations
            ],
            "passed": self.passed,
        }


_MATHMODE_RE = re.compile(r"^\$(.*)\$$")
_SPACING_RE = re.compile(r"\\[,;:! ]|\\quad|\\qquad|~|\s+")


def normalize_value(value: str) -> str:
    """Minimally normalize a value token for exact-equality comparison.

    Strips a single surrounding ``$...$`` math-mode wrapper, removes LaTeX
    spacing commands (``\\,`` ``\\;`` ``~`` etc.) and whitespace, and strips
    ``,`` thousands separators — so ``3\\,s`` / ``3 s`` / ``3s`` and
    ``1,024`` / ``1024`` compare equal. Does NOT apply a numeric tolerance:
    two distinct normative values are a defect, not a "close enough".
    """
    v = value.strip()
    m = _MATHMODE_RE.match(v)
    if m is not None:
        v = m.group(1)
    v = _SPACING_RE.sub("", v)
    v = v.replace(",", "")
    return v


def _section_at(tex_source: str, pos: int) -> str | None:
    """Best-effort nearest enclosing ``\\section``/``\\subsection`` heading
    text at or before ``pos`` (for the violation message's location anchor)."""
    best: str | None = None
    for m in re.finditer(
        r"\\(?:sub)*section\*?\s*\{(?P<title>[^{}]*)\}", tex_source
    ):
        if m.start() <= pos:
            best = m.group("title").strip()
        else:
            break
    return best


def _parse_one(
    tex_source: str, source: str | None
) -> tuple[list[ConstantDeclaration], list[ConstantViolation], bool]:
    """Parse markers + ``\\newcommand`` from a single source string.

    Returns ``(declarations, malformed_violations, found)``. Cross-declaration
    mismatch detection happens later, over the merged declaration list.
    """
    declarations: list[ConstantDeclaration] = []
    malformed: list[ConstantViolation] = []
    found = False

    for match in _MARKER_RE.finditer(tex_source):
        found = True
        line_no = tex_source.count("\n", 0, match.start()) + 1
        attrs = {
            m.group("key"): m.group("value")
            for m in _ATTR_RE.finditer(match.group("attrs"))
        }
        name = attrs.get("name")
        value = attrs.get("value")
        loc = f"{source + ':' if source else ''}line {line_no}"
        if not name or value is None:
            missing = [k for k in ("name", "value") if not attrs.get(k)]
            malformed.append(
                ConstantViolation(
                    kind="malformed-declaration",
                    name=name or f"<unnamed@{loc}>",
                    message=(
                        f"{loc}: anvil-const declaration is missing "
                        f"required attribute(s): {', '.join(missing)}"
                    ),
                )
            )
            continue
        declarations.append(
            ConstantDeclaration(
                name=name,
                value=value,
                unit=attrs.get("unit"),
                line_no=line_no,
                section=_section_at(tex_source, match.start()),
                source=source,
                kind="marker",
            )
        )

    for match in _NEWCOMMAND_RE.finditer(tex_source):
        found = True
        line_no = tex_source.count("\n", 0, match.start()) + 1
        declarations.append(
            ConstantDeclaration(
                name=match.group("macro"),
                value=match.group("body"),
                unit=None,
                line_no=line_no,
                section=_section_at(tex_source, match.start()),
                source=source,
                kind="newcommand",
            )
        )

    return declarations, malformed, found


def _anchor(d: ConstantDeclaration) -> str:
    """Location anchor for a declaration in a violation message."""
    parts = []
    if d.source:
        parts.append(d.source)
    if d.section:
        parts.append(f"§{d.section}")
    parts.append(f"line {d.line_no}")
    return " ".join(parts)


def _detect_mismatches(
    declarations: list[ConstantDeclaration],
) -> list[ConstantViolation]:
    """Cross-declaration value/unit mismatch pass over the merged list."""
    violations: list[ConstantViolation] = []
    by_name: dict[str, list[ConstantDeclaration]] = {}
    for d in declarations:
        by_name.setdefault(d.name, []).append(d)

    for name, decls in by_name.items():
        if len(decls) < 2:
            continue
        first = decls[0]
        for other in decls[1:]:
            # Different units → unit-mismatch (never a value-mismatch); we do
            # not attempt conversion, so we cannot say the values disagree.
            if (first.unit or None) != (other.unit or None):
                violations.append(
                    ConstantViolation(
                        kind="unit-mismatch",
                        name=name,
                        message=(
                            f"constant {name!r} is declared with different "
                            f"units: {first.value}{_unit_suffix(first.unit)} "
                            f"({_anchor(first)}) vs "
                            f"{other.value}{_unit_suffix(other.unit)} "
                            f"({_anchor(other)}) — resolve manually "
                            "(no automatic unit conversion)"
                        ),
                    )
                )
                continue
            if normalize_value(first.value) != normalize_value(other.value):
                violations.append(
                    ConstantViolation(
                        kind="value-mismatch",
                        name=name,
                        message=(
                            f"constant {name!r} disagrees: "
                            f"{first.value}{_unit_suffix(first.unit)} "
                            f"({_anchor(first)}) vs "
                            f"{other.value}{_unit_suffix(other.unit)} "
                            f"({_anchor(other)})"
                        ),
                    )
                )
    return violations


def _unit_suffix(unit: str | None) -> str:
    return f" {unit}" if unit else ""


def check_constant_consistency(tex_source: str) -> ConstantConsistencyResult:
    """Single-file entry point — mirrors ``check_pinmap`` / ``check_buswidths``.

    Parses every ``% anvil-const:`` marker and every ``\\newcommand`` in
    ``tex_source``, then flags same-name value/unit disagreements. Malformed
    markers (missing ``name=``/``value=``) are recorded as
    ``malformed-declaration`` violations. Returns ``found=False`` (inactive,
    passing) when no markers and no ``\\newcommand`` definitions exist.
    """
    return check_constant_consistency_multi({"": tex_source})


def check_constant_consistency_multi(
    sources: dict[str, str],
) -> ConstantConsistencyResult:
    """Multi-file entry point for spec's multi-file LaTeX shape.

    ``sources`` maps a relative path / label → full ``.tex`` text (the caller —
    ``spec-review.md`` — owns globbing the version dir's ``.tex`` files; this
    module does no directory walking or file I/O). Declarations retain their
    source label so cross-file ``value-mismatch`` / ``unit-mismatch`` /
    duplicate-``\\newcommand`` findings carry per-file provenance in the message.
    An empty-string key is treated as "no source label" (the single-file path).
    """
    all_decls: list[ConstantDeclaration] = []
    malformed: list[ConstantViolation] = []
    found = False

    for label, text in sources.items():
        decls, mal, src_found = _parse_one(text, label or None)
        all_decls.extend(decls)
        malformed.extend(mal)
        found = found or src_found

    violations = malformed + _detect_mismatches(all_decls)
    return ConstantConsistencyResult(
        found=found, declarations=all_decls, violations=violations
    )
