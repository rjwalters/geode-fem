"""On-disk recovery-chain fixtures with the actual hash-pinned policy module."""
import argparse
from contextlib import ExitStack
import copy
import fcntl
import hashlib
import importlib.util
import marshal
import shutil
import struct
import subprocess
import json
from pathlib import Path
import sys
import unittest
from unittest import mock
sys.path.insert(0,str(Path(__file__).resolve().parent))
import recovery_evidence as recovery
from trace_archive import Archive,load_generation
from test_trace_archive import Fixture,write,digest
import tempfile

HARNESS=Path(__file__).resolve().parent.parent/'harness'


def finish(fixture,complete):
    write(fixture.root/'manifest.json',{'complete':complete})
    fixture.emit('session_finished',manifest_sha256=digest(fixture.root/'manifest.json'),complete=complete)


def configure(fixture):
    fixture.bindings['recovery_registry']=fixture.bindings['session_directory']+'.recoveries'
    fixture.rebind('session.json')


class RegistryFixture:
    def __init__(self,original,hashes):
        self.original=original;self.hashes=hashes;self.root=original.root.with_name(original.root.name+'.recoveries')
        self.root.mkdir();(self.root/'owner.lock').write_text('')
        self.canonical=original.bindings['recovery_registry'];self.generations=[];self.events=[]
        origin={'schema':1,'source':original.bindings['session_directory'],
                'journal_sha256':digest(original.root/'events.jsonl'),'bindings_sha256':digest(original.root/'bindings.json')}
        write(self.root/'origin.json',origin);self.emit('registry_created',origin_sha256=digest(self.root/'origin.json'))

    def emit(self,kind,**fields):
        self.events.append(dict(sequence=len(self.events),kind=kind,**fields));self.save()

    def save(self):
        (self.root/'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in self.events))

    def policy(self,external=None):
        sources=[self.original.bindings['session_directory']]+[self.canonical+f'/generation-{i+1:04d}' for i in range(len(self.generations))]
        inventories=[];repairs=[]
        with ExitStack() as stack:
            readers=[]
            for f,source in zip([self.original,*self.generations],sources):
                generation=load_generation(f.root)
                inventories.append(recovery._inventory(generation,source,Archive(f.root)))
                repairs.extend(recovery._diagnostics(generation,source,readers,stack))
            return recovery._policy(inventories,repairs,external or {},HARNESS,self.hashes)

    def add(self,plan):
        index=len(self.generations)+1;name=f'generation-{index:04d}'
        prior=self.generations[-1] if self.generations else self.original
        previous=self.canonical+f'/generation-{index-1:04d}' if self.generations else self.original.bindings['session_directory']
        filename=f'plan-{index:04d}.json';write(self.root/filename,plan)
        bound={'file':filename,'sha256':digest(self.root/filename)}
        self.emit('generation_reserved',index=index,directory=name,previous=previous,
                  previous_journal_sha256=digest(prior.root/'events.jsonl'),plan=bound)
        f=Fixture(self.root/name)
        f.bindings=copy.deepcopy(self.original.bindings)
        f.bindings['recovery']={'registry':self.canonical,'original':self.original.bindings['session_directory'],
                               'index':index,'previous':previous,'plan':bound}
        write(f.root/'bindings.json',f.bindings)
        f.events[1]['bindings_sha256']=digest(f.root/'bindings.json');f.save()
        self.generations.append(f)
        return f

    def run(self,f,index,recovery_reason,**kwargs):
        name=f.add(index,**kwargs)
        created=next(e for e in reversed(f.events) if e['kind']=='attempt_created')
        created['recovery_reason']=recovery_reason;f.save();return name

    def seal(self,f,complete):
        index=self.generations.index(f)+1
        write(f.root/'manifest.json',{'complete':complete})
        self.emit('generation_finished',index=index,journal_sha256=digest(f.root/'events.jsonl'),
                  bindings_sha256=digest(f.root/'bindings.json'),
                  summary={'file':'manifest.json','sha256':digest(f.root/'manifest.json')})

    def external(self,attempt_id):
        folder=self.root/'external-evidence';folder.mkdir(exist_ok=True)
        payload=b'controlled external-stop evidence, synthetic fixture'
        value=hashlib.sha256(payload).hexdigest();(folder/(value+'.bin')).write_bytes(payload)
        key=f"{self.original.bindings['session_directory']}#{attempt_id}"
        original={'attempt':key,'verified_external_interruption':True,'event_kind':'operator_stop',
            'reason':'fixture','verified_by':'fixture reviewer','verification_reference':'fixture review',
            'event_id':'fixture event','evidence':[{'path':self.canonical+'/external-evidence/'+value+'.bin','sha256':value}]}
        mapped=copy.deepcopy(original);mapped['evidence'][0]['path']=str(folder/(value+'.bin'))
        return {key:original},{key:mapped}


class RecoveryEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not (HARNESS/'recovery_policy.py').exists():
            raise RuntimeError('supply --harness pointing at the reviewed candidate until dependencies merge')
        cls.hashes={n:digest(HARNESS/n) for n in ('recovery_policy.py','session_journal.py','process_identity.py')}

    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.f=Fixture(Path(self.temp.name)/'main');configure(self.f)

    def read(self,registry=None,**extra):
        return recovery.reconcile(self.f.root,registry=registry.root if registry else None,
                                  harness=HARNESS,policy_source_sha256=self.hashes,**extra)

    def populate(self,*,skip=(),contaminated=()):
        for index in range(100):
            if index not in skip:self.f.add(index,contaminated=index in contaminated)

    def test_complete_hundred_job_scientific_failures_are_preserved_without_writes(self):
        self.populate();finish(self.f,True)
        before={p.name:(p.read_bytes(),p.stat().st_mtime_ns) for p in self.f.root.iterdir()}
        out=self.read()
        self.assertTrue(out['all_registered_jobs_resolved']);self.assertFalse(out['can_select'])
        self.assertTrue(all(r['selected_completion'] for r in out['policy']['jobs']))
        self.assertEqual(out['policy']['jobs'][0]['attempts'][0]['outcome']['stop_reason'],'error')
        self.assertEqual(before,{p.name:(p.read_bytes(),p.stat().st_mtime_ns) for p in self.f.root.iterdir()})
        self.assertFalse(self.f.root.with_name('main.recoveries').exists())

    def test_first_clean_completion_within_original_generation_is_preserved(self):
        self.populate()
        first=next(e['sequence'] for e in self.f.events if e['kind']=='attempt_created')
        later=self.f.add(0,reason='converged')
        self.f.replace_trace(later,lambda raw:raw.update(validations=[{'index':0,'F_val':0.0}]))
        finish(self.f,True)
        row=self.read()['policy']['jobs'][0]
        self.assertEqual(len(row['attempts']),2)
        self.assertEqual(row['selected_completion']['attempt_id'],first)
        self.assertEqual(row['attempts'][0]['outcome']['stop_reason'],'error')

    def test_trace_and_parent_host_verdict_must_agree(self):
        name=self.f.add()
        self.f.replace_trace(name,lambda raw:raw.update(host_contaminated=True))
        with self.assertRaisesRegex(ValueError,'host verdict'):self.read()

    def test_missing_jobs_and_unfinalized_completed_optimizer_stay_unresolved(self):
        self.f.add(0,terminal=False);finish(self.f,False)
        out=self.read()
        self.assertFalse(out['all_registered_jobs_resolved'])
        self.assertEqual(out['policy']['jobs'][0]['disposition'],'unresolved')
        self.assertEqual(len(out['policy']['remaining_registered']),99)
        self.assertEqual(out['policy']['jobs'][0]['attempts'][0]['unfinalized_trace']['optimizer_completed'],True)

    def test_clean_contamination_replacement_uses_one_allowance_and_keeps_both_attempts(self):
        self.populate(contaminated=(0,));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes);plan=reg.policy();g=reg.add(plan)
        reg.run(g,0,'contamination');reg.seal(g,True)
        out=self.read(reg);row=out['policy']['jobs'][0]
        self.assertTrue(out['all_registered_jobs_resolved']);self.assertEqual(len(row['attempts']),2)
        self.assertEqual(row['contamination_reruns_used'],1)
        self.assertEqual(row['selected_completion']['source'],reg.canonical+'/generation-0001')
        self.assertTrue(row['attempts'][0]['finished']['result']['host_contaminated'])

    def test_exhausted_contamination_replacement_does_not_reset_in_another_generation(self):
        self.populate(contaminated=(0,));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes);g=reg.add(reg.policy())
        reg.run(g,0,'contamination',contaminated=True);reg.seal(g,False)
        out=self.read(reg)
        self.assertEqual(out['policy']['jobs'][0]['disposition'],'unresolved')
        self.assertEqual(out['policy']['jobs'][0]['contamination_reruns_used'],1)
        forged=reg.policy();forged['recovery_tail']=[{'job':self.f.queue[0],'reason':'contamination'}]
        reg.add(forged)
        with self.assertRaisesRegex(ValueError,'reserved plan'):self.read(reg)

    def test_external_replacement_remaps_bound_evidence_without_editing_it(self):
        self.populate(skip=(0,));self.f.add(0,complete=False);finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes)
        aid=next(e['sequence'] for e in reversed(self.f.events) if e['kind']=='attempt_created')
        original,mapped=reg.external(aid);plan=reg.policy(mapped);plan['external_events']=original
        g=reg.add(plan);reg.run(g,0,'external_interruption');reg.seal(g,True)
        before=(reg.root/'plan-0001.json').read_bytes()
        out=self.read(reg)
        self.assertTrue(out['all_registered_jobs_resolved'])
        self.assertEqual(out['policy']['jobs'][0]['external_replacements_used'],1)
        self.assertEqual(out['original_external_events'],original)
        self.assertEqual((reg.root/'plan-0001.json').read_bytes(),before)
        evidence=next((reg.root/'external-evidence').iterdir());evidence.write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError,'changed bound'):self.read(reg)

    def test_first_clean_scientific_failure_cannot_be_replaced_by_a_better_recovery(self):
        self.populate(skip=(1,));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes);g=reg.add(reg.policy())
        reg.run(g,0,'remaining_registered',reason='converged');reg.seal(g,False)
        with self.assertRaisesRegex(ValueError,'not authorized'):self.read(reg)

    def test_active_registry_omitted_registry_and_unsealed_generation_are_explicit(self):
        self.populate(skip=(0,));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes)
        with self.assertRaisesRegex(ValueError,'omitted'):self.read()
        with (reg.root/'owner.lock').open('rb') as stream:
            fcntl.flock(stream,fcntl.LOCK_EX|fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError,'active'):self.read(reg)
        g=reg.add(reg.policy());reg.run(g,0,'remaining_registered');finish(g,True)
        out=self.read(reg)
        self.assertIsNotNone(out['policy']['jobs'][0]['selected_completion'])
        self.assertFalse(out['all_registered_jobs_resolved'])

    def test_changed_origin_prior_journal_reserved_plan_and_seal_are_refused(self):
        self.populate(skip=(0,));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes);g=reg.add(reg.policy());reg.run(g,0,'remaining_registered');reg.seal(g,True)
        for path in (self.f.root/'events.jsonl',reg.root/'origin.json',reg.root/'plan-0001.json',g.root/'manifest.json'):
            with self.subTest(path=path.name):
                before=path.read_bytes();path.write_bytes(before+b' ')
                with self.assertRaises(ValueError):self.read(reg)
                path.write_bytes(before)
        reg.events[-1]['journal_sha256']='0'*64;reg.save()
        with self.assertRaisesRegex(ValueError,'sealed'):self.read(reg)

    def test_missing_unregistered_or_redirected_generation_refuses(self):
        self.populate(skip=(0,));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes);g=reg.add(reg.policy())
        extra=reg.root/'generation-9999';extra.mkdir()
        with self.assertRaisesRegex(ValueError,'unregistered'):self.read(reg)
        extra.rmdir()
        moved=Path(self.temp.name)/'moved';g.root.rename(moved);g.root.symlink_to(moved,target_is_directory=True)
        with self.assertRaisesRegex(ValueError,'redirected'):self.read(reg)

    def test_changed_frozen_inputs_and_failed_dispatch_verification_refuse(self):
        self.populate(skip=(0,));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes);g=reg.add(reg.policy())
        g.bindings['unit_seconds']=3.;g.rebind('session.json')
        with self.assertRaises(ValueError):self.read(reg)
        g.bindings['unit_seconds']=2.;g.rebind('session.json')
        g.emit('recovery_verification_failed',error='fixture mismatch')
        with self.assertRaisesRegex(ValueError,'failed dispatch'):self.read(reg)

    def test_policy_source_is_pinned_before_any_decision(self):
        bad=dict(self.hashes);bad['recovery_policy.py']='0'*64
        with mock.patch.object(recovery.subprocess,'run',side_effect=AssertionError('unpinned source executed')):
            with self.assertRaisesRegex(ValueError,'source changed'):
                recovery.reconcile(self.f.root,registry=None,harness=HARNESS,policy_source_sha256=bad)

    def diagnostic_fixture(self):
        self.populate()
        # Locate method D, original queue position 3, independently of filename order.
        created=next(e for e in self.f.events if e['kind']=='attempt_created' and e['job']['position']==3)
        dispatched=next(e for e in self.f.events if e['kind']=='attempt_dispatched' and e['attempt_id']==created['sequence'])
        name=dispatched['trace']
        raw=json.loads((self.f.root/name).read_text());raw.update(diagnostic_status='pending',diagnostic_state={'fixture':'stopped'})
        write(self.f.root/'stopped.json',raw);binding={'file':'stopped.json','sha256':digest(self.f.root/'stopped.json')}
        raw['optimizer_snapshot']=binding
        self.f.replace_trace(name,lambda r:r.update(raw));finish(self.f,False)
        reg=RegistryFixture(self.f,self.hashes);plan=reg.policy()
        self.assertEqual(len(plan['diagnostics']),1);g=reg.add(plan);action=plan['diagnostics'][0]
        aid=g.emit('diagnostic_attempt_created',action=action,directory='diagnostic-fixture')
        g.emit('diagnostic_attempt_dispatched',attempt_id=aid)
        worker=g.root/'diagnostic-fixture';worker.mkdir();(worker/'owner.lock').write_text('')
        key=reg.canonical+'/generation-0001#diagnostic-'+str(aid)
        inputs={'session':g.session,'attempt_id':key,'snapshot':{'path':'/cloud/experiment/main/stopped.json','sha256':binding['sha256']},
                'job':{k:action['job'][k] for k in ('method','space','start','repeat')}}
        write(worker/'inputs.json',inputs)
        terminal={'kind':'diagnostic_only','attempt_id':key,'status':'failed','optimizer_reexecuted':False,
                  'counted_in_run_budget':False,'inputs':{'file':'inputs.json','sha256':digest(worker/'inputs.json')}}
        write(worker/'terminal.json',terminal)
        events=[{'sequence':0,'kind':'session_created'},{'sequence':1,'kind':'diagnostic_finished',
                 'result':{'file':'terminal.json','sha256':digest(worker/'terminal.json')}}]
        (worker/'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
        g.emit('diagnostic_attempt_finished',attempt_id=aid,result={'terminal':{'file':'diagnostic-fixture/terminal.json',
                   'sha256':digest(worker/'terminal.json')}});reg.seal(g,True)
        return reg, g, worker, action, inputs, terminal, events

    def test_diagnostic_repair_does_not_add_or_replace_an_optimizer(self):
        reg, g, worker, action, inputs, terminal, events = self.diagnostic_fixture()
        out=self.read(reg);row=out['policy']['jobs'][3]
        self.assertTrue(out['all_registered_jobs_resolved']);self.assertEqual(len(row['attempts']),1)
        self.assertEqual(row['selected_completion']['source'],self.f.bindings['session_directory'])
        self.assertEqual(row['selected_diagnostic']['status'],'failed')
        (self.f.root/'stopped.json').write_text('{}')
        with self.assertRaisesRegex(ValueError,'changed bound'):self.read(reg)

    def test_production_registry_and_inventory_match_reader_policy(self):
        from recovery_schema_probe import probe
        summary=probe(Path(self.temp.name)/'production-probe',HARNESS)
        self.assertTrue(summary['policy_matches_production'])
        self.assertTrue(summary['archive_bytes_and_mtimes_unchanged'])
        self.assertEqual(summary['retained_attempts'],101)
        self.assertEqual(summary['selected_completions'],100)
        self.assertEqual(summary['replacement_job_count'],1)
        self.assertFalse(summary['can_select'])

    def test_pinned_source_execution_ignores_poisoned_bytecode_cache(self):
        private=Path(self.temp.name)/'policy';private.mkdir()
        for name in self.hashes:shutil.copyfile(HARNESS/name,private/name)
        source=private/'recovery_policy.py'
        cache=Path(importlib.util.cache_from_source(str(source)));cache.parent.mkdir()
        stamp=source.stat()
        header=importlib.util.MAGIC_NUMBER+struct.pack('<III',0,int(stamp.st_mtime),stamp.st_size)
        cache.write_bytes(header+marshal.dumps(compile("raise RuntimeError('poisoned cache fixture')",str(source),'exec')))
        ordinary=subprocess.run([sys.executable,'-I','-B','-c',
            "import sys;sys.path.insert(0,sys.argv[1]);import recovery_policy",str(private)],
            text=True,capture_output=True,timeout=60)
        self.assertNotEqual(ordinary.returncode,0)
        self.assertIn('poisoned cache fixture',ordinary.stderr)
        before=cache.read_bytes()
        out=recovery.reconcile(self.f.root,registry=None,harness=private,policy_source_sha256=self.hashes)
        self.assertEqual(len(out['policy']['remaining_registered']),100)
        self.assertEqual(cache.read_bytes(),before)

    def test_diagnostic_cannot_swap_full_optimizer_binding_or_snapshot_path(self):
        reg,g,worker,action,inputs,terminal,events=self.diagnostic_fixture()
        # Preserve the optimizer key but change a different part of its binding.
        created=next(e for e in g.events if e['kind']=='diagnostic_attempt_created')
        created['action']['optimizer']['trace_sha256']='9'*64
        g.save()
        reg.events[-1]['journal_sha256']=digest(g.root/'events.jsonl');reg.save()
        with self.assertRaisesRegex(ValueError,'not authorized'):self.read(reg)
        # Restore the action from the immutable reserved plan.
        created['action']=json.loads((reg.root/'plan-0001.json').read_text())['diagnostics'][0]
        g.save();reg.events[-1]['journal_sha256']=digest(g.root/'events.jsonl');reg.save()
        inputs['snapshot']['path']='/different/stopped.json'
        write(worker/'inputs.json',inputs)
        terminal['inputs']['sha256']=digest(worker/'inputs.json');write(worker/'terminal.json',terminal)
        events[-1]['result']['sha256']=digest(worker/'terminal.json')
        (worker/'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
        ended=next(e for e in g.events if e['kind']=='diagnostic_attempt_finished')
        ended['result']['terminal']['sha256']=digest(worker/'terminal.json')
        g.save();reg.events[-1]['journal_sha256']=digest(g.root/'events.jsonl');reg.save()
        with self.assertRaisesRegex(ValueError,'stopped optimizer/session binding'):self.read(reg)


if __name__=='__main__':
    parser=argparse.ArgumentParser(add_help=False);parser.add_argument('--harness',type=Path,default=HARNESS)
    args,rest=parser.parse_known_args();HARNESS=args.harness.resolve()
    unittest.main(argv=[sys.argv[0],*rest],verbosity=2)
