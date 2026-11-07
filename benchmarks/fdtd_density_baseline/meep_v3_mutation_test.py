#!/usr/bin/env python3
"""Mutation tests for meep_v3_verify.py (#1427). Each case must make the verifier fail.

Copies the files the verifier reads into a temporary directory, corrupts one of
them (the committed v1/v2 harness, the dump, a v2 record the parity check
compares with, or the v3 loader itself: rounding, feed position, chart frame,
scanline rule, contiguity and substrate guards), and runs the verifier there.
Exit status 0 means every mutation was caught. Standard library only.

    python3 benchmarks/fdtd_density_baseline/meep_v3_mutation_test.py
"""
import json, shutil, subprocess, sys, tempfile
from pathlib import Path
SRC = Path(__file__).resolve().parents[2]
WORK = Path(tempfile.mkdtemp(prefix='meep-v3-mutation-')) / 'repo'
PATHS = ['benchmarks/fdtd_density_baseline', 'benchmarks/patch_antenna_conformal/geometry',
         'reference/meep/docker']
V3 = 'reference/meep/docker/matched_antenna_v3.py'
VER = 'benchmarks/fdtd_density_baseline/meep_v3_verify.py'
GEOM = 'benchmarks/patch_antenna_conformal/geometry/rbD_L1_copper_38_sm45.json'
REC = 'benchmarks/fdtd_density_baseline/meep_runs/v2/c_bent_R2.5.json'


def tracked():
    out = subprocess.run(['git', '-C', str(SRC), 'ls-files', '-z', '--', *PATHS],
                         capture_output=True, check=True).stdout.decode().split('\0')
    files = [f for f in out if f]
    assert files, 'git ls-files returned nothing'
    return files


TRACKED = tracked()


def fresh():
    if WORK.exists(): shutil.rmtree(WORK)
    for f in TRACKED:
        d = WORK / f
        d.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(SRC / f, d)


def jedit(rel, fn):
    p = WORK / rel; d = json.loads(p.read_text()); fn(d); p.write_text(json.dumps(d, indent=2) + '\n')


def sub(rel, old, new):
    p = WORK / rel; t = p.read_text(); assert t.count(old) == 1, (rel, old); p.write_text(t.replace(old, new))


def move_vertex(d):
    n = d['patch_outline_loop'][5]
    d['moved'][n][0] += 0.5


CASES = {
 'loader: metal rows floored instead of rounded': lambda: sub(V3, 'lo, hi = int(round((iv[0] + yc) / dx)), int(round((iv[1] + yc) / dx))',
                                                             'lo, hi = int(math.floor((iv[0] + yc) / dx)), int(math.floor((iv[1] + yc) / dx))'),
 'loader: feed y negated': lambda: sub(V3, "target = outline['feed_y'] + yc", "target = -outline['feed_y'] + yc"),
 'loader: feed s and y swapped': lambda: sub(V3, 'feed_s=float(feed[0]), feed_y=float(feed[1])', 'feed_s=float(feed[1]), feed_y=float(feed[0])'),
 'loader: bend axis z offset ignored': lambda: sub(V3, 'nodes[n][2] - z_axis', 'nodes[n][2]'),
 'loader: moved outline read from base': lambda: sub(V3, 'nodes, loop = d[which], d', 'nodes, loop = d["base"], d'),
 'loader: column test on the centre scanline only': lambda: sub(V3, 'for s in (c - dx / 2 + SCAN_INSET_MM, c, c + dx / 2 - SCAN_INSET_MM):', 'for s in (c,):'),
 'loader: contiguity check removed': lambda: sub(V3, 'if len(ys) != 2:', 'if False:'),
 'loader: substrate guard removed': lambda: sub(V3, "raise ValueError('the outline extends beyond the substrate board')", 'pass'),
 'loader: port-inside-outline guard removed': lambda: sub(V3, 'if not (ra < j0 and j0 + n_w - 1 < rb):', 'if False:'),
 'loader: odd-gap guard removed': lambda: sub(V3, 'if n_gap % 2:', 'if False:'),
 'loader: dump/config board mismatch ignored': lambda: sub(V3, "if abs(nom[key] - p[key]) > 1e-9:", 'if False:'),
 'loader: board centre offset dropped from the rows': lambda: sub(V3, '(iv[0] + yc) / dx', 'iv[0] / dx'),
 'dump: one outline vertex moved 0.5 mm': lambda: jedit(GEOM, move_vertex),
 'dump: moved feed y shifted': lambda: jedit(GEOM, lambda d: d['feed_moved_s_y_mm'].__setitem__(1, d['feed_moved_s_y_mm'][1] + 1.0)),
 'v2 harness edited': lambda: (WORK / 'reference/meep/docker/matched_antenna_v2.py').open('a').write('# edit\n'),
 'v1 harness edited': lambda: (WORK / 'reference/meep/docker/matched_antenna.py').open('a').write('# edit\n'),
 'v2 record layout edited (parity reference)': lambda: jedit(REC, lambda d: d['layout'].__setitem__('patch_rows', [-35, 36])),
 'v2 record port moved (parity reference)': lambda: jedit(REC, lambda d: d['layout'].__setitem__('port_rows', [18, 21])),
}

fresh()
base = subprocess.run([sys.executable, '-I', str(WORK / VER)], capture_output=True, text=True)
print('unmutated:', 'PASS' if base.returncode == 0 else 'FAIL ' + (base.stderr or base.stdout)[-300:])
bad = base.returncode != 0
for name, mutate in CASES.items():
    fresh(); mutate()
    r = subprocess.run([sys.executable, '-I', str(WORK / VER)], capture_output=True, text=True)
    caught = r.returncode != 0
    msg = (r.stderr.strip() or r.stdout.strip()).splitlines()[-1] if (r.stderr.strip() or r.stdout.strip()) else 'passed'
    print(f"{'CAUGHT' if caught else 'MISSED'}  {name}: {msg}")
    bad |= not caught
shutil.rmtree(WORK.parent)
sys.exit(1 if bad else 0)
