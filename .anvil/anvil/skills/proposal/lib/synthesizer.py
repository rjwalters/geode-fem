"""Thin clustering primitive for the proposal-synthesis sibling.

This module ships the proposal-skill-local primitive that the
``proposal-synthesize`` command (see
``anvil/skills/proposal/commands/proposal-synthesize.md``) is built on:
a small ``Synthesizer`` class that discovers critic siblings, enumerates
their findings into a flat list, hands the list to a clustering function
(callback or LLM), and post-processes the result into a fully-validated
``GapList`` (see ``anvil/skills/proposal/lib/synthesis_schema.py``).

The architectural answer is the same as ``anvil/lib/vision.py``'s
``VisionCritic``: a callback-injection seam at the single boundary call.
Everything around the boundary (sibling discovery, finding enumeration,
severity normalization, ``GapList`` construction) is pure and testable
without ever invoking a model.

This primitive lives **skill-local** under
``anvil/skills/proposal/lib/synthesizer.py`` per the CLAUDE.md
"Skill-local first, lib promotion later" discipline. The proposal skill
is the first (and currently only) consumer of synthesis; lib promotion
is deferred until a second skill adopts the pattern.

Scope (this module is intentionally minimal)
--------------------------------------------

This is the seam, not a full ``proposal-synthesize`` implementation. The
LLM-side command spec lives in ``proposal-synthesize.md``. What this
module ships is exactly the layer the Studio reproducer integration test
(sub-issue 4 of #246) needs to pin clustering shape deterministically:

- A canonical way to enumerate critic findings from sibling prose into
  a flat ``list[dict]``.
- A callback signature the test can stub with a deterministic clustering
  function.
- Post-processing that normalizes raw callback output into a validated
  ``GapList`` (severity normalization per the documented "max across
  contributors" rule; defensive dim-list de-duplication; required-field
  defaults).
- A ``write`` helper that atomically writes ``gaps.json`` next to the
  caller-supplied ``verdict.md`` / ``synthesis.md`` prose.

Out of scope for this primitive (and tracked separately):

- The full LLM-side clustering prompt and prose-output generation (lives
  in the command spec; see ``proposal-synthesize.md``).
- ``verdict.md`` + ``synthesis.md`` + ``_meta.json`` + ``_progress.json``
  prose / housekeeping writes (the orchestrator / command handles).
- State-machine integration (sub-issue 3 of #246, already merged via
  PR #271; see ``commands/proposal.md``).

Design notes
------------

1. **Callback injection is first-class.** The default path raises
   ``NotImplementedError`` — this is a pinned seam, not a backstop
   shortcut. Consumers either pass an explicit ``callback=`` (for tests
   and offline use), or build their own LLM-backed clustering function
   on top of ``enumerate_findings`` + ``build_prompt`` + ``parse_payload``
   and inject it via ``callback=``. This is the same shape as
   ``VisionCritic`` in ``anvil/lib/vision.py``.

2. **Finding enumeration is purely structural.** ``enumerate_findings``
   discovers ``<thread>.{N}.<sibling>/`` directories and reads the
   prose / JSON files each sibling is known to emit. The enumeration is
   conservative: a tolerant filename fallback (`findings.md` /
   `claim-log.md` / `audit-findings.md` — see PR #255) is honored for
   the audit sibling. The shape of each enumerated entry is the same
   dict the callback receives.

3. **Severity normalization follows the "max across contributors" rule.**
   Documented in ``proposal-synthesize.md``: the gap-level severity is
   the strongest severity among contributing findings, mapped through
   the published ladder (``critical`` flag → ``critical``; per-finding
   ``blocker`` → ``blocker``; ``major`` → ``should-fix``; ``minor`` →
   ``should-fix``; ``nit`` → ``nice-to-have``). The callback may emit
   its own severity, but ``parse_payload`` will defensively override it
   to the documented max-across rule when the callback's severity is
   weaker than what the contributing findings imply.

4. **Pre-existing critic enumeration is structural, not semantic.** The
   primitive does NOT cluster — that is the callback's job. The
   primitive just makes the inputs machine-readable enough that a
   stub clustering function can be written for tests, and that the
   real LLM clustering call has a stable input shape.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Sequence

from anvil.skills.proposal.lib.synthesis_schema import (
    SCHEMA_VERSION,
    ContributingFinding,
    Gap,
    GapList,
    Singleton,
)

# Default model. Pinned for reproducibility — consumers override with
# ``model=`` to the ``Synthesizer`` constructor. Mirrors the precedent in
# ``anvil/lib/vision.py``.
DEFAULT_MODEL = "claude-opus-4-7-20251022"


# ---------------------------------------------------------------------------
# Sibling discovery
# ---------------------------------------------------------------------------

# Critic siblings that the proposal skill's synthesizer recognizes. The
# first two are REQUIRED preconditions (the command refuses to run
# without both); the rest are OPTIONAL. The synthesizer accepts arbitrary
# additional ``<thread>.{N}.<critic>/`` siblings via the glob path in
# ``discover_siblings`` — this list is just the documented v0 set.
REQUIRED_SIBLINGS: Sequence[str] = ("review", "audit")
KNOWN_OPTIONAL_SIBLINGS: Sequence[str] = ("perspective",)


@dataclass(frozen=True)
class SiblingPaths:
    """Discovered critic-sibling directory paths for one version.

    ``version_dir`` is the bare ``<thread>.{N}/`` (the proposal); the
    keys of ``siblings`` are the sibling tags (``"review"``, ``"audit"``,
    ``"perspective"``, …) and the values are the corresponding
    ``<thread>.{N}.<tag>/`` directories.
    """

    version_dir: Path
    siblings: Dict[str, Path]


def discover_siblings(version_dir: Path) -> SiblingPaths:
    """Discover all critic siblings at the same version as ``version_dir``.

    Globs for ``<version_dir_stem>.*`` next to ``version_dir``, drops the
    bare version directory and the ``.synthesis/`` sibling itself, and
    returns the remainder keyed by tag.

    Parameters
    ----------
    version_dir:
        Path to the bare proposal version directory (e.g.
        ``some/path/raytheon-pitch-strategy.1``).

    Returns
    -------
    A ``SiblingPaths`` with ``siblings`` mapping tag → path for every
    discovered critic-sibling directory.
    """
    version_dir = Path(version_dir)
    parent = version_dir.parent
    stem = version_dir.name  # e.g. "raytheon-pitch-strategy.1"

    siblings: Dict[str, Path] = {}
    if not parent.exists():
        return SiblingPaths(version_dir=version_dir, siblings=siblings)

    prefix = stem + "."
    for child in sorted(parent.iterdir()):
        if not child.is_dir():
            continue
        name = child.name
        if name == stem:
            continue
        if not name.startswith(prefix):
            continue
        tag = name[len(prefix):]
        # The synthesis sibling itself is not a critic of itself.
        if tag == "synthesis":
            continue
        siblings[tag] = child

    return SiblingPaths(version_dir=version_dir, siblings=siblings)


def required_siblings_present(paths: SiblingPaths) -> bool:
    """Return True iff both REQUIRED siblings have a ``verdict.md``.

    Matches the precondition documented in
    ``proposal-synthesize.md`` step 1: BOTH ``review`` and ``audit``
    MUST be present (and have a ``verdict.md``) before synthesis runs.
    """
    for tag in REQUIRED_SIBLINGS:
        sib = paths.siblings.get(tag)
        if sib is None:
            return False
        if not (sib / "verdict.md").exists():
            return False
    return True


# ---------------------------------------------------------------------------
# Finding enumeration
# ---------------------------------------------------------------------------

# Tolerant filename aliases for the audit sibling's findings file
# (matches the contract documented in PR #255).
AUDIT_FINDINGS_ALIASES: Sequence[str] = (
    "findings.md",
    "claim-log.md",
    "audit-findings.md",
)


@dataclass
class RawFinding:
    """One enumerated critic-side finding, pre-clustering.

    Each finding is a structural pointer back to a sibling output, plus
    whatever attributes the synthesizer needs to feed into clustering:
    a short prose summary, an optional severity, and the rubric
    dimensions it touches.

    ``ref`` is a conventional dot-path or section pointer (e.g.
    ``"dim6.comment.3"``, ``"findings.12lp_line"``). The synthesizer
    primitive does not parse or validate the ref shape — it is the
    callback's responsibility to interpret it.
    """

    sibling: str
    ref: str
    summary: str = ""
    # Per-finding severity from the critic side (vocabulary mirrors
    # ``anvil/lib/review_schema.py::Finding.severity``: ``blocker`` /
    # ``major`` / ``minor`` / ``nit``). May also be the string
    # ``"critical"`` to indicate a critical-flag-promoted finding.
    severity: Optional[str] = None
    rubric_dimensions: Sequence[int] = ()

    def as_dict(self) -> dict:
        return {
            "sibling": self.sibling,
            "ref": self.ref,
            "summary": self.summary,
            "severity": self.severity,
            "rubric_dimensions": list(self.rubric_dimensions),
        }


def _find_audit_findings_file(sib_dir: Path) -> Optional[Path]:
    """Return the first matching audit findings file or ``None``.

    Honors the tolerant filename aliases documented in PR #255.
    """
    for name in AUDIT_FINDINGS_ALIASES:
        candidate = sib_dir / name
        if candidate.exists():
            return candidate
    return None


# A finding marker in a sibling prose file. The synthesizer recognizes a
# light convention used in the v0 fixtures and the studio reproducer:
#
#   - A hash anchor at the start of a section heading, e.g.
#     ``### F: dim6.comment.3 — 12LP+ mask cost lacks anchor [major,dim6]``
#
# The format is intentionally loose: the synthesizer's job is to extract
# enough structure for the callback to act on, not to enforce a strict
# DSL on the critic-side prose. Critics that emit ``_review.json`` (the
# v1 canonical contract per ``anvil/lib/review_schema.py``) feed in via
# the ``RawFinding`` constructor directly; no parsing is needed for that
# path.
_FINDING_HEADER_RE = re.compile(
    r"""
    ^\#{1,6}\s+                            # markdown heading
    F\s*:\s*                               # "F:" marker
    (?P<ref>[\w\.\-]+)                     # the ref token
    (?:\s+[-—]\s*(?P<summary>[^\[\n]+))?  # optional summary
    (?:\s*\[(?P<attrs>[^\]]+)\])?          # optional [attr,attr,...]
    \s*$
    """,
    re.MULTILINE | re.VERBOSE,
)


def _parse_finding_attrs(attrs: str) -> Dict[str, Any]:
    """Parse the ``[major,dim6,dim7]`` style attributes block.

    Recognized tokens:
    - severity vocabulary: ``critical`` / ``blocker`` / ``major`` /
      ``minor`` / ``nit``
    - rubric dim references: ``dimN`` or ``dN`` where N is an integer
    """
    parsed: Dict[str, Any] = {"severity": None, "rubric_dimensions": []}
    if not attrs:
        return parsed
    sev_vocab = {"critical", "blocker", "major", "minor", "nit"}
    for raw in attrs.split(","):
        token = raw.strip().lower()
        if not token:
            continue
        if token in sev_vocab:
            parsed["severity"] = token
            continue
        m = re.fullmatch(r"d(?:im)?(\d+)", token)
        if m:
            parsed["rubric_dimensions"].append(int(m.group(1)))
    return parsed


def _enumerate_from_file(sibling: str, path: Path) -> List[RawFinding]:
    """Enumerate F:-marker findings from one sibling's prose file."""
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8")
    out: List[RawFinding] = []
    for m in _FINDING_HEADER_RE.finditer(text):
        ref = m.group("ref")
        summary = (m.group("summary") or "").strip()
        attrs = _parse_finding_attrs(m.group("attrs") or "")
        out.append(
            RawFinding(
                sibling=sibling,
                ref=ref,
                summary=summary,
                severity=attrs["severity"],
                rubric_dimensions=tuple(attrs["rubric_dimensions"]),
            )
        )
    return out


