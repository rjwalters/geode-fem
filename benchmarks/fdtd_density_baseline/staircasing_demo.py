#!/usr/bin/env python3
"""Cartesian profile and actual FEM facet diagnostics for a bent patch.

Defaults describe the physical 2.4 GHz fixture (R=100 mm, h=1.6 mm).
Supply --mesh LEVEL=PATH for each already-bent ASCII MSH 4.1 volume mesh.
Without --mesh, use the committed L0 and PATCH_CONFORMAL_L1 environment path.
These are geometric diagnostics, not solved resonance, S11 or solver cost.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
RESOLUTIONS = [1, 2, 4, 8, 16]  # cells/mm
N_ARC_SAMPLES = 40001


@dataclass(frozen=True)
class Geometry:
    bend_radius_mm: float = 100.0
    substrate_thickness_mm: float = 1.6
    patch_width_mm: float = 38.0
    substrate_width_mm: float = 62.0
    patch_tag: int = 12
    ground_tag: int = 13

    def __post_init__(self):
        values = [self.bend_radius_mm, self.substrate_thickness_mm,
                  self.patch_width_mm, self.substrate_width_mm]
        if not all(np.isfinite(x) and x > 0 for x in values):
            raise ValueError('Geometry lengths must be finite and positive')
        if self.r_bot <= 0 or self.patch_width_mm > self.substrate_width_mm:
            raise ValueError('Invalid substrate radius or footprint')
        if self.substrate_width_mm / (2*self.bend_radius_mm) >= np.pi/2:
            raise ValueError('Only arcs in the upper cylinder half are supported')
        if self.patch_tag == self.ground_tag:
            raise ValueError('Patch and ground physical tags must differ')

    @property
    def r_top(self):
        return self.bend_radius_mm + self.substrate_thickness_mm/2

    @property
    def r_bot(self):
        return self.bend_radius_mm - self.substrate_thickness_mm/2

    @property
    def axis_z(self):
        return self.substrate_thickness_mm/2 - self.bend_radius_mm


def slope(x, y):
    return float(np.polyfit(np.log(x), np.log(y), 1)[0])


def rasterize(resolution, half_angle, geometry, profile_radius):
    """Nearest-grid-face profile and independent occupied-sector diagnostics.

    Profiles sample column centres, clipped at arc endpoints, then round the
    height to a grid face. RMS is uniform in projected x. The occupied sector
    spans the entire slab under the chosen footprint; it is not a metal area.
    """
    if not np.isfinite(resolution) or resolution <= 0:
        raise ValueError('Resolution must be finite and positive')
    g = geometry
    h = 1.0 / resolution
    xmax = g.r_top * np.sin(half_angle)
    zmin = g.axis_z + g.r_bot * np.cos(half_angle)
    ix = np.arange(np.floor(-xmax/h)-2, np.ceil(xmax/h)+2)
    iz = np.arange(np.floor(zmin/h)-2, np.ceil(g.substrate_thickness_mm/h)+2)
    x, z = np.meshgrid((ix+.5)*h, (iz+.5)*h, indexing='ij')
    radius = np.hypot(x, z-g.axis_z)
    mask = ((radius >= g.r_bot) & (radius <= g.r_top)
            & (np.abs(np.arctan2(x, z-g.axis_z)) <= half_angle))
    padded = np.pad(mask.astype(int), 1)
    perimeter = float(np.abs(np.diff(padded, axis=0)).sum()
                      + np.abs(np.diff(padded, axis=1)).sum()) * h
    area = float(mask.sum()) * h*h
    profile_xmax = profile_radius * np.sin(half_angle)
    xs = np.linspace(-profile_xmax, profile_xmax, N_ARC_SAMPLES)
    centres = np.clip((np.floor(xs/h)+.5)*h, -profile_xmax, profile_xmax)
    true_z = g.axis_z + np.sqrt(profile_radius**2-xs**2)
    column_z = g.axis_z + np.sqrt(profile_radius**2-centres**2)
    error = np.round(column_z/h)*h - true_z
    rms = float(np.sqrt(np.mean(error**2)))
    true_area = half_angle * (g.r_top**2-g.r_bot**2)
    true_perimeter = 2*half_angle*(g.r_top+g.r_bot) + 2*g.substrate_thickness_mm
    taxicab = (2*(g.r_top+g.r_bot)*(np.sin(half_angle)+1-np.cos(half_angle))
               + 2*g.substrate_thickness_mm*(np.sin(half_angle)+np.cos(half_angle)))
    return {
        'resolution_cells_per_mm': resolution, 'cell_size_mm': h,
        'boundary_pos_rms_mm': rms, 'boundary_pos_max_mm': float(np.max(np.abs(error))),
        'resonance_shift_proxy_abs': rms/(2*profile_radius*half_angle),
        'area_mm2': area, 'area_rel_error': (area-true_area)/true_area,
        'perimeter_mm': perimeter, 'perimeter_rel_error': (perimeter-true_perimeter)/true_perimeter,
        'taxicab_perimeter_limit_mm': float(taxicab),
        'taxicab_perimeter_rel_error_limit': float(taxicab/true_perimeter-1),
    }


def mesh_surfaces(path, tags):
    """Read requested physical groups' linear triangles from ASCII MSH 4.1."""
    lines = Path(path).read_text().splitlines()
    fmt = lines[lines.index('$MeshFormat')+1].split()
    if fmt[:2] != ['4.1', '0']:
        raise ValueError('Only ASCII MSH 4.1 is supported')
    i = lines.index('$Entities')+1
    np_, nc, ns, _ = map(int, lines[i].split())
    i += 1+np_+nc
    groups = {}
    for line in lines[i:i+ns]:
        row = line.split()
        groups[int(row[0])] = set(map(int, row[8:8+int(row[7])]))
    i = lines.index('$Nodes')+1
    blocks = int(lines[i].split()[0])
    i += 1
    nodes = {}
    for _ in range(blocks):
        _, _, parametric, count = map(int, lines[i].split())
        i += 1
        if parametric:
            raise ValueError('Parametric fixture nodes are not supported')
        node_tags = []
        while len(node_tags) < count:
            node_tags.extend(map(int, lines[i].split()))
            i += 1
        for tag in node_tags:
            nodes[tag] = list(map(float, lines[i].split()))
            i += 1
    i = lines.index('$Elements')+1
    blocks = int(lines[i].split()[0])
    i += 1
    surfaces = {tag: [] for tag in tags}
    for _ in range(blocks):
        dim, entity, kind, count = map(int, lines[i].split())
        i += 1
        selected = set(tags) & groups.get(entity, set()) if dim == 2 else set()
        if selected and kind != 2:
            raise ValueError('Requested physical group contains non-linear/non-triangle elements')
        for group in selected:
            for line in lines[i:i+count]:
                surfaces[group].append([nodes[tag] for tag in map(int, line.split()[1:])])
        i += count
    if not all(surfaces.values()):
        raise ValueError(f'Missing requested surface triangles: {tags}')
    return {k: np.array(v) for k, v in surfaces.items()}


