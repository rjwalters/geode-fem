#!/usr/bin/env python3
"""Independent analytic checks and committed-artifact drift check for E6."""
import json
import math
from pathlib import Path
import numpy as np
import staircasing_demo as s

# A single triangle with two distinct x positions has an exact chord sagitta.
t = np.array([[[-4., 0., 2.], [4., 0., 2.], [-4., 3., 2.]]])
f = s.facet_error(t)
expected = 41 * (1 - math.cos(8/40/2))
assert math.isclose(f['facet_max_radial_error_mm'], expected, abs_tol=2e-14)
assert f['facet_rms_radial_error_mm'] > 0
assert math.isclose(f['facet_rms_radial_error_mm'], s.facet_error(t, order=24)['facet_rms_radial_error_mm'], abs_tol=1e-12)
# Subdivision before bending must reduce the maximum by approximately four.
g = s.facet_error(s.subdivide(t))
assert 3.99 < f['facet_max_radial_error_mm']/g['facet_max_radial_error_mm'] < 4.01
# Reject a collapsed triangle instead of silently publishing NaN RMS.
t[..., 0] = 0
try:
    s.facet_error(t)
except ValueError:
    pass
else:
    raise AssertionError('Degenerate triangle accepted')
r = s.build_results()
assert r['geometry']['substrate_thickness_mm'] == 2
assert (r['geometry']['top_radius_mm'], r['geometry']['bottom_radius_mm']) == (41, 39)
for name, phi in [('patch', .2), ('substrate', .3)]:
    a = r['arcs'][name]
    assert a['half_angle_rad'] == phi
    assert .85 < a['resonance_proxy_slope_vs_cell_size'] < 1.15
    assert a['resolutions'][-1]['resonance_shift_proxy_abs'] < a['resolutions'][0]['resonance_shift_proxy_abs']/10
    for row in a['resolutions']:
        assert math.isclose(row['resonance_shift_proxy_abs'], row['boundary_pos_rms_mm']/a['top_arc_length_mm'])
        # Bounded height quantization plus bounded columnwise arc variation.
        bound = row['cell_size_mm']*(.5 + .5*math.tan(phi))
        assert row['boundary_pos_max_mm'] <= bound + 1e-12
for name in ['patch', 'ground']:
    rows = [level[name] for level in r['fem_surface_ladder']]
    for a,b in zip(rows, rows[1:]):
        assert b['triangles'] == a['triangles']*4
        assert 3.99 < a['facet_max_radial_error_mm']/b['facet_max_radial_error_mm'] < 4.01
assert r == json.loads((Path(__file__).parent/'staircasing_results.json').read_text())
print('PASS: analytic sagitta, quadrature convergence, degenerate-facet rejection, raster displacement bound, proxy convergence, facet refinement and exact JSON regeneration')