def enumerate_findings(paths: SiblingPaths) -> List[RawFinding]:
    """Walk every discovered sibling and collect its findings.

    The synthesizer's documented finding-enumeration step (see
    ``proposal-synthesize.md`` step 6) yields a flat list of all
    cross-sibling findings, each tagged with the sibling that produced
    it. This helper is the pure / testable realization.

    For each sibling:

    - ``review`` → reads ``comments.md`` (and ``scoring.md`` if
      ``comments.md`` is absent).
    - ``audit``  → reads the first matching audit findings file per the
      tolerant aliases in ``AUDIT_FINDINGS_ALIASES``.
    - ``perspective`` → reads ``candidates.md``.
    - any other tag → reads ``findings.md`` if present, falling back to
      ``comments.md``.

    Findings are extracted using the lightweight ``### F: <ref> — ...``
    convention (see module docstring). Critics that prefer the v1
    canonical ``_review.json`` contract are responsible for emitting
    their findings as ``RawFinding`` instances directly (this primitive
    is the structural fallback for prose-only critics).
    """
    findings: List[RawFinding] = []
    for tag, sib_dir in paths.siblings.items():
        if tag == "audit":
            target = _find_audit_findings_file(sib_dir)
            if target is not None:
                findings.extend(_enumerate_from_file(tag, target))
        elif tag == "review":
            target = sib_dir / "comments.md"
            if not target.exists():
                target = sib_dir / "scoring.md"
            findings.extend(_enumerate_from_file(tag, target))
        elif tag == "perspective":
            target = sib_dir / "candidates.md"
            findings.extend(_enumerate_from_file(tag, target))
        else:
            target = sib_dir / "findings.md"
            if not target.exists():
                target = sib_dir / "comments.md"
            findings.extend(_enumerate_from_file(tag, target))
    return findings


