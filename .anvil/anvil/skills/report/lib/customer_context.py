"""Customer-context tier for the report skill (issue #429).

This module implements the **deterministic** half of the report
skill's cross-project customer-context store + disclosure ledger,
documented in ``anvil/skills/report/commands/report-draft.md``
(advisory load), ``report-review.md`` / ``report-audit.md``
(topics-to-avoid enforcement), ``report-promote.md`` (ledger append),
and ``anvil/skills/report/rubric.md`` (the
``audit_disclosure_topic_violation`` critical flag). The contract:

- **Store shape**: ``<customers_dir>/<slug>/`` holds two files with
  split ownership — the human-owned ``context.yaml`` (NDA scope,
  export-control class, topics-to-avoid; version-stamped like the
  #428 manifest) and the machine-owned, append-only
  ``disclosures.jsonl`` delivery ledger. Agents never rewrite
  ``context.yaml``; the ledger append is a single ``open(..., "a")``
  write — no read-modify-write of human prose.
- **Location**: default ``<repo_root>/customers/`` (customer context
  is *content*, not framework config, so it lives at the repo root
  rather than under ``.anvil/``). Consumers may relocate it via the
  single optional ``.anvil/config.json`` key ``report.customers_dir``
  (the ``figure_adapters.py`` config-surface precedent).
- **Activation**: a project opts in by declaring ONE optional
  ``_project.md`` frontmatter key: ``customer: "<slug>"``. No key →
  the tier is off and every command behaves **byte-identically** to
  the pre-#429 skill (the #428/#449 activation pattern). A declared
  customer with a missing or malformed ``context.yaml`` still
  ACTIVATES the tier — the breakage surfaces as a ``major`` finding
  ("a broken declaration is a defect to surface, not an opt-out",
  mirroring ``data_contract.py``'s invalid-manifest posture).
- **Consultation matrix**: ``report-draft`` reads the context
  (advisory — NDA scope and topics-to-avoid inform drafting; recent
  ledger entries extend prior-reports awareness across ALL projects
  for the customer); ``report-review`` and ``report-audit`` enforce
  (topics-to-avoid violations are critical flags); ``report-promote``
  is the ONLY ledger writer (promotion is the delivery event — an
  audit-time append would log never-delivered drafts and duplicate on
  re-audit). The append is idempotent on ``project/thread/version``.
- **Critical flag**: ``audit_disclosure_topic_violation`` follows the
  ``audit_flags.py`` convention — ONE aggregated flag entry
  referencing all originating findings rows, surfaced via the
  standard ``critical_flags[]`` field, no schema change. Topic
  matching is auditor JUDGMENT (like the scope-creep flag), not
  regex — the deterministic part here is only context-file
  load/validation, ledger IO, and flag aggregation.

**What stays judgment** (the agent, NOT this module): deciding
whether a draft passage "discusses" a topic on the avoid list,
weighing NDA scope against draft content, and reconciling the draft
against prior ledger entries. This module supplies the parsed
context, the ledger records, and the flag aggregator.

**YAML handling is a minimal stdlib subset parser** (no new deps per
the pyproject contract): top-level scalar keys, one nested mapping
level (``nda:``), and a list of scalar-or-flat-mapping items
(``topics_to_avoid:``). Anything outside the subset is a structured
:class:`ContextError` — never a crash.

This module is **skill-local** (``anvil/skills/report/lib/``) per the
#10/#26 pattern; it is the structural sibling of ``data_contract.py``
(PR #449): activation gating, validation-error-as-finding, aggregated
flags, vocabulary discipline. Pure stdlib (``json``, ``re``,
``pathlib``, ``datetime``).
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable, Mapping, Optional, Sequence

from anvil.skills.report.lib.audit_flags import CriticalFlag


# --------------------------------------------------------------------------
# Constants
# --------------------------------------------------------------------------

#: Default customers-dir name under the repo root. Customer context is
#: content, not framework config — hence repo root, not ``.anvil/``.
DEFAULT_CUSTOMERS_DIRNAME = "customers"

#: The single optional ``.anvil/config.json`` override key, spelled as
#: the dotted path consumers write: ``{"report": {"customers_dir": …}}``.
CUSTOMERS_DIR_CONFIG_KEY = "report.customers_dir"

#: Human-owned context file inside ``<customers_dir>/<slug>/``.
CONTEXT_FILENAME = "context.yaml"

#: Machine-owned append-only delivery ledger inside the same dir.
LEDGER_FILENAME = "disclosures.jsonl"

#: The only context.yaml schema version this module understands
#: (version-stamped like the #428 manifest).
CONTEXT_VERSION = 1

#: The ONE optional ``_project.md`` frontmatter key that activates the
#: tier for a project.
PROJECT_CUSTOMER_KEY = "customer"

#: Closed audience-class vocabulary (issue #450). The knob selects
#: consumer-supplied house-style boilerplate + render metadata in
#: ``report-figures`` and gates the defense-class distribution-statement
#: critical flag in ``report-review``. The vocabulary is CLOSED in v1 —
#: enforcement needs known semantics; a consumer-extensible class
#: registry is deferred. An out-of-vocabulary value is a structured
#: ``bad-value`` error (never a crash) and the field is treated as
#: absent. Orthogonal to ``confidentiality_class`` (watermark trigger)
#: and ``export_control`` (judgment input) — do NOT merge or derive.
#: Resolution helpers live in the sibling ``audience_class.py``.
AUDIENCE_CLASSES = ("commercial", "defense", "internal")

#: The optional top-level ``context.yaml`` key carrying the customer's
#: default audience class (overridable per project via the same-named
#: ``_project.md`` frontmatter key — see ``audience_class.py``).
AUDIENCE_CLASS_KEY = "audience_class"

#: Audit-side critical-flag identifier. Upper-case constant mirrors
#: the ``audit_flags.py`` / ``data_contract.py`` convention. The
#: review-side twin is the judgment-prose flag "Discusses a topic on
#: the customer's topics-to-avoid list" in ``rubric.md`` (same shape
#: as the scope-creep flag — no separate machine identifier).
CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION = (
    "audit_disclosure_topic_violation"
)

#: The ledger-record keys, in canonical write order. ``project`` /
#: ``thread`` / ``version`` triple is the idempotency key.
DISCLOSURE_RECORD_KEYS = (
    "ts",
    "customer",
    "engagement_id",
    "project",
    "thread",
    "version",
    "summary",
    "report_sha256",
)


# --------------------------------------------------------------------------
# Structured errors (never crash — the data_contract.py posture)
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class ContextError:
    """A structured customer-context validation error.

    ``kind`` is one of: ``context-missing``, ``malformed-yaml``,
    ``bad-shape``, ``bad-version``, ``bad-value``, ``missing-field``,
    ``customer-mismatch``, ``bad-config``, ``malformed-ledger-line``.
    ``message`` is operator-facing prose. Each error becomes a
    ``major`` finding when the tier is active — a declared-but-broken
    context is a defect to surface, not an opt-out.
    """

    kind: str
    message: str


# --------------------------------------------------------------------------
# Repo-root + customers-dir resolution
# --------------------------------------------------------------------------


def find_repo_root(start: Path) -> Optional[Path]:
    """Walk up from ``start`` for a dir containing ``.anvil/`` or ``.git``.

    ``.git`` may be a directory (normal clone) or a file (worktree).
    Returns ``None`` when no marker is found before the filesystem
    root — callers then surface a structured error rather than
    guessing.
    """
    current = Path(start).resolve()
    if current.is_file():
        current = current.parent
    while True:
        if (current / ".anvil").is_dir() or (current / ".git").exists():
            return current
        if current.parent == current:
            return None
        current = current.parent


@dataclass(frozen=True)
class CustomersDirResolution:
    """The resolved customers directory + how it was resolved.

    ``source`` is ``"default"`` (``<repo_root>/customers/``) or
    ``"config"`` (the ``report.customers_dir`` key). A malformed
    config surfaces in ``errors`` and resolution falls back to the
    default — broken config is a finding, never a crash.
    """

    path: Path
    source: str
    errors: Sequence[ContextError] = field(default_factory=tuple)


def resolve_customers_dir(
    repo_root: Path, config_path: Optional[Path] = None
) -> CustomersDirResolution:
    """Resolve the customers dir for a repo.

    Resolution order:

    1. ``.anvil/config.json`` key ``report.customers_dir`` when
       present and a non-empty string. Relative values resolve
       against ``repo_root``; absolute values are used as-is.
    2. Otherwise the default ``<repo_root>/customers/``.

    An absent config file or absent key is the clean default path
    (zero behavior change). A malformed config (bad JSON, non-string
    key) records a ``bad-config`` :class:`ContextError` and falls
    back to the default — the operator sees the problem as a finding.
    """
    root = Path(repo_root)
    default = CustomersDirResolution(
        path=root / DEFAULT_CUSTOMERS_DIRNAME, source="default"
    )
    cfg_path = (
        Path(config_path)
        if config_path is not None
        else root / ".anvil" / "config.json"
    )
    if not cfg_path.is_file():
        return default
    try:
        cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        return CustomersDirResolution(
            path=default.path,
            source="default",
            errors=(
                ContextError(
                    kind="bad-config",
                    message=(
                        f"{cfg_path} is not valid JSON ({exc}); "
                        f"falling back to the default customers dir "
                        f"{default.path}"
                    ),
                ),
            ),
        )
    if not isinstance(cfg, dict):
        return CustomersDirResolution(
            path=default.path,
            source="default",
            errors=(
                ContextError(
                    kind="bad-config",
                    message=(
                        f"{cfg_path}: top level must be a JSON "
                        f"object, got {type(cfg).__name__}; falling "
                        f"back to {default.path}"
                    ),
                ),
            ),
        )
    report_section = cfg.get("report")
    if not isinstance(report_section, dict):
        return default
    raw = report_section.get("customers_dir")
    if raw is None:
        return default
    if not isinstance(raw, str) or not raw.strip():
        return CustomersDirResolution(
            path=default.path,
            source="default",
            errors=(
                ContextError(
                    kind="bad-config",
                    message=(
                        f"{cfg_path}: {CUSTOMERS_DIR_CONFIG_KEY} must "
                        f"be a non-empty string, got {raw!r}; falling "
                        f"back to {default.path}"
                    ),
                ),
            ),
        )
    value = Path(raw.strip())
    resolved = value if value.is_absolute() else root / value
    return CustomersDirResolution(path=resolved, source="config")


# --------------------------------------------------------------------------
# Project-level customer declaration (_project.md frontmatter)
# --------------------------------------------------------------------------

_FRONTMATTER_FENCE = "---"
_CUSTOMER_LINE_RE = re.compile(
    r"^customer:\s*(.+?)\s*$"
)


def _unquote(value: str) -> str:
    v = value.strip()
    if len(v) >= 2 and v[0] == v[-1] and v[0] in ("'", '"'):
        return v[1:-1]
    return v


def read_project_customer(project_md: Path) -> Optional[str]:
    """Extract the optional ``customer:`` key from ``_project.md``.

    Scans the YAML frontmatter (between the leading ``---`` fences)
    for a TOP-LEVEL ``customer:`` key. Returns the slug string, or
    ``None`` when the file is absent, has no frontmatter, or has no
    ``customer:`` key — ``None`` means the tier is OFF for the
    project and every command behaves byte-identically to pre-#429.
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
        m = _CUSTOMER_LINE_RE.match(_strip_comment(line))
        if m:
            slug = _unquote(m.group(1))
            return slug or None
    return None


