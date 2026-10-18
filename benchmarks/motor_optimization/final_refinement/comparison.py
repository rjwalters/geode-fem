"""Pure post-run comparisons for the frozen #1203 cohort; no FEM or selection.

An eventual adapter must verify raw-file hashes, physical settings, actual solver
options and mesh/geometry evidence before supplying these records. This module
checks the declared identities/settings and independently recomputes aggregates.
It cannot certify an execution from a JSON declaration.
"""
import copy
import math

from check_plan import make_plan, _digest
from selection import _exact, TARGETS


def finite(x):
    return type(x) in (int, float) and math.isfinite(x)


def sha(value, length=64):
    return isinstance(value, str) and len(value) == length and all(c in '0123456789abcdef' for c in value)


def reduce_record(job, row):
    if row['design_key'] != job['design_key'] or not sha(row.get('raw_sha256')):
        raise ValueError('record design/raw identity mismatch')
    cost = row.get('cost', {})
    if cost.get('counted_in_run_budget') is not False or not (cost.get('wall_seconds') is None or
            finite(cost['wall_seconds']) and cost['wall_seconds'] >= 0):
        raise ValueError('post-run cost scope or wall time is invalid')
    status = row.get('status')
    if status not in ('ok', 'failed', 'mesh_infeasible', 'unavailable'):
        raise ValueError('unknown final-check status')
    if status != 'ok':
        return {'status': status, 'reason': row.get('reason', 'unspecified'), 'cost': copy.deepcopy(cost)}
    if _exact(row.get('actual_spec')) != _exact(job['spec']):
        raise ValueError('actual check settings differ from the frozen plan')
    provenance = row.get('provenance', {})
    if (set(provenance) != {'source_commit', 'binary_sha256', 'common_physics_sha256'}
            or not sha(provenance['source_commit'], 40)
            or not all(sha(provenance[k]) for k in ('binary_sha256', 'common_physics_sha256'))):
        raise ValueError('complete shared source/binary/physics provenance is required')
    currents = row['currents']
    spec = job['spec']
    if _exact([c['fraction'] for c in currents]) != _exact(spec['current_fractions']):
        raise ValueError('current set/order differs from the requested complete sweep')
    stats = []
    for current in currents:
        values = current['torques']
        if len(values) != len(spec['angles_deg']) or not all(finite(v) for v in values):
            raise ValueError('incomplete or nonfinite torque waveform')
        mean = math.fsum(values) / len(values)
        variance = math.fsum((v - mean)**2 for v in values) / len(values)
        std = math.sqrt(variance)
        if not all(finite(current.get(k)) and math.isclose(current[k], value, rel_tol=1e-10, abs_tol=1e-14)
                   for k, value in (('mean', mean), ('std', std))):
            raise ValueError('reported current aggregates disagree with raw waveform')
        stats.append({'fraction': current['fraction'], 'mean': mean, 'std': std, 'variance': variance})
    sum_variances = math.fsum(c['variance'] for c in stats)
    if not finite(row.get('sum_variances')) or not math.isclose(row['sum_variances'], sum_variances,
                                                              rel_tol=1e-10, abs_tol=1e-14):
        raise ValueError('reported sum of variances disagrees with raw waveforms')
    geometry = row['geometry']
    if (type(geometry.get('buildable')) is not bool or not geometry.get('guards')
            or not geometry.get('discrete') or type(row.get('mesh_pass')) is not bool
            or not all(finite(g['h']) for g in geometry['guards'])
            or not all(type(g['pass']) is bool for g in geometry['discrete'])):
        raise ValueError('explicit geometry/discrete/mesh decisions are required')
    admissible = (geometry['buildable'] and all(g['h'] >= -1e-12 for g in geometry['guards'])
                  and all(g['pass'] for g in geometry['discrete']) and row['mesh_pass'])
    registered = spec['current_fractions'] == [0., .5, 1.]
    return {'status': 'ok', 'currents': stats, 'sum_variances': sum_variances,
            'F': sum_variances if registered else None,
            'mean_full': stats[-1]['mean'] if registered else None,
            'admissible': admissible, 'cost': copy.deepcopy(cost)}


