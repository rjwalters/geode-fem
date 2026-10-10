# Dense vector-tracking tier under the #988 score (issue #1021)

This directory holds a re-measurement of the three `#[ignore]`d dense tests in
`crates/geode-core/tests/silvermuller_self_consistent_vector_tracking.rs`. They
were run with the phase-invariant overlap score
`|uᵀ M v| / √(|uᵀ M u| · |vᵀ M v|)` from issue #988.

- **Commit:** `f66b2a604ca0c60bf791630feff8d6f29697522c`. This is the branch
  `feature/issue-1021`: `origin/main` at `ea7e722b` plus the per-step report
  commit `8a76c20a`. It contains PRs #1013 (#988) and #1017 (#999).
  `eigen/self_consistent.rs` is the same as on main.
- **Host:** Apple M3 Ultra, 28 cores, 96 GiB, macOS 27.0.1, rustc 1.98.1. The
  host was shared with unrelated jobs. The 1-min load average ran from 17 to 85
  (`loadavg.txt`, one sample per minute). It was 17 at the start, about 40 to
  75 for most of the run, and 45 at the end.
- **Command** (from the worktree root, using a private target directory):

  ```sh
  CARGO_TARGET_DIR=/Volumes/Stripe/cargo-target-issue-1021 \
    cargo test -j 6 -p geode-core --release \
    --test silvermuller_self_consistent_vector_tracking \
    -- --ignored --nocapture --test-threads=1 vector_tracked_
  ```

  The command was wrapped in `/usr/bin/time -p` and detached with `nohup`.
- **Wall clock:** 4943 s (82 min). It started 2026-10-10T00:04:51Z
  (2026-10-09 17:04 local) and ended about 01:27Z. The per-test times were:

  | Test | Time |
  |---|---|
  | `vector_tracked_beats_frozen_int_idx` | 3842 s (frozen leg 1436 s) |
  | `vector_tracked_converges_on_lowest_mode` | 830 s |
  | `vector_tracked_handles_mode_death` | 182 s |

- **Result:** all 3 passed.

## Outcome

- **`vector_tracked_beats_frozen_int_idx`: unchanged.**
  - Vector tracking picks dense index 368 for solves 1 to 4 and index 767 from
    solve 5 on. It ends `Converged(28)` at `k = 0.562710 + 0.430975j`,
    `Q = 0.6528`, which is 0.88 % from `k*`.
  - The pick scores 0.8791 at iteration 5, where the best unpicked candidate
    scores 0.3762. At every other step the pick scores 0.999 to 1.0063 and no
    other candidate scores above 0.1635.
  - The frozen index changes eigenvector at iteration 5 (score 0.0007). It
    ends `MaxIterations(20)` at `k = 1.226173 + 1.148074j`, `Q = 0.5340`.
- **`vector_tracked_converges_on_lowest_mode`: unchanged.** The pick is index
  368 at all 15 solves, each scoring 1.0000, with no other candidate above
  0.0032. It ends `MaxIterations(15)` at `k = 1.226122 + 1.148031j`,
  `Q = 0.5340`.
- **`vector_tracked_handles_mode_death`: changed outcome.**
  - The run ends `ModeLost` at iteration 2, with best overlap 0.1042 and
    `last_k = 6.1457 + 0.5249j`. Before #988 it ended `MaxIterations(10)` at
    `k = 2.2580 + 2.0224j`.
  - At iteration 2 the old score gives index 675 a score of 1.0322, an
    artifact of the eigenvector's phase. The new score gives no candidate more
    than 0.104.
  - The sparse tier gets the same result (`ModeLost` at iteration 2).

Each solve has a `(report)` line in the log. It gives the pick (index, `λ`,
`k`, `Q`), the pick's new and old scores, the two best other candidates, and
the candidate the old score would have picked. The old-score column is a
counterfactual for that one step, computed on the same eigenvectors.

## Files

- `run.log`: the complete run.
- `loadavg.txt`: load average, one sample per minute, during the run.
- `run.partial-killed.log`, `loadavg.partial-killed.txt`: an earlier attempt,
  killed partway through `vector_tracked_beats_frozen_int_idx`. Its process died
  with the agent session that launched it, after the frozen leg (20 solves). It
  is kept because it records the same frozen-leg numbers at load 120 to 160. It
  is not used for the outcome above.
