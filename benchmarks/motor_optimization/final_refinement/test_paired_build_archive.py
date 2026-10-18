"""Replay real retained compiler evidence without executing either binary."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class PairedBuildArchiveTests(unittest.TestCase):
    def test_copied_real_build_and_archived_process_evidence(self):
        archive = HERE / 'smoke/paired_build_1c6784e7.tar.gz'
        self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(),
                         'bcda844984de44312be6c2655285f11f24a37127f0603f79da056d57049b4063')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            with tarfile.open(archive) as tar:
                members = tar.getmembers()
                self.assertEqual(len(members), 18)
                self.assertEqual(len({m.name for m in members}), len(members))
                for member in members:
                    path = PurePosixPath(member.name)
                    self.assertTrue(member.isfile())
                    self.assertFalse(path.is_absolute())
                    self.assertNotIn('..', path.parts)
                    self.assertIn(member.mode, (0o644, 0o555))
                # Preserve the retained 0555 binary modes checked by the census.
                tar.extractall(root, filter='tar')
            replay = subprocess.run(
                [sys.executable, '-I', str(root / 'replay.py'), str(HERE.parents[2])],
                text=True, capture_output=True)
            self.assertEqual(replay.returncode, 0, replay.stderr)
            result = json.loads(replay.stdout)
            self.assertEqual(result['status'], 'PASS')
            self.assertEqual(result['source_files'], 1502)
            self.assertEqual(result['source_commit'],
                             '1c6784e7e1fc8fe0564b1d793a6c4061588028f5')
            self.assertEqual(result['manifest_sha256'],
                             '62a4592a4e1a7f53acde1ff7bdd8aacdd7b3650ebdc5b684b62710a93a93baed')
            self.assertEqual(result['binary_sha256'], {
                'icem_harness_eval': 'e6ccc3d6ba11d5e4711bd0275bc8e0a80fbe3247f06aeefa81923437436d8f34',
                'icem_final_refinement': 'a96ab99cf18540a3028d463e6cd171c06959a6fada4d10e215aeebc376ec4f11'})
            self.assertFalse(result['can_launch'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
