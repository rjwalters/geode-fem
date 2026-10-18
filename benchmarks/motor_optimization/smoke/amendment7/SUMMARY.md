# Harness session `smoke-20261018T110340Z` (label: **smoke**)

**Smoke run: not data.** Both models at L0, an L0 anchor computed for this session, a short budget, and drills that exercise the safeguards (injected mesher failures, a boundary start, the gate). It checks the harness end to end, nothing else.

- Host: robb-studio, Apple M3 Ultra, 28 cores; load averages before 22.49, 22.92, 22.55, after 24.20, 24.83, 23.60; idle: False.
- Processes: 6 single-threaded; commit 88a10da7f7 (tree clean: True); rustc 1.98.1 (48a229cea 2026-09-01); scipy 1.18.1, numpy 2.5.4; lock 10a81655baf7.
- Unit: 3.3663 s, the median of 18 cold θ0 validations in 6 simultaneous processes (range 3.3534–3.3917 s); single-process median 3.3353 s (diagnostic); training forward 0.7795 FE, adjoint 0.1749 FE per evaluation; budget 20 FE.

| run | stop | wall FE | solve FE | train | remesh | valid. | best feasible F_val | T50 | T90 | T99 |
|---|---|---|---|---|---|---|---|---|---|---|
| slsqp_a_theta0_r1 | budget | 20.000 | 20.41 | 13 | 0 | 8 | 3.9877e-04 | 4.67 / 4.86 | 4.67 / 4.86 | censored |
| cobyqa_a_theta0_r1 | budget | 20.000 | 19.93 | 23 | 0 | 2 | 8.9644e-04 | 19.86 / 19.93 | 19.86 / 19.93 | censored |
| surrogate_a_theta0_r1 | budget | 20.000 | 19.37 | 21 | 0 | 3 | 7.0796e-03 | censored | censored | censored |
| slsqp_a_lhs1_r1 | budget | 20.000 | 21.45 | 12 | 0 | 10 | 3.9726e-04 | 4.48 / 4.86 | 4.48 / 4.86 | censored |
| cobyqa_a_lhs1_r1 | budget | 20.000 | 22.15 | 22 | 1 | 5 | 6.9292e-04 | 18.89 / 21.37 | 18.89 / 21.37 | censored |
| surrogate_a_lhs1_r1 | budget | 20.000 | 22.37 | 21 | 2 | 6 | 5.5795e-04 | 14.16 / 15.91 | 14.16 / 15.91 | censored |
| slsqp_b_theta0_r1 | T99 | 9.181 | 9.73 | 6 | 0 | 4 | 6.6380e-05 | 2.85 / 2.91 | 5.56 / 5.82 | 9.18 / 9.73 |
| cobyqa_b_theta0_r1 | budget | 20.000 | 19.49 | 25 | 0 | 0 | — | censored | censored | censored |
| surrogate_b_theta0_r1 | budget | 20.000 | 19.93 | 23 | 0 | 2 | 5.5054e-03 | censored | censored | censored |
| slsqp_b_lhs1_r1 | T99 | 9.925 | 10.73 | 6 | 1 | 5 | 8.5538e-05 | 2.91 / 2.91 | 8.26 / 8.77 | 9.93 / 10.73 |
| cobyqa_b_lhs1_r1 | budget | 20.000 | 23.38 | 30 | 1 | 0 | — | censored | censored | censored |
| surrogate_b_lhs1_r1 | budget | 20.000 | 22.49 | 25 | 3 | 3 | — | censored | censored | censored |
| slsqp_a_drill_r1 | budget | 20.000 | 23.41 | 13 | 1 | 11 | 4.1181e-04 | 2.42 / 2.91 | 2.42 / 2.91 | censored |
| cobyqa_a_drill_r1 | budget | 20.000 | 25.83 | 28 | 1 | 4 | 4.7738e-04 | 8.95 / 11.91 | 8.95 / 11.91 | censored |
| surrogate_a_drill_r1 | budget | 20.000 | 25.49 | 25 | 1 | 6 | 6.0880e-04 | 6.14 / 8.02 | 6.14 / 8.02 | censored |
| slsqp_b_lhs1_meshdrill_r1 | restoration_failed | 14.364 | 15.45 | 12 | 0 | 4 | 1.0804e-04 | 2.91 / 2.91 | 7.28 / 7.77 | censored |
| cobyqa_b_lhs1_meshdrill_r1 | budget | 20.000 | 22.61 | 29 | 0 | 0 | — | censored | censored | censored |
| surrogate_b_lhs1_meshdrill_r1 | budget | 20.000 | 22.71 | 24 | 0 | 4 | — | censored | censored | censored |
| slsqp_b_lhs1_repairdrill_r1 | T99 | 9.922 | 10.73 | 6 | 1 | 5 | 8.4630e-05 | 2.85 / 2.91 | 8.25 / 8.77 | 9.92 / 10.73 |
| cobyqa_b_lhs1_repairdrill_r1 | budget | 20.000 | 22.61 | 29 | 1 | 0 | — | censored | censored | censored |
| surrogate_b_lhs1_repairdrill_r1 | budget | 20.000 | 22.49 | 25 | 3 | 3 | — | censored | censored | censored |