def context_active(project_md: Path) -> bool:
    """True iff the customer-context tier is active for this project.

    Activation is purely the presence of the ``customer:`` key in
    ``_project.md`` frontmatter. A declared-but-broken context file
    does NOT deactivate the tier — load errors surface as findings.
    """
    return read_project_customer(project_md) is not None


# --------------------------------------------------------------------------
# Minimal YAML-subset parsing for context.yaml (stdlib only)
# --------------------------------------------------------------------------


def _strip_comment(line: str) -> str:
    """Remove a trailing ``#`` comment outside quotes."""
    out: list[str] = []
    in_single = in_double = False
    for ch in line:
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif ch == "#" and not in_single and not in_double:
            break
        out.append(ch)
    return "".join(out)


_KEY_VALUE_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_-]*):(?:\s+(.*))?$")


def _scalar(value: str) -> Any:
    v = _unquote(value)
    if re.fullmatch(r"-?\d+", v):
        return int(v)
    return v


def _parse_yaml_subset(
    text: str,
) -> tuple[dict[str, Any], list[ContextError]]:
    """Parse the constrained context.yaml subset.

    Supported shapes: top-level ``key: scalar``; one nested mapping
    level (``nda:`` with indented ``key: scalar`` lines); a list of
    items that are either quoted/plain scalars or flat mappings
    (``- topic: …`` with indented ``reason: …`` continuations). Lines
    outside the subset produce ``malformed-yaml`` errors and are
    skipped — structured errors, never a crash.
    """
    errors: list[ContextError] = []
    rows: list[tuple[int, str]] = []
    for lineno, raw in enumerate(text.splitlines(), start=1):
        stripped_line = _strip_comment(raw).rstrip()
        if not stripped_line.strip():
            continue
        if stripped_line.strip() == _FRONTMATTER_FENCE:
            continue  # tolerate a stray document fence
        rows.append((lineno, stripped_line))

    root: dict[str, Any] = {}
    i = 0
    while i < len(rows):
        lineno, line = rows[i]
        indent = len(line) - len(line.lstrip(" "))
        content = line.strip()
        if indent != 0:
            errors.append(
                ContextError(
                    kind="malformed-yaml",
                    message=(
                        f"line {lineno}: unexpected indentation "
                        f"outside a nested block: {content!r}"
                    ),
                )
            )
            i += 1
            continue
        m = _KEY_VALUE_RE.match(content)
        if not m:
            errors.append(
                ContextError(
                    kind="malformed-yaml",
                    message=(
                        f"line {lineno}: expected 'key: value', got "
                        f"{content!r}"
                    ),
                )
            )
            i += 1
            continue
        key, value = m.group(1), (m.group(2) or "").strip()
        if value:
            root[key] = _scalar(value)
            i += 1
            continue
        # Nested block: gather indented rows.
        block: list[tuple[int, str]] = []
        i += 1
        while i < len(rows):
            nlineno, nline = rows[i]
            nindent = len(nline) - len(nline.lstrip(" "))
            if nindent == 0:
                break
            block.append((nlineno, nline))
            i += 1
        if not block:
            root[key] = None
            continue
        root[key] = _parse_block(key, block, errors)
    return root, errors


