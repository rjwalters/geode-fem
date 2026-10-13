#!/usr/bin/env python3
"""Independent analytic, actual-mesh and artifact checks; active under python -O."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import tempfile
import tomllib

import numpy as np
import staircasing_demo as s


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def expect_error(kind, operation):
    try:
        operation()
    except kind:
        return
    raise AssertionError(f'Expected {kind.__name__}')


def analytic_checks():
    expect_error(AssertionError, lambda: check(False, 'Checks must remain active under -O'))
    for g in [s.Geometry(), s.Geometry(40, 2, 16, 24, 99, 101)]:
        flat = np.array([[[-4., 0., g.substrate_thickness_mm],
                          [4., 0., g.substrate_thickness_mm],
                          [-4., 3., g.substrate_thickness_mm]]])
        tri = s.bent(flat, g)
        f = s.facet_error(tri, g.r_top, g.axis_z)
        expected = g.r_top * (1-math.cos(4/g.bend_radius_mm))
        check(math.isclose(f['facet_max_radial_error_mm'], expected, abs_tol=4e-14),
              'Independent analytic chord sagitta')
        check(f['facet_rms_radial_error_mm'] > 0, 'Nonzero curved-facet RMS')
        q24 = s.facet_error(tri, g.r_top, g.axis_z, order=24)
        check(math.isclose(f['facet_rms_radial_error_mm'], q24['facet_rms_radial_error_mm'], abs_tol=1e-12),
              'Analytic triangle quadrature convergence')
        collapsed = tri.copy()
        collapsed[:, 2] = collapsed[:, 0]
        expect_error(ValueError, lambda: s.facet_error(collapsed, g.r_top, g.axis_z))
        expect_error(ValueError, lambda: s.facet_error(tri, g.r_top+1, g.axis_z))
    expect_error(ValueError, lambda: s.Geometry(substrate_thickness_mm=300))
    expect_error(ValueError, lambda: s.Geometry(patch_tag=13))

    # Hand-built format fixture checks configurable physical tags, not FEM data.
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory)/'tags.msh'
        path.write_text('''$MeshFormat
4.1 0 8
$EndMeshFormat
$Entities
0 0 2 0
1 0 0 0 1 1 0 1 99 0
2 0 0 0 1 1 0 1 101 0
$EndEntities
$Nodes
1 3 1 3
2 1 0 3
1
2
3
0 0 0
1 0 0
0 1 0
$EndNodes
$Elements
2 2 1 2
2 1 2 1
1 1 2 3
2 2 2 1
2 1 2 3
$EndElements
''')
        surfaces = s.mesh_surfaces(path, [99, 101])
        check(set(surfaces) == {99, 101}, 'Configurable physical group IDs')
        check(surfaces[99].shape == (1, 3, 3), 'Linear triangle parser')
        expect_error(ValueError, lambda: s.mesh_surfaces(path, [12, 13]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--l1-mesh', type=Path, default=os.environ.get('PATCH_CONFORMAL_L1'))
    parser.add_argument('--results', type=Path, default=s.HERE/'staircasing_results.json')
    args = parser.parse_args()
    if args.l1_mesh is None:
        parser.error('Pass --l1-mesh or set PATCH_CONFORMAL_L1 to the regenerated L1 mesh')
    analytic_checks()
    g = s.Geometry()
    meshes = [('L0', s.REPO/'crates/geode-core/tests/fixtures/patch_conformal_L0.msh'),
              ('L1', args.l1_mesh)]
    expect_error(ValueError, lambda: s.build_results(s.Geometry(patch_width_mm=37), meshes[:1]))
    r = s.build_results(g, meshes)
    check(r['geometry']['substrate_thickness_mm'] == 1.6, 'Physical substrate thickness')
    check((g.r_top, g.r_bot) == (100.8, 99.2), 'Physical top and bottom radii')
    for name, phi, length in [('patch', .19, 38.304), ('ground', .31, 61.504)]:
        a = r['arcs'][name]
        check(a['half_angle_rad'] == phi, f'{name}: physical arc angle')
        check(a['profile_arc_length_mm'] == length, f'{name}: citation arc length')
        check(.85 < a['resonance_proxy_slope_vs_cell_size'] < 1.15, f'{name}: first-order proxy convergence')
        rows = a['resolutions']
        check(rows[-1]['resonance_shift_proxy_abs'] < rows[0]['resonance_shift_proxy_abs']/10,
              f'{name}: proxy decreases with refinement')
        for row in rows:
            check(math.isclose(row['resonance_shift_proxy_abs'], row['boundary_pos_rms_mm']/length),
                  f'{name}: dimensionless proxy normalization')
            bound = row['cell_size_mm']*(.5+.5*math.tan(phi))
            check(row['boundary_pos_max_mm'] <= bound+1e-12, f'{name}: profile displacement bound')
    for (level, path), row in zip(meshes, r['fem_surface_ladder']):
        baseline = s.REPO/f'benchmarks/patch_antenna_conformal/baseline_2g4_{level}.toml'
        with baseline.open('rb') as source:
            expected_hash = tomllib.load(source)['meta']['fixture_sha256']
        check(row['mesh_sha256'] == expected_hash, f'{level}: independent GEODE fixture hash')
        surfaces = s.mesh_surfaces(path, [g.patch_tag, g.ground_tag])
        for name, tag, radius in [('patch', g.patch_tag, g.r_top), ('ground', g.ground_tag, g.r_bot)]:
            tri = surfaces[tag]
            angles = np.arctan2(tri[..., 0], tri[..., 2]-g.axis_z)
            max_span = np.max(np.max(angles, axis=1)-np.min(angles, axis=1))
            exact = radius*(1-math.cos(max_span/2))
            check(math.isclose(row[name]['facet_max_radial_error_mm'], exact, abs_tol=5e-14),
                  f'{level}/{name}: independent angular chord maximum')
            q24 = s.facet_error(tri, radius, g.axis_z, order=24)
            check(abs(q24['facet_rms_radial_error_mm']-row[name]['facet_rms_radial_error_mm']) < 1e-12,
                  f'{level}/{name}: doubled-order actual-mesh quadrature')
    for name in ['patch', 'ground']:
        a, b = [row[name] for row in r['fem_surface_ladder']]
        check(b['triangles'] > a['triangles'], f'{name}: actual L1 has more triangles')
        for metric in ['facet_max_radial_error_mm', 'facet_rms_radial_error_mm']:
            check(0 < b[metric] < a[metric], f'{name}: actual mesh refinement reduces {metric}')
    regenerated = json.dumps(r, indent=2, sort_keys=True, allow_nan=False)+'\n'
    check(regenerated == args.results.read_text(), 'Byte-identical JSON regeneration')
    digest = hashlib.sha256(args.results.read_bytes()).hexdigest()
    check(digest == (s.HERE/'staircasing_results.sha256').read_text().split()[0], 'v5 citation SHA-256')
    print('PASS: analytic sagitta, custom tags, rejection checks, Cartesian bounds, actual L0/L1 hashes, quadrature, JSON and citation hash (checks remain active under -O)')


if __name__ == '__main__':
    main()
