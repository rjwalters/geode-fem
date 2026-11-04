#!/usr/bin/env python3
"""Mutation tests for meep_diag_verify.py (#1088). Each case must make the verifier fail.

Copies the inputs the verifier reads into a temporary directory, corrupts one of
them, and (for "forged" cases) regenerates diagnosis.json and SHA256SUMS, as a
forger would. Exit status 0 means every mutation was caught. Standard library only.

    python3 benchmarks/fdtd_density_baseline/meep_diag_mutation_test.py
"""
import hashlib, json, re, shutil, subprocess, sys, tempfile
from pathlib import Path
SRC = Path(__file__).resolve().parents[2]
WORK = Path(tempfile.mkdtemp(prefix='meep-diag-mutation-')) / 'repo'
PATHS = ['benchmarks/fdtd_density_baseline', 'benchmarks/patch_antenna_conformal/convergence.toml',
         'benchmarks/patch_antenna_conformal/v6/v6_summary.toml', 'reference/meep/docker']
DIAG = 'benchmarks/fdtd_density_baseline/meep_runs/diag'
VER = 'benchmarks/fdtd_density_baseline/meep_diag_verify.py'

def fresh():
    if WORK.exists(): shutil.rmtree(WORK)
    for p in PATHS:
        s, d = SRC / p, WORK / p
        d.parent.mkdir(parents=True, exist_ok=True)
        (shutil.copytree if s.is_dir() else shutil.copy2)(s, d)

def jedit(rel, fn):
    p = WORK / rel; d = json.loads(p.read_text()); fn(d); p.write_text(json.dumps(d, indent=2) + '\n')

def rotate(rel, k):
    def f(d):
        for snap in d['snapshots']:
            for key in ('s11', 'impedance_ohm'):
                snap[key] = snap[key][k:] + snap[key][:k]
        for key in ('voltage_dft', 'injected_current_dft'):
            d[key] = d[key][k:] + d[key][:k]
    jedit(rel, f)

def forge():
    r = subprocess.run([sys.executable, str(WORK / VER), '--write'], capture_output=True, text=True)
    if r.returncode: return 'rejected at --write: ' + r.stderr.strip().splitlines()[-1]
    resum()

def resum():
    d = WORK / DIAG
    lines = []
    for f in sorted(p for p in d.rglob('*') if p.is_file() and p.name != 'SHA256SUMS'):
        lines.append(f'{hashlib.sha256(f.read_bytes()).hexdigest()}  {f.relative_to(d).as_posix()}')
    (d / 'SHA256SUMS').write_text('\n'.join(lines) + '\n')

def hole(d):
    rows = d['grid']['slice_y0_Ex']['rows']
    i = max(k for k, r in enumerate(rows) if 'M' in r)  # top patch row
    j = rows[i].index('M') + 3
    rows[i] = rows[i][:j] + 'F' + rows[i][j + 1:]

def flat_into_bent(d):
    flat = json.loads((WORK / DIAG / 'grid_flat_R2.json').read_text())
    for k in ('slice_y0_Ex', 'slice_y0_Ez'):
        d['grid'][k] = flat['grid'][k]

def toml_sub(rel, pat, rep):
    p = WORK / rel; t = p.read_text(); t2 = re.sub(pat, rep, t, count=1, flags=re.S); assert t2 != t; p.write_text(t2)

CONV = 'benchmarks/patch_antenna_conformal/convergence.toml'
CASES = {
 'bent_R2 spectrum shifted 30 MHz (forged)': (lambda: rotate(f'{DIAG}/bent_R2.json', 3), True),
 'cells_bent_R2 spectrum shifted 30 MHz (forged)': (lambda: rotate(f'{DIAG}/cells_bent_R2.json', 3), True),
 'flat_R2 spectrum shifted 30 MHz (forged)': (lambda: rotate(f'{DIAG}/flat_R2.json', 3), True),
 'bent grid replaced by flat grid (forged)': (lambda: jedit(f'{DIAG}/grid_bent_R2.json', flat_into_bent), True),
 'hole in the patch shell (forged)': (lambda: jedit(f'{DIAG}/grid_bent_R2.json', hole), True),
 'GEODE flat L3 dip set to 2.400 (forged)': (lambda: toml_sub(CONV, r'(level = "L3"\nelement_order = "p1"\nn_dofs = 844238\ndip_f_ghz = )2\.343357', r'\g<1>2.400000'), True),
 'GEODE detune set to -5 MHz (forged)': (lambda: toml_sub(CONV, r'L1_p1 = \{ dip_shift_mhz = -0\.971', 'L1_p1 = { dip_shift_mhz = -5.000'), True),
 'harness edited': (lambda: (WORK / 'reference/meep/docker/matched_antenna.py').open('a').write('# edit\n'), False),
 'diagnosis.json value edited (sums regenerated)': (lambda: (jedit(f'{DIAG}/diagnosis.json', lambda d: d['runs']['bent_R2'].__setitem__('dip_parabolic_ghz', 2.3)), resum()), False),
 'cost raised to $20.9 (sums regenerated)': (lambda: (jedit(f'{DIAG}/environment.json', lambda d: d.__setitem__('compute_cost_upper_elapsed_estimate_usd', 20.9)), resum()), False),
 'r1eq_narrow perturbed by 1e-9 (forged)': (lambda: jedit(f'{DIAG}/r1eq_narrow.json', lambda d: d['snapshots'][-1]['s11'][0].__setitem__(0, d['snapshots'][-1]['s11'][0][0] + 1e-9)), True),
}
fresh()
base = subprocess.run([sys.executable, str(WORK / VER)], capture_output=True, text=True)
print('unmutated:', 'PASS' if base.returncode == 0 else 'FAIL ' + base.stderr[-200:])
bad = base.returncode != 0
for name, (mutate, forged) in CASES.items():
    fresh(); mutate()
    note = forge() if forged else None
    r = subprocess.run([sys.executable, str(WORK / VER)], capture_output=True, text=True)
    caught = note is not None or r.returncode != 0
    msg = note or (r.stderr.strip().splitlines()[-1] if r.stderr.strip() else 'passed')
    print(f"{'CAUGHT' if caught else 'MISSED'}  {name}: {msg}")
    bad |= not caught
shutil.rmtree(WORK.parent)
sys.exit(1 if bad else 0)
