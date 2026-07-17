---
project: ax101-family
documents:
  - slug: ax101-objdet
    artifact_type: datasheet
  - slug: ax101-ocr
    artifact_type: datasheet
---

# AX101 edge-AI inference family

The **AX101** is a single base die packaged into two preliminary SKUs that share
one fabrication, one die, one QFN48 package, one absolute-maximum table, and one
DC characteristics block — and differ only in the on-chip network they ship
configured for and the performance that network delivers:

- **AX101-OD** — object-detection SKU (`ax101-objdet` thread). Single-shot
  detector, 320×320 input, MIPI CSI-2 camera front end.
- **AX101-OCR** — text-recognition SKU (`ax101-ocr` thread). CRNN recognizer,
  variable-width line input, same MIPI front end.

Sibling SKUs of one part family share this project root so the auditor can
cross-read their sheets: rubric dimension 5 (family / SKU coherence) and
`datasheet-audit` step 9 (shared-die cross-read) compare the **shared** spec
blocks (process, die, package, abs-max, DC) byte-for-byte across the two threads
and check that the **per-SKU** blocks (network, performance) are clearly
differentiated. The `family: AX101` value in each thread's BRIEF is what binds
them into one coherence set.

Both sheets are `status: preliminary`: every pre-silicon value carries a
provenance label (`\simval{}` for system-model simulation, `\est{}` for
estimates pending characterization) and both carry the standing preliminary
notice. The numeric claims are synthesized, NON-CONFIDENTIAL illustrative
content — not a real part — vendored to ground the skill's worked-example
contract.

This is a vendored worked example. Only the `ax101-objdet` thread ships a
realized version directory and critic siblings in-tree; `ax101-ocr` is declared
in this `documents:` list so the family-coherence dimension and the sibling
cross-read audit step are active against the object-detection sheet (see
`../expected-thread.1/README.md`).
