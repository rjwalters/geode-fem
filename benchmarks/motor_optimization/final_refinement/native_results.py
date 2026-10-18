"""Bind native post-run files to a frozen plan and produce comparison records.

This joins existing components; it does not authenticate a closed main capture,
select scientific candidates, launch processes, or turn declared provenance into
an execution receipt. The complete driver must supply those verified bindings.
"""
import copy
import hashlib
import json
from pathlib import Path

from check_plan import make_plan
from comparison import reduce_record, summarize, sha
from design_evidence import SCALARS, OFFSETS, bits, _bound_json
from selection import Design, _exact
from validation_evidence import geometry, number, same, verify_mesh, REPAIR_LABELS


def _file(path, expected):
    path = Path(path)
    if not sha(expected) or path.is_symlink() or not path.is_file():
        raise ValueError('missing or redirected bound artifact')
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected or path.read_bytes() != raw:
        raise ValueError('bound artifact changed')
    return raw


def _provenance(value):
    if (type(value) is not dict or set(value) != {'source_commit', 'binary_sha256', 'common_physics_sha256'}
            or not sha(value['source_commit'], 40)
            or not all(sha(value[k]) for k in ('binary_sha256', 'common_physics_sha256'))):
        raise ValueError('incomplete execution provenance')
    return copy.deepcopy(value)


def _design(adapted, definitions):
    design = Design.from_values(adapted['parameters'], adapted['settings'])
    if design != adapted['design']:
        raise ValueError('adapted parameters/settings disagree with design identity')
    space = adapted['settings']['design_space']
    definition = json.loads(definitions.spaces_json)[space]
    native_names = list(SCALARS[:9] + (OFFSETS if space == 'b' else ()) + SCALARS[9:])
    if (set(adapted['parameters']) != set(SCALARS + OFFSETS)
            or adapted['native_point_names'] != native_names
            or _exact(adapted['native_point_bits']) != _exact(bits([adapted['parameters'][n] for n in native_names]))
            or _exact(adapted['x_bits']) != _exact(bits([adapted['parameters'][n] for n in definition['free']]))
            or adapted['definition_sha256'] != {'reference': definitions.reference_sha256, 'starts': definitions.starts_sha256}
            or adapted['settings']['reference_sha256'] != definitions.reference_sha256
            or adapted['settings']['motion_map'] != definition['map']):
        raise ValueError('changed registered coordinate mapping')
    return design, space, definition


def requests(selection, designs, definitions, meshes):
    """Return exact requests for all six planned checks, without executing them.

    `meshes` maps levels 2/3 to {path, sha256}; its theta0-construction provenance
    must already have been verified. `designs` contains RegisteredDesigns output.
    """
    plan = make_plan(selection)
    if set(designs) != {row['design_key'] for row in plan['cohort']}:
        raise ValueError('design mapping differs from frozen cohort')
    mesh_hashes = {}
    for level in (2, 3):
        m = meshes[level]
        _file(m['path'], m['sha256'])
        mesh_hashes[level] = m['sha256']
    entries = {row['design_key']: row for row in plan['cohort']}
    output = {}
    for job in plan['jobs']:
        adapted = designs[job['design_key']]
        design, space, _ = _design(adapted, definitions)
        if (adapted['settings']['validation_reference_mesh_sha256'] != mesh_hashes[2]
                or design.canonical != entries[job['design_key']]['canonical_design']
                or design.key != job['design_key'] or design.settings_key != job['spec']['base_settings_sha256']):
            raise ValueError('request changes frozen design/settings')
        output[job['job_id']] = {
            'schema': 'icem-final-refinement/1', 'cost_scope': 'post_run',
            'space': space, 'check': job['spec']['name'], 'design_key': design.key,
            'base_settings_sha256': design.settings_key,
            'reference_data_sha256': definitions.reference_sha256,
            'reference_mesh_sha256': mesh_hashes[job['spec']['level']],
            'x_bits': copy.deepcopy(adapted['x_bits']),
            'point_names': list(adapted['native_point_names']),
            'point_bits': list(adapted['native_point_bits']),
        }
    return {'plan': plan, 'requests': output, 'can_launch': False,
            'scope': 'Exact request preparation only; closed-capture and execution authorization are separate.'}


def _config(spec, adapted):
    config = copy.deepcopy(adapted['settings']['common_validator'])
    config.update(name=spec['name'], level=spec['level'], angles_deg=spec['angles_deg'],
                  fractions=spec['current_fractions'], newton_rel_tol=spec['newton_rel_tol'])
    return config


