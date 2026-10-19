"""Prepare a same-AMI throwaway rehearsal; this module never calls a cloud API.

The remote report-only entry is closed_capture.py --quiet-rehearsal, so the
actual collector's command, identity observer and quiet classifier are used.
No study paths, inventories, journals or numerical environments are read.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import time
import types

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import reboot_attestation as proof

TOOL_FILES=('closed_capture.py','trace_archive.py','recovery_evidence.py',
            'producer_source.py','reboot_attestation.py','quiet_rehearsal.py')
IDENTITY='benchmarks/motor_optimization/harness/process_identity.py'


def classifications(snapshot, digest):
    """Classify original bytes first, then redact identifiers and unknown args."""
    blocked={r['pid'] for r in proof.quiet_refusals(snapshot,digest)}
    table={r['pid']:r for r in snapshot['process_table']}
    own=snapshot['collector']['identity']['pid'];rows=[]
    public_shapes={shape for shapes in proof.SYSTEM_DAEMONS.values() for shape in shapes}
    for command in snapshot['process_commands']:
        pid=command['pid'];row=table[pid];uid=command.get('uid');argv=command['argv']
        if tuple(argv) in public_shapes:
            shape=list(argv)  # Exact published Ubuntu commands contain no host identifiers.
        elif pid==own:
            shape=['python3','-I','-B','/tools/closed_capture.py','<rehearsal arguments>']
        else:
            shape=[(Path(arg).name if i<2 and re.fullmatch(r'python[0-9.]*|uv|jupyter|ipython|cargo|rustc',Path(arg).name) else
                    arg if re.fullmatch(r'--?[A-Za-z][A-Za-z-]*',arg) else '<arg>')
                   for i,arg in enumerate(argv)]
        verdict=('refuse' if pid in blocked else 'collector' if pid==own else
                 'disappeared' if command['disappeared'] else 'zombie' if row['state'].startswith('Z') else
                 'system_daemon' if proof.system_daemon(row,command) else 'quiet')
        rows.append({'comm':command['comm'],'argv_shape':shape,
                     'uid_class':('system' if type(uid) is int and 0<=uid<1000 else
                                  'user' if type(uid) is int and uid>=1000 else 'unknown'),
                     'ppid':row.get('ppid') if row.get('ppid') in (0,1,2) else 'other',
                     'verdict':verdict})
    return sorted(rows,key=lambda r:json.dumps(r,sort_keys=True))


def wait_boot(*, run=subprocess.run, sleep=time.sleep):
    # Never stop services or mask a refusal; wait out normal boot work first.
    result=run(['cloud-init','status','--wait'],capture_output=True,timeout=900)
    if result.returncode:raise ValueError('cloud-init did not finish successfully')
    for attempt in range(91):
        states=[]
        for unit in ('apt-daily.service','apt-daily-upgrade.service'):
            result=run(['systemctl','show',unit,'--property=ActiveState','--value'],
                       capture_output=True,timeout=30)
            if result.returncode:raise ValueError('cannot observe boot package service')
            states.append(result.stdout.decode().strip())
        if all(s=='inactive' for s in states):return
        if any(s not in ('inactive','activating','active','deactivating') for s in states):
            raise ValueError('boot package service failed or has unknown state')
        if attempt==90:break
        print('Waiting for boot package services',file=sys.stderr,flush=True);sleep(10)
    raise ValueError('boot package services did not settle within 900 seconds')


def collect(identity, script, *, wait=proof.wait_for_quiet):
    snapshots=[];logs=[];digest=proof.producer_source.digest(script)
    def observe():
        snapshot=proof.process_snapshot(identity,script)
        snapshots.append(copy.deepcopy(snapshot));return snapshot
    error=None
    try:
        observed=wait(observe,digest,log=logs.append)
        proof.quiet_history(observed,digest)
    except (ValueError,OSError) as failure:
        error=[type(failure).__name__]  # Do not publish raw blocking argv or paths.
    attempts=[]
    for index,snapshot in enumerate(snapshots):
        try:rows=classifications(snapshot,digest)
        except (ValueError,KeyError,TypeError):rows=[];error=['malformed process observation']
        attempts.append({'attempt':index+1,'elapsed_seconds':logs[index]['elapsed_seconds'] if index<len(logs) else None,
                         'processes':rows})
    return {'schema':1,'kind':'quiet_process_rehearsal','scope':'report only; no study access or capture approval',
            'clean_pass':error is None,'error':error,'attempts':attempts,
            'source_sha256':{name:proof.producer_source.digest(HERE/name) for name in (*TOOL_FILES,'process_identity.py')}}


def collect_main(script):
    parser=argparse.ArgumentParser(description='Report-only quiet-process rehearsal')
    parser.add_argument('--quiet-rehearsal',action='store_true',required=True)
    parser.add_argument('--identity-sha256',required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    if sys.platform!='linux':raise ValueError('rehearsal requires the matching Linux AMI')
    if args.out.exists():raise ValueError('rehearsal report must be new')
    path=HERE/'process_identity.py';raw=path.read_bytes()
    if hashlib.sha256(raw).hexdigest()!=args.identity_sha256:raise ValueError('identity source changed')
    identity=types.ModuleType('process_identity');identity.__file__=str(path)
    exec(compile(raw,str(path),'exec'),identity.__dict__)
    wait_boot()
    report=collect(identity,script)
    with args.out.open('x') as stream:json.dump(report,stream,indent=2);stream.write('\n')
    print(json.dumps({'clean_pass':report['clean_pass'],'report_sha256':proof.producer_source.digest(args.out)}))
    if not report['clean_pass']:raise SystemExit(1)


def launch_plan(instance,image,name):
    if not re.fullmatch(r'paper3-rehearsal-[a-z0-9-]+',name):raise ValueError('use a unique rehearsal name')
    if image['ImageId']!=instance['ImageId']:raise ValueError('AMI mismatch')
    devices=image['BlockDeviceMappings'];root=image['RootDeviceName']
    roots=[d for d in devices if d['DeviceName']==root and 'Ebs' in d]
    extra=[d for d in devices if d['DeviceName']!=root]
    if (len(roots)!=1 or len({d['DeviceName'] for d in devices})!=len(devices)
            or any(set(d)!={'DeviceName','VirtualName'} or not re.fullmatch(r'ephemeral[0-9]+',d['VirtualName']) for d in extra)):
        raise ValueError('rehearsal requires an AMI with one fresh root volume')
    root_ebs=roots[0]['Ebs']
    if not re.fullmatch(r'snap-[a-f0-9]+',root_ebs['SnapshotId']):raise ValueError('invalid AMI root snapshot')
    return {'ImageId':instance['ImageId'],'InstanceType':instance['InstanceType'],
            'MinCount':1,'MaxCount':1,'SubnetId':instance['SubnetId'],
            'SecurityGroupIds':[g['GroupId'] for g in instance['SecurityGroups']],
            'BlockDeviceMappings':[{'DeviceName':root,'Ebs':{'SnapshotId':root_ebs['SnapshotId'],
                'DeleteOnTermination':True,'VolumeType':'gp3','VolumeSize':root_ebs['VolumeSize']}},
                *[{'DeviceName':d['DeviceName'],'NoDevice':''} for d in extra]],
            'TagSpecifications':[{'ResourceType':resource,'Tags':[{'Key':'project','Value':'geode-fem'},
                {'Key':'Name','Value':'geode-worker-'+name},{'Key':'purpose','Value':'paper3-quiet-rehearsal'}]}
                for resource in ('instance','volume')]}


def prepare(instance,image,name,repo,producer_commit,out):
    """Write reviewable inputs only; never launch, attach, SSH, or stop anything."""
    plan=launch_plan(instance,image,name)
    if not re.fullmatch(r'[a-f0-9]{40}',producer_commit):raise ValueError('full producer commit required')
    def git(*args):return proof.producer_source.git(repo,*args)
    if git('status','--porcelain'):raise ValueError('tool checkout must be clean before preparation')
    head=git('rev-parse','HEAD').decode().strip()
    prefix='benchmarks/motor_optimization/final_refinement/'
    files={name:git('show',head+':'+prefix+name) for name in TOOL_FILES}
    files['process_identity.py']=git('show',producer_commit+':'+IDENTITY)
    digest=hashlib.sha256(files['process_identity.py']).hexdigest()
    files['run-remote.sh']=('set -eu\ncd "$(dirname "$0")"\n'
        'exec /usr/bin/python3 -I -B "$PWD/closed_capture.py" --quiet-rehearsal '
        '--identity-sha256 '+digest+' --out "$PWD/rehearsal-report.json"\n').encode()
    out.mkdir(parents=True,exist_ok=False);payload=out/'payload';payload.mkdir()
    for filename,data in files.items():(payload/filename).write_bytes(data)
    (out/'launch.json').write_text(json.dumps(plan,indent=2)+'\n')
    metadata={'executed':False,'source_instance':instance['InstanceId'],'forbidden_volumes':
              [r['Ebs']['VolumeId'] for r in instance['BlockDeviceMappings'] if 'Ebs' in r],
              'ami':instance['ImageId'],'instance_type':instance['InstanceType'],'name':name,
              'tool_commit':head,'producer_commit':producer_commit,
              'files_sha256':{n:hashlib.sha256(b).hexdigest() for n,b in files.items()}}
    (out/'preparation.json').write_text(json.dumps(metadata,indent=2)+'\n')
    with tarfile.open(out/'payload.tar.gz','w:gz') as archive:
        for path in sorted(payload.iterdir()):archive.add(path,arcname=path.name)
    return metadata


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--instance-json',type=Path,required=True)
    parser.add_argument('--image-json',type=Path,required=True)
    parser.add_argument('--name',required=True)
    parser.add_argument('--repo',type=Path,required=True)
    parser.add_argument('--producer-commit',required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    instance=json.loads(args.instance_json.read_text())['Reservations'][0]['Instances'][0]
    image=json.loads(args.image_json.read_text())['Images'][0]
    print(json.dumps(prepare(instance,image,args.name,args.repo,args.producer_commit,args.out),sort_keys=True))


if __name__=='__main__':main()