Attainment cells are wall-time FE / solve-count FE at the first validation that attained the target, within the budget. Method C is an empirical RBF surrogate, not a reduced physical or waveform model (#1185 amendment 6).

## Safeguards (amendment 7) and host

| run | refusals | restorations ok/all | restarts | mesh checks ok/all | repaired after a failed attempt | inadmissible validations | C initial retries | 60-FE checkpoint | contaminated | in-run s | post-run s |
|---|---|---|---|---|---|---|---|---|---|---|---|
| slsqp_a_theta0_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90 | True | 67.3 | 0.0 |
| cobyqa_a_theta0_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90 | True | 67.3 | 0.0 |
| surrogate_a_theta0_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | — | True | 67.3 | 4.1 |
| slsqp_a_lhs1_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90 | True | 67.3 | 0.0 |
| cobyqa_a_lhs1_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90 | True | 67.3 | 0.0 |
| surrogate_a_lhs1_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90 | True | 67.3 | 4.1 |
| slsqp_b_theta0_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90/T99 | False | 30.9 | 0.0 |
| cobyqa_b_theta0_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | — | True | 67.3 | 0.0 |
| surrogate_b_theta0_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | — | True | 67.3 | 4.0 |
| slsqp_b_lhs1_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90/T99 | False | 33.4 | 0.0 |
| cobyqa_b_lhs1_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | — | True | 67.3 | 0.0 |
| surrogate_b_lhs1_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | — | True | 67.3 | 4.1 |
| slsqp_a_drill_r1 | — | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90 | True | 67.3 | 0.0 |
| cobyqa_a_drill_r1 | {'not_buildable': 4} | 0/0 | 0 | 0/0 | 0 | 0 | 0 | T50/T90 | True | 67.3 | 0.0 |
| surrogate_a_drill_r1 | {'not_buildable': 2} | 0/0 | 0 | 0/0 | 0 | 0 | 2 | T50/T90 | True | 67.3 | 4.0 |
| slsqp_b_lhs1_meshdrill_r1 | {'mesh_infeasible': 8} | 7/8 | 7 | 7/84 | 0 | 0 | 0 | T50/T90 | False | 48.4 | 0.0 |
| cobyqa_b_lhs1_meshdrill_r1 | {'mesh_infeasible': 1} | 0/0 | 0 | 0/0 | 0 | 0 | 0 | — | True | 67.3 | 0.0 |
| surrogate_b_lhs1_meshdrill_r1 | {'mesh_infeasible': 5} | 0/0 | 0 | 0/0 | 0 | 0 | 0 | — | True | 67.3 | 3.5 |
| slsqp_b_lhs1_repairdrill_r1 | — | 0/0 | 0 | 0/0 | 1 | 0 | 0 | T50/T90/T99 | False | 33.4 | 0.0 |
| cobyqa_b_lhs1_repairdrill_r1 | — | 0/0 | 0 | 0/0 | 1 | 0 | 0 | — | True | 67.3 | 0.0 |
| surrogate_b_lhs1_repairdrill_r1 | — | 0/0 | 0 | 0/0 | 3 | 0 | 0 | — | True | 67.3 | 4.0 |

Totals: C initial-set retries 2; inadmissible validations (no credit) 0; mesh checks ok 7; refused mesh_infeasible (cobyqa) 1; refused mesh_infeasible (slsqp) 8; refused mesh_infeasible (surrogate) 5; refused not_buildable (cobyqa) 4; refused not_buildable (surrogate) 2; repairs after a failed attempt 5; restarts 7; restorations failed 1; restorations ok 7.

- Host gate: off (smoke on a shared host); pre-run limit load <= 7, contamination above 9.0 for more than 60.0 s of the in-run phase.
- Deferred (not started): 0; host_idle: False.
- Calibration: pre-run load 22.35, contaminated False (longest overload 53 s).
- Gate drill slsqp_a_theta0_r1_gatedrill: deferred (deferral 0, load 24.20); never started.
- Gate drill slsqp_a_theta0_r1_gatedrill: deferred (deferral 1, load 24.20); never started.
- slsqp_a_theta0_r1: budget abort during a validate request (validation).
- cobyqa_a_theta0_r1: budget abort during a train request (forward).
- surrogate_a_theta0_r1: budget abort during a train request (forward).
- slsqp_a_lhs1_r1: budget abort during a validate request (validation).
- cobyqa_a_lhs1_r1: budget abort during a train request (forward).
- surrogate_a_lhs1_r1: budget abort during a train request (forward).
- cobyqa_b_theta0_r1: budget abort during a train request (forward).
- surrogate_b_theta0_r1: budget abort during a train request (forward).
- cobyqa_b_lhs1_r1: budget abort during a train request (forward).
- surrogate_b_lhs1_r1: budget abort during a validate request (validation).
- slsqp_a_drill_r1: budget abort during a validate request (validation).
- cobyqa_a_drill_r1: budget abort during a validate request (validation).
- surrogate_a_drill_r1: budget abort during a train request (forward).
- cobyqa_b_lhs1_meshdrill_r1: budget abort during a train request (forward).
- surrogate_b_lhs1_meshdrill_r1: budget abort during a validate request (validation).
- cobyqa_b_lhs1_repairdrill_r1: budget abort during a train request (forward).
- surrogate_b_lhs1_repairdrill_r1: budget abort during a validate request (validation).

Method C post-run held-out-current diagnostic (amendment 6 §3; not counted in the run's budget or attainment): C's prediction at the final incumbent against the training and validation models.

| run | fraction | std pred | std train | std val | mean pred | mean train | mean val |
|---|---|---|---|---|---|---|---|
| surrogate_a_theta0_r1 | 0.25 | 2.1959e-02 | 1.7836e-02 | 1.7836e-02 | 0.331870 | 0.317343 | 0.317114 |
| surrogate_a_theta0_r1 | 0.75 | 5.1838e-02 | 5.1565e-02 | 5.5697e-02 | 1.057455 | 1.041143 | 1.039883 |
| surrogate_a_lhs1_r1 | 0.25 | 6.3357e-03 | 4.5052e-03 | 5.3474e-03 | 0.338435 | 0.322345 | 0.322452 |
| surrogate_a_lhs1_r1 | 0.75 | 1.4502e-02 | 1.4502e-02 | 1.6110e-02 | 1.083556 | 1.065592 | 1.065776 |
| surrogate_b_theta0_r1 | 0.25 | 1.9303e-02 | 1.5356e-02 | 1.4837e-02 | 0.334637 | 0.321252 | 0.320927 |
| surrogate_b_theta0_r1 | 0.75 | 4.5420e-02 | 4.4206e-02 | 4.7536e-02 | 1.060490 | 1.045671 | 1.044140 |
| surrogate_b_lhs1_r1 | 0.25 | 1.1003e-02 | 8.1817e-03 | 7.6918e-03 | 0.335145 | 0.325626 | 0.325596 |
| surrogate_b_lhs1_r1 | 0.75 | 2.4869e-02 | 2.4544e-02 | 2.4671e-02 | 1.043829 | 1.033059 | 1.032911 |
| surrogate_a_drill_r1 | 0.25 | 6.5544e-03 | 4.5070e-03 | 5.3840e-03 | 0.335083 | 0.321059 | 0.321120 |
| surrogate_a_drill_r1 | 0.75 | 1.4922e-02 | 1.5085e-02 | 1.6761e-02 | 1.066012 | 1.050373 | 1.050441 |
| surrogate_b_lhs1_meshdrill_r1 | 0.25 | 8.3097e-03 | 5.8054e-03 | 5.5956e-03 | 0.303481 | 0.294651 | 0.294604 |
| surrogate_b_lhs1_meshdrill_r1 | 0.75 | 2.0327e-02 | 1.9675e-02 | 1.9939e-02 | 0.947473 | 0.937820 | 0.937607 |
| surrogate_b_lhs1_repairdrill_r1 | 0.25 | 1.1344e-02 | 8.5147e-03 | 7.9656e-03 | 0.334771 | 0.325295 | 0.325284 |
| surrogate_b_lhs1_repairdrill_r1 | 0.75 | 2.5638e-02 | 2.5419e-02 | 2.5467e-02 | 1.042775 | 1.032130 | 1.032063 |
