"""Run the #1218 FD-ablation adapter tests against targeted semantic errors.

Each mutation edits a private copy of one module. The tests need the reviewed
Git history and the frozen setup in this checkout, so the copy is not moved:
a launcher in the temporary directory preloads the mutated module under its
own name and then runs the unchanged test file (and, for the CLI test, the
mutated entry point). Writes the exact output as JSON to stdout. Never
modifies the working tree.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
TESTS = HERE / "test_ablation_selection.py"
MUTATIONS = [
    # The verified copy is read only through its retained inventory.
    ("ablation_selection.py", "inventory_digest_ignored",
     "if hashlib.sha256(raw).hexdigest() != self.inventory_sha256:", "if False:"),
    ("ablation_selection.py", "duplicate_inventory_entries_accepted", "if name in files:", "if False:"),
    ("ablation_selection.py", "uninventoried_copy_files_accepted",
     "if set(observed) - set(expected):", "if False:"),
    ("ablation_selection.py", "copied_bytes_unchecked", "        if changed:\n", "        if False:\n"),
    ("ablation_selection.py", "read_after_census_unchecked",
     "if hashlib.sha256(raw).hexdigest() != self.files[relative]:", "if False:"),
    ("ablation_selection.py", "bound_generation_hash_ignored",
     "if expected is not None and hashlib.sha256(raw).hexdigest() != _sha(expected):", "if False:"),
    ("ablation_selection.py", "inventory_inside_copy_accepted",
     "if self.root == self.inventory or self.root in self.inventory.parents:", "if False:"),
    # Exactly the 40 registered jobs, each with one eligible terminal attempt.
    ("ablation_selection.py", "queue_unchecked", "if _exact(jobs) != _exact(expected):", "if False:"),
    ("ablation_selection.py", "extra_traces_accepted", "    if extra:\n", "    if False:\n"),
    ("ablation_selection.py", "duplicate_completion_accepted", "if len(clean) > 1:", "if False:"),
    ("ablation_selection.py", "trace_name_unbound",
     "if a.get('trace_file') and a['trace_file'] !=", "if False and a['trace_file'] !="),
    ("ablation_selection.py", "incomplete_session_accepted",
     "        raise ValueError('FD-ablation session did not finish complete')", "        pass"),
    ("ablation_selection.py", "recovery_registry_accepted",
     "if (self.root / registry).exists() or any(", "if False and any("),
    ("ablation_selection.py", "pending_diagnostic_accepted",
     "if (a.get('trace') or {}).get('diagnostic_status') in ('pending', 'interrupted'):", "if False:"),
    ("ablation_selection.py", "unregistered_session_accepted",
     "            or set(session.get('models', {})) != {'L1'} or 'recovery' in generation['bindings']):",
     "            and False):"),
    # The scientific selection is the FD-ablation cohort's, on the ablation's setup.
    ("ablation_selection.py", "selection_uses_main_cohort",
     "starts=starts, cohort=cohort, queue=check)", "starts=starts, queue=check)"),
    ("study_selection.py", "assemble_ignores_cohort",
     "theta0_references=[setup['theta0']['b']['design']],cohort=cohort)",
     "theta0_references=[setup['theta0']['b']['design']])"),
    ("setup_evidence.py", "ablation_block_unchecked",
     "    if cohort in ABLATIONS and (_exact(session.get('ablation'))", "    if False and (_exact(session.get('ablation'))"),
    ("setup_evidence.py", "ablation_preflight_l0_mesh_unbound",
     "        _equal(pre['mesh_sha256'],{'0':files[L0_MESH]", "        _equal(pre['mesh_sha256'],{'0':pre['mesh_sha256']['0']"),
    ("ablation_selection.py", "trace_label_unchecked",
     "if type(raw) is not dict or raw.get('label') != cohort:", "if type(raw) is not dict:"),
    ("ablation_selection.py", "calibration_unchecked",
     "        if (calibration.get('accepted_for_dispatch') is not True", "        if (False"),
    # #1224: the {A', F_w} cohort through the same adapter; #1218's unchanged.
    ("ablation_selection.py", "warm_cohort_replays_the_1218_schedule",
     "'fd_warm_ablation': {'schedule': 'fd_warm_schedule',\n"
     "                         'files': ('main_schedule.py', 'fd_warm_schedule.py'),",
     "'fd_warm_ablation': {'schedule': 'fd_ablation_schedule',\n"
     "                         'files': ('main_schedule.py', 'fd_ablation_schedule.py'),"),
    ("ablation_selection.py", "warm_cohort_accepts_repeat_2",
     "'letters': ['A'] * 10 + ['W'] * 10,", "'letters': ['A'] * 20 + ['W'] * 20,"),
    ("ablation_selection.py", "warm_block_is_the_1218_block",
     "'ablation': setup_evidence.WARM_ABLATION, 'scope': WARM_SCOPE,",
     "'ablation': setup_evidence.ABLATION, 'scope': WARM_SCOPE,"),
    ("ablation_selection.py", "warm_output_claims_the_1218_scope",
     "'ablation': setup_evidence.WARM_ABLATION, 'scope': WARM_SCOPE,",
     "'ablation': setup_evidence.WARM_ABLATION, 'scope': SCOPE,"),
    ("ablation_selection.py", "session_label_unchecked",
     "        if (session.get('label') != cohort or not _positive(session.get('unit_seconds'))",
     "        if (not _positive(session.get('unit_seconds'))"),
    ("ablation_selection.py", "manifest_label_unchecked",
     "            or manifest.get('label') != cohort or manifest.get('session_id')",
     "            or manifest.get('session_id')"),
    ("ablation_selection.py", "cli_cohort_ignored",
     "reviewed_commit=args.reviewed_commit, cohort=args.cohort)",
     "reviewed_commit=args.reviewed_commit)"),
    ("ablation_selection.py", "setup_checked_as_1218",
     "git_repo=git_repo, reviewed_commit=reviewed_commit, cohort=cohort)",
     "git_repo=git_repo, reviewed_commit=reviewed_commit, cohort='fd_ablation')"),
    ("setup_evidence.py", "warm_setup_block_unchecked",
     "ABLATIONS={'fd_ablation':ABLATION,'fd_warm_ablation':WARM_ABLATION}",
     "ABLATIONS={'fd_ablation':ABLATION}"),
    ("setup_evidence.py", "warm_block_repeats_dropped",
     "               'repeats':[1],'post_hoc':True}", "               'post_hoc':True}"),
]
LAUNCHER = '''import importlib.util, pathlib, runpy, sys
sys.path.insert(0, {here!r})
spec = importlib.util.spec_from_file_location({module!r}, {mutated!r})
module = importlib.util.module_from_spec(spec)
sys.modules[{module!r}] = module
spec.loader.exec_module(module)
import ablation_selection
ablation_selection.HARNESS = pathlib.Path({harness!r})
if sys.argv[1:2] == ['--cli']:
    sys.argv = [ablation_selection.__file__] + sys.argv[2:]
    ablation_selection.main()
else:
    runpy.run_path({tests!r}, run_name='__main__')
'''


def main():
    tests = TESTS.read_text()
    rows = []
    for filename, name, before, after in [("ablation_selection.py", "baseline", None, None), *MUTATIONS]:
        source = (HERE / filename).read_text()
        candidate = source
        if before is not None:
            if source.count(before) != 1:
                raise RuntimeError(f"{name}: mutation anchor must occur exactly once")
            candidate = source.replace(before, after)
        compile(candidate, f"{name}/{filename}", "exec")
        with tempfile.TemporaryDirectory(prefix="fd-ablation-1218-adapter-mutation-") as td:
            root = Path(td)
            (root / filename).write_text(candidate)
            launcher = root / "launch.py"
            launcher.write_text(LAUNCHER.format(here=str(HERE), module=filename[:-3], mutated=str(root / filename),
                                                harness=str(HERE.parent / "harness"), tests=str(TESTS)))
            p = subprocess.run([sys.executable, "-I", str(launcher)], capture_output=True, text=True,
                               timeout=300, env=dict(os.environ, ABLATION_SELECTION_CLI=str(launcher)))
        passed = p.returncode == 0 if before is None else (
            p.returncode != 0 and "AssertionError" in p.stderr)
        rows.append({"name": name, "file": filename, "expected_result": passed, "exit_code": p.returncode,
                     "source_sha256": hashlib.sha256(candidate.encode()).hexdigest(),
                     "stdout": p.stdout, "stderr": p.stderr})
    print(json.dumps({"source_sha256": {n: hashlib.sha256((HERE / n).read_bytes()).hexdigest() for n in (
                          "ablation_selection.py", "study_selection.py", "setup_evidence.py",
                          "ablation_mutations.py")},
                      "tests_sha256": hashlib.sha256(tests.encode()).hexdigest(),
                      "checks": rows}, indent=2))
    return 0 if all(r["expected_result"] for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
