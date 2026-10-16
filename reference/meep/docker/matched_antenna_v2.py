#!/usr/bin/env python3
"""Matched bent-patch FDTD harness, version 2 (#1146).

Version 1 (``matched_antenna.py``) is kept unchanged for the committed R1/R2/R4
and diagnosis records. DIAGNOSIS.md traced its Meep/GEODE disagreement on the
bent board to the discretization of its one-cell metal shells. This version
builds every conductor, the gap, the port and the source on Meep's Yee lattice,
in integer cell indices:

* **Lattice.** Meep places the origin on a lattice point; E_a is sampled half a
  cell off the lattice along a and on it along the other axes (Ez at
  (i, j, k + 1/2) dx, Ex at (i + 1/2, j, k) dx, Ey at (i, j + 1/2, k) dx). The
  convention is checked against Meep's own chi1inv after initialization
  (``audit_grid``), and the run aborts before stepping if it does not hold.
* **Metal.** Patch and ground are zero-thickness PEC sheets on tangential-E
  planes z = k dx (only the Ex/Ey samples on the plane are metal, as for
  GEODE's PEC faces). The curved board is a staircase of vertical columns
  [i dx, (i + 1) dx]. Each column takes the plane k_mid nearest the curved
  mid-surface at its centre; its ground is k_mid - n/2 and its patch
  k_mid + n/2, so **every column has the same n-cell gap for Ex, Ey and Ez**.
  Where neighbouring columns differ in k_mid, a vertical one-cell PEC riser on
  the shared Ez line closes each conductor.
* **Gap.** n = h/dx must be a whole, even number: 4 at R2.5, 8 at R5.
* **Port.** A resistor on one Ez line with n_w = w/dx rows and n samples per
  row, so its discrete resistance is exactly R = 50 ohm. The Norton source is
  one point current per resistor Ez sample (no interpolation onto neighbouring
  samples), and the voltage is the discrete line integral of the same samples,
  read by point DFT monitors on the Yee grid.
* **Loss.** FR-4 is a Lorentzian high above the band plus D-conductivity,
  solved so that eps' = 4.4, tan(delta) = 0.02 and d tan(delta)/df = 0 at
  ``--loss-anchor-ghz``. The realized eps(f) over the DFT window is recorded.
* **Lengths that are not whole cells** are snapped and recorded in
  ``layout``: the patch length 29 mm is 72 cells (28.8 mm) at R2.5 and exactly
  145 cells at R5. A board centre offset of dx/2 is used where a cell count is
  odd, so the patch stays centred on its own cells.

Length unit mm; Meep frequency f_GHz / 299.792458; fields exp(-i w t).
Modes: antenna (default), ``--rc-oracle`` (uniform periodic RC with the point
source and point monitors), ``--material-oracle`` (the FR-4 model in a periodic
cell, eps(f) recovered from V/I), ``--probe`` (lattice, source deposition and
monitor readout in small cells), ``--audit-only`` (antenna grid audit, no
stepping). No spatial convergence is declared by any single run.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import platform
import resource
import time
from pathlib import Path

import numpy as np
import meep as mp
from mpi4py import MPI

C_MM_NS = 299.792458
ETA0 = 376.730313668
COMM = MPI.COMM_WORLD
VERSION = 'matched-antenna/2'
HARNESS_FCEN_GHZ = 2.45  # v1 Gaussian pulse centre, kept
TOL = 1e-9


def whole(value, what):
    n = round(value)
    if abs(value - n) > 1e-9:
        raise ValueError(f'{what} = {value:.6g} cells is not a whole number of cells')
    return int(n)


# ------------------------------------------------------------------ FR-4 loss

def fr4_epsilon(f, eps_inf, sigma_d, sigma_l, f_n, gamma):
    """Meep's eps(f) for D-conductivity times (eps_inf + one Lorentzian), exp(-i w t)."""
    chi = sigma_l * f_n**2 / (f_n**2 - f**2 - 1j * f * gamma)
    return (1 + 1j * sigma_d / (2 * math.pi * f)) * (eps_inf + chi)


def design_fr4(eps_r, tan_delta, anchor_ghz, lorentz_ratio=20.0):
    """Solve eps_inf, sigma_D, sigma_L: eps'=eps_r, tan d = tan_delta, d tan d/df = 0 at anchor."""
    f0 = anchor_ghz / C_MM_NS
    f_n = lorentz_ratio * f0
    gamma = f_n

    def tand(f, x):
        e = fr4_epsilon(f, *x, f_n, gamma)
        return e.imag / e.real

    def residual(x):
        e = fr4_epsilon(f0, *x, f_n, gamma)
        h = 1e-4 * f0
        return np.array([e.real - eps_r, e.imag / e.real - tan_delta,
                         (tand(f0 + h, x) - tand(f0 - h, x)) / (2 * h) * f0])

    x = np.array([eps_r - 0.9, 2 * math.pi * f0 * tan_delta / 2, 0.9])
    for _ in range(50):
        r = residual(x)
        if np.max(np.abs(r)) < 1e-13:
            break
        jac = np.empty((3, 3))
        for k in range(3):
            d = np.zeros(3); d[k] = 1e-7 * max(1.0, abs(x[k]))
            jac[:, k] = (residual(x + d) - r) / d[k]
        x = x - np.linalg.solve(jac, r)
    r = residual(x)
    assert np.max(np.abs(r)) < 1e-10, ('FR-4 loss design did not converge', r)
    eps_inf, sigma_d, sigma_l = (float(v) for v in x)
    return dict(model='lorentz', eps_inf=eps_inf, sigma_d=sigma_d, sigma_l=sigma_l, f_n=f_n, gamma=gamma,
                anchor_ghz=anchor_ghz, lorentz_ratio=lorentz_ratio, eps_r=eps_r,
                tan_delta=tan_delta)


