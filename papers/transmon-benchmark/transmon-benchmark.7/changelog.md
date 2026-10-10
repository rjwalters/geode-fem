# transmon-benchmark.6 → transmon-benchmark.7

Prepared for #1039. This is an explicitly requested post-v6 evidence
revision, not an automatic attempt to improve a passing score. The thread
cap is raised from 6 to 7 for this pass, pending operator confirmation
(requested on PR #1040).
No operator critic or independent review is fabricated. v6 and every v6
critic remain byte-identical.

| Input | Change or disposition |
|---|---|
| #594 and #1034 curation | Added harmonic motion, FD validation, safe budget, stalled endpoint and conditional endpoint-slope shortfall. Distinguished 3.54× initial-slope estimate from the conditional extrapolation of at least about 6.26× (6.256959). |
| #964 / #927 / #763 | Replaced the old CPU table, chart, contribution bullet, discussion and conclusion claims. Report all three mode-recovering routes, extra modes, reference tuning, pinning, measured core-seconds, repeat counts, and output/estimator caveats. No winner chosen. |
| Later tuned-Palace evidence | Corrected Palace's timed default solver identity to SuperLU; qualified tuning by its same-box control. Removed the unsupported direct-versus-AMS interpretation. |
| #1002 / unresolved #1003 | Added single-factorization port-subspace result and retained spurious-mode limitation. Kept loaded-Mac CPU-time evidence separate from the EC2 table. Per the 2026-10-10 operator ruling on #1003, the limitation now names the planned `LumpedPort`-like port formulation (design spike first, not yet built) and drops the reference-free classifier alternative; frequency matching to Palace remains the interim mode identification. |
| #967 | Added separate assembled CPU-f64 driven AMS section/table with time-boundary, mesh, repeat and memory limits. No GPU or eigenmode extrapolation. |
| #1034 pending work | Visible pending section; no anchor or coupling success claimed. Final trajectory, validation and independent extraction still required. |
| v6 review D2 (evidence sufficiency) | Harmonic and AMS evidence added. Multi-parameter optimization remains pending; no new score asserted. |
| v6 review minor: operator inputs | Every TODO(operator) line retained verbatim, including title, affiliations, tracking URL, repository/DOI and acknowledgments. |
| v6 review minor: length/recap | Shortened conclusion and replaced the old CPU section. New evidence still leaves the paper above the original length target; final compression remains open. |
| v6 review minor / audit: off-disk citations | Bibliography and citation-key set preserved. No new literature verification claimed; existing support limitations remain. |
| v6 review nit: figure filename/number skew | Retained historical filenames to avoid unrelated source churn; rebuilt CPU figure with direct TOML input and a self-contained script. |
| v6 review nit: rounded core-second inset | Replaced obsolete inset with all mode-recovering wall-time routes; table reports measured user+sys CPU seconds. |
| v6 audit: prior quotation fix and traceability notes | Retained resolved quotation and anchor-rounding clarifier. Removed superseded off-target table (and its borrowed RSS cells), preserved unchanged GPU 136× convention. |
| v6 numeric sidecar | No prior findings. Added a focused executable check for new table values, evidence counts, harmonic arithmetic and marker preservation. |

New version vendors the canonical paper class, carries all existing figures
and their sources forward, adds `xurl` for long artifact-path wrapping, and
ships a rebuilt PDF and BibTeX output. See `validation.md` for observed checks.
This revision is a draft, pending the anchor, independent review/audit, and
operator submission inputs; it is not READY or AUDITED.
