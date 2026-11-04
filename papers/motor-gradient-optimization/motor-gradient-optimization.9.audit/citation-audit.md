# Citation audit: motor-gradient-optimization.9

`refs.bib` is byte-identical to v8. All 19 cite keys in `main.tex` + `optimization.tex` resolve, the bibtex log has 0 warnings, and the PDF has no `[?]` or `??`. The only v8 → v9 change at a citation site is the §7.1 attribution sentence (v8 N9). Every other citation and its surrounding claim is unchanged from v8 and keeps its v8 verdict (see `motor-gradient-optimization.8.audit/citation-audit.md`).

No external lookups (Crossref, OpenAlex or other) were made in this audit, and no email address was sent anywhere. The only network requests were HTTP status checks on the paper's 11 `\href` targets, all 200.

| Key | Resolved | Surrounding claim (changed site only) | Verdict | Notes |
|---|---|---|---|---|
| more2009benchmarking | yes | "follows the data- and performance-profile practice for benchmarking optimization solvers" (optimization.tex:27-29) | unverified (source not on disk); metadata consistent | Moré & Wild (2009) introduced data profiles. The v8 attribution gap (N9) is closed |
| dolan2002benchmarking | yes | same | unverified (source not on disk); metadata consistent | Dolan & Moré (2002) introduced performance profiles. Section 2 wording agrees |
| kraft1988, ragonneau2022 | yes | unchanged | unverified (source not on disk) | Carried forward from v7/v8 |
| all other keys (15) | yes | unchanged | as in v8 audit | |

Unresolved keys: none. Claim-support failures: none. Unverified (source not on disk): 4.
