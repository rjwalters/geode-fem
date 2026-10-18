"""Prepare plotting data from a hash-bound study_selection.py output.

This is a downstream presentation adapter, not a replacement for the closed
capture/scientific verification that produces its input. It reads no live traces.
"""
import hashlib
import json
import math

METHODS = {'A': ('slsqp', 'SLSQP L1'), 'B': ('cobyqa', 'COBYQA L1'),
           'C': ('surrogate', 'Empirical RBF L1'), 'D': ('cobyqa_l0', 'COBYQA L0'),
           'E': ('slsqp_l0', 'SLSQP L0')}
TARGETS = {'T50': .5, 'T90': .1, 'T99': .01}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def number(value):
    require(type(value) in (int, float) and math.isfinite(value), 'nonfinite/non-numeric value')
    return value


def same(a, b, label):
    require(math.isclose(number(a), number(b), rel_tol=1e-12, abs_tol=1e-14), label)


def load(path, expected_sha256, unit_seconds):
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == expected_sha256, 'selection digest mismatch')
    result = prepare(json.loads(raw), unit_seconds)
    result['selection_sha256'] = expected_sha256
    return result


def prepare(data, unit_seconds):
    require(number(unit_seconds) > 0, 'positive calibration unit required')
    require(data['schema'] == 1 and data['can_select'] is True, 'selection schema/eligibility')
    for key, length in [('capture_manifest_sha256', 64), ('source_commit', 40)]:
        value = data[key]
        require(isinstance(value, str) and len(value) == length and
                all(c in '0123456789abcdef' for c in value), 'invalid ' + key)
    selection = data['selection']
    f0 = number(selection['f0']); mean0 = number(selection['mean0'])
    require(f0 > 0 and mean0 > 0, 'positive anchor required')
    floor = mean0 * 1.0025
    same(selection['mean_floor'], floor, 'mean floor mismatch')
    require(set(selection['targets']) == set(TARGETS), 'target inventory mismatch')
    for name, fraction in TARGETS.items():
        same(selection['targets'][name], fraction * f0, 'target mismatch')

    attempts = {}; slots = {}; excluded = []
    for attempt in data['attempts']:
        rid = attempt['run_id']; job = attempt['job']
        require(rid not in attempts, 'duplicate attempt')
        attempts[rid] = attempt
        method, space, start, repeat = (job[k] for k in ('method_letter', 'space', 'start_index', 'repeat'))
        require(method in METHODS and space in ('a', 'b') and
                type(start) is int and 0 <= start < 5 and
                type(repeat) is int and repeat in (1, 2), 'invalid job identity')
        require(job['method'] == METHODS[method][0], 'method identity mismatch')
        require(job['start'] == ('theta0' if start == 0 else f'lhs{start}'), 'start identity mismatch')
        if not attempt['selected_completion']:
            excluded.append(attempt)
            continue
        require(attempt['selected_completion'] is True and attempt['terminal_bound'] is True and
                attempt['host_contaminated'] is False, 'selected attempt is not clean/terminal')
        slot = method, space, start, repeat
        require(slot not in slots, 'multiple selected attempts for one job')
        slots[slot] = attempt
        ledger = attempt['ledger']
        end = number(ledger['total_fe'])
        require(0 <= end <= 120 + 1e-10, 'run exceeds frozen budget')
        same(ledger['total_seconds'], end * unit_seconds, 'calibration/ledger mismatch')
    require(len(slots) == 100, 'complete 100-job selected cohort required')

    records = {}; by_run = {rid: [] for rid in attempts}
    for audit in selection['records']:
        record = audit['record']; rid = record['run_id']; ordinal = record['ordinal']
        require(rid in attempts and type(ordinal) is int and ordinal >= 0, 'unbound validation')
        key = f'{rid}:{ordinal}'
        require(key not in records, 'duplicate validation')
        records[key] = audit; by_run[rid].append(audit)
        attempt = attempts[rid]; job = attempt['job']
        require(record['source_sha256'] == attempt['trace_sha256'], 'trace identity mismatch')
        for rk, jk in [('method', 'method_letter'), ('space', 'space'),
                       ('start_index', 'start_index'), ('repeat', 'repeat')]:
            require(record[rk] == job[jk] and type(record[rk]) is type(job[jk]), 'validation job mismatch')
        require(record['eligible_run'] is attempt['selected_completion'] and
                record['host_contaminated'] is attempt['host_contaminated'], 'attempt eligibility mismatch')
        same(record['budget_seconds'], 120 * unit_seconds, 'validation budget mismatch')
        # Rejected records may contain explicit nonfinite encodings. Keep them
        # in the decision ledger; they cannot produce a curve or attainment.
        if audit['eligible'] is not True:
            require(bool(audit['rejection_reasons']) and not any(audit['attains'].values()),
                    'rejected validation carries attainment')
            continue
        require(audit['rejection_reasons'] == [], 'eligible validation has rejection reasons')
        require(all(record[k] is True for k in ('eligible_run', 'completed', 'geometry_admissible',
                                                'discrete_pass', 'mesh_pass')) and
                record['host_contaminated'] is False, 'ineligible validation credited')
        seconds = number(record['elapsed_seconds']); f = number(record['f_val'])
        require(0 <= seconds <= 120 * unit_seconds and f >= 0 and number(record['mean_full']) >= floor,
                'invalid feasible validation')
        require(seconds <= attempt['ledger']['total_seconds'] + 1e-10, 'validation after run stop')
        upper = f + max(.1 * f, 2.5e-7)
        same(audit['f_upper'], upper, 'objective/error-margin mismatch')
        require(audit['attains'] == {n: upper <= fraction * f0 for n, fraction in TARGETS.items()},
                'target attainment mismatch')

    for rid, attempt in attempts.items():
        listed = [v['record_id'] for v in attempt['validations']]
        observed = [f"{rid}:{v['record']['ordinal']}" for v in by_run[rid]]
        require(len(listed) == len(set(listed)) and set(listed) == set(observed), 'validation census mismatch')

    runs = []; table = []
    for slot, attempt in sorted(slots.items()):
        method, space, start, repeat = slot
        rows = sorted(by_run[attempt['run_id']], key=lambda v: v['record']['ordinal'])
        points = []; best = None; first = {}; previous = -math.inf
        checkpoints = {'60': None, '120': None}
        for audit in rows:
            if not audit['eligible']:
                continue
            r = audit['record']; seconds = r['elapsed_seconds']; fe = seconds / unit_seconds
            require(seconds >= previous, 'validation chronology mismatch'); previous = seconds
            upper = audit['f_upper']
            if best is None or upper < best:
                best = upper; points.append([fe, best / f0])
            for checkpoint in (60, 120):
                if fe <= checkpoint:
                    checkpoints[str(checkpoint)] = best / f0
            for name in TARGETS:
                if audit['attains'][name] and name not in first:
                    first[name] = {'fe': fe, 'seconds': seconds,
                                   'record_id': f"{attempt['run_id']}:{r['ordinal']}"}
        end = attempt['ledger']['total_fe']
        if points and points[-1][0] < end:
            points.append([end, points[-1][1]])
        run = {'run_id': attempt['run_id'], 'method': method, 'space': space, 'start_index': start,
               'repeat': repeat, 'stop': attempt['status'], 'stop_fe': end,
               'stop_seconds': attempt['ledger']['total_seconds'], 'points': points,
               'checkpoint_upper_ratio': checkpoints, 'first_attainment': first,
               'trace_sha256': attempt['trace_sha256']}
        runs.append(run)
        for name in TARGETS:
            hit = first.get(name)
            table.append({k: run[k] for k in ('run_id', 'method', 'space', 'start_index', 'repeat',
                                              'stop', 'stop_fe', 'stop_seconds', 'trace_sha256')} |
                         {'target': name, 'status': 'attained' if hit else 'not_attained',
                          'attainment_fe': hit['fe'] if hit else None,
                          'attainment_seconds': hit['seconds'] if hit else None,
                          'record_id': hit['record_id'] if hit else None})
    return {'schema': 1, 'capture_manifest_sha256': data['capture_manifest_sha256'],
            'source_commit': data['source_commit'], 'unit_seconds': unit_seconds,
            'runs': runs, 'attainment': table, 'excluded_attempts': excluded,
            'validation_decisions': selection['records'],
            'scope': 'Common-validation history from the supplied closed-study selection. '
                     'Not final-refinement certification, an end-to-end resource summary, '
                     'or independent authentication of the upstream capture.'}
