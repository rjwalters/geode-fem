# Citation audit: motor-gradient-optimization.17

Audited content: `motor-gradient-optimization.17/` at commit `45221c52` (branch `claude/motor-paper-v17`, pushed; no PR open at audit time).

## What changed since v16

- `refs.bib` is byte-identical to v16.
- Between v16 and v17, exactly one cited sentence changed: the `yu2018influence` sentence (main.tex:160-163). Every other `\cite` and its surrounding claim is unchanged, so the v16 claim-support verdicts carry forward (`../motor-gradient-optimization.16.audit/citation-audit.md`). Line numbers below are for v17.

## Resolution

- 47 distinct keys are cited across `main.tex`, its `\input` child `optimization.tex` and `tables/*.tex` (the tables carry no citations). `refs.bib` has 47 entries. 0 keys are unresolved and 0 entries are uncited.
- `main.blg` shows 0 warnings. The rebuilt PDF has 0 undefined citations (`compile-log.txt`).
- Every DOI was re-resolved in this audit:
  - 44 of 44 DOIs resolve (`auditor-scripts/crossref17.txt`).
  - 43 resolve on Crossref `works/{doi}`. `ragonneau2022` (`10.48550/arXiv.2210.12018`, DataCite) returns HTTP 200 on doi.org.
  - The three entries without a DOI (`arkkio1987`, `kraft1988`, `eaton2026octave`) are unchanged since v16, where they were verified.
- **Lookup hygiene.** Lookups went only to `api.crossref.org`, `doi.org` and `api.openalex.org`. The only header was `User-Agent: anvil-paper-audit/1.0` (the OpenAlex call used curl's default agent). No email address, `mailto` parameter, API key or other user identifier was sent.

## Claim-support spot check (delta)

| Key | Resolved | Surrounding claim | Verdict | Notes |
|---|---|---|---|---|
| yu2018influence | yes | "On an eight-variable wing twist problem, … found gradient-free optimizers 4 times (NOMAD) to over 100 times (NSGA2, which converged to a slightly worse design) as expensive as adjoint-driven ones" (main.tex:160-163). "They do not compare adjoint with finite-difference gradients, … but state its premise" (main.tex:165-167). | supports (notes); "slightly worse" partly off-disk | `refs/yu2018-fulltext-check.md` supports the NOMAD 4× and NSGA2 >100× endpoints, "all except NSGA2 reach the same optimum", the absence of an FD arm and the scaling premise. **The v16 N2 partial is resolved:** the sentence no longer claims a common optimum for the NSGA2 end. "Slightly worse" (the changelog cites an operator note: NSGA2 ended 0.82 drag counts higher) is not in the on-disk notes. OpenAlex holds no abstract for this work, so the magnitude is unverified on disk. See flags.md N3. |

## Carried verdicts (unchanged claims)

All other 46 keys carry their v16 verdicts unchanged:
- 42 `supports`, from abstract, on-disk notes, software or standard attribution;
- 4 `unverified — source not on disk`, title-level: `giles2000introduction`, `kuci2017design`, `peter2010numerical`, `kenway2019effective`.

The full table is in `../motor-gradient-optimization.16.audit/citation-audit.md`. Line numbers there are for v16. In v17, lines after main.tex:160 shift by +1 to +9.

## Summary

- Unresolved keys: **0**.
- Claim-support failures (`does-not-support`): **0**.
- Partial: **0**. The v16 Yu partial is resolved; the residual off-disk "slightly worse" magnitude is non-critical note N3.
- Unverified, source not on disk: **4** (title-level; recorded, not flagged).
