"""Deterministic interview-packet templating for inventorship v2 (issue #493).

Evidence-mode v2 surface for ``ip-uspto-inventorship --interview``: given
the v1-mined artifacts (``inventorship_map.json`` + ``evidence.jsonl``) and
a candidate inventor, assemble a per-inventor **interview packet** —
structured counsel-style questions about that person's contribution to a
specific filing.

Legal framing (load-bearing — do NOT weaken)
--------------------------------------------

Packets are **ATTORNEY WORK PRODUCT**. They are advisory only. They never
touch the ``●`` matrix, inventor columns, or TBD markers; the v1 ``●``
rules + "never guess attribution" + the attestation block stay
byte-identical. Every sentence in a packet body is either statutory
boilerplate (copied verbatim from the constants below) or a deterministic
projection of mined v1 rows. **The packet asks questions; it never answers
them.** Evidence anchors are labelled **memory aids only, NOT evidence of
conception**. The git bot is **never** a §115 inventor — it is not a
natural person.

This module consumes v1 outputs; it never re-mines. Without ``--interview``
the base command's behavior is unchanged.

Design contract (settled at #493 curation; do NOT re-litigate)
--------------------------------------------------------------

- **Skill-local placement**: this judgment-laden interview/synthesis
  module stays under ``ip-uspto/lib/``; do NOT promote to ``anvil/lib/``
  until a second skill consumes interview packets or ``--synthesize``
  determination parsing. (Its evidence-mining sibling
  ``inventorship_evidence.py`` WAS promoted to ``anvil/lib/`` in #516 once
  the provisional's inventorship-lite pass became its second consumer;
  this module is loaded by file path from the promoted location — see the
  ``_load_evidence_lib`` helper below.)
- **Adopt the native ``_lib/interview_packet.py`` API shape** where it
  carries over (so a future sphere migration round-trips), adapted to
  anvil's basis model (features under basis A, claim elements under basis
  B — both keyed in ``inventorship_map.json``'s ``elements`` dict) and
  anvil's flat ``evidence.jsonl`` schema (``claim_element`` is a single
  string per row, not the native ``claim_elements`` list).
- **Reuse v1's vendored helpers** (``is_vendored_path``,
  ``vendored_prefixes`` / ``vendored-primary`` role) from the promoted
  ``anvil/lib/inventorship_evidence.py`` — never reimplement.
- **Statutory prose copied verbatim** from the native module: it is legally
  derived. Sphere-specific PIIA/WSGR wording is generalized to neutral
  placeholder text (anvil ships to many consumers); the legal substance is
  preserved.
- **Bot identity is operator-configurable**, defaulting to a documented
  pattern (anvil is not Sphere-specific). The bot is never emitted as a
  candidate inventor.
- **CLI contract** per the v1 tool-evidence precedent: direct file
  invocation (the skill dir is hyphenated, so there is no dotted
  ``python -m`` path). JSON to stdout; exit ``0`` = packets written,
  ``2`` = missing v1 artifacts (the "run --evidence first" notice).
"""

from __future__ import annotations

import importlib.util
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Tuple

# ---------------------------------------------------------------------------
# v1 lib reuse — ``inventorship_evidence.py`` was promoted to ``anvil/lib/``
# (issue #516) when the provisional's inventorship-lite pass became the
# second consumer. The skill dir is hyphenated, so we load the promoted
# module by file path under a unique module name (the project-migrate
# ``_skill_lib`` precedent) rather than via a dotted ``anvil.lib`` package
# import — this file is itself loaded by file path from the hyphenated skill
# dir, so the ``anvil`` package is not guaranteed to be importable on
# ``sys.path``. We reuse ``is_vendored_path`` rather than reimplementing the
# vendored-prefix logic.
# ---------------------------------------------------------------------------

_LIB_DIR = Path(__file__).resolve().parent
# ``anvil/skills/ip-uspto/lib/`` -> repo root is four parents up; the
# promoted module lives at ``anvil/lib/inventorship_evidence.py``.
_EVIDENCE_FILE = _LIB_DIR.parents[3] / "anvil" / "lib" / "inventorship_evidence.py"
_EVIDENCE_MODULE_NAME = "_ip_uspto_inventorship_evidence_for_interview"


