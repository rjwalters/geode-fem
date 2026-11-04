# Numerical audit: motor-gradient-optimization.17

Audited content: `motor-gradient-optimization.17/` at commit `45221c52`.

## Scope and what carries from v16

- `diff -rq` of v16 against v17, excluding build outputs, shows that every data extract (`figures/src/data/**`, except `deposit/`), every table (`tables/*.tex`), every figure PDF and every render script is **byte-identical** to v16.
- Only these files changed: `main.tex`, `optimization.tex`, `README.md`, `changelog.md`, `_progress.json`, the three ablation and main verifiers, `figures/src/build_deposit.py`, `deposit/deposit-record.json` and the new `deposit/MANIFEST.sha256`.
- v16's numerical audit recomputed every headline, table and decomposition number from the raw validation rows, and found 0 inconsistencies.
- Its independent scripts (`../motor-gradient-optimization.16.audit/auditor-scripts/x16.py`, `x16cost.py`) were re-run here on the v17 package:
  - `x16.py` output is identical for v16 and v17;
  - `x16cost.py` reproduces the same cost figures (`auditor-scripts/x17.out`, `x17cost.out`).
- This audit therefore checks the numbers v17 **added, removed or reworded**, plus the deposit numbers. `numeric_consistency` (888 numbers extracted, 6 claims checked) passes; its single info finding is the known suppressed 0.01071/0.01220 pair (main.tex:1064).

## Changed or new numerical claims

| Text claim | Source | Source value | Match | Notes |
|---|---|---|---|---|
| F_w's A′ median base training-solve charge "0.216 and 0.233" units (optimization.tex:325-326) | `exp2c/runs.json` `base_charges_seconds` (A′ runs) / unit, median per space; independent `x16cost.py` | a: 0.216, b: 0.233 | yes | Same number the v16 audit computed independently in N3. |
| F_w median base-solve charge "0.220 and 0.233" (optimization.tex:325) | same, F_w runs | a: 0.220, b: 0.233 | yes | Unchanged value, relabelled measure. |
| "median over runs of training time per training solve … 0.240 and 0.236 for F against 0.232 and 0.245 for its A′, and 0.220 and 0.238 for F_w's A′" (optimization.tex:321-323) | v16 audit recomputation; `verify_exp2b/2c` | as stated | yes | Values unchanged since v16. Each measure is now named and compared like with like (resolves v16 N3). |
| F-session A′/main-study A primary-crossing ratio "0.94–1.04" (main.tex:821-822) | `x16.py` (exp2b runs vs exp2 runs, same start/repeat, own units) | 0.944–1.043 | yes | |
| F_w-session A′/main-study A "0.97–1.05" (main.tex:822-823) | `x16.py` (exp2c runs vs exp2 runs) | 0.973–1.052 | yes | New in v17 (resolves v16 N4). |
| "by 29% at the crossing validation and 18% one validation before it, respectively" (main.tex:933-934), assigned to LHS 2 and LHS 4 | v16 audit (verifier-confirmed assignment) | LHS 2 at crossing: 29%; LHS 4 one before: 18% | yes | "respectively" now follows the start order given (resolves v16 N5). |
| Archive "2,790,582,024 bytes" (main.tex:506; README) | `stat` of `motor-paper-raw-traces.tar.zst`; `deposit-record.json` | 2,790,582,024 | yes | |
| Archive SHA-256 `1dfbf388…a1b98b07` (main.tex:507; README) | `shasum -a 256` of the archive (this audit), `deposit-record.json`, verifier re-hash | `1dfbf3883a9f1f3d8ddc44f67ce594b88291ec235a154337d24efa07a1b98b07` | yes | Matches in three places. |
| "5,453, 2,118 and 1,180 inventory-listed files" (main.tex:502) | tar stream member counts per group (this audit); `deposit-record.json` | 5,453 / 2,118 / 1,180 | yes | |
| README: manifest lists "10,781 files" | `MANIFEST.sha256` lines; tar stream (this audit) | 10,781 entries; 10,782 members including the manifest itself | yes | Accounting: seven record groups (10,762) + 15 inventory/timestamp files + `README.txt` + `mesh-cache-regeneration.json` + 2 `selection.json` = 10,781. The record's refinement-group counts (451/427) exclude `selection.json`, which my per-directory counts (452/428) include. |
| Manifest digest `cc6ac676…` (`deposit-record.json` `manifest_sha256`) | SHA-256 of the archived `MANIFEST.sha256` (tar stream) and of the package copy | `cc6ac676ac1735d3b32d2a19abbfea03729fcb7eabc927897a829519f5c03417` (both) | yes | |
| Abstract: 242 words (changelog) | text unchanged from v16 | — | yes | Abstract text identical to v16. |