def _resources(raw):
    r = raw.get('resources')
    if type(r) is not dict or r.get('label') != 'post_run_refinement' or r.get('source') != 'nix::getrusage':
        raise ValueError('missing native resource scope')
    if r.get('aggregation') != 'self plus reaped children once; do not add nested request totals':
        raise ValueError('changed resource aggregation')
    if (type(r.get('children_complete')) is not bool or type(r.get('cpu_complete')) is not bool
            or r['cpu_complete'] != (r['children_complete'] and r.get('error') is None)):
        raise ValueError('contradictory resource completeness')
    if (type(r.get('before', {}).get('pid')) is not int or r['before']['pid'] <= 0
            or r['before']['pid'] != r.get('after', {}).get('pid')):
        raise ValueError('resource process identity changed')
    observed = r.get('observed_cpu_seconds')
    if observed is not None:
        totals = []
        for who in ('self', 'reaped_children'):
            before, after, cpu = r['before'][who], r['after'][who], r['cpu'][who]
            if before.get('available') is not True or after.get('available') is not True:
                raise ValueError('CPU totals lack available counters')
            values = []
            for part in ('user_seconds', 'system_seconds'):
                delta = number(after.get(part), part, minimum=0)-number(before.get(part), part, minimum=0)
                if delta < 0:
                    raise ValueError('CPU counters decreased')
                same(cpu.get(part), delta, 'CPU interval')
                values.append(delta)
            same(cpu.get('total_seconds'), sum(values), 'CPU subtotal')
            totals.append(sum(values))
        same(observed, sum(totals), 'CPU total')
    elif r['cpu_complete']:
        raise ValueError('complete CPU observation has no total')
    return {'counted_in_run_budget': False,
            'wall_seconds': number(raw.get('wall_seconds'), 'post-run wall time', minimum=0),
            'native_resources': copy.deepcopy(r)}


def _mesh_files(raw, meshes):
    """Verify every retained generated-mesh digest as well as the final mesh."""
    records = raw.get('fresh_mesh_records')
    if type(records) is not list:
        raise ValueError('missing native mesh records')
    for index, record in enumerate(records):
        if (type(record.get('attempt')) is not int or record['attempt'] != index
                or index >= len(REPAIR_LABELS) or record.get('label') != REPAIR_LABELS[index]):
            raise ValueError('native mesh record order changed')
        number(record.get('seconds'), 'native mesher time', minimum=0)
        digest = record.get('sha256')
        if digest is not None:
            if digest not in meshes:
                raise ValueError('missing retained mesh bytes')
            _file(meshes[digest], digest)
    digest = raw['validation']['msh_sha256']
    if digest not in meshes:
        raise ValueError('missing final mesh bytes')
    _file(meshes[digest], digest)
    if raw['validation']['mesh']['kind'] == 'fresh':
        attempts = raw['validation']['mesh']['attempts']
        if (len(records) != len(attempts) or records[-1].get('sha256') != digest
                or records[-1].get('error') is not None
                or [r.get('sha256') for r in records] != [a['msh_sha256'] for a in attempts]
                or _exact(raw.get('pre_solve_mesh_attempts')) != _exact(attempts)):
            raise ValueError('fresh mesh records disagree with repair history')
    elif records or raw.get('pre_solve_mesh_attempts') != []:
        raise ValueError('own/moved check has unexpected fresh-mesh execution')