def design_conductivity(eps_r, tan_delta, anchor_ghz):
    """v1-style loss (D-conductivity only), anchored at anchor_ghz: tan d = tan_delta * f0 / f."""
    f0 = anchor_ghz / C_MM_NS
    return dict(model='conductivity', eps_inf=eps_r, sigma_d=2 * math.pi * f0 * tan_delta,
                sigma_l=0.0, f_n=1.0, gamma=1.0, anchor_ghz=anchor_ghz, eps_r=eps_r,
                tan_delta=tan_delta)


def fr4_medium(d):
    if d.get('model') == 'conductivity':
        return mp.Medium(epsilon=d['eps_inf'], D_conductivity=d['sigma_d'])
    return mp.Medium(epsilon=d['eps_inf'], D_conductivity=d['sigma_d'],
                     E_susceptibilities=[mp.LorentzianSusceptibility(
                         frequency=d['f_n'], gamma=d['gamma'], sigma=d['sigma_l'])])


def fr4_table(d, f_ghz):
    rows = []
    for g in f_ghz:
        e = fr4_epsilon(g / C_MM_NS, d['eps_inf'], d['sigma_d'], d['sigma_l'], d['f_n'], d['gamma'])
        rows.append([float(g), float(e.real), float(e.imag / e.real)])
    return rows


# ------------------------------------------------------------------ layout

def layout(config, resolution, geometry):
    """Integer-cell layout of the board. All positions are absolute, mm."""
    p = config['patch']; dx = 1 / resolution
    bend = config['bend_radius_mm'] if geometry == 'bent' else None
    n_gap = whole(p['h'] / dx, 'substrate gap h/dx')
    if n_gap % 2:
        raise ValueError(f'gap of {n_gap} cells is odd; the mid-plane must be a lattice plane')
    n_w = whole(p['probe_w'] / dx, 'port width w/dx')
    n_pad = whole(p['sub_pad'] / dx, 'substrate pad/dx')
    n_l = int(round(p['patch_l'] / dx))  # round half to even: 72.5 -> 72 at R2.5
    n_wd = int(round(p['patch_w'] / dx))
    # Board centre: on a lattice line for an even cell count, half a cell off for odd.
    xc = 0.5 * dx * (n_wd % 2)
    yc = 0.5 * dx * (n_l % 2)
    ja = int(round(yc / dx - n_l / 2)); jb = ja + n_l          # patch Ex rows ja..jb
    sa, sb = ja - n_pad, jb + n_pad                             # substrate/ground rows

    def arc(u):  # arc length on the mid-surface of a column centre at u from the crown
        return u if bend is None else bend * math.asin(u / bend)

    def zmid(u):
        return 0.0 if bend is None else -bend + math.sqrt(bend * bend - u * u)

    def columns(half_width):
        lim = int(math.ceil(half_width / dx)) + 2
        out = []
        for i in range(-lim - 1, lim + 1):
            u = (i + 0.5) * dx - xc
            if abs(arc(u)) <= half_width - dx / 2 + TOL:
                out.append((i, int(round(zmid(u) / dx))))
        idx = [i for i, _ in out]
        assert idx == list(range(idx[0], idx[-1] + 1)), 'columns not contiguous'
        return out

    patch_cols = columns(n_wd * dx / 2)
    sub_cols = columns((n_wd + 2 * n_pad) * dx / 2)
    # Port: the Ez line nearest the board centre (x = 0 for both parities).
    ip = int(math.floor(xc / dx + 0.5 - TOL))
    kmap = dict(sub_cols)
    assert kmap[ip - 1] == kmap[ip], 'port line sits on a staircase step'
    km = kmap[ip]
    kg, kp = km - n_gap // 2, km + n_gap // 2
    # Port rows j0..j0+n_w-1 centred as close as possible to inset from the patch +y edge.
    target = jb * dx - p['probe_inset']
    j0 = int(round(target / dx - (n_w - 1) / 2))
    assert ja < j0 and j0 + n_w - 1 < jb
    port_y = (j0 + (n_w - 1) / 2) * dx
    return dict(
        dx=dx, resolution=resolution, geometry=geometry, bend_radius_mm=bend,
        n_gap=n_gap, n_port_rows=n_w, n_pad=n_pad, n_patch_l=n_l, n_patch_w=n_wd,
        board_centre_mm=[xc, yc], patch_rows=[ja, jb], substrate_rows=[sa, sb],
        patch_columns=patch_cols, substrate_columns=sub_cols,
        port_line_i=ip, port_rows=[j0, j0 + n_w - 1], port_planes=[kg, kp],
        port_centre_mm=[ip * dx, port_y, (kg + kp) / 2 * dx],
        patch_length_mm=n_l * dx, patch_width_mm=len(patch_cols) * dx,
        patch_y_edges_mm=[ja * dx, jb * dx],
        probe_inset_from_patch_edge_mm=jb * dx - port_y,
        port_width_mm=n_w * dx, gap_mm=n_gap * dx,
        nominal={'patch_l': p['patch_l'], 'patch_w': p['patch_w'], 'h': p['h'],
                 'probe_w': p['probe_w'], 'probe_inset': p['probe_inset']})


