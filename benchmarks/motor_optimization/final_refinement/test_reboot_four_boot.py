"""Synthetic four-boot chain B0 -> B1 -> refused B2 -> B3; no study outcomes or cloud calls."""
import copy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import capture_archive
import closed_capture
import failed_capture_witness
import reboot_attestation as proof
from test_reboot_attestation import fixture, raw, utc, listing, journal, B0, B1, B2, EXTRA

B3 = '00000005-0000-0000-0000-000000000000'
REFUSAL = proof.FAILED_CAPTURE_REFUSAL
# Shape of the retained e1814128 B2 log; paths and line numbers are synthetic.
CAPTURE_LOG = '\n'.join([
    'tools/closed_capture.py: OK', 'tools/producer_source.py: OK', 'tools/quiet_rehearsal.py: OK',
    'tools/reboot_attestation.py: OK', 'tools/recovery_evidence.py: OK', 'tools/trace_archive.py: OK',
    'policy.json: OK', 'Waiting for cloud-init and apt-daily before capture',
    'Boot services settled; starting merged collector', 'Traceback (most recent call last):',
    '  File "/home/fixture/tools/closed_capture.py", line 1, in <module>',
    '  File "/home/fixture/tools/reboot_attestation.py", line 2, in processes',
    '    observed = self.observe(self.p, self.ec2, identity, script)',
    '  File "/home/fixture/tools/reboot_attestation.py", line 3, in live_observation',
    "    journals = {boot: _command(['journalctl', '-b', boot, '-o', 'json', '--no-pager', '--utc'])",
    '  File "/home/fixture/tools/reboot_attestation.py", line 4, in _command',
    "    require(result.returncode == 0, 'read-only observation command failed: ' + args[0])",
    '  File "/home/fixture/tools/reboot_attestation.py", line 5, in require',
    "    raise ValueError('reboot attestation: ' + message)", REFUSAL]) + '\n'
# Timeline: B1 launch 295, B1 stop 400 (journal end 410.65), B2 launch 420,
# B2 journal 425.000001-470.5, B2 stop 465, B3 launch 500, B3 boots 505-550.
B2_LAUNCH, B2_STOP = 420, 465


def gmt(value):
    return datetime.fromtimestamp(value, timezone.utc).strftime('%Y-%m-%d %H:%M:%S')


def described(p, cloud, state, launch, stop=None):
    """Full describe-instances JSON, as retained by the operator before/after B2."""
    item = {'InstanceId': p['instance_id'], 'State': {'Code': 80 if state == 'stopped' else 16, 'Name': state},
            'LaunchTime': utc(launch), 'PrivateDnsName': cloud['PrivateDnsName'],
            'BlockDeviceMappings': copy.deepcopy(cloud['BlockDeviceMappings'])}
    if stop is not None:
        item.update(StateTransitionReason=f'User initiated ({gmt(stop)} GMT)',
                    StateReason={'Code': 'Client.UserInitiatedShutdown',
                                 'Message': 'Client.UserInitiatedShutdown: User initiated shutdown'})
    return {'Reservations': [{'Instances': [item]}]}


def failed_files(p, cloud):
    log = CAPTURE_LOG.encode()
    transition = lambda key, before, after: raw({key: [{'InstanceId': p['instance_id'],
        'CurrentState': {'Name': after}, 'PreviousState': {'Name': before}}]})
    files = {name: raw({'fixture': name}) for name in proof.FAILED_CAPTURE_FILES}
    files.update({
        'refusal-summary.json': raw({'quiet_attempts': 0, 'source_census_started': False,
                                     'capture_archive_created': False, 'start_requests': 1,
                                     'no_boot_retry': True, 'stopped': True, 'capture_exit': 1,
                                     'launch_time': utc(B2_LAUNCH), 'capture_log_sha256': proof.sha(log),
                                     'tool_commit': proof.FAILED_CAPTURE_TOOL_COMMIT}),
        'capture.log': log, 'transfer.log': b'', 'cloud-transfer.log': b'',
        'start.json': transition('StartingInstances', 'stopped', 'pending'),
        'stop-response.json': transition('StoppingInstances', 'running', 'stopping'),
        'before-start.json': raw(described(p, cloud, 'stopped', 295, 400)),
        'ec2-capture.json': raw(dict(cloud, LaunchTime=utc(B2_LAUNCH))),
        'final-instance.json': raw(described(p, cloud, 'stopped', B2_LAUNCH, B2_STOP))})
    return files


def failed_witness(p, files):
    return {'schema': 1, 'kind': proof.FAILED_CAPTURE_KIND,
            **{k: p[k] for k in ('instance_id', 'volume_id', 'study_boot', 'recovery_boot',
                                 'failed_capture_boot', 'recovery_witness_sha256')},
            'witness_statement': proof.FAILED_CAPTURE_STATEMENT, 'scope': 'Synthetic refused B2 witness',
            'files': {name: {'sha256': proof.sha(data), 'bytes': len(data)} for name, data in files.items()}}


def four_boot(p, observed):
    """Insert a refused B2 into the three-boot fixture; capture now runs on B3."""
    p.update(kind=proof.FOUR_BOOT, failed_capture_boot=B2)
    files = failed_files(p, observed['ec2'])
    w = failed_witness(p, files)
    p['failed_capture_witness_sha256'] = proof.sha(raw(w))
    host = dict(observed['host'], boot=B3)
    observed['host'] = host
    observed['collector']['identity'] = dict(observed['collector']['identity'], host=dict(host))
    observed['journal_boot_list'] = listing([(B0, 100, 200), (B1, 300, 410), (B2, 425, 470), (B3, 505, 550)])
    observed['ended_boot_journals'][B2] = journal(B2, 425_000_001, 470_500_000)
    return w, files


def b2_entry(comm=None, exe=None, cmdline=None, uid=None, t=450_000_000):
    entry = {'_BOOT_ID': proof.boot_id(B2), '__REALTIME_TIMESTAMP': str(t), 'MESSAGE': 'fixture'}
    entry.update({k: v for k, v in (('_COMM', comm), ('_EXE', exe), ('_CMDLINE', cmdline), ('_UID', uid))
                  if v is not None})
    return entry


