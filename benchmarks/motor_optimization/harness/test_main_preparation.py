"""Prospective #1202 queue/resource contracts, without FEM or harness wiring."""
from collections import Counter
import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import main_schedule as schedule  # noqa: E402
import resource_usage as usage  # noqa: E402

KEYS = dict(zip("ABCDE", ("slsqp", "cobyqa", "surrogate", "cobyqa_l0", "slsqp_l0")))


class ScheduleTests(unittest.TestCase):
    def test_registered_rotation_and_reverse_for_every_start(self):
        expected = ("ABCDE", "BCDEA", "CDEAB", "DEABC", "EABCD")
        for start, order in enumerate(expected):
            self.assertEqual("".join(schedule.method_order(start, 1)), order)
            self.assertEqual("".join(schedule.method_order(start, 2)), order[::-1])

    def test_complete_unique_paired_cohort_and_positions(self):
        jobs = schedule.build_jobs(KEYS)
        self.assertEqual(len(jobs), 100)
        self.assertEqual(len({j["name"] for j in jobs}), 100)
        self.assertEqual([j["position"] for j in jobs], list(range(100)))
        counts = Counter((j["method_letter"], j["space"], j["start"], j["repeat"]) for j in jobs)
        expected = {(m, s, start, r) for m in "ABCDE" for s in "ab"
                    for start in ("theta0", "lhs1", "lhs2", "lhs3", "lhs4") for r in (1, 2)}
        self.assertEqual(set(counts), expected)
        self.assertTrue(all(n == 1 for n in counts.values()))
        self.assertEqual([j["method_letter"] for j in jobs[:5]], list("ABCDE"))
        self.assertEqual([j["method_letter"] for j in jobs[50:55]], list("EDCBA"))
        for repeat in (1, 2):
            for start in range(5):
                a = [j["method"] for j in jobs if j["space"] == "a"
                     and j["start_index"] == start and j["repeat"] == repeat]
                b = [j["method"] for j in jobs if j["space"] == "b"
                     and j["start_index"] == start and j["repeat"] == repeat]
                self.assertEqual(a, b)

    def test_registry_order_does_not_change_registered_order(self):
        self.assertEqual(schedule.build_jobs(KEYS), schedule.build_jobs(dict(reversed(list(KEYS.items())))))

    def test_missing_extra_aliased_and_unsafe_method_keys_are_refused(self):
        cases = [{k: v for k, v in KEYS.items() if k != "E"}, dict(KEYS, F="extra"),
                 dict(KEYS, E="slsqp"), dict(KEYS, E="../slsqp"), dict(KEYS, E="")]
        for registry in cases:
            with self.subTest(registry=registry), self.assertRaises(ValueError):
                schedule.build_jobs(registry)
        for start, repeat in ((-1, 1), (5, 1), (True, 1), (0, True), (0, 3)):
            with self.assertRaises(ValueError):
                schedule.method_order(start, repeat)


def sample(user=2.0, system=1.0, children_user=4.0, children_system=3.0, peak=1000):
    def counter(u, s, p):
        return dict(available=True, user_seconds=u, system_seconds=s,
                    maxrss_raw=p, maxrss_raw_unit="bytes", maxrss_bytes=p)
    return dict(pid=123, platform="darwin", monotonic_seconds=10.0, source="fixture",
                self=counter(user, system, peak),
                reaped_children=counter(children_user, children_system, peak * 2))


class ResourceTests(unittest.TestCase):
    def test_platform_units_and_unknown_units(self):
        self.assertEqual(usage.rss_bytes(1234, "darwin"), (1234, "bytes"))
        self.assertEqual(usage.rss_bytes(1234, "linux"), (1234 * 1024, "KiB"))
        self.assertEqual(usage.rss_bytes(1234, "unverified-os"), (None, "unknown"))
        for invalid in (-1, True, 1.5):
            with self.assertRaises(ValueError):
                usage.rss_bytes(invalid, "darwin")

    def test_cpu_deltas_once_rss_observations_separate_and_no_mutation(self):
        before, after = sample(), sample(3.0, 1.5, 6.0, 3.25, 1500)
        originals = copy.deepcopy((before, after))
        out = usage.interval(before, after, label="run", children_complete=True)
        self.assertEqual(out["cpu"]["self"]["total_seconds"], 1.5)
        self.assertEqual(out["cpu"]["reaped_children"]["total_seconds"], 2.25)
        self.assertEqual(out["observed_cpu_seconds"], 3.75)
        self.assertTrue(out["cpu_complete"])
        self.assertEqual(out["rss"]["self"]["bytes"], 1500)
        self.assertEqual(out["rss"]["reaped_children"]["bytes"], 3000)
        self.assertNotIn("total_bytes", out["rss"])
        self.assertEqual((before, after), originals)

    def test_interruption_retains_observed_cpu_but_never_claims_complete(self):
        out = usage.interval(sample(), sample(3, 2, 5, 4), label="aborted request",
                             incomplete_reason="killed descendants may not have been reaped")
        self.assertEqual(out["observed_cpu_seconds"], 4)
        self.assertFalse(out["cpu_complete"])
        self.assertIn("killed", out["incomplete_reason"])

    def test_counter_reset_other_process_and_missing_data_are_unavailable(self):
        cases = [sample(user=1), dict(sample(), pid=124),
                 dict(sample(), monotonic_seconds=9),
                 dict(sample(), self={"available": False, "error": "fixture failure"})]
        for after in cases:
            with self.subTest(after=after):
                out = usage.interval(sample(), after, label="run", children_complete=True)
                self.assertIsNone(out["observed_cpu_seconds"])
                self.assertFalse(out["cpu_complete"])
                self.assertTrue(out["errors"])

    @unittest.skipIf(usage.resource is None, "getrusage unavailable")
    def test_live_child_excluded_until_reaped_including_its_reaped_child(self):
        # Independent nested subprocess fixture: no evaluator or FEM needed.
        child = r'''
import json,resource,subprocess,sys,time
subprocess.run([sys.executable,'-c',
 'import time; end=time.process_time()+.05\nwhile time.process_time()<end: pass'],check=True)
buf=bytearray(8*1024*1024)
end=time.process_time()+.02
while time.process_time()<end: pass
own=resource.getrusage(resource.RUSAGE_SELF)
children=resource.getrusage(resource.RUSAGE_CHILDREN)
print(json.dumps(dict(cpu=own.ru_utime+own.ru_stime+children.ru_utime+children.ru_stime)),flush=True)
sys.stdin.readline()
'''
        before = usage.snapshot()
        proc = subprocess.Popen([sys.executable, "-I", "-c", child], stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            evidence = json.loads(proc.stdout.readline())
            live = usage.snapshot()
            self.assertIsNone(proc.poll())
            for key in ("user_seconds", "system_seconds"):
                self.assertEqual(live["reaped_children"][key], before["reaped_children"][key])
            _, err = proc.communicate("finish\n", timeout=10)
            self.assertEqual(proc.returncode, 0, err)
            after = usage.snapshot()
            out = usage.interval(before, after, label="nested child fixture", children_complete=True)
            self.assertTrue(out["cpu_complete"])
            measured = out["cpu"]["reaped_children"]["total_seconds"]
            self.assertGreaterEqual(measured + 1e-6, evidence["cpu"])
            self.assertGreater(measured, .05)
            json.dumps(out, allow_nan=False)
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.communicate(timeout=10)


if __name__ == "__main__":
    unittest.main()