def _load_evidence_lib():
    if _EVIDENCE_MODULE_NAME in sys.modules:
        return sys.modules[_EVIDENCE_MODULE_NAME]
    spec = importlib.util.spec_from_file_location(
        _EVIDENCE_MODULE_NAME, _EVIDENCE_FILE
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[_EVIDENCE_MODULE_NAME] = module
    spec.loader.exec_module(module)
    return module


_ev = _load_evidence_lib()
is_vendored_path = _ev.is_vendored_path  # reused, never reimplemented


# ---------------------------------------------------------------------------
# Constants — counsel-grade question template (ported verbatim from the
# native ``_lib/interview_packet.py``; legally derived prose).
# ---------------------------------------------------------------------------

QUESTION_BLOCK = """
**Q1 (conception moment).** Describe the moment you first recognized the
inventive concept of this element. When, where, and what triggered the
recognition? A best-recollection date estimate (e.g., "around mid-February
2026") is acceptable; we are not asking for a sworn date.

> _Your answer:_

**Q2 (definiteness — Burroughs Wellcome / MPEP §2138.04).** When you first
had the idea, was it definite enough that a competent engineer could have
implemented it without further inventive thought from you?
( ) yes  ( ) partial  ( ) no  ( ) unsure — if `partial`, what was still missing?

> _Your answer:_

**Q3 (joint conception, §116).** Who else, if anyone, contributed to the
*idea* of this element (not just the code)? List by name. Under 35 USC
§116 joint inventors do not have to contribute equally and do not have to
be physically together — but each must contribute to at least one element
of at least one claim.

> _Your answer:_

**Q4 (corroboration).** What corroborating evidence — outside your own
statement — supports your claim to conception? Examples: commits authored
by you, design documents with timestamps, chat threads, meeting notes,
lab notebook entries, calendar invites, whiteboard photos, email threads.
List what you remember; the skill does not need to access them.

> _Your answer:_

**Q5 (derivation — §102(f) legacy / §102(a)(1) AIA).** Did you become
aware of this concept from any external source before you formed the
idea? Examples: a prior-art reference you read, a conversation with
someone outside the organization, a public talk, a consultant. If yes,
name the source. (Note: this question is not a trap — most concepts have
prior inspirations; counsel cares about *derivation* in the legal sense.)

> _Your answer:_

**Q6 (prior art).** Are you aware of prior art for this element that was
not cited in the filing's prior-art landscape? If yes, briefly describe.

> _Your answer:_

**Q7 (post-conception reduction-to-practice authorship).** After your
conception, who first reduced this element to practice (built / coded /
simulated / measured it)? They may or may not be a co-inventor —
reduction to practice alone is not conception.

> _Your answer:_
""".strip()


SENSITIVITY_LEVELS = (
    "counsel-eyes-only",
    "distribute-to-named-candidate-only",
    "confidential-internal",
)

#: Default sensitivity for a stored (unfilled) template — the safest level.
DEFAULT_SENSITIVITY = "counsel-eyes-only"


#: Default bot-identity match: a commit author/email matching this pattern
#: is treated as a CI/agent identity (never a §115 inventor). Operator-
#: configurable via ``BotConfig.pattern``; the default documents the common
#: ``…[bot]`` / ``…-agents[bot]`` GitHub-app author convention plus the
#: ``noreply`` CI-email shape.
DEFAULT_BOT_PATTERN = re.compile(r"(?i)(\[bot\]$|-agents\b|\bnoreply@)")


CONFIDENTIAL_FOOTER = """
---

> **CONFIDENTIAL — ATTORNEY WORK PRODUCT / INVENTORSHIP INTERVIEW.**
> This document is prepared in anticipation of patent counsel's §115
> inventorship determination for **{filing}**. It is **not** a sworn
> declaration. Do not share outside the organization without counsel
> approval. If you have left the organization, your obligation to
> cooperate truthfully with this inventorship inquiry survives termination
> per your IP-assignment agreement.
>
> **This is not a §115 declaration.** Counsel will use your responses,
> together with the evidence anchors and other contributors' packets, to
> draft the formal declaration at filing / conversion time.
""".strip()


STATUTORY_INTRO = """
## What this packet is, and what it is not

This is an **inventorship interview packet** — a structured set of
counsel-style questions about your contribution to a specific patent
filing. **It is not a §115 declaration.** Counsel will use your responses
(together with other candidates' packets and the underlying evidence) to
draft the formal §115 inventor declaration at the time the application is
filed (or converts from provisional to non-provisional / utility).

### The relevant statutes (plain English)

- **35 USC §115** — every named inventor on a US patent must have
  conceived at least one element of at least one claim. *Implementation
  alone does not qualify.* Mis-stated inventorship is a recognized
  invalidity attack at IPR and at trial.
- **35 USC §116** — *joint* inventorship. Joint inventors do not have to
  contribute equally, do not have to be physically together, and do not
  have to contribute to every claim — but each must contribute to at
  least one element of at least one claim.
- **35 USC §256** — naming the wrong inventors (over- or under-naming)
  can be corrected before issuance, but only with documentary evidence
  of who conceived what. This packet *produces* that evidence.

### Conception vs. reduction-to-practice — the most common confusion

The Federal Circuit defined conception in *Burroughs Wellcome Co. v. Barr
Labs., Inc.*, 40 F.3d 1223 (Fed. Cir. 1994), and the USPTO restates it
at MPEP §2138.04:

> **Conception** is *"the formation in the inventor's mind of a definite
> and permanent idea of the complete and operative invention, sufficient
> to enable a person of ordinary skill in the art to reduce it to
> practice without extensive research or experimentation."*

**Reduction to practice** is *building / coding / simulating / measuring
the invention*. Reduction to practice alone is **not** inventorship. If
someone gave you a clear, definite description of the idea and you wrote
the code, you may be a *contributor* but not the *inventor* of that
element. Conversely, if you conceived the complete and operative idea on
a whiteboard with no code at all, you may be an inventor with **zero**
git footprint.

This packet asks Q1–Q7 per element to disentangle the two.
""".strip()


EVIDENCE_ANCHORS_DISCLAIMER = """
### Evidence anchors from v1 git analysis

The repo paths below are commits the v1 inventorship audit attributed to
you. **They are NOT evidence of conception** — they are commits, which
may reflect implementation, refactoring, or vendoring. **They are
memory aids only.** Only you (and counsel) can determine whether any of
these commits coincided with your conception of a claim element.

Use them to jog your memory when answering Q1 above. Add other
corroborating evidence in Q4.
""".strip()


VENDORED_CODE_PROMPT = """
### Vendored-code prompt — upstream-conception question

The mechanism implementing one or more claim elements you are listed
against was imported into the repository from an external source (see
the BLOCKED rows in the per-element evidence below). The in-repo git
history is therefore not a useful conception signal for those elements —
but you may still have been involved in the upstream conception.

**Please confirm one of the following:**
- ( ) I was involved in the upstream conception. The work is documented
      at: ____________________________________________________________
      (upstream repo, design doc, prior employer, university lab notebook, …)
- ( ) I was NOT involved in the upstream conception.
- ( ) I was involved but cannot locate the documentation — counsel will
      need to interview me directly.
""".strip()


# ---------------------------------------------------------------------------
# Bot-author resolution
# ---------------------------------------------------------------------------


@dataclass
class BotConfig:
    """Operator-configurable bot-identity matcher.

    A row whose ``author`` or ``email`` matches :attr:`pattern` is a
    CI/agent identity (never a §115 inventor). :attr:`label` is the
    human-readable name surfaced in packets.
    """

    pattern: "re.Pattern[str]" = DEFAULT_BOT_PATTERN
    label: str = "CI / agent bot"

    def is_bot(self, author: str, email: str) -> bool:
        author = (author or "").strip()
        email = (email or "").strip()
        return bool(self.pattern.search(author) or self.pattern.search(email))


@dataclass
class BotResolution:
    """One resolved bot-attributed commit ready for the packet."""

    sha: str
    date: str
    subject: str
    elements: List[str]
    bot_author: str
    resolved_human: Optional[str]  # may be None if the chain failed
    resolution_step: str  # which step of the resolution chain matched
    note: str  # free-form rationale to surface to counsel


def resolve_bot_authors(
    evidence: List[dict],
    triggering_issue_authors: Optional[Dict[str, str]] = None,
    project_lead: Optional[str] = None,
    sync_commit_authors: Optional[Dict[str, str]] = None,
    bot_config: Optional[BotConfig] = None,
) -> List[BotResolution]:
    """Apply the 5-step bot-author resolution chain (native policy).

    Steps:
      1. Triggering-issue author (auto, when the caller pre-resolved a
         ``sha -> human-name`` mapping). The ONLY auto-attributed step.
      2. Triggering chat-thread author — counsel-resolved (surfaced as a
         question, NOT auto-resolved).
      3. Channel-agent operator at run time — counsel-resolved (surfaced,
         NOT auto-resolved).
      4. Project lead (last-resort fallback).
      5. Eventual sync-commit author (weakest signal).

    ``triggering_issue_authors`` / ``sync_commit_authors``: ``{sha ->
    human-name}`` (the caller pre-resolves lookups; this module never
    shells to GitHub). The bot is **never** returned as a candidate
    inventor — only the resolved (or UNRESOLVED) human is surfaced, and
    steps 2–5 are flagged for counsel rather than silently auto-attributed.
    """
    bot_config = bot_config or BotConfig()

    # Group bot rows by sha, coalescing the (singular) claim_element per row.
    bots: Dict[str, List[dict]] = {}
    for row in evidence:
        author = (row.get("author") or "").strip()
        email = (row.get("email") or "").strip()
        if bot_config.is_bot(author, email):
            bots.setdefault(row.get("sha", ""), []).append(row)

    out: List[BotResolution] = []
    for sha, rows in bots.items():
        elements = sorted(
            {r.get("claim_element") for r in rows if r.get("claim_element")}
        )
        head = rows[0]
        subject = head.get("subject", "")
        date = head.get("date", "")
        bot_author = (head.get("author") or "").strip()

        resolved_human: Optional[str] = None
        step = "unresolved"
        note = ""

        # Step 1 — triggering-issue author. The only auto-resolved step.
        if triggering_issue_authors and sha in triggering_issue_authors:
            resolved_human = triggering_issue_authors[sha]
            step = "1-triggering-issue-author"
            note = (
                "Bot commit references an issue authored by this human; "
                "verify the issue body reads as conception (specific design "
                "direction) rather than open-ended delegation."
            )
        # Step 4 — project lead, last-resort attribution (steps 2/3 are
        # counsel-resolved, not auto-resolved here).
        elif project_lead:
            resolved_human = project_lead
            step = "4-project-lead"
            note = (
                "Surfaced via project-lead fallback. Counsel must confirm "
                "the human director — channel-agent operator at run time "
                "(step 3) and the triggering chat thread (step 2) are "
                "unchecked here."
            )
        # Step 5 — sync-commit author, weakest signal.
        elif sync_commit_authors and sha in sync_commit_authors:
            resolved_human = sync_commit_authors[sha]
            step = "5-sync-commit-author"
            note = (
                "Resolved via sync-commit author only — weakest signal. "
                "Counsel should re-resolve from agent run logs."
            )
        else:
            note = (
                "No automatic resolution available — counsel must attribute "
                "this bot commit to the human director who triggered the run."
            )

        out.append(
            BotResolution(
                sha=sha,
                date=date,
                subject=subject,
                elements=elements,
                bot_author=bot_author,
                resolved_human=resolved_human,
                resolution_step=step,
                note=note,
            )
        )
    return out


def bot_resolution_block(
    bot_resolutions: List[BotResolution],
    candidate: str,
) -> str:
    """Render the bot-author resolution prompt for inclusion in a packet.

    If ``candidate`` matches a resolved human director, the block asks the
    candidate to confirm. The bot identity itself is surfaced only as the
    *source* of the commit; it is never proposed as an inventor.
    """
    if not bot_resolutions:
        return ""
    lines = [
        "### Bot-author resolution — REQUIRES YOUR CONFIRMATION",
        "",
        "One or more commits on paths you are listed against were authored",
        "by a CI / agent bot identity. **The bot is not a natural person and",
        "cannot be a §115 inventor.** Counsel must attribute those commits to",
        "a human director (the person who triggered or directed the run).",
        "",
        "The skill applied a 5-step resolution chain (triggering-issue author →",
        "chat thread → channel-agent operator → project lead → sync-commit",
        "author). Steps 2 and 3 require human / chat lookup and are NOT",
        "auto-resolved here — counsel resolves them at interview time.",
        "",
        "**Provisional attributions (please confirm or correct):**",
        "",
    ]
    for br in bot_resolutions:
        elements_str = ", ".join(br.elements) if br.elements else "(no elements)"
        resolved = br.resolved_human or "UNRESOLVED — counsel must follow up"
        match_marker = ""
        if br.resolved_human and br.resolved_human == candidate:
            match_marker = "  ← attributed to YOU (please confirm)"
        lines.append(
            f"- Commit `{br.sha[:10]}` ({br.date}) — _{br.subject[:80]}_"
        )
        lines.append(f"  - Authored by bot: `{br.bot_author}`")
        lines.append(f"  - Claim element(s): {elements_str}")
        lines.append(
            f"  - Provisional human director: **{resolved}** "
            f"(resolution step: `{br.resolution_step}`){match_marker}"
        )
        if br.note:
            lines.append(f"  - Note: {br.note}")
        lines.append(
            "  - ( ) Confirm I directed this run.  "
            "( ) I did NOT direct this run; the director was: ____________"
        )
        lines.append("")
    return "\n".join(lines).rstrip()


# ---------------------------------------------------------------------------
# Element selection — composite labels collapse to ONE Q-block
# ---------------------------------------------------------------------------


def expand_composite_label(label: str) -> List[str]:
    """Expand a composite label like ``1(b)(iv-v)`` into its constituent leaves.

    For Q1–Q7 question blocks we render ONE block at the composite label
    (the deepest level); the synthesis follow-up may want the leaves for
    row-counting. This helper returns the leaves; the packet renderer uses
    the composite label directly so each element produces exactly one block.
    """
    m = re.match(r"^(.*?)\(([ivxlcdm]+|[a-z])-([ivxlcdm]+|[a-z])\)$", label)
    if not m:
        return [label]
    base, lo, hi = m.group(1), m.group(2), m.group(3)

    def to_int(tok: str) -> int:
        roman_map = {
            "i": 1, "ii": 2, "iii": 3, "iv": 4, "v": 5, "vi": 6, "vii": 7,
            "viii": 8, "ix": 9, "x": 10,
        }
        if tok in roman_map:
            return roman_map[tok]
        if tok.isalpha() and len(tok) == 1:
            return ord(tok) - ord("a") + 1
        return -1

    def from_int(n: int, sample: str) -> str:
        roman_inv = {
            1: "i", 2: "ii", 3: "iii", 4: "iv", 5: "v", 6: "vi", 7: "vii",
            8: "viii", 9: "ix", 10: "x",
        }
        if sample in roman_inv.values():
            return roman_inv.get(n, sample)
        return chr(ord("a") + n - 1)

    lo_i, hi_i = to_int(lo), to_int(hi)
    if lo_i < 0 or hi_i < 0 or lo_i > hi_i:
        return [label]
    return [f"{base}({from_int(i, lo)})" for i in range(lo_i, hi_i + 1)]


# ---------------------------------------------------------------------------
# inventorship_map.json access (anvil schema: top-level ``elements`` dict)
# ---------------------------------------------------------------------------


def map_elements(inv_map: dict) -> List[Tuple[str, dict]]:
    """Return ``[(element_key, element_obj), ...]`` in map (insertion) order.

    Each packet asks about every element in the map (counsel asks the
    candidate about every element they MIGHT have conceived; the candidate
    self-selects in Q1). A composite element key produces exactly ONE
    Q1–Q7 block because the map keys the element at the composite label.
    """
    elements = inv_map.get("elements", {})
    if not isinstance(elements, dict):
        return []
    return list(elements.items())


def element_paths(element_obj: dict) -> List[str]:
    """The mapped repo paths for one map element (list of path strings)."""
    out: List[str] = []
    for entry in element_obj.get("paths", []):
        if isinstance(entry, dict) and entry.get("path"):
            out.append(entry["path"])
    return out


# ---------------------------------------------------------------------------
# Candidate list
# ---------------------------------------------------------------------------


def named_inventors(brief_inventors: Iterable[dict]) -> List[Tuple[str, Optional[str]]]:
    """Normalize BRIEF.md frontmatter inventors to ``[(name, email), ...]``."""
    out: List[Tuple[str, Optional[str]]] = []
    for inv in brief_inventors or []:
        if isinstance(inv, dict):
            name = (inv.get("name") or "").strip()
            email = inv.get("email")
            email = email.strip() if isinstance(email, str) else None
        else:
            name = str(inv).strip()
            email = None
        if name:
            out.append((name, email))
    return out


def candidate_list(
    brief_inventors: Iterable[dict],
    evidence: List[dict],
    bot_resolutions: Optional[List[BotResolution]] = None,
    bot_config: Optional[BotConfig] = None,
) -> List[Tuple[str, Optional[str]]]:
    """Compute the candidate-inventor union (never invents inventors).

    Union of:
      1. Named inventors from ``BRIEF.md`` frontmatter.
      2. ``(author, email)`` pairs surfaced by v1 classification as
         conception-class committers NOT already named (and NOT bots).
      3. Resolved human director(s) for bot rows.

    Deduped (case-insensitive on name), preserving discovery order.
    """
    bot_config = bot_config or BotConfig()
    seen: set = set()
    out: List[Tuple[str, Optional[str]]] = []

    def _add(name: str, email: Optional[str]) -> None:
        name = (name or "").strip()
        if not name:
            return
        key = name.lower()
        if key in seen:
            return
        seen.add(key)
        out.append((name, email.strip() if isinstance(email, str) and email.strip() else None))

    # (1) named inventors
    for name, email in named_inventors(brief_inventors):
        _add(name, email)

    # (2) conception-class committers not already named (skip bots)
    for row in evidence:
        if row.get("classification") not in ("conception", "mixed"):
            continue
        author = (row.get("author") or "").strip()
        email = (row.get("email") or "").strip()
        if not author or bot_config.is_bot(author, email):
            continue
        _add(author, email)

    # (3) resolved human directors for bot rows
    for br in bot_resolutions or []:
        if br.resolved_human:
            _add(br.resolved_human, None)

    return out


# ---------------------------------------------------------------------------
# Candidate ↔ evidence matching
# ---------------------------------------------------------------------------


def _path_matches(evidence_path: str, mapped_paths: Iterable[str]) -> bool:
    """Match an evidence row's path against the map's mapped paths.

    The v1 map sometimes uses directory prefixes while ``evidence.jsonl``
    rows are individual files. Match on equality or either-direction
    directory-prefix containment.
    """
    for mp in mapped_paths:
        if evidence_path == mp:
            return True
        mp_norm = mp.rstrip("/") + "/"
        ev_norm = evidence_path.rstrip("/") + "/"
        if evidence_path.startswith(mp_norm) or mp.startswith(ev_norm):
            return True
    return False


def candidate_matches_row(
    row: dict, candidate_name: str, candidate_email: Optional[str]
) -> bool:
    """True when an evidence row's author matches the candidate.

    Match by email (case-insensitive) OR display name (case-insensitive),
    with a first+last name fallback (people commit under multiple emails).
    A non-matching row does not leak into the packet.
    """
    author = (row.get("author") or "").strip()
    email = (row.get("email") or "").strip()
    if candidate_email and email and candidate_email.lower() == email.lower():
        return True
    if candidate_name and author and candidate_name.lower() == author.lower():
        return True
    if candidate_name and author:
        cn = candidate_name.lower().split()
        an = author.lower().split()
        if len(cn) >= 2 and len(an) >= 2 and cn[0] == an[0] and cn[-1] == an[-1]:
            return True
    return False


def evidence_anchors_for_element(
    element_key: str,
    mapped_paths: List[str],
    evidence: List[dict],
    candidate_name: str,
    candidate_email: Optional[str],
    bot_config: Optional[BotConfig] = None,
) -> List[str]:
    """Memory-aid anchor strings for this candidate on one element.

    Pulls ``evidence.jsonl`` rows where (a) the row's ``claim_element``
    matches this element, (b) the path matches a mapped path, and (c) the
    author matches the candidate. Bot rows are surfaced separately via the
    bot-resolution block, so they are excluded here.
    """
    bot_config = bot_config or BotConfig()
    out: List[str] = []
    for row in evidence:
        if row.get("claim_element") != element_key:
            continue
        if not _path_matches(row.get("path", ""), mapped_paths):
            continue
        author = (row.get("author") or "").strip()
        email = (row.get("email") or "").strip()
        if bot_config.is_bot(author, email):
            continue
        if not candidate_matches_row(row, candidate_name, candidate_email):
            continue
        sha = (row.get("sha") or "")[:10]
        subj = row.get("subject", "")
        date = row.get("date", "")
        cls = row.get("classification", "")
        out.append(f"{sha} ({date}) [{cls}] {subj} — path: {row.get('path')}")
    return out


# ---------------------------------------------------------------------------
# Vendored detection (reuses v1's is_vendored_path + role logic)
# ---------------------------------------------------------------------------


def detect_vendored_paths(inv_map: dict) -> List[str]:
    """Vendored paths in the map, by ``vendored-primary`` role or prefix.

    Reuses v1's ``is_vendored_path`` and the ``vendored_prefixes`` /
    ``vendored-primary`` semantics from ``inventorship_evidence.py`` — no
    reimplementation. Deduped, order-preserving.
    """
    prefixes = list(inv_map.get("vendored_prefixes", []) or [])
    out: List[str] = []
    seen: set = set()
    for _key, element in map_elements(inv_map):
        for entry in element.get("paths", []):
            if not isinstance(entry, dict):
                continue
            path = entry.get("path", "")
            if not path:
                continue
            role = entry.get("role", "primary")
            if role == "vendored-primary" or is_vendored_path(path, prefixes):
                if path not in seen:
                    seen.add(path)
                    out.append(path)
    return out


def candidate_vendored_paths(
    inv_map: dict,
    evidence: List[dict],
    candidate_name: str,
    candidate_email: Optional[str],
) -> List[str]:
    """Vendored paths this candidate is actually associated with.

    A candidate gets the vendored prompt only when their mapped/evidence
    paths intersect a vendored path — a candidate with no vendored
    intersection does NOT get the prompt. We treat a candidate as
    associated with a vendored path when an evidence row authored by them
    references it, OR (fallback) when the candidate has any anchor on an
    element that carries a vendored path.
    """
    vendored = set(detect_vendored_paths(inv_map))
    if not vendored:
        return []
    hit: List[str] = []
    seen: set = set()

    # Direct: an evidence row authored by the candidate references a
    # vendored path.
    for row in evidence:
        path = row.get("path", "")
        if path not in vendored:
            continue
        if candidate_matches_row(row, candidate_name, candidate_email):
            if path not in seen:
                seen.add(path)
                hit.append(path)

    # Fallback: the candidate has a (non-vendored) anchor on an element
    # that also carries a vendored path — the local history for the
    # vendored path is BLOCKED, so the prompt still applies.
    for key, element in map_elements(inv_map):
        el_paths = element_paths(element)
        el_vendored = [p for p in el_paths if p in vendored]
        if not el_vendored:
            continue
        anchors = evidence_anchors_for_element(
            key, el_paths, evidence, candidate_name, candidate_email
        )
        if anchors:
            for p in el_vendored:
                if p not in seen:
                    seen.add(p)
                    hit.append(p)
    return hit


# ---------------------------------------------------------------------------
# Packet rendering
# ---------------------------------------------------------------------------


@dataclass
class PacketContext:
    """All the inputs the renderer needs for one packet."""

    filing: str
    candidate_name: str
    candidate_email: Optional[str]
    thread: str
    generated_date: str
    sensitivity: str  # one of SENSITIVITY_LEVELS
    inv_map: dict
    evidence: List[dict]
    bot_resolutions: List[BotResolution] = field(default_factory=list)
    vendored_paths: List[str] = field(default_factory=list)
    distribution_note: Optional[str] = None
    bot_config: BotConfig = field(default_factory=BotConfig)


def slug(name: str) -> str:
    """``'Stylianos Kyriacou'`` -> ``'stylianos-kyriacou'`` (packet filename)."""
    s = (name or "").strip().lower()
    s = re.sub(r"[^a-z0-9]+", "-", s)
    return s.strip("-")


def render_packet(ctx: PacketContext) -> str:
    """Render one candidate's interview packet as Markdown.

    Layout: sensitivity header → confidential top disclaimer → statutory
    intro → bot-resolution block (when applicable) → vendored prompt (when
    applicable) → evidence-anchors disclaimer → per-element Q1–Q7 blocks
    with memory-aid anchors → signature block → confidential bottom footer.
    """
    if ctx.sensitivity not in SENSITIVITY_LEVELS:
        raise ValueError(
            f"sensitivity must be one of {SENSITIVITY_LEVELS}, "
            f"got {ctx.sensitivity!r}"
        )

    parts: List[str] = []

    # Header
    parts.append(f"# Inventorship Interview Packet — {ctx.filing}")
    parts.append("")
    parts.append(f"**Candidate:** {ctx.candidate_name}")
    if ctx.candidate_email:
        parts.append(f"**Email:** {ctx.candidate_email}")
    parts.append(f"**Filing reference:** `{ctx.thread}/`")
    parts.append(f"**Date generated:** {ctx.generated_date}")
    parts.append("**Skill:** ip-uspto-inventorship (interview mode, v2)")
    parts.append(f"**Sensitivity:** `{ctx.sensitivity}`")
    if ctx.distribution_note:
        parts.append(f"**Distribution:** {ctx.distribution_note}")
    parts.append("")

    # Confidential top disclaimer (also at bottom)
    parts.append("> **CONFIDENTIAL — ATTORNEY WORK PRODUCT.**")
    parts.append(
        "> This packet is prepared in anticipation of patent counsel's §115"
    )
    parts.append(
        f"> inventorship determination for **{ctx.filing}**. **This is not a §115**"
    )
    parts.append(
        "> **declaration.** Do not share outside the organization without"
    )
    parts.append("> counsel approval.")
    parts.append("")
    parts.append("---")
    parts.append("")

    # Statutory intro
    parts.append(STATUTORY_INTRO)
    parts.append("")
    parts.append("---")
    parts.append("")

    # Bot-author resolution (only when relevant)
    if ctx.bot_resolutions:
        parts.append(bot_resolution_block(ctx.bot_resolutions, ctx.candidate_name))
        parts.append("")
        parts.append("---")
        parts.append("")

    # Vendored-code prompt (only when relevant)
    if ctx.vendored_paths:
        parts.append(VENDORED_CODE_PROMPT)
        parts.append("")
        parts.append("**Vendored paths on this filing you are associated with:**")
        for p in ctx.vendored_paths:
            parts.append(f"- `{p}`")
        parts.append("")
        parts.append("---")
        parts.append("")

    # Evidence-anchors disclaimer
    parts.append(EVIDENCE_ANCHORS_DISCLAIMER)
    parts.append("")
    parts.append("---")
    parts.append("")

    # Per-element question blocks
    parts.append("## Element-by-element walkthrough")
    parts.append("")
    parts.append(
        'For each element below, please answer Q1–Q7. **If you had no role**'
    )
    parts.append(
        '**in conceiving an element, write "none" for Q1 and skip Q2–Q7 for that**'
    )
    parts.append("**element** — this is also useful information for counsel.")
    parts.append("")

    for element_key, element_obj in map_elements(ctx.inv_map):
        label = element_obj.get("label", "")
        mapped_paths = element_paths(element_obj)
        parts.append(f"### Element {element_key}")
        if label:
            parts.append("")
            parts.append(f"_Element text:_ {label}")
        parts.append("")

        anchors = evidence_anchors_for_element(
            element_key,
            mapped_paths,
            ctx.evidence,
            ctx.candidate_name,
            ctx.candidate_email,
            bot_config=ctx.bot_config,
        )
        if anchors:
            parts.append(
                "_Evidence anchors (memory aids only — not conception evidence):_"
            )
            parts.append("")
            for a in anchors:
                parts.append(f"- `{a}`")
            parts.append("")
        else:
            parts.append(
                "_No git-history anchors attributed to you on this element's_"
            )
            parts.append(
                "_mapped paths in the v1 audit. (This does NOT mean you didn't_"
            )
            parts.append(
                "_conceive the element; conception can happen on a whiteboard_"
            )
            parts.append("_with zero git footprint.)_")
            parts.append("")

        # The seven-question block (verbatim).
        parts.append(QUESTION_BLOCK)
        parts.append("")

    # Signature block
    parts.append("---")
    parts.append("")
    parts.append("## Signature block")
    parts.append("")
    parts.append(
        "I confirm the responses above are true to the best of my recollection."
    )
    parts.append("This is **not** a sworn §115 declaration.")
    parts.append("")
    parts.append(
        f"Name (typed): _____________________________________  ({ctx.candidate_name})"
    )
    parts.append("")
    parts.append("Date: _____________________________________")
    parts.append("")
    parts.append(CONFIDENTIAL_FOOTER.format(filing=ctx.filing))
    return "\n".join(parts) + "\n"


# ---------------------------------------------------------------------------
# Orchestration — build packets for every candidate
# ---------------------------------------------------------------------------


def load_inv_map(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def load_evidence(path: Path) -> List[dict]:
    rows: List[dict] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return rows


def build_packets(
    *,
    thread: str,
    filing: str,
    generated_date: str,
    inv_map: dict,
    evidence: List[dict],
    brief_inventors: Iterable[dict],
    sensitivity: str = DEFAULT_SENSITIVITY,
    bot_config: Optional[BotConfig] = None,
    triggering_issue_authors: Optional[Dict[str, str]] = None,
    project_lead: Optional[str] = None,
    sync_commit_authors: Optional[Dict[str, str]] = None,
) -> List[Tuple[str, str]]:
    """Build ``[(slug, packet_markdown), ...]`` for every candidate.

    Deterministic: every packet body is statutory boilerplate or a
    projection of mined rows. The ``●`` matrix is never read or written.
    Returns one packet per candidate in candidate-list order.
    """
    bot_config = bot_config or BotConfig()
    bot_resolutions = resolve_bot_authors(
        evidence,
        triggering_issue_authors=triggering_issue_authors,
        project_lead=project_lead,
        sync_commit_authors=sync_commit_authors,
        bot_config=bot_config,
    )
    candidates = candidate_list(
        brief_inventors, evidence, bot_resolutions, bot_config=bot_config
    )

    out: List[Tuple[str, str]] = []
    for name, email in candidates:
        vendored = candidate_vendored_paths(inv_map, evidence, name, email)
        ctx = PacketContext(
            filing=filing,
            candidate_name=name,
            candidate_email=email,
            thread=thread,
            generated_date=generated_date,
            sensitivity=sensitivity,
            inv_map=inv_map,
            evidence=evidence,
            bot_resolutions=bot_resolutions,
            vendored_paths=vendored,
            bot_config=bot_config,
        )
        out.append((slug(name), render_packet(ctx)))
    return out


# ===========================================================================
# Synthesis (v2 ``--synthesize``, issue #511)
# ---------------------------------------------------------------------------
# The judgment-laden half: parse *filled-in* interview packets back into
# structured per-candidate responses, then classify per-element conception
# claims into a determination FOR COUNSEL. Legal invariants (byte-identical
# to v1/v2): synthesis proposes, the attorney attests. It NEVER reads or
# writes the ``●`` matrix (``inventorship.md``), NEVER adds/removes named
# inventors, NEVER adjudicates, and NEVER infers conception in the absence
# of a candidate response (``unanswered`` / ``partial`` are surfaced, never
# resolved).
#
# Parse/judgment split (rubric-rebackport precedent, settled at #511
# curation): ``parse_packet`` extracts the *deterministic skeleton* (it is
# the unit-testable contract against the frozen #493 ``render_packet`` shape).
# Interpreting the raw free-text answers into the final
# ``CandidateResponse.answers`` is LLM-in-command. The classification +
# rollup below (``render_synthesis`` and the ``_identify_*`` / ``_summarize_*``
# / ``_suggest_*`` / ``_open_questions`` helpers) is a pure function of the
# structured ``CandidateResponse`` input — ported verbatim-adapted from the
# native ``render_synthesis`` (anvil basis: elements under
# ``inv_map["elements"]``, not native ``inv_map["claims"][].elements``).
# ===========================================================================


#: The placeholder line a respondent overwrites with their answer. An answer
#: still equal to this sentinel (after the ``> `` blockquote marker) means
#: the candidate did NOT fill it in.
ANSWER_PLACEHOLDER = "_Your answer:_"

#: The signature-block date line a respondent fills in. A trailing run of
#: underscores (the unfilled template) means no date was typed.
_SIGNATURE_DATE_RE = re.compile(r"^Date:\s*(.*)$")
_CANDIDATE_HEADER_RE = re.compile(r"^\*\*Candidate:\*\*\s*(.+?)\s*$")
_ELEMENT_HEADING_RE = re.compile(r"^###\s+Element\s+(.+?)\s*$")
#: ``**Q1 (conception moment).**`` … captures the bare ``Q1`` key.
_QUESTION_LABEL_RE = re.compile(r"^\*\*(Q[1-7])\b[^*]*\*\*")
#: A run of 5+ underscores is the unfilled signature-date template.
_BLANK_UNDERSCORES_RE = re.compile(r"^_{5,}$")


@dataclass
class ParsedPacket:
    """The deterministic skeleton lifted from one filled-in packet.

    This is the unit-testable contract against the frozen #493
    ``render_packet`` shape. It carries the *raw* free-text answer strings —
    interpreting them (``claimed-sole`` vs ``claimed-joint``? who did Q3
    name?) is the LLM-in-command half and is NOT done here.

    Fields:
      - ``candidate``: display name from the ``**Candidate:**`` header.
      - ``returned_date``: the typed signature-block date, or ``None`` when
        the date line is blank (the unfilled template). ``None`` is the
        ``unanswered`` signal downstream.
      - ``answers``: ``{element_key -> {"Q1": raw, …, "Q7": raw}}`` — the
        verbatim text the respondent wrote under each ``> _Your answer:_``
        line (empty string when left at the placeholder).
      - ``placeholder_unchanged``: ``{element_key -> {"Q1": bool, …}}`` —
        ``True`` when that answer line was NOT filled (still the template
        placeholder). A fully-unfilled template reports every flag ``True``.
    """

    candidate: str
    returned_date: Optional[str]
    answers: Dict[str, Dict[str, str]] = field(default_factory=dict)
    placeholder_unchanged: Dict[str, Dict[str, bool]] = field(
        default_factory=dict
    )


def _strip_answer_line(line: str) -> Tuple[Optional[str], bool]:
    """Parse a ``> …`` blockquote answer line.

    Returns ``(answer_text, is_placeholder)``. ``answer_text`` is the text
    after the ``> `` marker (stripped); ``is_placeholder`` is ``True`` when
    that text is still the unfilled ``_Your answer:_`` sentinel (or empty).
    Returns ``(None, False)`` for a line that is not a blockquote.
    """
    if not line.startswith(">"):
        return None, False
    body = line[1:].strip()
    if body == ANSWER_PLACEHOLDER or body == "":
        return "", True
    # A respondent who typed after the sentinel (e.g.
    # ``> _Your answer:_ around mid-Feb``) — strip the leading sentinel.
    if body.startswith(ANSWER_PLACEHOLDER):
        remainder = body[len(ANSWER_PLACEHOLDER):].strip()
        return remainder, remainder == ""
    return body, False


def parse_packet(markdown: str) -> ParsedPacket:
    """Deterministically lift the skeleton from a filled-in packet.

    Walks a packet emitted by the frozen #493 ``render_packet`` shape and
    extracts: (a) the candidate display name from the ``**Candidate:**``
    header, (b) the returned date from the signature block (``None`` when
    blank/unfilled), (c) per-``### Element <key>`` raw Q1–Q7 answer strings,
    and (d) a per-answer ``placeholder_unchanged`` flag (the
    ``> _Your answer:_`` line was not filled).

    Round-trips a ``render_packet`` output without churn: an unfilled
    template parses to every ``placeholder_unchanged=True`` and
    ``returned_date=None``. This is a *deterministic* extraction; it never
    interprets the answers (that is the command's LLM-in-command half).
    """
    candidate = ""
    returned_date: Optional[str] = None
    answers: Dict[str, Dict[str, str]] = {}
    placeholder_unchanged: Dict[str, Dict[str, bool]] = {}

    lines = markdown.splitlines()

    current_element: Optional[str] = None
    current_q: Optional[str] = None
    in_signature = False

    for raw_line in lines:
        line = raw_line.rstrip()
        stripped = line.strip()

        # Candidate header (take the first occurrence).
        if not candidate:
            m = _CANDIDATE_HEADER_RE.match(stripped)
            if m:
                candidate = m.group(1).strip()
                continue

        # Signature block opens the date-capture window.
        if stripped.startswith("## Signature block"):
            in_signature = True
            current_element = None
            current_q = None
            continue

        # Element heading.
        m = _ELEMENT_HEADING_RE.match(stripped)
        if m and not in_signature:
            current_element = m.group(1).strip()
            current_q = None
            answers.setdefault(current_element, {})
            placeholder_unchanged.setdefault(current_element, {})
            continue

        # Question label inside an element.
        if current_element is not None and not in_signature:
            qm = _QUESTION_LABEL_RE.match(stripped)
            if qm:
                current_q = qm.group(1)
                continue

            # The blockquote answer line for the active question.
            if current_q is not None and stripped.startswith(">"):
                ans, is_placeholder = _strip_answer_line(stripped)
                if ans is not None:
                    answers[current_element][current_q] = ans
                    placeholder_unchanged[current_element][current_q] = (
                        is_placeholder
                    )
                    # Each Q-block has exactly one answer line; close it.
                    current_q = None
                continue

        # Signature-block date capture.
        if in_signature:
            dm = _SIGNATURE_DATE_RE.match(stripped)
            if dm:
                value = dm.group(1).strip()
                if value and not _BLANK_UNDERSCORES_RE.match(value):
                    returned_date = value
                continue

    return ParsedPacket(
        candidate=candidate,
        returned_date=returned_date,
        answers=answers,
        placeholder_unchanged=placeholder_unchanged,
    )


@dataclass
class CandidateResponse:
    """One candidate's filled-in packet, interpreted into structured form.

    Native shape (so a future sphere migration round-trips):
      - ``candidate``: display name.
      - ``returned_date``: typed signature date, or ``None`` == unanswered.
      - ``answers``: ``{element_key -> {"Q1": "...", "Q3": "...", …}}`` —
        the *interpreted* answers (the command normalizes raw ``parse_packet``
        text into this; an element the candidate skipped is simply absent).
      - ``notes``: free-form counsel notes (e.g. bot-director confirmation).
    """

    candidate: str
    returned_date: Optional[str]
    answers: Dict[str, Dict[str, str]] = field(default_factory=dict)
    notes: str = ""


def _q1_indicates_no_claim(q1: str) -> bool:
    """Q1 is a 'no, I didn't conceive this' answer."""
    q1n = (q1 or "").strip().lower()
    if not q1n:
        return True
    if q1n in ("none", "no", "n/a", "no one", "not me"):
        return True
    if q1n.startswith(
        ("none ", "none—", "none–", "none -", "none.", "no ", "n/a ", "not me ")
    ):
        return True
    if q1n.startswith("none"):
        return True
    return False


def _q3_named_others(q3: str, candidate: str) -> List[str]:
    """Named joint conceivers from Q3 (excluding the candidate + sentinels).

    Filters obvious "no one" sentinels and generic non-names ("the team",
    "everyone"); keeps capitalized person-name-shaped tokens, trimmed to the
    first three words.
    """
    if not q3:
        return []
    q3n = q3.strip().lower()
    if q3n in ("", "none", "n/a", "no one", "no", "nobody"):
        return []
    if q3n.startswith(("none", "n/a", "no one", "nobody")):
        return []
    pieces = [p.strip() for p in re.split(r"[,;]| and ", q3) if p.strip()]
    out: List[str] = []
    for p in pieces:
        if not p:
            continue
        if p.lower() == candidate.lower():
            continue
        if p.lower().startswith(("none", "n/a", "no one", "nobody")):
            continue
        if p.lower() in (
            "team",
            "everyone",
            "the team",
            "various",
            "the broader team",
            "the whole team",
        ):
            continue
        words = p.split()
        if words and not (words[0][0].isupper() and words[0][0].isalpha()):
            continue
        out.append(" ".join(words[:3]).rstrip(".,;:"))
    return out


def _summarize_response_for_element(r: CandidateResponse, label: str) -> str:
    """One §1 candidacy-table cell for a candidate × element."""
    if not r.returned_date:
        return "`unanswered`"
    ans = r.answers.get(label)
    if not ans:
        # Returned the packet but skipped this element's Q-block.
        return "`partial`"
    q1 = (ans.get("Q1") or "").strip()
    q3 = (ans.get("Q3") or "").strip()
    if _q1_indicates_no_claim(q1):
        return "`claimed-none`"
    others = _q3_named_others(q3, r.candidate)
    if others:
        return f"`claimed-joint` (w/ {', '.join(others)})"
    return "`claimed-sole`"


def _identify_disputed_elements(
    element_labels: List[str], responses: List[CandidateResponse]
) -> Dict[str, dict]:
    """``{label -> {status, per_candidate, resolution?}}`` for disputes.

    Classifies each element into ``CONFLICTING`` (≥2 sole claimants),
    ``MIXED`` (sole + joint to reconcile), or ``NAMED NON-RESPONDENT`` (a
    joint claimant names a conceiver who returned no packet). v2 never
    resolves these — counsel does.
    """
    disputes: Dict[str, dict] = {}
    for label in element_labels:
        sole_claimants: List[Tuple[str, str, str]] = []
        joint_claimants: List[Tuple[str, str, str, List[str]]] = []
        for r in responses:
            if not r.returned_date:
                continue
            ans = r.answers.get(label)
            if not ans:
                continue
            q1 = (ans.get("Q1") or "").strip()
            q3 = (ans.get("Q3") or "").strip()
            if _q1_indicates_no_claim(q1):
                continue
            others = _q3_named_others(q3, r.candidate)
            if others:
                joint_claimants.append((r.candidate, q1, q3, others))
                continue
            sole_claimants.append((r.candidate, q1, q3))

        if len(sole_claimants) >= 2:
            per_cand: Dict[str, dict] = {}
            for cand, q1, q3 in sole_claimants:
                per_cand[cand] = {
                    "returned_date": next(
                        (r.returned_date for r in responses if r.candidate == cand),
                        None,
                    ),
                    "summary": f'claims sole conception. Q1: "{q1[:200]}"'
                    + (f" Joint candidates named (Q3): {q3}" if q3 else ""),
                }
            disputes[label] = {
                "status": (
                    "**CONFLICTING** — two or more candidates claim sole "
                    "conception"
                ),
                "per_candidate": per_cand,
                "resolution": (
                    "Counsel must interview and resolve. Both candidates' "
                    "filed evidence is in scope."
                ),
            }
        elif sole_claimants and joint_claimants:
            per_cand = {}
            for cand, q1, q3 in sole_claimants:
                per_cand[cand] = {
                    "returned_date": next(
                        (r.returned_date for r in responses if r.candidate == cand),
                        None,
                    ),
                    "summary": (
                        f'claims sole conception. Q1: "{q1[:160]}"'
                        + (f"  Q3: {q3}" if q3 else "")
                    ),
                }
            for cand, q1, q3, others in joint_claimants:
                per_cand[cand] = {
                    "returned_date": next(
                        (r.returned_date for r in responses if r.candidate == cand),
                        None,
                    ),
                    "summary": (
                        f"claims joint conception with {', '.join(others)}. "
                        f'Q1: "{q1[:160]}"'
                    ),
                }
            sole_names = {c for c, _, _ in sole_claimants}
            mentions_sole = False
            for _, _, _, others in joint_claimants:
                for sn in sole_names:
                    if any(
                        sn.lower() in o.lower() or o.lower() in sn.lower()
                        for o in others
                    ):
                        mentions_sole = True
                        break
            resolution = (
                "PROBABLE-JOINT — sole and joint claims reconcile if read as "
                "originator + extender. Counsel to confirm."
                if mentions_sole
                else "Counsel to interview both candidates."
            )
            disputes[label] = {
                "status": "**MIXED** — sole + joint claims to reconcile",
                "per_candidate": per_cand,
                "resolution": resolution,
            }
        else:
            respondent_names = {r.candidate for r in responses if r.returned_date}
            named_non_respondent: List[Tuple[str, str, str, str]] = []
            for cand, q1, q3, others in joint_claimants:
                for o in others:
                    if not any(
                        o.lower() == rn.lower() or rn.lower() in o.lower()
                        for rn in respondent_names
                    ):
                        named_non_respondent.append((cand, o, q1, q3))
            if named_non_respondent:
                per_cand = {}
                for cand, missing, q1, q3 in named_non_respondent:
                    per_cand[cand] = {
                        "returned_date": next(
                            (
                                r.returned_date
                                for r in responses
                                if r.candidate == cand
                            ),
                            None,
                        ),
                        "summary": (
                            f"claims joint conception, names **{missing}** "
                            f'(no packet returned). Q1: "{q1[:160]}"'
                        ),
                    }
                disputes[label] = {
                    "status": (
                        "**NAMED NON-RESPONDENT** — joint conceiver did not "
                        "return a packet"
                    ),
                    "per_candidate": per_cand,
                    "resolution": (
                        "Counsel to follow up with the named non-respondent."
                    ),
                }
    return disputes


def _identify_convergent_elements(
    element_labels: List[str], responses: List[CandidateResponse]
) -> Dict[str, dict]:
    """``{label -> {inventors: [...], type: 'sole' | 'joint'}}``.

    An element is convergent when exactly one candidate claims sole
    conception (and no other claims it), OR multiple candidates claim joint
    conception with a consistent naming set.
    """
    convergent: Dict[str, dict] = {}
    for label in element_labels:
        claimants: List[Tuple[str, List[str]]] = []
        for r in responses:
            if not r.returned_date:
                continue
            ans = r.answers.get(label)
            if not ans:
                continue
            q1 = (ans.get("Q1") or "").strip()
            q3 = (ans.get("Q3") or "").strip()
            if _q1_indicates_no_claim(q1):
                continue
            others = _q3_named_others(q3, r.candidate)
            claimants.append((r.candidate, others))

        if len(claimants) == 1:
            cand, others = claimants[0]
            if not others:
                convergent[label] = {"inventors": [cand], "type": "sole"}
        elif len(claimants) >= 2:
            sets: List[set] = []
            for cand, others in claimants:
                norm_others: set = set()
                for o in others:
                    matched = False
                    for r in responses:
                        if r.candidate == cand:
                            continue
                        if (
                            o.lower() == r.candidate.lower()
                            or o.lower() in r.candidate.lower()
                        ):
                            norm_others.add(r.candidate)
                            matched = True
                            break
                    if not matched:
                        norm_others.add(o)
                sets.append({cand} | norm_others)
            if all(s == sets[0] for s in sets):
                convergent[label] = {
                    "inventors": sorted(sets[0]),
                    "type": "joint",
                }
    return convergent


def _suggest_inventors(
    element_labels: List[str],
    responses: List[CandidateResponse],
    convergent: Dict[str, dict],
    disputes: Dict[str, dict],
) -> Dict[str, dict]:
    """Roll the convergent map up into a per-candidate inventor list."""
    out: Dict[str, dict] = {}
    for label, info in convergent.items():
        for inv in info["inventors"]:
            slot = out.setdefault(inv, {"elements": [], "framing": ""})
            slot["elements"].append(label)
            slot["framing"] = (
                "§116 joint inventor (multi-element coverage)"
                if len(slot["elements"]) > 1 or info["type"] == "joint"
                else "§115 sole-element inventor"
            )
    return out


def _open_questions(
    element_labels: List[str],
    responses: List[CandidateResponse],
    inv_map: dict,
) -> List[str]:
    """Counsel-follow-up items: non-respondents and unclaimed elements."""
    out: List[str] = []
    for r in responses:
        if not r.returned_date:
            out.append(
                f"Candidate **{r.candidate}** did not return a packet — "
                "re-issue and follow up before non-provisional drafting."
            )
    for label in element_labels:
        any_claim = False
        for r in responses:
            if not r.returned_date:
                continue
            ans = r.answers.get(label)
            if not ans:
                continue
            q1 = (ans.get("Q1") or "").strip()
            if not _q1_indicates_no_claim(q1):
                any_claim = True
                break
        if not any_claim:
            out.append(
                f"Element `{label}` is **unclaimed** — no responding "
                "candidate claims conception. Counsel to follow up with "
                "potential out-of-git conceivers (whiteboard sessions, "
                "design docs)."
            )
    return out


def _map_element_labels(inv_map: dict) -> List[str]:
    """Element keys in declaration (map insertion) order — anvil basis.

    Native keys elements under ``inv_map["claims"][].elements`` by ``label``;
    anvil keys them at the top level under ``inv_map["elements"]``. The §1
    table, §2 disputes, §3 convergence all key on the element *key* (e.g.
    ``C1`` / ``1(b)(iv-v)``), which is also what ``parse_packet`` lifts from
    the ``### Element <key>`` headings — so the two halves line up.
    """
    return [k for k, _ in map_elements(inv_map)]


def render_synthesis(
    filing: str,
    thread: str,
    generated_date: str,
    inv_map: dict,
    responses: List[CandidateResponse],
    bot_resolutions: Optional[List[BotResolution]] = None,
) -> str:
    """Render ``synthesis.md`` — a determination FOR COUNSEL.

    Seven sections: (1) candidacy table, (2) disputed elements
    (CONFLICTING / MIXED / NAMED NON-RESPONDENT), (3) convergent inventor
    list, (4) suggested inventor list (advisory-only), (5) open questions
    for counsel, (6) bot-author resolution status, (7) partial-response
    handling. Defaults to ``counsel-eyes-only`` (it aggregates every
    candidate's packet). NEVER auto-fills the ``●`` matrix, never
    adjudicates, never infers conception from a non-response.
    """
    bot_resolutions = bot_resolutions or []
    element_labels = _map_element_labels(inv_map)

    parts: List[str] = []
    parts.append(f"# Inventorship Synthesis — {filing}")
    parts.append("")
    parts.append(f"**Filing reference:** `{thread}/`")
    parts.append(f"**Date generated:** {generated_date}")
    parts.append("**Skill:** ip-uspto-inventorship (synthesis mode, v2)")
    parts.append("**Sensitivity:** `counsel-eyes-only` (aggregated packets)")
    parts.append("")
    parts.append("> **CONFIDENTIAL — ATTORNEY WORK PRODUCT.** This synthesis")
    parts.append("> aggregates filled-in interview packets from candidate")
    parts.append("> inventors. **This is not a §115 declaration** and not a")
    parts.append("> §115 determination. Counsel uses it to draft the formal")
    parts.append("> inventor declaration at filing / non-provisional")
    parts.append("> conversion. The synthesis **never** writes the `●`")
    parts.append("> inventorship matrix, never adds or removes named")
    parts.append("> inventors, and never infers conception from an")
    parts.append("> unanswered or partial response.")
    parts.append("")
    parts.append("---")
    parts.append("")

    # 1. Candidacy summary table
    parts.append("## 1. Inventor candidacy summary")
    parts.append("")
    parts.append("Per-element rows × candidate columns. Cell values:")
    parts.append("")
    parts.append("- `claimed-sole` — candidate claims sole conception")
    parts.append(
        "- `claimed-joint` — candidate claims joint conception (named others)"
    )
    parts.append('- `claimed-none` — candidate explicitly answered "none"')
    parts.append(
        "- `unanswered` — candidate did not return a packet for this element"
    )
    parts.append(
        "- `partial` — candidate returned the packet but skipped Q-blocks "
        "for this element"
    )
    parts.append("")
    candidates = [r.candidate for r in responses]
    if candidates:
        header = "| Element | " + " | ".join(candidates) + " |"
        sep = "|---|" + "|".join(["---"] * len(candidates)) + "|"
        parts.append(header)
        parts.append(sep)
        for label in element_labels:
            row = [f"`{label}`"]
            for r in responses:
                row.append(_summarize_response_for_element(r, label))
            parts.append("| " + " | ".join(row) + " |")
    else:
        parts.append("_No candidate packets to summarize._")
    parts.append("")
    parts.append("---")
    parts.append("")

    # 2. Disputed elements
    parts.append("## 2. Disputed elements")
    parts.append("")
    disputes = _identify_disputed_elements(element_labels, responses)
    if not disputes:
        parts.append(
            "_No disputed elements: every candidate's claim is internally "
            "consistent_"
        )
        parts.append("_with every other candidate's._")
    else:
        for label, claims_summary in disputes.items():
            parts.append(f"### Disputed: Element `{label}`")
            parts.append("")
            parts.append(f"**Status:** {claims_summary['status']}")
            parts.append("")
            for c, summary in claims_summary["per_candidate"].items():
                parts.append(
                    f"- **{c}** "
                    f"({summary['returned_date'] or 'no return date'}): "
                    f"{summary['summary']}"
                )
            if claims_summary.get("resolution"):
                parts.append("")
                parts.append(
                    f"**Resolution status:** {claims_summary['resolution']}"
                )
            parts.append("")
    parts.append("---")
    parts.append("")

    # 3. Convergent inventor list
    parts.append("## 3. Convergent inventor list")
    parts.append("")
    convergent = _identify_convergent_elements(element_labels, responses)
    if not convergent:
        parts.append("_No fully-converged elements._")
    else:
        parts.append("| Element | Convergent inventor(s) | Sole / Joint |")
        parts.append("|---|---|---|")
        for label, info in convergent.items():
            parts.append(
                f"| `{label}` | {', '.join(info['inventors'])} | "
                f"{info['type']} |"
            )
    parts.append("")
    parts.append("---")
    parts.append("")

    # 4. Suggested inventor list (advisory-only)
    parts.append("## 4. Suggested inventor list (for counsel review)")
    parts.append("")
    parts.append(
        "> **Advisory only.** This is the candidate inventor list the "
        "responses"
    )
    parts.append("> *support*. Counsel makes the final §115 determination.")
    parts.append("")
    suggested = _suggest_inventors(
        element_labels, responses, convergent, disputes
    )
    if not suggested:
        parts.append(
            "_No suggested inventors — every element is `unanswered`, "
            "`partial`, or disputed._"
        )
    else:
        parts.append("| Candidate | Supporting element(s) | §116 framing |")
        parts.append("|---|---|---|")
        for cand, info in suggested.items():
            els = ", ".join(f"`{e}`" for e in info["elements"])
            parts.append(f"| {cand} | {els} | {info['framing']} |")
    parts.append("")
    parts.append("---")
    parts.append("")

    # 5. Open questions for counsel
    parts.append("## 5. Open questions for counsel follow-up")
    parts.append("")
    opens = _open_questions(element_labels, responses, inv_map)
    if not opens:
        parts.append("_No open questions._")
    else:
        for q in opens:
            parts.append(f"- {q}")
    parts.append("")
    parts.append("---")
    parts.append("")

    # 6. Bot-author resolution status
    parts.append("## 6. Bot-author resolution status")
    parts.append("")
    if not bot_resolutions:
        parts.append(
            "_No CI/agent-bot-attributed commits in this filing's evidence._"
        )
    else:
        parts.append(
            "| Bot SHA | Element(s) | Provisional human director | "
            "Resolution step | Confirmed? |"
        )
        parts.append("|---|---|---|---|---|")
        for br in bot_resolutions:
            els = (
                ", ".join(f"`{e}`" for e in br.elements)
                if br.elements
                else "(none)"
            )
            human = br.resolved_human or "_UNRESOLVED_"
            confirmed = "?"
            for r in responses:
                if br.resolved_human and r.candidate == br.resolved_human:
                    notes = (r.notes or "").lower()
                    if "confirmed-bot-director" in notes:
                        confirmed = "yes"
                    elif "declined-bot-director" in notes:
                        confirmed = "no"
                    elif r.returned_date:
                        confirmed = "returned packet (see notes)"
                    else:
                        confirmed = "unanswered"
            parts.append(
                f"| `{br.sha[:10]}` | {els} | {human} | "
                f"`{br.resolution_step}` | {confirmed} |"
            )
    parts.append("")
    parts.append("---")
    parts.append("")

    # 7. Partial-response handling
    parts.append("## 7. Partial-response handling")
    parts.append("")
    unanswered = [r.candidate for r in responses if not r.returned_date]
    partial_elements: List[Tuple[str, str]] = []
    for r in responses:
        if not r.returned_date:
            continue
        for label in element_labels:
            if label not in r.answers:
                partial_elements.append((r.candidate, label))
    if unanswered:
        parts.append("**Candidates with no returned packet:**")
        for c in unanswered:
            parts.append(
                f"- {c} — flagged `unanswered` in §1 table. Counsel to "
                "follow up."
            )
        parts.append("")
    if partial_elements:
        parts.append("**Returned packets with skipped (partial) elements:**")
        for cand, label in partial_elements:
            parts.append(
                f"- {cand} skipped element `{label}` — flagged `partial` in "
                "§1 table. Counsel to follow up."
            )
        parts.append("")
    if not unanswered and not partial_elements:
        parts.append("_All candidates returned complete packets._")
        parts.append("")
    parts.append(
        "**v2 does NOT infer conception in the absence of a candidate "
        "response.** Every `unanswered` / `partial` element is surfaced "
        "for counsel and is never resolved to a conceiver here."
    )
    parts.append("")
    parts.append("---")
    parts.append("")
    parts.append(
        "> **Reminder:** this synthesis is advisory. Counsel makes the "
        "final"
    )
    parts.append(
        "> 35 USC §115 determination at filing / non-provisional conversion "
        "time."
    )
    return "\n".join(parts) + "\n"


# ---------------------------------------------------------------------------
# Synthesis orchestration (parse packets → responses → synthesis.md)
# ---------------------------------------------------------------------------


def response_from_parsed(parsed: ParsedPacket) -> CandidateResponse:
    """Project a deterministic :class:`ParsedPacket` to a :class:`CandidateResponse`.

    This is the **mechanical default** projection used by the CLI / end-to-end
    path. It drops elements whose Q-block was left entirely at the template
    placeholder (so a returned-but-skipped element is correctly absent →
    ``partial`` downstream) and keeps the raw answer text otherwise. The
    *interpretive* normalization of free-text answers (Q1 "around mid-Feb on
    the whiteboard with Bob" → structured) stays LLM-in-command per the #511
    parse/judgment split — the command can construct a richer
    ``CandidateResponse`` and call ``render_synthesis`` directly. v2 never
    infers conception here: an unfilled element simply does not appear.
    """
    answers: Dict[str, Dict[str, str]] = {}
    for element_key, qmap in parsed.answers.items():
        flags = parsed.placeholder_unchanged.get(element_key, {})
        kept: Dict[str, str] = {}
        for q, raw in qmap.items():
            if flags.get(q, False):
                continue  # placeholder unchanged → not an answer
            if (raw or "").strip() == "":
                continue
            kept[q] = raw
        if kept:
            answers[element_key] = kept
    return CandidateResponse(
        candidate=parsed.candidate,
        returned_date=parsed.returned_date,
        answers=answers,
    )


def load_packets(interviews_dir: Path) -> List[Tuple[str, str]]:
    """Return ``[(slug, markdown), ...]`` for every ``{slug}.md`` packet.

    Sorted by slug for deterministic ordering. Missing / empty dir → ``[]``.
    """
    if not interviews_dir.is_dir():
        return []
    out: List[Tuple[str, str]] = []
    for path in sorted(interviews_dir.glob("*.md")):
        out.append((path.stem, path.read_text(encoding="utf-8")))
    return out


def build_synthesis(
    *,
    thread: str,
    filing: str,
    generated_date: str,
    inv_map: dict,
    evidence: List[dict],
    interviews_dir: Path,
    bot_config: Optional[BotConfig] = None,
    triggering_issue_authors: Optional[Dict[str, str]] = None,
    project_lead: Optional[str] = None,
    sync_commit_authors: Optional[Dict[str, str]] = None,
) -> Tuple[str, List[str]]:
    """Parse packets in ``interviews_dir`` and render ``synthesis.md``.

    Returns ``(synthesis_markdown, parsed_candidate_slugs)``. Uses the
    mechanical :func:`response_from_parsed` projection; a command driving a
    richer LLM interpretation calls :func:`render_synthesis` directly. The
    ``●`` matrix (``inventorship.md``) is never read or written.
    """
    bot_config = bot_config or BotConfig()
    bot_resolutions = resolve_bot_authors(
        evidence,
        triggering_issue_authors=triggering_issue_authors,
        project_lead=project_lead,
        sync_commit_authors=sync_commit_authors,
        bot_config=bot_config,
    )
    responses: List[CandidateResponse] = []
    slugs: List[str] = []
    for packet_slug, body in load_packets(interviews_dir):
        parsed = parse_packet(body)
        responses.append(response_from_parsed(parsed))
        slugs.append(packet_slug)
    synthesis = render_synthesis(
        filing=filing,
        thread=thread,
        generated_date=generated_date,
        inv_map=inv_map,
        responses=responses,
        bot_resolutions=bot_resolutions,
    )
    return synthesis, slugs


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _utc_today() -> str:
    from datetime import datetime, timezone

    return datetime.now(timezone.utc).date().isoformat()


def main(argv: Optional[List[str]] = None) -> int:
    """CLI entry point. JSON to stdout; exit 0 packets written / 2 missing v1.

    ``--interview`` consumes v1 artifacts only; if
    ``inventorship_map.json`` / ``evidence.jsonl`` are absent it emits the
    "run --evidence first" notice and exits cleanly without writing packets.
    """
    import argparse

    parser = argparse.ArgumentParser(
        prog="inventorship_interview.py",
        description=(
            "Deterministic per-inventor interview-packet generation for the "
            "ip-uspto-inventorship --interview (v2) mode. Consumes the v1 "
            "inventorship_map.json + evidence.jsonl; never re-mines, never "
            "adjudicates, never touches the ● matrix."
        ),
    )
    parser.add_argument(
        "map_path", type=Path, help="Path to v1 inventorship_map.json."
    )
    parser.add_argument(
        "evidence_path", type=Path, help="Path to v1 evidence.jsonl."
    )
    parser.add_argument(
        "--thread", default="thread", help="Thread slug (filing reference)."
    )
    parser.add_argument(
        "--filing",
        default=None,
        help="Filing label for packet headers (default: --thread).",
    )
    parser.add_argument(
        "--inventor",
        action="append",
        default=[],
        metavar="NAME[:EMAIL]",
        help=(
            "A named inventor from BRIEF.md frontmatter (repeatable). "
            "Optional :email suffix."
        ),
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=None,
        help=(
            "Write packets to this directory (one {slug}.md per candidate). "
            "Omit to report only (no files written)."
        ),
    )
    parser.add_argument(
        "--sensitivity",
        default=DEFAULT_SENSITIVITY,
        choices=SENSITIVITY_LEVELS,
        help=f"Stored-template sensitivity (default {DEFAULT_SENSITIVITY}).",
    )
    parser.add_argument(
        "--synthesize",
        action="store_true",
        help=(
            "Synthesis (v2) mode: parse filled-in interview packets from "
            "--interviews-dir into a determination FOR COUNSEL written to "
            "--synthesis-out. Never reads/writes the ● matrix; never "
            "adjudicates; never infers conception from a non-response."
        ),
    )
    parser.add_argument(
        "--interviews-dir",
        type=Path,
        default=None,
        help=(
            "Synthesis input: directory of filled {slug}.md interview "
            "packets (default: <out-dir> when given)."
        ),
    )
    parser.add_argument(
        "--synthesis-out",
        type=Path,
        default=None,
        help="Synthesis output path (writes synthesis.md when in --synthesize).",
    )
    args = parser.parse_args(argv)

    # Graceful degradation: missing v1 artifacts → notice, clean exit 2,
    # no packets written.
    if not args.map_path.is_file() or not args.evidence_path.is_file():
        missing = []
        if not args.map_path.is_file():
            missing.append(str(args.map_path))
        if not args.evidence_path.is_file():
            missing.append(str(args.evidence_path))
        print(
            json.dumps(
                {
                    "status": "no-v1-artifacts",
                    "missing": missing,
                    "notice": (
                        "--interview / --synthesize consume v1 evidence "
                        "artifacts; run ip-uspto-inventorship <thread> "
                        "--evidence first to mine inventorship_map.json + "
                        "evidence.jsonl. Nothing written; the matrix is "
                        "untouched."
                    ),
                    "packets_written": 0,
                },
                indent=2,
            )
        )
        return 2

    inv_map = load_inv_map(args.map_path)
    evidence = load_evidence(args.evidence_path)

    # ----------------------------------------------------------------- #
    # Synthesis (v2 --synthesize): parse filled packets → synthesis.md   #
    # ----------------------------------------------------------------- #
    if args.synthesize:
        interviews_dir = args.interviews_dir or args.out_dir
        if interviews_dir is None or not interviews_dir.is_dir() or not any(
            interviews_dir.glob("*.md")
        ):
            print(
                json.dumps(
                    {
                        "status": "no-packets",
                        "interviews_dir": (
                            str(interviews_dir) if interviews_dir else None
                        ),
                        "notice": (
                            "--synthesize needs filled interview packets; run "
                            "ip-uspto-inventorship <thread> --interview first "
                            "to generate interviews/{slug}.md packets. No "
                            "synthesis written; the matrix is untouched."
                        ),
                        "synthesis_written": False,
                    },
                    indent=2,
                )
            )
            return 2

        synthesis, slugs = build_synthesis(
            thread=args.thread,
            filing=args.filing or args.thread,
            generated_date=_utc_today(),
            inv_map=inv_map,
            evidence=evidence,
            interviews_dir=interviews_dir,
        )
        synthesis_out = (
            args.synthesis_out
            or (interviews_dir.parent / "synthesis.md")
        )
        synthesis_out.parent.mkdir(parents=True, exist_ok=True)
        synthesis_out.write_text(synthesis, encoding="utf-8")
        print(
            json.dumps(
                {
                    "status": "ok",
                    "mode": "synthesize",
                    "candidates": slugs,
                    "synthesis_written": True,
                    "synthesis_path": str(synthesis_out),
                },
                indent=2,
            )
        )
        return 0

    brief_inventors: List[dict] = []
    for spec in args.inventor:
        if ":" in spec:
            name, email = spec.split(":", 1)
            brief_inventors.append({"name": name.strip(), "email": email.strip()})
        else:
            brief_inventors.append({"name": spec.strip()})

    packets = build_packets(
        thread=args.thread,
        filing=args.filing or args.thread,
        generated_date=_utc_today(),
        inv_map=inv_map,
        evidence=evidence,
        brief_inventors=brief_inventors,
        sensitivity=args.sensitivity,
    )

    written: List[str] = []
    if args.out_dir is not None:
        args.out_dir.mkdir(parents=True, exist_ok=True)
        for packet_slug, body in packets:
            dest = args.out_dir / f"{packet_slug}.md"
            dest.write_text(body, encoding="utf-8")
            written.append(str(dest))

    print(
        json.dumps(
            {
                "status": "ok",
                "candidates": [s for s, _ in packets],
                "packets_written": len(written),
                "paths": written,
                "out_dir": str(args.out_dir) if args.out_dir else None,
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())


__all__ = [
    "QUESTION_BLOCK",
    "STATUTORY_INTRO",
    "CONFIDENTIAL_FOOTER",
    "VENDORED_CODE_PROMPT",
    "EVIDENCE_ANCHORS_DISCLAIMER",
    "SENSITIVITY_LEVELS",
    "DEFAULT_SENSITIVITY",
    "DEFAULT_BOT_PATTERN",
    "BotConfig",
    "BotResolution",
    "PacketContext",
    "is_vendored_path",
    "resolve_bot_authors",
    "bot_resolution_block",
    "expand_composite_label",
    "map_elements",
    "element_paths",
    "named_inventors",
    "candidate_list",
    "candidate_matches_row",
    "evidence_anchors_for_element",
    "detect_vendored_paths",
    "candidate_vendored_paths",
    "render_packet",
    "slug",
    "load_inv_map",
    "load_evidence",
    "build_packets",
    # Synthesis (v2 --synthesize, #511)
    "ANSWER_PLACEHOLDER",
    "ParsedPacket",
    "parse_packet",
    "CandidateResponse",
    "render_synthesis",
    "response_from_parsed",
    "load_packets",
    "build_synthesis",
    "main",
]
