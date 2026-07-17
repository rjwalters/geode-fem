"""Audience-class house-style switches for the report skill (#450).

This module implements the **deterministic** half of the report
skill's audience-class knob, documented in
``anvil/skills/report/commands/report-figures.md`` (steps 5b/6/7/9 —
render metadata, boilerplate injection, defense DRAFT watermark,
``_progress.json`` provenance) and ``report-review.md`` /
``anvil/skills/report/rubric.md`` (the defense-class
missing-distribution-statement critical flag). The contract:

- **Closed vocabulary** (v1): ``commercial | defense | internal``
  (:data:`AUDIENCE_CLASSES`, canonical definition in the sibling
  ``customer_context.py``). Enforcement needs known semantics; a
  consumer-extensible class registry is deferred. An out-of-vocabulary
  value is a structured :class:`ContextError` surfaced as a ``major``
  finding — never a crash — and the render proceeds **class-less**.
- **Orthogonality**: ``audience_class`` is independent of the existing
  ``confidentiality_class`` (watermark trigger) and ``export_control``
  (judgment input). Do NOT merge or derive; a consistency observation
  (e.g. ``export_control: itar`` + ``audience_class: commercial``) is
  deferred.
- **Declaration locus + resolution order**:
  ``_project.md`` frontmatter ``audience_class:`` (per-project
  override; also the sole locus for customer-less internal reports) →
  ``customers/<slug>/context.yaml`` ``audience_class:`` (the
  customer's default; parsed + validated by
  ``customer_context.py::load_context``) → absent. **Absent everywhere
  = byte-identical pre-#450 behavior** (the #428/#449 activation
  pattern). Resolution works with the customer tier OFF — pass
  ``context=None``.
- **Invalid override does NOT fall back**: an out-of-vocabulary
  ``_project.md`` value resolves to ``None`` (class-less) with the
  structured error attached — it does NOT fall through to the
  customer's ``context.yaml`` default. Falling back would silently
  render under a class the operator explicitly tried to override;
  surfacing the typo and rendering class-less is the safer failure.
- **Anvil ships hooks, the consumer ships legal text**: boilerplate
  lives at ``assets/audience/<class>.md`` resolved through the
  standard 3-layer asset order (per-version ``<thread>.{N}/assets/`` →
  consumer ``.anvil/skills/report/assets/`` → skill defaults). Anvil's
  skill defaults contain NO audience boilerplate (only a README) — no
  jurisdiction-specific legal text (DMEA/ITAR/distribution statements)
  ships with the framework. A missing file is a no-op for
  ``commercial``/``internal``; for ``defense`` the render completes
  and the figurer records the gap in ``_progress.json``
  (``phases.figures.audience_boilerplate: null``) — enforcement is
  ``report-review``'s job (one conditional critical flag, the
  topics-to-avoid judgment-prose shape; the audit-side twin is
  deferred).

**What stays judgment** (the agent, NOT this module): whether the
deliverable's distribution-statement/boilerplate block is actually
present and adequate. This module supplies resolution, asset lookup,
and structured errors only.

Skill-local (``anvil/skills/report/lib/``) per the #10/#26 pattern;
structural sibling of ``customer_context.py``. Pure stdlib (``re``,
``pathlib``).
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional, Sequence

from anvil.skills.report.lib.customer_context import (
    AUDIENCE_CLASS_KEY,
    AUDIENCE_CLASSES,
    ContextError,
    CustomerContext,
    _strip_comment,
    _unquote,
)

# Re-exported so consumers of this module have one import surface for
# the audience-class feature; the canonical definitions live in
# ``customer_context.py`` (where ``load_context`` validates the
# context.yaml side).
__all__ = [
    "AUDIENCE_CLASSES",
    "AUDIENCE_CLASS_KEY",
    "AUDIENCE_CLASS_COMMERCIAL",
    "AUDIENCE_CLASS_DEFENSE",
    "AUDIENCE_CLASS_INTERNAL",
    "AUDIENCE_ASSET_SUBDIR",
    "DEFENSE_WATERMARK",
    "AudienceClassResolution",
    "read_project_audience_class",
    "resolve_audience_class",
    "resolve_audience_boilerplate",
]


# --------------------------------------------------------------------------
# Constants
# --------------------------------------------------------------------------

#: Individual class names (closed vocabulary, v1).
AUDIENCE_CLASS_COMMERCIAL = "commercial"
AUDIENCE_CLASS_DEFENSE = "defense"
AUDIENCE_CLASS_INTERNAL = "internal"

#: Subdirectory under each ``assets/`` layer holding the
#: consumer-supplied per-class boilerplate: ``assets/audience/<class>.md``.
AUDIENCE_ASSET_SUBDIR = "audience"

#: Watermark metadata value the figurer passes for defense-class
#: renders (``--metadata=watermark:DRAFT``), reusing the existing
#: confidentiality-watermark mechanism (``report-figures.md`` step 7).
#: Promoted/final watermark removal is owned by ``report-promote``.
DEFENSE_WATERMARK = "DRAFT"

#: Relative path (from the consumer repo root) of the consumer-repo
#: asset-override layer for the report skill.
_CONSUMER_ASSETS_RELPATH = Path(".anvil") / "skills" / "report" / "assets"


# --------------------------------------------------------------------------
# _project.md frontmatter parsing (mirrors read_project_customer)
# --------------------------------------------------------------------------

_FRONTMATTER_FENCE = "---"
_AUDIENCE_CLASS_LINE_RE = re.compile(
    rf"^{AUDIENCE_CLASS_KEY}:\s*(.+?)\s*$"
)


def read_project_audience_class(project_md: Path) -> Optional[str]:
    """Extract the optional ``audience_class:`` key from ``_project.md``.

    Scans the YAML frontmatter (between the leading ``---`` fences)
    for a TOP-LEVEL ``audience_class:`` key. Returns the RAW declared
    value string (unquoted, stripped) without vocabulary validation —
    :func:`resolve_audience_class` owns validation so an
    out-of-vocabulary value surfaces as a structured error rather than
    silently reading as absent. Returns ``None`` when the file is
    absent, has no frontmatter, or has no ``audience_class:`` key.
    """
    p = Path(project_md)
    if not p.is_file():
        return None
    try:
        text = p.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None
    lines = text.splitlines()
    if not lines or lines[0].strip() != _FRONTMATTER_FENCE:
        return None
    for line in lines[1:]:
        if line.strip() == _FRONTMATTER_FENCE:
            break
        if line.startswith((" ", "\t")):
            continue  # nested keys (e.g. prior_reports entries)
        m = _AUDIENCE_CLASS_LINE_RE.match(_strip_comment(line))
        if m:
            value = _unquote(m.group(1)).strip()
            return value or None
    return None


# --------------------------------------------------------------------------
# Resolution: _project.md → context.yaml → absent
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class AudienceClassResolution:
    """The resolved audience class + where it came from.

    ``source`` is ``"project"`` (``_project.md`` frontmatter override),
    ``"context"`` (the customer's ``context.yaml`` default), or
    ``"absent"`` (no declaration anywhere, OR an invalid project
    override — see ``errors``). ``audience_class is None`` ⇔
    ``source == "absent"`` ⇔ the render path is byte-identical to
    pre-#450 (no metadata variable, no boilerplate injection, no
    watermark, no provenance fields).
    """

    audience_class: Optional[str]
    source: str
    errors: Sequence[ContextError] = field(default_factory=tuple)


def resolve_audience_class(
    project_md: Path,
    context: Optional[CustomerContext] = None,
) -> AudienceClassResolution:
    """Resolve the effective audience class for a project.

    Resolution order:

    1. ``_project.md`` frontmatter ``audience_class:`` — the
       per-project override; also the sole locus for internal reports
       with NO customer (this function works with ``context=None``,
       i.e. the customer tier OFF).
    2. The customer's ``context.yaml`` ``audience_class:`` default
       (``context.audience_class`` — already vocabulary-validated by
       ``load_context``; an invalid context value arrives here as
       ``None`` with the ``bad-value`` error on ``context.errors``).
    3. Absent → ``AudienceClassResolution(None, "absent")`` — the
       byte-identical pre-#450 path.

    An **out-of-vocabulary project override** records a ``bad-value``
    :class:`ContextError` and resolves class-less (``None`` /
    ``"absent"``) WITHOUT falling back to the customer default —
    falling back would silently render under the very class the
    operator tried to override. The error becomes a ``major`` finding
    at review time; the render proceeds.
    """
    raw = read_project_audience_class(project_md)
    if raw is not None:
        if raw in AUDIENCE_CLASSES:
            return AudienceClassResolution(
                audience_class=raw, source="project"
            )
        return AudienceClassResolution(
            audience_class=None,
            source="absent",
            errors=(
                ContextError(
                    kind="bad-value",
                    message=(
                        f"_project.md declares "
                        f"{AUDIENCE_CLASS_KEY}: {raw!r} but the "
                        f"closed v1 vocabulary is "
                        f"{', '.join(AUDIENCE_CLASSES)} — proceeding "
                        f"class-less (no fallback to the customer's "
                        f"context.yaml default; fix the override)"
                    ),
                ),
            ),
        )
    if context is not None and context.audience_class is not None:
        return AudienceClassResolution(
            audience_class=context.audience_class, source="context"
        )
    return AudienceClassResolution(audience_class=None, source="absent")


# --------------------------------------------------------------------------
# Boilerplate-asset resolution (the standard 3-layer order)
# --------------------------------------------------------------------------


def _default_skill_assets_dir() -> Path:
    """The shipped skill's own ``assets/`` dir (module-relative)."""
    return Path(__file__).resolve().parents[1] / "assets"