# ---------------------------------------------------------------------------
# Severity normalization
# ---------------------------------------------------------------------------

# Per-finding severity → gap-level severity ladder. Documented in
# ``proposal-synthesize.md`` §"Compose each Gap" step 8.
_GAP_SEVERITIES = ("nice-to-have", "should-fix", "blocker", "critical")
_GAP_SEVERITY_ORDER = {s: i for i, s in enumerate(_GAP_SEVERITIES)}

# Map critic-side severity vocabulary to gap-level severity.
_CRITIC_TO_GAP_SEVERITY = {
    "critical": "critical",
    "blocker": "blocker",
    "major": "should-fix",
    "minor": "should-fix",
    "nit": "nice-to-have",
}


def gap_severity_from_contributors(
    contributors: Sequence[RawFinding],
) -> str:
    """Return the max-across-contributors gap severity.

    Implements the documented "severity is the max across contributors"
    rule. A ``critical`` flag on any contributor short-circuits to
    ``critical``; otherwise the strongest mapped severity wins. A
    contributor with no severity contributes nothing to the max
    (treated as ``nice-to-have`` for ordering).
    """
    best = "nice-to-have"
    best_rank = 0
    for c in contributors:
        sev = c.severity or "nit"
        mapped = _CRITIC_TO_GAP_SEVERITY.get(sev, "nice-to-have")
        rank = _GAP_SEVERITY_ORDER[mapped]
        if rank > best_rank:
            best_rank = rank
            best = mapped
    return best


