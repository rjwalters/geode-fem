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
    if args[:2]==['ec2','run-instances']:
        token=args[args.index('--client-token')+1]
        pathlib.Path(os.environ['CALLS']+'.token').write_text(token)
        if mode.startswith('lost-launch'):sys.exit(19)
        print('i-abc123')
    elif args[:2]==['ec2','describe-instances']:
        i={'InstanceId':'i-abc123','ImageId':'ami-123','InstanceType':'c7a.4xlarge','BlockDeviceMappings':[{'Ebs':{'VolumeId':'vol-new'}}],'PublicIpAddress':'192.0.2.1','Placement':{'AvailabilityZone':'us-east-1b'}}
        if mode=='actual-study-volume':i['BlockDeviceMappings'][0]['Ebs']['VolumeId']='vol-study'
        if mode=='actual-two-volumes':i['BlockDeviceMappings'].append({'Ebs':{'VolumeId':'vol-extra'}})
        if mode=='actual-wrong-ami':i['ImageId']='ami-other'
        if mode=='actual-wrong-type':i['InstanceType']='c7a.large'
        if mode=='actual-study-instance':i['InstanceId']='i-study'
        if mode.startswith('lost-launch'):
            assert '--filters' in args and 'Name=tag:Name,Values=geode-worker-paper3-rehearsal-test' in args
            i['ClientToken']=pathlib.Path(os.environ['CALLS']+'.token').read_text()
            i['Tags']=[{'Key':'Name','Value':'geode-worker-paper3-rehearsal-test'},{'Key':'project','Value':'geode-fem'},{'Key':'purpose','Value':'paper3-quiet-rehearsal'}]
            if mode=='lost-launch-other-token':i['ClientToken']='unrelated'
            if mode=='lost-launch-study-volume':i['BlockDeviceMappings'][0]['Ebs']['VolumeId']='vol-study'
        rows=[i,i.copy()] if mode=='lost-launch-ambiguous' else [i]
        print(json.dumps({'Reservations':[{'Instances':rows}]}))
    elif args[0]=='ec2-instance-connect':print('True')
    elif args[:2]==['ec2','terminate-instances']:
        if mode=='cleanup-failure':sys.exit(3)
        if mode=='cleanup-term':os.kill(os.getppid(),signal.SIGTERM)
    elif args[:3]==['ec2','wait','volume-deleted'] and mode=='volume-wait-failure':sys.exit(3)
elif name=='ssh':
    if 'shutdown -P +75' in args[-1] and mode=='cap-failure':
        # Evaluate the actual remote shell operators with harmless fake commands.
        import subprocess
        command='sudo() { return 1; }; mkdir() { return 0; }; '+args[-1]
        sys.exit(subprocess.run(['bash','-c',command]).returncode)
    if 'tar xzf' in args[-1]:
        if mode=='remote-failure':sys.exit(17)
        if mode=='term':os.kill(os.getppid(),signal.SIGTERM)
