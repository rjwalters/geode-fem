"""Recompute recorded validation decisions before final-candidate selection.

Caller must establish file, run/recovery, source, anchor and geometry provenance.
This checks recorded arithmetic and mesh policy; it does not run FEM or reconstruct
geometry from coordinates. No tolerance relaxes a budget, guard or target test.
"""
import math
from geometry_evidence import verify_geometry
from selection import Design, Validation

TARGETS = {'T50': .5, 'T90': .1, 'T99': .01}
REPAIR_LABELS = ('default', 'algorithm_delaunay', 'size_factor_0.9', 'size_factor_0.8')


def number(value, name, *, minimum=None):
    if (type(value) not in (int, float) or not math.isfinite(value)
            or (minimum is not None and value < minimum)):
        raise ValueError(f'invalid {name}')
    return float(value)


def same(observed, expected, name, ulps=8):
    observed = number(observed, name)
    expected = number(expected, f'recomputed {name}')
    # Serialization and recomputation only; eligibility inequalities below are exact.
    if abs(observed - expected) > ulps * max(math.ulp(observed), math.ulp(expected)):
        raise ValueError(f'inconsistent {name}')


def flag(observed, expected, name):
    if type(observed) is not bool or observed != expected:
        raise ValueError(f'inconsistent {name}')


def sha(value):
    if (type(value) is not str or len(value) != 64
            or any(c not in '0123456789abcdef' for c in value)):
        raise ValueError('missing mesh SHA-256')
    return value


def quality(q):
    if type(q) is not dict or type(q.get('inverted')) is not int or q['inverted'] < 0:
        raise ValueError('missing mesh quality')
    lo = (None if q.get('min_angle_deg') is None and q['inverted'] > 0
          else number(q.get('min_angle_deg'), 'minimum mesh angle', minimum=0))
    hi = number(q.get('max_angle_deg'), 'maximum mesh angle', minimum=0)
    amin = number(q.get('min_area_ratio'), 'minimum mesh area ratio')
    amax = number(q.get('max_area_ratio'), 'maximum mesh area ratio')
    if hi > 180 or (lo is not None and lo > hi) or amin > amax or (q['inverted'] == 0 and amin <= 0):
        raise ValueError('contradictory mesh quality')
    return q['inverted'] == 0 and lo >= 5.


def attempts(rows, *, passed):
    if type(rows) is not list or not 1 <= len(rows) <= 4:
        raise ValueError('missing repair attempt history')
    for index, row in enumerate(rows):
        if (type(row) is not dict or type(row.get('attempt')) is not int
                or row['attempt'] != index or row.get('label') != REPAIR_LABELS[index]):
            raise ValueError('changed repair attempt order')
        expected_pass = passed and index == len(rows)-1
        if expected_pass:
            if row.get('outcome') != 'passed' or row.get('detail') is not None:
                raise ValueError('fresh mesh has no final passing repair')
        elif row.get('outcome') not in ('mesher_failed', 'policy_failed'):
            raise ValueError('repair history has an early pass or unknown failure')
        for key in ('mesh_s', 'setup_s'):
            number(row.get(key), key, minimum=0)
        if row.get('msh_sha256') is not None:
            sha(row['msh_sha256'])
        if expected_pass or row['outcome'] == 'policy_failed':
            sha(row.get('msh_sha256'))
        detail = row.get('detail')
        if row['outcome'] == 'mesher_failed' and (type(detail) is not str or not detail):
            raise ValueError('missing mesher failure detail')
        if row['outcome'] == 'policy_failed':
            if type(detail) is not dict or detail.get('kind') not in ('inverted', 'min_angle'):
                raise ValueError('missing repair policy failure detail')
            if detail['kind'] == 'inverted':
                if type(detail.get('triangles')) is not int or detail['triangles'] <= 0:
                    raise ValueError('invalid failed-repair inversion count')
                if number(detail.get('min_area_ratio'), 'failed-repair area') > 0:
                    raise ValueError('failed-repair inversion has positive area')
            elif (detail.get('threshold_deg') != 5. or
                  number(detail.get('min_angle_deg'), 'failed-repair angle', minimum=0) >= 5.):
                raise ValueError('failed repair passes its declared angle policy')
    if not passed and len(rows) != 4:
        raise ValueError('mesh-infeasible validation lacks four exhausted repair attempts')