def _observation(job, request, adapted, definitions, artifact, *, provenance, guard_specs, discrete_names):
    """Read a hash-bound native result and independently validate its contents.

    Artifact: {path, sha256, geometry:{path,sha256,space}, meshes:{sha:path}}.
    The caller must bind these expected hashes and execution provenance to its
    verified process/capture receipts. This reader never infers that authority.
    """
    raw = _bound_json(artifact['path'], artifact['sha256'])
    if (raw.get('schema') != 'icem-final-refinement/1' or raw.get('cost_scope') != 'post_run'
            or raw.get('target_credit_policy') != 'preserve_original_common_validation'
            or _exact(raw.get('request')) != _exact(request)):
        raise ValueError('native result changes request or cost/credit scope')
    request_path = Path(artifact['path']).parent/'request.json'
    stored = _bound_json(request_path, raw.get('request_sha256'))
    if _exact(stored) != _exact(request):
        raise ValueError('retained request differs from frozen request')
    cost = _resources(raw)
    row = {'job_id': job['job_id'], 'design_key': job['design_key'], 'raw_sha256': artifact['sha256'],
           'cost': cost, 'provenance': _provenance(provenance)}
    if raw.get('ok') is False:
        if raw.get('status') not in ('failed', 'refused', 'unavailable') or not isinstance(raw.get('error'), str) or not raw['error']:
            raise ValueError('failed native check has no explicit reason')
        row.update(status='unavailable' if raw['status']=='unavailable' else 'failed', reason=raw['error'],
                   native_status=raw['status'], failure_evidence=copy.deepcopy(raw))
        return row
    if raw.get('ok') is not True or raw.get('status') != 'complete':
        raise ValueError('native result is not a terminal outcome')
    _, space, definition = _design(adapted, definitions)
    expected_config = _config(job['spec'], adapted)
    v = raw.get('validation')
    if (type(v) is not dict or _exact(raw.get('config')) != _exact(expected_config)
            or _exact(v.get('config')) != _exact(expected_config)
            or _exact(raw.get('space_definition')) != _exact(definition)
            or _exact(raw.get('x_bits')) != _exact(request['x_bits']) or _exact(raw.get('point_bits')) != _exact(request['point_bits'])
            or raw.get('point_names') != request['point_names'] or v.get('point_names') != request['point_names']
            or v.get('space') != definition['map'] or bits(v.get('point', [])) != request['point_bits']):
        raise ValueError('actual native point/configuration differs from requested check')
    geobinding = artifact['geometry']
    geo = _bound_json(geobinding['path'], geobinding['sha256'])
    if (geobinding['space'] != space or geo.get('ok') is not True or geo.get('op') != 'geometry'
            or _exact(geo.get('x_bits')) != _exact(request['x_bits'])
            or bits(geo.get('x', [])) != request['x_bits']):
        raise ValueError('geometry evidence belongs to different coordinates/map')
    detail = geo['geometry']
    record = {'geometry': detail, 'admissible': detail['credit_admissible'],
              'h_nonnegative': detail['admissible'], 'min_h': detail['min_h']}
    geometry(record, guard_specs, discrete_names)
    verify_mesh(v, request['reference_mesh_sha256'])
    _mesh_files(raw, artifact['meshes'])
    currents = v.get('currents')
    if type(currents) is not list:
        raise ValueError('missing current sweep')
    for current in currents:
        same(current.get('i_peak_A'), current['fraction']*expected_config['i_peak0_A'], 'current amplitude')
        number(current.get('solve_s'), 'current solve time', minimum=0)
        if type(current.get('newton_iterations')) is not int or current['newton_iterations'] < 0:
            raise ValueError('missing Newton count')
        same(current.get('std2'), current['std']**2, 'current variance')
    for key in ('setup_s',):
        number(raw.get(key), key, minimum=0)
    for key in ('motion_s','remesh_mesh_s','remesh_setup_s','solve_s','total_s'):
        number(v.get(key), key, minimum=0)
    if raw['wall_seconds'] < v['total_s']:
        raise ValueError('inclusive wall time is smaller than validation time')
    row.update(status='ok', actual_spec=copy.deepcopy(job['spec']), currents=copy.deepcopy(currents),
               sum_variances=v.get('F'), geometry=copy.deepcopy(detail), mesh_pass=True,
               geometry_raw_sha256=geobinding['sha256'], native_mesh=copy.deepcopy(v['mesh']))
    reduced = reduce_record(job, row)
    if job['spec']['name'] == 'withheld':
        if v.get('full_current_mean') is not None:
            raise ValueError('withheld sweep has a full-current mean')
    else:
        same(v.get('full_current_mean'), reduced['mean_full'], 'full-current mean', ulps=64)
    return row


def report(selection, designs, definitions, meshes, artifacts, *, provenance, geometry_definitions):
    """Join exact requests, supplied observations and all missing planned checks."""
    prepared = requests(selection, designs, definitions, meshes)
    jobs = {j['job_id']: j for j in prepared['plan']['jobs']}
    if not set(artifacts) <= jobs.keys():
        raise ValueError('artifact is outside the frozen plan')
    rows = []
    for key, artifact in artifacts.items():
        adapted = designs[jobs[key]['design_key']]
        space = adapted['settings']['design_space']
        rows.append(_observation(jobs[key], prepared['requests'][key], adapted, definitions, artifact,
                                provenance=provenance, **geometry_definitions[space]))
    result = summarize(selection, rows)
    result['scope'] = 'Hash-bound native harmonic results mapped to the frozen plan. Caller must authenticate closed capture and execution provenance; no new original target credit.'
    result['can_select'] = False
    result['can_launch'] = False
    return result
