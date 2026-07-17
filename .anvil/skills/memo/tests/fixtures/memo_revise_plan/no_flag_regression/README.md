# `no_flag_regression/` fixture

This fixture is intentionally **empty of plan artifacts**. It exists to
preserve the regression contract for AC3 of issue #243:

> **Default path unchanged.** `memo-revise <thread>` with no flags
> continues to produce `<thread>.{N+1}/` directly per the current
> 11-step procedure. The plan gate is opt-in; absence of `--plan` and
> `--apply` is the legacy behavior. This is load-bearing — every
> existing consumer MUST NOT break.

The associated test (`test_no_flag_path_unchanged` in
`tests/test_memo_revise_plan.py`) reads `commands/memo-revise.md` and
asserts:

1. The phrase "Default path (no flags)" / "legacy 11-step procedure" /
   "unchanged by issue #243" is documented somewhere in the file so a
   future edit that drops the unchanged-default contract trips the
   test.
2. The default-path step labels (1 through 11) survive verbatim — no
   silent renumbering or step deletion.
3. No `--plan` / `--apply` reference contaminates the original 11-step
   procedure block — the two-phase mode lives in its own dedicated
   §"Plan-then-apply mode" section + dispatch steps 0a / 0b at the top
   of the Procedure block.

If a future edit refactors the 11-step procedure, this test will fail
loudly so the unchanged-default-path contract is re-confirmed rather
than silently broken.

No `plan.md` lives in this directory. Its absence IS the fixture.
