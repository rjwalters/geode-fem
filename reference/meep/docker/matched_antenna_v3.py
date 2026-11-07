#!/usr/bin/env python3
"""Matched bent-patch FDTD harness, version 3 (#1427): free-form patch outline.

Version 2 (``matched_antenna_v2.py``, kept byte-identical; its pinned provenance
is checked by ``meep_v2_verify.py``) builds a rectangular patch from
``patch_w``, ``patch_l`` and ``probe_inset``. This version replaces only that
patch-geometry input. The patch outline is read from an E4 dump
(``benchmarks/patch_antenna_conformal/geometry/*.json``) and voxelized onto the
same integer-cell Yee layout rules as v2; the FR-4 model, the substrate, the
gap, the port construction, the source, the stopping test and the grid audit
are v2's (called, not copied) or v2's logic with per-column patch rows.

Outline chart. The dump holds ``base`` and ``moved`` node coordinates in the
bent fixture frame. A node (x, y, z) maps to the arc-length chart
``s = R atan2(x, z - z_axis)``, ``y`` (R = ``bend_radius_mm``), the mid-surface
arc used by v2's ``arc(u)``. ``patch_outline_loop`` indexes the dump's local node
arrays and gives the closed outline polygon in (s, y).

Voxelization rules (v2's rules, extended column by column):

* A substrate column i with centre chart c = arc(u), u = (i + 1/2) dx - xc, is a
  patch column iff the outline contains the whole chart extent
  [c - dx/2, c + dx/2]; for a rectangle this is exactly v2's
  ``|arc(u)| <= half_width - dx/2``.
* Its metal rows are ``round((y + yc) / dx)`` of the lower and upper outline
  crossings, taking the tightest of the scanlines at c - dx/2, c and c + dx/2
  (so the voxelized column lies inside the outline up to half a cell). A
  scanline that crosses the outline other than twice raises: the column would
  not be one contiguous metal span, which ``sheet_and_risers`` assumes.
* The substrate, gap, board centre (xc, yc) and port column follow v2 from the
  dump's nominal board (patch_w, patch_l, h, sub_pad), so the initial (``base``)
  outline reproduces v2's layout exactly.
* Port: ``--port config`` uses v2's rule (config probe_w, probe_inset);
  ``--port dump`` places the dump's feed (``feed_*_s_y_mm``, ``probe_w_*``)
  snapped to whole cells, and raises if the port rows are not strictly inside
  the outline at the port column.

The geometry half of this file needs only the standard library; Meep, MPI and
numpy are imported (through v2) only when a simulation is built or run.
"""
import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

VERSION = 'matched-antenna/3'
TOL = 1e-9
SCAN_INSET_MM = 1e-7  # scanlines sit this far inside the column extent (dump noise ~1e-15 mm)
HERE = Path(__file__).resolve().parent


def whole(value, what):
    n = round(value)
    if abs(value - n) > 1e-9:
        raise ValueError(f'{what} = {value:.6g} cells is not a whole number of cells')
    return int(n)


# ------------------------------------------------------------------ outline loader

def load_outline(dump_path, which='moved'):
    """Read a patch outline polygon [(s, y)] (mm, chart) and feed/board data from an E4 dump."""
    raw = Path(dump_path).read_bytes()
    d = json.loads(raw)
    if which not in ('base', 'moved'):
        raise ValueError(f'outline must be base or moved, got {which!r}')
    radius, z_axis = float(d['bend_radius_mm']), float(d['z_axis_mm'])
    nodes, loop = d[which], d['patch_outline_loop']
    if len(set(loop)) != len(loop) or len(loop) < 3:
        raise ValueError('patch_outline_loop must list at least 3 distinct nodes')
    poly = [(radius * math.atan2(nodes[n][0], nodes[n][2] - z_axis), float(nodes[n][1])) for n in loop]
    feed = d['feed_s_y_mm'] if which == 'base' else d['feed_moved_s_y_mm']
    width = d['probe_w_mm'] if which == 'base' else d['probe_w_moved_mm']
    return dict(polygon=poly, feed_s=float(feed[0]), feed_y=float(feed[1]), probe_w=float(width),
                nominal=dict(patch_w=float(d['patch_w_mm']), patch_l=float(d['patch_l_mm']),
                             h=float(d['substrate_h_mm']), sub_pad=float(d['sub_pad_mm'])),
                bend_radius_mm=radius, which=which, dump=Path(dump_path).name,
                dump_sha256=hashlib.sha256(raw).hexdigest())