def _parse_block(
    key: str,
    block: list[tuple[int, str]],
    errors: list[ContextError],
) -> Any:
    first_content = block[0][1].strip()
    if first_content.startswith("- ") or first_content == "-":
        return _parse_list_block(key, block, errors)
    return _parse_mapping_block(key, block, errors)


def _parse_mapping_block(
    key: str,
    block: list[tuple[int, str]],
    errors: list[ContextError],
) -> dict[str, Any]:
    mapping: dict[str, Any] = {}
    for lineno, line in block:
        m = _KEY_VALUE_RE.match(line.strip())
        if not m or not (m.group(2) or "").strip():
            errors.append(
                ContextError(
                    kind="malformed-yaml",
                    message=(
                        f"line {lineno}: '{key}' block expects flat "
                        f"'key: value' entries, got {line.strip()!r}"
                    ),
                )
            )
            continue
        mapping[m.group(1)] = _scalar(m.group(2))
    return mapping


def _parse_list_block(
    key: str,
    block: list[tuple[int, str]],
    errors: list[ContextError],
) -> list[Any]:
    items: list[Any] = []
    current: Optional[dict[str, Any]] = None
    for lineno, line in block:
        content = line.strip()
        if content.startswith("- ") or content == "-":
            rest = content[1:].strip()
            if not rest:
                current = {}
                items.append(current)
                continue
            m = _KEY_VALUE_RE.match(rest)
            if m and (m.group(2) or "").strip():
                current = {m.group(1): _scalar(m.group(2))}
                items.append(current)
            else:
                items.append(_scalar(rest))
                current = None
            continue
        # Continuation line under the current mapping item.
        m = _KEY_VALUE_RE.match(content)
        if current is None or not m or not (m.group(2) or "").strip():
            errors.append(
                ContextError(
                    kind="malformed-yaml",
                    message=(
                        f"line {lineno}: '{key}' list expects '- ' "
                        f"items or 'key: value' continuations, got "
                        f"{content!r}"
                    ),
                )
            )
            continue
        current[m.group(1)] = _scalar(m.group(2))
    return items


