# Findings — transmon-benchmark.5 (cross-section observations)

## 1. The v4 critical flag is genuinely resolved (verified site by site)

`numerical_inconsistency_scale_story` (v4 review) asserted the retracted
OOM/memory-wall causal story at six sites. All six are verified fixed in
v5, and the corrected record — completes 565.5 s / 92.2 GB at ~1.16M DOFs
on a 128 GB box, loses to Palace (423.12 s / ~33 GB aggregate) on both
axes, flop-and-fill crossover below 1M — is stated identically everywhere
it appears (abstract L97–99, contributions L215–222, Table 3 caption +
rows + dagger note L1000–1056, §11.1 body + trade-off L1081–1102,
Threats to validity L1309–1312, Limitations (iv) L1323–1327). The
residual occurrences of "OOM" / "63.9 GB" / "memory wall" all sit inside
the explicit retraction framing or the header comment; grep-level scan
found no site asserting the old story. The v5 rewrite also did NOT
re-import the memory-wall claim into the new abstract (the specific risk
the v4 verdict warned about).

## 2. Numbers-vs-committed-artifacts (new content, exhaustive spot-check)

Every checked claim traces exactly. Key rows (text → artifact):

| Claim (v5 text) | Artifact value | Source |
|---|---|---|
| Newton hits 0.2156 GHz target; fresh solve 1.4e-15 | `e_c_target_ghz = 0.215600`; `e_c_fresh_rel_err = 1.382e-15` | `benchmarks/transmon_diffopt/results.toml` |
| θ matches closed form to 3e-15; damped 13 steps; C₀ = 120.0 fF, E_C0 = 0.1614 | `theta_minus_closed_form = 3.109e-15`; `damped_n_steps = 13`; `c0_ff = 120.0`, `e_c0_ghz = 0.161419` | same |
| E_J/E_C = 51.0, ω01 = 4.128, α = −0.247 at hit target | 51.021 / 4.127823 / −0.247317 | same, `[qubit_at_target]` |
| ∂C_Σ/∂θ = 198.198 fF; FD 1.15e-4 at h = 1e-4; O(h²) sweep 1.2e-2→1.0e-3→1.15e-4→1.0e-5 | 198.198075; 1.154e-4; 1.200e-2/1.041e-3/1.154e-4/1.038e-5 | `benchmarks/transmon_diffopt/pad_results.toml` |
| 2-step demo convergence, fresh 5.6e-6; gradient changes ~21%; run 10.7 s | `n_steps = 2`, `c_sigma_fresh_rel_err = 5.633e-6`; −2.0245e8→−1.5990e8 (21.0%); `wall_clock_s = 10.7` | same |
| Anchor θ ≈ −0.241; inversion at −0.0097; θ_safe = −0.0073; stalls 136.5 fF; 33×; 101 nodes; ~225 μm lever | −0.2412 / −0.009677 / −0.007258 / 136.537467 / 33.2 / 101 / 225.042 | same |
| C_Σ = 136.7 fF, E_C = 0.142, E_J/E_C = 77.6, ω01 = 3.38, α = −0.158; BC delta 6e-5; scalar-vs-tensor 0.75% | 136.6847 / 0.141715 / 77.621 / 3.383324 / −0.157535 / 5.9747e-5 / 7.4772e-3 | `benchmarks/transmon_quantum/results.toml` |
| 565.5 s (setup 35.78 + solve 529.75), 92.2 GB peak, 1,157,564 interior DOFs | SETUP_S 35.784, SOLVE_S 529.747, TOTAL_S 565.531; max RSS 92,166,884 kB; 1157564 | `benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log` |
| CPU matched cells 28.7/29.0/130.9/44.5 and 36.8/26.6/248.0/64.7; Palace large 423.12 s / 4.1 GB/rank; 63.9 GB truncated | identical, `[matched.*]` blocks | `benchmarks/transmon_bench_cpu/results.toml` |
| GPU table 0.024/0.203/1.540/6.036 · 0.032/0.198/0.709/1.865 · 1.652/8.885/30.234/82.056 · 4.388/13.351/34.381/81.764; accuracy 1.26e-4→1.20e-3 | identical (medians) | `benchmarks/gpu_driven_scaling/results.toml` |
| Roadmap: 14,300 inner iters, ‖r‖≈2.6e-6, coarse-solve-invariant, 1e-2 tol → ~56 s → 0.42–0.64 GHz cluster | identical | `benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md` |
| Adjoint FD rel-errs ~3e-8 / ~1e-9 / 2.3e-5 (worst region) / 2.2e-9 & 1.2e-8; n_factorizations = 1; conjugation mutation test | doc-comment "Achieved worst-region rel-err ≈ 2.3e-5" in `driven/adjoint.rs`; `n_factorizations` asserts in `adjoint.rs`; conjugation-rejection test in `driven/shape.rs`; Table 2 caption honestly discloses observed-vs-acceptance distinction | `crates/geode-core/src/{adjoint,shape,driven/adjoint,driven/shape}.rs` |
| Eigen table + tripwires (0.7071; spurious 3.4528/p=0.9942/0.7081; tree–cotree 1.64%, 5.1528→5.2372; div ratio 50.2; projected norm 1.06e-4; port-aware 0.029%; 13,747 cluster) | identical | `benchmarks/transmon_eigen/results.toml` |

