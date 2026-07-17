# Inventorship Interview Packet — acme-widget

**Candidate:** Bob Builder
**Email:** bob@example.com
**Filing reference:** `acme-widget/`
**Date generated:** 2026-06-13
**Skill:** ip-uspto-inventorship (interview mode, v2)
**Sensitivity:** `counsel-eyes-only`

> **CONFIDENTIAL — ATTORNEY WORK PRODUCT.**
> This packet is prepared in anticipation of patent counsel's §115
> inventorship determination for **acme-widget**. **This is not a §115**
> **declaration.** Do not share outside the organization without
> counsel approval.

---

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

---

### Bot-author resolution — REQUIRES YOUR CONFIRMATION

One or more commits on paths you are listed against were authored
by a CI / agent bot identity. **The bot is not a natural person and
cannot be a §115 inventor.** Counsel must attribute those commits to
a human director (the person who triggered or directed the run).

The skill applied a 5-step resolution chain (triggering-issue author →
chat thread → channel-agent operator → project lead → sync-commit
author). Steps 2 and 3 require human / chat lookup and are NOT
auto-resolved here — counsel resolves them at interview time.

**Provisional attributions (please confirm or correct):**

- Commit `dddddddddd` (2026-03-10T10:00:00Z) — _wire codec glue (#42)_
  - Authored by bot: `acme-agents[bot]`
  - Claim element(s): C2
  - Provisional human director: **UNRESOLVED — counsel must follow up** (resolution step: `unresolved`)
  - Note: No automatic resolution available — counsel must attribute this bot commit to the human director who triggered the run.
  - ( ) Confirm I directed this run.  ( ) I did NOT direct this run; the director was: ____________

---

### Evidence anchors from v1 git analysis

The repo paths below are commits the v1 inventorship audit attributed to
you. **They are NOT evidence of conception** — they are commits, which
may reflect implementation, refactoring, or vendoring. **They are
memory aids only.** Only you (and counsel) can determine whether any of
these commits coincided with your conception of a claim element.

Use them to jog your memory when answering Q1 above. Add other
corroborating evidence in Q4.

---

## Element-by-element walkthrough

For each element below, please answer Q1–Q7. **If you had no role**
**in conceiving an element, write "none" for Q1 and skip Q2–Q7 for that**
**element** — this is also useful information for counsel.

### Element C1

_Element text:_ Independent claim 1 — adaptive widget controller

_Evidence anchors (memory aids only — not conception evidence):_

- `bbbbbbbbbb (2026-03-05T10:00:00Z) [implementation] tune controller threshold — path: src/controller.py`

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

### Element 1(b)(iv-v)

_Element text:_ Claim 1 limitations (iv)-(v) — threshold scheduler

_No git-history anchors attributed to you on this element's_
_mapped paths in the v1 audit. (This does NOT mean you didn't_
_conceive the element; conception can happen on a whiteboard_
_with zero git footprint.)_

**Q1 (conception moment).** Describe the moment you first recognized the
inventive concept of this element. When, where, and what triggered the
recognition? A best-recollection date estimate (e.g., "around mid-February
2026") is acceptable; we are not asking for a sworn date.

> Alice and I worked out the scheduler cadence together.

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

> Alice Author.

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

### Element C2

_Element text:_ Independent claim 2 — vendored signal codec

_No git-history anchors attributed to you on this element's_
_mapped paths in the v1 audit. (This does NOT mean you didn't_
_conceive the element; conception can happen on a whiteboard_
_with zero git footprint.)_

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

---

## Signature block

I confirm the responses above are true to the best of my recollection.
This is **not** a sworn §115 declaration.

Name (typed): _____________________________________  (Bob Builder)

Date: 2026-06-15

---

> **CONFIDENTIAL — ATTORNEY WORK PRODUCT / INVENTORSHIP INTERVIEW.**
> This document is prepared in anticipation of patent counsel's §115
> inventorship determination for **acme-widget**. It is **not** a sworn
> declaration. Do not share outside the organization without counsel
> approval. If you have left the organization, your obligation to
> cooperate truthfully with this inventorship inquiry survives termination
> per your IP-assignment agreement.
>
> **This is not a §115 declaration.** Counsel will use your responses,
> together with the evidence anchors and other contributors' packets, to
> draft the formal declaration at filing / conversion time.
