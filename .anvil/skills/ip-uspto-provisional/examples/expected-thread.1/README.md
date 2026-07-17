# Expected thread.1 — illustrative snapshot

This directory is an **illustrative reference**, NOT a strict golden file. The
worked example is a synthesized, NON-CONFIDENTIAL provisional application
(`acme-widget-prov` — a passively thermally-compensated piezoresistive pressure
sensor). It exists to ground the `anvil:ip-uspto-provisional` skill's lifecycle
in a realized thread, and to make the *class-distinguishing* shape concrete: this
artifact is **claims-optional** and **enablement-depth-first**.

Strict golden files make refactors painful for a generative skill where prose,
ranges, and scores reasonably vary across runs and model versions. This README
documents the **structural contract** the vendored companion satisfies — it is
illustrative, not text-equality.

## What this directory shows

If you run `ip-uspto-provisional-draft acme-widget-prov` against the inventor
brief at `acme-widget-prov/acme-widget-prov/BRIEF.md`, then the `s112` critic,
you should see something structurally like:

```
acme-widget-prov/
  BRIEF.md                  Project brief — frontmatter declares
                            artifact_type: ip-uspto-provisional; parses under
                            load_project_brief_strict.

  acme-widget-prov/
    BRIEF.md                Thread-level inventor brief (same shape as
                            ip-uspto-intake output): §1 problem, §2 prior
                            approaches, §3 inventive features (the disclosure
                            denominator the s112 critic scores against), §4
                            embodiments, §5 ranges & alternatives.

  acme-widget-prov.1/
    spec.tex                XeLaTeX specification; \documentclass{anvil-uspto}.
                            The FIVE required sections (no abstract): FIELD,
                            BACKGROUND, SUMMARY, BRIEF DESCRIPTION OF THE
                            DRAWINGS, DETAILED DESCRIPTION OF EMBODIMENTS. The
                            detailed description is the enablement-depth surface
                            the s112 critic scores at weight 8 — concrete
                            embodiments, working ranges with preferred values,
                            named alternatives, \refnum{N} numerals,
                            \anvilpara{...} paragraph numbering.
    claims.tex              OPTIONAL claim-seed (1 independent + 2 dependents).
                            Present here only to exercise dim 9 *Conversion
                            readiness*. Every seed limitation traces to enabling
                            disclosure in spec.tex. ABSENCE OF claims.tex IS
                            NEVER A FINDING — the provisional is claims-optional.
    anvil-uspto.cls         Copied verbatim from anvil/skills/ip-uspto/assets/
                            so the version dir compiles standalone with
                            `xelatex spec.tex`. (The provisional reuses the
                            ip-uspto class — install the two skills together.)
    drawings/
      drawing-descriptions.md   Stub descriptions for FIG. 1–3 (no rendered
                                figures vendored).
    figures/.gitkeep        Created (empty in this stub-default snapshot).
    _progress.json          { phases.draft.state: "done",
                              metadata.iteration: 1 }
```

After running `ip-uspto-provisional-112 acme-widget-prov` (the load-bearing
§112(a) enablement-depth critic):

```
  acme-widget-prov.1.s112/        <-- NOTE: .s112/, NOT .review/
    _summary.md       machine-summary scorecard: full 9-row table with dims
                      1, 2, 3, 9 SCORED and the rest null ("n/a — see <owning
                      critic>"); the rubric block
                      { "id": "anvil-ip-provisional-v1", "total": 45,
                        "advance_threshold": 39, "dimensions": 9 };
                      critical-flag section; top-3 revision priorities.
    findings.md       Per-feature findings (enablement, coverage, possession,
                      conversion readiness), each with severity / location /
                      rationale / suggested fix / question-to-inventors.
    _meta.json        { scorecard_kind: "machine-summary",
                        rubric_id: "anvil-ip-provisional-v1",
                        rubric_total: 45, advance_threshold: 39, ... }
    _progress.json    { for_version: 1, phases.s112.state: "done" }
```

## The class-distinguishing delta — machine-summary, NOT human-verdict

The single most important thing this example demonstrates versus the
`anvil:proposal` / `anvil:ip-uspto`-reviewer worked examples:

- The proposal/reviewer worked examples ship a **`human-verdict`** sidecar:
  `verdict.md` + `scoring.md` + `comments.md`.
- The natural critic for this enablement-dominant artifact is the **`s112`**
  critic, which is a **`machine-summary`** critic. Its sidecar is named
  `<slug>.1.s112/` (the critic's own discovery marker, NOT `<slug>.1.review/`)
  and it writes `_summary.md` + `findings.md` + `_meta.json` + `_progress.json`.
  The `_summary.md` is a *partial* scorecard — `s112` owns dims 1, 2, 3, 9 and
  leaves the rest `null` for the reviser to aggregate across siblings.

`_meta.json` carries `scorecard_kind: "machine-summary"` plus the per-review
version stamps `rubric_id: "anvil-ip-provisional-v1"` / `rubric_total: 45` /
`advance_threshold: 39`. The rubric is the **/45** ip rubric (NOT the /44
artifact-class rubric), enablement-depth-dominant: dim 1 *§112(a) enablement
depth* carries weight 8, and dim 9 *Conversion readiness* replaces ip-uspto's
*Claim-spec correspondence*.

## Claims-optional / enablement-depth-first framing

This example deliberately INCLUDES a `claims.tex` claim-seed to demonstrate the
conversion-readiness path. But the posture is **claims-optional**:

- The absence of `claims.tex` is **never a finding, never a deduction, never a
  critical flag** (`rubric.md`, `SKILL.md`). A drafter may omit the seed
  entirely.
- When present, the seed is read as *positive evidence* for dim 9: seeds whose
  limitations all trace to enabling disclosure raise the reachable ceiling. A
  seed limitation with NO disclosure is a dim 1–3 finding (the seed surfaced a
  disclosure gap), not a dim-9 win.
- A provisional has **no abstract** and **no 37 CFR 1.77(b) formal regime** —
  the spec ships the five sections above and nothing more.

## Why not a full text snapshot

- The drafter's prose, ranges, and scores will vary across runs and models.
- LaTeX whitespace and macro use will vary.
- The `_summary.md` scores are illustrative (owned-dim subtotal 22/25 here),
  not a fixed target.

## Smoke test assertions (structural)

The shipped test `tests/test_ip_uspto_provisional_example_brief_parses.py`
asserts the load-bearing structural properties on the vendored companion:

```python
from anvil.lib.project_brief import ArtifactType, load_project_brief_strict

brief = load_project_brief_strict(EXAMPLE_DIR)          # acme-widget-prov/
doc = next(d for d in brief.documents if d.slug == "acme-widget-prov")
assert doc.artifact_type == ArtifactType.IP_USPTO_PROVISIONAL

tex = (EXAMPLE_DIR / "acme-widget-prov.1" / "spec.tex").read_text()
assert "\\documentclass{anvil-uspto}" in tex
assert (EXAMPLE_DIR / "acme-widget-prov.1" / "anvil-uspto.cls").exists()
```

The version dir compiles standalone (`xelatex spec.tex`) because
`anvil-uspto.cls` is copied alongside.
