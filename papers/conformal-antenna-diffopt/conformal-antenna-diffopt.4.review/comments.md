# Comments — conformal-antenna-diffopt.4

Keyed to `main.tex` sections. Grouped by severity. No `\input`/`\include` children (single-file thread; `ResolvedTex.missing` empty).

## blocker

None.

## major

1. **`refs.bib` `fdtdx` entry / §1 + §2** (`FDTDX~\cite{fdtdx,mahlau2026fdtdx}`) — The carried preprint entry attributes FDTDX (arXiv:2412.12360) to "Schubert, Martin F. and others". The resolver-verified JOSS entry for the *same software* (`mahlau2026fdtdx`) lists Mahlau, Schubert (Frederik), Berg, Rosenhahn — and Martin F. Schubert is the `invrs-gym` author of the adjacent entry. The two jointly-cited references for one tool thus carry incompatible author lists; the preprint field is very likely misattributed. This entry predates v4 (carried byte-preserved from v1–v3 and never claim-support-verified — the audits recorded all carried lit cites as "unverified — source not on disk"), and `web_search: false` prevents this reviewer from resolving it live. `related-work` tag: verify against the arXiv record (a `paper-litsearch` micro-re-run or the pre-submission audit can do this with one resolver call) and fix the author field before submission. Not raised as a critical flag because the citation supports the surrounding claim correctly (FDTDX exists and is the JAX 3-D FDTD framework); the defect is the bibliographic author field, and the correct author list is already on display in the sibling entry.

## minor

2. **§3.4 Guards and validation / §Availability — overfull hboxes (D7)** — Reviewer rebuild logs 5 overfull hboxes > 5 pt: 92.9 pt at the body-section line "through the public `\texttt{driven\_solve\_with\_ports(MatchedUpml)}` path" (§3.4); 89.6, 57.8, and 15.5 pt in the availability section's long `\texttt` artifact paths; 7.9 pt in the intro contributions bullet. v3 shipped AUDITED with 4 (worst 161 pt in the same availability section), so the worst case improved while the count grew by one (the added `\url` line). Fix: allow breaks in the long monospace paths (e.g., `\path`/`\seqsplit`, or manual `\allowbreak` after directory separators) and reword the §3.4 sentence so the call signature does not start mid-line.
3. **§4 Results / §5.1 / §5.2 — unnumbered tables (D6)** — The per-frequency S11 table, the staircasing table, and the Meep runtime table are uncaptioned `tabular` blocks inside `center` environments: no number, no caption, not cross-referenceable (§5.3's prose must re-state their contents instead of citing "Table 2"). Promote all three to `table` floats with self-contained captions carrying the artifact filenames (matching the figures' provenance-stamp style).
4. **§Availability — no commit SHA (D5, carried from v2)** — "committed on the `main` branch ... publicly available at https://github.com/rjwalters/geode-fem" is now public (good; closes the v2 availability gap) but still unpinned. Pin a commit SHA or tag for the artifact of record.
5. **§5.1 staircasing table — non-monotonic perimeter column (carried from v2)** — 13.0/15.4/14.2/14.2% is verified correct against `staircasing_results.json` but reads as noise to a first-pass reviewer; a half-sentence ("the perimeter error oscillates about its plateau rather than converging") would preempt confusion. Numbers are frozen this pass, so this remains open.
6. **`refs.bib` completeness (D8)** — `ghassemi2013` lacks volume/pages (IET Microw. Antennas Propag. 7(4):268–276 is the usual citation; verify, do not guess into the file); `hooten2025` and `fdtdx` use "and others" author truncation, below the house standard the other 21 entries meet.

## nit

7. **§2 scoping paragraph** — "and is closed-source and cloud-executed besides" is factually correct and load-bearing for the open-source compute-axis scoping, but "besides" reads editorial; consider "and is, additionally, closed-source and cloud-executed" or fold into the §5.3 mention which already carries the parenthetical "(closed-source, cloud-execution) cost".
8. **§2 / global (D9, carried from v2)** — The −5.51→−12.06 dB and ~R³/~R⁴ headline numbers recur in abstract, intro, §4, §5.3, §6, and §7; §5.3 recapitulates §5.1–§5.2. Unchanged in this pass by design (numbers/results frozen); still the first candidate if a venue page cap ever binds.
9. **Procedural** — Render-gate skipped fail-open (contract input `.4.audit/compile-log.txt` absent; `paper-audit` has not run on v4); the reviewer rebuilt in a scratch directory for evidence — clean compile, 12 pages, 0 undefined citations/references, committed `main.bbl` byte-identical to the rebuild. Numeric-consistency detector ran via `uv` (285 numbers, 0 findings). `web_search: false` honored: all claim-support judgments defer to the `.3.litsearch/` sibling as citation authority; per-citation claim-support verification remains `paper-audit`'s job.
