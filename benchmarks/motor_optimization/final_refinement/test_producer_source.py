"""Trusted-Git source binding and source-only import tests; no FEM or main data."""
import copy
import importlib
import importlib.util
import os
from pathlib import Path
import py_compile
import subprocess
import sys
import tempfile
import unittest

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import producer_source as source


class SourceFixture:
    def __init__(self,root):
        root=root.resolve();self.root=root;root.mkdir()
        self.file=root/source.HARNESS/'capture_fixture_module.py';self.file.parent.mkdir(parents=True)
        self.file.write_text('ANSWER = 42\n')
        (root/'Cargo.toml').write_text('[workspace]\n')
        self.run('init','-q');self.run('add','.')
        self.run('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','Fixture')
        self.commit=self.run('rev-parse','HEAD').strip()
        self.expected={'repo':str(root),'commit':self.commit,'tree_clean':True,'untracked_sources':[],
                       'files':source.committed_files(root,self.commit)}

    def run(self,*args):
        return subprocess.check_output(['git','-C',str(self.root),*args],text=True,stderr=subprocess.PIPE)


class ProducerSourceTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.fixture=SourceFixture(Path(self.temp.name)/'repo');self.f=self.fixture

    def verify(self,expected=None,**kwargs):
        return source.verify(self.f.root,self.f.expected if expected is None else expected,
                             reviewed_commit=self.f.commit,**kwargs)

    def test_complete_git_map_and_live_bytes_are_required(self):
        verified=self.verify();self.assertEqual(verified['files'],self.f.expected['files'])
        self.f.file.write_text('ANSWER = 43\n')
        with self.assertRaises(ValueError):self.verify()
        # An offline consumer can authenticate the recorded map against Git even
        # while its own checkout is at another state.
        self.assertFalse(self.verify(check_worktree=False)['worktree_checked'])

    def test_self_declared_hash_or_commit_cannot_replace_trusted_git(self):
        for mutate in (lambda x:x['files'].pop('Cargo.toml'),
                       lambda x:x['files'].update({'Cargo.toml':'a'*64}),
                       lambda x:x.update(commit='a'*40),
                       lambda x:x.update(tree_clean=False),
                       lambda x:x.update(untracked_sources=['extra.py'])):
            bad=copy.deepcopy(self.f.expected);mutate(bad)
            with self.assertRaises(ValueError):self.verify(bad)
        with self.assertRaises(ValueError):source.committed_files(self.f.root,'HEAD')

    def test_untracked_code_is_refused_but_new_data_is_not_source(self):
        extra=self.f.file.with_name('extra.py');extra.write_text('pass\n')
        with self.assertRaisesRegex(ValueError,'untracked'):self.verify()
        extra.unlink();extra=self.f.file.with_name('data.json');extra.write_text('{}')
        self.verify()

    def test_staged_file_census_or_redirected_sources_are_refused(self):
        extra=self.f.file.with_name('extra.py');extra.write_text('pass\n');self.f.run('add',str(extra))
        with self.assertRaisesRegex(ValueError,'index'):self.verify()
        self.f.run('reset','-q','HEAD','--',str(extra));extra.unlink()
        original=self.f.file.read_bytes();self.f.file.unlink()
        target=Path(self.temp.name)/'outside';target.write_bytes(original);self.f.file.symlink_to(target)
        with self.assertRaises(ValueError):self.verify()

    def test_source_loader_ignores_a_timestamp_valid_stale_bytecode_cache(self):
        path=self.f.file;before=path.stat()
        path.write_text('ANSWER = 99\n');os.utime(path,ns=(before.st_atime_ns,before.st_mtime_ns))
        py_compile.compile(str(path),doraise=True)
        path.write_text('ANSWER = 42\n');os.utime(path,ns=(before.st_atime_ns,before.st_mtime_ns))
        # Equal size and timestamps mean ordinary timestamp invalidation cannot
        # distinguish these sources; the verified loader compiles retained bytes.
        verified=self.verify()
        with source.modules(self.f.root,verified):
            m=importlib.import_module('capture_fixture_module')
            self.assertEqual(m.ANSWER,42)
        self.assertNotIn('capture_fixture_module',sys.modules)

    def test_loaded_producer_module_or_changed_source_is_not_reused(self):
        verified=self.verify()
        sys.modules['capture_fixture_module']=object()
        try:
            with self.assertRaisesRegex(ValueError,'fresh'):
                with source.modules(self.f.root,verified):pass
        finally:sys.modules.pop('capture_fixture_module')
        with self.assertRaisesRegex(ValueError,'during verification'):
            with source.modules(self.f.root,verified):self.f.file.write_text('ANSWER = 43\n')

    def test_offline_module_replay_uses_git_not_changed_worktree(self):
        self.f.file.write_text('ANSWER = 99\n')
        with source.committed_modules(self.f.root,self.f.commit,['capture_fixture_module']):
            self.assertEqual(importlib.import_module('capture_fixture_module').ANSWER,42)


if __name__=='__main__':unittest.main(verbosity=2)
