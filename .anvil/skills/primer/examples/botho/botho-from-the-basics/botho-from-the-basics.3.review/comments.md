# Comments — botho-from-the-basics.3

Line references are to `botho-from-the-basics.3/botho-from-the-basics.md`. All five comments are scope-preserve: the seven insertions did their job and nothing rises above nit. Severity legend: blocker / major / minor / nit; scope legend: preserve / expand / reduce.

## 1. [nit / preserve] Figure 2 caption previews the lattice-scheme names one subsection early (line 387)

The caption names ML-KEM-768 and ML-DSA-65 before the "ML-KEM-768 replaces the handshake" (line 389) and "Minting signatures: ML-DSA-65" (line 421) subsections teach them, and uses "lattice-based" a few lines before the prose introduces it (line 392). This is a deliberate map-before-walk (the changelog's G2 rationale: the boxes the figure shows are "then unpacked by the three subsections that follow it"), and the caption glosses each name with its already-taught role — "(ML-KEM-768 handshake)", "(ML-DSA-65 signatures)" — so a newcomer is oriented, not blocked. Keep the placement: the figure IS the secrecy-lifetime split, which is fully taught by line 385. No change requested.

## 2. [nit / preserve] Figure 3's diagram says "RandomX" and "no ASIC edge" a few lines before the RandomX subsection (lines 607–613)

The caption opens "RandomX proof-of-work meters out new coins and the right to propose blocks" and the diagram's issuance lane is labeled "ISSUANCE and PROPOSAL — RandomX proof-of-work" with a "no ASIC edge, linear rewards" box — all a few lines ahead of the "RandomX: keeping the mining lottery egalitarian" subsection (line 609) where the name and the new ASIC gloss (lines 612–613) land. The load-bearing concept (PoW meters issuance and proposal, not votes) is fully taught above the figure; "RandomX" is a proper noun defined six lines below. Bridgeable preview, correctly homed at the wall subsection it depicts. No change requested.

## 3. [nit / preserve] Figure 1 caption compresses the two-key derivation into one clause (line 211)

"The sender derives a fresh one-time output key from the recipient's two published keys via a one-way handshake" could be misread as "the handshake runs against both keys"; the prose (lines 198–201) and the diagram itself are precise — handshake against the view key, shared secret then combined with the public spend key. Lossy-but-true compression in a caption whose own diagram disambiguates two inches below; flagged here so the auditor can adjudicate if it disagrees, but this reviewer reads it as a dim-4-clean simplification. No change requested.

## 4. [nit / preserve] Figure 4's caption is the densest single sentence the revision adds (line 834)

One sentence carries intake pricing, the 20/80 split, the emission slice, and the tilted four-UTXO draw. It parses on first read *because* it restates §§9.3–9.5 in their teaching order and sits beside the diagram that renders the same circuit — but it is at the ceiling. If any future pass touches it, split at the semicolon rather than densifying further; do not add parameters (the caption's zero-new-numbers property is what keeps the dim-5 sweep trivially clean).

## 5. [nit / preserve] Captions render as figures only under pandoc's implicit-figure rule (all five insertions)

Each image reference is a paragraph containing only the image, so the alt text becomes the rendered caption (the changelog's stated convention). This is correct and the five insertions all honor it (blank lines on both sides, verified). Preserve the convention: if a future edit ever puts text in the same paragraph as an image reference, the caption silently degrades to inline alt text in the PDF. A concern for the render phase to gate, not a body defect.

## Scope distribution

preserve: 5, expand: 0, reduce: 0