# ---------------------------------------------------------------------------
# Callback contract
# ---------------------------------------------------------------------------

# A synthesis callback receives the enumerated findings, the proposal
# context, and an assembled prompt, and returns a dict matching the
# clustering payload schema below:
#
#     {
#         "gaps": [
#             {
#                 "id": "g-12lp-mask-cost",
#                 "contributing_refs": [
#                     {"sibling": "review", "ref": "dim6.comment.3"},
#                     {"sibling": "audit",  "ref": "findings.12lp_line"},
#                     {"sibling": "perspective", "ref": "candidates.cluster_foundry_pricing"},
#                 ],
#                 "root_concern": "...",
#                 "recommended_response": "...",
#                 "severity": "should-fix",       # OPTIONAL; defaults to max-across
#                 "rubric_dimensions": [6],       # OPTIONAL
#             },
#             ...
#         ],
#         "singletons": [
#             {"sibling": "review", "ref": "dim7.comment.1",
#              "note": "stylistic finding, no overlap"},
#             ...
#         ]
#     }
#
# Missing keys default to empty lists. The callback's ``severity`` is
# accepted if present, but ``parse_payload`` defensively overrides it to
# the max-across-contributors value if the callback's severity is
# weaker than what contributing findings imply (the documented "max"
# rule is the contract; the callback gets the benefit of the doubt only
# when its severity is at least as strong as the rule would compute).
SynthesisCallback = Callable[
    [List[dict], Optional[str], str], dict
]


