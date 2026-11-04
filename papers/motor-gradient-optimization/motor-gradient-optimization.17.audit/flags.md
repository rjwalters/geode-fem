# Audit flags for motor-gradient-optimization.17

Audited content: `motor-gradient-optimization.17/` at commit `45221c52`. The branch `claude/motor-paper-v17` is pushed with that commit as its head; no PR was open at audit time. This verdict applies to this content only.

**Verdict: audit clean, 0 critical flags. AUDITED is held by 1 outstanding dependency (`[PENDING zenodo-doi]`). 7 non-critical notes.**

- **C1 is resolved.** The `.17` package link returns 404 until the branch merges. Under step 6c that does **not** block (ruling below), but merging is a precondition for depositing and publishing.
- **Every v16 N-flag is resolved or dispositioned**:
  - N2–N9 are resolved;
  - N1 (tooling) and N10 (unverified citations) carry forward, as directed.

Evidence summary:
- **Build.** 3 pdflatex passes reach the `.aux` fixpoint, run in a scratch copy, so the version dir is untouched. The PDF has 38 pages, 0 undefined references or citations and 0 overfull boxes. `main.bbl` is byte-identical to the committed one, and the PDF text is identical to the committed `main.pdf`.
- **Citations.** 47/47 keys resolve and 44/44 DOIs resolve. There are 0 claim-support failures and 0 partials; 4 citations remain unverified (title-level).
- **Numbers.** 0 inconsistencies. The data, tables and figures are byte-identical to v16. Every new number (0.216/0.233, 0.97–1.05, the archive size, digest and counts) was recomputed independently.
- **Verifiers, extract mode:** 20/4/6/6 PASS, all exits 0, both in the repository and on a lone copy.
- **Verifiers, raw mode** (lone copy, plus the unpacked archive and the archive path): 30 PASS, exit 0, a single `.anvil.json` NOTE. The auditor's own tamper control fails as it should.
- **Archive.** The size (2,790,582,024 bytes) and SHA-256 `1dfbf388…a1b98b07` were re-measured. All 10,781 members were streamed from the archive itself and match `MANIFEST.sha256`, and the manifest digest equals `deposit-record.json`.
- **Regeneration.** `render_v6.py` re-renders every figure and table byte-identically.
- **Links.** 21 of 22 `\href` targets return HTTP 200. The 22nd is the `.17` `tree/main` URL (404 until merge). Every pinned commit is on `origin/main`.
- **Lints.**
  - `audience_check` passes (public URL declared via `.anvil.json`; 0 hits).
  - `numeric_consistency` passes.
  - `pending_marker` finds `zenodo-doi` only.

## Critical flags (block advancement to AUDITED)

None.

### v16 C1 disposition: RESOLVED (link resolves after a pending merge; does not block)

- main.tex:468: the package path is now `\href{https://github.com/rjwalters/geode-fem/tree/main/papers/motor-gradient-optimization/motor-gradient-optimization.17}{\texttt{papers/…/motor-gradient-optimization.17}}`.
- App. D commit `63b3e3d8` (main.tex:997) is also linked now. A manual read of the whole availability section (main.tex:436-512) finds no other unlinked artifact path. The archive filename is tied to the pending DOI.
- **Ruling.** Step 6c requires each named artifact path to be "an `\href`/`\url` to a public location". The path is now an `\href`, to the correct and conventional location in the declared `public_repo_url`, so the fix the v16 audit prescribed is applied exactly.
- The target 404s today only because the branch has not merged. The same directory is public on the pushed branch: `tree/claude/motor-paper-v17/…/motor-gradient-optimization.17` returns HTTP 200 at commit `45221c52`.
- The fault C1 described was a bare path with no link at all. That fault is gone. The remaining 404 is a merge-ordering condition, not a defect in the paper's text, so it is not a critical flag.
- It blocks nothing **today**, because AUDITED is held by `[PENDING zenodo-doi]` regardless. **Condition:** before the thread is declared AUDITED, and before the Zenodo deposit is made (see N2), the branch must be merged and the URL re-checked for HTTP 200. If the URL still 404s at that point, the link becomes a critical flag.

