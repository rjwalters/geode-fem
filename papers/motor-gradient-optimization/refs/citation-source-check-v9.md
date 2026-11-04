# Citation source check: motor-gradient-optimization.9

Paper read from `/Volumes/Stripe/geode-fem/motor-paper-v7/papers/motor-gradient-optimization/motor-gradient-optimization.9/`. The local `/Users/rwalters/GitHub/geode-fem/papers/...` copy does not exist. The paper was not edited.

Where the keys are cited:
- optimization.tex:15-17: "A and E are safeguarded SLSQP \citep{kraft1988} using the field adjoints ...; B and D are COBYQA \citep{ragonneau2022} with $2n+1$ interpolation points ..."
- optimization.tex:27-29: "Time to a fixed target, with unattained targets kept as censored, follows the data- and performance-profile practice for benchmarking optimization solvers \citep{more2009benchmarking,dolan2002benchmarking}."
- main.tex:129-133: "Its time-to-target measure follows data and performance profiles for benchmarking derivative-free solvers \citep{more2009benchmarking,dolan2002benchmarking}, here with a fixed wall-time budget and censored targets."

---

## 1. kraft1988: SUPPORTED

**What I read**
- A scan of the report itself, https://degenerateconic.com/uploads/2018/03/DFVLR_FB_88_28.pdf. It has 8 pages: the cover, the title page, the summary, the table of contents, chapter 1 and the start of chapter 2. The full report is 33 pages.
- The SciPy SLSQP docs, https://docs.scipy.org/doc/scipy/reference/optimize.minimize-slsqp.html. They cite "Kraft, D., A software package for sequential quadratic programming, 1988, Tech. Rep. DFVLR-FB 88-28, DLR German Aerospace Center, Germany."

**What the source says.** The cover reads: "DFVLR Forschungsbericht. A Software Package for Sequential Quadratic Programming. Dieter Kraft, DFVLR, Institut für Dynamik der Flugsysteme, Oberpfaffenhofen. 33 pages, 42 references. DFVLR-FB 88-28". The title page reads "Oberpfaffenhofen, im Juli 1988", and the manuscript was submitted on 28 July 1988.
- The summary says it gives the SQP principles, the QP subproblem algorithms (primal, primal/dual and dual) and the package parameters.
- The contents list section 2.2.5, "Least Squares Subproblems", and Appendix A, "Subroutine SQP". Solving the QP as a least-squares problem is the "LSQ" in SLSQP.
- This is the report SciPy's SLSQP wrapper cites, and SciPy wraps Kraft's Fortran code.

**Verdict: supported.** This is the original SLSQP source. "Safeguarded" describes the authors' own wrapper and is not attributed to Kraft, which is fine.

**Bib correction (optional, the entry is already accurate).** You could add the address and month to match the title page:
```
  address = {Oberpfaffenhofen, Germany},
  month   = jul,
```
Nothing needs to change.

---

## 2. ragonneau2022: SUPPORTED. No better citation exists yet.

**What I read**
- arXiv 2210.12018, https://arxiv.org/abs/2210.12018, versions v1 (21 Oct 2022) to v5 (27 Feb 2023). It is the PhD thesis, supervised by Zaikun Zhang and Xiaojun Chen at PolyU, and runs to 199 pages.
- The COBYQA docs, https://www.cobyqa.com/stable/, which give the official "how to cite".
- The SciPy COBYQA docs, https://docs.scipy.org/doc/scipy/reference/optimize.minimize-cobyqa.html. Their only reference is the cobyqa.com link.
- An arXiv API search for "COBYQA", https://export.arxiv.org/api/query?search_query=all:COBYQA, plus web searches for a journal paper.

**What the source says.** The abstract calls the thesis's main contribution "COBYQA", a new derivative-free trust-region SQP method for nonlinearly constrained problems with equality and inequality constraints. COBYQA always respects bound constraints, and the thesis presents it as a successor to COBYLA. So the thesis does introduce COBYQA.

**A better citation?** None found as of 2026-10-20.
- There is no COBYQA journal article or standalone arXiv method paper by Ragonneau and Zhang.
- Ragonneau & Zhang, "An optimal interpolation set for model-based derivative-free optimization methods" (arXiv 2302.09992; Optimization Methods and Software, 2024) is a related paper about the initial interpolation set. It does not introduce COBYQA, so it should not replace the thesis.
- The COBYQA docs recommend citing the thesis plus a software entry. The software entry reads: T. M. Ragonneau and Z. Zhang, "COBYQA Version 1.1.4", 2026, https://www.cobyqa.com.

