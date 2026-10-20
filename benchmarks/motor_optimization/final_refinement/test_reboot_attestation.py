"""Synthetic boot evidence only; no study outcomes, cloud calls or native runs."""
import copy
from datetime import datetime, timezone
import importlib
import fcntl
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import reboot_attestation as proof
import producer_source

PRODUCER = '9486a9da58beeca91968d4d3608f06b998abff45'
B0, B1, B2, EXTRA = [f'{n:08x}-0000-0000-0000-000000000000' for n in range(1, 5)]


def raw(value):
    return (json.dumps(value, sort_keys=True) + '\n').encode()


def utc(value):
    return datetime.fromtimestamp(value, timezone.utc).isoformat()


def listing(rows):
    stamp = lambda value: datetime.fromtimestamp(value, timezone.utc).strftime('%a %Y-%m-%d %H:%M:%S UTC')
    return 'IDX BOOT ID FIRST ENTRY LAST ENTRY\n' + ''.join(
        f'{i-len(rows)+1} {boot.replace("-", "")} {stamp(first)} {stamp(last)}\n'
        for i, (boot, first, last) in enumerate(rows))


def journal(boot, first, last):
    return ''.join(json.dumps({'_BOOT_ID': boot.replace('-', ''), '__REALTIME_TIMESTAMP': str(t),
                               '_COMM': comm, 'MESSAGE': message}) + '\n'
                   for t, comm, message in [(first, 'kernel', 'Boot'),
                                            (last, 'systemd-shutdown', 'Syncing filesystems')])


def fixture():
    exit_text = '0 ' + utc(199) + '\n'
    inventory = ('a'*64 + '  exp2/main/events.jsonl\n' + 'b'*64 + '  exp2/build-record/events.jsonl\n'
                 + proof.sha(exit_text.encode()) + '  exp2/session_exit.txt\n').encode()
    p = {'schema': 1, 'kind': 'three_boot_closed_capture_policy', 'inventory_root': '/home/worker/exp2',
         'inventory_sha256': proof.sha(inventory), 'recovery_witness_sha256': '0'*64,
         'study_boot': B0, 'recovery_boot': B1, 'host': 'fixture-host',
         'machine_id_sha256': 'c'*64, 'instance_id': 'i-abc', 'volume_id': 'vol-def',
         'producer_commit': PRODUCER}
    p['session_exit_file'] = 'session_exit.txt'
    old_host = {'host': p['host'], 'platform': 'linux', 'boot': B0}
    cloud = {'InstanceId': p['instance_id'], 'State': 'running', 'LaunchTime': utc(295),
             'PrivateDnsName': p['host'] + '.internal',
             'BlockDeviceMappings': [{'Ebs': {'VolumeId': p['volume_id'], 'Status': 'attached'}}]}
    host = {'host': p['host'], 'platform': 'Linux', 'boot': B1, 'machine_id_sha256': p['machine_id_sha256'],
            'observed_utc': utc(390.5), 'journal_boot_list': listing([(B0, 100, 200), (B1, 300, 390)]),
            'records': {'main': {'recorded_host': old_host, 'journal_sha256': 'a'*64,
                                 'terminal_kinds': ['session_finished']},
                        'build-record': {'recorded_host': old_host, 'journal_sha256': 'b'*64,
                                         'terminal_kinds': ['build_recorded']}},
            'process_table': 'PID PPID COMMAND\n1 0 systemd\n100 1 python3\n101 100 ps\n'}
    stopped = dict(cloud, State='stopped', StateTransitionReason='User initiated (1970-01-01 00:06:40 GMT)',
                   StateReason={'Code': 'Client.UserInitiatedShutdown'})
    files = {name: raw({'fixture': name}) for name in proof.WITNESS_FILES}
    files.update({'remote-sha256-before.txt': inventory, 'remote-sha256-after.txt': inventory,
                  'host-before-stop.json': raw(host), 'ec2-observation.json': raw(cloud),
                  'ec2-stopped-transition.json': raw(stopped)})
    w = {'schema': 1, 'kind': 'paper3_recovery_boot_witness',
         **{key: p[key] for key in ('instance_id', 'volume_id', 'study_boot', 'recovery_boot')},
         'original_inventory_sha256': p['inventory_sha256'], 'scope': 'Synthetic witness, no real observation',
         'witness_statement': 'B1 was recovery-copy only. I launched no evaluator, optimizer, '
                              'closed-capture exporter or scientific outcome analysis on B1.',
         'files': {name: {'sha256': proof.sha(data), 'bytes': len(data)} for name, data in files.items()}}
    p['recovery_witness_sha256'] = proof.sha(raw(w))
    current_host = dict(old_host, boot=B2)
    identity = {'pid': 500, 'pgid': 500, 'ppid': 1, 'start': 'fixture capture', 'state': 'S', 'host': current_host}
    observed = {'host': current_host, 'machine_id_sha256': p['machine_id_sha256'], 'observed_time': 550.5,
                'journal_boot_list': listing([(B0, 100, 200), (B1, 300, 410), (B2, 505, 550)]),
                'ended_boot_journals': {B0: journal(B0, 100_000_001, 200_123_456),
                                       B1: journal(B1, 300_000_001, 410_654_321)},
                'ec2': dict(cloud, LaunchTime=utc(500)), 'session_exit_text': exit_text,
                'source_files': {name: {'sha256': digest, 'size': 30, 'mode': 0o644, 'mtime_ns': 200_123_456_000}
                                 for name, digest in proof.inventory(inventory, p).items()},
                'process_table': [{k: v for k, v in identity.items() if k != 'host'}],
                'process_commands': [{'pid': 500, 'comm': 'python3', 'disappeared': False,
                                      'argv': ['/usr/bin/python3', '-I', '-B', '/tools/closed_capture.py']}],
                'collector': {'identity': identity, 'script': '/tools/closed_capture.py', 'script_sha256': 'd'*64}}
    return p, w, files, observed