## Outstanding dependencies (hold AUDITED; not scored defects, issue #842)

- **Pending marker** `[PENDING zenodo-doi]` (main.tex:508, Code and data availability).
  - `anvil.lib.pending_marker` reports exactly one active marker: `outstanding_sources: ["zenodo-doi"]`, `pass: false`, exit 1. A manual grep of all `.tex` sources finds no other.
  - It is an honest disclosure of the operator's deposit of the rebuilt archive: 2,790,582,024 bytes, SHA-256 `1dfbf3883a9f1f3d8ddc44f67ce594b88291ec235a154337d24efa07a1b98b07`. Both values were re-measured here. `deposit-record.json` reads `"uploaded": false`.
  - This is the declared operator dependency: not a critical flag, and it lowers no score. It holds AUDITED until the DOI exists and replaces the marker in a new version, which should make it an `\href{https://doi.org/…}`.
  - The tool was run **without** `--write-review`, so no `.17.pending/` sidecar was written, because the operator restricted this audit to one write.

## Non-critical notes

- **N1. Audit scope and tooling.**
  - The corpus tier is inactive, so no `.corpus-audit/` sibling was written.
  - The worktree has no `.anvil/` payload, so the `sidecar`, `pending_marker`, `audience_check` and `numeric_consistency` tools ran from the main checkout's `.anvil` (`uv run --project /Users/rwalters/GitHub/geode-fem/.anvil`). The sidecar was staged and committed with the CLI shim.
  - The build, lone-copy verifier runs and re-render ran in scratch copies, so `motor-gradient-optimization.17/` is unmodified and no `main.pdf` was regenerated in place.
  - Carried from v16: `audience_check`'s path regex still cannot see extension-less directory paths. The v17 path is linked, so this no longer bites, but an upstream anvil issue is still worth filing.
- **N2. Merge before deposit: the archive itself names the `.17` `tree/main` URL.**
  - The archive's internal `README.txt` says "Paper package …: https://github.com/rjwalters/geode-fem/tree/main/papers/motor-gradient-optimization/motor-gradient-optimization.17". That file is covered by the SHA-256 the paper prints.
  - If the archive is deposited before the merge, Zenodo readers get a 404 from inside the deposit. Order of operations:
    1. open and merge the PR for `claude/motor-paper-v17`;
    2. confirm the URL returns 200;
    3. deposit.
  - `tree/main/…` is a moving reference. It stays correct only while `motor-gradient-optimization.17/` is kept on `main`. Version dirs are immutable and retained (`.16` still resolves), so this holds as long as old versions are never pruned or renamed. Treat `.17/` as permanent once the deposit exists.
  - The DOI fill requires a new version (`.18`), whose package dir the verifiers will name. When it is drafted:
    - keep the archive byte-identical, so the printed digest stays valid;
    - let the `.18` verifiers accept the archive README's `.17` pointer;
    - consider pinning the `.18` package link to a commit or tag, not `main`.
- **N3. Yu et al. 2018: "slightly worse" is not on disk** (main.tex:160-163).
  - The rewording resolves the v16 N2 partial. The NOMAD/NSGA2 endpoints and "NSGA2 did not reach the common optimum" are supported by `refs/yu2018-fulltext-check.md`.
  - The new qualifier "which converged to a slightly worse design" (per the changelog, NSGA2 ended 0.82 drag counts higher) comes from an operator note that is not in the on-disk notes. OpenAlex has no abstract for this work.
  - Add the 0.82-count observation, with its section or table locator, to `refs/yu2018-fulltext-check.md`, so the qualifier is traceable on disk. No text change is needed.
