"""Detection for `anvil:rubric-rebackport` (issue #358).

Walks a project tree and lists every ``<thread>.{N}.review/`` directory
whose ``_meta.json`` lacks one or more of the post-#346 rubric
stamping fields (``rubric_id``, ``rubric_total``,
``advance_threshold``). Each entry is annotated with the inferred
owning skill (from the version-dir naming convention + optional
``BRIEF.md`` ``documents:`` block + body filename heuristic) and a
typed snapshot of the sibling ``_progress.json`` / ``_summary.md``
files so the planner can decide what to rewrite.

Design notes
------------

- **Pure detector — no mutations.** Like the project-migrate detector,
  this module reads files but never writes. This is load-bearing for
  the dry-run contract: the same code path drives dry-run and apply,
  and detection MUST never touch disk.
- **Skill-local first.** Lives under
  ``anvil/skills/rubric-rebackport/lib/`` per the CLAUDE.md
  "skill-local first, lib promotion later" pattern.
- **Subprocess-free.** Stdlib-only walker; no LLM calls, no shell
  invocations. The reviewer LLM is only invoked under
  ``--rescore`` via the per-skill reviewer command, and that
  invocation lives in :mod:`orchestrate`, not here.

Public API
----------

- ``ReviewSnapshot`` — typed per-review snapshot.
- ``ProjectInventory`` — typed snapshot of the whole tree.
- ``detect_unstamped_reviews(project_tree)`` — top-level entry.
- ``inventory_tree(project_tree)`` — return the full typed inventory
  (including already-stamped reviews) without filtering.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------


META_FILENAME = "_meta.json"
PROGRESS_FILENAME = "_progress.json"
SUMMARY_FILENAME = "_summary.md"
BRIEF_FILENAME = "BRIEF.md"

# The post-#346 stamping fields a fully-stamped review's _meta.json must
# carry. The detector flags a review as unstamped when any of these
# fields is absent.
REQUIRED_STAMP_FIELDS = ("rubric_id", "rubric_total", "advance_threshold")

# Version-dir naming: <stem>.<N> where N is one or more digits.
_VERSION_DIR_RE = re.compile(r"^(?P<stem>.+)\.(?P<num>\d+)$")

# Review-dir naming: <stem>.<N>.review (the canonical reviewer sibling).
# We deliberately do NOT match arbitrary critic-tag siblings (e.g.,
# `.audit/`) here — the post-#346 stamping contract is specifically
# about reviewer-produced reviews. A separate pass for audit critics is
# a follow-on if it ever proves necessary.
_REVIEW_DIR_RE = re.compile(
    r"^(?P<stem>.+)\.(?P<num>\d+)\.review$"
)

# Body filename → skill inference. Mirrors the project-migrate
# _SKILL_FIXED_BODY_FILENAMES list but maps to skill names instead.
#
# Note: ``deck.md``, ``slides.md``, and ``ip-uspto.md`` are the
# non-slug-echoed body filename conventions for the deck / slides /
# ip-uspto skills (the canary uses thread slugs like ``aldus`` whose
# version dirs contain a fixed-name body file, not a slug-echoed one).
# Without these entries, rule 2 in ``_infer_skill`` misses on those
# skills' threads and inference falls through to ``None``. See
# issue #374.
_BODY_FILENAME_TO_SKILL: Dict[str, str] = {
    "memo.md": "memo",
    "proposal.md": "proposal",
    "report.md": "report",
    "installation.md": "installation",
    "paper.md": "paper",
    # Legacy body filename (issue #694): the `pub` skill was renamed to
    # `paper`. Existing consumer threads still carry a `pub.md` body — it
    # resolves to the CURRENT skill name `paper` so their reviews keep
    # inferring the right (paper) rubric row.
    "pub.md": "paper",
    "deck.md": "deck",
    "slides.md": "slides",
    "ip-uspto.md": "ip-uspto",
}

# Dirs we never descend into during the tree walk.
_SKIP_DIR_NAMES = frozenset({
    ".git",
    ".anvil-migrate-rollback",
    ".anvil-rebackport-rollback",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
})


@dataclass
class ReviewSnapshot:
    """Typed per-review snapshot for the planner.

    Attributes
    ----------
    review_dir
        Absolute path to the ``<thread>.{N}.review/`` directory.
    review_id
        A stable, human-readable id for this review: the relative path
        from the project tree root, slash-joined. Used for snapshot
        directories and report headers.
    meta_path
        Absolute path to ``_meta.json`` (always present in a valid
        review dir; missing meta files surface as an error note).
    meta
        Parsed contents of ``_meta.json``. Empty dict on parse error
        (a note records the failure).
    meta_parse_error
        Diagnostic when ``_meta.json`` failed to parse. ``None`` on
        success.
    progress_path
        Absolute path to the sibling ``_progress.json`` (if present).
    progress
        Parsed contents (empty dict when absent or parse-failed).
    summary_path
        Absolute path to the sibling ``_summary.md`` (if present).
    summary_has_rubric_block
        True iff ``_summary.md``'s top-level frontmatter / JSON block
        carries a ``rubric:`` key.
    inferred_skill
        Skill name (e.g., ``"memo"``) inferred from the version-dir
        stem + project BRIEF + body filename heuristic. ``None`` when
        no inference rule fires.
    skill_source
        A short tag describing where the skill inference came from
        (``"brief"`` / ``"body-filename"`` / ``"version-dir-stem"`` /
        ``"unknown"``). Surfaced in operator-facing reports.
    is_stamped
        True iff the review's ``_meta.json`` carries every required
        stamping field.
    progress_score_history_unstamped_rows
        Number of ``score_history[]`` rows in ``_progress.json`` that
        lack ``rubric_id``. Zero when the file is absent or fully
        stamped.
    """

    review_dir: Path
    review_id: str
    meta_path: Path
    meta: Dict[str, object] = field(default_factory=dict)
    meta_parse_error: Optional[str] = None
    progress_path: Optional[Path] = None
    progress: Dict[str, object] = field(default_factory=dict)
    summary_path: Optional[Path] = None
    summary_has_rubric_block: bool = False
    inferred_skill: Optional[str] = None
    skill_source: str = "unknown"
    is_stamped: bool = False
    progress_score_history_unstamped_rows: int = 0


@dataclass
class ProjectInventory:
    """Typed snapshot of the whole tree.

    Attributes
    ----------
    project_tree
        The directory the walker descended from.
    reviews
        Every recognized review dir, in walk order.
    project_brief_skill_map
        Map of slug → skill (resolved from any ``BRIEF.md``
        ``documents:`` block discovered during the walk). Used by the
        planner for the highest-confidence skill inference.
    """

    project_tree: Path
    reviews: List[ReviewSnapshot] = field(default_factory=list)
    project_brief_skill_map: Dict[str, str] = field(default_factory=dict)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _safe_read_json(path: Path) -> Dict[str, object]:
    """Return parsed JSON dict, or empty dict on any failure."""
    try:
        with path.open("r", encoding="utf-8") as fh:
            data = json.load(fh)
        if isinstance(data, dict):
            return data
        return {}
    except (OSError, json.JSONDecodeError):
        return {}


def _read_json_with_error(path: Path) -> tuple[Dict[str, object], Optional[str]]:
    """Return (parsed_dict, error_message). On success error_message is None."""
    try:
        with path.open("r", encoding="utf-8") as fh:
            data = json.load(fh)
    except OSError as exc:
        return {}, f"read failed: {exc}"
    except json.JSONDecodeError as exc:
        return {}, f"JSON parse failed: {exc}"
    if not isinstance(data, dict):
        return {}, "top-level JSON is not an object"
    return data, None


def _summary_has_rubric_block(summary_path: Path) -> bool:
    """Return True iff ``summary_path`` carries a ``rubric:`` block.

    Surface-level check: looks for either:

    - A YAML ``rubric:`` key at column 0 in the file's top frontmatter
      (delimited by ``---``).
    - A JSON top-level ``"rubric"`` key in the first JSON object the
      file contains.

    Tolerant of malformed input — returns False rather than raising.
    """
    try:
        text = summary_path.read_text(encoding="utf-8")
    except OSError:
        return False
    if "rubric" not in text:
        # Fast-path: no occurrence of the key at all.
        return False
    # Look for ``rubric:`` at column 0 in a YAML block.
    for line in text.splitlines():
        if line.startswith("rubric:") or line.startswith("rubric :"):
            return True
    # Fall back to checking JSON-shaped text. The test fixture in
    # ``tests/skills/memo/fixtures/rubric_version_transition/`` carries a
    # ``"rubric": {...}`` JSON block at the top of a JSON file. We
    # detect that by scanning for the key inside a JSON object.
    try:
        parsed = json.loads(text)
        if isinstance(parsed, dict) and "rubric" in parsed:
            return True
    except json.JSONDecodeError:
        pass
    return False


def _is_stamped(meta: Dict[str, object]) -> bool:
    """Return True iff ``meta`` carries every required stamping field."""
    for field_name in REQUIRED_STAMP_FIELDS:
        if field_name not in meta:
            return False
        # An explicit ``None`` is treated as missing — the stamping
        # contract is positive ("the rubric was X"), not nullable.
        if meta[field_name] is None:
            return False
    return True


def _count_unstamped_progress_rows(progress: Dict[str, object]) -> int:
    """Count ``score_history[]`` rows that lack ``rubric_id``."""
    metadata = progress.get("metadata")
    if not isinstance(metadata, dict):
        return 0
    history = metadata.get("score_history")
    if not isinstance(history, list):
        return 0
    unstamped = 0
    for row in history:
        if not isinstance(row, dict):
            continue
        rubric_id = row.get("rubric_id")
        if rubric_id is None or rubric_id == "":
            unstamped += 1
    return unstamped


def _infer_skill(
    review_dir: Path,
    brief_skill_map: Dict[str, str],
) -> tuple[Optional[str], str]:
    """Return (skill, source) for ``review_dir``.

    Inference rules in priority order:

    1. The version-dir stem matches a slug declared in a project BRIEF;
       use the BRIEF entry's ``artifact_type`` → skill.
    2. The sibling version dir (or any descendant) contains a known
       skill-fixed body filename (``memo.md``, ``proposal.md``, …).
    3. The sibling ``_progress.json``'s ``thread`` slug exists in the
       BRIEF skill map.
    4. Unknown.
    """
    m = _REVIEW_DIR_RE.match(review_dir.name)
    if m is None:
        return None, "unknown"
    stem = m.group("stem")
    n = m.group("num")
    parent = review_dir.parent

    # Rule 1 — BRIEF map.
    if stem in brief_skill_map:
        return brief_skill_map[stem], "brief"

    # Rule 2 — sibling body filename.
    sibling_thread = parent / f"{stem}.{n}"
    if sibling_thread.is_dir():
        # Look for canonical (slug-echoed) body name first.
        slug_echo = sibling_thread / f"{stem}.md"
        if slug_echo.is_file():
            # If stem looks like a skill name, use it; otherwise we still
            # need to look at a body or progress file to determine skill.
            # A slug-echoed body file does NOT directly identify the skill.
            pass
        for body_name, skill_name in _BODY_FILENAME_TO_SKILL.items():
            if (sibling_thread / body_name).is_file():
                return skill_name, "body-filename"

    # Rule 3 — sibling _progress.json's thread slug.
    sibling_progress = sibling_thread / PROGRESS_FILENAME
    if sibling_progress.is_file():
        progress = _safe_read_json(sibling_progress)
        thread_slug = progress.get("thread")
        if (
            isinstance(thread_slug, str)
            and thread_slug in brief_skill_map
        ):
            return brief_skill_map[thread_slug], "brief"

    # Rule 4 — give up.
    return None, "unknown"


def _load_brief_skill_map(project_tree: Path) -> Dict[str, str]:
    """Walk ``project_tree`` for ``BRIEF.md`` files; map slug → skill.

    We accept multiple BRIEFs (portfolio root with sub-project BRIEFs).
    On collision (same slug, different skill), later wins — but in
    practice the slug is the project name and collisions are rare.

    The mapping uses a small artifact-type → skill table aligned with
    the v0 skill set.
    """
    out: Dict[str, str] = {}
    if not project_tree.is_dir():
        return out

    # Map BRIEF.md ``artifact_type`` field → skill name.
    artifact_type_to_skill = {
        "investment-memo": "memo",
        "position-memo": "memo",
        "strategy-memo": "memo",
        "memo": "memo",
        "proposal": "proposal",
        "report": "report",
        "deck": "deck",
        "slides": "slides",
        "paper": "paper",
        # Legacy artifact_type strings (issue #694): the `pub` skill was
        # renamed to `paper`. A pre-rename BRIEF carrying `artifact_type:
        # pub` (or the informal `publication`) still resolves to the
        # CURRENT skill name `paper`.
        "pub": "paper",
        "publication": "paper",
        "ip-uspto": "ip-uspto",
        "patent": "ip-uspto",
        "installation": "installation",
        # Post-#366 skills (issue #482). Without these rows, BRIEF-route
        # inference (rule 1) misses datasheet / provisional / essay
        # threads even after the KNOWN_RUBRICS catalog gained their
        # entries — the planner would see ``inferred_skill is None``.
        "datasheet": "datasheet",
        "ip-uspto-provisional": "ip-uspto-provisional",
        "essay": "essay",
    }

    for brief in _walk_for_briefs(project_tree):
        try:
            text = brief.read_text(encoding="utf-8")
        except OSError:
            continue
        fm = _extract_frontmatter(text)
        if fm is None:
            continue
        docs = fm.get("documents")
        if not isinstance(docs, list):
            continue
        for entry in docs:
            if not isinstance(entry, dict):
                continue
            slug = entry.get("slug")
            artifact_type = entry.get("artifact_type")
            if not isinstance(slug, str) or not slug:
                continue
            skill_name = artifact_type_to_skill.get(
                artifact_type if isinstance(artifact_type, str) else "",
                None,
            )
            if skill_name is None:
                continue
            out[slug] = skill_name
    return out


def _walk_for_briefs(project_tree: Path) -> List[Path]:
    """Walk ``project_tree`` for ``BRIEF.md`` files. Skip rollback dirs."""
    out: List[Path] = []
    if not project_tree.is_dir():
        return out
    for path in sorted(project_tree.rglob(BRIEF_FILENAME)):
        if any(part in _SKIP_DIR_NAMES for part in path.parts):
            continue
        out.append(path)
    return out


def _extract_frontmatter(text: str) -> Optional[Dict[str, object]]:
    """Extract YAML frontmatter dict, or None when absent / unparseable.

    Mirrors the project-migrate detector's helper — tries pyyaml when
    available and falls back to a tiny hand-rolled parser. Anvil ships
    pyyaml as a base dep, so the fallback is just for paranoid hosts.
    """
    lines = text.splitlines()
    if not lines:
        return None
    if lines[0].startswith("﻿"):
        lines[0] = lines[0][1:]
    first_idx = 0
    while first_idx < len(lines) and lines[first_idx].strip() == "":
        first_idx += 1
    if first_idx >= len(lines):
        return None
    if lines[first_idx].strip() != "---":
        return None
    close_idx = None
    for i in range(first_idx + 1, len(lines)):
        if lines[i].strip() == "---":
            close_idx = i
            break
    if close_idx is None:
        return None
    yaml_text = "\n".join(lines[first_idx + 1:close_idx])
    try:
        import yaml  # type: ignore
        parsed = yaml.safe_load(yaml_text)
    except Exception:
        return _hand_parse_minimal_yaml(yaml_text)
    if not isinstance(parsed, dict):
        return None
    return parsed


def _hand_parse_minimal_yaml(yaml_text: str) -> Optional[Dict[str, object]]:
    """Minimal hand-rolled YAML parser for the project BRIEF shape.

    Recognizes ``documents:`` with ``- slug: <name>`` and optional
    ``artifact_type: <type>`` entries. Returns ``None`` when nothing
    matches.
    """
    docs: List[Dict[str, str]] = []
    in_documents = False
    current_entry: Optional[Dict[str, str]] = None
    for raw_line in yaml_text.splitlines():
        line = raw_line.rstrip()
        if not line.strip():
            continue
        if line.startswith("documents:"):
            in_documents = True
            continue
        if not in_documents:
            continue
        m_slug = re.match(r"^\s*-\s+slug:\s*(\S+)", line)
        if m_slug:
            current_entry = {"slug": m_slug.group(1).strip('"\'')}
            docs.append(current_entry)
            continue
        m_at = re.match(r"^\s+artifact_type:\s*(\S+)", line)
        if m_at and current_entry is not None:
            current_entry["artifact_type"] = m_at.group(1).strip('"\'')
            continue
        # Top-level key — exit documents block.
        if re.match(r"^[A-Za-z_]+:", line):
            in_documents = False
    if docs:
        return {"documents": docs}
    return None


def _is_review_dir(directory: Path) -> bool:
    """Return True iff ``directory`` looks like a reviewer sibling.

    The shape is ``<stem>.<N>.review/``. Critic dirs with other tags
    (``<stem>.<N>.audit/``, ``<stem>.<N>.foo/``) are NOT review dirs
    for the purpose of this skill — the post-#346 stamping contract is
    specifically about reviewer-produced reviews.
    """
    if not directory.is_dir():
        return False
    return _REVIEW_DIR_RE.match(directory.name) is not None


def _walk_for_review_dirs(project_tree: Path) -> List[Path]:
    """Walk ``project_tree`` and return every reviewer sibling dir."""
    out: List[Path] = []
    if not project_tree.is_dir():
        return out

    # We need to descend into subdirectories but skip rollback dirs.
    stack: List[Path] = [project_tree]
    while stack:
        current = stack.pop()
        try:
            children = list(current.iterdir())
        except OSError:
            continue
        for child in children:
            if not child.is_dir():
                continue
            if child.name in _SKIP_DIR_NAMES:
                continue
            if _is_review_dir(child):
                out.append(child)
                # Don't descend into a review dir.
                continue
            stack.append(child)
    out.sort()
    return out


def _build_review_snapshot(
    review_dir: Path,
    project_tree: Path,
    brief_skill_map: Dict[str, str],
) -> ReviewSnapshot:
    """Build a typed snapshot for a single review dir."""
    try:
        review_id = str(review_dir.relative_to(project_tree)).replace(
            "/", "__"
        )
    except ValueError:
        review_id = review_dir.name

    meta_path = review_dir / META_FILENAME
    meta, meta_err = _read_json_with_error(meta_path) if meta_path.is_file() else (
        {},
        f"{META_FILENAME} not found",
    )

    progress_path = None
    progress: Dict[str, object] = {}
    # The reviewer's _progress.json typically lives in the SIBLING
    # version dir, not inside the review dir. We pick the most plausible
    # candidate: the version dir paired with the review dir.
    m = _REVIEW_DIR_RE.match(review_dir.name)
    if m is not None:
        sibling_version_dir = review_dir.parent / f"{m.group('stem')}.{m.group('num')}"
        candidate = sibling_version_dir / PROGRESS_FILENAME
        if candidate.is_file():
            progress_path = candidate
            progress = _safe_read_json(candidate)
        else:
            # Some skills emit a _progress.json INSIDE the review dir.
            in_review = review_dir / PROGRESS_FILENAME
            if in_review.is_file():
                progress_path = in_review
                progress = _safe_read_json(in_review)

    summary_path = None
    summary_has_block = False
    candidate = review_dir / SUMMARY_FILENAME
    if candidate.is_file():
        summary_path = candidate
        summary_has_block = _summary_has_rubric_block(candidate)

    skill, source = _infer_skill(review_dir, brief_skill_map)

    return ReviewSnapshot(
        review_dir=review_dir,
        review_id=review_id,
        meta_path=meta_path,
        meta=meta,
        meta_parse_error=meta_err,
        progress_path=progress_path,
        progress=progress,
        summary_path=summary_path,
        summary_has_rubric_block=summary_has_block,
        inferred_skill=skill,
        skill_source=source,
        is_stamped=_is_stamped(meta) if meta else False,
        progress_score_history_unstamped_rows=(
            _count_unstamped_progress_rows(progress) if progress else 0
        ),
    )


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def inventory_tree(project_tree: Path) -> ProjectInventory:
    """Return a typed inventory of every reviewer sibling under ``project_tree``.

    Includes already-stamped reviews. Callers wanting only the
    unstamped subset should use :func:`detect_unstamped_reviews`.
    """
    project_tree = Path(project_tree).resolve()
    inv = ProjectInventory(project_tree=project_tree)
    if not project_tree.is_dir():
        return inv
    inv.project_brief_skill_map = _load_brief_skill_map(project_tree)
    for review_dir in _walk_for_review_dirs(project_tree):
        inv.reviews.append(
            _build_review_snapshot(
                review_dir, project_tree, inv.project_brief_skill_map
            )
        )
    return inv


def detect_unstamped_reviews(project_tree: Path) -> List[ReviewSnapshot]:
    """Return only the unstamped reviews under ``project_tree``.

    A review is "unstamped" when its ``_meta.json`` lacks any of the
    three required stamping fields OR when its sibling
    ``_progress.json.score_history[]`` has at least one row missing
    ``rubric_id``. The latter case catches reviews whose ``_meta.json``
    has been stamped by hand but whose progress file was missed.
    """
    inv = inventory_tree(project_tree)
    out: List[ReviewSnapshot] = []
    for review in inv.reviews:
        if not review.is_stamped:
            out.append(review)
            continue
        if review.progress_score_history_unstamped_rows > 0:
            out.append(review)
    return out


__all__ = [
    "BRIEF_FILENAME",
    "META_FILENAME",
    "PROGRESS_FILENAME",
    "ProjectInventory",
    "REQUIRED_STAMP_FIELDS",
    "ReviewSnapshot",
    "SUMMARY_FILENAME",
    "detect_unstamped_reviews",
    "inventory_tree",
]
