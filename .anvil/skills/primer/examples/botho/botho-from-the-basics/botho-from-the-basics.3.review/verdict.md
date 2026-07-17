# Review verdict — botho-from-the-basics.3

**Verdict: ADVANCE (review side).** Total **44/44** (threshold ≥35). Review critical flags: **none**. The final READY/AUDITED determination combines this verdict with the parallel `primer-audit` sibling at revise time.

Scored on v3's own merits. Per the operator's scope note, the v2→v3 delta was verified mechanically: a `diff` of the two bodies confirms exactly **seven hunks** — five figure references with captions (pandoc implicit-figure paragraphs pointing at `exhibits/fig1..fig5.png`) and the two acronym glosses (ASIC in §8, TEE in §11) that were this critic line's own v2 dim-6 recommendation. Everything else is byte-identical to the AUDITED v2 body, so for unchanged material this review leans on `botho-from-the-basics.2.review/` (43/44, zero flags) rather than re-deriving; each such reliance is cited in `scoring.md`. The seven insertions themselves were reviewed fresh, including a visual pass over all five PNGs.

## Critical flags: none

**Duplicates formal spec section — did not fire.** The `spec_ref` tier is ACTIVE (`../sections/*.tex` resolves to the 18 whitepaper LaTeX sections; `resolve_spec_ref` returns `missing: false`). Because the only new content is the five captions, the duplication sweep focused there: every caption is a single teaching sentence restating numbers and claims already present in the audited v2 prose — no formula, no derivation, no normative table is reproduced. Spot-checks against the resolved spec confirm the captions restate rather than duplicate *and* do not drift: the 1,088-byte ML-KEM ciphertext (04-cryptography.tex:91, 05-transactions.tex:83), the ~3.3 KB minting signature (3,309 bytes, 05-transactions.tex:98), the 20% burn / 80% pool split (07-monetary.tex:270–276), the 4 winners per block (07-monetary.tex:274, :501), and the emission-slice ramp to 50% (07-monetary.tex:282, :502) all match. The unchanged body's clean sweep carries over from the v2 review.

## Insertion-integrity check (the operator's "did the insertions break anything" ask)

All seven hunks pass:

- **Caption ↔ diagram**: each caption accurately describes what its PNG shows (verified visually against all five exhibits). No caption claims an element the diagram lacks, and no diagram element contradicts its caption.
- **Caption ↔ prose/spec**: no caption contradicts the surrounding prose or the resolved spec (spot-checks above). The one compression worth noting — Figure 1's "derives a fresh one-time output key from the recipient's two published keys via a one-way handshake" folds the handshake-against-view-key / combine-with-spend-key split into one clause — is lossy-but-true, and the diagram itself renders the split correctly (handshake box fed by the view key; spend key feeding the output key directly). Noted as a nit, not a defect.
- **Placement ↔ dependency order**: every figure lands after its concepts are taught (G1 after the full classical derive/scan/match/spend paragraph; G3 at the end of the "mining weight ≠ consensus weight" subsection it depicts; G4 after all of §9.5's machinery and before §9.6's "why this shape" argument; G5 as an announced map-before-the-walk at the top of §10). The two deliberate preview moments — Figure 2 naming ML-KEM-768/ML-DSA-65 one subsection before their teach-downs, and Figure 3's diagram saying "RandomX"/"no ASIC edge" a few lines before the RandomX subsection — are role-glossed in the caption and unpacked immediately below; bridgeable previews, not blocking forward references (comments 1–2).
- **No duplicated-spec-section risk in captions**: captions carry zero new quantitative material (the changelog's claim, confirmed by the diff) — they cite primer-internal sections ("Section 6"), not spec sections, so they cannot even mis-point.

## Scores

1. Pedagogical scaffolding / learnability — **7/7**
2. Intuition before formalism — **6/6**
3. Worked-example / walkthrough concreteness — **5/5**
4. Technical accuracy (judgment side) — **5/5**
5. Spec cross-reference discipline — **5/5**
6. Audience calibration — **4/4**
7. Structure & navigation — **4/4**
8. Prose clarity — **4/4**
9. Rhetorical economy — **4/4**

Dim 6 recovers its point: both v2 residuals are cured verbatim at point of use ("chips custom-built for a single algorithm" — §8; "trusted-execution-environment (TEE, secure-enclave) approaches" — §11), and no new unglossed jargon rides in with the captions.

## Top revision priorities

None required — the artifact is advance-clean at the rubric ceiling on the review side. Nothing in `comments.md` rises above nit; all five comments are scope-preserve. If the operator elects any future pass, the only candidates are the two caption-compression nits (comments 3–4), and both are explicitly fine as-is.

## What's working (do not sand off in any future revision)

Everything on the v1 and v2 lists — problems-first spine, recap-before-use, §9.6 design graveyard, honest caveats, teach-then-point blockquotes, eleven-step capstone, the three v2 quantitative insertions, the fee-leakage honesty — all verified byte-identical in v3. Plus the five figures, which are now load-bearing teaching devices in their own right:

- **Figure 5 as a map-before-the-walk** ("The whole payment at a glance" — §10): the reader sees build/propagate/finalize/aftermath before the eleven steps and can reuse it as the recap. Keep it at the top of §10.
- **Figure 3's wall metaphor made visual** ("a candidate block crosses the wall, voting power never does" — §8): the single sharpest rendering of the issuance/finality decoupling in the whole document.
- **Captions as one-sentence re-teaches**, not references — each caption re-states the section's core claim in teaching voice with zero new numbers. Resist any future urge to make them terse figure labels or to load them with parameters.