COLLECTOR = ('/opt/numerics/bin/python3 -I -B /home/fixture/tools/closed_capture.py --source /exp2/main '
             '--repo /src --reviewed-commit ' + 'a'*40 + ' --out /new --reboot-policy /p.json '
             '--reboot-policy-sha256 ' + 'b'*64 + ' --recovery-evidence /r --capture-ec2 /e.json')
ALLOWED = [
    b2_entry('sshd', '/usr/sbin/sshd', 'sshd: fixture [priv]', '0'),
    b2_entry('sudo', '/usr/bin/sudo', 'sudo journalctl --list-boots --no-pager', '0'),
    b2_entry('cloud-init', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/cloud-init modules --mode=final', '0'),
    b2_entry('unattended-upgr', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/unattended-upgrade', '0'),
    b2_entry('apt.systemd.dai', '/usr/bin/dash', '/bin/sh /usr/lib/apt/apt.systemd.daily update', '0'),
    b2_entry('networkd-dispat', '/usr/bin/python3.12',
             '/usr/bin/python3 /usr/bin/networkd-dispatcher --run-startup-triggers', '0'),
    b2_entry('dbus-daemon', '/usr/bin/dbus-daemon',
             '@dbus-daemon --system --address=systemd: --nofork --nopidfile --systemd-activation --syslog-only', '101'),
    b2_entry('systemd', '/usr/lib/systemd/systemd', '/sbin/init', '0'),
    b2_entry(),  # kernel transport: no process fields
    b2_entry('hibinit-agent', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/hibinit-agent -c /etc/hibinit-config.cfg', '0'),
]
REFUSED = [
    b2_entry('python3', '/usr/bin/python3.12', '/usr/bin/python3 /tmp/evaluate.py', '1000'),
    b2_entry('icem_harness_ev', '/tmp/icem_harness_eval', '/tmp/icem_harness_eval --job 1', '1000'),
    b2_entry('worker', '/usr/bin/python3.12', '/usr/bin/python3 /tmp/run', '1000'),
    b2_entry('worker', '/usr/bin/python3.12', 'worker /tmp/run', '1000'),  # renamed interpreter argv
    b2_entry('tmux: server', '/usr/bin/tmux', 'tmux new -d', '1000'),
    b2_entry('sudo', '/usr/bin/sudo', 'sudo -n python3 /tmp/x.py', '0'),
    b2_entry('sudo', '/usr/bin/sudo', 'sudo -u fixture /tmp/run-analysis.sh', '0'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR.replace(' -B', ''), '1000'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR + ' --quiet-rehearsal', '1000'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR + ' --extra value', '1000'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR + ' --out', '1000'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR.replace(' -B', ' -u'), '1000'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR.replace('/home/fixture/tools/', 'tools/'), '1000'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR.replace('closed_capture', 'analysis'), '1000'),
    b2_entry('cloud-init', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/cloud-init init', '1000'),
    b2_entry('cloud-init', '/usr/bin/python3.12', '/usr/bin/python3 /tmp/cloud-init', '0'),
    b2_entry('unattended-upgr', '/usr/bin/python3.12', '/usr/bin/python3 /tmp/unattended-upgrade', '0'),
    b2_entry('networkd-dispat', '/usr/bin/python3.12',
             '/usr/bin/python3 /usr/bin/networkd-dispatcher --run-startup-triggers --extra', '0'),
    b2_entry('networkd-dispat', '/usr/bin/python3.12',
             '/usr/bin/python3 /usr/bin/networkd-dispatcher --run-startup-triggers', None),
    b2_entry('dbus-daemon', '/usr/bin/dbus-daemon',
             '/usr/bin/dbus-daemon --session --address=systemd: --nofork --nopidfile --systemd-activation --syslog-only', '1000'),
    b2_entry('dbus-daemon', '/tmp/python3',
             '/usr/bin/dbus-daemon --system --address=systemd: --nofork --nopidfile --systemd-activation --syslog-only', '0'),
    b2_entry('dbus-daemon', '/tmp/other',
             '/usr/bin/dbus-daemon --system --address=systemd: --nofork --nopidfile --systemd-activation --syslog-only', '0'),
    b2_entry('ssm-session-wor', '/usr/bin/ssm-session-worker', '/usr/bin/ssm-session-worker fixture-session', '0'),
    b2_entry('jupyter-lab', '/usr/bin/jupyter-lab', '/usr/bin/jupyter-lab', '1000'),
    # Judge bypasses of the removed collector exception.
    b2_entry('python3', '/usr/bin/python3.12', '/usr/bin/python3 -I -B /tmp/closed_capture.py --source /exp2', '1000'),
    b2_entry('python3', '/opt/numerics/bin/python3.12', COLLECTOR, '1000'),  # study python titled as the collector
    b2_entry('worker', '/usr/bin/worker', COLLECTOR, '1000'),
    b2_entry('sudo', '/usr/bin/sudo', 'sudo -n /usr/bin/python3 -I -B '
             '/home/ubuntu/paper3-capture-e1814128/tools/closed_capture.py --source /exp2', '0'),
    # Rust study binary: comm, executable basename or command word.
    b2_entry('geode', '/opt/geode/bin/geode', None, '1000'),  # _CMDLINE nulled
    b2_entry('tokio-runtime-w', '/home/fixture/target/release/geode', None, '1000'),
    b2_entry('sh', '/usr/bin/dash', 'sh -c /home/fixture/target/release/geode run', '1000'),
    # hibinit-agent: exact argv, system UID and Python executable only.
    b2_entry('hibinit-agent', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/hibinit-agent -c /tmp/x.cfg', '0'),
    b2_entry('hibinit-agent', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/hibinit-agent -c /etc/hibinit-config.cfg', '1000'),
    b2_entry('hibinit-agent', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/hibinit-agent', '0'),
    b2_entry('evaluate', '/usr/bin/python3.12', '/usr/bin/python3 /usr/bin/hibinit-agent -c /etc/hibinit-config.cfg', '0'),
]


class FourBootTests(unittest.TestCase):
    def setUp(self):
        self.p, self.w, self.files, self.observed = fixture()
        self.fw, self.ffiles = four_boot(self.p, self.observed)

    def history(self):
        return proof.recovery(raw(self.w), self.files, self.p)

    def failed(self, *, repin=True):
        # Fixture-only repin so changed records reach the semantic checks.
        if repin:
            self.fw['files'] = {n: {'sha256': proof.sha(d), 'bytes': len(d)} for n, d in self.ffiles.items()}
            self.p['failed_capture_witness_sha256'] = proof.sha(raw(self.fw))
        return proof.failed_capture(raw(self.fw), self.ffiles, self.p, self.history())

    def verdict(self, observed=None):
        failed = self.failed()
        return proof.observations(self.p, self.history(), observed or self.observed, failed)

    def edit(self, name, change):
        value = json.loads(self.ffiles[name]); change(value); self.ffiles[name] = raw(value)

    def set_log(self, text):
        self.ffiles['capture.log'] = text.encode()
        self.edit('refusal-summary.json', lambda s: s.update(capture_log_sha256=proof.sha(text.encode())))

    def set_b2_launch(self, value):
        self.edit('ec2-capture.json', lambda r: r.update(LaunchTime=utc(value)))
        self.edit('final-instance.json', lambda r: r['Reservations'][0]['Instances'][0].update(LaunchTime=utc(value)))
        self.edit('refusal-summary.json', lambda s: s.update(launch_time=utc(value)))

    def b2_journal(self, *entries):
        lines = self.observed['ended_boot_journals'][B2].splitlines()
        self.observed['ended_boot_journals'][B2] = ''.join(
            line + '\n' for line in [lines[0], *(json.dumps(e) for e in entries), lines[-1]])

    def test_complete_four_boot_proof_accepts(self):
        p = proof.policy(raw(self.p), proof.sha(raw(self.p)))
        failed = self.failed()
        self.assertEqual((failed['launch'], failed['stop']), (B2_LAUNCH, B2_STOP))
        verdict = proof.observations(p, self.history(), self.observed, failed)
        self.assertEqual(verdict['study_end_us'], 200_123_456)
        self.assertEqual((verdict['capture_boot'], verdict['failed_capture_boot']), (B3, B2))
        self.assertEqual(verdict['scope'], proof.FOUR_BOOT_SCOPE)
        self.assertEqual(proof.ended_boots(p), (B0, B1, B2))
        self.assertTrue(proof.quiet_processes(self.observed, capture_script_sha256='d'*64))

    def test_three_boot_policy_is_unchanged(self):
        p, w, files, observed = fixture()
        self.assertEqual(proof.ended_boots(p), (B0, B1))
        self.assertEqual(proof.observations(p, proof.recovery(raw(w), files, p), observed)['scope'], proof.SCOPE)

    def test_policy_requires_exact_four_boot_fields_and_distinct_boots(self):
        for change in (lambda p: p.pop('failed_capture_boot'), lambda p: p.update(kind=proof.THREE_BOOT),
                       lambda p: p.update(extra=1)):
            p = dict(self.p); change(p)
            with self.assertRaisesRegex(ValueError, 'unsupported policy'): proof.policy(raw(p), proof.sha(raw(p)))
        three = fixture()[0]; three['failed_capture_boot'] = B2
        with self.assertRaisesRegex(ValueError, 'unsupported policy'): proof.policy(raw(three), proof.sha(raw(three)))
        for boot in (B0, B1):
            p = dict(self.p, failed_capture_boot=boot)
            with self.assertRaisesRegex(ValueError, 'four-boot policy reuses a boot'):
                proof.policy(raw(p), proof.sha(raw(p)))
        for key, value in (('failed_capture_boot', proof.boot_id(B2)), ('failed_capture_witness_sha256', 'x')):
            p = dict(self.p, **{key: value})
            with self.assertRaises(ValueError): proof.policy(raw(p), proof.sha(raw(p)))

    def test_extra_boot_refused(self):
        for rows in ([(B0, 100, 200), (B1, 300, 410), (EXTRA, 415, 416), (B2, 425, 470), (B3, 505, 550)],
                     [(B0, 100, 200), (B1, 300, 410), (B2, 425, 470), (EXTRA, 480, 490), (B3, 505, 550)],
                     [(B0, 100, 200), (B1, 300, 410), (B2, 425, 470), (B3, 505, 550), (EXTRA, 600, 610)],
                     [(B0, 100, 200), (EXTRA, 250, 260), (B1, 300, 410), (B2, 425, 470), (B3, 505, 550)]):
            with self.subTest(rows=rows):
                obs = copy.deepcopy(self.observed); obs['journal_boot_list'] = listing(rows)
                with self.assertRaisesRegex(ValueError, 'missing, extra or unobserved intermediate boot'):
                    self.verdict(obs)

    def test_missing_b2_refused(self):
        obs = copy.deepcopy(self.observed)
        obs['journal_boot_list'] = listing([(B0, 100, 200), (B1, 300, 410), (B3, 505, 550)])
        with self.assertRaisesRegex(ValueError, 'missing, extra or unobserved intermediate boot'): self.verdict(obs)
        obs = copy.deepcopy(self.observed); obs['ended_boot_journals'].pop(B2)
        with self.assertRaisesRegex(ValueError, 'missing/extra shutdown journal'): self.verdict(obs)

    def test_reordered_b2_b3_refused(self):
        obs = copy.deepcopy(self.observed)
        obs['journal_boot_list'] = listing([(B0, 100, 200), (B1, 300, 410), (B3, 425, 470), (B2, 505, 550)])
        with self.assertRaisesRegex(ValueError, 'missing, extra or unobserved intermediate boot'): self.verdict(obs)
        obs = copy.deepcopy(self.observed); obs['host'] = dict(obs['host'], boot=B2)
        with self.assertRaisesRegex(ValueError, 'capture reused an earlier boot'): self.verdict(obs)

    def test_b2_launch_before_b1_stop_refused(self):
        self.set_b2_launch(399)
        with self.assertRaisesRegex(ValueError, 'B2 launch/stop ordering differs'): self.failed()

    def test_b2_launch_must_follow_b1_journal_and_precede_b2_first_entry(self):
        for launch in (405, 426):
            with self.subTest(launch=launch):
                self.setUp(); self.set_b2_launch(launch)
                with self.assertRaisesRegex(ValueError, 'B1 stop/B2 launch/B2 first entry ordering differs'):
                    self.verdict()

    def test_b2_stop_must_not_follow_its_journal(self):
        self.edit('final-instance.json', lambda r: r['Reservations'][0]['Instances'][0].update(
            StateTransitionReason=f'User initiated ({gmt(471)} GMT)'))
        with self.assertRaisesRegex(ValueError, 'B2 stop follows its journal end'): self.verdict()

    def test_b3_launch_must_follow_b2_journal_end(self):
        for launch in (470, 506):
            with self.subTest(launch=launch):
                obs = copy.deepcopy(self.observed); obs['ec2']['LaunchTime'] = utc(launch)
                with self.assertRaisesRegex(ValueError, 'B2 journal end/B3 launch ordering differs'):
                    self.verdict(obs)

    def test_operator_ip_log_is_never_witness_evidence(self):
        self.assertNotIn('allow-ip.log', proof.FAILED_CAPTURE_FILES)
        self.assertEqual(len(proof.FAILED_CAPTURE_FILES), 15)
        log = b'geode-worker: ssh rule for 203.0.113.7/32 already present\n'
        self.fw['files']['allow-ip.log'] = {'sha256': proof.sha(log), 'bytes': len(log)}
        self.p['failed_capture_witness_sha256'] = proof.sha(raw(self.fw))
        with self.assertRaisesRegex(ValueError, 'missing or extra failed-capture evidence'):
            proof.failed_capture(raw(self.fw), dict(self.ffiles, **{'allow-ip.log': log}), self.p, self.history())

    def test_b2_witness_hash_mismatch_refused(self):
        with self.assertRaisesRegex(ValueError, 'changed B2 witness'):
            proof.failed_capture(raw(self.fw) + b' ', self.ffiles, self.p, self.history())
        for name in sorted(proof.FAILED_CAPTURE_FILES):
            with self.subTest(name=name):
                files = dict(self.ffiles); files[name] += b' '
                with self.assertRaisesRegex(ValueError, 'changed failed-capture evidence: ' + name):
                    proof.failed_capture(raw(self.fw), files, self.p, self.history())
        for files in ({k: v for k, v in self.ffiles.items() if k != 'capture.log'}, dict(self.ffiles, extra=b'')):
            with self.assertRaisesRegex(ValueError, 'missing or extra failed-capture evidence'):
                proof.failed_capture(raw(self.fw), files, self.p, self.history())
        self.fw['files']['capture.log'] = dict(self.fw['files']['capture.log'], bytes=len(self.ffiles['capture.log']) + 1)
        self.p['failed_capture_witness_sha256'] = proof.sha(raw(self.fw))
        with self.assertRaisesRegex(ValueError, 'changed failed-capture evidence: capture.log'):
            proof.failed_capture(raw(self.fw), self.ffiles, self.p, self.history())
        self.setUp()
        # A manifest may not name an unsupplied, unverified extra file either.
        self.fw['files']['before-alarm.json'] = {'sha256': '0'*64, 'bytes': 0}
        self.p['failed_capture_witness_sha256'] = proof.sha(raw(self.fw))
        with self.assertRaisesRegex(ValueError, 'missing or extra failed-capture evidence'):
            proof.failed_capture(raw(self.fw), self.ffiles, self.p, self.history())

    def test_witness_kind_identity_and_statement_required(self):
        for key in ('instance_id', 'volume_id', 'study_boot', 'recovery_boot', 'failed_capture_boot',
                    'recovery_witness_sha256'):
            with self.subTest(key=key):
                self.setUp(); self.fw[key] = 'changed'
                with self.assertRaisesRegex(ValueError, 'B2 witness changed ' + key): self.failed()
        for kind, value in (('kind', 'paper3_recovery_boot_witness'), ('schema', True), ('schema', 2)):
            self.setUp(); self.fw[kind] = value
            with self.assertRaisesRegex(ValueError, 'unsupported failed-capture witness'): self.failed()
        for change in (lambda w: w.update(operator_ip='203.0.113.7'), lambda w: w.pop('scope'),
                       lambda w: w.update(scope=None)):
            self.setUp(); change(self.fw)
            with self.assertRaisesRegex(ValueError, 'failed-capture witness has missing or extra fields'): self.failed()
        for statement in ('B2 was copy-only.', None, 'Prefix. ' + proof.FAILED_CAPTURE_STATEMENT):
            self.setUp(); self.fw['witness_statement'] = statement
            with self.assertRaisesRegex(ValueError, 'missing refused-capture witness statement'): self.failed()

    def test_ec2_start_and_stop_records_bind_instance_and_b1_stop(self):
        cases = [
            ('start.json', lambda r: r['StartingInstances'][0].update(InstanceId='i-fff'), 'B2 start/stop requests differ'),
            ('start.json', lambda r: r['StartingInstances'][0]['PreviousState'].update(Name='running'), 'B2 start/stop requests differ'),
            ('start.json', lambda r: r['StartingInstances'].append(r['StartingInstances'][0]), 'B2 start/stop requests differ'),
            ('stop-response.json', lambda r: r['StoppingInstances'][0].update(InstanceId='i-fff'), 'B2 start/stop requests differ'),
            ('stop-response.json', lambda r: r['StoppingInstances'][0]['CurrentState'].update(Name='running'), 'B2 start/stop requests differ'),
            ('before-start.json', lambda r: r['Reservations'][0]['Instances'][0].update(LaunchTime=utc(296)),
             'B2 start does not follow the witnessed B1 stop'),
            ('before-start.json', lambda r: r['Reservations'][0]['Instances'][0].update(
                StateTransitionReason=f'User initiated ({gmt(401)} GMT)'), 'B2 start does not follow the witnessed B1 stop'),
            ('before-start.json', lambda r: r.update(Reservations=[]), 'malformed EC2 instance record'),
            ('ec2-capture.json', lambda r: r['BlockDeviceMappings'][0].update(DeviceName='/dev/sdb'), 'B2 attachment changed'),
            ('ec2-capture.json', lambda r: r['BlockDeviceMappings'][0]['Ebs'].update(VolumeId='vol-123'), 'changed attached volume'),
            ('ec2-capture.json', lambda r: r.update(InstanceId='i-fff'), 'changed EC2 instance/state'),
            ('final-instance.json', lambda r: r['Reservations'][0]['Instances'][0].update(LaunchTime=utc(421)),
             'B2 stop record changed launch/attachment'),
            ('final-instance.json', lambda r: r['Reservations'][0]['Instances'][0]['BlockDeviceMappings'][0].update(DeviceName='/dev/sdb'),
             'B2 stop record changed launch/attachment'),
            ('final-instance.json', lambda r: r['Reservations'][0]['Instances'][0].pop('StateReason'), 'missing witnessed B2 stop'),
            ('final-instance.json', lambda r: r['Reservations'][0]['Instances'][0]['StateReason'].update(
                Code='Server.InternalError'), 'missing witnessed B2 stop'),
            ('start.json', lambda r: r['StartingInstances'][0].update(PreviousState='stopped'), 'B2 start/stop requests differ'),
            ('final-instance.json', lambda r: r['Reservations'][0]['Instances'][0].update(
                StateTransitionReason=f'User initiated ({gmt(B2_LAUNCH)} GMT)'), 'B2 launch/stop ordering differs'),
        ]
        for name, change, message in cases:
            with self.subTest(name=name, message=message):
                self.setUp(); self.edit(name, change)
                with self.assertRaisesRegex(ValueError, message): self.failed()

    def test_capture_log_showing_a_quiet_attempt_refused(self):
        lines = CAPTURE_LOG.splitlines()
        for extra in (json.dumps({'attempt': 1, 'elapsed_seconds': 0, 'issues': [], 'kind': 'reboot_quiet_check'}),
                      '  File "/home/fixture/tools/reboot_attestation.py", line 9, in wait_for_quiet',
                      '  File "/home/fixture/tools/reboot_attestation.py", line 9, in source_census',
                      'tools/quiet_rehearsal.py: OK (census)', 'Quiet check attempt 1', 'CENSUS started'):
            with self.subTest(extra=extra):
                self.setUp(); self.set_log('\n'.join([*lines[:-1], extra, lines[-1]]) + '\n')
                with self.assertRaisesRegex(ValueError, 'B2 capture log shows a quiet check or census'): self.failed()

    def test_capture_log_requires_terminal_journalctl_refusal(self):
        lines = CAPTURE_LOG.splitlines()
        for text in ('\n'.join(lines[:-1]), '\n'.join([*lines, 'later output']),
                     '\n'.join(l for l in lines if not l.endswith('in live_observation')),
                     '\n'.join(l for l in lines if not l.endswith('in _command')),
                     '\n'.join([*lines[:-1], REFUSAL + ' (rc=1)']), ''):
            with self.subTest(text=text[-60:]):
                self.setUp(); self.set_log(text + '\n')
                with self.assertRaisesRegex(ValueError, 'B2 capture log lacks the journalctl refusal'): self.failed()

    def test_refusal_summary_census_quiet_or_archive_refused(self):
        for key, value in (('source_census_started', True), ('source_census_started', None), ('quiet_attempts', 1),
                           ('quiet_attempts', False), ('capture_archive_created', True), ('capture_archive_created', 0)):
            with self.subTest(key=key, value=value):
                self.setUp(); self.edit('refusal-summary.json', lambda s: s.update({key: value}))
                with self.assertRaisesRegex(ValueError, 'B2 refusal summary shows quiet/census/archive activity'):
                    self.failed()
        for key, value in (('start_requests', 2), ('start_requests', True), ('no_boot_retry', False), ('stopped', False)):
            with self.subTest(key=key, value=value):
                self.setUp(); self.edit('refusal-summary.json', lambda s: s.update({key: value}))
                with self.assertRaisesRegex(ValueError, 'B2 refusal summary shows a retry or no stop'): self.failed()
        for key, value in (('launch_time', utc(B2_LAUNCH + 1)), ('capture_log_sha256', 'e'*64), ('launch_time', None)):
            with self.subTest(key=key, value=value):
                self.setUp(); self.edit('refusal-summary.json', lambda s: s.update({key: value}))
                with self.assertRaisesRegex(ValueError, 'B2 refusal summary changed launch or capture log'): self.failed()
        for key, value in (('tool_commit', '0'*40), ('tool_commit', None), ('capture_exit', 0),
                           ('capture_exit', True), ('capture_exit', '1'), ('capture_exit', None)):
            with self.subTest(key=key, value=value):
                self.setUp(); self.edit('refusal-summary.json', lambda s: s.update({key: value}))
                with self.assertRaisesRegex(ValueError, 'B2 refusal summary changed tool or exit'): self.failed()
        self.setUp(); self.ffiles['refusal-summary.json'] = b'[]\n'
        with self.assertRaisesRegex(ValueError, 'B2 refusal summary shows quiet/census/archive activity'): self.failed()

    def test_b2_journal_system_and_collector_entries_accepted(self):
        self.b2_journal(*ALLOWED)
        self.assertEqual(proof.failed_boot_activity(self.observed['ended_boot_journals'][B2]), [])
        self.verdict()

    def test_b2_journal_with_a_work_process_refused(self):
        for entry in REFUSED:
            with self.subTest(entry=entry):
                self.setUp(); self.b2_journal(*ALLOWED, entry)
                with self.assertRaisesRegex(ValueError, 'B2 journal shows evaluator/session/analysis activity'):
                    self.verdict()

    def test_b2_journal_must_be_a_complete_shutdown_journal(self):
        for kind in ('torn', 'no_shutdown', 'wrong_boot', 'boundary'):
            with self.subTest(kind=kind):
                obs = copy.deepcopy(self.observed); rows = [json.loads(l) for l in obs['ended_boot_journals'][B2].splitlines()]
                if kind == 'no_shutdown': rows[-1]['_COMM'] = 'kernel'
                elif kind == 'wrong_boot': rows[-1]['_BOOT_ID'] = proof.boot_id(B3)
                elif kind == 'boundary': rows[-1]['__REALTIME_TIMESTAMP'] = '469000000'
                text = ''.join(json.dumps(r) + '\n' for r in rows)
                obs['ended_boot_journals'][B2] = text.rstrip('\n') if kind == 'torn' else text
                with self.assertRaisesRegex(ValueError, 'shutdown'): self.verdict(obs)

    def test_b2_journal_process_fields_must_be_text(self):
        for field, value in (('_COMM', ['python3']), ('_CMDLINE', [112, 121]), ('_EXE', 7), ('_UID', 0)):
            with self.subTest(field=field):
                self.setUp(); self.b2_journal(dict(b2_entry('sshd', '/usr/sbin/sshd', 'sshd', '0'), **{field: value}))
                with self.assertRaisesRegex(ValueError, 'malformed B2 journal process field'): self.verdict()

    def test_evidence_must_match_policy_kind(self):
        p, w, files, observed = fixture()
        history = proof.recovery(raw(w), files, p)
        with self.assertRaisesRegex(ValueError, 'failed-capture evidence does not match the policy boot chain'):
            proof.observations(p, history, observed, {'launch': 0, 'stop': 0, 'witness_sha256': '0'*64})
        with self.assertRaisesRegex(ValueError, 'failed-capture evidence does not match the policy boot chain'):
            proof.failed_capture(raw(self.fw), self.ffiles, p, history)
        with self.assertRaisesRegex(ValueError, 'failed-capture evidence does not match the policy boot chain'):
            proof.observations(self.p, self.history(), self.observed)

    def test_process_report_refuses_every_earlier_boot_host(self):
        for boot in (B0, B1, B2):
            with self.subTest(boot=boot), self.assertRaisesRegex(ValueError, 'not the attested capture host'):
                proof.process_report('/unused', None, host=dict(self.observed['host'], boot=boot), table=[], p=self.p)


def fake_journalctl(directory, boots):
    """A `journalctl` on PATH that parses `-b` like systemd 255: 32-hex IDs only."""
    data = directory/'data'; data.mkdir()
    (data/'list-boots').write_text(boots['list'])
    for boot, text in boots['journals'].items():
        (data/proof.boot_id(boot)).write_text(text)
    hex32 = '[0-9a-f]' * 32
    script = directory/'bin/journalctl'; script.parent.mkdir()
    script.write_text(f'''#!/bin/sh
# Synthetic systemd 255 (255.4-1ubuntu8.18) -b parser; test-generated, never committed.
DATA='{data}'
if [ "$#" -eq 3 ] && [ "$*" = "--list-boots --no-pager --utc" ]; then cat "$DATA/list-boots"; exit 0; fi
if [ "$#" -eq 7 ] && [ "$1" = "-b" ] && [ "$3 $4 $5 $6 $7" = "--all -o json --no-pager --utc" ]; then
  case "$2" in
    {hex32})
      if [ -f "$DATA/$2" ]; then cat "$DATA/$2"; exit 0; fi
      echo "Data from the specified boot ($2) is not available: No such boot ID in journal" >&2; exit 1 ;;
  esac
  echo "Failed to add match '$2': Invalid argument" >&2; exit 1
fi
echo "unexpected journalctl argv: $*" >&2; exit 2
''')
    script.chmod(0o755)
    return script.parent


class FakeJournalctlTests(unittest.TestCase):
    def run_live(self, p, observed):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            bin_dir = fake_journalctl(base, {'list': observed['journal_boot_list'],
                                             'journals': observed['ended_boot_journals']})
            root = base/'inventory'; root.mkdir(); (root/'session_exit.txt').write_text(observed['session_exit_text'])
            p = dict(p, inventory_root=str(root)); machine = b'c'*32 + b'\n'
            p['machine_id_sha256'] = proof.sha(machine)
            script = HERE/'closed_capture.py'; pid = os.getpid()
            row = dict(observed['process_table'][0], pid=pid, pgid=pid)
            identity = SimpleNamespace(host=lambda: observed['host'], table=lambda: [row])
            read_bytes, read_text, stat = Path.read_bytes, Path.read_text, Path.stat
            def stat_at(path, *args, **kwargs):
                if path == Path('/proc')/str(pid): return SimpleNamespace(st_uid=1000)
                return stat(path, *args, **kwargs)
            def bytes_at(path):
                if path == Path('/etc/machine-id'): return machine
                if path == Path('/proc')/str(pid)/'cmdline':
                    return b'/usr/bin/python3\0-I\0-B\0' + str(script).encode() + b'\0'
                return read_bytes(path)
            def text_at(path, *args, **kwargs):
                if path == Path('/proc')/str(pid)/'comm': return 'python3\n'
                return read_text(path, *args, **kwargs)
            calls = []; real = subprocess.run
            def spy(args, **kwargs):
                calls.append(list(args)); return real(args, **kwargs)
            with mock.patch.dict(os.environ, {'PATH': str(bin_dir) + os.pathsep + os.environ.get('PATH', '')}), \
                 mock.patch.object(Path, 'read_bytes', bytes_at), mock.patch.object(Path, 'read_text', text_at), \
                 mock.patch.object(Path, 'stat', stat_at), mock.patch.object(proof.subprocess, 'run', side_effect=spy), \
                 mock.patch.object(proof, 'print', create=True):
                live = proof.live_observation(p, observed['ec2'], identity, script)
            return live, calls

    def test_live_observation_runs_real_argv_through_systemd_255_parser(self):
        p3, w3, files3, obs3 = fixture()
        p4, w4, files4, obs4 = fixture(); fw, ffiles = four_boot(p4, obs4)
        for label, p, obs, boots in (('three', p3, obs3, (B0, B1)), ('four', p4, obs4, (B0, B1, B2))):
            with self.subTest(policy=label):
                live, calls = self.run_live(p, obs)
                self.assertEqual(calls, [['journalctl', '--list-boots', '--no-pager', '--utc'],
                    *(['journalctl', '-b', proof.boot_id(b), '--all', '-o', 'json', '--no-pager', '--utc'] for b in boots)])
                self.assertEqual(live['ended_boot_journals'], obs['ended_boot_journals'])
                self.assertEqual(live['journal_boot_list'], obs['journal_boot_list'])
                observed = dict(obs, **{k: live[k] for k in ('journal_boot_list', 'ended_boot_journals')})
                history = proof.recovery(raw(w3 if label == 'three' else w4), files3 if label == 'three' else files4, p)
                failed = None if label == 'three' else proof.failed_capture(raw(fw), ffiles, p, history)
                proof.observations(p, history, observed, failed)

    def test_hyphenated_boot_refusal_reports_exit_code_and_stderr(self):
        p, _, _, observed = fixture()
        with tempfile.TemporaryDirectory() as temporary:
            bin_dir = fake_journalctl(Path(temporary).resolve(), {'list': observed['journal_boot_list'],
                                                                  'journals': observed['ended_boot_journals']})
            with mock.patch.dict(os.environ, {'PATH': str(bin_dir) + os.pathsep + os.environ.get('PATH', '')}):
                with self.assertRaisesRegex(ValueError, r"^reboot attestation: read-only observation command failed: "
                                            r"journalctl \(rc=1; stderr: Failed to add match "
                                            r"'00000001-0000-0000-0000-000000000000': Invalid argument\)$"):
                    proof._command(['journalctl', '-b', B0, '--all', '-o', 'json', '--no-pager', '--utc'])
                self.assertEqual(proof._command(['journalctl', '-b', proof.boot_id(B0), '--all', '-o', 'json', '--no-pager', '--utc']),
                                 observed['ended_boot_journals'][B0])
                # Without --all journald nulls fields over 4096 bytes; the fake refuses that argv.
                with self.assertRaisesRegex(ValueError, r'rc=2; stderr: unexpected journalctl argv'):
                    proof._command(['journalctl', '-b', proof.boot_id(B0), '-o', 'json', '--no-pager', '--utc'])

    def test_command_refusal_keeps_five_stderr_lines_and_no_stdout(self):
        with self.assertRaises(ValueError) as caught:
            proof._command(['/bin/sh', '-c', 'echo stdout-secret; for i in 1 2 3 4 5 6 7; do echo err$i >&2; done; exit 3'])
        message = str(caught.exception)
        self.assertIn('(rc=3; stderr: err1 | err2 | err3 | err4 | err5)', message)
        self.assertNotIn('err6', message); self.assertNotIn('stdout-secret', message)


def write_evidence(directory, p, w, files, cloud, failed=None):
    """Retained evidence files on disk, as the operator supplies them."""
    evidence = directory/'evidence'; evidence.mkdir()
    for name, data in files.items(): (evidence/name).write_bytes(data)
    (evidence/'recovery-witness.json').write_bytes(raw(w))
    (evidence/'policy.json').write_bytes(raw(p)); (evidence/'ec2.json').write_bytes(raw(cloud))
    if failed is None:
        return evidence, None
    fw, ffiles = failed
    target = directory/'failed'; target.mkdir()
    for name, data in ffiles.items(): (target/name).write_bytes(data)
    (target/'failed-capture-witness.json').write_bytes(raw(fw))
    return evidence, target


class FailedEvidencePlumbingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()

    def context(self, p, evidence, failed):
        return proof.CaptureContext(evidence/'policy.json', proof.sha(raw(p)), evidence, evidence/'ec2.json',
                                    failed_directory=failed, observe=None)

    def test_four_boot_context_reads_and_pins_failed_capture_evidence(self):
        p, w, files, observed = fixture(); fw, ffiles = four_boot(p, observed)
        evidence, failed = write_evidence(self.base, p, w, files, observed['ec2'], (fw, ffiles))
        context = self.context(p, evidence, failed)
        self.assertEqual((context.failed['launch'], context.scope), (B2_LAUNCH, proof.FOUR_BOOT_SCOPE))
        self.assertIn('reboot/failed-capture/capture.log', context.paths)
        (failed/'operator.log').write_bytes(b'changed\n')
        with self.assertRaisesRegex(ValueError, 'policy/recovery/cloud proof changed during capture'): context.stable()

    def test_three_boot_policy_given_b2_evidence_refused(self):
        p, w, files, observed = fixture()
        p4, _, _, obs4 = fixture(); fw, ffiles = four_boot(p4, obs4)
        evidence, failed = write_evidence(self.base, p, w, files, observed['ec2'], (fw, ffiles))
        with self.assertRaisesRegex(ValueError, 'four-boot policy requires, and three-boot policy refuses'):
            self.context(p, evidence, failed)
        self.assertEqual(self.context(p, evidence, None).scope, proof.SCOPE)

    def test_four_boot_policy_without_b2_evidence_refused(self):
        p, w, files, observed = fixture(); four_boot(p, observed)
        evidence, _ = write_evidence(self.base, p, w, files, observed['ec2'])
        with self.assertRaisesRegex(ValueError, 'four-boot policy requires, and three-boot policy refuses'):
            self.context(p, evidence, None)

    def test_capture_refuses_failed_capture_evidence_without_policy(self):
        flags = SimpleNamespace(flags=SimpleNamespace(isolated=1), dont_write_bytecode=True)
        with mock.patch.object(closed_capture, 'sys', flags), \
             self.assertRaisesRegex(ValueError, 'failed-capture evidence requires a four-boot reboot policy'):
            closed_capture.capture(self.base/'missing', self.base/'missing', self.base/'out', reviewed_commit='a'*40,
                                   failed_capture_evidence=self.base)
        self.assertFalse((self.base/'out').exists())

    def test_cli_refuses_failed_capture_evidence_without_policy(self):
        result = subprocess.run([sys.executable, '-I', '-B', str(HERE/'closed_capture.py'), '--source', '/missing',
                                 '--repo', '/missing', '--out', str(self.base/'out'), '--reviewed-commit', 'a'*40,
                                 '--failed-capture-evidence', str(self.base)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('failed-capture evidence requires a four-boot reboot policy', result.stderr)
        self.assertFalse((self.base/'out').exists())
        usage = subprocess.run([sys.executable, '-I', '-B', str(HERE/'closed_capture.py'), '--help'],
                               capture_output=True, text=True)
        self.assertIn('--failed-capture-evidence', usage.stdout)

    def test_witness_builder_output_verifies_and_is_new(self):
        p, w, files, observed = fixture(); fw, ffiles = four_boot(p, observed)
        source = self.base/'records'; source.mkdir()
        for name, data in ffiles.items(): (source/name).write_bytes(data)
        (source/'before-alarm.json').write_bytes(b'{}\n')  # retained, not part of the witness
        (source/'allow-ip.log').write_bytes(b'ssh rule for 203.0.113.7/32 already present\n')  # never copied
        receipt = failed_capture_witness.build(source, self.base/'witness', **{k: p[k] for k in (
            'instance_id', 'volume_id', 'study_boot', 'recovery_boot', 'failed_capture_boot', 'recovery_witness_sha256')})
        out = Path(receipt['directory'])
        self.assertEqual({x.name for x in out.iterdir()}, proof.FAILED_CAPTURE_FILES | {'failed-capture-witness.json'})
        manifest = (out/'failed-capture-witness.json').read_bytes()
        self.assertEqual(proof.sha(manifest), receipt['sha256'])
        p['failed_capture_witness_sha256'] = receipt['sha256']
        result = proof.failed_capture(manifest, {n: (out/n).read_bytes() for n in proof.FAILED_CAPTURE_FILES},
                                      p, proof.recovery(raw(w), files, p))
        self.assertEqual(result['launch'], B2_LAUNCH)
        with self.assertRaisesRegex(ValueError, 'witness output must be new'):
            failed_capture_witness.build(source, out, **{k: p[k] for k in (
                'instance_id', 'volume_id', 'study_boot', 'recovery_boot', 'failed_capture_boot', 'recovery_witness_sha256')})
        (source/'copy.log').unlink()
        with self.assertRaisesRegex(ValueError, 'missing or redirected'):
            failed_capture_witness.build(source, self.base/'again', **{k: p[k] for k in (
                'instance_id', 'volume_id', 'study_boot', 'recovery_boot', 'failed_capture_boot', 'recovery_witness_sha256')})
        self.assertFalse((self.base/'again').exists())


def extend_four_boot(p, observed, base):
    """exported_fixture hook: refused B2 evidence on disk, capture on B3."""
    fw, ffiles = four_boot(p, observed)
    target = base/'failed'; target.mkdir()
    for name, data in ffiles.items(): (target/name).write_bytes(data)
    (target/'failed-capture-witness.json').write_bytes(raw(fw))
    return target


class FourBootCaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from test_reboot_capture import committed_test_tool, exported_fixture
        cls.temp = tempfile.TemporaryDirectory(); cls.base = Path(cls.temp.name).resolve()
        cls.repo, cls.commit = committed_test_tool(cls.base)
        cls.receipt, cls.policy_sha = exported_fixture(cls.base, extend=extend_four_boot)

    @classmethod
    def tearDownClass(cls): cls.temp.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()/'portable'; shutil.copytree(self.base/'capture', self.root)
        self.digest = self.receipt['sha256']

    def load(self):
        from test_capture_archive import PRODUCER
        return capture_archive.load(self.root, manifest_sha256=self.digest, git_repo=self.repo,
                                    reviewed_commit=PRODUCER, capture_tool_commit=self.commit,
                                    reboot_policy_sha256=self.policy_sha)

    def repin(self):
        path = self.root/'capture.json'; m = json.loads(path.read_bytes())
        census = closed_capture.census(self.root); census['files'].pop('capture.json')
        m['inventory'] = census; path.write_bytes(raw(m)); self.digest = proof.sha(path.read_bytes())

    def change_observations(self, change):
        for phase in ('before', 'after'):
            path = self.root/('processes-'+phase+'.json'); value = json.loads(path.read_bytes())
            change(value['reboot_observation']); path.write_bytes(raw(value))
        self.repin()

    def test_four_boot_portable_replay_verifies_failed_capture_witness(self):
        result = self.load()
        self.assertTrue(result['capture_verified']); self.assertFalse(result['can_launch'])
        verification = result['reboot_verification']
        self.assertEqual((verification['failed_capture_boot'], verification['scope']), (B2, proof.FOUR_BOOT_SCOPE))
        policy = json.loads((self.root/'reboot/policy.json').read_bytes())
        self.assertEqual(verification['failed_capture_witness_sha256'], policy['failed_capture_witness_sha256'])
        self.assertEqual(proof.sha((self.root/'reboot/failed-capture/witness.json').read_bytes()),
                         policy['failed_capture_witness_sha256'])
        evidence = json.loads((self.root/'processes-before.json').read_bytes())
        self.assertEqual(set(evidence['reboot_observation']['ended_boot_journals']), {B0, B1, B2})
        self.assertEqual(evidence['host']['boot'], B3)

    def test_replay_rehashes_failed_capture_files_after_repin(self):
        path = self.root/'reboot/failed-capture/capture.log'; path.write_bytes(path.read_bytes() + b'quiet\n'); self.repin()
        with self.assertRaisesRegex(ValueError, 'changed failed-capture evidence: capture.log'): self.load()

    def test_replay_refuses_missing_or_extra_failed_capture_file(self):
        (self.root/'reboot/failed-capture/copy.log').unlink(); self.repin()
        with self.assertRaisesRegex(ValueError, 'four-boot capture omits or adds failed-capture evidence'): self.load()

    def test_replay_rechecks_b2_journal_activity_after_repin(self):
        def change(obs):
            lines = obs['ended_boot_journals'][B2].splitlines()
            entry = json.dumps(b2_entry('python3', '/usr/bin/python3.12', '/usr/bin/python3 /tmp/evaluate.py', '1000'))
            obs['ended_boot_journals'][B2] = '\n'.join([lines[0], entry, lines[-1]]) + '\n'
        self.change_observations(change)
        with self.assertRaisesRegex(ValueError, 'B2 journal shows evaluator/session/analysis activity'): self.load()

    def test_replay_rechecks_b2_b3_order_after_repin(self):
        self.change_observations(lambda obs: obs.update(journal_boot_list=listing(
            [(B0, 100, 200), (B1, 300, 410), (B3, 425, 470), (B2, 505, 550)])))
        with self.assertRaisesRegex(ValueError, 'missing, extra or unobserved intermediate boot'): self.load()


if __name__ == '__main__':
    unittest.main()