## Removed numbers

- The superseded archive size and digest (2,609,375,627 bytes, `a70a0699…`) are gone from `main.tex`, `optimization.tex` and the README.
- `verify_exp2b.py` asserts that no earlier package name remains.

## Archive-to-manifest binding (independent of the verifiers)

- The archive was streamed (`zstd -dc | tarfile 'r|'`, `auditor-scripts/tarhash.py`) and every member hashed.
- Against `figures/src/data/deposit/MANIFEST.sha256`: 0 manifest entries are missing from the archive, 0 hash mismatches, 0 non-regular members. The only extra member is `MANIFEST.sha256` itself.
- The verifiers then check the reviser's unpacked copy against the same manifest. The unpacked tree is therefore bound to the archive bytes, not only to the reviser's say-so.
- The refinement mesh cache is absent, as stated; only `refinement/mesh-cache-regeneration.json` is present.

## Raw re-derivation (v16 N6)

Run from a **lone copy** of the package directory (no repository around it) with `MOTOR_RAW_ROOT=<unpacked>/motor-paper-raw-traces MOTOR_RAW_ARCHIVE=<archive> python3 -I -B verify.py`. It exits 0 with 30 PASS lines and one NOTE (no thread-root `.anvil.json`; `auditor-scripts/verify.raw.out`). Among the PASS lines:
- Exp 2 `runs`, `refinement` and `supplement` are byte-identical from the 100 traces and 213 results.
- Exp 2b and Exp 2c run and final-check extracts match exactly (40 traces / 90 results; 20 / 84), and their selection checks are byte-identical.
- Both ablation timelines match exactly.
- Binary equivalence: 12 + 12 results byte-identical and the rebuilt binary re-hashed.
- All 10,781 files match the manifest, and the archive re-hashes against the deposit record.

**Auditor's own tamper control:**
- A symlink overlay of the raw root was built in which one F_w final-check result (`refinement/exp2-fd-warm/results/be8f95d5eadf/tight/result.json`) had its first non-config float (`fresh_mesh_records[0].seconds`) multiplied by 1.001.
- `verify_exp2c.py` fails with `AssertionError: extract differs from raw re-derivation` (`auditor-scripts/verify_exp2c.tamper.out`).
- The raw check therefore discriminates even a timing field.

## Verifier execution (extract mode)

- In the repository: `verify.py` 20 PASS (exit 0; chains the other three), `verify_exp2.py` 4, `verify_exp2b.py` 6, `verify_exp2c.py` 6. Every exit is 0, and each prints a NOTE that no raw inputs were given. Two lines contain the word "FAIL" only inside a PASS description of the documented component-wise L1 failure.
- Lone copy without raw inputs: 20 PASS, exit 0, plus the `.anvil.json` NOTE (resolves v16 N8).

## Figure source-of-truth check

- Figure PDFs and render scripts are byte-identical to v16.
- The mtime test is uninformative in this worktree:
  - most files carry the checkout time (2026-10-20 15:35:33);
  - `figures/src/render_v6.py` (15:53) is newer than the nine figures dated 15:35:33;
  - git dates are no help either, because each version directory is copied wholesale in its revision commit.
- A direct test was run instead. On the lone scratch copy, `uv run --no-project --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render_v6.py` exits 0 and rewrites figures and tables. Afterwards, **every** figure PDF and every `tables/*.tex` is byte-identical to the committed files.
  - These include `coupling-waveforms`, `coupling-convergence`, `design-gradients`, `model-gradients`, `refined-validation` and `opt120-waveforms`, plus the three `render.py` figures it chains.
- v16's audit already showed that the Exp 2 figures re-render byte-identically, and those inputs are unchanged.
- No stale figures.