def resolve_audience_boilerplate(
    audience_class: str,
    *,
    version_dir: Path,
    repo_root: Optional[Path] = None,
    skill_assets_dir: Optional[Path] = None,
) -> Optional[Path]:
    """Resolve ``assets/audience/<class>.md`` through the 3-layer order.

    Layers (first existing file wins — the ``report-figures.md``
    "Render-pipeline customization" order):

    1. Per-version: ``<version_dir>/assets/audience/<class>.md``.
    2. Consumer repo: ``<repo_root>/.anvil/skills/report/assets/
       audience/<class>.md`` (skipped when ``repo_root`` is ``None``).
    3. Skill defaults: ``<skill>/assets/audience/<class>.md``
       (module-relative unless ``skill_assets_dir`` overrides it for
       testing). **Anvil ships NO boilerplate here** — the directory
       contains only a README; this layer only resolves when a
       consumer install drops a file into the shipped skill's own
       assets (layers 2 and 3 coincide for an installed skill).

    Returns ``None`` when no layer has the file: a no-op for
    ``commercial``/``internal``; for ``defense`` the figurer records
    the gap in ``_progress.json`` (``audience_boilerplate: null``) and
    ``report-review`` raises the missing-distribution-statement
    critical flag — enforcement is review's job, not the figurer's.
    """
    filename = f"{audience_class}.md"
    candidates = [
        Path(version_dir) / "assets" / AUDIENCE_ASSET_SUBDIR / filename
    ]
    if repo_root is not None:
        candidates.append(
            Path(repo_root)
            / _CONSUMER_ASSETS_RELPATH
            / AUDIENCE_ASSET_SUBDIR
            / filename
        )
    assets_dir = (
        Path(skill_assets_dir)
        if skill_assets_dir is not None
        else _default_skill_assets_dir()
    )
    candidates.append(assets_dir / AUDIENCE_ASSET_SUBDIR / filename)
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None