**Verdict: supported.** The `2n+1` interpolation-point setting has no citation in the sentence, and I did not check it against the thesis.

**Suggested bib additions** to make the entry findable (title, author, school and year are already correct):
```
  address = {Hong Kong, China},
  type    = {{Ph.D.} thesis},
  note    = {Department of Applied Mathematics. arXiv:2210.12018},
  doi     = {10.48550/arXiv.2210.12018},
  url     = {https://arxiv.org/abs/2210.12018}
```
Optional: the paper runs COBYQA through SciPy 1.18.1, which bundles the COBYQA package. You could also cite the software entry from cobyqa.com, using the version SciPy actually bundles.

---

## 3. more2009benchmarking: SUPPORTED

**What I read**
- Full text of the Argonne preprint ANL/MCS-P1471-1207 (April 2008), https://optimization-online.org/wp-content/uploads/2008/01/1883.pdf. It is the preprint of the SIAM paper.
- Crossref metadata for DOI 10.1137/080724083: SIAM J. Optim. 20(1):172-191, 2009, Moré and Wild. Every bib field matches.

**What the source says**
- The paper proposes data profiles "as a tool for analyzing the performance of derivative-free optimization solvers when there are constraints on the computational budget".
- It uses a convergence test against a target value: f(x) <= f_L + tau (f(x0) - f_L), with tau = 10^-k for k in {1,3,5,7}.
- The budget is a limit mu_f on function evaluations.
- It uses the convention r_{p,s} = infinity when a solver fails the test, which is effectively censoring.
- It notes that the performance measure t_{p,s} "could be based on the amount of computing time or the number of function evaluations".

**Verdict: supported.** Time to reach a target, with failures within a budget kept as unsolved, is the paper's method. Three differences from the source are already flagged by "here with", so the sentence is honest about them:
- The paper uses wall time, where Moré & Wild budget in function evaluations, measured in units of n_p+1 simplex gradients.
- The paper uses a fixed target, where Moré & Wild set f_L to the best value any solver found.
- "Censored" is the paper's own term. Moré & Wild use an infinity convention.

No bib correction is needed.

---

## 4. dolan2002benchmarking: SUPPORTED in optimization.tex, PARTIALLY in main.tex

**What I read**
- Full text of arXiv cs/0102001, https://arxiv.org/abs/cs/0102001. Its journal reference reads "Math. Program., Ser. A 91: 201-213 (2002)".
- Crossref metadata for DOI 10.1007/s101070100263: Mathematical Programming 91(2):201-213, Dolan and Moré. Every bib field matches.

**What the source says**
- The paper proposes performance profiles, "distribution functions for a performance metric", for benchmarking optimization software.
- Its metric is computing time: "t_{p,s} = computing time required to solve problem p by solver s".
- Failures get r_{p,s} = r_M, so 1 - rho_s(tau) is the fraction of problems a solver cannot solve. This handles failures without throwing them away. It is the source of the censoring idea, and it also supports using a time-based measure.

**Verdict**
- optimization.tex:29, "benchmarking optimization solvers": **supported**.
- main.tex:131, "data and performance profiles for benchmarking derivative-free solvers": **partially supported**. Dolan & Moré is not about derivative-free solvers. Its test sets are COPS and CUTE models, run with solvers such as MINOS, SNOPT, LOQO and KNITRO. Only Moré & Wild is derivative-free. The comparison in this paper also includes gradient-based SLSQP, so "derivative-free" is narrower than what is being benchmarked.

**Exact sentence fix (main.tex:129-133).** Match the optimization.tex wording:
> Its time-to-target measure follows data and performance profiles for benchmarking optimization solvers \citep{more2009benchmarking,dolan2002benchmarking}, here with a fixed wall-time budget and censored targets.

Or split the attributions:
> ... follows performance profiles \citep{dolan2002benchmarking} and their derivative-free data-profile extension \citep{more2009benchmarking}, here with ...

No bib correction is needed.

---

## Summary table

| Key | Bib metadata | Claim support | Action |
|---|---|---|---|
| kraft1988 | correct (checked against the report scan) | supported, original SLSQP source | optional: address/month |
| ragonneau2022 | correct | supported, thesis introduces COBYQA; no newer COBYQA paper exists | optional: add arXiv DOI/url/department; optionally cite software |
| more2009benchmarking | correct (Crossref) | supported | none |
| dolan2002benchmarking | correct (Crossref) | supported in optimization.tex; partial in main.tex | change "derivative-free solvers" to "optimization solvers" in main.tex:131 |
