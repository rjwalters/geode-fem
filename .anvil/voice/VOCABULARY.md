<!--
  VOCABULARY.template.md — anvil voice-grounding starter template

  This is a STARTING POINT, not finished content. It ships the proven
  "reminder tool, not injection tool" PHILOSOPHY and the judgment-side
  tests that generalize across authors. It does NOT ship any one author's
  domain vocabularies — those are left as a marked `<!-- replace me -->`
  placeholder for you to fill with the word categories that are natural to
  YOUR background.

  This is the judgment-side guidance doc. Deterministic vocabulary
  screening (word counting, em-dash frequency, banned-phrase matching) is
  the rhetoric lint's job, not this doc's. An optional `vocab` reminder
  tool may be wired alongside it — see the note in "The Tool" below — but
  this doc does not depend on one.

  Declare it in a project BRIEF.md `voice:` block:

      voice:
        vocabulary: .anvil/voice/VOCABULARY.md

  See anvil/templates/voice/README.md for the four-doc taxonomy and the
  full wiring instructions.
-->

# Vocabulary Expansion Guide

AI-generated text tends toward "newspaper English" — safe, predictable word choices that signal machine authorship. This guide helps break that pattern.

> **This is a template.** The philosophy and tests below generalize. The
> word categories in the last section are a placeholder — replace them
> with the vocabularies natural to your own background.

## The Tool

This doc is guidance, not automation. Anvil ships a small **reminder**
tool that surfaces a random sample of words you know but might not reach
for while reviewing a draft. That tool is **optional and additive** —
this guidance stands on its own without it.

```bash
python -m anvil.lib.vocab_reminder        # surface ~20 random reminder words
python -m anvil.lib.vocab_reminder 30     # surface 30 for more options
```

It draws from a sibling `*.words.txt` next to this doc when one exists
(e.g. `VOCABULARY.words.txt` beside `VOCABULARY.md` — drop your own
larger list there), else from a small anvil-shipped default.

**It is a reminder, not an injector.** It surfaces candidates; it never
edits your draft. Apply a word ONLY when it clicks with a concept you are
already expressing — precision over novelty, 0–2 per 1000, and revert if
a simpler word loses nothing. The judgment-side tests below are what
matter; the tool only nudges you to remember a word you already know.

<!--
  replace me (optional): if you point the tool at your own word list,
  note its path here, e.g. "reminder words live in VOCABULARY.words.txt".
  Do NOT make the rest of this doc depend on the tool. The judgment-side
  tests below are what matter.
-->

## The Key Insight

This is a **reminder tool**, not an injection tool.

Human writers don't randomly sprinkle fancy words — they reach for specific vocabulary when it's the precise fit for a concept. A specialist uses a technical term not to sound smart, but because it captures something a plain phrase doesn't.

A vocabulary list reminds you of words you *know* but might not reach for. If a word doesn't feel like "oh, that's exactly what I mean," skip it.

## The Process

1. Review the draft with a vocabulary list (or your own memory) at hand.
2. Look for words that click with concepts you're already trying to express.
3. For each candidate, ask: does this word carry meaning the current word doesn't?
4. Apply sparingly — 0-2 words per 1000, and only when they earn it.

## The Real Test

**Precision over novelty.** The word must add meaning, not just variety. If you can swap it back to a simpler word without losing anything, revert it.

**Respect what's working.** Good prose doesn't need enhancement. When alliteration or parallel structure is already doing work, don't break what's effective.

**Read it aloud.** Does it sound like something you'd say when explaining the idea to a smart colleague? If it sounds like you're performing, revert it.

## Gloss Pattern

One effective way to use vocabulary: **name a concept, then explain it**.

> <!-- replace me: a sentence that names a precise term, then glosses it in plain language for the reader -->

A precise term names an idea exactly; a short gloss right after unpacks it. Readers familiar with the term get a signal; readers unfamiliar get an explanation.

**Use sparingly.** Count your glosses — more than 1-2 per piece starts to feel like a lecture. The pattern works because it's rare.

## Examples

**Good enhancement (standalone):**
> <!-- replace me: a plain sentence in your domain -->

becomes:

> <!-- replace me: the same sentence with ONE precise word that earns its place; note what meaning it adds -->

The replaced word should capture something the plain word can't — a structural, load-bearing, or technically exact sense. If it doesn't, it hasn't earned its place.

**Good enhancement (gloss):**
> <!-- replace me: a sentence stating an idea in plain language -->

becomes:

> <!-- replace me: the same idea named with a precise term and immediately glossed -->

The term names what the sentence describes; the gloss ensures clarity.

**Bad enhancement:**
> <!-- replace me: a concrete, unpretentious sentence that already works -->

becomes:

> <!-- replace me: the same sentence with a "fancy" word swapped in that breaks the rhythm or wordplay -->

The fancier word tries too hard and breaks what was working. Revert it.

## Red Flags

Your vocabulary enhancement is probably failing if:

- You're adding more than 2 words per 1000.
- The word could be replaced with a simpler one without losing meaning.
- You chose the word because it's unusual, not because it's precise.
- You broke existing wordplay, rhythm, or parallel structure.
- A reader would notice the word as "fancy."
- The surrounding prose is already tight and direct.

## Word Categories

A reminder list draws from the vocabularies natural to a specific writer.
These are *yours to define* — list the domains and registers you actually
work in, so the reminder pulls words you genuinely know rather than words
that would read as costume.

<!--
  replace me: list your own domain vocabularies here. Replace the example
  scaffold below with the categories natural to YOUR background, e.g.:

  - **General vocabulary** — uncommon but natural English words
  - **<your field>** — <a few terms you'd actually reach for>
  - **<another field>** — <a few terms you'd actually reach for>

  Keep each list to words you would use because they're the precise fit,
  not words that would signal "I am trying to sound clever."
-->

## Remember

The goal isn't to sound sophisticated. It's to sound like a human with a specific background and vocabulary — not a language model hedging toward the mean.

Sometimes that means zero enhancements. If the prose is already doing its job, leave it alone.
