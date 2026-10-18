"""Controlled child builds exercise paired provenance; no Cargo or FEM here."""
from contextlib import contextmanager
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

HERE=Path(__file__).resolve().parent;sys.path.insert(0,str(HERE))
import refinement_build as paired
from closed_capture import census

COMMIT='9486a9da58beeca91968d4d3608f06b998abff45'
COMPILER='''from pathlib import Path
import json,sys
name=sys.argv[1];p=Path(sys.argv[2]);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(('controlled '+name).encode())
print(json.dumps({'reason':'compiler-artifact','target':{'name':name,'kind':['example']},'executable':str(p),'fresh':False}))
print(json.dumps({'reason':'build-finished','success':True}))
'''


class RefinementBuildTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve();self.repo=self.root/'repo';self.repo.mkdir()
        self.before=Path.cwd();os.chdir(self.repo);self.addCleanup(os.chdir,self.before)
        self.context=paired.api(HERE.parents[2],COMMIT);self.build,self.hashes=self.context.__enter__()
        self.addCleanup(self.context.__exit__,None,None,None)
        self.source={'repo':str(self.repo),'commit':COMMIT,'tree_clean':True,'untracked_sources':[],
                     'files':{'fixture.rs':'fixture-source-not-a-real-build'}}
        self.calls=[]
    def environment(self,root,env):return {'tools':{'cargo':{'path':sys.executable}},'target':env['CARGO_TARGET_DIR']}
    def runner(self,command,**kwargs):
        name=command[command.index('--example')+1];self.calls.append(name)
        self.assertEqual(command[1:8],['build','--locked','--release','-p','geode-core','--example',name])
        target=Path(command[command.index('--target-dir')+1]);self.assertEqual(target,self.repo/'target')
        return self.build['owned_child'].wait_run([sys.executable,'-I','-c',COMPILER,name,str(target/'release/examples'/name)],**kwargs)
    def record(self,**overrides):
        args={'source_probe':lambda _:copy.deepcopy(self.source),'environment_probe':self.environment,'runner':self.runner}
        args.update(overrides)
        return paired._pair(self.root/'builds',self.repo,self.source,self.build,self.hashes,**args)
    def verify(self):
        root=self.root/'builds';metadata=json.loads((root/'executors.json').read_text())
        return paired._verify(root,metadata,self.build,self.hashes)
    def test_pair_uses_both_fixed_names_and_retains_independent_immutable_builds(self):
        receipt=self.record();self.assertEqual(self.calls,list(paired.EXAMPLES))
        self.assertFalse(receipt['can_launch']);proofs=self.verify()
        self.assertEqual(set(proofs),set(paired.EXAMPLES));self.assertNotEqual(*(p['binary_sha256'] for p in proofs.values()))
        self.assertEqual(self.build['NAME'],'icem_harness_eval')
        for name in paired.EXAMPLES:
            (self.repo/'target/release/examples'/name).write_bytes(b'later build')
        self.assertEqual(self.verify(),proofs)
        with self.assertRaises(FileExistsError):self.record()
    def test_second_compiler_failure_leaves_no_pair_success_and_preserves_first(self):
        def fail(command,**kwargs):
            if command[command.index('--example')+1]==paired.EXAMPLES[1]:
                return self.build['owned_child'].wait_run([sys.executable,'-I','-c','raise SystemExit(7)'],**kwargs)
            return self.runner(command,**kwargs)
        with self.assertRaisesRegex(ValueError,'exit successfully'):self.record(runner=fail)
        root=self.root/'builds'
        self.assertTrue((root/paired.EXAMPLES[0]/'build-receipt.json').exists())
        self.assertFalse((root/'executors.json').exists());self.assertTrue((root/'executors-failed.json').exists())
        self.assertEqual(self.build['NAME'],paired.EXAMPLES[0])
    def test_different_native_artifact_name_is_refused(self):
        def wrong(command,**kwargs):
            target=Path(command[command.index('--target-dir')+1])
            return self.build['owned_child'].wait_run([sys.executable,'-I','-c',COMPILER,'wrong_example',str(target/'release/examples/wrong_example')],**kwargs)
        with self.assertRaisesRegex(ValueError,'one successful evaluator artifact'):self.record(runner=wrong)
        self.assertFalse((self.root/'builds/executors.json').exists())
    def test_source_change_between_builds_is_not_a_coherent_pair(self):
        calls=0
        def probe(_):
            nonlocal calls
            calls+=1
            return dict(self.source,commit='b'*40) if calls>=3 else copy.deepcopy(self.source)
        with self.assertRaisesRegex(ValueError,'current clean|sources changed'):self.record(source_probe=probe)
        self.assertFalse((self.root/'builds/executors.json').exists())
    def test_changed_binary_is_refused(self):
        self.record();p=self.root/'builds'/paired.EXAMPLES[1]/paired.EXAMPLES[1]
        p.chmod(0o755);p.write_bytes(b'foreign binary')
        with self.assertRaisesRegex(ValueError,'census'):self.verify()
    def test_changed_build_identity_is_refused_even_with_new_fixture_census(self):
        self.record();root=self.root/'builds';path=root/'executors.json';m=json.loads(path.read_text())
        m['examples'].reverse()
        with self.assertRaisesRegex(ValueError,'identity'):paired._verify(root,m,self.build,self.hashes)
    def test_full_commit_source_verification_precedes_any_compiler(self):
        os.chdir(HERE.parents[2])
        try:
            with self.assertRaisesRegex(ValueError,'full object ID'):
                paired.record(HERE.parents[2],reviewed_commit='HEAD',out=self.root/'forbidden')
        finally:os.chdir(self.repo)
        self.assertFalse((self.root/'forbidden').exists());self.assertEqual(self.calls,[])


if __name__=='__main__':unittest.main(verbosity=2)