# ---------------------------------------------------------------------------
# Prompt template
# ---------------------------------------------------------------------------


def build_prompt(
    enumerated: Sequence[RawFinding],
    proposal_text: Optional[str] = None,
) -> str:
    """Build the JSON-instruction prompt sent to the clustering model.

    The prompt enumerates the findings and asks for a JSON payload
    matching the callback contract above. Kept short to leave room for
    the proposal context.
    """
    lines: List[str] = []
    lines.append(
        "You are a proposal-synthesis clustering agent. Given the "
        "findings emitted by N parallel critic siblings, cluster those "
        "that name the same underlying gap into a single Gap; surface "
        "the rest as Singletons. Cluster CONSERVATIVELY — when in "
        "doubt, leave the finding as a singleton."
    )
    lines.append("")
    lines.append("Cluster ACROSS siblings, not within. Two findings from "
                 "the same sibling are never a cluster.")
    lines.append("")
    lines.append("Findings to cluster:")
    for f in enumerated:
        sev_str = f.severity or "—"
        dim_str = (
            "dim=" + ",".join(str(d) for d in f.rubric_dimensions)
            if f.rubric_dimensions else "dim=—"
        )
        summary = f.summary or "(no summary)"
        lines.append(
            f"- [{f.sibling}] {f.ref} ({sev_str}; {dim_str}) — {summary}"
        )
    lines.append("")
    if proposal_text:
        # Truncate; the prompt should never balloon. The model gets
        # enough context to point at evidence spans.
        snippet = proposal_text.strip()
        if len(snippet) > 4000:
            snippet = snippet[:4000] + "\n[... truncated ...]"
        lines.append("Proposal context (for evidence-span pointing):")
        lines.append(snippet)
        lines.append("")
    lines.append(
        "Return JSON ONLY (no markdown wrapper, no commentary): "
        '{"gaps": [{"id": "g-<kebab>", "contributing_refs": '
        '[{"sibling": "<tag>", "ref": "<ref>"}, ...], '
        '"root_concern": "<1-2 sentences>", '
        '"recommended_response": "<1-2 sentences, concrete>", '
        '"severity": "critical|blocker|should-fix|nice-to-have|null", '
        '"rubric_dimensions": [<int>, ...]}, ...], '
        '"singletons": [{"sibling": "<tag>", "ref": "<ref>", '
        '"note": "<one line|null>"}, ...]}'
    )
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Payload parsing
# ---------------------------------------------------------------------------


def _strongest_severity(a: str, b: str) -> str:
    """Return whichever of ``a``, ``b`` ranks higher in the gap ladder."""
    return a if _GAP_SEVERITY_ORDER[a] >= _GAP_SEVERITY_ORDER[b] else b