def bent(flat, geometry):
    """Analytic nodal map, used only for the independent toy checks."""
    g = geometry
    radius = g.bend_radius_mm + flat[..., 2] - g.substrate_thickness_mm/2
    phi = flat[..., 0] / g.bend_radius_mm
    return np.stack([radius*np.sin(phi), flat[..., 1],
                     g.axis_z+radius*np.cos(phi)], axis=-1)


def facet_error(tri, radius, axis_z, order=12):
    """Area-weighted radial RMS and exact maximum of already-bent facets.

    With all projected vertices above the cylinder centre, the origin is
    outside each projected triangle. Its nearest point is therefore on an
    edge. RMS uses Gauss quadrature under the Duffy triangle map.
    """
    if tri.ndim != 3 or tri.shape[1:] != (3, 3) or not np.isfinite(tri).all():
        raise ValueError('Expected finite N x 3 x 3 triangle coordinates')
    xz = tri[..., [0, 2]] - np.array([0., axis_z])
    if not np.all(xz[..., 1] > 0):
        raise ValueError('Facet must be in the upper cylinder half')
    node_error = float(np.max(np.abs(np.linalg.norm(xz, axis=2)-radius)))
    if node_error > 1e-7:
        raise ValueError(f'Nodes are not on the requested cylinder: residual {node_error} mm')
    min_r = np.full(len(tri), np.inf)
    for j in range(3):
        a, b = xz[:, j], xz[:, (j+1) % 3]
        delta = b-a
        den = np.sum(delta*delta, axis=1)
        t = np.divide(-np.sum(a*delta, axis=1), den, out=np.zeros_like(den), where=den > 0)
        point = a + np.clip(t, 0, 1)[:, None]*delta
        min_r = np.minimum(min_r, np.linalg.norm(point, axis=1))
    jac = np.linalg.norm(np.cross(tri[:, 1]-tri[:, 0], tri[:, 2]-tri[:, 0]), axis=1)
    if np.any(jac <= 0):
        raise ValueError('Degenerate surface triangle')
    q, w = np.polynomial.legendre.leggauss(order)
    q, w = (q+1)/2, w/2
    integral = 0.
    for u, wu in zip(q, w):
        for v, wv in zip(q, w):
            point = (1-u)*xz[:, 0] + u*(1-v)*xz[:, 1] + u*v*xz[:, 2]
            error = radius-np.linalg.norm(point, axis=1)
            integral += float(np.sum(jac*error**2))*u*wu*wv
    return {'triangles': len(tri), 'radius_mm': radius,
            'facet_max_radial_error_mm': float(np.max(radius-min_r)),
            'facet_rms_radial_error_mm': float(np.sqrt(integral/(jac.sum()/2))),
            'max_node_radial_residual_mm': node_error,
            'max_angular_span_rad': float(np.ptp(np.arctan2(xz[..., 0], xz[..., 1]), axis=1).max())}


