# Citation audit: motor-gradient-optimization.8

Audited tree: `git archive 92120067 papers` (branch `claude/motor-paper-v7`), extracted into a scratch copy. The worktree was not modified apart from this audit sidecar.

## Resolution

`refs.bib` has 19 entries: the 17 from v7, byte-identical, plus 2 new ones (`diff` of v7 and v8 `refs.bib` shows only the two additions). Cite keys were enumerated in `main.tex` and its `\input` children (`optimization.tex` and the tables). There are 19 distinct keys. All 19 resolve, all 19 render in `main.bbl` (19 `\bibitem`), and bibtex reports 0 warnings. The rebuilt `main.bbl` is byte-identical to the committed one, and the rendered PDF has no `[?]`.

The 17 inherited entries and the sentences that cite them are unchanged from v7, with one exception: the `ion2018robust` paragraph in Section 2 gains two sentences that cite the new keys (below). The v7 audit's verdicts therefore carry forward: 15 entries `supports`, with Crossref metadata matching, and kraft1988 and ragonneau2022 checked at metadata level only. The `ion2018robust` claim itself ("compare deterministic gradient-based optimization with stochastic search for electric devices, including a permanent-magnet motor, using geometry parametrizations that avoid remeshing") is unchanged.

## New citations (v8)

Both keys are cited together twice:

- main.tex:126-131: "Section 7 extends that comparison with a coarse finite-element competitor and a common validation target. Its time-to-target measure follows data and performance profiles for benchmarking derivative-free solvers \citep{more2009benchmarking,dolan2002benchmarking}, here with a fixed wall-time budget and censored targets."
- optimization.tex:27-29: "Time to a fixed target, with unattained targets kept as censored, follows the data-profile practice for derivative-free benchmarks \citep{more2009benchmarking,dolan2002benchmarking}."

| Key | Resolved | Surrounding claim | Verdict | Notes |
|---|---|---|---|---|
| more2009benchmarking | yes (article, DOI) | Time-to-target with censored failures follows the data-profile practice for derivative-free benchmarks | supports (metadata) / unverified, source not on disk | Crossref `10.1137/080724083`, fetched anonymously: journal-article, "Benchmarking Derivative-Free Optimization Algorithms", Jorge J. Moré and Stefan M. Wild, SIAM Journal on Optimization 20(1):172-191, 2009. Every bib field matches. Moré and Wild introduced data profiles: the fraction of problems a solver meets a fixed fractional-reduction test on within a given budget, with failures counted as unsolved. That matches the claim as worded. The paper's T50/T90/T99 targets resemble that test's tolerance τ. Two differences: Moré and Wild measure cost in function evaluations (simplex gradients), not wall time, and measure reduction against the best value any solver found, not a fixed F0 fraction. Section 2 says "here with a fixed wall-time budget and censored targets", which covers the first difference. No PDF is in `refs/`. |
| dolan2002benchmarking | yes (article, DOI) | Same two sentences | supports (metadata) / unverified, source not on disk | Crossref `10.1007/s101070100263`, fetched anonymously: journal-article, "Benchmarking optimization software with performance profiles", Elizabeth D. Dolan and Jorge J. Moré, Mathematical Programming 91(2):201-213, 2002. Every bib field matches. Dolan and Moré introduced performance profiles, which use ratios of cost to the best solver and give failed runs a sentinel ratio. Section 2 correctly says "data and performance profiles". The §7.1 sentence calls both "the data-profile practice", which loosely attributes data profiles to Dolan and Moré as well (advisory N9 in flags.md). The paper's paired per-start ratios (Table 6) are closer to performance-profile ratios than to data profiles, so the citation is apt. No PDF is in `refs/`. |

## Linked (non-`\cite`) sources changed in v8

| Link | HTTP | Claim | Verdict |
|---|---|---|---|
| `github.com/rjwalters/geode-fem/issues/1185` (optimization.tex:6) | 200 | "The protocol and all of its amendments were fixed in the preregistration before any main-run outcome was observed. One later deviation, in how the final refinement was run, was adopted after the main runs had finished ... it changes no selection rule, check or target." | **supports**. Read-only `gh api`: all 24 protocol, amendment, addendum, correction and clarification comments run from 2026-10-17T19:59Z to 2026-10-18T16:33Z. The amendments, addenda and rulings are labelled "before any run", "before any main run" or "before any main outcome"; the few unlabelled corrections and the "Pre-main clarification" also fall in that window. The main session id is `main-20261018T181644Z` (session.json), so the session started after the last of the 24. The only later comment is the deviation (6284369486, 2026-10-20T05:49:54Z). It states "The cohort follows the amendment 8 addendum ... exactly", "The check matrix is unchanged" and "Target credit and budgets are unchanged, and no design is replaced". It also describes the closed-capture refusals on host attestation, the 5,453-file inventory taken before any reboot, the RFC 3161 and OpenTimestamps tokens, and the macOS arm64 refinement host. All of these match §7.2 (optimization.tex:227-233). This resolves v7 critical flag C4. |
| `tree/9486a9da.../benchmarks/motor_optimization` (main.tex:864) | 200 | Harness "the source of the study's evaluator" | supports. `git cat-file` gives a tree at that commit, and the commit is an ancestor of origin/main. `anchor_preflight.json` records `base_commit: 9486a9da...` and `tree_clean: true`. Resolves v7 N10. |
| `tree/3e4ee0c6.../crates/geode-core/examples/icem_final_refinement.rs` (main.tex:866) | 200 | "the final-refinement binary at 3e4ee0c6" | supports. The path is a blob at that commit, and the commit is on origin/main. The deviation comment names "the native `icem_final_refinement` binary from main 3e4ee0c6". The link targets the example's source, not a binary; the binary's hash is in `refinement-tools/binary.sha256` (advisory N11). |

All 11 `\href` targets return 200 (anonymous `curl -L`, User-Agent `paper-audit/1.0`).

**Privacy:** every network request was anonymous. Crossref got 2 requests with User-Agent `paper-audit/1.0`, no `mailto` parameter, and no email or identity in any header, URL or payload. GitHub got 11 href GETs with the same User-Agent, plus read-only `gh api` calls for #1185.

## Summary

- Unresolved keys: 0
- does-not-support (`\cite`): 0
- partial: 0. One attribution looseness in §7.1 is advisory N9.
- Unverified, source not on disk: 4. The new more2009benchmarking and dolan2002benchmarking, and the carried-forward kraft1988 and ragonneau2022, were all checked at metadata level only.
- Linked-source contradictions: 0. The v7 C4 contradiction is resolved.