# --------------------------------------------------------------------------
# context.yaml load + validation
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class TopicToAvoid:
    """One topics-to-avoid entry. ``reason`` is optional prose."""

    topic: str
    reason: Optional[str] = None


@dataclass(frozen=True)
class CustomerContext:
    """A parsed (possibly broken) ``<customers_dir>/<slug>/context.yaml``.

    ``errors`` carries every validation problem found; ``ok`` is True
    iff no errors. A missing or malformed file still returns a
    :class:`CustomerContext` (with the structured errors) — when the
    tier is active, every error is a ``major`` finding directing the
    operator to create or fix the file. The tier is never silently
    deactivated by a broken context.
    """

    slug: str
    path: Path
    version: Optional[int]
    customer: Optional[str]
    nda: Mapping[str, Any]
    export_control: Optional[str]
    topics_to_avoid: Sequence[TopicToAvoid]
    audience_class: Optional[str] = None
    errors: Sequence[ContextError] = field(default_factory=tuple)

    @property
    def ok(self) -> bool:
        return not self.errors


def customer_dir(customers_dir: Path, slug: str) -> Path:
    """The per-customer directory ``<customers_dir>/<slug>/``."""
    return Path(customers_dir) / slug


def load_context(customers_dir: Path, slug: str) -> CustomerContext:
    """Load + validate ``<customers_dir>/<slug>/context.yaml``.

    ALWAYS returns a :class:`CustomerContext` — this function is only
    called when a project declared ``customer: <slug>``, so the tier
    is active and a missing/malformed file is a structured error to
    surface as a ``major`` finding, never a silent skip and never a
    crash. Error kinds:

    - ``context-missing`` — the file does not exist (the finding
      directs the operator to create it from
      ``templates/customer-context.template.yaml``).
    - ``malformed-yaml`` — a line falls outside the supported subset.
    - ``bad-version`` — ``version`` present but not
      :data:`CONTEXT_VERSION`.
    - ``bad-shape`` — ``nda`` is not a mapping, ``topics_to_avoid``
      is not a list, or ``export_control`` is not a scalar string
      (the field is then treated as absent).
    - ``missing-field`` — a ``topics_to_avoid`` mapping item lacks
      ``topic``.
    - ``customer-mismatch`` — the file's ``customer:`` field differs
      from the directory slug (a copied-file defect worth surfacing).
    """
    path = customer_dir(customers_dir, slug) / CONTEXT_FILENAME
    if not path.is_file():
        return CustomerContext(
            slug=slug,
            path=path,
            version=None,
            customer=None,
            nda={},
            export_control=None,
            topics_to_avoid=(),
            errors=(
                ContextError(
                    kind="context-missing",
                    message=(
                        f"project declares customer {slug!r} but "
                        f"{path} does not exist — create it from "
                        f"anvil/skills/report/templates/"
                        f"customer-context.template.yaml"
                    ),
                ),
            ),
        )
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        return CustomerContext(
            slug=slug,
            path=path,
            version=None,
            customer=None,
            nda={},
            export_control=None,
            topics_to_avoid=(),
            errors=(
                ContextError(
                    kind="malformed-yaml",
                    message=f"{path} is not readable UTF-8 text: {exc}",
                ),
            ),
        )

    raw, errors = _parse_yaml_subset(text)

    version = raw.get("version")
    if version is not None and version != CONTEXT_VERSION:
        errors.append(
            ContextError(
                kind="bad-version",
                message=(
                    f"unsupported context.yaml version {version!r} "
                    f"(this module understands version "
                    f"{CONTEXT_VERSION})"
                ),
            )
        )

    customer = raw.get("customer")
    if customer is not None and not isinstance(customer, str):
        customer = str(customer)
    if isinstance(customer, str) and customer != slug:
        errors.append(
            ContextError(
                kind="customer-mismatch",
                message=(
                    f"context.yaml declares customer {customer!r} "
                    f"but lives in the {slug!r} directory — the file "
                    f"may have been copied from another customer"
                ),
            )
        )

    nda = raw.get("nda")
    if nda is None:
        nda = {}
    elif not isinstance(nda, dict):
        errors.append(
            ContextError(
                kind="bad-shape",
                message=(
                    f"'nda' must be a mapping, got "
                    f"{type(nda).__name__} — treating it as absent"
                ),
            )
        )
        nda = {}

    export_control = raw.get("export_control")
    if export_control is not None and not isinstance(export_control, str):
        errors.append(
            ContextError(
                kind="bad-shape",
                message=(
                    f"'export_control' must be a scalar string, got "
                    f"{type(export_control).__name__} — treating it "
                    f"as absent"
                ),
            )
        )
        export_control = None

    audience_class = raw.get(AUDIENCE_CLASS_KEY)
    if audience_class is not None and not isinstance(audience_class, str):
        errors.append(
            ContextError(
                kind="bad-shape",
                message=(
                    f"'{AUDIENCE_CLASS_KEY}' must be a scalar string, "
                    f"got {type(audience_class).__name__} — treating "
                    f"it as absent"
                ),
            )
        )
        audience_class = None
    elif isinstance(audience_class, str):
        audience_class = audience_class.strip()
        if audience_class not in AUDIENCE_CLASSES:
            errors.append(
                ContextError(
                    kind="bad-value",
                    message=(
                        f"'{AUDIENCE_CLASS_KEY}' must be one of "
                        f"{', '.join(AUDIENCE_CLASSES)}; got "
                        f"{audience_class!r} — treating it as absent "
                        f"(the vocabulary is closed in v1; a "
                        f"consumer-extensible class registry is "
                        f"deferred)"
                    ),
                )
            )
            audience_class = None

    topics: list[TopicToAvoid] = []
    raw_topics = raw.get("topics_to_avoid")
    if raw_topics is None:
        raw_topics = []
    elif not isinstance(raw_topics, list):
        errors.append(
            ContextError(
                kind="bad-shape",
                message=(
                    f"'topics_to_avoid' must be a list, got "
                    f"{type(raw_topics).__name__} — treating it as "
                    f"empty"
                ),
            )
        )
        raw_topics = []
    for idx, item in enumerate(raw_topics):
        if isinstance(item, str):
            if item.strip():
                topics.append(TopicToAvoid(topic=item.strip()))
            continue
        if isinstance(item, dict):
            topic = item.get("topic")
            if not isinstance(topic, str) or not topic.strip():
                errors.append(
                    ContextError(
                        kind="missing-field",
                        message=(
                            f"topics_to_avoid[{idx}] is missing the "
                            f"required 'topic' field"
                        ),
                    )
                )
                continue
            reason = item.get("reason")
            topics.append(
                TopicToAvoid(
                    topic=topic.strip(),
                    reason=(
                        str(reason).strip()
                        if reason is not None and str(reason).strip()
                        else None
                    ),
                )
            )
            continue
        errors.append(
            ContextError(
                kind="bad-shape",
                message=(
                    f"topics_to_avoid[{idx}] must be a string or a "
                    f"mapping with 'topic', got "
                    f"{type(item).__name__}"
                ),
            )
        )

    return CustomerContext(
        slug=slug,
        path=path,
        version=version if isinstance(version, int) else None,
        customer=customer if isinstance(customer, str) else None,
        nda=nda,
        export_control=export_control,
        topics_to_avoid=tuple(topics),
        audience_class=audience_class,
        errors=tuple(errors),
    )