def build_results(geometry, meshes):
    g = geometry
    if not meshes or len({label for label, _ in meshes}) != len(meshes):
        raise ValueError('Supply at least one mesh, with distinct level labels')
    arcs = {}
    for name, width, radius in [('patch', g.patch_width_mm, g.r_top),
                                ('ground', g.substrate_width_mm, g.r_bot)]:
        phi = width/(2*g.bend_radius_mm)
        rows = [rasterize(r, phi, g, radius) for r in RESOLUTIONS]
        arcs[name] = {'half_angle_rad': phi, 'profile_radius_mm': radius,
                      'profile_arc_length_mm': round(2*radius*phi, 12),
                      'top_arc_length_mm': round(2*g.r_top*phi, 12),
                      'resolutions': rows, 'resonance_proxy_slope_vs_cell_size': slope(
                          [r['cell_size_mm'] for r in rows],
                          [r['resonance_shift_proxy_abs'] for r in rows])}
    facets = []
    for label, path in meshes:
        surfaces = mesh_surfaces(path, [g.patch_tag, g.ground_tag])
        for tag, width in [(g.patch_tag, g.patch_width_mm), (g.ground_tag, g.substrate_width_mm)]:
            tri = surfaces[tag]
            half_width = g.bend_radius_mm*float(np.max(np.abs(np.arctan2(tri[..., 0], tri[..., 2]-g.axis_z))))
            if abs(half_width-width/2) > 1e-7:
                raise ValueError(f'{label}: physical group {tag} does not span the requested arc width')
        facets.append({'level': label, 'mesh': Path(path).name,
                       'mesh_sha256': hashlib.sha256(Path(path).read_bytes()).hexdigest(),
                       'patch': facet_error(surfaces[g.patch_tag], g.r_top, g.axis_z),
                       'ground': facet_error(surfaces[g.ground_tag], g.r_bot, g.axis_z)})
    return {'_schema': 'geode-fdtd-staircasing/3',
            'scope': 'Physical bent-patch geometry; no EM solve or cost-to-accuracy claim.',
            'geometry': asdict(g),
            'provenance': {'generator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                           'mesh_coordinates': 'Already bent; no second bend or synthetic subdivision.',
                           'historical_smoke': 'Retired from current outputs; preserved in PR #1095 / commit 7baa2e16.'},
            'metric_notes': {
                'proxy': 'RMS vertical profile displacement / corresponding arc length; not solved resonance or S11.',
                'profiles': 'Patch uses the top radius and patch width; ground uses the bottom radius and full substrate width.',
                'perimeter': 'Slab sector under each footprint; taxicab paradox, not an electromagnetic error or metal-patch perimeter.',
                'fem': 'Area-weighted radial facet error on actual mesh surfaces; vertices on the arc do not make facets exact.',
                'ladder': 'Independent actual volume meshes; no surface-only midpoint refinement.',
                'comparison': 'Cartesian vertical RMS and facet radial RMS use different measures; no equal-error solver cost is inferred.'},
            'arcs': arcs, 'fem_surface_ladder': facets}


