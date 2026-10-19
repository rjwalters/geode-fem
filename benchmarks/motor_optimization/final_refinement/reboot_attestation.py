"""Capture-only evidence of termination across an explicit B0 -> B1 -> B2 chain.

Original identities and journals remain unchanged. The producer verifier must
still refuse under the real B2 host; this layer supplements only its enumerated
boot-mismatch refusals. It grants no recovery or scientific launch permission.
"""
import hashlib
import importlib
import math
import os
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
import re
import subprocess
import time

import producer_source
from trace_archive import _json, _sha

POLICY_KEYS = {'schema', 'kind', 'inventory_root', 'inventory_sha256', 'session_exit_file',
               'recovery_witness_sha256', 'study_boot', 'recovery_boot',
               'host', 'machine_id_sha256', 'instance_id', 'volume_id', 'producer_commit'}
WITNESS_FILES = {'remote-sha256-before.txt', 'remote-sha256-after.txt',
                 'copy-verification.json', 'host-before-stop.json', 'ec2-observation.json',
                 'stop-response.json', 'ec2-after-stop.json', 'ec2-stopped-transition.json',
                 'alarm-restored.json'}
SCOPE = ('Tracked processes terminated by an explicitly witnessed boot transition; '
         'original same-boot verification refused. Recovery boot was copy-only. '
         'Time-bound observations, not a signature or complete process history; '
         'no recovery, scientific selection or launch permission.')


