#!/usr/bin/env python3
"""Geometric diagnostics for the R=40 mm, h=2 mm historical smoke fixture.

This is NOT an electromagnetic solve or a comparison with conformal FDTD.
A cell-centred occupancy model is only one possible Cartesian representation.
The perimeter plateau is the taxicab-length paradox, not a resonance error.
The displacement/arc-length proxy is dimensionless and has no calibrated EM
coefficient. Linear FEM facets also approximate the cylinder between nodes.

L0 is the committed smoke surface; L1–L3 subdivide its flat triangles before
bending. These are surface-only geometric ladders, not solved volume meshes.
The physical 2.4 GHz fixture under #1084 is a different antenna.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
MESH = REPO / 'crates/geode-core/tests/fixtures/patch_2g4_smoke.msh'
R_BEND = 40.0
H_SUB = 2.0
R_TOP = R_BEND + H_SUB / 2
R_BOT = R_BEND - H_SUB / 2
ARC_CENTER_Z = H_SUB / 2 - R_BEND
RESOLUTIONS = [1, 2, 4, 8, 16]  # cells/mm
N_ARC_SAMPLES = 40001


def slope(x, y):
    return float(np.polyfit(np.log(x), np.log(y), 1)[0])


def rasterize(resolution, half_angle):
    """Full-cell occupancy and a columnwise, nearest-grid-face top profile.

    The height profile evaluates the analytic arc at each column centre,
    then rounds to a GRID FACE (not a cell centre). End-column centres are
    clipped to the arc endpoints; this explicit endpoint convention avoids
    silently dropping samples. RMS is uniformly weighted in projected x.
    Occupancy area/perimeter are separate diagnostics of the entire sector.
    """
    h = 1.0 / resolution
    xmax = R_TOP * np.sin(half_angle)
    zmin = ARC_CENTER_Z + R_BOT * np.cos(half_angle)
    # Fixed physical grid origin; grid faces lie on integer multiples of h.
    ix = np.arange(np.floor(-xmax / h) - 2, np.ceil(xmax / h) + 2)
    iz = np.arange(np.floor(zmin / h) - 2, np.ceil(H_SUB / h) + 2)
    x, z = np.meshgrid((ix + .5) * h, (iz + .5) * h, indexing='ij')
    radius = np.hypot(x, z - ARC_CENTER_Z)
    mask = ((radius >= R_BOT) & (radius <= R_TOP)
            & (np.abs(np.arctan2(x, z - ARC_CENTER_Z)) <= half_angle))
    padded = np.pad(mask.astype(int), 1)
    perimeter = float(np.abs(np.diff(padded, axis=0)).sum()
                      + np.abs(np.diff(padded, axis=1)).sum()) * h
    area = float(mask.sum()) * h*h
    xs = np.linspace(-xmax, xmax, N_ARC_SAMPLES)
    centres = np.clip((np.floor(xs / h) + .5) * h, -xmax, xmax)
    true_z = ARC_CENTER_Z + np.sqrt(R_TOP**2 - xs**2)
    column_z = ARC_CENTER_Z + np.sqrt(R_TOP**2 - centres**2)
    snapped_z = np.round(column_z / h) * h
    error = snapped_z - true_z
    rms = float(np.sqrt(np.mean(error**2)))
    true_area = half_angle * (R_TOP**2 - R_BOT**2)
    true_perimeter = 2*half_angle*(R_TOP + R_BOT) + 2*H_SUB
    # Total variation of both circular arcs plus the radial end caps.
    taxicab = (2*(R_TOP+R_BOT)*(np.sin(half_angle)+1-np.cos(half_angle))
               + 2*H_SUB*(np.sin(half_angle)+np.cos(half_angle)))
    return {
        'resolution_cells_per_mm': resolution, 'cell_size_mm': h,
        'boundary_pos_rms_mm': rms, 'boundary_pos_max_mm': float(np.max(np.abs(error))),
        'resonance_shift_proxy_abs': rms / (2*R_TOP*half_angle),
        'area_mm2': area, 'area_rel_error': (area-true_area)/true_area,
        'perimeter_mm': perimeter, 'perimeter_rel_error': (perimeter-true_perimeter)/true_perimeter,
        'taxicab_perimeter_limit_mm': float(taxicab),
        'taxicab_perimeter_rel_error_limit': float(taxicab/true_perimeter-1),
    }


def mesh_surfaces(path):
    """Read physical 12/13 linear surface triangles from ASCII MSH 4.1."""
    lines = path.read_text().splitlines()
    i = lines.index('$Entities') + 1
    np_, nc, ns, _ = map(int, lines[i].split())
    i += 1 + np_ + nc
    groups = {}
    for line in lines[i:i+ns]:
        row = line.split(); n = int(row[7])
        groups[int(row[0])] = set(map(int, row[8:8+n]))
    i = lines.index('$Nodes') + 1
    blocks = int(lines[i].split()[0]); i += 1
    nodes = {}
    for _ in range(blocks):
        _, _, parametric, count = map(int, lines[i].split()); i += 1
        assert parametric == 0, 'Only non-parametric fixture nodes are supported'
        tags = []
        while len(tags) < count:
            tags.extend(map(int, lines[i].split())); i += 1
        for tag in tags:
            nodes[tag] = list(map(float, lines[i].split())); i += 1
    i = lines.index('$Elements') + 1
    blocks = int(lines[i].split()[0]); i += 1
    surfaces = {12: [], 13: []}
    for _ in range(blocks):
        dim, entity, kind, count = map(int, lines[i].split()); i += 1
        if dim == 2 and kind == 2:
            for group in surfaces:
                if group in groups[entity]:
                    for line in lines[i:i+count]:
                        surfaces[group].append([nodes[tag] for tag in map(int, line.split()[1:])])
        i += count
    assert all(surfaces.values())
    return {k: np.array(v) for k, v in surfaces.items()}


def subdivide(tri):
    a, b, c = tri[:, 0], tri[:, 1], tri[:, 2]
    ab, bc, ca = (a+b)/2, (b+c)/2, (c+a)/2
    return np.concatenate([np.stack(t, axis=1) for t in
                           [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]])


def bent(flat):
    radius = R_BEND + flat[..., 2] - H_SUB/2
    phi = flat[..., 0] / R_BEND
    return np.stack([radius*np.sin(phi), flat[..., 1],
                     ARC_CENTER_Z+radius*np.cos(phi)], axis=-1)


def facet_error(flat, order=12):
    """Area-weighted RMS radial distance to cylinder, and exact maximum.

    The maximum deficit is the nearest point to the origin in each triangle's
    x-z projection (vertices and edges suffice for these small-angle arcs).
    RMS uses tensor Gauss quadrature under the Duffy triangle map; a higher
    order is used independently by the verification script.
    """
    tri = bent(flat)
    radius = R_BEND + float(flat[0, 0, 2]) - H_SUB/2
    assert np.allclose(flat[..., 2], flat[0, 0, 2])
    xz = tri[..., [0, 2]] - np.array([0., ARC_CENTER_Z])
    min_r = np.full(len(tri), np.inf)
    for j in range(3):
        a, b = xz[:, j], xz[:, (j+1) % 3]
        delta = b-a
        den = np.sum(delta*delta, axis=1)
        t = np.divide(-np.sum(a*delta, axis=1), den, out=np.zeros_like(den), where=den>0)
        point = a + np.clip(t, 0, 1)[:, None]*delta
        min_r = np.minimum(min_r, np.linalg.norm(point, axis=1))
    jac = np.linalg.norm(np.cross(tri[:, 1]-tri[:, 0], tri[:, 2]-tri[:, 0]), axis=1)
    if np.any(jac <= 0):
        raise ValueError('Degenerate surface triangle')
    q, w = np.polynomial.legendre.leggauss(order); q=(q+1)/2; w=w/2
    integral = 0.
    for u, wu in zip(q, w):
        for v, wv in zip(q, w):
            point = (1-u)*xz[:, 0] + u*(1-v)*xz[:, 1] + u*v*xz[:, 2]
            error = radius - np.linalg.norm(point, axis=1)
            integral += float(np.sum(jac * error**2)) * u * wu * wv
    return {'triangles': len(tri), 'radius_mm': radius,
            'facet_max_radial_error_mm': float(np.max(radius-min_r)),
            'facet_rms_radial_error_mm': float(np.sqrt(integral/(jac.sum()/2))),
            'max_flat_x_span_mm': float(np.ptp(flat[..., 0], axis=1).max())}


def build_results():
    arcs = {}
    for name, phi in [('patch', .2), ('substrate', .3)]:
        rows = [rasterize(r, phi) for r in RESOLUTIONS]
        arcs[name] = {'half_angle_rad': phi, 'top_arc_length_mm': 2*R_TOP*phi,
                      'resolutions': rows, 'resonance_proxy_slope_vs_cell_size': slope(
                          [r['cell_size_mm'] for r in rows],
                          [r['resonance_shift_proxy_abs'] for r in rows])}
    facets = []
    surfaces = mesh_surfaces(MESH)
    for level in range(4):
        facets.append({'level': f'L{level}', 'patch': facet_error(surfaces[12]),
                       'ground': facet_error(surfaces[13])})
        if level < 3:
            surfaces = {k: subdivide(v) for k, v in surfaces.items()}
    return {'_schema': 'geode-fdtd-staircasing/2',
            'scope': 'Historical smoke geometry only; geometric proxy, no EM solve or cost-to-accuracy claim.',
            'geometry': {'bend_radius_mm': R_BEND, 'substrate_thickness_mm': H_SUB,
                         'top_radius_mm': R_TOP, 'bottom_radius_mm': R_BOT,
                         'patch_width_mm': 16., 'substrate_width_mm': 24.},
            'provenance': {'mesh': str(MESH.relative_to(REPO)),
                           'mesh_sha256': hashlib.sha256(MESH.read_bytes()).hexdigest(),
                           'generator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()},
            'metric_notes': {'proxy': 'RMS vertical height displacement / top arc length; not solved resonance or S11.',
                             'perimeter': 'Taxicab staircase paradox; not an electromagnetic error.',
                             'fem': 'Area-weighted radial facet error; vertices on the arc do not make facets exact.',
                             'ladder': 'L0 committed smoke surface; L1-L3 flat midpoint surface subdivision then exact cylindrical nodal bend. No refined volume solves.',
                             'comparison': 'Cartesian vertical RMS and facet radial RMS use different measures; no equal-error solver cost is inferred.'},
            'arcs': arcs, 'fem_surface_ladder': facets}


def plot(results, path):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    plt.rcParams.update({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False})
    fig, axes = plt.subplots(2, 2, figsize=(11, 8), layout='constrained')
    for name, color in [('patch', '#23679A'), ('substrate', '#16847B')]:
        arc = results['arcs'][name]; rows = arc['resolutions']
        x = [r['resolution_cells_per_mm'] for r in rows]
        axes[0, 0].loglog(x, [r['resonance_shift_proxy_abs'] for r in rows], 'o-', color=color,
                         label=f"{name}; slope vs cell size {arc['resonance_proxy_slope_vs_cell_size']:.2f}")
        axes[0, 1].loglog(x, [r['boundary_pos_rms_mm']*1000 for r in rows], 'o-', color=color, label=name)
        axes[1, 1].semilogx(x, [r['perimeter_rel_error']*100 for r in rows], 'o-', color=color, label=name+' footprint')
        axes[1, 1].axhline(rows[0]['taxicab_perimeter_rel_error_limit']*100, color=color, ls='--', lw=1)
    for name, color in [('patch', '#23679A'), ('ground', '#72777D')]:
        rows = [r[name] for r in results['fem_surface_ladder']]
        axes[1, 0].semilogy(range(4), [r['facet_max_radial_error_mm']*1000 for r in rows], 'o-', color=color, label=f'{name} max')
        axes[1, 0].semilogy(range(4), [r['facet_rms_radial_error_mm']*1000 for r in rows], 's--', color=color, label=f'{name} RMS')
    axes[0, 0].set(title='(a) Displacement / arc-length proxy converges', ylabel='Dimensionless geometric proxy')
    axes[0, 1].set(title='(b) Cartesian top-profile error', ylabel='Vertical RMS displacement (µm)')
    axes[1, 0].set(title='(c) Linear FEM facets are not exact arcs', ylabel='Radial distance to cylinder (µm)',
                   xlabel='Surface-only midpoint refinement level', xticks=range(4), xticklabels=['L0', 'L1', 'L2', 'L3'])
    axes[1, 1].set(title='(d) Perimeter: staircase paradox, not EM error', ylabel='Perimeter error (%)')
    for ax in [axes[0, 0], axes[0, 1], axes[1, 1]]:
        ax.set_xlabel('Cartesian resolution (cells/mm)')
        ax.set_xticks(RESOLUTIONS, labels=RESOLUTIONS)
    for ax in axes.flat:
        ax.legend(fontsize=9); ax.grid(alpha=.2, which='both')
    fig.suptitle('Historical smoke fixture: R = 40 mm, h = 2 mm; patch ±0.2 rad, substrate ±0.3 rad\nGeometry diagnostics only — no solved frequency, S11, or solver-cost claim', fontsize=12)
    fig.savefig(path, dpi=160)
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, default=HERE)
    parser.add_argument('--no-plot', action='store_true')
    args = parser.parse_args()
    results = build_results()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir/'staircasing_results.json').write_text(json.dumps(results, indent=2, sort_keys=True, allow_nan=False)+'\n')
    if not args.no_plot:
        plot(results, args.output_dir/'staircasing.png')
    for name, arc in results['arcs'].items():
        vals = [r['resonance_shift_proxy_abs'] for r in arc['resolutions']]
        print(f"{name}: proxy {vals[0]:.6g} -> {vals[-1]:.6g}; slope {arc['resonance_proxy_slope_vs_cell_size']:.4f}")
    for row in results['fem_surface_ladder']:
        print(row)


if __name__ == '__main__':
    main()