def difference(left, right, reduced):
    """Signed left-minus-right changes; waveforms only at identical angles."""
    a, b = reduced[left['job_id']], reduced[right['job_id']]
    if a['status'] != 'ok' or b['status'] != 'ok':
        return {'status': 'unavailable', 'reason': 'one or both checks failed, invalid or missing'}
    if left['provenance'] != right['provenance']:
        raise ValueError('comparison mixes source/binary/physical settings')
    la, ra = left['actual_spec']['angles_deg'], right['actual_spec']['angles_deg']
    common = [angle for angle in la if angle in ra]
    if [c['fraction'] for c in a['currents']] != [c['fraction'] for c in b['currents']]:
        return {'status': 'unavailable', 'reason': 'different current sets'}
    rows = []
    for k, (x, y) in enumerate(zip(a['currents'], b['currents'])):
        delta = [left['currents'][k]['torques'][la.index(t)] - right['currents'][k]['torques'][ra.index(t)]
                 for t in common]
        rows.append({'fraction': x['fraction'], 'mean_change': x['mean'] - y['mean'],
                     'std_change': x['std'] - y['std'],
                     'waveform_change': {'status': 'ok' if common else 'unavailable',
                         'angles_deg': common, 'torques': delta,
                         'scope': 'identical angle samples only; no interpolation or index-based subtraction'}})
    return {'status': 'ok', 'direction': 'candidate_minus_reference', 'currents': rows,
            'F_change': a['F'] - b['F'] if a['F'] is not None and b['F'] is not None else None}


def target_check(candidate, reference):
    if candidate['status'] != 'ok' or reference['status'] != 'ok':
        return {'status': 'unavailable', 'reason': 'missing, failed or invalid candidate/reference'}
    if candidate['F'] is None or reference['F'] is None:
        return {'status': 'unavailable', 'reason': 'withheld-current sums are not the registered objective'}
    f0, m0 = reference['F'], reference['mean_full']
    if f0 <= 0 or m0 <= 0 or not reference['admissible']:
        return {'status': 'unavailable', 'reason': 'invalid matching-settings theta0 anchor'}
    upper = candidate['F'] + max(.1 * candidate['F'], 2.5e-7)
    feasible = candidate['admissible'] and candidate['mean_full'] >= 1.0025 * m0
    return {'status': 'ok', 'feasible': feasible, 'F_upper': upper,
            'mean_floor': 1.0025 * m0,
            'targets': {name: feasible and upper <= fraction * f0 for name, fraction in TARGETS},
            'scope': 'post-run matching-settings check only; original attainment/credit unchanged'}


def summarize(selection, records):
    """Retain every declared observation and every missing planned check."""
    plan = make_plan(selection)
    jobs = {j['job_id']: j for j in plan['jobs']}
    observed, reduced = {}, {}
    for row in records:
        key = row['job_id']
        if key not in jobs or key in observed:
            raise ValueError('unknown or duplicate check; selected outcomes cannot be replaced')
        observed[key] = copy.deepcopy(row)
        reduced[key] = reduce_record(jobs[key], row)
    for key in jobs:
        reduced.setdefault(key, {'status': 'not_run', 'reason': 'no supplied observation'})
    comparisons = {}
    for key, job in jobs.items():
        reference = job['theta0_job_id']
        pair = (difference(observed[key], observed[reference], reduced)
                if key in observed and reference in observed else
                {'status': 'unavailable', 'reason': 'missing candidate or matching reference'})
        primary = job['primary_job_id']
        base = (difference(observed[key], observed[primary], reduced)
                if primary and key in observed and primary in observed else
                {'status': 'unavailable', 'reason': 'missing primary or different current sets'})
        comparisons[key] = {'matching_theta0': pair, 'versus_primary': base,
                            'target_check': target_check(reduced[key], reduced[reference])}
    anchor = next(c['design_key'] for c in plan['cohort'] if 'theta0' in c['roles'])
    rankings = {}
    for spec in plan['specs']:
        rows = []
        for job in plan['jobs']:
            key = job['job_id']
            if job['spec']['name'] != spec['name'] or job['design_key'] == anchor:
                continue
            decision = comparisons[key]['target_check']
            if decision.get('status') == 'ok' and decision['feasible']:
                rows.append({'design_key': job['design_key'], 'F': reduced[key]['F']})
        rankings[spec['name']] = sorted(rows, key=lambda r: (r['F'], r['design_key']))
    return {'schema_version': 1, 'selection_manifest_sha256': _digest(selection),
            'plan_sha256': _digest(plan), 'observations_sha256': _digest(observed),
            'cohort': copy.deepcopy(plan['cohort']), 'observations': observed,
            'reductions': reduced, 'comparisons': comparisons, 'feasible_orderings': rankings,
            'cost_policy': 'retain individual post-run scopes; no CPU/RSS or elapsed-time summation',
            'target_credit_policy': plan['target_credit_policy'],
            'replacement_policy': plan['replacement_policy'],
            'scope': 'Pure declared-record comparisons; adapter provenance verification and FEM execution remain required.'}