def crossings(poly, s):
    """Sorted y of the outline crossings of the vertical line at chart position s (half-open rule)."""
    ys = []
    for (s0, y0), (s1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (s0 <= s < s1) or (s1 <= s < s0):
            ys.append(y0 + (s - s0) / (s1 - s0) * (y1 - y0))
    return sorted(ys)


def column_interval(poly, c, dx):
    """(lo, hi) y of the outline over the whole chart extent of the column at c, or None if outside."""
    spans = []
    for s in (c - dx / 2 + SCAN_INSET_MM, c, c + dx / 2 - SCAN_INSET_MM):
        ys = crossings(poly, s)
        if not ys:
            return None
        if len(ys) != 2:
            raise ValueError(f'outline crosses the scanline at s = {s:.4f} mm {len(ys)} times: '
                             'the column would not be one contiguous metal span')
        spans.append(ys)
    return max(y[0] for y in spans), min(y[1] for y in spans)


# ------------------------------------------------------------------ layout

def layout3(config, resolution, geometry, outline, port='config'):
    """v2's integer-cell layout with the patch voxelized from ``outline`` (see module doc)."""
    p = config['patch']; dx = 1 / resolution
    bend = config['bend_radius_mm'] if geometry == 'bent' else None
    nom = outline['nominal']
    for key in ('patch_w', 'patch_l', 'h', 'sub_pad'):
        if abs(nom[key] - p[key]) > 1e-9:
            raise ValueError(f'config patch.{key} = {p[key]} differs from the dump board {nom[key]}')
    if geometry == 'bent' and abs(bend - outline['bend_radius_mm']) > 1e-9:
        raise ValueError('config bend radius differs from the dump')
    poly = outline['polygon']
    n_gap = whole(p['h'] / dx, 'substrate gap h/dx')
    if n_gap % 2:
        raise ValueError(f'gap of {n_gap} cells is odd; the mid-plane must be a lattice plane')
    n_pad = whole(p['sub_pad'] / dx, 'substrate pad/dx')
    n_l = int(round(p['patch_l'] / dx)); n_wd = int(round(p['patch_w'] / dx))
    xc = 0.5 * dx * (n_wd % 2); yc = 0.5 * dx * (n_l % 2)
    ja = int(round(yc / dx - n_l / 2)); jb = ja + n_l           # nominal patch Ex rows
    sa, sb = ja - n_pad, jb + n_pad

    def arc(u):
        return u if bend is None else bend * math.asin(u / bend)

    def zmid(u):
        return 0.0 if bend is None else -bend + math.sqrt(bend * bend - u * u)

    half_w = (n_wd + 2 * n_pad) * dx / 2
    lim = int(math.ceil(half_w / dx)) + 2
    sub_cols = []
    for i in range(-lim - 1, lim + 1):
        u = (i + 0.5) * dx - xc
        if abs(arc(u)) <= half_w - dx / 2 + TOL:
            sub_cols.append((i, int(round(zmid(u) / dx))))
    idx = [i for i, _ in sub_cols]
    assert idx == list(range(idx[0], idx[-1] + 1)), 'columns not contiguous'
    if (max(abs(s) for s, _ in poly) > half_w - dx or min(y for _, y in poly) + yc <= sa * dx
            or max(y for _, y in poly) + yc >= sb * dx):
        raise ValueError('the outline extends beyond the substrate board')
    patch_cols, rows = [], {}
    for i, k in sub_cols:
        u = (i + 0.5) * dx - xc
        iv = column_interval(poly, arc(u), dx)
        if iv is None:
            continue
        lo, hi = int(round((iv[0] + yc) / dx)), int(round((iv[1] + yc) / dx))
        if hi - lo < 2:
            raise ValueError(f'patch column {i} spans {hi - lo} cells; the outline is too thin for this grid')
        if lo <= sa + 1 or hi >= sb - 1:
            raise ValueError(f'patch column {i} reaches the substrate edge (rows {lo}..{hi})')
        patch_cols.append((i, k)); rows[i] = (lo, hi)
    if not patch_cols:
        raise ValueError('no column lies inside the outline')
    pidx = [i for i, _ in patch_cols]
    if pidx != list(range(pidx[0], pidx[-1] + 1)):
        raise ValueError('patch columns are not contiguous after voxelization')
    kmap = dict(sub_cols)
    ip = int(math.floor(xc / dx + 0.5 - TOL))
    assert kmap[ip - 1] == kmap[ip], 'port line sits on a staircase step'
    if ip not in rows:
        raise ValueError('the port column is outside the outline')
    km = kmap[ip]; kg, kp = km - n_gap // 2, km + n_gap // 2
    if port == 'config':
        n_w = whole(p['probe_w'] / dx, 'port width w/dx')
        target = jb * dx - p['probe_inset']
        feed_snap = None
    elif port == 'dump':
        n_w = max(1, int(round(outline['probe_w'] / dx)))
        target = outline['feed_y'] + yc
        feed_snap = abs(arc(ip * dx - xc) - outline['feed_s'])
        if feed_snap > 1.01 * dx / 2:  # arc(u) exceeds u by ~1e-3 mm at the half-cell offset
            raise ValueError(f'dump feed at s = {outline["feed_s"]} mm is {feed_snap:.3f} mm from the port column')
    else:
        raise ValueError(f'port must be config or dump, got {port!r}')
    j0 = int(round(target / dx - (n_w - 1) / 2))
    ra, rb = rows[ip]
    if not (ra < j0 and j0 + n_w - 1 < rb):
        raise ValueError(f'port rows {j0}..{j0 + n_w - 1} are not strictly inside the outline rows {ra}..{rb}')
    port_y = (j0 + (n_w - 1) / 2) * dx
    pja, pjb = min(r[0] for r in rows.values()), max(r[1] for r in rows.values())
    return dict(
        dx=dx, resolution=resolution, geometry=geometry, bend_radius_mm=bend,
        n_gap=n_gap, n_port_rows=n_w, n_pad=n_pad, n_patch_l=pjb - pja, n_patch_w=n_wd,
        board_centre_mm=[xc, yc], patch_rows=[pja, pjb], substrate_rows=[sa, sb],
        patch_columns=patch_cols, substrate_columns=sub_cols,
        port_line_i=ip, port_rows=[j0, j0 + n_w - 1], port_planes=[kg, kp],
        port_centre_mm=[ip * dx, port_y, (kg + kp) / 2 * dx],
        patch_length_mm=(pjb - pja) * dx, patch_width_mm=len(patch_cols) * dx,
        patch_y_edges_mm=[pja * dx, pjb * dx],
        probe_inset_from_patch_edge_mm=rb * dx - port_y,
        port_width_mm=n_w * dx, gap_mm=n_gap * dx,
        nominal={'patch_l': p['patch_l'], 'patch_w': p['patch_w'], 'h': p['h'],
                 'probe_w': p['probe_w'], 'probe_inset': p['probe_inset']},
        patch_column_rows=[[i, *rows[i]] for i in pidx],
        outline={'dump': outline['dump'], 'dump_sha256': outline['dump_sha256'], 'which': outline['which'],
                 'vertices': len(poly), 'port': port, 'feed_snap_error_mm': feed_snap})


# ------------------------------------------------------------------ Meep side (lazy)

OPTIONS = {'dump': None, 'which': 'moved', 'port': 'dump'}
_V2 = None


def v2mod():
    global _V2
    if _V2 is None:
        sys.path.insert(0, str(HERE))
        import matched_antenna_v2
        _V2 = matched_antenna_v2
    return _V2


def current_layout(config, resolution, geometry):
    return layout3(config, resolution, geometry, load_outline(OPTIONS['dump'], OPTIONS['which']),
                   OPTIONS['port'])


def patch_sheet_and_risers(L, offset, dx, e):
    """Zero-thickness patch sheet per column over its own rows, plus risers where the plane steps."""
    v2 = v2mod()
    rows = {i: (a, b) for i, a, b in L['patch_column_rows']}
    runs = []  # [i0, i1, k, ja, jb]
    for i, k in L['patch_columns']:
        ja, jb = rows[i]
        if runs and runs[-1][1] == i and runs[-1][2:] == [k, ja, jb]:
            runs[-1][1] = i + 1
        else:
            runs.append([i, i + 1, k, ja, jb])
    out = []
    for i0, i1, k, ja, jb in runs:
        z = (k + offset) * dx
        out.append(v2.box(i0 * dx - e, i1 * dx + e, ja * dx - e, jb * dx + e, z - e, z + e, v2.mp.metal))
    for a, b in zip(runs, runs[1:]):
        if a[2] == b[2]:
            continue
        ya, yb = max(a[3], b[3]) * dx - e, min(a[4], b[4]) * dx + e
        za, zb = sorted(((a[2] + offset) * dx, (b[2] + offset) * dx))
        out.append(v2.box(a[1] * dx - e, a[1] * dx + e, ya, yb, za - e, zb + e, v2.mp.metal))
    return out


def antenna_setup3(config, resolution, geometry, loss):
    """v2.antenna_setup with the patch sheet built from the outline layout."""
    v2 = v2mod(); mp = v2.mp
    L = current_layout(config, resolution, geometry)
    p = config['patch']; d = config['domain']; dx = L['dx']; e = 1e-3 * dx
    n = L['n_gap']; half = n // 2
    cell = mp.Vector3(p['patch_w'] + 2 * p['sub_pad'] + 2 * (d['air_margin'] + d['pml_thick']),
                      p['patch_l'] + 2 * p['sub_pad'] + 2 * (d['air_margin'] + d['pml_thick']),
                      p['h'] + 2 * (d['air_margin'] + d['pml_thick']))
    eps_r = config['materials']['substrate_eps_r']
    fr4 = v2.fr4_medium(loss)
    w0 = 2 * math.pi * loss['anchor_ghz'] / v2.C_MM_NS
    eps_face = (1 + eps_r) / 2
    face = mp.Medium(epsilon=eps_face, D_conductivity=eps_r * loss['tan_delta'] * w0 / (2 * eps_face))
    width, length = L['n_port_rows'] * dx, n * dx
    sigma = length * v2.ETA0 / (config['port_resistance_ohm'] * width * dx)
    loss0 = loss['tan_delta'] * w0
    resistor = mp.Medium(epsilon=eps_r, D_conductivity_diag=mp.Vector3(
        loss0, loss0 + sigma / eps_r, loss0 + sigma / eps_r))
    sa, sb = L['substrate_rows']
    geo = []
    for i0, i1, k in v2.runs_of(L['substrate_columns']):
        z = (k + half) * dx
        geo.append(v2.box(i0 * dx - e, i1 * dx + e, sa * dx - e, sb * dx + e, z - e, z + e, face))
    for i0, i1, k in v2.runs_of(L['substrate_columns']):
        geo.append(v2.box(i0 * dx - e, i1 * dx + e, sa * dx - e, sb * dx + e,
                          (k - half) * dx - e, (k + half) * dx - e, fr4))
    ip = L['port_line_i']; j0, j1 = L['port_rows']; kg, kp = L['port_planes']
    port_block = v2.box(ip * dx - dx / 2 + e, ip * dx + dx / 2 - e,
                        (j0 - 0.5) * dx + e, (j1 + 0.5) * dx - e, kg * dx + e, kp * dx - e, resistor)
    geo.append(port_block)
    geo += v2.sheet_and_risers(L['substrate_columns'], L['substrate_rows'], -half, dx, e)
    geo += patch_sheet_and_risers(L, +half, dx, e)
    samples = [(ip * dx, j * dx, (k + 0.5) * dx) for j in range(j0, j1 + 1) for k in range(kg, kp)]
    return dict(cell=cell, geometry=geo, boundary=[mp.PML(d['pml_thick'])], layout=L,
                samples=samples, amplitude=dx * dx, monitor_samples=samples, line_length=length,
                sigma=sigma, reference_z=config['port_resistance_ohm'], resistor_block=port_block,
                media={'fr4_eps_inf': loss['eps_inf'], 'resistor_eps': eps_r, 'face_eps': eps_face},
                spatial_current=width, periodic=False, default_material=mp.air,
                discrete_port_ohm=v2.ETA0 * length / (sigma * width * dx))


def audit_grid3(sim, setup):
    """v2's grid audit with every row-dependent check taken on the column's own metal rows."""
    v2 = v2mod(); mp = v2.mp; YEE = v2.YEE
    L = setup['layout']; dx = L['dx']; n = L['n_gap']; half = n // 2
    chi = v2.chi_reader(sim); cls, table = v2.classifier(setup['media'])
    errors, out = [], {'class_values': table}
    rows = {i: (a, b) for i, a, b in L['patch_column_rows']}
    pc = dict(L['patch_columns'])

    def want(cond, msg):
        if not cond:
            errors.append(msg)

    def code(comp, i, j, k):
        return cls(chi(comp, v2.pos(comp, i, j, k, dx)))

    def mid(i):
        return (rows[i][0] + rows[i][1]) // 2

    ip = L['port_line_i']; kmid = dict(L['substrate_columns'])[ip]
    jrow = mid(ip)
    conv = {}
    for comp in YEE:
        pure = all(code(comp, ip, jrow, kmid + k) != '?' for k in range(-half - 3, half + 3))
        o = YEE[comp]
        shifted = [cls(chi(comp, ((ip + o[0]) * dx, (jrow + o[1]) * dx, (kmid + k + o[2] + 0.5) * dx)))
                   for k in range(-half - 3, half + 3)]
        conv[comp] = {'assumed_pure': pure, 'shifted_has_mixed': '?' in shifted}
        want(pure and '?' in shifted, f'{comp} z-offset convention')
    i_edge = L['patch_columns'][-1][0]; jr_edge = mid(i_edge)
    jb_ip = rows[ip][1]
    for comp, axis in (('Ex', 0), ('Ey', 1), ('Ez', 0)):
        o = list(YEE[comp])
        if axis == 0:
            line = [(i, jr_edge) for i in range(i_edge - 3, i_edge + 4)]
        else:
            line = [(ip, j) for j in range(jb_ip - 3, jb_ip + 4)]
        kplane = pc[line[3][0] if axis == 0 else ip] + half
        kz = kplane if comp != 'Ez' else kplane - 1
        vals = [cls(chi(comp, ((i + o[0]) * dx, (j + o[1]) * dx, (kz + o[2]) * dx))) for i, j in line]
        o2 = list(o); o2[axis] += 0.5
        sh = [cls(chi(comp, ((i + o2[0]) * dx, (j + o2[1]) * dx, (kz + o2[2]) * dx))) for i, j in line]
        conv[f'{comp}_axis{axis}'] = {'assumed': ''.join(vals), 'shifted': ''.join(sh)}
        want('?' not in vals, f'{comp} axis-{axis} offset convention (assumed offsets mixed)')
        if comp != 'Ez':
            want('?' in sh, f'{comp} axis-{axis} offset convention (shifted offsets pure)')
    out['lattice_convention'] = conv
    # 2. Per-column gap under the patch, on a row inside each column's own metal span.
    hist_h, hist_e, steps = {}, {}, 0
    for i, k in L['patch_columns']:
        col = ''.join(code('Ex', i, mid(i), k + q) for q in range(-half - 2, half + 3))
        ok = col == 'AAM' + 'F' * (n - 1) + 'MAA'
        want(ok, f'Ex column {i}: {col}')
        hist_h[n] = hist_h.get(n, 0) + ok
    ez_rows, skipped = {}, 0
    for e_ in [i for i, _ in L['patch_columns'][1:]]:
        lo_r, hi_r = max(rows[e_ - 1][0], rows[e_][0]), min(rows[e_ - 1][1], rows[e_][1])
        if hi_r - lo_r < 2:
            skipped += 1
            continue
        jr = (lo_r + hi_r) // 2
        ks = [pc[e_ - 1], pc[e_]]
        lo, hi = min(ks) - half - 2, max(ks) + half + 2
        col = ''.join(code('Ez', e_, jr, q) for q in range(lo, hi))
        free = max((len(r) for r in col.replace('M', ' ').replace('A', ' ').replace('I', ' ').split()), default=0)
        step = len(set(ks)) > 1
        steps += step
        want(col == ('AAM' + 'F' * (n - 1) + 'MAA' if step else 'AA' + 'F' * n + 'AA'),
             f'Ez {"step " if step else ""}line {e_}: {col}')
        hist_e[free] = hist_e.get(free, 0) + 1
        ez_rows[e_] = col
    out['patch_columns'] = len(L['patch_columns'])
    out['ex_columns_with_n_gap'] = hist_h.get(n, 0)
    out['ez_free_run_histogram'] = {str(k): v for k, v in sorted(hist_e.items())}
    out['ez_step_lines'] = steps
    out['ez_lines'] = len(L['patch_columns']) - 1 - skipped
    # 3. Port (v2 checks, unchanged).
    j0, j1 = L['port_rows']; kg, kp = L['port_planes']
    blk = setup['resistor_block']
    rcode = 'R' if 'R' in table else 'F'

    def member(comp, i, j, k):
        return 'in' if mp.is_point_in_object(mp.Vector3(*v2.pos(comp, i, j, k, dx)), blk) else 'out'
    groups = {
        'ez': [('Ez', ip, j, k) for j in range(j0, j1 + 1) for k in range(kg, kp)],
        'ez_rows_outside': [('Ez', ip, j, k) for j in (j0 - 1, j1 + 1) for k in range(kg, kp)],
        'ez_lines_beside': [('Ez', i, j, k) for i in (ip - 1, ip + 1) for j in range(j0, j1 + 1)
                            for k in range(kg, kp)],
        'ey_inner': [('Ey', ip, j, k) for j in range(j0, j1) for k in range(kg + 1, kp)],
        'ey_outer': [('Ey', ip, j, k) for j in (j0 - 1, j1) for k in range(kg + 1, kp)]}
    port = {}
    for name, pts in groups.items():
        port[name] = ''.join(code(*q) for q in pts)
        port[name + '_membership'] = sorted({member(*q) for q in pts})
    want(port['ez'] == rcode * (L['n_port_rows'] * n) and port['ez_membership'] == ['in'], 'port Ez samples')
    for name in ('ez_rows_outside', 'ez_lines_beside', 'ey_outer'):
        want(set(port[name]) == {'F'} and port[name + '_membership'] == ['out'], f'port {name}')
    want(set(port['ey_inner']) == {rcode} and port['ey_inner_membership'] == ['in'], 'port Ey links')
    want(all(mp.is_point_in_object(mp.Vector3(*q), blk) for q in setup['samples']),
         'source samples outside the resistor')
    out['port'] = port
    out['port_discrete_ohm'] = setup['discrete_port_ohm']
    # 4. Metal extent of every patch column: Ex rows ja..jb, Ey rows ja..jb-1, open face beyond.
    bad = 0
    for i, k in L['patch_columns']:
        ja, jb = rows[i]; kpl = k + half
        ex = ''.join(code('Ex', i, j, kpl) for j in range(ja - 2, jb + 3))
        ey = ''.join(code('Ey', i, j, kpl) for j in range(ja - 2, jb + 2))
        bad += not (ex == 'II' + 'M' * (jb - ja + 1) + 'II' and ey == 'II' + 'M' * (jb - ja) + 'II')
        want(ex.count('M') == jb - ja + 1, f'patch Ex rows of column {i}: {ex}')
        want(ey.count('M') == jb - ja, f'patch Ey rows of column {i}: {ey}')
    out['patch_columns_with_wrong_metal_rows'] = bad
    for comp in ('Ex', 'Ez'):
        cols = [i for i, _ in L['substrate_columns']] + [L['substrate_columns'][-1][0] + 1]
        cols = [i for i in cols if i >= ip]
        kmin = min(k for _, k in L['substrate_columns']) - half - 2
        kmax = max(k for _, k in L['substrate_columns']) + half + 2
        out[f'slice_{comp}'] = {'i0': cols[0], 'k0': kmin, 'j': jrow, 'rows': [
            ''.join(code(comp, i, jrow, k) for i in cols) for k in range(kmin, kmax + 1)]}
    out['errors'] = errors
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, add_help=False,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--outline', type=Path, help='E4 dump JSON holding patch_outline_loop (required)')
    ap.add_argument('--outline-geometry', choices=['base', 'moved'], default='moved')
    ap.add_argument('--port', choices=['config', 'dump'], default='dump')
    ap.add_argument('-h', '--help', action='store_true')
    own, rest = ap.parse_known_args()
    if own.help:
        ap.print_help()
        print('\nAll other options are v2\'s (--config, --resolution, --output, --audit-only, ...).')
        return
    if own.outline is None:
        ap.error('--outline is required')
    OPTIONS.update(dump=own.outline, which=own.outline_geometry, port=own.port)
    v2 = v2mod()
    v3_sha = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    original_write = v2.write

    def write3(path, result, t_import):
        result.update(schema='geode-matched-meep/3', harness_version=VERSION, source_sha256=v3_sha,
                      v2_source_sha256=hashlib.sha256(Path(v2.__file__).read_bytes()).hexdigest())
        original_write(path, result, t_import)

    v2.antenna_setup, v2.audit_grid, v2.write = antenna_setup3, audit_grid3, write3
    sys.argv = [sys.argv[0], *rest]
    v2.main()


if __name__ == '__main__':
    main()
