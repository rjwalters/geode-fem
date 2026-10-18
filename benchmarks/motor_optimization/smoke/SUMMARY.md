# Harness session `smoke-20261018T031255Z` (label: **smoke**)

**Smoke run: not pilot data.** Both models at L0, an L0 anchor computed for this session, a short budget. It checks the harness end to end, nothing else.

- Host: robb-studio, Apple M3 Ultra, 28 cores; load averages before 9.77, 9.93, 12.49, after 11.84, 10.46, 12.33; idle: False.
- Processes: 6 single-threaded; commit 9d780b9006 (tree clean: True); rustc 1.98.1 (48a229cea 2026-09-01); scipy 1.18.1, numpy 2.5.4; lock 10a81655baf7.
- Unit: 3.2837 s, the median of 18 cold θ0 validations in 6 simultaneous processes (range 3.2676–3.2951 s); single-process median 3.0901 s (diagnostic); training forward 0.7787 FE, adjoint 0.1754 FE per evaluation; budget 20 FE.

| run | stop | wall FE | solve FE | train | remesh | valid. | best feasible F_val | T50 | T90 | T99 |
|---|---|---|---|---|---|---|---|---|---|---|
| slsqp_a_theta0_r1 | budget | 20.000 | 20.40 | 13 | 0 | 8 | 3.9877e-04 | 4.65 / 4.86 | 4.65 / 4.86 | censored |
| cobyqa_a_theta0_r1 | budget | 20.000 | 19.91 | 23 | 0 | 2 | 8.9644e-04 | 19.82 / 19.91 | 19.82 / 19.91 | censored |
| surrogate_a_theta0_r1 | budget | 20.000 | 19.35 | 21 | 0 | 3 | 7.0796e-03 | censored | censored | censored |
| slsqp_b_theta0_r1 | T99 | 9.150 | 9.72 | 6 | 0 | 4 | 6.6380e-05 | 2.81 / 2.91 | 5.52 / 5.82 | 9.15 / 9.72 |
| cobyqa_b_theta0_r1 | budget | 20.000 | 19.47 | 25 | 0 | 0 | — | censored | censored | censored |
| surrogate_b_theta0_r1 | budget | 20.000 | 19.91 | 23 | 0 | 2 | 5.5054e-03 | censored | censored | censored |

Attainment cells are wall-time FE / solve-count FE at the first validation that attained the target, within the budget. Method C is an empirical RBF surrogate, not a reduced physical or waveform model (#1185 amendment 6).
- slsqp_a_theta0_r1: budget abort during a validate request (validation).
- cobyqa_a_theta0_r1: budget abort during a train request (forward).
- surrogate_a_theta0_r1: budget abort during a train request (forward).
- cobyqa_b_theta0_r1: budget abort during a train request (forward).
- surrogate_b_theta0_r1: budget abort during a train request (forward).

Method C post-run held-out-current diagnostic (amendment 6 §3; not counted in the run's budget or attainment): C's prediction at the final incumbent against the training and validation models.

| run | fraction | std pred | std train | std val | mean pred | mean train | mean val |
|---|---|---|---|---|---|---|---|
| surrogate_a_theta0_r1 | 0.25 | 2.1959e-02 | 1.7836e-02 | 1.7836e-02 | 0.331870 | 0.317343 | 0.317114 |
| surrogate_a_theta0_r1 | 0.75 | 5.1838e-02 | 5.1565e-02 | 5.5697e-02 | 1.057455 | 1.041143 | 1.039883 |
| surrogate_b_theta0_r1 | 0.25 | 1.9303e-02 | 1.5356e-02 | 1.4837e-02 | 0.334637 | 0.321252 | 0.320927 |
| surrogate_b_theta0_r1 | 0.75 | 4.5420e-02 | 4.4206e-02 | 4.7536e-02 | 1.060490 | 1.045671 | 1.044140 |
