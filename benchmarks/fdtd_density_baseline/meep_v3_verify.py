#!/usr/bin/env python3
"""Validate the #1427 harness-v3 outline loader (``matched_antenna_v3.py``).

Standard library only; every check is an explicit ``raise`` so it stays active
under ``python -O``. No Meep run is involved: the loader/voxelizer is pure
Python, and the parity check compares it with the *committed* v2 records.

* v1 and v2 are untouched: SHA-256 of ``matched_antenna.py``,
  ``diag_antenna.py`` and ``matched_antenna_v2.py`` equal the pinned values.
* The optimized-geometry dump is the pinned file (SHA-256).
* Initial-geometry parity: the loader applied to the dump's ``base`` outline with
  v2's port rule reproduces, key by key, the ``layout`` of the committed v2
  records ``c_bent_R2.5``, ``c_bent_R5`` and ``c_flat_R2.5``: same columns, plane
  indices, metal rows, port, substrate, so Meep receives the same Yee-grid
  conductors and the same grid audit as in those runs. Every patch column has
  the same rows [ja, jb].
* Optimized outline (``moved``, port from the dump): a golden digest of the
  layout at R2.5 and R5; an independent point-in-polygon rasterization of every
  patch column (rows ja + 1 .. jb - 1 inside the outline, rows ja - 1 and jb + 1
  outside; the columns next to the first and last are not wholly inside); the
  port rows lie strictly inside the port column's metal rows; the voxelized
  copper area is within 3 % of the outline area (R5).
* Loader edge cases raise: a U-shaped outline (non-contiguous metal), an outline
  that leaves the substrate, a feed outside the outline, an odd gap-cell
  count, a config that disagrees with the dump board.
* A perturbed outline vertex or a moved feed changes the layout.
"""
import copy
import hashlib
import importlib.util
import json
import math
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
DOCKER = ROOT / 'reference/meep/docker'
V2REC = HERE / 'meep_runs/v2'
DUMP = ROOT / 'benchmarks/patch_antenna_conformal/geometry/rbD_L1_copper_38_sm45.json'
PINS = {
    'matched_antenna.py': '66d3285f058b42a62bd26671bb9c2955fd66572de99f4c0a1d96f35f7d9d6d05',
    'diag_antenna.py': '6c77c54503ed6346c9a97e5f94735d8568d6c7afa51777f23ec1de7118bcf306',
    'matched_antenna_v2.py': 'af90734973f04325fa3e21119eaa259c44de9fdb2b350653c0add26190aeddbc',
}
DUMP_SHA256 = '1454c7c750dbc78ff6626b35754f4183d08bb163d45a0d259e19a21e7d38156c'
GOLDEN = {2.5: 'b504b739a3928cd23be26a31c2bb4b0e62d077561b1e3860cdfbfe808e0b9707',
          5: '751d8537e7426cd2480dec8912f3e6825b59566a0a345bdaef8e621b7cb6920b'}
PARITY = {'c_bent_R2.5': ('bent', 2.5), 'c_bent_R5': ('bent', 5), 'c_flat_R2.5': ('flat', 2.5)}


def check(cond, msg):
    if not cond:
        raise SystemExit(f'FAIL: {msg}')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_v3():
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('matched_antenna_v3', DOCKER / 'matched_antenna_v3.py')
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def digest(layout):
    return hashlib.sha256(json.dumps(layout, sort_keys=True).encode()).hexdigest()


def inside(poly, s, y):
    """Even-odd ray casting, written independently of the loader's scanline rule."""
    hit = False
    for (s0, y0), (s1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and s < s0 + (y - y0) / (y1 - y0) * (s1 - s0):
            hit = not hit
    return hit


def area(poly):
    return abs(sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(poly, poly[1:] + poly[:1]))) / 2


def raises(fn, what):
    try:
        fn()
    except (ValueError, AssertionError):
        return
    raise SystemExit(f'FAIL: no error for {what}')


