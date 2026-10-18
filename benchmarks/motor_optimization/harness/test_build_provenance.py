"""Local build-receipt bindings with controlled compiler processes (no Cargo)."""
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from contextlib import contextmanager
import json
import os
import tempfile
import unittest
from unittest.mock import patch
import build_provenance as build
import owned_child
import session
from session_journal import read_events


@contextmanager
def fixture():
    with tempfile.TemporaryDirectory() as td:
        root = Path(td).resolve()
        before = Path.cwd()
        os.chdir(root)
        try:
            yield root
        finally:
            os.chdir(before)


def source(root):
    return {"repo": str(root), "commit": "fixture-commit", "tree_clean": True,
            "files": {"source.rs": "fixture-source-hash"}}


def environment(root, env):
    return {"tools": {"cargo": {"path": sys.executable}}, "selected_target": env["CARGO_TARGET_DIR"]}


COMPILER = '''
from pathlib import Path
import json,sys
p=Path(sys.argv[1]);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b'controlled evaluator')
print(json.dumps({'reason':'compiler-artifact','target':{'name':'icem_harness_eval','kind':['example']},'executable':str(p),'fresh':False}))
print(json.dumps({'reason':'build-finished','success':True}))
'''


def runner(command, **kwargs):
    assert command[1:8] == ['build', '--locked', '--release', '-p', 'geode-core', '--example', build.NAME]
    target = Path(command[command.index('--target-dir')+1])
    return owned_child.wait_run([sys.executable, '-I', '-c', COMPILER,
                               str(target/'release/examples'/build.NAME)], **kwargs)


def record(root, **kwargs):
    return build.record(root/'receipt', repo=root, source_probe=kwargs.pop('source_probe', source),
                        environment_probe=environment, runner=kwargs.pop('runner', runner), **kwargs)


class BuildTests(unittest.TestCase):
    def test_real_child_receipt_pins_snapshot_without_requiring_mutable_target(self):
        with fixture() as root:
            path = record(root)
            binary = path.parent/build.NAME
            result = build.verify(path, binary, source(root))
            self.assertTrue(result['verified'])
            data = json.loads(path.read_text())
            self.assertEqual(data['launcher_receipt']['returncode'], 0)
            self.assertFalse(data['cargo_artifact']['fresh'])
            # A later build changes its mutable output; the frozen copy stays valid.
            (root/'target/release/examples'/build.NAME).write_bytes(b'newer build')
            self.assertTrue(build.verify(path, binary, source(root))['verified'])
            with self.assertRaisesRegex(ValueError, 'selected evaluator'):
                build.verify(path, root/'target/release/examples'/build.NAME, source(root))
            with self.assertRaises(FileExistsError): record(root)

    def test_changed_source_or_any_bound_evidence_is_refused(self):
        with fixture() as root:
            path = record(root);binary=path.parent/build.NAME
            with self.assertRaisesRegex(ValueError, 'current clean'):
                build.verify(path,binary,dict(source(root),commit='different'))
            for name in ['inputs.json','cargo.log','build-receipt.json','events.jsonl',build.NAME]:
                p=path.parent/name;raw=p.read_bytes()
                if name==build.NAME:p.chmod(0o755)
                if name=='build-receipt.json':
                    value=json.loads(raw);value['wall_seconds']+=1;p.write_text(json.dumps(value))
                elif name=='events.jsonl':p.write_bytes(raw+b'{')
                else:p.write_bytes(raw+b'changed')
                with self.subTest(name=name),self.assertRaises((ValueError,KeyError,json.JSONDecodeError)):
                    build.verify(path,binary,source(root))
                p.write_bytes(raw)

    def test_dirty_or_changing_sources_never_publish_success(self):
        for dirty in (True,False):
            with fixture() as root:
                values=([dict(source(root),tree_clean=False)] if dirty else
                        [source(root),dict(source(root),commit='changed-during-build')])
                with patch.object(build,'cargo_artifact',wraps=build.cargo_artifact) as artifact:
                    with self.assertRaisesRegex(ValueError, 'clean experiment|changed during'):
                        record(root,source_probe=lambda _:values.pop(0))
                    artifact.assert_not_called()
                self.assertFalse((root/'receipt/build-receipt.json').exists())
                self.assertEqual(read_events(root/'receipt/events.jsonl')['events'][-1]['kind'],'build_failed')

    def test_failed_child_and_wrong_artifact_never_publish_success(self):
        for mode in ('failed','foreign'):
            with fixture() as root:
                def wrong(command,**kwargs):
                    if mode=='failed':
                        return owned_child.wait_run([sys.executable,'-I','-c','raise SystemExit(7)'],**kwargs)
                    return owned_child.wait_run([sys.executable,'-I','-c',COMPILER,
                                                str(root/'foreign'/build.NAME)],**kwargs)
                with self.assertRaisesRegex(ValueError,'successfully|outside'):
                    record(root,runner=wrong)
                self.assertFalse((root/'receipt/build-receipt.json').exists())

    def test_target_policy_and_exclusion(self):
        with fixture() as root:
            self.assertEqual(build.target_directory(root,{'CARGO_TARGET_DIR':'/shared/cache'}),root/'target')
            loom=root/'.loom/targets/owned'
            self.assertEqual(build.target_directory(root,{'CARGO_TARGET_DIR':str(loom)}),loom)
            def concurrent(command,**kwargs):
                with self.assertRaises(BlockingIOError):
                    build.record(root/'other',repo=root,source_probe=source,environment_probe=environment,runner=runner)
                return runner(command,**kwargs)
            record(root,runner=concurrent)
            self.assertFalse((root/'other').exists())

    def test_main_without_receipt_stops_before_calibration_or_preflight(self):
        with fixture() as root:
            binary=root/'eval';binary.write_bytes(b'old binary')
            with patch.object(session,'provenance',return_value={}), \
                 patch('start_preflight.verify', side_effect=AssertionError('unverified build reached preflight')) as preflight, \
                 patch.object(session,'gated_calibration') as calibration:
                with self.assertRaisesRegex(ValueError,'requires --build-receipt'):
                    session.main(['--main','--procs','1','--binary',str(binary),'--out',str(root/'session')])
                preflight.assert_not_called();calibration.assert_not_called()


if __name__=='__main__':unittest.main(verbosity=2)