# --------------------------------------------------------------------------
# Disclosure ledger (machine-owned, append-only JSONL)
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class DisclosureLedger:
    """The parsed ``disclosures.jsonl`` for one customer.

    ``records`` carries every well-formed JSON-object line (in file
    order); ``errors`` carries one ``malformed-ledger-line`` entry per
    unparseable line (the line is skipped, never fatal — readers
    degrade gracefully so one corrupt line cannot disable the
    cross-project consistency check).
    """

    path: Path
    records: Sequence[Mapping[str, Any]]
    errors: Sequence[ContextError] = field(default_factory=tuple)


def load_disclosures(customers_dir: Path, slug: str) -> DisclosureLedger:
    """Read the customer's delivery ledger. Absent file → empty ledger."""
    path = customer_dir(customers_dir, slug) / LEDGER_FILENAME
    if not path.is_file():
        return DisclosureLedger(path=path, records=(), errors=())
    records: list[Mapping[str, Any]] = []
    errors: list[ContextError] = []
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        return DisclosureLedger(
            path=path,
            records=(),
            errors=(
                ContextError(
                    kind="malformed-ledger-line",
                    message=f"{path} is not readable UTF-8 text: {exc}",
                ),
            ),
        )
    for lineno, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError as exc:
            errors.append(
                ContextError(
                    kind="malformed-ledger-line",
                    message=(
                        f"{path} line {lineno} is not valid JSON "
                        f"({exc}) — line skipped"
                    ),
                )
            )
            continue
        if not isinstance(obj, dict):
            errors.append(
                ContextError(
                    kind="malformed-ledger-line",
                    message=(
                        f"{path} line {lineno} must be a JSON "
                        f"object, got {type(obj).__name__} — line "
                        f"skipped"
                    ),
                )
            )
            continue
        records.append(obj)
    return DisclosureLedger(
        path=path, records=tuple(records), errors=tuple(errors)
    )