- **N4. "Only by one unused import"** (main.tex:466).
  - `git diff 3e4ee0c6 63b3e3d8 -- crates/geode-core/examples/icem_final_refinement.rs` is a single `use` line: `use icem_remesh::{CpSpace, ScalarSpace, Space};` at `63b3e3d8` against `use icem_remesh::Space;`.
  - Both extra names are unused. That is two unused imported names on one line, so "one unused import" is loosely correct.
  - Optional precision: "only by one unused-import line". The claim of functional equivalence is correct.
- **N5. Timeline scope in the re-derivation sentence** (main.tex:510-512).
  - The sentence reads "re-derive the run, timeline and final-check extracts of all three sessions". Only the two ablations have timeline extracts (`exp2b/timeline.json`, `exp2c/timeline.json`). For the main study the verifiers re-derive `runs`, `refinement` and `supplement`.
  - Read distributively, the sentence implies a main-study timeline that does not exist.
  - Optional fix, in `main.tex` only: "the run and final-check extracts of all three sessions, the two ablations' timelines and the main study's supplement".
  - The archive `README.txt` carries the same wording but is inside the hash. Leave it, as a low-impact imprecision addressed to archive readers.
- **N6. Unverified citations (4, title-level, no source on disk).** `giles2000introduction`, `kuci2017design`, `peter2010numerical` and `kenway2019effective` are unchanged from v16, as the operator directed. Each is used only in the sense its title states; the author should confirm off-disk.
- **N7. Counting convention in `deposit-record.json`.**
  - The refinement-group counts (`refinement/exp2-fd-ablation`: 451, `refinement/exp2-fd-warm`: 427) exclude each ablation's `selection.json`. A per-directory count of the archive gives 452 and 428.
  - The totals reconcile: 10,762 group-counted files, plus 15 inventory/timestamp files, `README.txt`, `mesh-cache-regeneration.json` and 2 `selection.json`, give the 10,781 manifest entries.
  - The paper and the README cite neither 451 nor 427. Optional: add the selection files as their own key, or note the convention in the record.

## v16 flag disposition

| v16 flag | Status in v17 | Evidence |
|---|---|---|
| C1 (critical) unlinked package path | **Resolved**; link resolves after a pending merge | main.tex:468 `\href`; branch URL 200; ruling above |
| N1 tooling | Carried (scanner blind spot; now moot for this paper) | N1 |
| N2 Yu "at the same optimum" | **Resolved** | main.tex:160-163; residual on-disk traceability in N3 |
| N3 mixed statistics | **Resolved** | optimization.tex:321-326; 0.216/0.233 recomputed (`x17cost.out`) |
| N4 control paragraph session | **Resolved** | main.tex:821-823; 0.973–1.052 recomputed (`x17.out`) |
| N5 "respectively" | **Resolved** | main.tex:933-934 |
| N6 raw re-derivation gap | **Resolved** | Lone copy plus unpacked archive: 30 PASS, exit 0. Runs, final-check, supplement, timelines, selection checks and binary comparison are all re-derived. Archive members bound to the manifest independently; tamper control fails as expected. Residual wording imprecision in N5 |
| N7 two commits | **Resolved** | main.tex:462-466; both blob URLs 200; diff confirmed (N4) |
| N8 verifiers need the repo layout | **Resolved** | Lone copy: 20 PASS without raw inputs and 30 with them. The only NOTE is the `.anvil.json` skip |
| N9 stale "within 4%" comment | **Resolved** | The cost-closure comment is gone. The surviving `# 'within 4%'` at `verify_exp2c.py:129` binds a different, current claim (A′ across sessions, main.tex:962). No PASS label carries a version |
| N10 four title-level citations | Carried, as directed | N6 |

## Audience-fit notes (non-critical, step 6c)

None. `audience_check` finds 0 governance-vocabulary and 0 private-locator hits. A manual read of the availability section finds no further unlinked artifact paths.
