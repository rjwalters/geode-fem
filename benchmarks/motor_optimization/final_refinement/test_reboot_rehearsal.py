"""Report-only rehearsal, redaction and fresh-volume launch-plan regressions."""
import copy
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0,str(Path(__file__).resolve().parent))
import quiet_rehearsal as rehearsal
import reboot_attestation as proof
import closed_capture
from test_reboot_attestation import fixture
from test_reboot_quiet import process


class RehearsalTests(unittest.TestCase):
    def test_plan_matches_source_ami_type_and_never_reuses_study_volume(self):
        instance={'InstanceId':'i-study','ImageId':'ami-123','InstanceType':'r8g.4xlarge','SubnetId':'subnet-123',
                  'SecurityGroups':[{'GroupId':'sg-123'}],
                  'BlockDeviceMappings':[{'DeviceName':'/dev/sda1','Ebs':{'VolumeId':'vol-study'}}]}
        image={'ImageId':'ami-123','RootDeviceName':'/dev/sda1','BlockDeviceMappings':[
            {'DeviceName':'/dev/sda1','Ebs':{'SnapshotId':'snap-abc','VolumeSize':8}}]}
        plan=rehearsal.launch_plan(instance,image,'paper3-rehearsal-test')
        self.assertEqual(plan['ImageId'],instance['ImageId'])
        self.assertEqual(plan['InstanceType'],instance['InstanceType'])
        self.assertEqual((plan['MinCount'],plan['MaxCount']),(1,1))
        self.assertEqual(plan['InstanceInitiatedShutdownBehavior'],'terminate')
        self.assertEqual({r['ResourceType'] for r in plan['TagSpecifications']},{'instance','volume'})
        for row in plan['TagSpecifications']:
            self.assertIn({'Key':'project','Value':'geode-fem'},row['Tags'])
        self.assertEqual(plan['BlockDeviceMappings'],[{'DeviceName':'/dev/sda1','Ebs':{
            'SnapshotId':'snap-abc','VolumeSize':8,'VolumeType':'gp3','DeleteOnTermination':True}}])
        self.assertNotIn('vol-study',json.dumps(plan))
        self.assertNotIn('i-study',json.dumps(plan))
        self.assertNotIn('IamInstanceProfile',plan)
        # The actual noble AMI also advertises ephemeral0/1; suppress them.
        extended=copy.deepcopy(image)
        extended['BlockDeviceMappings'] += [{'DeviceName':'/dev/sdb','VirtualName':'ephemeral0'},
                                           {'DeviceName':'/dev/sdc','VirtualName':'ephemeral1'}]
        with_ephemeral=rehearsal.launch_plan(instance,extended,'paper3-rehearsal-test')
        self.assertEqual(with_ephemeral['BlockDeviceMappings'],plan['BlockDeviceMappings']+
                         [{'DeviceName':'/dev/sdb','NoDevice':''},{'DeviceName':'/dev/sdc','NoDevice':''}])
        for change in (lambda i:i.update(ImageId='ami-other'),
                       lambda i:i['BlockDeviceMappings'].append({'DeviceName':'/dev/sdb'})):
            bad=copy.deepcopy(image);change(bad)
            with self.assertRaises(ValueError):rehearsal.launch_plan(instance,bad,'paper3-rehearsal-test')
        with self.assertRaisesRegex(ValueError,'unique rehearsal'):
            rehearsal.launch_plan(instance,image,'exp2main')

    def test_classification_precedes_redaction(self):
        observed=fixture()[3]
        argv=['@dbus-daemon','--system','--address=systemd:','--nofork',
              '--nopidfile','--systemd-activation','--syslog-only']
        process(observed,'dbus-daemon',argv,uid=101)
        process(observed,'worker',['/home/private/python3','/home/private/task.py','--seed=42'],pid=701,uid=1000,parent=9876)
        rows=rehearsal.classifications(observed,'d'*64)
        daemon=next(r for r in rows if r['comm']=='dbus-daemon')
        self.assertEqual(daemon,{'comm':'dbus-daemon','argv_shape':argv,'ppid':1,
                                 'uid_class':'system','verdict':'system_daemon'})
        worker=next(r for r in rows if r['comm']=='worker')
        self.assertEqual(worker['verdict'],'refuse');self.assertEqual(worker['uid_class'],'user')
        self.assertEqual(worker['argv_shape'],['python3','<arg>','<arg>'])
        self.assertEqual(worker['ppid'],'other')
        self.assertNotIn('private',json.dumps(rows));self.assertNotIn('secret-session-id',json.dumps(rows))

    def test_collect_uses_actual_snapshot_and_quiet_check(self):
        observed=fixture()[3]
        with mock.patch.object(proof,'process_snapshot',return_value=observed) as snapshot, \
             mock.patch.object(proof.producer_source,'digest',return_value='d'*64):
            def bounded(observe,digest,log):
                return proof.wait_for_quiet(observe,digest,clock=lambda:0,log=log)
            result=rehearsal.collect('identity',Path('/tools/closed_capture.py'),wait=bounded)
        snapshot.assert_called_once_with('identity',Path('/tools/closed_capture.py'))
        self.assertTrue(result['clean_pass']);self.assertEqual(len(result['attempts']),1)
        self.assertEqual(result['attempts'][0]['processes'][0]['verdict'],'collector')

    def test_refused_rehearsal_retains_classification_without_private_argv(self):
        observed=fixture()[3];process(observed,'python3',['python3','private-session-token'])
        def once(observe,digest,log):
            item=observe();proof.quiet_processes(item,capture_script_sha256=digest)
        with mock.patch.object(proof,'process_snapshot',return_value=observed), \
             mock.patch.object(proof.producer_source,'digest',return_value='d'*64):
            result=rehearsal.collect('identity',Path('/tools/closed_capture.py'),wait=once)
        self.assertFalse(result['clean_pass'])
        self.assertIn('refuse',[r['verdict'] for r in result['attempts'][0]['processes']])
        self.assertNotIn('private-session-token',json.dumps(result))

    def test_boot_wait_observes_services_without_stopping_them(self):
        calls=[];sleeps=[];answers=iter(['active','inactive','inactive','inactive'])
        def run(argv,**kwargs):
            calls.append(argv)
            return SimpleNamespace(returncode=0,stdout=(next(answers).encode() if argv[0]=='systemctl' else b''))
        rehearsal.wait_boot(run=run,sleep=sleeps.append)
        self.assertEqual(calls[0],['cloud-init','status','--wait'])
        self.assertTrue(all(c[:2]==['systemctl','show'] for c in calls[1:]))
        self.assertEqual(sleeps,[10])
        with self.assertRaisesRegex(ValueError,'cloud-init'):
            rehearsal.wait_boot(run=lambda *a,**kw:SimpleNamespace(returncode=1),sleep=lambda _:None)
        def failed(argv,**kwargs):
            return SimpleNamespace(returncode=0,stdout=b'failed')
        with self.assertRaisesRegex(ValueError,'failed or has unknown state'):
            rehearsal.wait_boot(run=failed,sleep=lambda _:None)

    def test_identity_hash_is_checked_before_loading_or_collecting(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'process_identity.py').write_text('raise AssertionError("untrusted code executed")')
            with mock.patch.object(rehearsal,'HERE',root), mock.patch.object(sys,'platform','linux'), \
                 mock.patch.object(sys,'argv',['capture','--quiet-rehearsal','--identity-sha256','0'*64,'--out',str(root/'report')]), \
                 mock.patch.object(rehearsal,'wait_boot') as boot, mock.patch.object(rehearsal,'collect') as collect:
                with self.assertRaisesRegex(ValueError,'identity source changed'):
                    rehearsal.collect_main(root/'closed_capture.py')
                boot.assert_not_called();collect.assert_not_called()

    def test_report_created_during_collection_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();raw=b'# synthetic identity\n';(root/'process_identity.py').write_bytes(raw)
            out=root/'report'
            def race(*args):
                out.write_text('concurrent evidence');return {'clean_pass':True}
            with mock.patch.object(rehearsal,'HERE',root), mock.patch.object(sys,'platform','linux'), \
                 mock.patch.object(sys,'argv',['capture','--quiet-rehearsal','--identity-sha256',hashlib.sha256(raw).hexdigest(),'--out',str(out)]), \
                 mock.patch.object(rehearsal,'wait_boot'), mock.patch.object(rehearsal,'collect',side_effect=race):
                with self.assertRaises(FileExistsError):rehearsal.collect_main(root/'closed_capture.py')
            self.assertEqual(out.read_text(),'concurrent evidence')

    def test_dirty_checkout_cannot_prepare(self):
        def git_response(repo,*args):
            if args[0]=='status':return b' M dirty.py\n'
            if args[0]=='rev-parse':return ('b'*40+'\n').encode()
            return b'# synthetic committed file\n'
        instance={'InstanceId':'i-study','BlockDeviceMappings':[],
                  'ImageId':'ami-123','InstanceType':'c7a.4xlarge'}
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(rehearsal,'launch_plan',return_value={}), \
             mock.patch.object(proof.producer_source,'git',side_effect=git_response) as git:
            out=Path(tmp)/'package'
            with self.assertRaisesRegex(ValueError,'checkout must be clean'):
                rehearsal.prepare(instance, {}, 'paper3-rehearsal-test',Path(tmp),'a'*40,out)
            self.assertFalse(out.exists())
            git.assert_called_once_with(Path(tmp),'status','--porcelain')

    def test_normal_capture_cli_does_not_dispatch_rehearsal(self):
        argv=['capture','--source','/source','--repo','/repo','--out','/out','--reviewed-commit','a'*40]
        with mock.patch.object(sys,'argv',argv), mock.patch.object(closed_capture,'capture',return_value={}) as capture, \
             mock.patch.object(Path,'read_bytes',side_effect=AssertionError('rehearsal dispatched')), mock.patch('builtins.print'):
            closed_capture.main()
        capture.assert_called_once()


if __name__=='__main__':unittest.main()
