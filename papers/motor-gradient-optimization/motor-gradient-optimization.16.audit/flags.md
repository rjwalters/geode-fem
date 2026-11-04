# Audit flags for motor-gradient-optimization.16

Audited content: `motor-gradient-optimization.16/` at `463c7319` (byte-identical at worktree HEAD `481e32c8`, branch `claude/motor-paper-v11`, PR #1226, open). This verdict applies to this content only.

**Verdict: READY-WITH-AUDIT-FLAGS. 1 critical flag, 1 outstanding dependency, 10 non-critical notes.**

Everything else passes:
- **Build.** 4 pdflatex passes reach the `.aux` fixpoint. The PDF has 38 pages, 0 undefined references or citations and 0 overfull boxes. Its text is identical to the committed `main.pdf`.
- **Citations.** All 47 keys resolve; 0 claim-support failures; 1 partial (N2).
- **Numbers.** 0 inconsistencies. Every headline, table and decomposition number was recomputed independently from the raw validation rows.
- **Verifiers.** All pass in extract mode (20/4/6/6). In raw mode against the staged archive:
  - the archive's SHA-256 and byte count match the paper;
  - all 8,751 archived files match their inventories;
  - both ablation timelines and all 60 ablation run extracts re-derive exactly;
  - the main study's `supplement.json` re-derives byte-identically.
- **Regeneration.** Every table and every Exp 2 figure re-renders byte-identically.
- **Link targets.** All 19 `\href` targets return HTTP 200, and every pinned commit is on `origin/main`.

**State impact.**
- The critical flag holds the thread at READY-WITH-AUDIT-FLAGS. A `paper-revise` pass should fix C1 (one line).
- Independently, `[PENDING zenodo-doi]` holds AUDITED until the deposit returns a DOI.
- Both can be cleared in the same revision: v17 adds the DOI and the package link.
- N6 must be decided **before** the deposit, because the archive's internal README is covered by the SHA-256 the paper cites.

## Critical flags (block advancement to AUDITED)

- **C1. Unlinked artifact path** (Code and data availability, main.tex:463-465). The sentence reads: "The paper package is the directory `papers/motor-gradient-optimization/motor-gradient-optimization.16` of the GEODE repository."
  - The path is plain `\texttt`. Only the words "GEODE repository" carry an `\href`, and it points to the repository root.
  - This directory is the paper's reproducibility package: the verifiers, the hash-bound extracts, the inventories, the deposit record and the regeneration scripts.
  - `public_repo_url` is declared (`.anvil.json` and `BRIEF.md`: `https://github.com/rjwalters/geode-fem`), so under step 6c an unlinked named artifact path in the availability section is a critical flag.
  - It is not a dormant risk. PR #1226 is open, and `https://github.com/rjwalters/geode-fem/tree/main/papers/motor-gradient-optimization/motor-gradient-optimization.16` returns **HTTP 404** today. A reader following the paper cannot reach the package.
  - The `audience_check` scanner reported 0 hits because its path regex requires a final segment with a letter-led file extension, which `….16` does not have. This flag comes from the manual read the rule requires.
  - **Fix (mechanical):** after PR #1226 merges, wrap the path in an `\href` to a public location that contains it. Two options:
    - `\href{https://github.com/rjwalters/geode-fem/tree/main/papers/motor-gradient-optimization/motor-gradient-optimization.<N>}{…}`, or a tag;
    - a commit-pinned URL to the merge commit, if the next version links to an already-merged package directory.
  - Alternatively, include the package in the Zenodo deposit and link it through the DOI.
  - In v17 the directory name changes, and `verify_exp2b.py`/`verify_exp2c.py` assert it. Update those assertions to match.

## Outstanding dependencies (hold AUDITED; not scored defects, issue #842)

- **Pending marker** `[PENDING zenodo-doi]` (main.tex:502, Code and data availability).
  - `anvil.lib.pending_marker` reports exactly one active marker (`outstanding_sources: ["zenodo-doi"]`, `pass: false`, exit 1). A manual grep of all `.tex` sources finds no other.
  - The marker is a well-formed, honest disclosure of the operator's deposit of `motor-paper-raw-traces.tar.zst`: 2,609,375,627 bytes, SHA-256 `a70a0699261e40573187a0e4007e8d4b50f608be707a31a59aaf87c650203f12`. This audit re-hashed the staged file and confirmed both values. `deposit-record.json` reads `"uploaded": false`.
  - It is **not** a critical flag and lowers no score. It holds the AUDITED transition until the DOI exists and replaces the marker. When it is filled, make the DOI an `\href{https://doi.org/…}`.
  - The tool was run **without** `--write-review`, so no `motor-gradient-optimization.16.pending/` sidecar was written. The operator restricted this audit to one write.

## Non-critical notes

- **N1. Audit scope and tooling.**
  - The corpus tier is inactive (`resolve_corpus_dirs` returns `[]`), so no `.corpus-audit/` sibling was written.
  - `audience_check` passes: public URL `anvil_json`-declared; 0 governance-vocabulary and 0 private-locator hits. For the scanner's blind spot on extension-less directory paths, see C1. Worth an upstream anvil issue.
  - The build ran in a scratch copy, so the version directory was untouched and no `main.pdf` was regenerated in place.
- **N2. Yu et al. 2018: claim scope partial** (main.tex:160-162).
  - The paper says: "found gradient-free optimizers 4 to over 100 times as expensive as adjoint-driven ones **at the same optimum**".
  - The operator's full-text notes (`refs/yu2018-fulltext-check.md`) say all optimizers except NSGA2 reach the same optimum. The 4× end is NOMAD and the >100× end is NSGA2, so "at the same optimum" does not cover the upper end.
  - Suggested: "…4 times (NOMAD) to over 100 times (NSGA2, which did not reach the common optimum) as expensive as adjoint-driven ones". Or drop "at the same optimum" and keep "on an eight-variable wing twist problem".
  - The operator should confirm against the full text, which the auditor could not read.
- **N3. Mixed statistics in one comparison** (optimization.tex:320-323, §5 Gradient cost). "Medians of 0.240 and 0.236 units for F against 0.232 and 0.245 for its A′, and 0.220 and 0.233 for F_w against 0.220 and 0.238" uses two definitions:
  - F and both A′ figures are run-medians of (forward + line-search seconds)/solves;
  - F_w's figures are medians of individual base-solve charges, because its warm stencils would dilute the run average.
  - On F_w's own definition, its A′ gives 0.216 and 0.233.
  - The cost-model inputs are internally consistent, and no ratio or conclusion changes. Either label the two statistics or quote like-for-like values.
- **N4. "Control" paragraph names no session** (main.tex:811-814). "A′'s primary crossing times are 0.94–1.04 times those of the main-study A" holds for the **F**-session A′ (0.944–1.043). The F_w-session A′, the one plotted in Fig. 5 (exp2b-per-start), gives 0.97–1.05. Add "(F session; 0.97–1.05 in the F_w session)".
- **N5. Unassigned values** (main.tex:921-926). "At space-B LHS 2 and LHS 4 the iterates drift apart …, by 29% at the crossing validation and 18% one validation before it." The verifier confirms that 29% is LHS 2 at its crossing and 18% is LHS 4 one validation before its crossing (LHS 2 before its crossing: 6%; LHS 4 at its crossing: 3%). Add "respectively" or name the starts.
- **N6. Raw re-derivation claim vs. what the deposit enables. Decide before depositing** (main.tex:503-504; archive `README.txt`; package README).
  - The paper says "given the unpacked sessions, the package's verifiers re-derive the ablation extracts from the raw traces".
  - Verified: run extracts and timelines re-derive exactly from the archive. The timelines re-derive through the verifiers' `EXP2B_SESSION`/`EXP2C_SESSION` path.
  - Gap: the verifiers re-derive `runs.json` only when **both** `EXP2*_SESSION` and `EXP2*_REFINE` are set. The refinement result trees (`bundle/cohort.json`, `results/*/result.json`) are in neither the archive nor the package.
  - So a reader holding only the deposit cannot trigger the run-extract check as shipped. This audit had to call `run_record` directly. The final-check extracts (`refinement.json`) cannot be re-derived from deposited material at all.
  - The archive's own `README.txt` makes the same claim about `verify_exp2b.py`. Because that README is inside the archive, editing it changes the SHA-256 printed in the paper.
  - Options:
    - (a) split the env gate so session-only re-derivation of `runs.json` runs, and reword "re-derive the run extracts and timelines";
    - (b) add the three refinement result trees to the archive before deposit, and re-hash;
    - (c) leave the archive and narrow the paper's sentence.
- **N7. Two commits for the final-refinement program** (main.tex:462 against 986-988).
  - The data statement links the program at `3e4ee0c6`. The binary-equivalence paragraph says the pinned binary was built and rebuilt from `63b3e3d8`.
  - The two differ only by one unused-import line in `crates/geode-core/examples/icem_final_refinement.rs` (commit `94da11a1`).
  - Link `63b3e3d8`, or say that `3e4ee0c6` differs only by that import.
- **N8. Package verifiers need the repository layout.**
  - `verify_exp2b.py`/`verify_exp2c.py` assert that the directory is named `motor-gradient-optimization.16`, and `verify_exp2c.py` reads `../.anvil.json`.
  - A reader who downloads only the package directory gets `FileNotFoundError` from `verify_exp2c.py`, and `verify.py` aborts. Reproduced here before the scratch copy had the thread-root file.
  - Make the `.anvil.json` / `_progress.json` checks conditional on presence, so the standard-library verifiers run standalone. This matters if the package itself goes into the deposit.
- **N9. Stale verifier comment.** `verify_exp2c.py:369` still says "with that cost the estimates come within 4%". v16 removed that closure; the assertion at line 378 correctly forbids it in the text. This verdict advisory is unaddressed in v16. Comment only; no effect.
- **N10. Unverified citations (4, title-level, no source on disk).** `giles2000introduction`, `kuci2017design`, `peter2010numerical` and `kenway2019effective` have no retrievable abstract. Each is used only in the sense its title states. The author should confirm off-disk. Everything else was checked against a retrieved abstract or on-disk notes (citation-audit.md).

## Carried items: disposition

- The v9 audit notes A1–A5 are resolved or superseded:
  - the v9 A1 sentence on the exp2-quality figure (now Fig. 3) reads "the A, B, D and E level appears near 0.048 (C's near 0.054)";
  - the host type is now stated in the paper (main:669);
  - `dolan2002benchmarking` attribution split (main:166-168).
- The four v9-era unverified citations are now covered by `refs/citation-source-check-v9.md`.

## Audience-fit notes (non-critical, step 6c)

None. `audience_check`: 0 governance vocabulary, 0 private locators. The single link-hygiene finding is C1.
