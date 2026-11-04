"""Check recorded geometry verdicts before adapting a validation for selection.

Pure preparation for #1203. Expected guard names must come from independently
verified frozen definitions. This does not verify trace hashes, rerun geometry,
check the validation mesh, or establish whole-run eligibility.
"""
import math

CREDIT_H_TOLERANCE = 1e-12


def _rows(rows, expected, label):
    expected = tuple(expected)
    if (not expected or any(type(n) is not str or not n for n in expected)
            or len(set(expected)) != len(expected) or type(rows) is not list):
        raise ValueError(f"invalid expected {label} or recorded list")
    if any(type(r) is not dict or type(r.get('name')) is not str for r in rows):
        raise ValueError(f"invalid {label} record")
    names = [r['name'] for r in rows]
    if len(names) != len(set(names)) or set(names) != set(expected):
        raise ValueError(f"missing, extra or duplicate {label}")
    if any(type(r.get('pass')) is not bool for r in rows):
        raise ValueError(f"non-boolean {label} verdict")
    return rows


def _finite(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError('missing or nonfinite geometry quantity')
    return value


def verify_geometry(validation, *, guard_names, discrete_names):
    """Recompute credit from recorded margins; cross-check both stored verdicts.

    Construction and smooth/discrete guard verdicts remain declared evidence;
    the full adapter must establish their provenance and registered definitions.
    A false construction verdict is scientific ineligibility, not permission to
    discard this validation. Contradictory or incomplete records raise ValueError.
    """
    if type(validation) is not dict or type(validation.get('geometry')) is not dict:
        raise ValueError('missing geometry evidence')
    geometry = validation['geometry']
    guards = _rows(geometry.get('guards'), guard_names, 'smooth guards')
    discrete = _rows(geometry.get('discrete'), discrete_names, 'discrete guards')
    if ('construction_error' not in geometry or
            (geometry['construction_error'] is not None
             and type(geometry['construction_error']) is not str)):
        raise ValueError('missing construction verdict')
    margins = [_finite(r.get('h')) for r in guards]
    discrete_pass = all(r['pass'] for r in discrete)
    buildable = (geometry['construction_error'] is None and discrete_pass
                 and all(r['pass'] for r in guards))
    exact = buildable and all(h >= 0 for h in margins)
    credit = buildable and all(h >= -CREDIT_H_TOLERANCE for h in margins)
    for observed, expected, name in (
        (geometry.get('buildable'), buildable, 'buildable'),
        (geometry.get('admissible'), exact, 'geometry.admissible'),
        (validation.get('h_nonnegative'), exact, 'h_nonnegative'),
        (validation.get('admissible'), credit, 'credit admissible'),
    ):
        if type(observed) is not bool or observed != expected:
            raise ValueError(f'inconsistent {name}')
    if _finite(validation.get('min_h')) != min(margins):
        raise ValueError('inconsistent min_h')
    return {'geometry_admissible': credit, 'discrete_pass': discrete_pass,
            'buildable': buildable, 'exact_admissible': exact,
            'min_h': min(margins), 'credit_h_tolerance': CREDIT_H_TOLERANCE}
