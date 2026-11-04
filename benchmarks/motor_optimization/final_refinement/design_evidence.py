"""Bind recorded free coordinates to the complete registered motor design.

Reads hash-bound reference/start files, checks native ready-space/model fields,
and preserves exact coordinates and the originating motion map. Source/runtime
authentication, geometry construction, mesh validity and scientific credit are
separate checks; this module never invokes a solver.
"""
from dataclasses import dataclass
import hashlib
import json
import math
from pathlib import Path
import struct

from selection import Design, _exact
from trace_archive import _json, _sha

SCALARS = ('MAG', 'MH', 'MW', 'SW1', 'SW2', 'SW3', 'SW4', 'SD1', 'Sr1',
           'LENGTH', 'OPERATING_ANGLE', 'ScaleR')
FREE = ('MAG', 'MH', 'MW', 'SW1', 'SW2', 'SW3', 'SW4', 'Sr1')
OFFSETS = tuple(f'CPR{i}' for i in range(1, 8)) + tuple(f'CPS{i}' for i in range(1, 4))
FROZEN = {'LENGTH': .1, 'SD1': .135, 'ScaleR': 1., 'OPERATING_ANGLE': 0.}
OFFSET_BOUNDS = ((-.0015, 0.), (-.0015, 0.), (-.001, 0.), *([(-.0015, 0.)] * 4),
                 *((0., .0005),) * 3)


