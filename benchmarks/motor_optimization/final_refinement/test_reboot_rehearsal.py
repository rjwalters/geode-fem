"""Report-only rehearsal, redaction and fresh-volume launch-plan regressions."""
import copy
import json
from pathlib import Path
from types import SimpleNamespace
import sys
import unittest
from unittest import mock

sys.path.insert(0,str(Path(__file__).resolve().parent))
import quiet_rehearsal as rehearsal
import reboot_attestation as proof
from test_reboot_attestation import fixture
from test_reboot_quiet import process


class RehearsalTests(unittest.TestCase):
    def test_plan_matches_source_ami_type_and_never_reuses_study_volume(self):
        instance={'ImageId':'ami-123','InstanceType':'r8g.4xlarge','SubnetId':'subnet-123',
                  'SecurityGroups':[{'GroupId':'sg-123'}],
                  'BlockDeviceMappings':[{'DeviceName':'/dev/sda1','Ebs':{'VolumeId':'vol-study'}}]}
        image={'ImageId':'ami-123','RootDeviceName':'/dev/sda1','BlockDeviceMappings':[
            {'DeviceName':'/dev/sda1','Ebs':{'SnapshotId':'snap-abc','VolumeSize':8}}]}
        plan=rehearsal.launch_plan(instance,image,'paper3-rehearsal-test')
        self.assertEqual(plan['ImageId'],instance['ImageId'])
        self.assertEqual(plan['InstanceType'],instance['InstanceType'])
        self.assertEqual((plan['MinCount'],plan['MaxCount']),(1,1))
        self.assertEqual(plan['BlockDeviceMappings'],[{'DeviceName':'/dev/sda1','Ebs':{
            'SnapshotId':'snap-abc','VolumeSize':8,'VolumeType':'gp3','DeleteOnTermination':True}}])
        self.assertNotIn('vol-study',json.dumps(plan))
        self.assertNotIn('IamInstanceProfile',plan)
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
        process(observed,'worker',['/home/private/python3','/home/private/task.py','secret-session-id'],pid=701,uid=1000)
        rows=rehearsal.classifications(observed,'d'*64)
        daemon=next(r for r in rows if r['comm']=='dbus-daemon')
        self.assertEqual(daemon,{'comm':'dbus-daemon','argv_shape':argv,'ppid':1,
                                 'uid_class':'system','verdict':'system_daemon'})
        worker=next(r for r in rows if r['comm']=='worker')
        self.assertEqual(worker['verdict'],'refuse');self.assertEqual(worker['uid_class'],'user')
        self.assertEqual(worker['argv_shape'],['python3','<arg>','<arg>'])
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


if __name__=='__main__':unittest.main()
