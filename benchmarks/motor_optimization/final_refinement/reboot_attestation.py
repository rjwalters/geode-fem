"""Capture-only evidence of termination across an explicit B0 -> B1 -> B2 chain.

A four-boot policy inserts a witnessed refused capture boot before the capture
boot (B0 -> B1 -> B2 refused -> B3). Original identities and journals remain unchanged. The producer verifier must
still refuse under the real B2 host; this layer supplements only its enumerated
boot-mismatch refusals. It grants no recovery or scientific launch permission.
"""
import copy
import hashlib
import importlib
import json
import math
import os
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
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
THREE_BOOT = 'three_boot_closed_capture_policy'
FOUR_BOOT = 'four_boot_closed_capture_policy'
FOUR_BOOT_KEYS = POLICY_KEYS | {'failed_capture_boot', 'failed_capture_witness_sha256'}
# Operational records of the refused Paper 3 capture boot, verified as one set.
# The operator's ingress-rule log (allow-ip.log) names a public operator IP and is
# deliberately excluded: the witness is copied into a capture that may be published.
FAILED_CAPTURE_FILES = {'refusal-summary.json', 'capture.log', 'start.json', 'before-start.json',
                        'ec2-capture.json', 'stop-response.json', 'final-instance.json',
                        'final-alarm.json', 'operator.log', 'transfer.log', 'cloud-transfer.log',
                        'copy.log', 'capture-copy-status.txt', 'operator-exit.txt',
                        'operator-sources.json'}
FAILED_CAPTURE_KIND = 'paper3_failed_capture_boot_witness'
FAILED_CAPTURE_KEYS = {'schema', 'kind', 'instance_id', 'volume_id', 'study_boot', 'recovery_boot',
                       'failed_capture_boot', 'recovery_witness_sha256', 'witness_statement', 'scope', 'files'}
# The exact reviewed collector revision that ran, and refused, on B2.
FAILED_CAPTURE_TOOL_COMMIT = 'e1814128c293e939916eb7aedce1e38d4547a48b'
FAILED_CAPTURE_STATEMENT = ('B2 was a refused capture boot; no quiet check, census, archive, '
                            'evaluator or study writes.')
# Final line of the e1814128 collector traceback on B2 (systemd 255 refused a
# hyphenated `journalctl -b` match before any quiet check or source census).
FAILED_CAPTURE_REFUSAL = 'ValueError: reboot attestation: read-only observation command failed: journalctl'
FOUR_BOOT_SCOPE = ('Tracked processes terminated by an explicitly witnessed boot transition; '
                   'original same-boot verification refused. Recovery boot was copy-only. '
                   'The first capture boot was a witnessed refusal with no quiet check, census, '
                   'archive, evaluator or study writes. '
                   'Time-bound observations, not a signature or complete process history; '
                   'no recovery, scientific selection or launch permission.')


def require(condition, message):
    if not condition:
        raise ValueError('reboot attestation: ' + message)


def work_process(name):
    """Shared B1/B2 classifier, also applied to observed interpreter argv."""
    name = PurePosixPath(name).name
    return bool(re.search(r'python|icem|tmux|octave|matlab|julia|cargo|uv|session|optimizer|capture|analysis|jupyter',
                          name, re.I)) or name in ('R', 'Rscript')


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
    require(isinstance(p, dict) and ((set(p) == POLICY_KEYS and p.get('kind') == THREE_BOOT)
                                     or (set(p) == FOUR_BOOT_KEYS and p.get('kind') == FOUR_BOOT))
            and type(p['schema']) is int and p['schema'] == 1, 'unsupported policy')
    root = PurePosixPath(p['inventory_root'])
    require(root.is_absolute() and str(root) == p['inventory_root']
            and '..' not in root.parts and root.name, 'unsafe inventory root')
    relative(p['session_exit_file'])
    for key in ('inventory_sha256', 'recovery_witness_sha256', 'machine_id_sha256'):
        _sha(p[key])
    boot_id(p['study_boot']); boot_id(p['recovery_boot'])
    require(p['study_boot'] != p['recovery_boot'], 'reused study/recovery boot')
    if p['kind'] == FOUR_BOOT:
        _sha(p['failed_capture_witness_sha256']); boot_id(p['failed_capture_boot'])
        require(p['failed_capture_boot'] not in (p['study_boot'], p['recovery_boot']),
                'four-boot policy reuses a boot')
    require(re.fullmatch(r'[0-9a-f]{40}', p['producer_commit']) is not None,
            'producer must be an exact commit')
    require(isinstance(p['host'], str) and p['host'], 'missing host')
    require(re.fullmatch(r'i-[0-9a-f]+', p['instance_id']) is not None
            and re.fullmatch(r'vol-[0-9a-f]+', p['volume_id']) is not None,
            'invalid instance/volume')
    return p