Derived arithmetic verified: 8×44.5 = 356.0 core-s (~12.4× → "~12×");
248.0/36.8 = 6.74 → "6.7×"; 64.7/26.6 = 2.43 → "2.4×"; 4.388/0.032 =
137.1 → "137×"; 81.8/1.86 = 44.0 → "44×"; 81.764/6.036 = 13.5 → "13.5×";
8×4.1 = 32.8 → "~33 GB"; the displayed-value rounding convention is now
stated in §12 (the v4 nit is resolved).

The one number the changelog flags as unlocatable in a committed artifact
(the old "fails FD at 0.58" figure for the dropped ∂b/∂X term) is
correctly stated qualitatively in v5 ("omitting it fails the
finite-difference cross-check outright") — the honest handling.

## 3. Reframe-honesty audit (BRIEF ⭐ 2026-07-16 + spine rules)

- **Leads with the contribution, credential demoted**: §1 states "a
  credential, not the contribution" and pivots; §5 opens with the same
  boundary. PASS
- **LOM-now / eigenmode-roadmap discipline**: §10 carries the two-row
  branch table, cites the PHJD anchor "as method, not result", and closes
  "no claim in this paper extends to derivatives of the eigenmode
  spectrum"; Limitations (vi) repeats the boundary. No overclaim found
  anywhere (abstract says "eigenmode/EPR derivatives remain roadmap"). PASS
- **Affine-Newton honesty**: disclosed three times (abstract,
  §9.1 "one-step convergence is expected... proves the loop end-to-end",
  Threats to validity). PASS
- **33×-short anchor honest negative**: presented as a diagnosed
  mesh-morphing finding with named follow-ons, "a result, not an
  embarrassment". PASS
- **E_C anchor gap intact**: §7 reports 136.7 fF vs ~90 fF with the
  BC-insensitivity diagnosis; no retrofit. PASS
- **GPU Branch-A posture**: §11.2 is an explicit honest negative
  ("GPU-f32 matrix-free loses to every CPU configuration at every
  measured size") with trajectory-not-achievement framing; no Branch-B
  residue. PASS
- **Not over-indexed on qubits**: one generality sentence in §1, one in
  Discussion. PASS
- **Matrix-free eigensolve retired as scale story**: §11.1 closes with
  the explicit retirement; the wall is roadmap-only in §10. PASS

## 4. Compile / render

Independent compile to fixpoint: pdflatex ×3 + bibtex, 25 pages, zero
undefined citations/references, zero missing `\input` targets
(single-file), one 1.13 pt overfull hbox (below the 5 pt gate; v4's
36.96/17.75 pt boxes resolved by the §11.2 rewrite). Rendered-page
inspection of the new content (pp. 13–17): Figure 5 renders both panels
with the budget/anchor annotations; Table 3 renders both large-scale rows
with the dagger note; no clipped columns or captions. Shipped `main.pdf`
matches the fresh compile (the v4 staleness defect is fixed). `_gate.json`
is not emitted: the render gate's declared `log_path`
(`transmon-benchmark.5.audit/compile-log.txt`) does not exist yet —
fail-open per step 4b; pub-audit's compile will produce it.

## 5. Score trajectory

39 (v1) → 42 (v3) → 40 (v4, blocked on flag) → 41 (v5, no flags). The
v5 delta over v4: +1 D6 (Table 3 repaired), +1 D7 (overfulls + seam
fixed), −1 D8 (the §1 inexact quotation, new at v5). Same rubric
(`anvil-pub-v2`) as the prior review — no rubric version transition.
