"""Regression replay of one actual parent-finalized trace from a live study.

This checks schema/arithmetic only. It never constructs a final cohort or grants
eligibility to the still-incomplete study.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

HERE=Path(__file__).resolve().parent


class MainTraceSchemaTests(unittest.TestCase):
    def test_actual_l0_trace_preserves_all_six_validations_and_ledger_prefixes(self):
        archive=HERE/'smoke/main_first_terminal_schema.tar.gz'
        self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(),
                         '6acb09661166cfd849f661fe71ec26b38f3d60da0d1c944c0dceb4ee02bbf126')
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve()
            with tarfile.open(archive) as tar:tar.extractall(root,filter='data')
            expected=json.loads((root/'result.json').read_text())
            replay=subprocess.run([sys.executable,'-I',str(root/'replay.py'),str(HERE.parents[2])],
                                  text=True,capture_output=True)
            self.assertEqual(replay.returncode,0,replay.stderr)
            result=json.loads(replay.stdout)
            self.assertEqual(result,expected)
            self.assertEqual(result['validations'],6)
            self.assertEqual(result['ledger_entries'],1377)
            self.assertEqual(result['mesh_kinds'],{'moved':6})
            self.assertFalse(result['can_select']);self.assertFalse(result['can_launch'])


if __name__=='__main__':unittest.main(verbosity=2)