def parse_payload(
    payload: dict,
    enumerated: Sequence[RawFinding],
    for_version: int,
    thread: Optional[str] = None,
) -> GapList:
    """Map a callback payload to a validated ``GapList``.

    Severity normalization: for each gap the callback emits, compute the
    documented max-across-contributors severity from the enumerated
    findings the gap references, then take the strongest of (callback
    severity, max-across) as the final gap severity. This honors the
    documented "max across contributors" rule while still letting the
    callback explicitly *escalate* (e.g. flag a gap as critical because
    of a cross-cutting concern the contributors did not individually
    name as critical).
    """
    # Build a ref-lookup so we can resolve callback refs back to the
    # enumerated findings (for severity normalization and dim union).
    by_key: Dict[tuple, RawFinding] = {
        (f.sibling, f.ref): f for f in enumerated
    }

    gaps_out: List[Gap] = []
    for g in payload.get("gaps", []) or []:
        refs_in = g.get("contributing_refs") or g.get("contributing_findings") or []
        contributing: List[ContributingFinding] = []
        contributors: List[RawFinding] = []
        for r in refs_in:
            sib = r.get("sibling", "")
            ref = r.get("ref", "")
            if not sib or not ref:
                continue
            contributing.append(ContributingFinding(sibling=sib, ref=ref))
            found = by_key.get((sib, ref))
            if found is not None:
                contributors.append(found)
        if not contributing:
            # A gap with no contributing findings is structurally a
            # singleton — skip it (the schema would reject it anyway).
            continue

        # Severity normalization.
        rule_sev = gap_severity_from_contributors(contributors)
        cb_sev = g.get("severity")
        if cb_sev in _GAP_SEVERITY_ORDER:
            final_sev = _strongest_severity(rule_sev, cb_sev)
        else:
            final_sev = rule_sev

        # Rubric dimensions: union of callback dims and contributor dims.
        cb_dims = g.get("rubric_dimensions") or []
        dims: List[int] = []
        seen: set = set()
        for d in cb_dims:
            try:
                d_int = int(d)
            except (TypeError, ValueError):
                continue
            if d_int not in seen:
                seen.add(d_int)
                dims.append(d_int)
        for c in contributors:
            for d in c.rubric_dimensions:
                if d not in seen:
                    seen.add(d)
                    dims.append(d)

        gaps_out.append(
            Gap(
                id=g.get("id", "g-unnamed"),
                contributing_findings=contributing,
                root_concern=(g.get("root_concern") or "").strip(),
                recommended_response=(
                    g.get("recommended_response") or ""
                ).strip(),
                severity=final_sev,  # type: ignore[arg-type]
                rubric_dimensions=dims,
            )
        )

    singletons_out: List[Singleton] = []
    for s in payload.get("singletons", []) or []:
        sib = s.get("sibling", "")
        ref = s.get("ref", "")
        if not sib or not ref:
            continue
        singletons_out.append(
            Singleton(
                sibling=sib,
                ref=ref,
                note=(s.get("note") or None),
            )
        )

    return GapList(
        schema_version=SCHEMA_VERSION,
        for_version=for_version,
        thread=thread,
        gaps=gaps_out,
        singletons=singletons_out,
    )


# ---------------------------------------------------------------------------
# Synthesizer
# ---------------------------------------------------------------------------