def verify_mesh(record, reference_sha256):
    mesh = record.get('mesh')
    if type(mesh) is not dict or mesh.get('kind') not in ('own', 'moved', 'fresh'):
        raise ValueError('unknown validation mesh kind')
    if not quality(mesh.get('quality')):
        raise ValueError('successful validation failed the mesh policy')
    actual = sha(record.get('msh_sha256'))
    if mesh['kind'] != 'fresh':
        if actual != sha(reference_sha256):
            raise ValueError('moved/own mesh does not bind the registered reference')
        return True
    if actual != sha(mesh.get('msh_sha256')):
        raise ValueError('fresh mesh digest differs from the validation')
    attempts(mesh.get('attempts'), passed=True)
    if actual != mesh['attempts'][-1]['msh_sha256']:
        raise ValueError('fresh mesh differs from its passing repair attempt')
    moved = mesh.get('moved_quality')
    if quality(moved):
        raise ValueError('fresh primary mesh has no failed moved-mesh policy')
    trigger = mesh.get('trigger')
    if type(trigger) is not dict:
        raise ValueError('fresh primary mesh has no trigger')
    if moved['inverted'] > 0:
        if (trigger.get('kind') != 'inverted' or type(trigger.get('triangles')) is not int
                or trigger['triangles'] != moved['inverted']):
            raise ValueError('fresh mesh inversion trigger disagrees')
        same(trigger.get('min_area_ratio'), moved['min_area_ratio'], 'inversion trigger area')
    else:
        if trigger.get('kind') != 'min_angle' or trigger.get('threshold_deg') != 5.:
            raise ValueError('fresh mesh angle trigger disagrees')
        same(trigger.get('min_angle_deg'), moved['min_angle_deg'], 'angle trigger')
    return True


def geometry(record, guard_specs, discrete_names):
    if type(guard_specs) is not dict or not guard_specs:
        raise ValueError('missing frozen guard definitions')
    detail = record.get('geometry')
    if type(detail) is not dict or type(detail.get('guards')) is not list:
        raise ValueError('missing recorded geometry')
    rows = detail['guards']
    for row in rows:
        if type(row) is not dict:
            raise ValueError('invalid geometry guard row')
        spec = guard_specs.get(row.get('name'))
        if (type(spec) is not dict or type(spec.get('strict')) is not bool
                or type(spec.get('unit')) is not str or not spec['unit']):
            raise ValueError('unknown or incomplete guard definition')
        scale = number(spec.get('scale'), 'frozen guard scale', minimum=0)
        if scale == 0 or number(row.get('scale'), 'recorded scale') != scale or row.get('unit') != spec['unit']:
            raise ValueError('guard changed its frozen unit or scale')
        margin = row.get('margin')
        if margin is None:
            c, passed = -1., False
        else:
            margin = number(margin, 'guard margin')
            c, passed = margin / scale, margin > 0 if spec['strict'] else margin >= 0
        same(row.get('c'), c, 'normalized guard margin')
        # c is itself serialized. Use its recorded bits for the subtraction,
        # as the driver does, after checking its normalization above.
        h = number(row.get('h'), 'guard h')
        error = 8 * max(math.ulp(row['c']), math.ulp(.05), math.ulp(h))
        if abs(h - (row['c'] - .05)) > error:
            raise ValueError('inconsistent five-percent guard margin')
        flag(row.get('pass'), passed, 'construction guard')
    # Mesh-infeasible rows omit the redundant top-level geometry fields and
    # set admissible=false because the validation mesh failed. Verify their
    # separate geometry verdict without granting mesh feasibility.
    if record.get('status') == 'mesh_infeasible':
        adapted = dict(record, admissible=detail.get('credit_admissible'),
                       h_nonnegative=detail.get('admissible'),
                       min_h=min((number(r.get('h'), 'guard h') for r in rows), default=math.nan))
    else:
        adapted = record
    result = verify_geometry(adapted, guard_names=guard_specs, discrete_names=discrete_names)
    flag(detail.get('credit_admissible'), result['geometry_admissible'], 'nested credit admissibility')
    return result


