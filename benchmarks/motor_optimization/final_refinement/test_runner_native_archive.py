"""Replay retained real native IO/process evidence; never launch a solver."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class RunnerNativeArchiveTests(unittest.TestCase):
    def test_relocated_native_archive_process_and_scientific_adapter_replay(self):
        archive = HERE / 'smoke/runner_native_1885192d.tar.gz'
        self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(),
                         '664c9a2facce6e2b41c0e37970a967805096baf86f0cd82e2fa8d4dfceb5afbd')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            with tarfile.open(archive) as tar:
                members = tar.getmembers()
                self.assertEqual(len(members), 50)
                self.assertEqual(len({m.name for m in members}), len(members))
                for member in members:
                    path = PurePosixPath(member.name)
                    self.assertTrue(member.isfile() or member.isdir())
                    self.assertFalse(path.is_absolute()); self.assertNotIn('..', path.parts)
                    self.assertIn(member.mode, (0o644, 0o755))
                tar.extractall(root, filter='tar')
            result = subprocess.run([sys.executable, '-I', str(root / 'replay.py'), str(HERE.parents[2])],
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads(result.stdout)
            self.assertEqual(summary['status'], 'PASS')
            self.assertEqual(summary['native_requests'], 1)
            self.assertEqual(summary['geometry_queries'], 1)
            self.assertEqual(summary['torque_samples'], 60)
            self.assertTrue(summary['processes_stopped'])  # Retained historical observations.
            self.assertEqual(summary['adapter_status'], 'ok')
            self.assertFalse(summary['can_report_main'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
