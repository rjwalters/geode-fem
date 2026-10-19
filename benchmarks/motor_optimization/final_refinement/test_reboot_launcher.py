"""Exercise the canonical rehearsal controller using fake cloud and SSH tools."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parent))
import quiet_rehearsal as rehearsal

FAKE = r'''
import json,os,pathlib,signal,sys
name=pathlib.Path(sys.argv[0]).name;args=sys.argv[1:]
with open(os.environ['CALLS'],'a') as f:
    f.write(json.dumps({'tool':name,'args':args,'profile':os.environ.get('AWS_PROFILE'),'region':os.environ.get('AWS_REGION')})+'\n')
mode=os.environ['CASE']
if name=='aws':
    assert args[:4]==['--profile','geode-fem-bench','--region','us-east-1']
    args=args[4:]
    if args[:2]==['ec2','run-instances']:print('i-abc123')
    elif args[:2]==['ec2','describe-instances']:
        print(json.dumps({'Reservations':[{'Instances':[{'InstanceId':'i-abc123','ImageId':'ami-123','InstanceType':'c7a.4xlarge','BlockDeviceMappings':[{'Ebs':{'VolumeId':'vol-new'}}],'PublicIpAddress':'192.0.2.1','Placement':{'AvailabilityZone':'us-east-1b'}}]}]}))
    elif args[0]=='ec2-instance-connect':print('True')
    elif args[:2]==['ec2','terminate-instances'] and mode=='cleanup-failure':sys.exit(3)
elif name=='ssh':
    if 'shutdown -P +75' in args[-1] and mode=='cap-failure':sys.exit(1)
    if 'tar xzf' in args[-1]:
        if mode=='remote-failure':sys.exit(17)
        if mode=='term':os.kill(os.getppid(),signal.SIGTERM)
'''


class RehearsalLauncherTests(unittest.TestCase):
    def run_case(self,mode):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);pkg=root/'pkg';pkg.mkdir();bin_dir=root/'bin';bin_dir.mkdir()
            for name in ('aws','ssh','scp','ssh-keygen','sleep'):
                path=bin_dir/name;path.write_text('#!'+sys.executable+'\n'+FAKE);path.chmod(0o755)
            instance={'InstanceId':'i-study','ImageId':'ami-123','InstanceType':'c7a.4xlarge',
                      'SubnetId':'subnet-123','SecurityGroups':[{'GroupId':'sg-123'}],
                      'BlockDeviceMappings':[{'DeviceName':'/dev/sda1','Ebs':{'VolumeId':'vol-study'}}]}
            image={'ImageId':'ami-123','RootDeviceName':'/dev/sda1','BlockDeviceMappings':[
                {'DeviceName':'/dev/sda1','Ebs':{'SnapshotId':'snap-abc','VolumeSize':8}}]}
            plan=rehearsal.launch_plan(instance,image,'paper3-rehearsal-test')
            if mode=='bad-plan':plan['InstanceInitiatedShutdownBehavior']='stop'
            (pkg/'launch.json').write_text(json.dumps(plan))
            raw=b'echo synthetic rehearsal\n'
            with tarfile.open(pkg/'payload.tar.gz','w:gz') as a:
                info=tarfile.TarInfo('run-remote.sh');info.size=len(raw);a.addfile(info,io.BytesIO(raw))
            (pkg/'preparation.json').write_text(json.dumps({'source_instance':'i-study','forbidden_volumes':['vol-study'],
                'ami':'ami-123','instance_type':'c7a.4xlarge','name':'paper3-rehearsal-test',
                'files_sha256':{'run-remote.sh':hashlib.sha256(raw).hexdigest()}}))
            calls=root/'calls';out=root/'out'
            env=dict(os.environ,PATH=str(bin_dir)+os.pathsep+os.environ['PATH'],CALLS=str(calls),CASE=mode,
                     AWS_PROFILE='wrong-profile',AWS_REGION='wrong-region')
            script=Path(__file__).with_name('run_quiet_rehearsal.sh')
            result=subprocess.run(['bash',str(script),'--execute-authorized-rehearsal',str(pkg),str(out)],
                                  env=env,capture_output=True,text=True,timeout=30)
            entries=[json.loads(line) for line in calls.read_text().splitlines()] if calls.exists() else []
            self.assertFalse(list(out.glob('key.*')))
            return result,entries

    def test_success_failure_and_signal_always_terminate(self):
        for mode,code in (('success',0),('remote-failure',17),('term',143),('cleanup-failure',1)):
            with self.subTest(mode=mode):
                result,calls=self.run_case(mode)
                self.assertEqual(result.returncode,code,result.stderr)
                cloud=[r for r in calls if r['tool']=='aws']
                self.assertTrue(all((r['profile'],r['region'])==('geode-fem-bench','us-east-1') for r in cloud))
                commands=[r['args'][4:] for r in cloud]
                self.assertIn(['ec2','terminate-instances','--instance-ids','i-abc123'],commands)
                if mode!='cleanup-failure':
                    self.assertIn(['ec2','wait','instance-terminated','--instance-ids','i-abc123'],commands)
                    self.assertIn(['ec2','wait','volume-deleted','--volume-ids','vol-new'],commands)
                cap=next(i for i,r in enumerate(calls) if r['tool']=='ssh' and 'shutdown -P +75' in r['args'][-1])
                transfer=next(i for i,r in enumerate(calls) if r['tool']=='scp')
                self.assertLess(cap,transfer)
                self.assertNotIn('i-study',json.dumps(calls));self.assertNotIn('vol-study',json.dumps(calls))

    def test_invalid_plan_refused_before_cloud_call(self):
        result,calls=self.run_case('bad-plan')
        self.assertNotEqual(result.returncode,0);self.assertEqual(calls,[])

    def test_failed_shutdown_cap_prevents_transfer_and_terminates(self):
        result,calls=self.run_case('cap-failure')
        self.assertNotEqual(result.returncode,0)
        self.assertFalse(any(r['tool']=='scp' for r in calls))
        self.assertTrue(any(r['tool']=='aws' and 'terminate-instances' in r['args'] for r in calls))


if __name__=='__main__':unittest.main()