@dataclass(frozen=True)
class AppendResult:
    """Outcome of one :func:`append_disclosure` call.

    ``appended`` is False for the idempotent-duplicate case (a record
    with the same ``project/thread/version`` already exists);
    ``reason`` then explains the skip. ``record`` is the record that
    was (or would have been) written.
    """

    appended: bool
    path: Path
    record: Mapping[str, Any]
    reason: Optional[str] = None


def _utc_now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def append_disclosure(
    customers_dir: Path,
    slug: str,
    *,
    project: str,
    thread: str,
    version: int,
    summary: str,
    engagement_id: Optional[str] = None,
    report_sha256: Optional[str] = None,
    ts: Optional[str] = None,
) -> AppendResult:
    """Append one delivery record to the customer's disclosure ledger.

    Called by ``report-promote`` at promotion time — promotion is the
    delivery event (an audit-time append would log never-delivered
    drafts and duplicate on re-audit; the ledger reads at draft/audit
    time, writes at promote time). Properties:

    - **Append-only**: a single ``open(path, "a")`` write of one JSON
      line. This function NEVER modifies ``context.yaml`` (the
      human-owned file) and never rewrites existing ledger lines.
    - **Idempotent** on the ``project/thread/version`` triple: when a
      record with the same triple already exists, nothing is written
      and the result carries ``appended=False`` with a reason
      (re-running a completed promotion is a no-op, mirroring the
      receipt idempotency in ``report-promote.md`` step 2).
    - The per-customer directory is created if absent (the ledger is
      machine-owned; ``context.yaml`` is never auto-created).
    """
    ledger = load_disclosures(customers_dir, slug)
    record = {
        "ts": ts if ts is not None else _utc_now_iso(),
        "customer": slug,
        "engagement_id": engagement_id,
        "project": project,
        "thread": thread,
        "version": version,
        "summary": summary,
        "report_sha256": report_sha256,
    }
    for existing in ledger.records:
        if (
            existing.get("project") == project
            and existing.get("thread") == thread
            and existing.get("version") == version
        ):
            return AppendResult(
                appended=False,
                path=ledger.path,
                record=record,
                reason=(
                    f"a disclosure for {project}/{thread} v{version} "
                    f"is already recorded (ts: "
                    f"{existing.get('ts', 'unknown')}) — append is "
                    f"idempotent on project/thread/version"
                ),
            )
    ledger.path.parent.mkdir(parents=True, exist_ok=True)
    with ledger.path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(record, ensure_ascii=False) + "\n")
    return AppendResult(appended=True, path=ledger.path, record=record)