def number(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError('design coordinate must be a finite number')
    return float(value)


def bits(values):
    return [struct.unpack('>Q', struct.pack('>d', number(v)))[0] for v in values]


def _bound_json(path, expected):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError('missing or redirected design-definition file')
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != _sha(expected):
        raise ValueError('changed design-definition bytes')
    data = _json(raw)
    if path.read_bytes() != raw:
        raise ValueError('design-definition file changed during read')
    return data


def _primary():
    return {'angles_deg': [float(i) for i in range(20)], 'fractions': [0., .5, 1.],
            'i_peak0_A': 9.677657374172274, 'level': 2, 'mesh_repair': True,
            'mode': 'h93', 'name': 'primary', 'newton_max_iterations': 60,
            'newton_rel_tol': 1e-8, 'policy_min_angle_deg': 5., 'start': 'cold',
            'torque': 'h93/coupling'}


@dataclass(frozen=True)
class RegisteredDesigns:
    initial: tuple
    spaces_json: str
    reference_sha256: str
    starts_sha256: str

    @classmethod
    def load(cls, reference, starts, *, reference_sha256, starts_sha256):
        ref = _bound_json(reference, reference_sha256)
        frozen_starts = _bound_json(starts, starts_sha256)
        initial = ref['designs']['initial']['parameters_by_source_registration_order']
        if set(initial) != set(SCALARS):
            raise ValueError('reference lacks the complete twelve scalar parameters')
        initial = {name: number(initial[name]) for name in SCALARS}
        if bits([initial[k] for k in FROZEN]) != bits(FROZEN.values()):
            raise ValueError('reference changed registered frozen parameters')
        bounds = ref['optimization_parameter_bounds']
        if (type(bounds) is not list or len({b['name'] for b in bounds}) != len(bounds)
                or {b['name'] for b in bounds} != set(SCALARS)):
            raise ValueError('reference bounds are missing, duplicated or unknown')
        bounds = {b['name']: (number(b['minimum']), number(b['maximum'])) for b in bounds}
        if any(lo > hi or not lo <= initial[name] <= hi for name, (lo, hi) in bounds.items()):
            raise ValueError('reference bounds or initial coordinates are invalid')
        spaces = frozen_starts['spaces']
        if type(spaces) is not dict or set(spaces) != {'a', 'b'}:
            raise ValueError('both frozen design spaces are required')
        for label, map_name in (('a', 'scalar'), ('b', 'cp')):
            free = FREE + (OFFSETS if label == 'b' else ())
            pairs = [bounds[n] for n in FREE] + (list(OFFSET_BOUNDS) if label == 'b' else [])
            expected = {'label': label.upper(), 'map': map_name, 'free': list(free),
                        'lower': [p[0] for p in pairs], 'upper': [p[1] for p in pairs],
                        'x0': [initial[n] for n in FREE] + ([0.] * 10 if label == 'b' else []),
                        'frozen': [{'name': n, 'value': v} for n, v in FROZEN.items()]}
            if _exact(spaces[label]) != _exact(expected):
                raise ValueError('frozen space differs from registered parameter mapping/bounds')
            if bits(frozen_starts['starts']['theta0'][label]) != bits(expected['x0']):
                raise ValueError('frozen theta0 start differs from its space')
        return cls(tuple(initial[n] for n in SCALARS), json.dumps(spaces, sort_keys=True),
                   reference_sha256, starts_sha256)

    def adapt(self, trace, record, *, provenance):
        """Produce Design + matched theta0 from a separately authenticated trace.

        provenance binds the evaluator binary and full source-file manifest; the
        caller must verify those hashes against build/capture evidence. Training
        resolution and optimizer identity intentionally do not enter final-check
        identity. Common-validator settings, reference mesh and motion map do.
        """
        required = {'source_commit', 'binary_sha256', 'source_files_sha256'}
        if type(provenance) is not dict or set(provenance) != required:
            raise ValueError('complete declared source/binary provenance required')
        commit = provenance['source_commit']
        if type(commit) is not str or len(commit) != 40 or any(c not in '0123456789abcdef' for c in commit):
            raise ValueError('invalid source commit')
        for name in required - {'source_commit'}:
            _sha(provenance[name])
        space = trace.get('space')
        if space not in ('a', 'b'):
            raise ValueError('unknown originating design space')
        definition = json.loads(self.spaces_json)[space]
        ready = trace.get('evaluator_ready') or {}
        native_names = list(SCALARS[:9] + (OFFSETS if space == 'b' else ()) + SCALARS[9:])
        if (ready.get('ok') is not True or ready.get('op') != 'ready'
                or ready.get('protocol') != 'icem-harness/1'
                or _exact(ready.get('space')) != _exact(definition)
                or ready.get('point_names') != native_names):
            raise ValueError('observed evaluator changed the registered coordinate map')
        primary = ready.get('session', {}).get('validation')
        if _exact(primary) != _exact(_primary()):
            raise ValueError('observed validator differs from the common primary model')
        mesh = _sha(ready.get('validation_msh_sha256'))
        if trace.get('anchor', {}).get('msh_sha256') != mesh:
            raise ValueError('observed validator and trace anchor meshes differ')
        x = record.get('x')
        if type(x) is not list or len(x) != len(definition['free']):
            raise ValueError('wrong free-coordinate vector length')
        x = [number(value) for value in x]
        if any(not lo <= value <= hi for value, lo, hi in
               zip(x, definition['lower'], definition['upper'])):
            raise ValueError('validation coordinates lie outside the registered box')
        if 'x_bits' in record and (type(record['x_bits']) is not list
                or any(type(v) is not int for v in record['x_bits']) or record['x_bits'] != bits(x)):
            raise ValueError('recorded coordinate bits disagree')
        parameters = dict(zip(SCALARS, self.initial))
        parameters.update({name: 0. for name in OFFSETS})
        parameters.update(zip(definition['free'], x))
        settings = {**provenance, 'reference_sha256': self.reference_sha256,
                    'design_space': space, 'motion_map': definition['map'],
                    'common_validator': json.loads(json.dumps(primary)), 'validation_reference_mesh_sha256': mesh}
        initial = dict(zip(SCALARS, self.initial))
        initial.update({name: 0. for name in OFFSETS})
        return {'design': Design.from_values(parameters, settings),
                'theta0': Design.from_values(initial, settings),
                'parameters': parameters, 'settings': settings, 'x_bits': bits(x),
                'native_point_names': native_names,
                'native_point_bits': bits([parameters[n] for n in native_names]),
                'stator_offsets_zero': all(parameters[n] == 0. for n in OFFSETS[7:]),
                'definition_sha256': {'reference': self.reference_sha256, 'starts': self.starts_sha256},
                'can_launch': False,
                'scope': 'Complete registered parameter/settings mapping only; no geometry construction, mesh, source/runtime authentication or scientific credit.'}