def main():
    for name, want in PINS.items():
        check(sha(DOCKER / name) == want, f'{name} changed (v1/v2 must stay byte-identical)')
    check(sha(DUMP) == DUMP_SHA256, 'optimized-geometry dump differs from the pinned file')
    v3 = load_v3()
    cfg = json.loads((DOCKER / 'matched_initial_2g4.json').read_text())
    base, moved = v3.load_outline(DUMP, 'base'), v3.load_outline(DUMP, 'moved')
    check(base['dump_sha256'] == DUMP_SHA256, 'loader hash')

    # 1. Initial-geometry parity with the committed v2 records.
    for name, (geo, res) in PARITY.items():
        rec = json.loads((V2REC / f'{name}.json').read_text())['layout']
        got = json.loads(json.dumps(v3.layout3(cfg, res, geo, base, 'config')))
        for key, value in rec.items():
            check(got.get(key) == value, f'parity {name}: layout[{key!r}] differs from the v2 record')
        rows = {tuple(r[1:]) for r in got['patch_column_rows']}
        check(rows == {tuple(rec['patch_rows'])}, f'parity {name}: patch columns do not all span {rec["patch_rows"]}')
        check(len(got['patch_column_rows']) == len(rec['patch_columns']), f'parity {name}: column count')

    # 2. Optimized outline.
    for res, golden in GOLDEN.items():
        L = v3.layout3(cfg, res, 'bent', moved, 'dump')
        check(digest(L) == golden, f'R{res}: optimized-outline layout differs from the golden digest')
        dx, yc = L['dx'], L['board_centre_mm'][1]
        xc = L['board_centre_mm'][0]; R = L['bend_radius_mm']
        poly = moved['polygon']
        rows = {i: (a, b) for i, a, b in L['patch_column_rows']}
        idx = sorted(rows)
        check(idx == list(range(idx[0], idx[-1] + 1)), f'R{res}: patch columns not contiguous')
        check(len(rows) == len(L['patch_columns']), f'R{res}: row table and column list differ')
        chart = lambda i: R * math.asin(((i + 0.5) * dx - xc) / R)  # noqa: E731
        for i in idx:
            ja, jb = rows[i]; c = chart(i)
            check(all(inside(poly, c, j * dx - yc) for j in range(ja + 1, jb)), f'R{res}: column {i} metal outside the outline')
            edge = (c - dx / 2 + 1e-7, c, c + dx / 2 - 1e-7)  # tightest of the column's scanlines binds
            check(not all(inside(poly, s, (ja - 1) * dx - yc) for s in edge)
                  and not all(inside(poly, s, (jb + 1) * dx - yc) for s in edge),
                  f'R{res}: column {i} stops short of the outline')
        for i in (idx[0] - 1, idx[-1] + 1):  # neighbours must not be wholly inside
            c = chart(i)
            whole_inside = all(inside(poly, c + o, y) for o in (-dx / 2 + 1e-7, 0, dx / 2 - 1e-7)
                               for y in (0.0,))
            check(not whole_inside, f'R{res}: column {i} next to the patch is wholly inside the outline')
        ip = L['port_line_i']; j0, j1 = L['port_rows']
        check(rows[ip][0] < j0 and j1 < rows[ip][1], f'R{res}: port outside the metal rows of its column')
        check(L['n_port_rows'] == j1 - j0 + 1, f'R{res}: port row count')
        check(abs(L['port_centre_mm'][1] - (moved['feed_y'] + yc)) <= 0.5 * dx + 1e-9, f'R{res}: feed y snapped too far')
        if res == 5:
            vox = sum((b - a) for _, a, b in L['patch_column_rows']) * dx * dx
            check(abs(vox / area(poly) - 1) < 0.03, f'R5: voxelized area {vox:.1f} vs outline {area(poly):.1f} mm^2')

    # 3. Edge cases.
    sq = lambda w, h: [(-w, -h), (w, -h), (w, h), (-w, h)]  # noqa: E731
    def with_poly(poly, **kw):
        o = copy.deepcopy(base); o['polygon'] = poly; o.update(kw); return o
    u_shape = [(-19, -14), (19, -14), (19, -2), (5, -2), (5, 2), (19, 2), (19, 14), (-19, 14)]  # slot cut in from +s
    raises(lambda: v3.layout3(cfg, 2.5, 'bent', with_poly(u_shape), 'config'), 'non-contiguous (U-shaped) outline')
    raises(lambda: v3.layout3(cfg, 2.5, 'bent', with_poly(sq(40, 14)), 'config'), 'outline beyond the substrate')
    raises(lambda: v3.layout3(cfg, 2.5, 'bent', with_poly(sq(19, 14), feed_y=20.0), 'dump'), 'feed outside the outline')
    odd = copy.deepcopy(cfg); odd['patch']['h'] = 1.5
    raises(lambda: v3.layout3(odd, 2, 'bent', with_poly(base['polygon'], nominal=dict(base['nominal'], h=1.5)), 'dump'),
           'odd gap-cell count (1.5 mm at R2 = 3 cells)')
    bad = copy.deepcopy(cfg); bad['patch']['patch_w'] = 37.0
    raises(lambda: v3.layout3(bad, 2.5, 'bent', base, 'config'), 'config board differing from the dump')
    off = [(s + 0.013, y - 0.007) for s, y in base['polygon']]  # off-lattice vertices still snap, like v2
    L = v3.layout3(cfg, 2.5, 'bent', with_poly(off), 'config')
    check(L['patch_rows'] in ([-36, 36], [-37, 35], [-36, 35], [-37, 36]), 'off-lattice outline snapped wrongly')

    # 4. The loader reacts to outline and feed changes.
    ref = digest(v3.layout3(cfg, 2.5, 'bent', moved, 'dump'))
    pert = copy.deepcopy(moved)
    k = max(range(len(pert['polygon'])), key=lambda n: pert['polygon'][n][0])
    pert['polygon'][k] = (pert['polygon'][k][0] + 1.0, pert['polygon'][k][1])
    try:
        changed = digest(v3.layout3(cfg, 2.5, 'bent', pert, 'dump')) != ref
    except ValueError:
        changed = True  # the loader refused the degenerate outline
    check(changed, 'a perturbed outline vertex went unnoticed')
    fed = copy.deepcopy(moved); fed['feed_y'] -= 2.0
    check(digest(v3.layout3(cfg, 2.5, 'bent', fed, 'dump')) != ref, 'a moved feed went unnoticed')
    print('meep_v3_verify: OK (parity with v2 R2.5/R5/flat layouts; optimized outline R2.5, R5)')


if __name__ == '__main__':
    main()