def require(condition, message):
    if not condition:
        raise ValueError('reboot attestation: ' + message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def timestamp(value):
    require(isinstance(value, str), 'missing UTC timestamp')
    try:
        parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
        require(parsed.utcoffset() is not None, 'timestamp has no timezone')
        return parsed.timestamp()
    except (TypeError, ValueError) as error:
        raise ValueError('reboot attestation: invalid timestamp') from error


def boot_id(value):
    require(isinstance(value, str) and re.fullmatch(
        r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', value),
        'invalid boot identity')
    return value.replace('-', '')


def relative(value):
    require(isinstance(value, str) and value and value != '.'
            and not PurePosixPath(value).is_absolute() and '..' not in PurePosixPath(value).parts
            and str(PurePosixPath(value)) == value, 'unsafe relative path')
    return value


def policy(raw, expected_sha256):
    require(sha(raw) == _sha(expected_sha256), 'policy differs from independent digest')
    p = _json(raw)
    require(set(p) == POLICY_KEYS and type(p['schema']) is int and p['schema'] == 1
            and p['kind'] == 'three_boot_closed_capture_policy', 'unsupported policy')
    root = PurePosixPath(p['inventory_root'])
    require(root.is_absolute() and str(root) == p['inventory_root']
            and '..' not in root.parts and root.name, 'unsafe inventory root')
    relative(p['session_exit_file'])
    for key in ('inventory_sha256', 'recovery_witness_sha256', 'machine_id_sha256'):
        _sha(p[key])
    boot_id(p['study_boot']); boot_id(p['recovery_boot'])
    require(p['study_boot'] != p['recovery_boot'], 'reused study/recovery boot')
    require(re.fullmatch(r'[0-9a-f]{40}', p['producer_commit']) is not None,
            'producer must be an exact commit')
    require(isinstance(p['host'], str) and p['host'], 'missing host')
    require(re.fullmatch(r'i-[0-9a-f]+', p['instance_id']) is not None
            and re.fullmatch(r'vol-[0-9a-f]+', p['volume_id']) is not None,
            'invalid instance/volume')
    return p


def inventory(raw, p):
    require(sha(raw) == p['inventory_sha256'], 'inventory differs from independent digest')
    require(raw.endswith(b'\n'), 'torn SHA inventory')
    prefix = PurePosixPath(p['inventory_root']).name + '/'
    result = {}
    for line in raw.decode('utf-8').splitlines():
        require(len(line) > 66 and line[64:66] == '  ', 'malformed SHA inventory')
        digest, name = _sha(line[:64]), relative(line[66:])
        require(name.startswith(prefix), 'inventory escapes original root')
        name = relative(name[len(prefix):])
        require(name not in result, 'duplicate inventory member')
        result[name] = digest
    require(bool(result), 'empty original inventory')
    return result


def boot_list(raw):
    result = []
    pattern = (r'\s*(-?\d+)\s+([0-9a-f]{32})\s+'
               r'(\w{3} \d{4}-\d\d-\d\d \d\d:\d\d:\d\d UTC)\s+'
               r'(\w{3} \d{4}-\d\d-\d\d \d\d:\d\d:\d\d UTC)\s*')
    for line in raw.splitlines():
        if not line.strip() or line.strip().startswith('IDX '):
            continue
        match = re.fullmatch(pattern, line)
        require(match is not None, 'malformed boot list')
        number, boot, first, last = match.groups()
        parse = lambda s: datetime.strptime(s, '%a %Y-%m-%d %H:%M:%S UTC').replace(
            tzinfo=timezone.utc).timestamp()
        result.append({'index': int(number), 'boot': boot, 'first': parse(first), 'last': parse(last)})
    require(result and result[-1]['index'] == 0, 'missing current boot')
    require(len({r['boot'] for r in result}) == len(result), 'duplicate boot')
    for i, row in enumerate(result):
        require(row['first'] <= row['last'], 'backwards boot interval')
        if i:
            require(row['index'] == result[i-1]['index'] + 1
                    and result[i-1]['last'] < row['first'], 'missing or overlapping boot')
    return result


def ec2(record, p, state):
    require(record.get('InstanceId') == p['instance_id'] and record.get('State') == state,
            'changed EC2 instance/state')
    volumes = [r.get('Ebs', {}) for r in record.get('BlockDeviceMappings', [])]
    require(len(volumes) == 1 and volumes[0].get('VolumeId') == p['volume_id']
            and volumes[0].get('Status') == 'attached', 'changed attached volume')
    require(record.get('PrivateDnsName', '').split('.')[0] == p['host'], 'changed EC2 host')
    return timestamp(record.get('LaunchTime'))


def recovery(witness_raw, files, p):
    """Authenticate B1 bytes before checking their semantic links."""
    require(sha(witness_raw) == p['recovery_witness_sha256'], 'changed B1 witness')
    w = _json(witness_raw)
    require(type(w.get('schema')) is int and w['schema'] == 1 and w.get('kind') == 'paper3_recovery_boot_witness',
            'unsupported recovery witness')
    for key in ('instance_id', 'volume_id', 'study_boot', 'recovery_boot'):
        require(w.get(key) == p[key], 'witness changed ' + key)
    require(w.get('original_inventory_sha256') == p['inventory_sha256'], 'witness changed inventory')
    require(set(w.get('files', {})) == WITNESS_FILES and set(files) == WITNESS_FILES,
            'missing or extra recovery evidence')
    for name, raw in files.items():
        require(w['files'][name] == {'sha256': sha(raw), 'bytes': len(raw)},
                'changed recovery evidence: ' + name)
    require(files['remote-sha256-before.txt'] == files['remote-sha256-after.txt'],
            'recovery inventory changed')
    members = inventory(files['remote-sha256-before.txt'], p)
    host = _json(files['host-before-stop.json'])
    require(host.get('boot') == p['recovery_boot'] and host.get('host') == p['host']
            and host.get('platform') == 'Linux'
            and host.get('machine_id_sha256') == p['machine_id_sha256'], 'changed recovery host')
    expected_host = {'host': p['host'], 'platform': 'linux', 'boot': p['study_boot']}
    require(set(host.get('records', {})) == {'main', 'build-record'}, 'missing B1 journal observations')
    for name, record in host['records'].items():
        terminal = 'session_finished' if name == 'main' else 'build_recorded'
        require(record.get('recorded_host') == expected_host
                and record.get('journal_sha256') == members.get(name + '/events.jsonl')
                and record.get('terminal_kinds') == [terminal], 'B1 journal binding changed')
    require(w.get('witness_statement', '').startswith(
        'B1 was recovery-copy only. I launched no evaluator, optimizer, '
        'closed-capture exporter or scientific outcome analysis on B1.'),
        'missing recovery-only witness statement')
    processes = []
    for line in host.get('process_table', '').splitlines():
        if line.split() == ['PID', 'PPID', 'COMMAND']:
            continue
        fields = line.split()
        require(len(fields) == 3 and fields[0].isdigit() and fields[1].isdigit(),
                'malformed B1 process snapshot')
        processes.append((int(fields[0]), int(fields[1]), fields[2]))
    require(processes and len({r[0] for r in processes}) == len(processes), 'missing/duplicate B1 processes')
    python = [r for r in processes if r[2].startswith('python')]
    require(len(python) == 1 and any(r[1] == python[0][0] and r[2] == 'ps' for r in processes),
            'B1 Python process is not the documented read-only collector')
    # Linux's kprobe optimizer is a kernel worker parented by kthreadd (PID 2),
    # not an optimization experiment. The saved B1 table has only PID/PPID/comm.
    require(not any(re.search(r'icem|tmux|octave|matlab|julia|cargo|uv|session|optimizer|capture|analysis',
                              r[2], re.I) and not (r[1] == 2 and r[2] == 'kprobe-optimizer')
                    for r in processes), 'B1 work process present')
    before = _json(files['ec2-observation.json']); stopped = _json(files['ec2-stopped-transition.json'])
    launch = ec2(before, p, 'running')
    require(ec2(stopped, p, 'stopped') == launch, 'B1 EC2 launch changed')
    require(before.get('BlockDeviceMappings') == stopped.get('BlockDeviceMappings'), 'B1 attachment changed')
    match = re.fullmatch(r'User initiated \((\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) GMT\)',
                         stopped.get('StateTransitionReason', ''))
    require(match is not None and stopped.get('StateReason', {}).get('Code') == 'Client.UserInitiatedShutdown',
            'missing witnessed B1 stop')
    stop = timestamp(match.group(1).replace(' ', 'T') + 'Z'); observed = timestamp(host['observed_utc'])
    require(launch < observed <= stop, 'B1 observation/stop ordering differs')
    boots = boot_list(host['journal_boot_list'])
    require([r['boot'] for r in boots[-2:]] == [boot_id(p['study_boot']), boot_id(p['recovery_boot'])],
            'B1 snapshot omitted or inserted a boot')
    require(boots[-2]['last'] < launch <= boots[-1]['first'] <= observed,
            'B1 launch does not fit boot history')
    return {'inventory': members, 'host': host, 'launch': launch, 'stop': stop,
            'boots': boots, 'ec2': before, 'scope': w['scope']}


def ended_boot(raw, row):
    require(raw and raw.endswith('\n'), 'missing/torn shutdown journal')
    entries = [_json(line) for line in raw.splitlines()]
    require(entries and all(e.get('_BOOT_ID') == row['boot'] for e in entries),
            'shutdown journal changed boot')
    stamps = []
    for entry in entries:
        value = entry.get('__REALTIME_TIMESTAMP')
        require(isinstance(value, str) and value.isdigit(), 'missing exact journal timestamp')
        stamps.append(int(value))
    require(stamps == sorted(stamps) and stamps[0] // 1_000_000 == row['first']
            and stamps[-1] // 1_000_000 == row['last'], 'shutdown boundaries differ from boot list')
    shutdown = [t for e, t in zip(entries, stamps)
                if (e.get('_COMM') in ('systemd-shutdown', 'systemd-shutdow')
                    or (e.get('_COMM') == 'systemd' and re.search(
                        r'Shutting down|Reached target .*([Ss]hutdown|[Pp]ower.[Oo]ff)', str(e.get('MESSAGE', '')))))]
    require(shutdown and 0 <= stamps[-1] - shutdown[-1] <= 120_000_000,
            'no terminal shutdown evidence')
    return stamps[0], stamps[-1]


def observations(p, history, observed):
    """Verify the exact chain and temporal file census in one capture phase."""
    host = observed.get('host', {})
    require(host.get('host') == p['host'] and host.get('platform') == 'linux'
            and observed.get('machine_id_sha256') == p['machine_id_sha256'], 'capture host changed')
    current = boot_id(host.get('boot'))
    require(current not in (boot_id(p['study_boot']), boot_id(p['recovery_boot'])), 'capture reused an earlier boot')
    boots = boot_list(observed['journal_boot_list'])
    require([r['boot'] for r in boots[-3:]] == [boot_id(p['study_boot']), boot_id(p['recovery_boot']), current],
            'missing, extra or unobserved intermediate boot')
    require([r['boot'] for r in boots[:-1]] == [r['boot'] for r in history['boots']],
            'boot history changed since recovery')
    for i, (old, new) in enumerate(zip(history['boots'], boots[:-1])):
        require(old['first'] == new['first'] and
                (old['last'] <= new['last'] if i == len(history['boots'])-1 else old['last'] == new['last']),
                'recorded boot boundary changed')
    journals = observed.get('ended_boot_journals', {})
    require(set(journals) == {p['study_boot'], p['recovery_boot']}, 'missing/extra shutdown journal')
    bounds = [ended_boot(journals[key], row) for key, row in
              zip((p['study_boot'], p['recovery_boot']), boots[-3:-1])]
    require(bounds[0][1] < bounds[1][0], 'overlapping study/recovery boots')
    launch = ec2(observed['ec2'], p, 'running')
    require(observed['ec2']['BlockDeviceMappings'] == history['ec2']['BlockDeviceMappings'],
            'capture volume attachment changed')
    now = observed.get('observed_time')
    require(type(now) in (int, float) and math.isfinite(now), 'invalid observation time')
    require(history['stop'] <= bounds[1][1] / 1_000_000 < launch <= boots[-1]['first'] <= now
            and boots[-1]['last'] <= now, 'B1 stop/B2 launch ordering differs')
    files = observed.get('source_files', {})
    require(set(files) == set(history['inventory']), 'missing or extra source file')
    for name, record in files.items():
        require(record.get('sha256') == history['inventory'][name], 'source inventory hash changed')
        require(type(record.get('size')) is int and record['size'] >= 0
                and type(record.get('mode')) is int and 0 <= record['mode'] <= 0o777
                and type(record.get('mtime_ns')) is int
                and 0 <= record['mtime_ns'] <= bounds[0][1] * 1000,
                'source file is malformed or newer than study boot end')
    exit_raw = observed.get('session_exit_text')
    require(isinstance(exit_raw, str) and p['session_exit_file'] in history['inventory']
            and sha(exit_raw.encode()) == history['inventory'][p['session_exit_file']],
            'missing or changed terminal session exit')
    fields = exit_raw.split()
    require(len(fields) == 2 and fields[0] == '0', 'session exit was not successful')
    require(bounds[0][0] / 1_000_000 <= timestamp(fields[1]) <= bounds[0][1] / 1_000_000,
            'session exit outside original study boot')
    return {'study_end_us': bounds[0][1], 'capture_boot': host['boot'], 'scope': SCOPE}


def process_report(root, verifier, *, host, table, p):
    """Retain the original refusal; never fabricate an old-boot observation."""
    require(host == {'host': p['host'], 'platform': 'linux', 'boot': host.get('boot')}
            and host['boot'] not in (p['study_boot'], p['recovery_boot']), 'not the attested capture host')
    original = verifier.verify(root, observe=lambda: table, host=lambda: host)
    read_events = importlib.import_module('session_journal').read_events
    expected_issues = []
    old_host = {'host': p['host'], 'platform': 'linux', 'boot': p['study_boot']}
    for key, summary in original.get('journals', {}).items():
        require(not summary['owner_active'] and not summary['incomplete_tail_bytes'], 'open/torn process journal')
        events = read_events(Path(root) / key / 'events.jsonl')['events']
        require(events and events[0].get('kind') == 'session_created', 'missing journal owner')
        identities = [(key + '/owner', events[0].get('owner_identity'))]
        started, finished = {}, {}
        for event in events:
            if event['kind'] == 'process_spawn_started':
                identities.append((key + '/spawn' + str(event.get('spawn_id')), event.get('identity')))
                started[event.get('spawn_id')] = event.get('identity')
            elif event['kind'] == 'process_spawn_reaped':
                finished[event.get('spawn_id')] = event.get('receipt')
        require(set(started) == set(finished), 'unfinished tracked spawn')
        for index, receipt in finished.items():
            require(isinstance(receipt, dict) and isinstance(started[index], dict)
                    and receipt.get('pid') == started[index].get('pid')
                    and receipt.get('state') == 'reaped' and type(receipt.get('returncode')) is int,
                    'missing or malformed exit receipt')
        for label, identity in identities:
            require(isinstance(identity, dict) and identity.get('host') == old_host
                    and type(identity.get('pid')) is int and identity['pid'] > 0
                    and type(identity.get('pgid')) is int and identity['pgid'] > 0
                    and isinstance(identity.get('start'), str) and identity['start'],
                    'malformed or changed original process identity')
            expected_issues.append(label + ': missing identity or changed host/boot')
    require(expected_issues and original.get('processes_stopped') is False
            and sorted(original.get('issues', [])) == sorted(expected_issues),
            'producer refusal is not exclusively the attested boot change')
    return {'source': original['source'], 'observed_time': original['observed_time'],
            'host': host, 'processes_stopped': True, 'can_launch': False,
            'termination_basis': 'attested_reboot', 'original_verifier': original, 'scope': SCOPE}


def quiet_processes(observed, *, capture_script_sha256):
    """Only the exact current exporter may be a live study-capable interpreter."""
    table = observed['process_table']; commands = observed['process_commands']
    require(isinstance(table, list) and isinstance(commands, list), 'missing process observations')
    by_pid = {}
    for row in table:
        require(type(row.get('pid')) is int and row['pid'] > 0
                and type(row.get('pgid')) is int and row['pgid'] >= 0
                and isinstance(row.get('start'), str) and row['start']
                and isinstance(row.get('state'), str) and row['state'], 'malformed observed process')
        require(row['pid'] not in by_pid, 'duplicate observed PID')
        by_pid[row['pid']] = row
    require(len(commands) == len(table) and {r['pid'] for r in commands} == set(by_pid),
            'incomplete process command census')
    collector = observed['collector']; own = collector['identity']
    require(own.get('host') == observed['host'] and own.get('pid') in by_pid
            and all(own.get(k) == by_pid[own['pid']].get(k) for k in ('pid', 'pgid', 'start'))
            and collector.get('script_sha256') == _sha(capture_script_sha256),
            'collector identity/source differs')
    script = collector.get('script')
    require(isinstance(script, str) and PurePosixPath(script).is_absolute()
            and PurePosixPath(script).name == 'closed_capture.py', 'unbound collector script')
    for row in commands:
        require(isinstance(row.get('comm'), str) and isinstance(row.get('argv'), list)
                and all(isinstance(a, str) for a in row['argv'])
                and type(row.get('disappeared')) is bool, 'malformed command observation')
        if row['pid'] == own['pid']:
            require(not row['disappeared'] and row['comm'].startswith('python')
                    and len(row['argv']) >= 4 and Path(row['argv'][0]).name.startswith('python')
                    and set(row['argv'][1:3]) == {'-I', '-B'} and row['argv'][3] == script,
                    'collector command differs (requires isolated Python with bytecode writes disabled)')
        elif not row['disappeared'] and not by_pid[row['pid']]['state'].startswith('Z'):
            name = row['comm']
            # B2 has stronger observations than B1: require kernel parent,
            # process group zero and no userspace command line as well.
            if (name == 'kprobe-optimizer' and by_pid[row['pid']].get('ppid') == 2
                    and by_pid[row['pid']]['pgid'] == 0 and row['argv'] == []):
                continue
            require(not re.search(r'python|icem|tmux|octave|matlab|julia|cargo|optimizer|capture|analysis|jupyter',
                                  name, re.I) and name not in ('uv', 'R', 'Rscript'),
                    'live evaluator/session/analysis worker: ' + name)
    return True


def source_census(root):
    """Observe original mtimes without trusting transport-modified archive times."""
    root = Path(root)
    require(root.is_absolute() and root.is_dir()
            and not any(p.is_symlink() for p in (root, *root.parents)), 'redirected inventory root')
    files, directories = {}, []
    for path in sorted(root.rglob('*')):
        require(not path.is_symlink(), 'redirected source member')
        name = relative(path.relative_to(root).as_posix())
        if path.is_dir():
            directories.append(name)
            continue
        require(path.is_file(), 'special source member')
        before = path.stat(); digest = producer_source.digest(path); after = path.stat()
        identity = lambda st: (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_mode)
        require(identity(before) == identity(after), 'source changed during census')
        files[name] = {'sha256': digest, 'size': after.st_size, 'mode': after.st_mode & 0o777,
                       'mtime_ns': after.st_mtime_ns}
    return files, directories


def archive_mapping(name, inventory_root, roots):
    original = PurePosixPath(inventory_root)/relative(name)
    matches = [(PurePosixPath(str(base)), PurePosixPath(str(target))) for base, target in roots.items()
               if original == PurePosixPath(str(base)) or PurePosixPath(str(base)) in original.parents]
    require(len(matches) <= 1, 'ambiguous portable source mapping')
    if matches:
        base, target = matches[0]
        return (target/original.relative_to(base)).as_posix()
    return 'reboot-source/' + name


def _command(args):
    env = dict(os.environ, LC_ALL='C', LANG='C', TZ='UTC')
    result = subprocess.run(args, capture_output=True, env=env, timeout=60)
    require(result.returncode == 0, 'read-only observation command failed: ' + args[0])
    return result.stdout.decode('utf-8')


def live_observation(p, ec2_record, identity, script):
    """Linux read-only collection; no evaluators, recovery, builds or journal writes."""
    files, directories = source_census(p['inventory_root'])
    boots = _command(['journalctl', '--list-boots', '--no-pager', '--utc'])
    journals = {boot: _command(['journalctl', '-b', boot, '-o', 'json', '--no-pager', '--utc'])
                for boot in (p['study_boot'], p['recovery_boot'])}
    machine = Path('/etc/machine-id').read_bytes()
    require(re.fullmatch(rb'[0-9a-f]{32}\n?', machine) is not None, 'malformed machine identity file')
    # The retained B1 witness did not label whether its hash included the final
    # newline. Both encodings identify the same 32 hex digits; record which one
    # matched instead of choosing a new machine identity on B2.
    encoding = next((name for name, value in [('file_bytes', machine), ('hex_digits', machine.strip())]
                     if sha(value) == p['machine_id_sha256']), None)
    require(encoding is not None, 'actual machine identity changed')
    host, table = identity.host(), identity.table()
    own = next((r for r in table if r['pid'] == os.getpid()), None)
    require(own is not None, 'collector absent from process table')
    commands = []
    for row in table:
        proc = Path('/proc')/str(row['pid'])
        try:
            comm = (proc/'comm').read_text().strip()
            argv = (proc/'cmdline').read_bytes().split(b'\0')
            argv = [arg.decode('utf-8') for arg in argv if arg]
            commands.append({'pid': row['pid'], 'comm': comm, 'argv': argv, 'disappeared': False})
        except FileNotFoundError:
            commands.append({'pid': row['pid'], 'comm': '', 'argv': [], 'disappeared': True})
    return {'host': host, 'machine_id_sha256': p['machine_id_sha256'],
            'machine_id_text': machine.decode(), 'machine_id_encoding': encoding,
            'journal_boot_list': boots, 'ended_boot_journals': journals, 'ec2': ec2_record,
            'source_files': files, 'source_directories': directories,
            'session_exit_text': producer_source.regular(Path(p['inventory_root'])/p['session_exit_file']).read_text(),
            'process_table': table, 'process_commands': commands,
            'collector': {'identity': dict(own, host=host), 'script': str(script),
                          'script_sha256': producer_source.digest(script)},
            'observed_time': time.time()}


def machine_observation(observed, p):
    text = observed.get('machine_id_text'); encoding = observed.get('machine_id_encoding')
    require(isinstance(text, str) and re.fullmatch(r'[0-9a-f]{32}\n?', text) is not None
            and encoding in ('file_bytes', 'hex_digits'), 'missing machine identity bytes/encoding')
    value = text if encoding == 'file_bytes' else text.strip()
    require(sha(value.encode()) == p['machine_id_sha256'], 'recorded machine identity changed')


def terminal_exit(events, observed, p):
    terminals = [e for e in events if e['kind'] == 'session_finished']
    receipt_time_ns = observed['source_files'][p['session_exit_file']]['mtime_ns']
    require(len(terminals) == 1 and type(terminals[0].get('time')) in (int, float)
            and math.isfinite(terminals[0]['time']),
            'launcher receipt predates session completion')
    numerator, denominator = terminals[0]['time'].as_integer_ratio()
    require(numerator * 1_000_000_000 <= receipt_time_ns * denominator,
            'launcher receipt predates session completion')
    # The shell receipt's text may have only whole seconds. Its file mtime is
    # the precise retained write observation; do not invent subsecond ordering
    # from truncated text or round up the B0 cutoff.
    numerator, denominator = timestamp(observed['session_exit_text'].split()[1]).as_integer_ratio()
    require(numerator * 1_000_000_000 <= receipt_time_ns * denominator,
            'launcher receipt timestamp follows its recorded file write')


class CaptureContext:
    """Pinned external evidence and fresh per-phase observations for an exporter."""
    def __init__(self, policy_path, policy_sha256, recovery_directory, ec2_path, *, observe=live_observation):
        self.paths = {'reboot/policy.json': Path(policy_path),
                      'reboot/recovery/witness.json': Path(recovery_directory)/'recovery-witness.json',
                      'reboot/ec2-capture.json': Path(ec2_path)}
        self.paths.update({'reboot/recovery/'+name: Path(recovery_directory)/name for name in WITNESS_FILES})
        self.raw = {name: producer_source.regular(path).read_bytes() for name, path in self.paths.items()}
        self.policy_sha256 = _sha(policy_sha256)
        self.p = policy(self.raw['reboot/policy.json'], self.policy_sha256)
        self.history = recovery(self.raw['reboot/recovery/witness.json'],
                                {name: self.raw['reboot/recovery/'+name] for name in WITNESS_FILES}, self.p)
        self.ec2 = _json(self.raw['reboot/ec2-capture.json'])
        self.observe = observe

    def stable(self):
        for name, path in self.paths.items():
            require(producer_source.regular(path).read_bytes() == self.raw[name],
                    'policy/recovery/cloud proof changed during capture')

    def processes(self, chain, build_root, verifier, script):
        self.stable()
        root = Path(self.p['inventory_root'])
        require(chain['source_directories'][0] == str(root/'main')
                and str(build_root) == str(root/'build-record'), 'policy changed original study/build roots')
        identity = importlib.import_module('process_identity')
        observed = self.observe(self.p, self.ec2, identity, script)
        machine_observation(observed, self.p)
        verdict = observations(self.p, self.history, observed)
        quiet_processes(observed, capture_script_sha256=producer_source.digest(script))
        reports = {path: process_report(path, verifier, host=observed['host'],
                                       table=observed['process_table'], p=self.p)
                   for path in [*chain['source_directories'], str(build_root)]}
        # Reconciliation already checks session closure. Bind its terminal event
        # timestamp to the independently inventoried successful launcher receipt.
        events = importlib.import_module('session_journal').read_events(root/'main/events.jsonl')['events']
        terminal_exit(events, observed, self.p)
        self.stable()
        return {'host': observed['host'], 'table': observed['process_table'], 'reports': reports,
                'reboot_observation': observed, 'reboot_verdict': verdict,
                'reboot_policy_sha256': self.policy_sha256}
