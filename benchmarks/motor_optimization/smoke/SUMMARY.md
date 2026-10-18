# Harness session `smoke-20261018T021605Z` (label: **smoke**)

**Smoke run: not pilot data.** Both models at L0, an L0 anchor computed for this session, a short budget. It checks the harness end to end, nothing else.

- Host: robb-studio, Apple M3 Ultra, 28 cores; load averages before 9.69, 11.35, 10.78, after 12.29, 11.65, 10.95; idle: False.
- Processes: 6 single-threaded; commit 332b1cc625 (tree clean: True); rustc 1.98.1 (48a229cea 2026-09-01); scipy 1.18.1, numpy 2.5.4; lock 10a81655baf7.
- Unit: 3.0774 s (median of 3.0774, 3.0763, 3.0938 s, cold θ0 validations); training forward 0.8163 FE, adjoint 0.1769 FE per evaluation; budget 20 FE.

| run | stop | wall FE | solve FE | train | remesh | valid. | best feasible F_val | T50 | T90 | T99 |
|---|---|---|---|---|---|---|---|---|---|---|
| slsqp_a_theta0_r1 | budget | 20.000 | 19.92 | 12 | 0 | 8 | 3.9877e-04 | 4.98 / 4.98 | 4.98 / 4.98 | censored |
| cobyqa_a_theta0_r1 | budget | 20.000 | 18.88 | 17 | 0 | 5 | 6.0825e-03 | censored | censored | censored |
| surrogate_a_theta0_r1 | budget | 20.000 | 19.14 | 21 | 0 | 2 | 7.1654e-03 | censored | censored | censored |
| slsqp_b_theta0_r1 | T99 | 9.759 | 9.96 | 6 | 0 | 4 | 6.6380e-05 | 2.99 / 2.99 | 5.89 / 5.97 | 9.76 / 9.96 |
| cobyqa_b_theta0_r1 | budget | 20.000 | 18.88 | 17 | 0 | 5 | 7.8163e-03 | censored | censored | censored |
| surrogate_b_theta0_r1 | budget | 20.000 | 18.96 | 22 | 0 | 1 | 7.8163e-03 | censored | censored | censored |

Attainment cells are wall-time FE / solve-count FE at the first validation that attained the target, within the budget.
