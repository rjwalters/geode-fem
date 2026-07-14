"""Deterministic rhetoric lint (anti-trope / banned-phrase / AI-tell scan).

Anvil's rubric dim 9 (*Rhetorical economy*) is judged by critics but was
never linted. This module adds a deterministic **rhetoric lint** to the
pre-flight family ("deterministic pre-flight before judgment" is a core
anvil principle): rule-set-driven phrase/trope/AI-tell scanning over body
markdown, producing **advisory** findings that downstream consumers (the
memo render gate's ``memo_rhetoric_lint`` dimension, issue #463) surface
as mechanical evidence for dim 9 scoring. Rhetoric rules have irreducible
false positives (quoted material, deliberate style), so this lint never
blocks: findings are warning severity at most, and the judgment call
stays with the dim 9 critics.

The rule-set SHAPE reimplements draftwell's ``packages/styleguide/``
named-rule-set model in pure stdlib Python (no TypeScript port). Three
rule kinds:

- ``phrase`` — a case-insensitive, word-boundary literal. Straight
  apostrophes in the pattern also match the typographic apostrophe
  (``'`` matches ``’``).
- ``regex`` — compiled as written, with ``re.IGNORECASE`` applied
  (the lint is a vocabulary check; casing never changes the verdict).
- ``frequency`` — a literal token counted per 1000 words of scanned
  text against ``max_per_1000_words`` (e.g. the em-dash density rule:
  more than 8 ``—`` per 1000 words is the documented AI-tell). A
  ``min_words`` floor (default :data:`DEFAULT_FREQUENCY_MIN_WORDS`)
  keeps density estimates from firing on statistically tiny texts.

JSON rule-set schema (consumer files)
-------------------------------------

Consumer rule files are JSON with this shape (identical to the dict
shape of :data:`DEFAULT_RHETORIC_RULES`, so this module is
self-documenting for consumer files)::

    {
      "name": "consumer-rules",
      "rules": [
        {"id": "no-delve", "kind": "phrase", "pattern": "delve",
         "message": "...", "severity": "warning"},
        {"id": "no-tapestry", "kind": "regex",
         "pattern": "\\\\btapestr(y|ies)\\\\b", "message": "..."},
        {"id": "no-opening-emdash", "kind": "regex", "scope": "first-line",
         "pattern": "[—–]", "message": "..."},
        {"id": "em-dash-density", "kind": "frequency", "pattern": "—",
         "max_per_1000_words": 8, "message": "..."}
      ],
      "disable": ["<default-rule-id>", "..."]
    }

Positional scope: ``phrase`` and ``regex`` rules accept an optional
``scope`` key. The default ``"body"`` evaluates the rule on every
non-excluded line (the original behavior). ``"first-line"`` restricts
the rule to the document's **first prose line** — the first non-blank
body line after skipping a leading YAML front-matter block and any
heading lines (layered on top of the fenced-code / comment / inline-code
exclusions). This makes document-positional tells expressible: e.g. the
``no-opening-emdash`` default fires on an em-dash in the opening line
regardless of overall density, but not on the same em-dash mid-document.
Unknown or absent ``scope`` coerces to ``"body"``. ``scope`` is
meaningless for ``frequency`` rules (frequency is always document-level)
and is not stored on them.

Merge semantics: consumer rules are appended to the framework defaults;
a consumer rule whose ``id`` collides with a default **replaces** it;
``disable`` switches off rules by id. ``severity`` defaults to
``"warning"``; consumers may downgrade to ``"info"`` but never upgrade
to ``"error"`` — an ``"error"`` (or any unknown severity) is coerced
back to ``"warning"`` because the dimension is advisory by contract.

Graceful degradation: malformed consumer JSON (unparseable file, or a
top-level shape that is not the documented object) produces a
defaults-only run plus ONE warning finding naming the parse error (the
``customer_context.py`` broken-declaration posture: a broken
declaration is surfaced, not silently ignored). An individually
malformed rule inside an otherwise valid file is skipped with a
warning finding naming the rule; the remaining rules still apply.

Scan exclusions
---------------

Fenced code blocks (``` / ~~~), HTML comments (including multi-line),
and inline code spans are excluded from the scan: code samples must not
fire the lint, and the suppression directive must not self-match.

Suppression
-----------

Per-line suppression follows the established ``anvil-lint-disable``
contract (marp_lint / memo_image_refs / render_gate): a directive of
the form ``<!-- anvil-lint-disable: memo_rhetoric_lint -->`` on the
same line as a hit, or on the line directly above it, downgrades that
line's phrase/regex hits to info findings (surfaced, not hidden).
Frequency findings are document-level (no line) and have no per-line
suppression surface; consumers tune or ``disable`` the rule instead.

Default rule set
----------------

:data:`DEFAULT_RHETORIC_RULES` is the in-module default set (the
``DEFAULT_PLACEHOLDER_PATTERNS`` precedent): ~25 conservative,
high-confidence AI-tells. Inclusion bar: the phrase must be (a) a
documented LLM-overuse marker and (b) rare in competent human
business/technical prose. Common discourse markers (``moreover``,
``furthermore``, ``however``) are explicitly excluded — too many false
positives on good prose.

Public API
----------

- ``lint_rhetoric(text, *, extra_rules=None, extra_rules_path=None)``
  → :class:`RhetoricLintResult` (mirrors ``marp_lint.LintResult``:
  findings list + ``to_json()``). Standalone-callable so review-phase
  commands and non-render_gate skills can adopt without the gate.
- ``DEFAULT_RHETORIC_RULES`` — the framework default rule set.

Pure stdlib (``re``, ``json``, ``dataclasses``, ``pathlib``) — no
pydantic, no third-party imports (issue #463 acceptance criterion).
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional, Sequence, Union


# Rule kinds ------------------------------------------------------------------

RULE_KIND_PHRASE = "phrase"
RULE_KIND_REGEX = "regex"
RULE_KIND_FREQUENCY = "frequency"
_VALID_KINDS = (RULE_KIND_PHRASE, RULE_KIND_REGEX, RULE_KIND_FREQUENCY)

# Severities. The dimension is advisory by contract: ``warning`` is the
# ceiling. Anything else (notably ``"error"``) is coerced to ``warning``.
SEVERITY_WARNING = "warning"
SEVERITY_INFO = "info"
_VALID_SEVERITIES = (SEVERITY_WARNING, SEVERITY_INFO)

# Pseudo-rule id used for configuration problems (malformed consumer
# JSON, invalid individual rules). Config findings are warning severity
# so a broken declaration is surfaced, not silently ignored.
CONFIG_RULE_ID = "rhetoric_lint_config"

# Frequency rules need a minimum corpus before a per-1000-words density
# is meaningful (1 em-dash in a 40-word abstract is 25/1000 — noise,
# not signal). Rule-overridable via the ``min_words`` key.
DEFAULT_FREQUENCY_MIN_WORDS = 50

# Em-dash density ceiling for the default frequency rule. The
# consumer's own em-dash counting precedent (rjwalters.info
# VOCABULARY.md): sustained density above 8 per 1000 words is the
# documented AI-tell in business/technical prose.
EMDASH_MAX_PER_1000_WORDS = 8

# Suppression-directive rule tokens honored by default. The memo gate's
# dimension name is the documented consumer-facing token
# (``<!-- anvil-lint-disable: memo_rhetoric_lint -->``); the generic
# ``rhetoric_lint`` token works for standalone (non-gate) callers.
DEFAULT_SUPPRESS_RULES: tuple[str, ...] = (
    "memo_rhetoric_lint",
    "rhetoric_lint",
)


# Default rule set --------------------------------------------------------------
#
# Conservative, high-confidence AI-tells only (curation Decision 2,
# issue #463). Every entry is calibrated against the repo's memo-prose
# corpus (templates + fixture memo bodies) — the enforced
# zero-findings-on-good-prose bar in tests/lib/test_rhetoric_lint.py.
DEFAULT_RHETORIC_RULES: tuple[dict, ...] = (
    {
        "id": "no-delve",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bdelv(e|es|ed|ing)\b",
        "message": "'delve' is a documented LLM-overuse marker; prefer a plain verb (examine, explore, look at).",
    },
    {
        "id": "no-tapestry",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\btapestr(y|ies)\b",
        "message": "'tapestry' (rich tapestry, tapestry of ...) is a documented AI-tell metaphor; say what the parts actually are.",
    },
    {
        "id": "no-important-to-note",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bit(?:['’]s| is) important to note\b",
        "message": "'it's important to note' is filler; if it matters, state it directly.",
    },
    {
        "id": "no-worth-noting",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bit(?:['’]s| is) worth noting\b",
        "message": "'it's worth noting' is filler; if it's worth noting, just note it.",
    },
    {
        "id": "no-fast-paced",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bin today['’]s fast-paced\b|\bfast-paced (?:world|environment|landscape|digital)\b",
        "message": "'in today's fast-paced ...' is an AI-tell opener; cut the throat-clearing and lead with the claim.",
    },
    {
        "id": "no-testament-to",
        "kind": RULE_KIND_PHRASE,
        "pattern": "a testament to",
        "message": "'a testament to' is an AI-tell intensifier; show the evidence instead of labeling it.",
    },
    {
        "id": "no-end-of-the-day",
        "kind": RULE_KIND_PHRASE,
        "pattern": "at the end of the day",
        "message": "'at the end of the day' is a hedge-cliché; state the conclusion without the ramp.",
    },
    {
        "id": "no-serves-as-a",
        "kind": RULE_KIND_PHRASE,
        "pattern": "serves as a",
        "message": "'serves as a' is indirection; prefer 'is' or the concrete verb.",
    },
    {
        "id": "no-crucial-role",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bplay(?:s|ed|ing)? a (?:crucial|vital|pivotal|key) role\b",
        "message": "'plays a crucial/vital role' is an AI-tell construction; name what the thing actually does.",
    },
    {
        "id": "no-seamless",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bseamless(?:ly)?\b",
        "message": "'seamless(ly)' is marketing filler and a documented LLM marker; describe the actual integration behavior.",
    },
    {
        "id": "no-navigate-complexities",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bnavigat(?:e|es|ed|ing) the complexit(?:y|ies)\b",
        "message": "'navigate the complexities' is an AI-tell metaphor; name the specific difficulty.",
    },
    {
        "id": "no-realm-of",
        "kind": RULE_KIND_PHRASE,
        "pattern": "in the realm of",
        "message": "'in the realm of' is an AI-tell scoping phrase; prefer 'in' or name the field plainly.",
    },
    {
        "id": "no-multifaceted",
        "kind": RULE_KIND_PHRASE,
        "pattern": "multifaceted",
        "message": "'multifaceted' is a documented LLM marker; enumerate the facets instead.",
    },
    {
        "id": "no-underscores-verb",
        "kind": RULE_KIND_REGEX,
        # Inflected verb forms only — the bare noun "underscore" (the
        # character) is deliberately excluded for technical prose.
        "pattern": r"\bunderscor(?:es|ed|ing)\b",
        "message": "'underscores the ...' is an AI-tell verb; prefer 'shows', 'confirms', or drop the sentence.",
    },
    {
        "id": "no-ever-evolving",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bever-(?:evolving|changing)\b",
        "message": "'ever-evolving/ever-changing' is an AI-tell modifier; cut it or cite the actual change.",
    },
    {
        "id": "no-embark-journey",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bembark(?:s|ed|ing)? (?:up)?on a journey\b",
        "message": "'embark on a journey' is an AI-tell metaphor; say what is starting.",
    },
    {
        "id": "no-harness-power",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bharness(?:es|ed|ing)? the power of\b",
        "message": "'harness the power of' is marketing filler; name the capability being used.",
    },
    {
        "id": "no-unlock-potential",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bunlock(?:s|ed|ing)? the (?:full )?(?:potential|power)\b",
        "message": "'unlock the potential/power' is marketing filler; state the concrete benefit.",
    },
    {
        "id": "no-goes-without-saying",
        "kind": RULE_KIND_PHRASE,
        "pattern": "it goes without saying",
        "message": "'it goes without saying' — then don't say it, or say it without the preamble.",
    },
    {
        "id": "no-myriad-of",
        "kind": RULE_KIND_PHRASE,
        "pattern": "a myriad of",
        "message": "'a myriad of' is an AI-tell quantifier; give a number or say 'many'.",
    },
    {
        "id": "no-plethora",
        "kind": RULE_KIND_PHRASE,
        "pattern": "plethora",
        "message": "'plethora' is a documented LLM marker; give a number or say 'many'.",
    },
    {
        "id": "no-look-no-further",
        "kind": RULE_KIND_PHRASE,
        "pattern": "look no further",
        "message": "'look no further' is marketing copy; state the recommendation directly.",
    },
    {
        "id": "no-meticulously-x",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bmeticulously (?:crafted|designed|curated)\b",
        "message": "'meticulously crafted/designed/curated' is an AI-tell intensifier; describe the actual care taken.",
    },
    {
        "id": "no-ai-model-leak",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bas an? (?:AI|artificial intelligence)(?: language)? model\b",
        "message": "Assistant-persona leak ('as an AI model ...') — remove the chat artifact from the document body.",
    },
    {
        "id": "no-finds-you-well",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bhopes? this (?:email|message|letter|memo) finds you well\b",
        "message": "'hope this message finds you well' is boilerplate; open with the point.",
    },
    {
        "id": "no-game-changer",
        "kind": RULE_KIND_REGEX,
        "pattern": r"\bgame[- ]chang(?:er|ers|ing)\b",
        "message": "'game-changer' is hype vocabulary; quantify the change instead.",
    },
    {
        "id": "no-opening-emdash",
        "kind": RULE_KIND_REGEX,
        "scope": "first-line",
        # Unicode em-dash (U+2014) and en-dash (U+2013) only — the
        # dash-variant fold used by ``em-dash-density``. The Markdown
        # ``--``/``---`` shorthands are deliberately excluded: on the
        # first prose line they collide with a thematic break (``---``)
        # and would false-positive. Positional, not density: fires on
        # ANY opening-line em-dash regardless of overall frequency.
        "pattern": r"[—–]",
        "message": "Opening line contains an em-dash — a documented generic-AI-cadence tell; rewrite the opening without em-dashes.",
    },
    {
        "id": "em-dash-density",
        "kind": RULE_KIND_FREQUENCY,
        "pattern": "—",
        "max_per_1000_words": EMDASH_MAX_PER_1000_WORDS,
        "message": "Em-dash density exceeds the AI-tell threshold; vary punctuation (commas, colons, parentheses, periods).",
    },
)


# Result types ------------------------------------------------------------------


@dataclass
class RhetoricFinding:
    """One rhetoric-lint hit (or config problem)."""

    rule_id: str
    severity: str            # "warning" | "info"
    message: str
    line: Optional[int] = None      # 1-based source line; None for document-level
    match: Optional[str] = None     # the matched text, when line-anchored

    def to_dict(self) -> dict:
        return {
            "rule_id": self.rule_id,
            "severity": self.severity,
            "message": self.message,
            "line": self.line,
            "match": self.match,
        }


@dataclass
class RhetoricLintResult:
    """Outcome of one rhetoric-lint pass.

    Mirrors ``marp_lint.LintResult``: a findings list plus
    ``to_json()``. There is no ``errors`` bucket by design — the lint
    is advisory and ``warning`` is the severity ceiling.
    """

    findings: list[RhetoricFinding] = field(default_factory=list)
    words: int = 0                       # words in the scanned (non-excluded) text
    rules_applied: list[str] = field(default_factory=list)

    @property
    def warnings(self) -> list[RhetoricFinding]:
        return [f for f in self.findings if f.severity == SEVERITY_WARNING]

    @property
    def infos(self) -> list[RhetoricFinding]:
        return [f for f in self.findings if f.severity == SEVERITY_INFO]

    @property
    def total(self) -> int:
        return len(self.findings)

    def to_json(self) -> dict:
        return {
            "lint": "rhetoric_lint",
            "words": self.words,
            "rules_applied": list(self.rules_applied),
            "warnings": len(self.warnings),
            "infos": len(self.infos),
            "findings": [f.to_dict() for f in self.findings],
        }


# Scan preprocessing ------------------------------------------------------------

# Word tokens for the per-1000-words denominator. Hyphenated and
# apostrophized compounds count once ("fast-paced", "it's").
_WORD_RE = re.compile(r"[A-Za-z0-9]+(?:['’\-][A-Za-z0-9]+)*")

# Code-fence opener/closer (``` or ~~~, up to 3 leading spaces per
# CommonMark).
_FENCE_RE = re.compile(r"^ {0,3}(```|~~~)")

# Inline code span: `...` (non-greedy, single line).
_INLINE_CODE_RE = re.compile(r"`[^`\n]*`")

# Suppression directive (mirrors render_gate._MEMO_LINT_DISABLE_RE /
# memo_image_refs): comma-separated rule list inside an HTML comment.
_LINT_DISABLE_RE = re.compile(
    r"<!--\s*anvil-lint-disable:\s*(?P<rules>[a-zA-Z0-9_,\-\s]+?)\s*-->",
)

# YAML front-matter fence: a ``---`` (optionally trailing whitespace) on
# the document's very first line opens a block that runs to the next
# such line. Used by ``_first_prose_lineno`` to skip metadata before the
# first prose line.
_FRONT_MATTER_FENCE_RE = re.compile(r"^---\s*$")

# ATX heading line (``#`` … ``######`` followed by whitespace). Skipped
# when locating the first prose line — a heading is not prose.
_HEADING_RE = re.compile(r"^#{1,6}\s")


def _scannable_lines(text: str) -> list[str]:
    """Per-line scan text with exclusions blanked.

    Returns one string per source line (index ``i`` ↔ source line
    ``i + 1``) with fenced code blocks, HTML comments (including
    multi-line spans), and inline code spans removed. Line count and
    line numbering are preserved so findings stay anchored to the
    original source.
    """
    out: list[str] = []
    in_fence = False
    fence_marker = ""
    in_comment = False
    for raw in text.splitlines():
        line = raw
        # --- fenced code blocks: blank the fence lines AND the body ---
        if not in_comment:
            m = _FENCE_RE.match(line)
            if m:
                if not in_fence:
                    in_fence = True
                    fence_marker = m.group(1)
                elif m.group(1) == fence_marker:
                    in_fence = False
                out.append("")
                continue
        if in_fence:
            out.append("")
            continue
        # --- HTML comments (multi-line aware) --------------------------
        if in_comment:
            end = line.find("-->")
            if end == -1:
                out.append("")
                continue
            line = line[end + 3:]
            in_comment = False
        # Strip any complete or opening comment spans on this line.
        while True:
            start = line.find("<!--")
            if start == -1:
                break
            end = line.find("-->", start + 4)
            if end == -1:
                line = line[:start]
                in_comment = True
                break
            line = line[:start] + " " + line[end + 3:]
        # --- inline code spans -----------------------------------------
        line = _INLINE_CODE_RE.sub(" ", line)
        out.append(line)
    return out


def _first_prose_lineno(
    scan_lines: list[str], raw_text: str
) -> Optional[int]:
    """1-based line number of the document's first prose line, or ``None``.

    "First prose line" = the first non-blank :func:`_scannable_lines`
    entry after skipping, in order:

    1. a leading YAML front-matter block (``---`` on line 1 through the
       next ``---``);
    2. ATX heading lines (``#`` … ``######``);
    3. blank scan lines (already blanked by :func:`_scannable_lines` for
       fenced code, HTML comments, and inline code spans).

    ``scan_lines`` is index-aligned with ``raw_text.splitlines()`` (both
    derive one entry per source line), so front-matter and heading
    detection reads the raw line while the blank check reads the blanked
    scan line. Returns ``None`` for an empty document, a front-matter- or
    heading-only document, or one whose entire body is excluded — in
    which case ``scope: "first-line"`` rules produce no finding. Anchors
    positional (``scope: "first-line"``) rules.
    """
    raw_lines = raw_text.splitlines()
    start = 0
    # (1) Leading YAML front-matter: a bare ``---`` on the very first
    #     line opens a block that runs to the next bare ``---``. Skip the
    #     whole block (including both fences). An unterminated block means
    #     there is no prose.
    if raw_lines and _FRONT_MATTER_FENCE_RE.match(raw_lines[0]):
        for i in range(1, len(raw_lines)):
            if _FRONT_MATTER_FENCE_RE.match(raw_lines[i]):
                start = i + 1
                break
        else:
            return None
    # (2)/(3) First non-heading, non-blank scan line.
    for i in range(start, len(scan_lines)):
        raw_line = raw_lines[i] if i < len(raw_lines) else ""
        if _HEADING_RE.match(raw_line):
            continue
        if not scan_lines[i].strip():
            continue
        return i + 1
    return None


def _collect_disabled_lines(
    text: str, suppress_rules: Sequence[str]
) -> set[int]:
    """1-based line numbers suppressed via ``anvil-lint-disable``.

    Same contract as ``render_gate._collect_memo_disabled_lines``:
    same-line directives suppress that line; a standalone directive
    line suppresses the next non-blank, non-directive line.
    Comma-separated rule lists are honored; any token in
    ``suppress_rules`` activates the directive.
    """
    wanted = set(suppress_rules)
    disabled: set[int] = set()
    lines = text.splitlines()
    for i, line in enumerate(lines):
        for m in _LINT_DISABLE_RE.finditer(line):
            rules = {r.strip() for r in m.group("rules").split(",") if r.strip()}
            if not (rules & wanted):
                continue
            disabled.add(i + 1)
            tail = line[m.end():].strip()
            head = line[: m.start()].strip()
            if tail or head:
                # Inline directive — same-line suppression only.
                continue
            for j in range(i + 1, len(lines)):
                next_line = lines[j]
                if not next_line.strip():
                    continue
                if _LINT_DISABLE_RE.search(next_line):
                    continue
                disabled.add(j + 1)
                break
    return disabled


# Rule loading / validation -------------------------------------------------------


def _coerce_severity(value: object) -> str:
    """Coerce a declared severity to the advisory contract.

    ``"info"`` passes through; everything else — including the
    forbidden upgrade to ``"error"`` — coerces to ``"warning"``.
    """
    if isinstance(value, str) and value.strip().lower() in _VALID_SEVERITIES:
        return value.strip().lower()
    return SEVERITY_WARNING


def _validate_rule(rule: object) -> tuple[Optional[dict], Optional[str]]:
    """Normalize one rule dict. Returns ``(normalized, error)``.

    A valid rule yields ``(dict, None)``; an invalid one yields
    ``(None, "<reason>")`` for the caller to surface as a config
    finding (the broken-declaration posture: skipped, not silent).
    """
    if not isinstance(rule, dict):
        return (None, f"rule is not an object: {rule!r}")
    rule_id = rule.get("id")
    if not isinstance(rule_id, str) or not rule_id.strip():
        return (None, f"rule missing string 'id': {rule!r}")
    kind = rule.get("kind")
    if kind not in _VALID_KINDS:
        return (
            None,
            f"rule {rule_id!r}: invalid kind {kind!r} "
            f"(expected one of {', '.join(_VALID_KINDS)})",
        )
    pattern = rule.get("pattern")
    if not isinstance(pattern, str) or not pattern:
        return (None, f"rule {rule_id!r}: missing string 'pattern'")
    normalized: dict = {
        "id": rule_id.strip(),
        "kind": kind,
        "pattern": pattern,
        "message": rule.get("message")
        if isinstance(rule.get("message"), str)
        else f"rule {rule_id!r} matched",
        "severity": _coerce_severity(rule.get("severity")),
    }
    if kind == RULE_KIND_FREQUENCY:
        threshold = rule.get("max_per_1000_words")
        if (
            isinstance(threshold, bool)
            or not isinstance(threshold, (int, float))
            or threshold <= 0
        ):
            return (
                None,
                f"rule {rule_id!r}: frequency kind requires numeric "
                f"'max_per_1000_words' > 0 (got {threshold!r})",
            )
        normalized["max_per_1000_words"] = float(threshold)
        min_words = rule.get("min_words", DEFAULT_FREQUENCY_MIN_WORDS)
        if (
            isinstance(min_words, bool)
            or not isinstance(min_words, (int, float))
            or min_words < 0
        ):
            min_words = DEFAULT_FREQUENCY_MIN_WORDS
        normalized["min_words"] = int(min_words)
    else:
        # Positional scope (phrase/regex only; frequency is always
        # document-level and never receives a ``scope`` key). Unknown or
        # absent values coerce to ``"body"`` — the original behavior.
        scope_raw = rule.get("scope", "body")
        normalized["scope"] = "first-line" if scope_raw == "first-line" else "body"
        # Compile now so a malformed regex is a config finding, not a
        # mid-scan crash.
        try:
            normalized["_compiled"] = _compile_rule_pattern(kind, pattern)
        except re.error as exc:
            return (None, f"rule {rule_id!r}: invalid regex pattern: {exc}")
    return (normalized, None)


def _compile_rule_pattern(kind: str, pattern: str) -> re.Pattern:
    """Compile a phrase/regex rule pattern (always case-insensitive)."""
    if kind == RULE_KIND_PHRASE:
        escaped = re.escape(pattern)
        # Straight apostrophe in a phrase also matches the typographic
        # apostrophe (memo prose is usually smart-quoted).
        escaped = escaped.replace("'", "['’]")
        return re.compile(r"\b" + escaped + r"\b", re.IGNORECASE)
    return re.compile(pattern, re.IGNORECASE)


def _load_rule_source(
    source: object, *, origin: str
) -> tuple[list[dict], set[str], list[RhetoricFinding]]:
    """Normalize one consumer rule source (parsed dict or rule list).

    Returns ``(valid_rules, disable_ids, config_findings)``. A source
    may be either the documented file shape (``{"name", "rules",
    "disable"}``) or a bare list of rule dicts.
    """
    findings: list[RhetoricFinding] = []
    if isinstance(source, list):
        rules_raw: list = source
        disable_raw: list = []
    elif isinstance(source, dict):
        rules_raw = source.get("rules", [])
        disable_raw = source.get("disable", [])
        if not isinstance(rules_raw, list):
            findings.append(
                RhetoricFinding(
                    rule_id=CONFIG_RULE_ID,
                    severity=SEVERITY_WARNING,
                    message=(
                        f"{origin}: 'rules' is not a list; consumer rules "
                        "ignored (framework defaults still apply)."
                    ),
                )
            )
            rules_raw = []
        if not isinstance(disable_raw, list):
            findings.append(
                RhetoricFinding(
                    rule_id=CONFIG_RULE_ID,
                    severity=SEVERITY_WARNING,
                    message=(
                        f"{origin}: 'disable' is not a list; ignored."
                    ),
                )
            )
            disable_raw = []
    else:
        findings.append(
            RhetoricFinding(
                rule_id=CONFIG_RULE_ID,
                severity=SEVERITY_WARNING,
                message=(
                    f"{origin}: expected an object with 'rules'/'disable' "
                    f"or a list of rules, got {type(source).__name__}; "
                    "consumer rules ignored (framework defaults still apply)."
                ),
            )
        )
        return ([], set(), findings)

    valid: list[dict] = []
    for raw in rules_raw:
        normalized, error = _validate_rule(raw)
        if normalized is not None:
            valid.append(normalized)
        else:
            findings.append(
                RhetoricFinding(
                    rule_id=CONFIG_RULE_ID,
                    severity=SEVERITY_WARNING,
                    message=f"{origin}: skipped invalid rule — {error}",
                )
            )
    disable = {d.strip() for d in disable_raw if isinstance(d, str) and d.strip()}
    return (valid, disable, findings)


def _resolve_rules(
    extra_rules: Optional[object],
    extra_rules_path: Optional[Union[str, Path]],
) -> tuple[list[dict], list[RhetoricFinding]]:
    """Merge defaults + in-memory extras + consumer rule file.

    Merge order: framework defaults → ``extra_rules`` →
    ``extra_rules_path``. Later sources win on id collision; ``disable``
    ids (from any source) remove rules from the merged set. Returns
    ``(effective_rules, config_findings)``.
    """
    findings: list[RhetoricFinding] = []
    merged: dict[str, dict] = {}
    disabled_ids: set[str] = set()

    # Defaults are trusted but run through the same validator so the
    # in-module set can never drift from the documented schema.
    for raw in DEFAULT_RHETORIC_RULES:
        normalized, error = _validate_rule(raw)
        if normalized is not None:
            merged[normalized["id"]] = normalized
        else:  # pragma: no cover — defaults are validated by tests
            findings.append(
                RhetoricFinding(
                    rule_id=CONFIG_RULE_ID,
                    severity=SEVERITY_WARNING,
                    message=f"default rule set: {error}",
                )
            )

    if extra_rules is not None:
        rules, disable, src_findings = _load_rule_source(
            extra_rules, origin="extra_rules"
        )
        findings.extend(src_findings)
        for rule in rules:
            merged[rule["id"]] = rule
        disabled_ids |= disable

    if extra_rules_path is not None:
        path = Path(extra_rules_path)
        origin = str(path)
        try:
            parsed = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            # Graceful-degrade: defaults-only run + ONE warning finding
            # naming the parse error (customer_context.py posture).
            findings.append(
                RhetoricFinding(
                    rule_id=CONFIG_RULE_ID,
                    severity=SEVERITY_WARNING,
                    message=(
                        f"{origin}: could not load consumer rhetoric rules "
                        f"({exc}); framework defaults still apply."
                    ),
                )
            )
        else:
            rules, disable, src_findings = _load_rule_source(
                parsed, origin=origin
            )
            findings.extend(src_findings)
            for rule in rules:
                merged[rule["id"]] = rule
            disabled_ids |= disable

    effective = [r for rid, r in merged.items() if rid not in disabled_ids]
    return (effective, findings)


# Public API ----------------------------------------------------------------------


def lint_rhetoric(
    text: str,
    *,
    extra_rules: Optional[object] = None,
    extra_rules_path: Optional[Union[str, Path]] = None,
    suppress_rules: Sequence[str] = DEFAULT_SUPPRESS_RULES,
) -> RhetoricLintResult:
    """Run the deterministic rhetoric lint over ``text``.

    Parameters
    ----------
    text:
        Body markdown to scan. Fenced code blocks, HTML comments, and
        inline code spans are excluded (code samples must not fire;
        the suppression directive must not self-match).
    extra_rules:
        Optional in-memory consumer rules: either a bare list of rule
        dicts or the documented file shape (``{"name", "rules",
        "disable"}``). Merged over the defaults (id collision →
        consumer wins).
    extra_rules_path:
        Optional path to a consumer JSON rule file (same shape).
        Malformed input graceful-degrades to a defaults-only run with
        one warning finding naming the parse error. This is the
        integration point for the #461 voice contract's
        ``voice.rhetoric_rules`` sub-key, wired in issue #468:
        ``anvil.lib.project_brief.resolve_rhetoric_rules`` resolves
        the declared path and memo-render step 4g forwards it through
        ``render_gate.gate(kind="memo", rhetoric_rules_path=...)``
        (a missing declared file is forwarded too, so the OSError
        graceful-degrade above surfaces the broken declaration).
    suppress_rules:
        Directive tokens honored for per-line suppression. Defaults to
        :data:`DEFAULT_SUPPRESS_RULES` (the memo gate dimension name
        plus the generic ``rhetoric_lint`` token).

    Returns
    -------
    RhetoricLintResult
        ``findings`` (warning/info only — never error), ``words``
        (the per-1000-words denominator over the scanned text), and
        ``rules_applied`` (effective rule ids after merge/disable).
    """
    rules, findings = _resolve_rules(extra_rules, extra_rules_path)
    scan_lines = _scannable_lines(text)
    disabled_lines = _collect_disabled_lines(text, suppress_rules)
    words = sum(len(_WORD_RE.findall(line)) for line in scan_lines)
    # Computed once for all positional (``scope: "first-line"``) rules.
    first_prose_lineno = _first_prose_lineno(scan_lines, text)

    for rule in rules:
        if rule["kind"] == RULE_KIND_FREQUENCY:
            count = sum(line.count(rule["pattern"]) for line in scan_lines)
            if words < rule["min_words"] or words == 0:
                continue
            density = count / words * 1000.0
            if density > rule["max_per_1000_words"]:
                findings.append(
                    RhetoricFinding(
                        rule_id=rule["id"],
                        severity=rule["severity"],
                        message=(
                            f"{rule['message']} "
                            f"({count} occurrence(s) of {rule['pattern']!r} "
                            f"in {words} words = {density:.1f}/1000; "
                            f"threshold {rule['max_per_1000_words']:g}/1000)."
                        ),
                        line=None,
                        match=rule["pattern"],
                    )
                )
            continue
        regex = rule["_compiled"]
        rule_scope = rule.get("scope", "body")
        for lineno, line in enumerate(scan_lines, start=1):
            # Positional rules evaluate only the first prose line; when
            # there is none (empty / all-excluded doc), they never fire.
            if rule_scope == "first-line" and lineno != first_prose_lineno:
                continue
            for m in regex.finditer(line):
                if lineno in disabled_lines:
                    findings.append(
                        RhetoricFinding(
                            rule_id=rule["id"],
                            severity=SEVERITY_INFO,
                            message=f"{rule['message']} (suppressed)",
                            line=lineno,
                            match=m.group(0),
                        )
                    )
                else:
                    findings.append(
                        RhetoricFinding(
                            rule_id=rule["id"],
                            severity=rule["severity"],
                            message=rule["message"],
                            line=lineno,
                            match=m.group(0),
                        )
                    )

    return RhetoricLintResult(
        findings=findings,
        words=words,
        rules_applied=sorted(r["id"] for r in rules),
    )


__all__ = [
    "CONFIG_RULE_ID",
    "DEFAULT_FREQUENCY_MIN_WORDS",
    "DEFAULT_RHETORIC_RULES",
    "DEFAULT_SUPPRESS_RULES",
    "EMDASH_MAX_PER_1000_WORDS",
    "RULE_KIND_FREQUENCY",
    "RULE_KIND_PHRASE",
    "RULE_KIND_REGEX",
    "RhetoricFinding",
    "RhetoricLintResult",
    "SEVERITY_INFO",
    "SEVERITY_WARNING",
    "lint_rhetoric",
]