def runs_of(cols):
    """Group (i, k) columns into [i0, i1, k] runs of equal plane index."""
    out = []
    for i, k in cols:
        if out and out[-1][2] == k and out[-1][1] == i:
            out[-1][1] = i + 1
        else:
            out.append([i, i + 1, k])
    return out


def box(x0, x1, y0, y1, z0, z1, material):
    return mp.Block(center=mp.Vector3((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2),
                    size=mp.Vector3(x1 - x0, y1 - y0, z1 - z0), material=material)


def sheet_and_risers(cols, rows, offset, dx, e):
    """Zero-thickness PEC sheet on plane k + offset of each column, plus step risers."""
    ya, yb = rows[0] * dx - e, rows[1] * dx + e
    out = []
    rs = runs_of(cols)
    for i0, i1, k in rs:
        z = (k + offset) * dx
        out.append(box(i0 * dx - e, i1 * dx + e, ya, yb, z - e, z + e, mp.metal))
    for a, b in zip(rs, rs[1:]):
        za, zb = sorted(((a[2] + offset) * dx, (b[2] + offset) * dx))
        out.append(box(a[1] * dx - e, a[1] * dx + e, ya, yb, za - e, zb + e, mp.metal))
    return out


def antenna_setup(config, resolution, geometry, loss):
    L = layout(config, resolution, geometry)
    p = config['patch']; d = config['domain']; dx = L['dx']; e = 1e-3 * dx
    n = L['n_gap']; half = n // 2
    cell = mp.Vector3(p['patch_w'] + 2 * p['sub_pad'] + 2 * (d['air_margin'] + d['pml_thick']),
                      p['patch_l'] + 2 * p['sub_pad'] + 2 * (d['air_margin'] + d['pml_thick']),
                      p['h'] + 2 * (d['air_margin'] + d['pml_thick']))
    eps_r = config['materials']['substrate_eps_r']
    fr4 = fr4_medium(loss)
    w0 = 2 * math.pi * loss['anchor_ghz'] / C_MM_NS
    # Tangential samples on the open top face of the substrate: arithmetic mean
    # of FR-4 and air (exact averaging for a grid-aligned tangential interface).
    eps_face = (1 + eps_r) / 2
    face = mp.Medium(epsilon=eps_face, D_conductivity=eps_r * loss['tan_delta'] * w0 / (2 * eps_face))
    # Resistor: non-dispersive eps_r (it occupies one Ez line), D-conductivity = FR-4 loss
    # at the anchor plus the port conductance. Discrete R = n dx / (sigma n_w dx * dx).
    width, length = L['n_port_rows'] * dx, n * dx
    sigma = length * ETA0 / (config['port_resistance_ohm'] * width * dx)
    loss0 = loss['tan_delta'] * w0
    resistor = mp.Medium(epsilon=eps_r, D_conductivity_diag=mp.Vector3(
        loss0, loss0 + sigma / eps_r, loss0 + sigma / eps_r))
    sa, sb = L['substrate_rows']
    geo = []
    for i0, i1, k in runs_of(L['substrate_columns']):  # open-face samples first ...
        z = (k + half) * dx
        geo.append(box(i0 * dx - e, i1 * dx + e, sa * dx - e, sb * dx + e, z - e, z + e, face))
    for i0, i1, k in runs_of(L['substrate_columns']):  # ... FR-4 overrides at steps
        geo.append(box(i0 * dx - e, i1 * dx + e, sa * dx - e, sb * dx + e,
                       (k - half) * dx - e, (k + half) * dx - e, fr4))
    ip = L['port_line_i']; j0, j1 = L['port_rows']; kg, kp = L['port_planes']
    port_block = box(ip * dx - dx / 2 + e, ip * dx + dx / 2 - e,
                     (j0 - 0.5) * dx + e, (j1 + 0.5) * dx - e, kg * dx + e, kp * dx - e, resistor)
    geo.append(port_block)
    geo += sheet_and_risers(L['substrate_columns'], L['substrate_rows'], -half, dx, e)
    geo += sheet_and_risers(L['patch_columns'], L['patch_rows'], +half, dx, e)
    samples = [(ip * dx, j * dx, (k + 0.5) * dx) for j in range(j0, j1 + 1) for k in range(kg, kp)]
    return dict(cell=cell, geometry=geo, boundary=[mp.PML(d['pml_thick'])], layout=L,
                samples=samples, amplitude=dx * dx, monitor_samples=samples, line_length=length,
                sigma=sigma,
                reference_z=config['port_resistance_ohm'], resistor_block=port_block,
                media={'fr4_eps_inf': loss['eps_inf'], 'resistor_eps': eps_r, 'face_eps': eps_face},
                spatial_current=width, periodic=False, default_material=mp.air,
                discrete_port_ohm=ETA0 * length / (sigma * width * dx))


def oracle_setup(resolution, material=None, loss=None, sheet=False):
    """Uniform periodic RC (v1 rc_setup values) or the FR-4 material oracle, 2 x 2 x 2 mm.

    Volume variant (default): a point current of moment dx^3 on every Ez sample
    gives a uniform current density, so the field is uniform and the analytic
    capacitor is exact; V is read by point DFT monitors on the interior Ez
    samples of the z-line x = y = 0 (the antenna's readout). Sheet variant
    (``sheet``): point currents of moment dx^2 on the x = 0 plane only, read
    with v1's box monitor and extraction, to compare with the committed v1
    rc_sheet spectra (the sheet-equivalent normalization the antenna uses).
    Material oracle: the cell is the FR-4 model with an extra sigma_R = 0.01 on
    Ez to damp the static charge; eps(f) is recovered from V/I exactly.
    """
    dx = 1 / resolution
    length = 2.; area = 4.; eps = 4.; resistance = 50.
    n = whole(length / dx, 'oracle cell')
    lat = [m for m in range(-n, n + 1) if -1 - TOL <= m * dx < 1 - TOL]        # x, y lattice
    half = [m for m in range(-n, n + 1) if -1 - TOL <= (m + 0.5) * dx < 1 - TOL]  # z, Ez
    assert len(lat) == n and len(half) == n
    xs = [0] if sheet else lat
    samples = [(i * dx, j * dx, (k + 0.5) * dx) for i in xs for j in lat for k in half]
    line = [(0.0, 0.0, (k + 0.5) * dx) for k in half if abs((k + 0.5) * dx) < 1 - TOL]
    if material:
        sigma_r = 0.01
        med = mp.Medium(epsilon=loss['eps_inf'], D_conductivity_diag=mp.Vector3(
            loss['sigma_d'], loss['sigma_d'], loss['sigma_d'] + sigma_r),
            E_susceptibilities=[mp.LorentzianSusceptibility(
                frequency=loss['f_n'], gamma=loss['gamma'], sigma=loss['sigma_l'])])
        extra = dict(sigma_r=sigma_r, reference_z=None)
    else:
        sigma = ETA0 * length / (resistance * area)
        med = mp.Medium(epsilon=eps, D_conductivity_diag=mp.Vector3(0, 0, sigma / eps))
        extra = dict(reference_z=resistance, analytic_capacitance_natural=eps * area / length)
    out = dict(cell=mp.Vector3(2, 2, length), geometry=[], boundary=[], default_material=med,
               samples=samples, amplitude=dx * dx if sheet else dx ** 3,
               spatial_current=2.0 if sheet else 4.0, periodic=True,
               capacitance0_natural=area / length, **extra)
    if sheet:
        out['box_monitor'] = dict(size=mp.Vector3(2, 2, length), area=area)
    else:
        # Uniform field: the line integral is length x the mean of the sampled Ez.
        out.update(monitor_samples=line, line_length=length)
    return out


# ------------------------------------------------------------------ spectrum

def point_sources(src, samples, amplitude):
    # Antenna: sheet-equivalent dipole moment per sample, K w h / (n_w n) = dx^2.
    return [mp.Source(src, mp.Ez, center=mp.Vector3(*q), amplitude=amplitude) for q in samples]


MONITOR_NUDGE = 1e-7  # cells


def point_monitors(sim, samples, frequencies, dx):
    # A zero-size yee_grid DFT volume exactly on a binary-exact sample position
    # (e.g. R2, dx = 0.5) reads 0 in Meep 1.34; 1e-7 dx off the sample it reads
    # the sample with a 1e-7 relative weight on the neighbour (probe and oracles).
    d = MONITOR_NUDGE * dx
    return [sim.add_dft_fields([mp.Ez], frequencies, center=mp.Vector3(q[0] + d, q[1] + d, q[2] + d),
                               size=mp.Vector3(), yee_grid=True) for q in samples]


def read_point(sim, mon, i):
    a = np.asarray(sim.get_dft_array(mon, mp.Ez, i)).ravel()
    assert a.size == 1, f'point DFT monitor returned {a.size} values'
    return complex(a[0])


def spectrum(sim, monitors, src, setup, frequencies):
    nf = len(frequencies)
    if 'box_monitor' in setup:  # v1 readout (sheet-equivalence oracle only)
        _, _, _, w = sim.get_array_metadata(dft_cell=monitors[0])
        voltage = np.array([np.sum(w * sim.get_dft_array(monitors[0], mp.Ez, i))
                            for i in range(nf)]) / setup['box_monitor']['area']
    else:
        volts = np.zeros(nf, complex)
        for mon in monitors:
            for i in range(nf):
                volts[i] += read_point(sim, mon, i)
        # Line integral of Ez: antenna, n dx times the mean over the n_w x n
        # resistor samples, i.e. the row-averaged discrete sum.
        voltage = volts * setup['line_length'] / len(monitors)
    # Same source correction as v1 (validated by the RC oracle): Meep 1.34's
    # Gaussian transform omits f/fcen; the forward-differenced dipole adds
    # sinc(f dt) exp(-i pi f dt). Ampere's law has -J_impressed on its RHS.
    st = np.array([src.fourier_transform(f) * (f / src.frequency) * np.sinc(f * sim.fields.dt)
                   * np.exp(-1j * np.pi * f * sim.fields.dt) for f in frequencies])
    injected = -setup['spatial_current'] * st
    if setup.get('reference_z') is None:
        return voltage, injected, ETA0 * voltage / injected, None
    current = injected - voltage * ETA0 / setup['reference_z']
    impedance = ETA0 * voltage / current
    reflection = (impedance - setup['reference_z']) / (impedance + setup['reference_z'])
    return voltage, injected, impedance, reflection


# ------------------------------------------------------------------ audit

def chi_reader(sim):
    comps = {'Ex': (mp.Ex, mp.X), 'Ey': (mp.Ey, mp.Y), 'Ez': (mp.Ez, mp.Z)}

    def chi(comp, q):
        c, d = comps[comp]
        v = complex(sim.fields.get_chi1inv(c, d, mp.vec(*q)))
        assert abs(v.imag) < 1e-12, v
        return v.real
    return chi


def classifier(media):
    table = {'M': 0.0, 'A': 1.0, 'F': 1 / media['fr4_eps_inf'], 'R': 1 / media['resistor_eps'],
             'I': 1 / media['face_eps']}
    if abs(table['R'] - table['F']) < 1e-9:  # conductivity-only FR-4: resistor told apart by membership
        del table['R']

    def cls(v):
        for k, t in table.items():
            if abs(v - t) < 1e-9:
                return k
        return '?'
    return cls, table


YEE = {'Ex': (0.5, 0.0, 0.0), 'Ey': (0.0, 0.5, 0.0), 'Ez': (0.0, 0.0, 0.5)}


def pos(comp, i, j, k, dx):
    o = YEE[comp]
    return ((i + o[0]) * dx, (j + o[1]) * dx, (k + o[2]) * dx)


def audit_grid(sim, setup):
    """Check the discrete board against Meep's chi1inv; raise before stepping on any mismatch."""
    L = setup['layout']; dx = L['dx']; n = L['n_gap']; half = n // 2
    chi = chi_reader(sim); cls, table = classifier(setup['media'])
    errors, out = [], {'class_values': table}

    def want(cond, msg):
        if not cond:
            errors.append(msg)

    def code(comp, i, j, k):
        return cls(chi(comp, pos(comp, i, j, k, dx)))

    # 1. Lattice convention: the assumed sample offsets give pure values on lines
    #    crossing the gap at the board centre column, the half-shifted ones do not.
    ip = L['port_line_i']; kmid = dict(L['substrate_columns'])[ip]
    jrow = 0
    conv = {}
    for comp in YEE:
        pure = all(code(comp, ip, jrow, kmid + k) != '?' for k in range(-half - 3, half + 3))
        o = YEE[comp]
        shifted = [cls(chi(comp, ((ip + o[0]) * dx, (jrow + o[1]) * dx, (kmid + k + o[2] + 0.5) * dx)))
                   for k in range(-half - 3, half + 3)]
        conv[comp] = {'assumed_pure': pure, 'shifted_has_mixed': '?' in shifted}
        want(pure and '?' in shifted, f'{comp} z-offset convention')
    for comp, axis in (('Ex', 0), ('Ey', 1), ('Ez', 0)):
        o = list(YEE[comp])
        if axis == 0:
            i_edge = L['patch_columns'][-1][0]
            line = [(i, jrow) for i in range(i_edge - 3, i_edge + 4)]
        else:
            jb = L['patch_rows'][1]
            line = [(ip, j) for j in range(jb - 3, jb + 4)]
        kplane = dict(L['patch_columns'])[line[3][0] if axis == 0 else ip] + half
        kz = kplane if comp != 'Ez' else kplane - 1  # Ez: top gap sample (FR-4 / face boundary)
        vals = [cls(chi(comp, ((i + o[0]) * dx, (j + o[1]) * dx, (kz + o[2]) * dx))) for i, j in line]
        o2 = list(o); o2[axis] += 0.5
        sh = [cls(chi(comp, ((i + o2[0]) * dx, (j + o2[1]) * dx, (kz + o2[2]) * dx))) for i, j in line]
        conv[f'{comp}_axis{axis}'] = {'assumed': ''.join(vals), 'shifted': ''.join(sh)}
        want('?' not in vals, f'{comp} axis-{axis} offset convention (assumed offsets mixed)')
        if comp != 'Ez':
            want('?' in sh, f'{comp} axis-{axis} offset convention (shifted offsets pure)')
    out['lattice_convention'] = conv
    # 2. Per-column gap under the patch, on a row through the patch centre.
    pc = dict(L['patch_columns'])
    hist_h, hist_e, steps = {}, {}, 0
    for i, k in L['patch_columns']:
        col = ''.join(code('Ex', i, jrow, k + q) for q in range(-half - 2, half + 3))
        want(col == 'AAM' + 'F' * (n - 1) + 'MAA', f'Ex column {i}: {col}')
        hist_h[n] = hist_h.get(n, 0) + (col == 'AAM' + 'F' * (n - 1) + 'MAA')
    # Ez lines strictly inside the patch (both neighbouring columns are patch columns).
    edges = [i for i, _ in L['patch_columns'][1:]]
    ez_rows = {}
    for e_ in edges:
        ks = [pc[e_ - 1], pc[e_]]
        lo, hi = min(ks) - half - 2, max(ks) + half + 2
        col = ''.join(code('Ez', e_, jrow, q) for q in range(lo, hi))
        free = max((len(r) for r in col.replace('M', ' ').replace('A', ' ').replace('I', ' ').split()), default=0)
        step = len(set(ks)) > 1
        steps += step
        if not step:
            want(col == 'AA' + 'F' * n + 'AA', f'Ez line {e_}: {col}')
        else:
            want(col == 'AAM' + 'F' * (n - 1) + 'MAA', f'Ez step line {e_}: {col}')
        hist_e[free] = hist_e.get(free, 0) + 1
        ez_rows[e_] = col
    out['patch_columns'] = len(L['patch_columns'])
    out['ex_columns_with_n_gap'] = hist_h.get(n, 0)
    out['ez_free_run_histogram'] = {str(k): v for k, v in sorted(hist_e.items())}
    out['ez_step_lines'] = steps
    out['ez_lines'] = len(edges)
    # 3. Port: resistor Ez samples, neighbours, inner Ey links.
    j0, j1 = L['port_rows']; kg, kp = L['port_planes']
    blk = setup['resistor_block']
    rcode = 'R' if 'R' in table else 'F'

    def member(comp, i, j, k):
        return 'in' if mp.is_point_in_object(mp.Vector3(*pos(comp, i, j, k, dx)), blk) else 'out'
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
    inside = [mp.is_point_in_object(mp.Vector3(*q), blk) for q in setup['samples']]
    want(all(inside), 'source samples outside the resistor')
    out['port'] = port
    out['port_discrete_ohm'] = setup['discrete_port_ohm']
    # 4. Patch length on its plane (centre column) and the open face beyond it.
    kpl = pc[ip] + half; ja, jb = L['patch_rows']
    ex = ''.join(code('Ex', ip, j, kpl) for j in range(ja - 2, jb + 3))
    ey = ''.join(code('Ey', ip, j, kpl) for j in range(ja - 2, jb + 2))
    want(ex == 'II' + 'M' * (L['n_patch_l'] + 1) + 'II', f'patch Ex rows: {ex}')
    want(ey == 'II' + 'M' * L['n_patch_l'] + 'II', f'patch Ey rows: {ey}')
    out['patch_ex_metal_rows'] = ex.count('M'); out['patch_ey_metal_rows'] = ey.count('M')
    # 5. y = row slice of Ex and Ez over the half board for the record.
    for comp in ('Ex', 'Ez'):
        cols = [i for i, _ in L['substrate_columns']] + [L['substrate_columns'][-1][0] + 1]
        cols = [i for i in cols if i >= ip]
        kmin = min(k for _, k in L['substrate_columns']) - half - 2
        kmax = max(k for _, k in L['substrate_columns']) + half + 2
        out[f'slice_{comp}'] = {'i0': cols[0], 'k0': kmin, 'rows': [
            ''.join(code(comp, i, jrow, k) for i in cols) for k in range(kmin, kmax + 1)]}
    out['errors'] = errors
    return out


# ------------------------------------------------------------------ probe

def probe(resolution):
    """Small-cell checks of the conventions the harness relies on.

    a. Yee offsets read from chi1inv for even and odd cell counts on every axis.
    b. One time step with a constant current: where a point source and a v1-style
       sheet source deposit current (only the sourced samples change), and what a
       zero-size yee_grid DFT monitor reads at a sample and at its neighbour.
    """
    dx = 1 / resolution; out = {'resolution': resolution}
    for parity in (0, 1):
        n = 24 + parity
        cell = mp.Vector3(n * dx, n * dx, n * dx)
        diel = mp.Medium(epsilon=2.0)
        geo = [mp.Block(center=mp.Vector3(0.3 * dx, 0.3 * dx, 0.3 * dx),
                        size=mp.Vector3(6.2 * dx, 6.2 * dx, 6.2 * dx), material=diel)]
        sim = mp.Simulation(cell_size=cell, resolution=resolution, geometry=geo,
                            eps_averaging=False, force_complex_fields=True)
        sim.init_sim()
        chi = chi_reader(sim)
        found = {}
        for comp in YEE:
            per_axis = []
            for axis in range(3):
                pure = []
                for off in (0.0, 0.5):
                    vals = []
                    for m in range(-6, 7):
                        q = [0.0, 0.0, 0.0]
                        for a2 in range(3):
                            q[a2] = (YEE[comp][a2]) * dx
                        q[axis] = (m + off) * dx
                        v = chi(comp, q)
                        vals.append(abs(v - 1) < 1e-9 or abs(v - 0.5) < 1e-9)
                    if all(vals):
                        pure.append(off)
                per_axis.append(pure)
            found[comp] = per_axis
        out[f'yee_offsets_cells_n{n}'] = found
        sim.reset_meep()
    n = 24
    cell = mp.Vector3(n * dx, n * dx, n * dx)
    src = mp.CustomSource(src_func=lambda t: 1.0)
    p_pt = (0.0, -3 * dx, 0.5 * dx)           # Ez sample (0, -3, 0.5) dx
    sheet_c = (0.0, 4.5 * dx, 0.0)            # v1-style sheet: y and z edges at sample midpoints
    sources = [mp.Source(src, mp.Ez, center=mp.Vector3(*p_pt), amplitude=dx * dx),
               mp.Source(src, mp.Ez, center=mp.Vector3(*sheet_c), size=mp.Vector3(0, 2 * dx, 2 * dx))]
    sim = mp.Simulation(cell_size=cell, resolution=resolution, sources=sources,
                        eps_averaging=False, force_complex_fields=True)
    freqs = [2.45 / C_MM_NS]
    nb = (0.0, -3 * dx, 1.5 * dx)
    mons = point_monitors(sim, (p_pt, nb), freqs, dx)
    sim.init_sim()
    sim._evaluate_dft_objects()  # sim.run would do this; the probe steps fields directly
    sim.fields.step()
    dt = sim.fields.dt

    def ez(q):
        return complex(sim.get_field_point(mp.Ez, mp.Vector3(*q)))
    point = {f'{di},{dj},{dk}': ez((di * dx, (-3 + dj) * dx, (0.5 + dk) * dx)) for di in (-1, 0, 1)
             for dj in (-1, 0, 1) for dk in (-1, 0, 1)}
    sheet = {f'{dj},{dk}': ez((0.0, (4 + dj) * dx, (dk + 0.5) * dx)) for dj in range(-3, 4) for dk in range(-3, 3)}
    out['dt'] = dt
    out['point_deposition_ez'] = {k: [v.real, v.imag] for k, v in point.items()}
    out['sheet_deposition_ez'] = {k: [v.real, v.imag] for k, v in sheet.items()}
    out['point_dft_at_sample'] = [read_point(sim, mons[0], 0).real, read_point(sim, mons[0], 0).imag]
    out['point_dft_at_neighbour'] = [read_point(sim, mons[1], 0).real, read_point(sim, mons[1], 0).imag]
    return out


# ------------------------------------------------------------------ main

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--config', type=Path)
    ap.add_argument('--geometry', choices=['bent', 'flat'], default='bent')
    ap.add_argument('--resolution', type=float, default=2.5)
    ap.add_argument('--fmin', type=float, default=1.6)
    ap.add_argument('--fmax', type=float, default=3.0)
    ap.add_argument('--df', type=float, default=0.01)
    ap.add_argument('--loss-anchor-ghz', type=float, default=2.36)
    ap.add_argument('--loss-model', choices=['lorentz', 'conductivity'], default='lorentz',
                    help='lorentz: flat tan(delta) about the anchor; conductivity: v1-style 1/f, at the anchor')
    ap.add_argument('--stop-after-steps', type=int, default=0,
                    help='timing probe: stop after this many steps, no spectrum (0 = off)')
    ap.add_argument('--after-source', type=float, default=1200)
    ap.add_argument('--check-interval', type=float, default=600)
    ap.add_argument('--max-checks', type=int, default=5)
    ap.add_argument('--temporal-tolerance', type=float, default=.001)
    ap.add_argument('--max-estimated-gib', type=float, default=48)
    ap.add_argument('--rc-oracle', action='store_true')
    ap.add_argument('--rc-sheet', action='store_true',
                    help='RC oracle with sheet-equivalent point currents and the v1 box readout')
    ap.add_argument('--material-oracle', action='store_true')
    ap.add_argument('--probe', action='store_true')
    ap.add_argument('--audit-only', action='store_true')
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    if mp.__version__ != '1.34.0':
        raise RuntimeError('Source Fourier correction is validated only for Meep 1.34.0')
    t_import = time.perf_counter()
    started_utc = datetime.now(timezone.utc).isoformat()
    me = Path(__file__)
    result = {'schema': 'geode-matched-meep/2', 'harness_version': VERSION,
              'meep_version': mp.__version__, 'started_utc': started_utc,
              'source_sha256': hashlib.sha256(me.read_bytes()).hexdigest(),
              'mpi_ranks': mp.count_processors(), 'architecture': platform.machine(),
              'resolution_cells_per_mm': args.resolution}
    config = json.loads(args.config.read_text()) if args.config else None
    eps_r = config['materials']['substrate_eps_r'] if config else 4.4
    tan_d = config['materials']['substrate_tan_delta'] if config else 0.02
    loss = (design_fr4(eps_r, tan_d, args.loss_anchor_ghz) if args.loss_model == 'lorentz'
            else design_conductivity(eps_r, tan_d, args.loss_anchor_ghz))
    nf = int(round((args.fmax - args.fmin) / args.df)) + 1
    f_ghz = np.round(args.fmin + args.df * np.arange(nf), 6)
    frequencies = f_ghz / C_MM_NS
    med = fr4_medium(loss)
    meep_eps = []
    for g in (1.6, 2.36, 3.0):
        e = complex(np.asarray(med.epsilon(g / C_MM_NS))[2, 2])
        mine = fr4_epsilon(g / C_MM_NS, loss['eps_inf'], loss['sigma_d'], loss['sigma_l'],
                           loss['f_n'], loss['gamma'])
        meep_eps.append([g, e.real, e.imag, mine.real, mine.imag])
    result.update({'fr4_model': loss, 'fr4_realized': fr4_table(loss, f_ghz),
                   'fr4_meep_medium_epsilon_vs_formula': meep_eps,
                   'frequency_ghz': [float(f) for f in f_ghz]})
    if args.probe:
        result['mode'] = 'probe'
        result['probe'] = probe(args.resolution)
        write(args.output, result, t_import)
        return
    dx = 1 / args.resolution
    if args.rc_oracle or args.material_oracle:
        result['mode'] = 'material_oracle' if args.material_oracle else 'rc_oracle'
        setup = oracle_setup(args.resolution, args.material_oracle, loss, args.rc_sheet)
        result['rc_sheet'] = args.rc_sheet
    else:
        if config is None:
            ap.error('--config required for the antenna')
        result['mode'] = 'antenna'
        setup = antenna_setup(config, args.resolution, args.geometry, loss)
        result.update({'geometry': args.geometry, 'configuration': config, 'layout': setup['layout'],
                       'media': setup['media'], 'port_sigma_natural': setup['sigma'],
                       'discrete_port_ohm': setup['discrete_port_ohm'],
                       'geometry_objects': len(setup['geometry'])})
    cells = int(np.prod([math.ceil(s * args.resolution - 1e-9) for s in setup['cell']]))
    estimated_gib = cells * 400 / 2**30
    result.update({'cell_mm': list(setup['cell']), 'estimated_cells': cells,
                   'estimated_memory_gib': estimated_gib})
    if estimated_gib > args.max_estimated_gib:
        raise RuntimeError(f'Preflight: {estimated_gib:.1f} GiB > {args.max_estimated_gib}')
    src = mp.GaussianSource(frequency=HARNESS_FCEN_GHZ / C_MM_NS, fwidth=.012, cutoff=5)
    sim = mp.Simulation(cell_size=setup['cell'], resolution=args.resolution,
                        geometry=setup['geometry'], boundary_layers=setup['boundary'],
                        k_point=mp.Vector3() if setup['periodic'] else False,
                        default_material=setup['default_material'], eps_averaging=False,
                        force_complex_fields=True,
                        sources=point_sources(src, setup['samples'], setup['amplitude']))
    if 'box_monitor' in setup:
        monitors = [sim.add_dft_fields([mp.Ez], frequencies, center=mp.Vector3(),
                                       size=setup['box_monitor']['size'], yee_grid=False)]
    else:
        monitors = point_monitors(sim, setup['monitor_samples'], frequencies, dx)
    t0 = time.perf_counter()
    sim.init_sim()
    t_init = COMM.allreduce(time.perf_counter() - t0, op=MPI.MAX)
    result.update({'time_step': sim.fields.dt, 'init_seconds': t_init,
                   'n_source_samples': len(setup['samples']), 'monitor_nudge_cells': MONITOR_NUDGE,
                   'n_monitor_samples': len(monitors), 'source_amplitude': setup['amplitude']})
    if result['mode'] == 'antenna':
        audit = audit_grid(sim, setup)
        result['audit'] = audit
        if audit['errors'] or args.audit_only:
            result['aborted_before_stepping'] = bool(audit['errors'])
            write(args.output, result, t_import)
            if audit['errors']:
                raise SystemExit(f'grid audit failed: {audit["errors"][:5]}')
            return
    t1 = time.perf_counter()
    if args.stop_after_steps:
        sim.run(until=args.stop_after_steps * sim.fields.dt)
        t_step = COMM.allreduce(time.perf_counter() - t1, op=MPI.MAX)
        result.update({'mode': 'timing_probe', 'steps': args.stop_after_steps, 'stepping_seconds': t_step,
                       'seconds_per_step': t_step / args.stop_after_steps})
        write(args.output, result, t_import)
        return
    sim.run(until_after_sources=args.after_source)
    snapshots, previous, drift = [], None, float('inf')
    for check in range(args.max_checks + 1):
        if check:
            sim.run(until=args.check_interval)
        voltage, injected, impedance, reflection = spectrum(sim, monitors, src, setup, frequencies)
        snap = {'time': sim.meep_time(), 'impedance_ohm': [[float(z.real), float(z.imag)] for z in impedance]}
        quantity = impedance if reflection is None else reflection
        if reflection is not None:
            snap['s11'] = [[float(z.real), float(z.imag)] for z in reflection]
        if previous is not None:
            change = np.abs(quantity - previous)
            if reflection is None:  # oracle without a reference load: relative impedance change
                change = change / np.abs(quantity)
            drift = float(np.max(change))
            snap['max_change'] = drift
        snapshots.append(snap)
        if result['mode'] == 'antenna':  # partial record after every check (budget stops)
            partial = dict(result, snapshots=snapshots, complete=False,
                           max_temporal_change=drift if math.isfinite(drift) else None)
            write(args.output.with_suffix('.partial.json'), partial, t_import)
        if previous is not None and drift <= args.temporal_tolerance:
            break
        previous = quantity.copy()
    t_step = COMM.allreduce(time.perf_counter() - t1, op=MPI.MAX)
    result.update({
        'after_source': args.after_source, 'check_interval': args.check_interval,
        'max_checks': args.max_checks, 'temporal_tolerance': args.temporal_tolerance,
        'temporal_tolerance_met': drift <= args.temporal_tolerance,
        'max_temporal_change': drift, 'snapshots': snapshots,
        'steps': int(round(sim.meep_time() / sim.fields.dt)), 'stepping_seconds': t_step,
        'voltage_dft': [[float(z.real), float(z.imag)] for z in voltage],
        'injected_current_dft': [[float(z.real), float(z.imag)] for z in injected],
        'spatially_converged': False})
    if result['mode'] == 'rc_oracle':
        expected = 1j * ETA0 / (2 * np.pi * frequencies * setup['analytic_capacitance_natural'])
        result['max_rc_impedance_relative_error'] = float(np.max(np.abs(impedance / expected - 1)))
    if result['mode'] == 'material_oracle':
        w = 2 * np.pi * frequencies
        eps_zz = 1j * ETA0 / (w * setup['capacitance0_natural'] * impedance)
        sd, sr = loss['sigma_d'], setup['sigma_r']
        eps_fr4 = eps_zz * (1 + 1j * sd / w) / (1 + 1j * (sd + sr) / w)
        result['sigma_r'] = sr
        result['measured_fr4'] = [[float(g), float(e.real), float(e.imag / e.real)]
                                  for g, e in zip(f_ghz, eps_fr4)]
    write(args.output, result, t_import)


def write(path, result, t_import):
    wall = COMM.allreduce(time.perf_counter() - t_import, op=MPI.MAX)
    rss = COMM.allreduce(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024, op=MPI.SUM)
    result['wall_seconds'] = wall
    result['sum_rank_peak_rss_bytes'] = rss
    result['rss_note'] = 'Sum of per-rank high-water marks; conservative, not simultaneous.'
    if mp.am_master():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
        print('V2_RESULT', path, 'mode', result.get('mode'), 'wall', wall,
              'drift', result.get('max_temporal_change'), 'rc', result.get('max_rc_impedance_relative_error'))


if __name__ == '__main__':
    main()