class AttestationTests(unittest.TestCase):
    def setUp(self):
        self.p, self.w, self.files, self.observed = fixture()

    def history(self):
        # Repinning is fixture-only: mutations must reach semantic checks.
        self.w['files'] = {name: {'sha256': proof.sha(data), 'bytes': len(data)} for name, data in self.files.items()}
        self.p['recovery_witness_sha256'] = proof.sha(raw(self.w))
        return proof.recovery(raw(self.w), self.files, self.p)

    def test_b1_launch_must_follow_b0_end_in_both_cloud_records(self):
        for name in ('ec2-observation.json','ec2-stopped-transition.json'):
            record=json.loads(self.files[name]);record['LaunchTime']=utc(150);self.files[name]=raw(record)
        with self.assertRaisesRegex(ValueError,'B1 launch does not fit boot history'):self.history()

    def test_b1_snapshot_cannot_hide_an_intermediate_boot(self):
        host=json.loads(self.files['host-before-stop.json'])
        host['journal_boot_list']=listing([(B0,100,200),(EXTRA,250,260),(B1,300,390)])
        self.files['host-before-stop.json']=raw(host)
        with self.assertRaisesRegex(ValueError,'B1 snapshot omitted or inserted a boot'):self.history()

    def test_b2_requires_unchanged_older_boot_history(self):
        history=self.history()
        history['boots'].insert(0,{'index':-2,'boot':proof.boot_id(EXTRA),'first':10.,'last':20.})
        with self.assertRaisesRegex(ValueError,'boot history changed since recovery'):
            proof.observations(self.p,history,self.observed)

    def test_work_process_names_are_shared_by_recovery_and_capture(self):
        original=json.loads(self.files['host-before-stop.json'])
        for comm in ('jupyter','R','Rscript','session','uv','python3','icem_harness_ev'):
            with self.subTest(comm=comm):
                host=dict(original,process_table=original['process_table']+f'700 1 {comm}\n')
                self.files['host-before-stop.json']=raw(host)
                message='B1 Python process' if comm=='python3' else 'B1 work process'
                with self.assertRaisesRegex(ValueError,message):self.history()
                obs=copy.deepcopy(self.observed)
                obs['process_table'].append(dict(obs['process_table'][0],pid=700,pgid=700))
                obs['process_commands'].append({'pid':700,'comm':comm,'argv':[comm],'disappeared':False})
                with self.assertRaisesRegex(ValueError,'live evaluator'):proof.quiet_processes(obs,capture_script_sha256='d'*64)

    def test_interpreter_argv_cannot_hide_behind_a_script_process_name(self):
        for argv in (['/usr/bin/python3','/tmp/worker'],['/usr/bin/env','python3','/tmp/worker'],
                     ['/usr/local/bin/jupyter','lab'],['/usr/bin/Rscript','/tmp/worker'],['/usr/bin/R','--vanilla']):
            with self.subTest(argv=argv):
                obs=copy.deepcopy(self.observed)
                obs['process_table'].append(dict(obs['process_table'][0],pid=700,pgid=700))
                obs['process_commands'].append({'pid':700,'comm':'worker','argv':argv,'disappeared':False})
                with self.assertRaisesRegex(ValueError,'live evaluator'):proof.quiet_processes(obs,capture_script_sha256='d'*64)

    def test_b2_tail_check_has_its_own_refusal(self):
        history=self.history()
        history['boots'].insert(1,{'index':-1,'boot':proof.boot_id(EXTRA),'first':250.,'last':260.})
        self.observed['journal_boot_list']=listing([(B0,100,200),(EXTRA,250,260),(B1,300,410),(B2,505,550)])
        with self.assertRaisesRegex(ValueError,'missing, extra or unobserved intermediate boot'):
            proof.observations(self.p,history,self.observed)

    def test_complete_three_boot_proof_preserves_exact_microsecond_boundary(self):
        p = proof.policy(raw(self.p), proof.sha(raw(self.p)))
        verdict = proof.observations(p, self.history(), self.observed)
        self.assertEqual(verdict['study_end_us'], 200_123_456)
        self.assertTrue(proof.quiet_processes(self.observed, capture_script_sha256='d'*64))

    def test_missing_extra_intermediate_reused_and_unobserved_boots_refused(self):
        for rows in ([(B1, 300, 410), (B2, 505, 550)],
                     [(B0, 100, 200), (B2, 505, 550)],
                     [(B0, 100, 200), (EXTRA, 250, 260), (B1, 300, 410), (B2, 505, 550)],
                     [(B0, 100, 200), (B1, 300, 410), (EXTRA, 450, 460), (B2, 505, 550)],
                     [(B0, 100, 200), (B1, 300, 410), (B1, 505, 550)],
                     [(B0, 100, 200), (B1, 300, 410), (B2, 505, 550), (EXTRA, 600, 610)]):
            with self.subTest(rows=rows):
                obs = copy.deepcopy(self.observed); obs['journal_boot_list'] = listing(rows)
                with self.assertRaisesRegex(ValueError, "duplicate boot" if len({r[0] for r in rows}) != len(rows) else "missing, extra or unobserved intermediate boot"): proof.observations(self.p, self.history(), obs)

    def test_one_nanosecond_after_study_end_refused(self):
        self.observed['source_files']['main/events.jsonl']['mtime_ns'] += 1
        with self.assertRaisesRegex(ValueError, 'newer'): proof.observations(self.p, self.history(), self.observed)

    def test_successful_exit_must_precede_study_shutdown(self):
        for text in ('1 '+utc(199)+'\n', '0 '+utc(201)+'\n', '0 '+utc(99)+'\n', ''):
            with self.subTest(text=text):
                history=self.history();observed=copy.deepcopy(self.observed)
                # Fixture-only digest replacement reaches exit semantics.
                history['inventory'][self.p['session_exit_file']]=proof.sha(text.encode())
                observed['source_files'][self.p['session_exit_file']]['sha256']=proof.sha(text.encode())
                observed['session_exit_text']=text
                with self.assertRaises(ValueError):proof.observations(self.p,history,observed)

    def test_terminal_order_uses_receipt_file_precision_not_truncated_text(self):
        self.observed['source_files'][self.p['session_exit_file']]['mtime_ns']=199_100_000_000
        events=[{'kind':'session_finished','time':199.05}]
        proof.terminal_exit(events,self.observed,self.p)
        events[0]['time']=199.2
        with self.assertRaisesRegex(ValueError,'predates'):proof.terminal_exit(events,self.observed,self.p)

    def test_source_census_refuses_symlinks_and_special_files_without_reading_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();(root/'regular').write_text('fixture')
            (root/'redirect').symlink_to(root/'regular')
            with self.assertRaisesRegex(ValueError,'redirected'):proof.source_census(root)
            (root/'redirect').unlink();os.mkfifo(root/'fifo')
            with self.assertRaisesRegex(ValueError,'special'):proof.source_census(root)

    def test_live_collector_reads_boots_proc_identity_and_exact_source_mtimes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();(root/'session_exit.txt').write_text(self.observed['session_exit_text'])
            p=dict(self.p,inventory_root=str(root));machine=b'c'*32+b'\n'
            p['machine_id_sha256']=proof.sha(machine)
            script=HERE/'closed_capture.py';pid=os.getpid()
            row=dict(self.observed['process_table'][0],pid=pid,pgid=pid)
            identity=SimpleNamespace(host=lambda:self.observed['host'],table=lambda:[row])
            read_bytes=Path.read_bytes;read_text=Path.read_text;stat=Path.stat
            def stat_at(path,*args,**kwargs):
                if path==Path('/proc')/str(pid):return SimpleNamespace(st_uid=1000)
                return stat(path,*args,**kwargs)
            def bytes_at(path):
                if path==Path('/etc/machine-id'):return machine
                if path==Path('/proc')/str(pid)/'cmdline':
                    return b'/usr/bin/python3\0-I\0-B\0'+str(script).encode()+b'\0'
                return read_bytes(path)
            def text_at(path,*args,**kwargs):
                if path==Path('/proc')/str(pid)/'comm':return 'python3\n'
                return read_text(path,*args,**kwargs)
            commands=[]
            def command(args):
                commands.append(args)
                if '--list-boots' in args:return self.observed['journal_boot_list']
                # systemd 255 accepts only the plain 32-hex boot ID form.
                return {proof.boot_id(k):v for k,v in self.observed['ended_boot_journals'].items()}[args[2]]
            ordering=[];original_wait=proof.wait_for_quiet;original_census=proof.source_census
            def wait(*args,**kwargs):
                result=original_wait(*args,**kwargs);ordering.append('quiet');return result
            def census(*args,**kwargs):
                ordering.append('census');return original_census(*args,**kwargs)
            with mock.patch.object(Path,'read_bytes',bytes_at),mock.patch.object(Path,'read_text',text_at), \
                 mock.patch.object(proof,'_command',side_effect=command),mock.patch.object(Path,'stat',stat_at), \
                 mock.patch.object(proof,'wait_for_quiet',side_effect=wait),mock.patch.object(proof,'source_census',side_effect=census):
                observed=proof.live_observation(p,self.observed['ec2'],identity,script)
            self.assertEqual(ordering,['quiet','census'])
            self.assertEqual(commands,[['journalctl','--list-boots','--no-pager','--utc'],
                ['journalctl','-b',proof.boot_id(B0),'-o','json','--no-pager','--utc'],
                ['journalctl','-b',proof.boot_id(B1),'-o','json','--no-pager','--utc']])
            self.assertEqual(observed['source_files']['session_exit.txt']['mtime_ns'],(root/'session_exit.txt').stat().st_mtime_ns)
            self.assertEqual(observed['collector']['identity']['pid'],pid)
            self.assertEqual(observed['process_commands'][0]['uid'],1000)
            proof.quiet_history(observed,proof.sha(script.read_bytes()))
            proof.machine_observation(observed,p)
            self.assertTrue(proof.quiet_processes(observed,capture_script_sha256=proof.sha(script.read_bytes())))

    def test_census_missing_extra_changed_members_refused(self):
        for kind in ('missing', 'extra', 'hash', 'size', 'mode'):
            with self.subTest(kind=kind):
                obs = copy.deepcopy(self.observed); members = obs['source_files']
                if kind == 'missing': members.pop('main/events.jsonl')
                elif kind == 'extra': members['extra'] = members['main/events.jsonl'].copy()
                else: members['main/events.jsonl'][{'hash': 'sha256'}.get(kind, kind)] = {'hash': 'f'*64, 'size': -1, 'mode': True}[kind]
                with self.assertRaises(ValueError): proof.observations(self.p, self.history(), obs)

    def test_host_machine_instance_volume_and_launch_changes_refused(self):
        for section, key, value in [('host', 'host', 'other'), ('host', 'boot', B0),
                                    ('ec2', 'InstanceId', 'i-fff'), ('ec2', 'LaunchTime', utc(299)),
                                    ('ec2', 'PrivateDnsName', 'other.internal'), ('ec2', 'State', 'stopped')]:
            with self.subTest(key=key):
                obs = copy.deepcopy(self.observed); obs[section][key] = value
                with self.assertRaises(ValueError): proof.observations(self.p, self.history(), obs)
        for kind in ('machine', 'volume'):
            obs = copy.deepcopy(self.observed)
            if kind == 'machine': obs['machine_id_sha256'] = 'e'*64
            else: obs['ec2']['BlockDeviceMappings'][0]['Ebs']['VolumeId'] = 'vol-123'
            with self.assertRaises(ValueError): proof.observations(self.p, self.history(), obs)

    def test_missing_malformed_and_changed_shutdown_evidence_refused(self):
        for kind in ('missing', 'torn', 'no_shutdown', 'timestamp', 'wrong_boot', 'backwards'):
            with self.subTest(kind=kind):
                obs = copy.deepcopy(self.observed); journals = obs['ended_boot_journals']
                if kind == 'missing': journals.pop(B0)
                elif kind == 'torn': journals[B0] = journals[B0].rstrip('\n')
                else:
                    rows = [json.loads(line) for line in journals[B0].splitlines()]
                    if kind == 'no_shutdown': rows[-1]['_COMM'] = 'kernel'
                    elif kind == 'timestamp': rows[-1]['__REALTIME_TIMESTAMP'] = '201000000'
                    elif kind == 'wrong_boot': rows[-1]['_BOOT_ID'] = B1.replace('-', '')
                    else: rows.reverse()
                    journals[B0] = ''.join(json.dumps(r)+'\n' for r in rows)
                with self.assertRaises(ValueError): proof.observations(self.p, self.history(), obs)

    def test_independent_policy_witness_and_inventory_digests_required(self):
        with self.assertRaisesRegex(ValueError, 'independent digest'): proof.policy(raw(self.p), 'e'*64)
        with self.assertRaisesRegex(ValueError, 'changed B1 witness'): proof.recovery(raw(self.w)+b' ', self.files, self.p)
        p = dict(self.p, inventory_sha256='e'*64)
        with self.assertRaisesRegex(ValueError, 'independent digest'): proof.inventory(self.files['remote-sha256-before.txt'], p)
        for name in proof.WITNESS_FILES:
            with self.subTest(name=name):
                files = dict(self.files); files[name] += b' '
                with self.assertRaisesRegex(ValueError, 'changed recovery evidence'): proof.recovery(raw(self.w), files, self.p)

    def test_inventory_paths_duplicates_and_torn_lines_refused_after_fixture_repin(self):
        line='a'*64+'  exp2/main/events.jsonl\n'
        for value in (line+line,line.rstrip('\n'),'a'*64+'  exp2/../escape\n',
                      'a'*64+'  /absolute\n','a'*64+'  other/main/events.jsonl\n','malformed\n'):
            with self.subTest(value=value):
                data=value.encode();p=dict(self.p,inventory_sha256=proof.sha(data))
                with self.assertRaises(ValueError):proof.inventory(data,p)

    def test_recovery_statement_processes_and_journal_bindings_rechecked_after_repin(self):
        for kind in ('statement', 'python', 'tmux', 'journal', 'boot'):
            with self.subTest(kind=kind):
                self.setUp(); host = json.loads(self.files['host-before-stop.json'])
                if kind == 'statement': self.w['witness_statement'] = 'unspecified'
                elif kind == 'python': host['process_table'] += '102 1 python3\n'
                elif kind == 'tmux': host['process_table'] += '102 1 tmux\n'
                elif kind == 'journal': host['records']['main']['journal_sha256'] = 'f'*64
                else: host['boot'] = B0
                self.files['host-before-stop.json'] = raw(host)
                with self.assertRaises(ValueError): self.history()

    def test_live_study_interpreters_and_workers_refused(self):
        for comm in ('python3', 'icem_harness_ev', 'tmux', 'octave', 'Rscript', 'jupyter', 'uv', 'Python3', 'CARGO'):
            with self.subTest(comm=comm):
                obs = copy.deepcopy(self.observed)
                obs['process_table'].append(dict(obs['process_table'][0], pid=700, pgid=700))
                obs['process_commands'].append({'pid': 700, 'comm': comm, 'argv': [comm], 'disappeared': False})
                with self.assertRaisesRegex(ValueError, 'live evaluator'): proof.quiet_processes(obs, capture_script_sha256='d'*64)

    def test_kernel_process_group_zero_does_not_invalidate_a_quiet_linux_snapshot(self):
        self.observed['process_table'].append({'pid':2,'ppid':0,'pgid':0,'start':'kernel boot','state':'S'})
        self.observed['process_commands'].append({'pid':2,'comm':'kthreadd','argv':[],'disappeared':False})
        self.assertTrue(proof.quiet_processes(self.observed,capture_script_sha256='d'*64))

    def test_recovery_kprobe_kernel_worker_requires_exact_name_and_kernel_parent(self):
        original = json.loads(self.files['host-before-stop.json'])
        for comm, parent, accepted in [('kprobe-optimizer', 2, True),
                                      ('kprobe-optimizer', 1, False),
                                      ('optimizer', 2, False),
                                      ('kprobe-optimizer-extra', 2, False)]:
            with self.subTest(comm=comm, parent=parent):
                host = dict(original, process_table=original['process_table'] + f'20 {parent} {comm}\n')
                self.files['host-before-stop.json'] = raw(host)
                if accepted:
                    self.history()
                else:
                    with self.assertRaisesRegex(ValueError, 'B1 work process'): self.history()

    def test_capture_kprobe_kernel_worker_requires_parent_group_and_empty_command(self):
        for change in ({}, {'ppid': 1}, {'ppid': 3}, {'ppid': None}, {'pgid': 20}, {'argv': ['optimizer']},
                       {'comm': 'optimizer'}, {'comm': 'kprobe-optimizer-extra'}):
            with self.subTest(change=change):
                obs = copy.deepcopy(self.observed)
                row = {'pid': 20, 'ppid': 2, 'pgid': 0, 'start': 'kernel boot', 'state': 'S'}
                command = {'pid': 20, 'comm': 'kprobe-optimizer', 'argv': [], 'disappeared': False}
                row.update({k: v for k, v in change.items() if k in row})
                command.update({k: v for k, v in change.items() if k in command})
                obs['process_table'].append(row); obs['process_commands'].append(command)
                if not change:
                    self.assertTrue(proof.quiet_processes(obs, capture_script_sha256='d'*64))
                else:
                    with self.assertRaisesRegex(ValueError, 'live evaluator'):
                        proof.quiet_processes(obs, capture_script_sha256='d'*64)

    def test_collector_exception_is_source_identity_and_command_bound(self):
        for kind in ('hash', 'pid', 'script', 'argv', 'missing', 'duplicate', 'bytecode'):
            with self.subTest(kind=kind):
                obs = copy.deepcopy(self.observed)
                if kind == 'hash': obs['collector']['script_sha256'] = 'e'*64
                elif kind == 'pid': obs['collector']['identity']['pid'] = 999
                elif kind == 'script': obs['collector']['script'] = '/tools/analysis.py'
                elif kind == 'argv': obs['process_commands'][0]['argv'][3] = '/tools/analysis.py'
                elif kind == 'bytecode':obs['process_commands'][0]['argv'].remove('-B')
                elif kind == 'missing': obs['process_commands'].clear()
                else: obs['process_table'].append(obs['process_table'][0].copy())
                with self.assertRaises(ValueError): proof.quiet_processes(obs, capture_script_sha256='d'*64)


class ProcessAdapterTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); (self.root/'owner.lock').touch()
        self.p, _, _, self.obs = fixture()
        self.identity = {'pid': 123, 'pgid': 123, 'start': 'old process',
                         'host': {'host': self.p['host'], 'platform': 'linux', 'boot': B0}}
        self.events = [{'sequence': 0, 'kind': 'session_created', 'process_tracking_schema': 1,
                        'owner_identity': self.identity},
                       {'sequence': 1, 'kind': 'process_spawn_intent', 'role': 'evaluator', 'command': ['fixture']},
                       {'sequence': 2, 'kind': 'process_spawn_started', 'spawn_id': 1, 'identity': copy.deepcopy(self.identity)},
                       {'sequence': 3, 'kind': 'process_spawn_reaped', 'spawn_id': 1,
                        'receipt': {'pid': 123, 'state': 'reaped', 'returncode': 0}}]

    def verify(self, suffix=b''):
        (self.root/'events.jsonl').write_bytes(b''.join(raw(e) for e in self.events)+suffix)
        with producer_source.committed_modules(HERE.parents[2], PRODUCER,
                ('process_identity', 'session_journal', 'recovery_inventory', 'process_verification')):
            verifier = importlib.import_module('process_verification')
            return proof.process_report(self.root, verifier, host=self.obs['host'], table=[], p=self.p)

    def test_original_changed_boot_refusal_is_retained_not_rewritten(self):
        result = self.verify()
        self.assertTrue(result['processes_stopped']); self.assertFalse(result['can_launch'])
        self.assertFalse(result['original_verifier']['processes_stopped'])
        self.assertEqual(result['original_verifier']['host']['boot'], B2)
        self.assertEqual(self.events[0]['owner_identity']['host']['boot'], B0)

    def test_old_boot_must_not_hide_malformed_or_changed_identity(self):
        for key, value in [('pid', True), ('pid', 0), ('pgid', -1), ('start', ''),
                           ('host', dict(self.identity['host'], boot=B1))]:
            with self.subTest(key=key, value=value):
                self.setUp(); self.events[0]['owner_identity'][key] = value
                with self.assertRaisesRegex(ValueError, "malformed or changed original process identity"): self.verify()

    def test_unfinished_spawn_missing_receipt_and_torn_journal_refused(self):
        for kind in ('no_reap', 'missing_receipt', 'bad_receipt', 'intent_only', 'torn', 'sequence',
                     'unknown_role', 'missing_intent', 'duplicate_receipt', 'bad_group', 'missing_child'):
            with self.subTest(kind=kind):
                self.setUp()
                if kind == 'no_reap': self.events.pop()
                elif kind == 'missing_receipt': self.events[-1].pop('receipt')
                elif kind == 'bad_receipt': self.events[-1]['receipt']['pid'] = 999
                elif kind == 'intent_only': self.events = self.events[:2]
                elif kind == 'sequence': self.events[-1]['sequence'] = 7
                elif kind == 'unknown_role':self.events[1]['role']='unknown'
                elif kind == 'missing_intent':self.events[1]['kind']='unrelated'
                elif kind == 'duplicate_receipt':self.events.append(dict(self.events[-1],sequence=4))
                elif kind == 'bad_group':self.events[2]['identity']['pgid']=999
                elif kind == 'missing_child':self.events[1]['role']='run';self.events[1]['child_journal']='missing'
                message = {'torn':'open/torn process journal','no_reap':'unfinished tracked spawn','missing_receipt':'missing or malformed exit receipt','bad_receipt':'missing or malformed exit receipt'}.get(kind,'producer refusal is not exclusively the attested boot change')
                with self.assertRaisesRegex(ValueError,message): self.verify(b'{' if kind == 'torn' else b'')

    def test_owner_lock_must_be_unheld_despite_reboot(self):
        with (self.root/'owner.lock').open('rb') as owner:
            fcntl.flock(owner,fcntl.LOCK_EX|fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError,'open/torn'):self.verify()


if __name__ == '__main__': unittest.main()
