"""Portable reboot export/replay with committed synthetic tool identities."""
import copy
import importlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE=Path(__file__).resolve().parent; REPO=HERE.parents[2]; sys.path.insert(0,str(HERE))
import capture_archive
import closed_capture
import producer_source
import reboot_attestation as proof
from test_capture_archive import fixture as study_fixture, PRODUCER
from test_reboot_attestation import fixture as proof_fixture, raw, utc, B0


def committed_test_tool(root):
    """New temporary Git commit, with read-only access to existing producer objects."""
    repo=root/'tool.git'; subprocess.run(['git','init','--bare','-q',str(repo)],check=True)
    objects=producer_source.git(REPO,'rev-parse','--path-format=absolute','--git-path','objects').decode().strip()
    (repo/'objects/info/alternates').write_text(objects+'\n')
    env=dict(os.environ,GIT_INDEX_FILE=str(root/'tool-index'))
    def git(*args,input=None):
        return subprocess.run(['git','-C',str(repo),*args],input=input,env=env,
                              capture_output=True,check=True).stdout.strip().decode()
    for name in (*capture_archive.TOOL_FILES,'reboot_attestation.py'):
        blob=git('hash-object','-w','--stdin',input=(HERE/name).read_bytes())
        git('update-index','--add','--cacheinfo','100644,'+blob+','+capture_archive.TOOL_DIRECTORY+name)
    tree=git('write-tree')
    commit=git('-c','user.name=Synthetic fixture','-c','user.email=fixture@invalid',
               '-c','commit.gpgSign=false','commit-tree',tree,input=b'Synthetic capture tool source, not production review\n')
    return repo,commit


def exported_fixture(base, *, alter_observation=None, prepared=None):
    p,w,files,observed=proof_fixture()
    owner={'host':p['host'],'platform':'linux','boot':B0}
    if prepared is None:
        root=base/'exp2';root.mkdir()
        f,frozen,verified,bindings=study_fixture(root,build_name='build-record',owner_host=owner,export=False)
    else:
        root=prepared['root'];frozen=prepared['frozen'];verified=prepared['verified'];bindings=prepared['bindings']
    journal=root/'main/events.jsonl';events=[json.loads(line) for line in journal.read_text().splitlines()]
    for event in events:event['time']=198.
    journal.write_bytes(b''.join(raw(e) for e in events))
    (root/'session_exit.txt').write_text('0 '+utc(199)+'\n')
    (root/'setup').mkdir();(root/'setup/retained.txt').write_text('synthetic setup evidence\n')
    (root/'setup/empty').mkdir()
    for path in root.rglob('*'):
        if path.is_file():os.utime(path,ns=(198_000_000_000,198_000_000_000))
    os.utime(root/'session_exit.txt',ns=(199_000_000_000,199_000_000_000))
    members,directories=proof.source_census(root)
    inventory=''.join(item['sha256']+'  '+root.name+'/'+name+'\n' for name,item in sorted(members.items())).encode()
    p['inventory_root']=str(root);p['inventory_sha256']=proof.sha(inventory)
    machine='c'*32+'\n';p['machine_id_sha256']=proof.sha(machine.encode())
    files['remote-sha256-before.txt']=files['remote-sha256-after.txt']=inventory
    host=json.loads(files['host-before-stop.json']);host['machine_id_sha256']=p['machine_id_sha256']
    for name in ('main','build-record'):host['records'][name]['journal_sha256']=members[name+'/events.jsonl']['sha256']
    files['host-before-stop.json']=raw(host)
    w['original_inventory_sha256']=p['inventory_sha256']
    w['files']={name:{'sha256':proof.sha(data),'bytes':len(data)} for name,data in files.items()}
    p['recovery_witness_sha256']=proof.sha(raw(w))
    evidence=base/'evidence';evidence.mkdir()
    for name,data in files.items():(evidence/name).write_bytes(data)
    (evidence/'recovery-witness.json').write_bytes(raw(w))
    (evidence/'policy.json').write_bytes(raw(p));policy_sha=proof.sha(raw(p))
    (evidence/'ec2.json').write_bytes(raw(observed['ec2']))
    observed.update(machine_id_text=machine,machine_id_encoding='file_bytes',
                    machine_id_sha256=p['machine_id_sha256'],observed_time=551.)
    calls=[]
    def observe(policy,cloud,identity,script):
        result=copy.deepcopy(observed)
        result['source_files'],result['source_directories']=proof.source_census(root)
        result['session_exit_text']=(root/'session_exit.txt').read_text()
        result['collector']['script']=str(script);result['collector']['script_sha256']=producer_source.digest(script)
        result['process_commands'][0]['argv'][3]=str(script)
        calls.append(True)
        if alter_observation:alter_observation(result,len(calls))
        return result
    context=proof.CaptureContext(evidence/'policy.json',policy_sha,evidence,evidence/'ec2.json',observe=observe)
    before=proof.source_census(root)
    with producer_source.committed_modules(REPO,PRODUCER,capture_archive.PROCESS_MODULES):
        verifier=importlib.import_module('process_verification')
        with mock.patch('time.time',return_value=551.):
            receipt=closed_capture._export(root/'main',root/'producer',base/'capture',frozen,verified,
                chain_probe=lambda source,registry:closed_capture._chain(source,registry,REPO,verified),
                binding_probe=lambda:copy.deepcopy(bindings),
                process_probe=lambda chain,build:context.processes(chain,build,verifier,HERE/'closed_capture.py'),
                reboot=context)
    assert proof.source_census(root)==before
    return receipt,policy_sha


class RebootCaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp=tempfile.TemporaryDirectory();cls.base=Path(cls.temp.name).resolve()
        cls.repo,cls.commit=committed_test_tool(cls.base)
        cls.receipt,cls.policy_sha=exported_fixture(cls.base)

    @classmethod
    def tearDownClass(cls):cls.temp.cleanup()

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve()/'portable';shutil.copytree(self.base/'capture',self.root)
        self.digest=self.receipt['sha256']

    def load(self,**kwargs):
        args={'manifest_sha256':self.digest,'git_repo':self.repo,'reviewed_commit':PRODUCER,
              'capture_tool_commit':self.commit,'reboot_policy_sha256':self.policy_sha}
        args.update(kwargs);return capture_archive.load(self.root,**args)

    def repin(self):
        # Test-only receipt after corruption, to exercise semantic replay gates.
        path=self.root/'capture.json';m=json.loads(path.read_bytes())
        census=closed_capture.census(self.root);census['files'].pop('capture.json')
        m['inventory']=census;path.write_bytes(raw(m));self.digest=proof.sha(path.read_bytes())

    def mutate(self,name,change):
        path=self.root/name;value=json.loads(path.read_bytes());change(value);path.write_bytes(raw(value));self.repin()

    def test_complete_portable_replay_retains_original_refusals_and_all_inventory_bytes(self):
        before={p.relative_to(self.root).as_posix():(p.read_bytes(),p.stat().st_mtime_ns)
                for p in self.root.rglob('*') if p.is_file()}
        result=self.load();self.assertTrue(result['capture_verified']);self.assertFalse(result['can_launch'])
        self.assertEqual(result['reboot_verification']['policy_sha256'],self.policy_sha)
        self.assertEqual(len(result['chain']['policy']['jobs']),100)
        self.assertEqual((self.root/'reboot-source/setup/retained.txt').read_text(),'synthetic setup evidence\n')
        self.assertTrue((self.root/'reboot-source/setup/empty').is_dir())
        reports=json.loads((self.root/'processes-before.json').read_bytes())['reports']
        self.assertTrue(all(not r['original_verifier']['processes_stopped'] for r in reports.values()))
        self.assertEqual(before,{p.relative_to(self.root).as_posix():(p.read_bytes(),p.stat().st_mtime_ns)
                                 for p in self.root.rglob('*') if p.is_file()})

    def test_new_same_boot_export_binds_every_imported_tool_source(self):
        base=Path(self.temp.name).resolve()/'same-boot';base.mkdir()
        receipt=study_fixture(base,exporter=closed_capture)
        result=capture_archive.load(base/'capture',manifest_sha256=receipt['sha256'],git_repo=self.repo,
                                    reviewed_commit=PRODUCER,capture_tool_commit=self.commit)
        self.assertEqual(result['manifest']['schema'],2)
        self.assertEqual(result['manifest']['process_mode'],'same_boot')
        self.assertIn('reboot_attestation.py',result['capture_tool_sha256'])
        self.assertNotIn('reboot_verification',result)

    def test_missing_or_wrong_external_policy_is_refused(self):
        for digest in (None,'e'*64):
            with self.subTest(digest=digest):
                with self.assertRaisesRegex(ValueError,'independently supplied'):self.load(reboot_policy_sha256=digest)

    def test_original_inventory_missing_extra_and_changed_files_refused_after_repin(self):
        path=self.root/'reboot-source/setup/retained.txt';path.write_text('changed');self.repin()
        with self.assertRaisesRegex(ValueError,'bytes/size/mode'):self.load()
        path.unlink();self.repin()
        with self.assertRaisesRegex(ValueError,'missing or extra'):self.load()

    def test_extra_original_tree_member_is_refused(self):
        (self.root/'study/main/extra').write_text('unobserved');self.repin()
        with self.assertRaisesRegex(ValueError,'missing or extra'):self.load()

    def test_extra_empty_original_directory_is_refused(self):
        (self.root/'study/main/unobserved').mkdir();self.repin()
        with self.assertRaisesRegex(ValueError,'directory census'):self.load()

    def test_retained_original_refusal_timestamp_cannot_be_rewritten(self):
        def change(evidence):
            next(iter(evidence['reports'].values()))['original_verifier']['observed_time']=0
        self.mutate('processes-before.json',change)
        with self.assertRaisesRegex(ValueError,'source/time identity'):self.load()

    def test_runtime_binding_drift_still_refuses_in_reboot_mode(self):
        self.mutate('producer-after.json',lambda value:value['observed']['runtime'].update(changed=True))
        with self.assertRaisesRegex(ValueError,'runtime/build/input bindings'):self.load()

    def test_reboot_helper_source_binding_is_mandatory(self):
        self.mutate('capture.json',lambda m:m['capture_tool_sha256'].pop('reboot_attestation.py'))
        with self.assertRaisesRegex(ValueError,'reviewed revision'):self.load()

    def test_process_reports_cannot_omit_build_or_replace_original_refusal(self):
        evidence=json.loads((self.root/'processes-before.json').read_bytes())
        build=next(k for k in evidence['reports'] if k.endswith('/build-record'))
        self.mutate('processes-before.json',lambda m:m['reports'].pop(build))
        with self.assertRaisesRegex(ValueError,'generation or build'):self.load()

    def test_changed_proof_between_export_phases_refuses(self):
        base=Path(self.temp.name).resolve()/'changed';base.mkdir()
        def change(observed,phase):
            if phase==2:observed['source_files']['setup/retained.txt']['mtime_ns']-=1
        with self.assertRaisesRegex(ValueError,'proof changed during capture'):
            exported_fixture(base,alter_observation=change)
        self.assertTrue((base/'capture/capture-failed.json').is_file())
        self.assertFalse((base/'capture/capture.json').exists())


if __name__=='__main__':unittest.main()
