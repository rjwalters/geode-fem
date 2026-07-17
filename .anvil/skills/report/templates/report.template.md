---
title: "{{REPORT_TITLE}}"
recipient: "{{RECIPIENT}}"
engagement_id: "{{ENGAGEMENT_ID}}"
version: {{VERSION}}
date: {{DATE}}
confidentiality: {{CONFIDENTIALITY_CLASS}}
---

{{COVER}}

# Executive summary

{{EXEC_SUMMARY}}

# Scope and method

Describe what was assessed, what was excluded, how the assessment was conducted, time window, data sources, sample size. State the engagement scope from `_project.md` explicitly. A report that does not bound its scope cannot be defended later.

## What is in scope

- (List)

## What is not in scope

- (List)

## Method

- (Narrative — interviews conducted, documents reviewed, measurements taken, tools used)

# Findings

Numbered findings, each with: heading, narrative, evidence citation, severity (if applicable), cross-reference to recommendations.

## Finding 1: <short title>

**Evidence**: [citation]

(Body)

## Finding 2: <short title>

**Evidence**: [citation]

(Body)

# Recommendations

Numbered recommendations, each with: owner, scope, "what done looks like", cross-reference to the findings they address. A recommendation without an owner is a wish, not a recommendation.

## Recommendation 1: <imperative action>

- **Addresses**: Finding 1 (and 3)
- **Owner**: <role/team>
- **Scope**: <specific systems / processes / docs>
- **Done when**: <concrete criterion the recipient can verify>

## Recommendation 2: <imperative action>

- **Addresses**: Finding 2
- **Owner**: <role/team>
- **Scope**: <specific>
- **Done when**: <concrete criterion>

# Risks and limitations

State scope boundaries, sample limits, assumptions, and known limitations of the assessment. A report that omits its limits is a report that overclaims.

- **Sample limits**: (e.g., "interviewed 6 of ~40 engineers; findings reflect their experience and may not generalize")
- **Data limits**: (e.g., "performance data covers Q1 2026 only; seasonal variation not assessed")
- **Time limits**: (e.g., "assessment conducted 2026-03 through 2026-04; conditions may have changed since")
- **Methodological limits**: (e.g., "no production access; findings based on documentation and interviews")

# Appendices

(Optional supplementary material — detailed measurements, full interview notes index, glossary, etc.)

# Evidence index

Bibliography / citation list. Every quantitative claim in this report references one of these entries. Every entry is traceable to a primary source.

| # | Citation | Source type | Location | Used by |
|---|----------|-------------|----------|---------|
| 1 | (e.g., "Acme perf dashboard, 2026-Q1 export") | dataset | refs/perf-2026-q1.csv | Findings 1, 3; Recommendation 1 |
| 2 | (e.g., "Interview, J. Doe, 2026-03-15") | interview | refs/interviews/jdoe-2026-03-15.md | Finding 2 |
| 3 | (e.g., "ISO/IEC 27001:2022 §A.5.1") | standard | (external) | Recommendation 2 |
