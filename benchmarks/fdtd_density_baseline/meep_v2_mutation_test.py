#!/usr/bin/env python3
"""Mutation tests for meep_v2_verify.py (#1146). Each case must make the verifier fail.

Copies the inputs the verifier reads into a temporary directory, corrupts one of
them, and (for "forged" cases) regenerates ladder.json and SHA256SUMS, as a
forger would. Exit status 0 means every mutation was caught. Standard library only.

    python3 benchmarks/fdtd_density_baseline/meep_v2_mutation_test.py
"""
import hashlib, json, re, shutil, subprocess, sys, tempfile
from pathlib import Path
SRC = Path(__file__).resolve().parents[2]
WORK = Path(tempfile.mkdtemp(prefix='meep-v2-mutation-')) / 'repo'
PATHS = ['benchmarks/fdtd_density_baseline', 'benchmarks/patch_antenna_conformal/convergence.toml',
         'benchmarks/patch_antenna_conformal/v6/v6_summary.toml',
         'benchmarks/patch_antenna_conformal/v6/x1/dip_x1comb_L1.toml', 'reference/meep/docker']
V2 = 'benchmarks/fdtd_density_baseline/meep_runs/v2'
VER = 'benchmarks/fdtd_density_baseline/meep_v2_verify.py'


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
    d = WORK / V2
    lines = []
    for f in sorted(p for p in d.rglob('*') if p.is_file() and p.name != 'SHA256SUMS'):
        lines.append(f'{hashlib.sha256(f.read_bytes()).hexdigest()}  {f.relative_to(d).as_posix()}')
    (d / 'SHA256SUMS').write_text('\n'.join(lines) + '\n')


def hole(d):
    rows = d['audit']['slice_Ex']['rows']
    i = max(k for k, r in enumerate(rows) if 'M' in r)  # top patch row in the slice
    j = rows[i].index('M') + 2
    rows[i] = rows[i][:j] + 'F' + rows[i][j + 1:]


def thick_gap(d):
    rows = d['audit']['slice_Ez']['rows']
    for i, r in enumerate(rows):  # one FR-4 Ez sample more above the gap on line 2 of the slice
        if r[2] == 'F' and rows[i + 1][2] == 'A':
            rows[i + 1] = rows[i + 1][:2] + 'F' + rows[i + 1][3:]
            return


def toml_sub(rel, pat, rep):
    p = WORK / rel; t = p.read_text(); t2 = re.sub(pat, rep, t, count=1, flags=re.S); assert t2 != t; p.write_text(t2)


CONV = 'benchmarks/patch_antenna_conformal/convergence.toml'
V6 = 'benchmarks/patch_antenna_conformal/v6/v6_summary.toml'


def drop_source():
    src = sorted((WORK / V2 / 'sources').glob('matched_antenna_v2.*.py'))
    for p in src:
        p.unlink()


CASES = {
 'c_bent_R5 spectrum shifted 30 MHz (forged)': (lambda: rotate(f'{V2}/c_bent_R5.json', 3), True),
 'c_bent_R2.5 spectrum shifted 30 MHz (forged)': (lambda: rotate(f'{V2}/c_bent_R2.5.json', 3), True),
 'c_flat_R2.5 spectrum shifted 30 MHz (forged)': (lambda: rotate(f'{V2}/c_flat_R2.5.json', 3), True),
 'hole in the patch sheet of c_bent_R5 (forged)': (lambda: jedit(f'{V2}/c_bent_R5.json', hole), True),
 'thicker gap on one Ez line of c_flat_R2.5 (forged)': (lambda: jedit(f'{V2}/c_flat_R2.5.json', thick_gap), True),
 'port set to 66.7 ohm (forged)': (lambda: jedit(f'{V2}/c_bent_R5.json', lambda d: d.__setitem__('discrete_port_ohm', 66.7)), True),
 'grid audit error hidden in a record (forged)': (lambda: jedit(f'{V2}/c_bent_R2.5.json', lambda d: d['audit']['errors'].append('Ex column 3')), True),
 'stopping flag flipped (forged)': (lambda: jedit(f'{V2}/c_flat_R2.5.json', lambda d: d.__setitem__('temporal_tolerance_met', not d['temporal_tolerance_met'])), True),
 'material oracle tan d +1 % (forged)': (lambda: jedit(f'{V2}/material_R5.json', lambda d: [r.__setitem__(2, r[2] * 1.01) for r in d['measured_fr4']]), True),
 'RC point oracle error 1e-2 (forged)': (lambda: jedit(f'{V2}/rc_point_R5.json', lambda d: d.__setitem__('max_rc_impedance_relative_error', 1e-2)), True),
 'probe: point current spilled (forged)': (lambda: jedit(f'{V2}/probe_R5.json', lambda d: d['probe']['point_deposition_ez'].__setitem__('0,0,1', [1e-6, 0.0])), True),
 'v1 R1 replay perturbed by 1e-9 (forged)': (lambda: jedit(f'{V2}/v1_initial_R1.json', lambda d: d['snapshots'][-1]['s11'][0].__setitem__(0, d['snapshots'][-1]['s11'][0][0] + 1e-9)), True),
 'v2 harness version not archived (forged)': (drop_source, True),
 'GEODE bent lower bound set to 2.300 (forged)': (lambda: toml_sub(CONV, r'(\[gate_g1\.bent\].*?lower_bound_dip_f_ghz = )2\.356643', r'\g<1>2.300000'), True),
 'Palace p3 dip set to 2.300 (forged)': (lambda: toml_sub(V6, r'(\[x1\.palace\.init_L1_p3\].*?dip_f_ghz = )2\.36597', r'\g<1>2.30000'), True),
 'v1 harness edited': (lambda: (WORK / 'reference/meep/docker/matched_antenna.py').open('a').write('# edit\n'), False),
 'ladder.json value edited (sums regenerated)': (lambda: (jedit(f'{V2}/ladder.json', lambda d: d['runs']['c_bent_R5'].__setitem__('dip_parabolic_ghz', 2.40)), resum()), False),
 'cost raised to $41, arithmetic consistent (sums regenerated)': (lambda: (jedit(f'{V2}/environment.json', lambda d: d.update(
     compute_cost_upper_elapsed_estimate_usd=41.0,
     launch_to_terminate_seconds=41.0 / d['cost_estimate_rate_usd_per_hour'] * 3600)), resum()), False),
 'worker left running (sums regenerated)': (lambda: (jedit(f'{V2}/environment.json', lambda d: d.__setitem__('worker_final_state', 'running')), resum()), False),
}

fresh()
base = subprocess.run([sys.executable, str(WORK / VER)], capture_output=True, text=True)
print('unmutated:', 'PASS' if base.returncode == 0 else 'FAIL ' + base.stderr[-300:])
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