class Synthesizer:
    """A clustering primitive that consolidates cross-sibling findings.

    The default path raises ``NotImplementedError`` on ``synthesize`` —
    this primitive is a *seam*, not a backstop. Consumers either pass
    an explicit ``callback`` (for tests and offline use) or build an
    LLM-backed clustering function on top of ``enumerate_findings`` +
    ``build_prompt`` + ``parse_payload`` and inject it via
    ``callback=``. The full LLM-backed path is documented in
    ``proposal-synthesize.md``.

    Parameters
    ----------
    callback:
        Optional clustering callable with signature
        ``(enumerated_findings: list[dict], proposal_text: str|None, prompt: str) -> dict``
        that returns the raw clustering payload. When provided, no LLM
        call is made. Test suites pass a deterministic stub here.
    model:
        Recorded for reproducibility on ``_meta.json``. Defaults to
        ``DEFAULT_MODEL``.
    critic_id:
        Identifier for the synthesizer (recorded on ``_meta.json``).
        Defaults to ``"synthesis"``.
    """

    def __init__(
        self,
        callback: Optional[SynthesisCallback] = None,
        model: str = DEFAULT_MODEL,
        critic_id: str = "synthesis",
    ) -> None:
        self.callback = callback
        self.model = model
        self.critic_id = critic_id

    # -- Public API ---------------------------------------------------------

    def synthesize(
        self,
        version_dir: Path,
        for_version: int,
        thread: Optional[str] = None,
        extra_findings: Optional[Sequence[RawFinding]] = None,
    ) -> GapList:
        """Cluster findings across critic siblings and return a ``GapList``.

        Parameters
        ----------
        version_dir:
            Path to the bare proposal version directory.
        for_version:
            The ``N`` of the version being synthesized. Surfaced on the
            resulting ``GapList.for_version``.
        thread:
            Optional thread slug for the resulting ``GapList.thread``.
        extra_findings:
            Optional list of additional ``RawFinding`` instances to fold
            into the enumeration. This is the seam consumers use to
            inject ``_review.json``-sourced findings alongside the
            prose-discovered ones.

        Returns
        -------
        A fully-validated ``GapList``.

        Raises
        ------
        NotImplementedError
            If no ``callback`` was provided. Consumers must supply a
            callback (test stub or LLM-backed clustering function).
        FileNotFoundError
            If ``version_dir`` does not exist.
        ValueError
            If both REQUIRED siblings (review + audit) are not present.
        """
        version_dir = Path(version_dir)
        if not version_dir.exists():
            raise FileNotFoundError(
                f"version_dir does not exist: {version_dir}"
            )

        paths = discover_siblings(version_dir)
        if not required_siblings_present(paths):
            raise ValueError(
                "both review and audit are required before "
                "synthesizing; run the missing critic first"
            )

        enumerated = enumerate_findings(paths)
        if extra_findings:
            enumerated = list(enumerated) + list(extra_findings)

        # Read proposal context (best-effort).
        proposal_text: Optional[str] = None
        for candidate in ("proposal.tex", "proposal.md"):
            p = version_dir / candidate
            if p.exists():
                proposal_text = p.read_text(encoding="utf-8")
                break

        prompt = build_prompt(enumerated, proposal_text=proposal_text)

        if self.callback is None:
            raise NotImplementedError(
                "Synthesizer requires an explicit callback. Pass "
                "callback= to the constructor (or build an LLM-backed "
                "clustering function on top of enumerate_findings + "
                "build_prompt + parse_payload)."
            )

        payload = self.callback(
            [f.as_dict() for f in enumerated],
            proposal_text,
            prompt,
        )

        return parse_payload(
            payload=payload,
            enumerated=enumerated,
            for_version=for_version,
            thread=thread,
        )

    def write(self, gaps: GapList, target: Path) -> Path:
        """Write a validated ``GapList`` to ``target`` as ``gaps.json``.

        ``target`` may be either the synthesis directory (in which case
        ``gaps.json`` is appended) or the explicit file path. A target
        whose ``name`` ends in ``.json`` is treated as the file path;
        anything else is treated as the synthesis directory. The
        directory is created if it does not exist. Atomic write via
        write-then-rename.
        """
        target = Path(target)
        if target.name.endswith(".json"):
            target.parent.mkdir(parents=True, exist_ok=True)
            out = target
        else:
            target.mkdir(parents=True, exist_ok=True)
            out = target / "gaps.json"

        tmp = out.with_suffix(out.suffix + ".tmp")
        tmp.write_text(
            json.dumps(gaps.model_dump(mode="json"), indent=2) + "\n",
            encoding="utf-8",
        )
        tmp.replace(out)
        return out


__all__ = [
    "DEFAULT_MODEL",
    "REQUIRED_SIBLINGS",
    "KNOWN_OPTIONAL_SIBLINGS",
    "AUDIT_FINDINGS_ALIASES",
    "SiblingPaths",
    "RawFinding",
    "SynthesisCallback",
    "Synthesizer",
    "discover_siblings",
    "required_siblings_present",
    "enumerate_findings",
    "gap_severity_from_contributors",
    "build_prompt",
    "parse_payload",
]