elif name=='scp' and mode=='fetch-failure' and any(':rehearsal/rehearsal-report.json' in a for a in args):sys.exit(4)
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
            if mode=='plan-study-volume':plan['TagSpecifications'][0]['Tags'].append({'Key':'unexpected','Value':'vol-study'})
            if mode=='plan-study-instance':plan['TagSpecifications'][0]['Tags'].append({'Key':'unexpected','Value':'i-study'})
            if mode=='plan-keep-root':plan['BlockDeviceMappings'][0]['Ebs']['DeleteOnTermination']=False
            if mode.startswith('plan-forbidden-'):plan[mode.removeprefix('plan-forbidden-')]='unexpected'
            if mode=='plan-no-project':
                for spec in plan['TagSpecifications']:spec['Tags']=[t for t in spec['Tags'] if t['Key']!='project']
            if mode=='plan-multi':plan['MaxCount']=2
            if mode=='plan-wrong-ami':plan['ImageId']='ami-other'
            if mode=='plan-wrong-type':plan['InstanceType']='c7a.large'
            (pkg/'launch.json').write_text(json.dumps(plan))
            raw=b'echo synthetic rehearsal\n'
            with tarfile.open(pkg/'payload.tar.gz','w:gz') as a:
                info=tarfile.TarInfo('run-remote.sh');info.size=len(raw);a.addfile(info,io.BytesIO(raw.replace(b'synthetic',b'CORRUPTED') if mode=='payload-corrupt' else raw))
            (pkg/'preparation.json').write_text(json.dumps({'source_instance':'i-study','forbidden_volumes':['vol-study'],
                'ami':'ami-123','instance_type':'c7a.4xlarge','name':'paper3-rehearsal-test',
                'files_sha256':{'run-remote.sh':hashlib.sha256(raw).hexdigest()}}))
            calls=root/'calls';out=root/'out'
            if mode=='existing-output':out.mkdir();(out/'retained').write_text('prior evidence')
            env=dict(os.environ,PATH=str(bin_dir)+os.pathsep+os.environ['PATH'],CALLS=str(calls),CASE=mode,
                     AWS_PROFILE='wrong-profile',AWS_REGION='wrong-region')
            script=Path(__file__).with_name('run_quiet_rehearsal.sh')
            result=subprocess.run(['bash',str(script),'--execute-authorized-rehearsal',str(pkg),str(out)],
                                  env=env,capture_output=True,text=True,timeout=30)
            entries=[json.loads(line) for line in calls.read_text().splitlines()] if calls.exists() else []
            self.assertFalse(list(out.glob('key.*')))
            for entry in entries:
                if entry['tool']=='ssh-keygen':
                    key=Path(entry['args'][entry['args'].index('-f')+1])
                    self.assertFalse(key.parent.exists())
            if mode=='existing-output':self.assertEqual((out/'retained').read_text(),'prior evidence')
            return result,entries

    def test_success_failure_and_signal_always_terminate(self):
        for mode,code in (('success',0),('remote-failure',17),('term',143),('cleanup-failure',1),('volume-wait-failure',1),('fetch-failure',1),('cleanup-term',0)):
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
        modes=['bad-plan','plan-study-volume','plan-study-instance','plan-keep-root','plan-no-project',
               'plan-multi','plan-wrong-ami','plan-wrong-type','payload-corrupt','existing-output']
        modes += ['plan-forbidden-'+key for key in ('UserData','IamInstanceProfile','KeyName','LaunchTemplate','NetworkInterfaces')]
        for mode in modes:
            with self.subTest(mode=mode):
                result,calls=self.run_case(mode)
                self.assertNotEqual(result.returncode,0,result.stdout+result.stderr);self.assertEqual(calls,[])

    def test_invalid_actual_instance_terminates_before_transfer(self):
        for mode in ('actual-study-volume','actual-two-volumes','actual-wrong-ami','actual-wrong-type','actual-study-instance'):
            with self.subTest(mode=mode):
                result,calls=self.run_case(mode)
                self.assertNotEqual(result.returncode,0)
                self.assertFalse(any(r['tool']=='scp' for r in calls))
                self.assertTrue(any(r['tool']=='aws' and r['args'][4:]==['ec2','terminate-instances','--instance-ids','i-abc123'] for r in calls))

    def test_lost_launch_response_recovers_only_our_token(self):
        for mode in ('lost-launch','lost-launch-other-token','lost-launch-ambiguous','lost-launch-study-volume'):
            with self.subTest(mode=mode):
                result,calls=self.run_case(mode)
                self.assertNotEqual(result.returncode,0)
                self.assertFalse(any(r['tool'] in ('ssh','scp') for r in calls))
                self.assertTrue(any(r['tool']=='aws' and '--filters' in r['args'] for r in calls))
                terminated=[r for r in calls if r['tool']=='aws' and 'terminate-instances' in r['args']]
                self.assertEqual(len(terminated),1 if mode=='lost-launch' else 0)
                if terminated:self.assertEqual(terminated[0]['args'][4:],['ec2','terminate-instances','--instance-ids','i-abc123'])

    def test_failed_shutdown_cap_prevents_transfer_and_terminates(self):
        result,calls=self.run_case('cap-failure')
        self.assertNotEqual(result.returncode,0)
        self.assertFalse(any(r['tool']=='scp' for r in calls))
        self.assertTrue(any(r['tool']=='aws' and 'terminate-instances' in r['args'] for r in calls))


if __name__=='__main__':unittest.main()