def plot(results, path):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    plt.rcParams.update({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False})
    fig, axes = plt.subplots(2, 2, figsize=(11, 8), layout='constrained')
    for name, color in [('patch', '#23679A'), ('ground', '#16847B')]:
        arc = results['arcs'][name]
        rows = arc['resolutions']
        x = [r['resolution_cells_per_mm'] for r in rows]
        axes[0, 0].loglog(x, [r['resonance_shift_proxy_abs'] for r in rows], 'o-', color=color,
                         label=f"{name}; slope {arc['resonance_proxy_slope_vs_cell_size']:.2f}")
        axes[0, 1].loglog(x, [r['boundary_pos_rms_mm']*1000 for r in rows], 'o-', color=color, label=name)
        axes[1, 1].semilogx(x, [r['perimeter_rel_error']*100 for r in rows], 'o-', color=color, label=name+' footprint')
        axes[1, 1].axhline(rows[0]['taxicab_perimeter_rel_error_limit']*100, color=color, ls='--', lw=1)
        rows = [r[name] for r in results['fem_surface_ladder']]
        levels = range(len(rows))
        axes[1, 0].semilogy(levels, [r['facet_max_radial_error_mm']*1000 for r in rows], 'o-', color=color, label=f'{name} max')
        axes[1, 0].semilogy(levels, [r['facet_rms_radial_error_mm']*1000 for r in rows], 's--', color=color, label=f'{name} RMS')
    axes[0, 0].set(title='(a) Displacement / arc-length proxy', ylabel='Dimensionless geometric proxy')
    axes[0, 1].set(title='(b) Cartesian boundary-profile error', ylabel='Vertical RMS displacement (µm)')
    labels = [row['level'] for row in results['fem_surface_ladder']]
    axes[1, 0].set(title='(c) Facet error on actual FEM meshes', ylabel='Radial distance to cylinder (µm)',
                   xlabel='Volume-mesh level', xticks=range(len(labels)), xticklabels=labels)
    axes[1, 1].set(title='(d) Perimeter: geometric diagnostic only', ylabel='Slab-sector perimeter error (%)')
    for ax in [axes[0, 0], axes[0, 1], axes[1, 1]]:
        ax.set_xlabel('Cartesian resolution (cells/mm)')
        ax.set_xticks(RESOLUTIONS, labels=RESOLUTIONS)
    for ax in axes.flat:
        ax.legend(fontsize=9)
        ax.grid(alpha=.2, which='both')
    g = results['geometry']
    fig.suptitle(f"Physical bent patch: R = {g['bend_radius_mm']:g} mm, h = {g['substrate_thickness_mm']:g} mm; "
                 f"patch {g['patch_width_mm']:g} mm, ground {g['substrate_width_mm']:g} mm\n"
                 'Geometry diagnostics only — no solved frequency, S11, or solver-cost claim', fontsize=12)
    fig.savefig(path, dpi=160)
    plt.close(fig)


def default_meshes():
    l1 = os.environ.get('PATCH_CONFORMAL_L1')
    if not l1:
        raise ValueError('Set PATCH_CONFORMAL_L1 to the regenerated L1 mesh or pass --mesh LEVEL=PATH')
    return [('L0', REPO/'crates/geode-core/tests/fixtures/patch_conformal_L0.msh'), ('L1', Path(l1))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mesh', action='append', metavar='LEVEL=PATH')
    parser.add_argument('--bend-radius', type=float, default=100.0)
    parser.add_argument('--thickness', type=float, default=1.6)
    parser.add_argument('--patch-width', type=float, default=38.0)
    parser.add_argument('--substrate-width', type=float, default=62.0)
    parser.add_argument('--patch-tag', type=int, default=12)
    parser.add_argument('--ground-tag', type=int, default=13)
    parser.add_argument('--output-dir', type=Path, default=HERE)
    parser.add_argument('--no-plot', action='store_true')
    args = parser.parse_args()
    try:
        g = Geometry(args.bend_radius, args.thickness, args.patch_width,
                     args.substrate_width, args.patch_tag, args.ground_tag)
        meshes = [(label, Path(path)) for label, path in (x.split('=', 1) for x in args.mesh)] if args.mesh else default_meshes()
        results = build_results(g, meshes)
    except (ValueError, OSError) as error:
        parser.error(str(error))
    args.output_dir.mkdir(parents=True, exist_ok=True)
    output = args.output_dir/'staircasing_results.json'
    output.write_text(json.dumps(results, indent=2, sort_keys=True, allow_nan=False)+'\n')
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    (args.output_dir/'staircasing_results.sha256').write_text(f'{digest}  {output.name}\n')
    if not args.no_plot:
        plot(results, args.output_dir/'staircasing.png')
    for name, arc in results['arcs'].items():
        vals = [r['resonance_shift_proxy_abs'] for r in arc['resolutions']]
        print(f"{name}: proxy {vals[0]:.6g} -> {vals[-1]:.6g}; slope {arc['resonance_proxy_slope_vs_cell_size']:.4f}")
    for row in results['fem_surface_ladder']:
        print(row)


if __name__ == '__main__':
    main()