def verify_validation(record, *, anchor, unit_seconds, budget_fe, guard_specs, discrete_names):
    """Return evidence for a retained successful or mesh-infeasible validation.

    anchor must be the verified effective common anchor (including the registered
    parser's permitted round trip), identical across the cohort. A post-budget
    record may satisfy the numerical target; it still earns no target credit.
    """
    if type(record) is not dict or record.get('status') not in ('ok', 'mesh_infeasible'):
        raise ValueError('unknown validation status')
    unit = number(unit_seconds, 'validation unit', minimum=0)
    budget = number(budget_fe, 'budget FE', minimum=0)
    if unit == 0 or budget == 0:
        raise ValueError('unit and budget must be positive')
    budget_seconds = number(budget*unit, 'budget seconds', minimum=0)
    seconds = number(record.get('seconds'), 'completed validation seconds', minimum=0)
    fe = number(record.get('wall_fe'), 'completed validation FE', minimum=0)
    same(fe, seconds / unit, 'validation wall-FE stamp')
    within = fe <= budget
    if within != (seconds <= budget_seconds):
        raise ValueError('ambiguous floating-point budget boundary; preserve for explicit reconciliation')
    flag(record.get('credited'), within, 'within-budget credit')
    f0 = number(anchor.get('F0'), 'anchor F0', minimum=0)
    m0 = number(anchor.get('mean0_full_current'), 'anchor mean', minimum=0)
    if f0 == 0 or m0 == 0:
        raise ValueError('anchor quantities must be positive')
    sha(anchor.get('msh_sha256'))
    geom = geometry(record, guard_specs, discrete_names)
    if record['status'] == 'mesh_infeasible':
        attempts(record.get('attempts'), passed=False)
        if record.get('F_val') is not None or record.get('mean_full') is not None:
            raise ValueError('mesh-infeasible validation has a numerical value')
        flag(record.get('admissible'), False, 'mesh-infeasible admissibility')
        flag(record.get('mean_ok'), False, 'mesh-infeasible mean verdict')
        expected = {name: False for name in TARGETS}
        mesh_pass, completed = False, False
    else:
        mesh_pass, completed = verify_mesh(record, anchor['msh_sha256']), True
        for key in ('fractions', 'means', 'stds'):
            if type(record.get(key)) is not list or len(record[key]) != 3:
                raise ValueError('missing three-current validation aggregates')
            for v in record[key]: number(v, key, minimum=0 if key == 'stds' else None)
        if record['fractions'] != [0., .5, 1.]:
            raise ValueError('changed validation currents')
        f = number(record.get('F_val'), 'validation objective', minimum=0)
        mean = number(record.get('mean_full'), 'full-current mean')
        same(f, sum(s*s for s in record['stds']), 'sum-of-variances objective')
        same(mean, record['means'][2], 'full-current mean aggregate')
        floor = 1.0025 * m0
        delta = max(.1*f, 2.5e-7)
        same(record.get('delta_F'), delta, 'target margin')
        same(record.get('F_upper'), f+delta, 'upper objective')
        same(record.get('mean_floor'), floor, 'mean floor')
        mean_ok = mean >= floor
        flag(record.get('mean_ok'), mean_ok, 'mean feasibility')
        expected = {name: f+delta <= frac*f0 and mean_ok and geom['geometry_admissible']
                    for name, frac in TARGETS.items()}
    if type(record.get('attained')) is not dict or set(record['attained']) != set(TARGETS):
        raise ValueError('missing or extra target verdict')
    for name, value in expected.items(): flag(record['attained'][name], value, name)
    return {**geom, 'completed': completed, 'mesh_pass': mesh_pass, 'within_budget': within,
            'elapsed_seconds': seconds, 'budget_seconds': budget_seconds,
            'target_credit': {name: value and within for name, value in expected.items()},
            'scope': 'Recorded validation arithmetic and mesh-policy evidence; whole-run eligibility not established.'}


def adapt_validation(record, *, context, design, **checks):
    """Convert checked evidence using an independently reconciled run context.

    The caller must bind context to a verified terminal trace/queue and recovery
    decision, and design to its full physical coordinates and settings. This
    function cannot infer those from a scalar objective or an optimizer status.
    Unknown host evidence stays excluded. The selector retains rejected rows.
    """
    required = {'run_id', 'source_sha256', 'method', 'space', 'start_index', 'repeat',
                'eligible_run', 'host_contaminated', 'stop_reason'}
    if type(context) is not dict or set(context) != required or not isinstance(design, Design):
        raise ValueError('complete reconciled context and physical design required')
    if (type(context['eligible_run']) is not bool or
            (context['host_contaminated'] is not None and type(context['host_contaminated']) is not bool)):
        raise ValueError('invalid reconciled run/host verdict')
    if type(record.get('index')) is not int or record['index'] < 0:
        raise ValueError('missing validation ordinal')
    result = verify_validation(record, **checks)
    return Validation(run_id=context['run_id'], source_sha256=context['source_sha256'],
        method=context['method'], space=context['space'], start_index=context['start_index'],
        repeat=context['repeat'], ordinal=record['index'], design=design,
        elapsed_seconds=result['elapsed_seconds'], budget_seconds=result['budget_seconds'],
        f_val=record['F_val'], mean_full=record['mean_full'], eligible_run=context['eligible_run'],
        host_contaminated=context['host_contaminated'] is not False,
        completed=result['completed'], geometry_admissible=result['geometry_admissible'],
        discrete_pass=result['discrete_pass'], mesh_pass=result['mesh_pass'],
        stop_reason=context['stop_reason'])
