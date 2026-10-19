#!/usr/bin/env bash
# Execute an operator-authorized, report-only rehearsal; never start the study.
set -euo pipefail
export AWS_PROFILE=geode-fem-bench AWS_REGION=us-east-1 AWS_DEFAULT_REGION=us-east-1
export AWS_PAGER=""
if [[ $# != 3 || $1 != --execute-authorized-rehearsal ]]; then
  echo "Usage: $0 --execute-authorized-rehearsal PREPARATION NEW_OUTPUT" >&2
  exit 2
fi
PKG=$(cd "$2" && pwd)
mkdir "$3"  # refuse to reuse an evidence directory
OUT=$(cd "$3" && pwd)
# Check the prepared request and every payload member before any cloud call.
python3 -I - "$PKG" <<'PY'
import hashlib,json,pathlib,sys,tarfile
p=pathlib.Path(sys.argv[1]); plan=json.loads((p/'launch.json').read_text())
m=json.loads((p/'preparation.json').read_text())
assert plan['ImageId']==m['ami'] and plan['InstanceType']==m['instance_type']
assert plan['MinCount']==plan['MaxCount']==1
assert plan['InstanceInitiatedShutdownBehavior']=='terminate'
assert not {'UserData','IamInstanceProfile','KeyName','LaunchTemplate','NetworkInterfaces'} & plan.keys()
s=json.dumps(plan)
assert all(x not in s for x in [m['source_instance'],*m['forbidden_volumes']])
roots=[d['Ebs'] for d in plan['BlockDeviceMappings'] if 'Ebs' in d]
assert len(roots)==1 and roots[0]['DeleteOnTermination'] is True
assert roots[0]['SnapshotId'].startswith('snap-') and 'VolumeId' not in roots[0]
assert all(set(d)=={'DeviceName','Ebs'} or d.get('NoDevice')=='' for d in plan['BlockDeviceMappings'])
tags={r['ResourceType']:{t['Key']:t['Value'] for t in r['Tags']} for r in plan['TagSpecifications']}
assert set(tags)=={'instance','volume'}
assert all(t['project']=='geode-fem' and t['purpose']=='paper3-quiet-rehearsal'
           and t['Name']=='geode-worker-'+m['name'] for t in tags.values())
with tarfile.open(p/'payload.tar.gz') as a:
    members=a.getmembers()
    assert len(members)==len(m['files_sha256'])
    assert {x.name for x in members}==set(m['files_sha256'])
    for x in members:
        assert x.isfile() and pathlib.PurePosixPath(x.name).name==x.name
        assert hashlib.sha256(a.extractfile(x).read()).hexdigest()==m['files_sha256'][x.name]
PY
cp "$PKG/preparation.json" "$PKG/launch.json" "$OUT/"
KEYDIR=$(mktemp -d "$OUT/key.XXXXXX")
ID=""
aws() { command aws --profile geode-fem-bench --region us-east-1 "$@"; }
# Invoked by the EXIT trap, including the signal handlers below.
# shellcheck disable=SC2329
cleanup() {
  rc=$?
  trap - EXIT INT TERM
  if [[ -n "$ID" ]]; then
    if aws ec2 terminate-instances --instance-ids "$ID" > "$OUT/termination.json" &&
       aws ec2 wait instance-terminated --instance-ids "$ID"; then
      echo "terminated $ID" >> "$OUT/controller.log"
      if [[ -f "$OUT/root-volume.txt" ]]; then
        read -r ROOT_VOLUME < "$OUT/root-volume.txt"
        if ! aws ec2 wait volume-deleted --volume-ids "$ROOT_VOLUME"; then
          echo "ROOT DELETION NOT VERIFIED: $ROOT_VOLUME" >&2
          rc=1
        fi
      fi
    else
      echo "TERMINATION NOT VERIFIED: $ID" >&2
      rc=1
    fi
  fi
  rm -rf "$KEYDIR"
  exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
ID=$(aws ec2 run-instances --cli-input-json "file://$PKG/launch.json" --query 'Instances[0].InstanceId' --output text)
[[ $ID =~ ^i-[a-f0-9]+$ ]] || { ID=""; echo 'Invalid launch instance ID' >&2; exit 1; }
echo "launched $ID" > "$OUT/controller.log"
aws ec2 wait instance-running --instance-ids "$ID"
aws ec2 describe-instances --instance-ids "$ID" > "$OUT/instance.json"
python3 -I - "$PKG" "$OUT" "$ID" > "$OUT/address.txt" <<'PY'
import json,pathlib,sys
p,o=map(pathlib.Path,sys.argv[1:3]);m=json.loads((p/'preparation.json').read_text())
i=json.loads((o/'instance.json').read_text())['Reservations'][0]['Instances'][0]
assert i['InstanceId']==sys.argv[3] and i['InstanceId']!=m['source_instance']
assert (i['ImageId'],i['InstanceType'])==(m['ami'],m['instance_type'])
v=[d['Ebs']['VolumeId'] for d in i['BlockDeviceMappings']]
assert len(v)==1 and not set(v)&set(m['forbidden_volumes'])
(o/'root-volume.txt').write_text(v[0]+'\n')
print(i['PublicIpAddress'],i['Placement']['AvailabilityZone'])
PY
read -r IP AZ < "$OUT/address.txt"
ssh-keygen -q -t ed25519 -N '' -C rehearsal-ephemeral -f "$KEYDIR/key"
SSH=(ssh -i "$KEYDIR/key" -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new -o "UserKnownHostsFile=$KEYDIR/known_hosts" -o LogLevel=ERROR -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -o ConnectTimeout=5)
SCP=(scp -i "$KEYDIR/key" -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new -o "UserKnownHostsFile=$KEYDIR/known_hosts" -o LogLevel=ERROR -o ConnectTimeout=5 -o ServerAliveInterval=30 -o ServerAliveCountMax=3)
push_key() {
  local result
  result=$(aws ec2-instance-connect send-ssh-public-key --instance-id "$ID" --instance-os-user ubuntu --availability-zone "$AZ" --ssh-public-key "file://$KEYDIR/key.pub" --query Success --output text)
  [[ $result == True ]]
}
up=0
for _ in $(seq 1 40); do
  if push_key && "${SSH[@]}" "ubuntu@$IP" 'sudo shutdown -P +75 && mkdir ~/rehearsal'; then up=1; break; fi
  sleep 6
done
[[ $up == 1 ]] || { echo 'SSH or shutdown cap could not be established' >&2; exit 1; }
push_key
"${SCP[@]}" "$PKG/payload.tar.gz" "ubuntu@$IP:rehearsal/"
RC=0
push_key
"${SSH[@]}" "ubuntu@$IP" 'cd ~/rehearsal && tar xzf payload.tar.gz && sha256sum *.py run-remote.sh && bash run-remote.sh' > "$OUT/remote.stdout" 2> "$OUT/remote.stderr" || RC=$?
echo "remote exit $RC" >> "$OUT/controller.log"
push_key
"${SCP[@]}" "ubuntu@$IP:rehearsal/rehearsal-report.json" "$OUT/" || RC=1
exit "$RC"
