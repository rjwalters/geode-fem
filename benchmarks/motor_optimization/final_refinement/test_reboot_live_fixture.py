"""Replay preserved live daemon argv in a synthetic process identity envelope."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reboot_attestation as proof
from test_reboot_attestation import fixture
from test_reboot_quiet import process


class LiveRehearsalFixtureTests(unittest.TestCase):
    def test_observed_daemons_pass_only_with_system_identity(self):
        raw = (Path(__file__).resolve().parent / 'smoke' /
               'quiet-rehearsal-e6e7e3ae.json').read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(),
                         '956f5530cbe7b0a03ac8ce8dd77ab5f48e7a78490630863136d28f4620aeed16')
        report = json.loads(raw)
        self.assertTrue(report['clean_pass'])
        self.assertEqual(len(report['attempts']), 1)
        rows = [r for r in report['attempts'][0]['processes']
                if r['verdict'] == 'system_daemon']
        self.assertEqual(len(rows), 3)
        self.assertEqual({r['comm'] for r in rows},
                         {'networkd-dispat', 'unattended-upgr', 'dbus-daemon'})
        observed = fixture()[3]
        for pid, row in enumerate(rows, 700):
            self.assertEqual((row['uid_class'], row['ppid']), ('system', 1))
            self.assertFalse(any('<' in arg for arg in row['argv_shape']))
            process(observed, row['comm'], row['argv_shape'], uid=999, parent=1, pid=pid)
        self.assertTrue(proof.quiet_processes(observed, capture_script_sha256='d'*64))
        # The live argv must not confer an exception on user-owned processes
        # or processes outside the system service parent relationship.
        for pid, row in enumerate(rows, 700):
            for field, value in (('uid', 1000), ('ppid', 2)):
                with self.subTest(comm=row['comm'], field=field):
                    bad = copy.deepcopy(observed)
                    records = bad['process_commands' if field == 'uid' else 'process_table']
                    next(r for r in records if r['pid'] == pid)[field] = value
                    with self.assertRaisesRegex(ValueError, 'live evaluator'):
                        proof.quiet_processes(bad, capture_script_sha256='d'*64)


if __name__ == '__main__':
    unittest.main()