def ended_boots(p):
    """Original hyphenated UUIDs of every boot that ended before the capture boot."""
    boots = (p['study_boot'], p['recovery_boot'])
    return boots + (p['failed_capture_boot'],) if p['kind'] == FOUR_BOOT else boots


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
    require(not any(work_process(r[2]) and not r[2].startswith('python')
                    and not (r[1] == 2 and r[2] == 'kprobe-optimizer')
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


def instance(raw):
    """One full `describe-instances` record, flattened to the observation shape."""
    d = _json(raw)
    reservations = d.get('Reservations') if isinstance(d, dict) else None
    instances = (reservations[0].get('Instances') if isinstance(reservations, list) and len(reservations) == 1
                 and isinstance(reservations[0], dict) else None)
    require(isinstance(instances, list) and len(instances) == 1 and isinstance(instances[0], dict)
            and isinstance(instances[0].get('State'), dict), 'malformed EC2 instance record')
    return dict(instances[0], State=instances[0]['State'].get('Name'))


def user_stop(record):
    match = re.fullmatch(r'User initiated \((\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) GMT\)',
                         str(record.get('StateTransitionReason', '')))
    reason = record.get('StateReason')
    require(match is not None and isinstance(reason, dict) and reason.get('Code') == 'Client.UserInitiatedShutdown',
            'missing witnessed B2 stop')
    return timestamp(match.group(1).replace(' ', 'T') + 'Z')


def failed_capture(witness_raw, files, p, history):
    """Authenticate the refused B2 capture boot bytes before checking their semantic links."""
    require(p['kind'] == FOUR_BOOT, 'failed-capture evidence does not match the policy boot chain')
    require(sha(witness_raw) == p['failed_capture_witness_sha256'], 'changed B2 witness')
    w = _json(witness_raw)
    require(isinstance(w, dict) and type(w.get('schema')) is int and w['schema'] == 1
            and w.get('kind') == FAILED_CAPTURE_KIND, 'unsupported failed-capture witness')
    require(set(w) == FAILED_CAPTURE_KEYS and isinstance(w['scope'], str),
            'failed-capture witness has missing or extra fields')
    require(isinstance(w.get('files'), dict) and set(w['files']) == FAILED_CAPTURE_FILES
            and set(files) == FAILED_CAPTURE_FILES, 'missing or extra failed-capture evidence')
    for name, raw in files.items():
        require(w['files'][name] == {'sha256': sha(raw), 'bytes': len(raw)},
                'changed failed-capture evidence: ' + name)
    for key in ('instance_id', 'volume_id', 'study_boot', 'recovery_boot', 'failed_capture_boot',
                'recovery_witness_sha256'):
        require(w.get(key) == p[key], 'B2 witness changed ' + key)
    require(isinstance(w.get('witness_statement'), str)
            and w['witness_statement'].startswith(FAILED_CAPTURE_STATEMENT),
            'missing refused-capture witness statement')
    # The start/stop requests name the policy instance, from stopped and running.
    record = lambda name: (lambda d: d if isinstance(d, dict) else {})(_json(files[name]))
    started = record('start.json').get('StartingInstances')
    stopping = record('stop-response.json').get('StoppingInstances')
    state = lambda value: value.get('Name') if isinstance(value, dict) else None
    transition = lambda rows, before, after: (
        isinstance(rows, list) and len(rows) == 1 and isinstance(rows[0], dict)
        and rows[0].get('InstanceId') == p['instance_id']
        and state(rows[0].get('PreviousState')) == before
        and state(rows[0].get('CurrentState')) == after)
    require(transition(started, 'stopped', 'pending') and transition(stopping, 'running', 'stopping'),
            'B2 start/stop requests differ')
    # Immediately before B2 the instance was still in the witnessed B1 stop.
    before = instance(files['before-start.json'])
    require(ec2(before, p, 'stopped') == history['launch'] and user_stop(before) == history['stop'],
            'B2 start does not follow the witnessed B1 stop')
    running = record('ec2-capture.json')
    launch = ec2(running, p, 'running')
    require(running.get('BlockDeviceMappings') == history['ec2'].get('BlockDeviceMappings'),
            'B2 attachment changed')
    stopped = instance(files['final-instance.json'])
    require(ec2(stopped, p, 'stopped') == launch
            and stopped.get('BlockDeviceMappings') == running['BlockDeviceMappings'],
            'B2 stop record changed launch/attachment')
    stop = user_stop(stopped)
    require(history['stop'] < launch < stop, 'B2 launch/stop ordering differs')
    summary = record('refusal-summary.json')
    require(type(summary.get('quiet_attempts')) is int
            and summary['quiet_attempts'] == 0 and summary.get('source_census_started') is False
            and summary.get('capture_archive_created') is False,
            'B2 refusal summary shows quiet/census/archive activity')
    require(type(summary.get('start_requests')) is int and summary['start_requests'] == 1
            and summary.get('no_boot_retry') is True and summary.get('stopped') is True,
            'B2 refusal summary shows a retry or no stop')
    require(isinstance(summary.get('launch_time'), str) and timestamp(summary['launch_time']) == launch
            and summary.get('capture_log_sha256') == sha(files['capture.log']),
            'B2 refusal summary changed launch or capture log')
    require(summary.get('tool_commit') == FAILED_CAPTURE_TOOL_COMMIT
            and type(summary.get('capture_exit')) is int and summary['capture_exit'] == 1,
            'B2 refusal summary changed tool or exit')
    lines = [line for line in files['capture.log'].decode('utf-8').splitlines() if line.strip()]
    require(lines and lines[-1] == FAILED_CAPTURE_REFUSAL
            and any(line.strip().endswith(', in live_observation') for line in lines)
            and any(line.strip().endswith(', in _command') for line in lines),
            'B2 capture log lacks the journalctl refusal')
    # Exact tool hash-check lines name quiet_rehearsal.py; anything else that
    # mentions a quiet check or census (including its JSON log) refuses.
    hashed = re.compile(r'(tools/[A-Za-z0-9_]+\.py|policy\.json): OK')
    require(not any(re.search(r'quiet|census', line, re.I) for line in lines if not hashed.fullmatch(line)),
            'B2 capture log shows a quiet check or census')
    return {'launch': launch, 'stop': stop, 'ec2': running,
            'witness_sha256': p['failed_capture_witness_sha256']}


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


def observations(p, history, observed, failed=None):
    """Verify the exact chain and temporal file census in one capture phase."""
    four = p['kind'] == FOUR_BOOT
    require((failed is not None) == four, 'failed-capture evidence does not match the policy boot chain')
    host = observed.get('host', {})
    require(host.get('host') == p['host'] and host.get('platform') == 'linux'
            and observed.get('machine_id_sha256') == p['machine_id_sha256'], 'capture host changed')
    current = boot_id(host.get('boot'))
    earlier = ended_boots(p)
    require(current not in [boot_id(b) for b in earlier], 'capture reused an earlier boot')
    boots = boot_list(observed['journal_boot_list'])
    tail = len(earlier) + 1
    require([r['boot'] for r in boots[-tail:]] == [*(boot_id(b) for b in earlier), current],
            'missing, extra or unobserved intermediate boot')
    # The B1 snapshot ended at B1: every later boot is exactly the tail above.
    older = boots[:len(boots) - (tail - 2)]
    require([r['boot'] for r in older] == [r['boot'] for r in history['boots']],
            'boot history changed since recovery')
    for i, (old, new) in enumerate(zip(history['boots'], older)):
        require(old['first'] == new['first'] and
                (old['last'] <= new['last'] if i == len(history['boots'])-1 else old['last'] == new['last']),
                'recorded boot boundary changed')
    journals = observed.get('ended_boot_journals', {})
    require(set(journals) == set(earlier), 'missing/extra shutdown journal')
    bounds = [ended_boot(journals[key], row) for key, row in zip(earlier, boots[-tail:-1])]
    require(bounds[0][1] < bounds[1][0], 'overlapping study/recovery boots')
    if four:
        require(bounds[1][1] < bounds[2][0], 'overlapping recovery/failed-capture boots')
        busy = failed_boot_activity(journals[p['failed_capture_boot']])
        require(not busy, 'B2 journal shows evaluator/session/analysis activity: ' + json.dumps(busy, sort_keys=True))
    launch = ec2(observed['ec2'], p, 'running')
    require(observed['ec2']['BlockDeviceMappings'] == history['ec2']['BlockDeviceMappings'],
            'capture volume attachment changed')
    now = observed.get('observed_time')
    require(type(now) in (int, float) and math.isfinite(now), 'invalid observation time')
    if not four:
        require(history['stop'] <= bounds[1][1] / 1_000_000 < launch <= boots[-1]['first'] <= now
                and boots[-1]['last'] <= now, 'B1 stop/B2 launch ordering differs')
    else:
        require(history['stop'] <= bounds[1][1] / 1_000_000 < failed['launch'] <= bounds[2][0] / 1_000_000,
                'B1 stop/B2 launch/B2 first entry ordering differs')
        require(failed['stop'] <= bounds[2][1] / 1_000_000, 'B2 stop follows its journal end')
        require(bounds[2][1] / 1_000_000 < launch <= boots[-1]['first'] <= now
                and boots[-1]['last'] <= now, 'B2 journal end/B3 launch ordering differs')
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
    verdict = {'study_end_us': bounds[0][1], 'capture_boot': host['boot'], 'scope': SCOPE}
    if four:
        verdict.update(failed_capture_boot=p['failed_capture_boot'], scope=FOUR_BOOT_SCOPE,
                       failed_capture_witness_sha256=failed['witness_sha256'])
    return verdict


def process_report(root, verifier, *, host, table, p):
    """Retain the original refusal; never fabricate an old-boot observation."""
    require(host == {'host': p['host'], 'platform': 'linux', 'boot': host.get('boot')}
            and host['boot'] not in ended_boots(p), 'not the attested capture host')
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


SYSTEM_DAEMONS = {
    'unattended-upgr': (
        ('/usr/bin/python3', '/usr/share/unattended-upgrades/unattended-upgrade-shutdown', '--wait-for-signal'),),
    'networkd-dispat': (
        ('/usr/bin/python3', '/usr/bin/networkd-dispatcher', '--run-startup-triggers'),),
    'dbus-daemon': (
        # Ubuntu noble dbus.service:13 uses ExecStart=@... @dbus-daemon.
        ('@dbus-daemon', '--system', '--address=systemd:', '--nofork', '--nopidfile',
         '--systemd-activation', '--syslog-only'),
        ('/usr/bin/dbus-daemon', '--system', '--address=systemd:', '--nofork', '--nopidfile',
         '--systemd-activation', '--syslog-only'),
        ('/usr/bin/dbus-daemon', '--session', '--address=systemd:', '--nofork', '--nopidfile',
         '--systemd-activation', '--syslog-only'),),
    # No session ID, document, shell or other argument is admitted here.
    'ssm-session-wor': (('/usr/bin/ssm-session-worker',),),
}


def system_daemon(row, command):
    uid = command.get('uid')
    return (type(uid) is int and 0 <= uid < 1000 and type(row.get('ppid')) is int and row.get('ppid') == 1
            and tuple(command['argv']) in SYSTEM_DAEMONS.get(command['comm'], ()))


# Ubuntu Python system tools that may log to the B2 journal under a system UID.
# Their first two _CMDLINE words are exact; later words are their own subcommands.
B2_PYTHON_SERVICES = {
    'cloud-init': ('/usr/bin/python3', '/usr/bin/cloud-init'),
    'unattended-upgr': ('/usr/bin/python3', '/usr/bin/unattended-upgrade'),
}
# Exact whole argv only: the AWS EC2 hibernation agent (ec2-hibinit-agent), observed
# under uid 0 in B2's real journal. B2-only; the shared SYSTEM_DAEMONS are unchanged.
B2_PYTHON_EXACT = {
    'hibinit-agent': ('/usr/bin/python3', '/usr/bin/hibinit-agent', '-c', '/etc/hibinit-config.cfg'),
}
# Native study programs outside the shared interpreter word list (B2 check only).
B2_WORK_PROGRAMS = {'geode'}


def failed_boot_activity(raw):
    """Classify every B2 journal entry's process with the shared B1/B2 classifier.

    Only exact system daemons and the enumerated Ubuntu Python system tools are
    excepted. _COMM, the _EXE basename and every _CMDLINE word are checked. There
    is no collector exception: B2's real journal has no collector entry, and an
    argv-only exception cannot be bound to the hash-pinned collector source.
    """
    busy = []
    for line in raw.splitlines():
        entry = _json(line)
        require(isinstance(entry, dict), 'malformed B2 journal process field')
        comm, exe, cmdline, uid = (entry.get(k) for k in ('_COMM', '_EXE', '_CMDLINE', '_UID'))
        require(all(v is None or isinstance(v, str) for v in (comm, exe, cmdline, uid)),
                'malformed B2 journal process field')
        comm, exe, argv = comm or '', exe or '', (cmdline or '').split()
        uid = int(uid) if uid is not None and re.fullmatch(r'[0-9]+', uid) else None
        system = uid is not None and 0 <= uid < 1000
        interpreter = PurePosixPath(exe).name.startswith('python')
        # The executable must be the daemon itself, or Python for the Python daemons.
        if (system and tuple(argv) in SYSTEM_DAEMONS.get(comm, ())
                and (argv[0] == '/usr/bin/python3' if interpreter
                     else PurePosixPath(exe).name == PurePosixPath(argv[0].lstrip('@')).name)):
            continue
        if (system and comm in B2_PYTHON_SERVICES and interpreter
                and tuple(argv[:2]) == B2_PYTHON_SERVICES[comm]):
            continue
        if system and interpreter and comm in B2_PYTHON_EXACT and tuple(argv) == B2_PYTHON_EXACT[comm]:
            continue
        words = [word for word in (comm, exe, *argv) if word]
        if (comm in SYSTEM_DAEMONS or any(work_process(word) for word in words)
                or any(PurePosixPath(word).name in B2_WORK_PROGRAMS for word in words)):
            busy.append({'comm': comm, 'exe': exe, 'cmdline': cmdline, 'uid': uid})
    return busy


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
    busy = []
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
            if system_daemon(by_pid[row['pid']], row):
                continue
            daemon_program = bool(row['argv']) and PurePosixPath(row['argv'][0]).name in {'dbus-daemon', '@dbus-daemon', 'ssm-session-worker'}
            if name in SYSTEM_DAEMONS or daemon_program or any(work_process(candidate) for candidate in (name, *row['argv'][:2])):
                busy.append({k: row.get(k) for k in ('pid', 'comm', 'argv', 'uid')})
    require(not busy, 'live evaluator/session/analysis worker: ' + json.dumps(busy, sort_keys=True))
    return True


def quiet_refusals(observed, digest):
    try:
        quiet_processes(observed, capture_script_sha256=digest)
    except ValueError as error:
        prefix = 'reboot attestation: live evaluator/session/analysis worker: '
        if not str(error).startswith(prefix):
            raise  # Malformed or unbound observations are not transient work.
        return json.loads(str(error)[len(prefix):])
    return []


PROCESS_FIELDS = ('host', 'process_table', 'process_commands', 'collector')


def wait_for_quiet(observe, digest, *, sleep=time.sleep, clock=time.monotonic, log=None):
    """At most ten read-only snapshots within 600 seconds, without a new boot."""
    started = clock(); attempts = []; host = None
    for number in range(1, 11):
        snapshot = observe()
        elapsed = clock() - started
        host = snapshot['host'] if host is None else host
        require(snapshot['host'] == host, 'host/boot changed while waiting for quiet')
        require(0 <= elapsed <= 600, 'quiet check time budget exhausted')
        issues = quiet_refusals(snapshot, digest)
        record = {'attempt': number, 'elapsed_seconds': elapsed, 'issues': issues,
                  'snapshot': copy.deepcopy({k: snapshot[k] for k in PROCESS_FIELDS})}
        attempts.append(record)
        # Printed even on final refusal, before a capture archive can exist.
        message = dict(kind='reboot_quiet_check', **{k: record[k] for k in record if k != 'snapshot'})
        if log is None:
            print(json.dumps(message, sort_keys=True), file=sys.stderr, flush=True)
        else:
            log(message)
        if not issues:
            return dict(snapshot, quiet_checks=attempts)
        if number == 10 or elapsed + 60 > 600:
            break
        sleep(60)
    require(False, 'quiet check exhausted: ' + json.dumps(attempts[-1]['issues'], sort_keys=True))


def quiet_history(observed, digest, *, required=True):
    """Replay every attempt; only independently pinned pre-retry tools may omit it."""
    if 'quiet_checks' not in observed:
        require(not required, 'missing quiet check history')
        return
    records = observed['quiet_checks']
    require(isinstance(records, list) and 1 <= len(records) <= 10, 'invalid quiet check history')
    previous = None
    for number, record in enumerate(records, 1):
        elapsed = record.get('elapsed_seconds'); snapshot = record.get('snapshot', {})
        require(record.get('attempt') == number and type(elapsed) in (int, float)
                and math.isfinite(elapsed) and 0 <= elapsed <= 600
                and (previous is None or elapsed >= previous + 60), 'invalid quiet check ordering')
        require(number != 1 or elapsed < 60, 'quiet check history starts after first interval')
        require(snapshot.get('host') == observed['host'], 'quiet check host/boot changed')
        issues = quiet_refusals(snapshot, digest)
        require(record.get('issues') == issues and bool(issues) == (number < len(records)),
                'quiet check refusal log differs')
        previous = elapsed
    require(records[-1]['snapshot'] == {k: observed[k] for k in PROCESS_FIELDS},
            'quiet check final snapshot differs')


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
    # Report the exit code and the first stderr lines; never stdout or environment.
    detail = ' | '.join(line[:200] for line in result.stderr.decode('utf-8', 'replace').splitlines()[:5])
    require(result.returncode == 0, 'read-only observation command failed: ' + args[0]
            + ' (rc=' + str(result.returncode) + '; stderr: ' + detail + ')')
    return result.stdout.decode('utf-8')


def process_snapshot(identity, script):
    """Read process names, argv and effective owner; never environment values."""
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
            commands.append({'pid': row['pid'], 'comm': comm, 'argv': argv,
                             'uid': proc.stat().st_uid, 'disappeared': False})
        except FileNotFoundError:
            commands.append({'pid': row['pid'], 'comm': '', 'argv': [], 'disappeared': True})
    return {'host': host, 'process_table': table, 'process_commands': commands,
            'collector': {'identity': dict(own, host=host), 'script': str(script),
                          'script_sha256': producer_source.digest(script)}}


def live_observation(p, ec2_record, identity, script):
    """Linux read-only collection; no evaluators, recovery, builds or journal writes."""
    boots = _command(['journalctl', '--list-boots', '--no-pager', '--utc'])
    # systemd 255 parses `-b` matches as plain 32-hex IDs; hyphenated UUIDs refuse.
    # --all keeps fields over 4096 bytes (e.g. long _CMDLINE) instead of nulling them.
    journals = {boot: _command(['journalctl', '-b', boot_id(boot), '--all', '-o', 'json', '--no-pager', '--utc'])
                for boot in ended_boots(p)}
    machine = Path('/etc/machine-id').read_bytes()
    require(re.fullmatch(rb'[0-9a-f]{32}\n?', machine) is not None, 'malformed machine identity file')
    # The retained B1 witness did not label whether its hash included the final
    # newline. Both encodings identify the same 32 hex digits; record which one
    # matched instead of choosing a new machine identity on B2.
    encoding = next((name for name, value in [('file_bytes', machine), ('hex_digits', machine.strip())]
                     if sha(value) == p['machine_id_sha256']), None)
    require(encoding is not None, 'actual machine identity changed')
    snapshot = wait_for_quiet(lambda: process_snapshot(identity, script), producer_source.digest(script))
    files, directories = source_census(p['inventory_root'])
    return {**snapshot, 'machine_id_sha256': p['machine_id_sha256'],
            'machine_id_text': machine.decode(), 'machine_id_encoding': encoding,
            'journal_boot_list': boots, 'ended_boot_journals': journals, 'ec2': ec2_record,
            'source_files': files, 'source_directories': directories,
            'session_exit_text': producer_source.regular(Path(p['inventory_root'])/p['session_exit_file']).read_text(),
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
    def __init__(self, policy_path, policy_sha256, recovery_directory, ec2_path, *,
                 failed_directory=None, observe=live_observation):
        self.paths = {'reboot/policy.json': Path(policy_path),
                      'reboot/recovery/witness.json': Path(recovery_directory)/'recovery-witness.json',
                      'reboot/ec2-capture.json': Path(ec2_path)}
        self.paths.update({'reboot/recovery/'+name: Path(recovery_directory)/name for name in WITNESS_FILES})
        self.raw = {name: producer_source.regular(path).read_bytes() for name, path in self.paths.items()}
        self.policy_sha256 = _sha(policy_sha256)
        self.p = policy(self.raw['reboot/policy.json'], self.policy_sha256)
        require((failed_directory is not None) == (self.p['kind'] == FOUR_BOOT),
                'four-boot policy requires, and three-boot policy refuses, failed-capture evidence')
        self.history = recovery(self.raw['reboot/recovery/witness.json'],
                                {name: self.raw['reboot/recovery/'+name] for name in WITNESS_FILES}, self.p)
        self.failed = None
        if failed_directory is not None:
            failed = {'reboot/failed-capture/witness.json': Path(failed_directory)/'failed-capture-witness.json'}
            failed.update({'reboot/failed-capture/'+name: Path(failed_directory)/name
                           for name in FAILED_CAPTURE_FILES})
            self.paths.update(failed)
            self.raw.update({name: producer_source.regular(path).read_bytes() for name, path in failed.items()})
            self.failed = failed_capture(self.raw['reboot/failed-capture/witness.json'],
                                         {name: self.raw['reboot/failed-capture/'+name]
                                          for name in FAILED_CAPTURE_FILES}, self.p, self.history)
        self.scope = FOUR_BOOT_SCOPE if self.failed is not None else SCOPE
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
        verdict = observations(self.p, self.history, observed, self.failed)
        quiet_processes(observed, capture_script_sha256=producer_source.digest(script))
        quiet_history(observed, producer_source.digest(script))
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
