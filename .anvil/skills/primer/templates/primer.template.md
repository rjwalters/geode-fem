<!--
primer body template — the slug-echo body filename (`<slug>.md`, per #295)
lives at `<thread>.{N}/<slug>.md`. This is a teaching text: intuition first,
dependency-ordered, cross-referencing the spec rather than duplicating it.
Delete these comments before drafting. Replace bracketed placeholders.
-->

# [Subject] from the Basics

*A teaching companion to [the formal spec / whitepaper]. For the formal
treatment of any mechanism below, this primer points you to the relevant
section of the spec rather than restating it — the goal here is intuition.*

## Who this is for

[One short paragraph pitching the reader: the stated non-specialist audience.
Name the background assumed and the background NOT assumed. State which standard
primitives are cited out to external literature rather than taught here.]

## The problem, before any machinery

[Motivate the subject in plain language. What is the reader trying to do? What
goes wrong with the naive approach? Every later mechanism should feel like an
answer to a problem posed here — this section seeds the dependency order.]

## [First primitive] — the intuition

[Intuition BEFORE formalism (dim 2). Why does this mechanism exist? Why this
design choice? A load-bearing, correct analogy if one fits. Only after the
"why" is clear, name the notation — and immediately cross-reference the spec
for the formal definition:]

> For the formal definition, see §[X] of the spec.

[Ground it with a small concrete example (dim 3) the reader can trace by hand.]

## [Second primitive] — building on the first

[Dependency order (dim 1, the dominant dimension): this section may assume ONLY
what earlier sections taught. If it needs a concept not yet introduced, either
teach that concept first or reorder. Same intuition-first, cross-reference-not-
duplicate discipline.]

## [Novel-to-this-subject piece]

[The pieces the reader can't find in external literature get the most ink. Teach
them fully from intuition; cross-reference the spec's formal treatment.]

## Putting it together — the end-to-end walkthrough

[The capstone worked example (dim 3). Trace one complete instance of the subject
end to end, using only mechanisms taught above. This is where the reader sees
the whole thing move. A rendered diagram (authored as mermaid under `refs/`,
rendered to `exhibits/` by `primer-figures`) usually earns its place here.]

## Where to go next

[Point the reader at the spec for the formal treatment, and at any external
literature for the cited-out standard primitives. A short synthesis pass (dim 7):
what did the reader just learn, and how do the pieces fit?]