# --------------------------------------------------------------------------
# Critical-flag detector (over the auditor's topic-sweep findings rows)
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class TopicViolationRow:
    """One row of the auditor's topics-to-avoid sweep findings table.

    The auditor (judgment, like the scope-creep flag) has already
    decided each row IS a violation — this module only aggregates.
    ``topic`` is the matched ``topics_to_avoid`` entry; ``excerpt``
    is the offending draft passage; ``location`` is the report
    location (``§2.1 ¶3`` style).
    """

    row_number: int
    location: str
    excerpt: str
    topic: str


def detect_disclosure_topic_violations(
    rows: Iterable[TopicViolationRow],
    *,
    context_active: bool,
) -> Optional[CriticalFlag]:
    """Detect ``audit_disclosure_topic_violation`` (aggregated).

    Fires iff the customer-context tier is **active** (the project
    declares a ``customer:``) and at least one sweep row exists. With
    the tier inactive this returns ``None`` unconditionally — the
    byte-identical no-customer contract.

    Severity calibration: **critical**, not a rubric deduction — an
    NDA/ITAR breach in a delivered report is not recoverable by a
    higher score elsewhere (the same line ``data_contract.py`` draws:
    a stale source may still be correct; fabrication cannot be).
    Aggregation rule: ONE flag entry referencing all originating rows
    (the ``audit_flags.py`` convention).
    """
    if not context_active:
        return None

    offending = list(rows)
    if not offending:
        return None

    rows_ref = ", ".join(f"row #{r.row_number}" for r in offending)
    topics = sorted({r.topic for r in offending})
    topics_ref = ", ".join(repr(t) for t in topics)
    justification = (
        f"{len(offending)} draft passage(s) discuss topic(s) on the "
        f"customer's topics-to-avoid list ({rows_ref} in findings.md; "
        f"topic(s): {topics_ref}). An NDA/export-control breach in a "
        "delivered report is not recoverable by a higher score "
        "elsewhere. Reviser MUST remove or rework the passage(s); if "
        "the topic restriction no longer applies, the OPERATOR (not "
        "the agent) must update the customer's context.yaml."
    )
    return CriticalFlag(
        type=CRITICAL_FLAG_AUDIT_DISCLOSURE_TOPIC_VIOLATION,
        justification=justification,
        originating_rows=tuple(r.row_number for r in offending),
    )
