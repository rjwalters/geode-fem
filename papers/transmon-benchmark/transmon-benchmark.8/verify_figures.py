#!/usr/bin/env python3
"""Verify numerical figure inputs and reproducible rendering (Python >=3.11).
No field solves or independent scientific approval are implied.
"""
from pathlib import Path
import hashlib
import json
import math
import subprocess
import sys
import tomllib

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
SRC = HERE / 'figures/src'
DATA = SRC / 'data'


def load(name):
    return json.loads((DATA / name).read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    manifest = load('provenance.json')
    for name, expected in manifest['files'].items():
        assert sha(DATA / name) == expected, name
    for name, expected in manifest['repository_inputs'].items():
        assert sha(REPO / name) == expected, name
    results = load('refined_results.json')
    summary = load('refined_validation.json')
    design = load('refined_design.json')
    rows = results['history']
    opt = [r for r in rows if r['kind'] == 'optimization']
    fine = [r for r in rows if r['kind'] == 'held_out_refinement'][0]
    assert len(opt) - 1 == summary['accepted_steps'] == 3
    assert opt[0]['c_ff'] == summary['initial_ff']
    assert opt[-1]['c_ff'] == summary['optimized_ff']
    assert fine['c_ff'] == summary['held_out_ff']
    assert abs(fine['c_ff'] / opt[-1]['c_ff'] - 1) < .01
    assert max(abs(b / a - 1) for a, b in zip(opt[-1]['gradient_ff'], fine['gradient_ff'])) < .01
    fd = [r['relative_error'] for r in rows if r['kind'] == 'final_gradient_check']
    assert len(fd) == 3 and max(fd) == summary['worst_solved_fd_relative_error']
    points = [p for tri in load('optimized_island.json')['triangles'] for p in tri]
    width = max(p[0] for p in points) - min(p[0] for p in points)
    length = max(p[1] for p in points) - min(p[1] for p in points)
    assert math.isclose(width, design['pad_width_um'], abs_tol=1e-9)
    assert math.isclose(length, design['island_length_um'], abs_tol=1e-9)
    eigen = tomllib.loads((REPO / 'benchmarks/transmon_eigen/results.toml').read_text())
    pal = eigen['oracles']['palace_target2']
    qubit_error = 100 * (pal['geode_qubit_f_ghz'] / pal['qubit_f_ghz'] - 1)
    assert f'{qubit_error:+.5f}%' == '+0.00005%'
    tex = (HERE / 'main.tex').read_text()
    for value in ['78.387918', '89.900518', '89.662755', '89.900294', '448{,}279', '528{,}324', '0.2645', '0.605', '0.000249']:
        assert value in tex, value
    assert r'3.87\times10^{-7}' in tex
    figures = sorted((HERE / 'figures').glob('fig*.pdf'))
    assert len(figures) == 7
    before = {p.name: sha(p) for p in figures}
    for script in sorted(SRC.glob('fig[1-7]_*.py')):
        subprocess.run([sys.executable, str(script)], check=True)
    assert before == {p.name: sha(p) for p in figures}, 'Rendered figure bytes changed'
    for pdf in figures:
        text = subprocess.check_output(['pdftotext', str(pdf), '-'], text=True)
        assert text.strip(), pdf
    agreement = subprocess.check_output(['pdftotext', str(HERE/'figures/fig2-agreement.pdf'), '-'], text=True)
    assert '+0.00005%' in agreement and 'junction_lc' not in agreement
    print('PASS: pinned input hashes, optimization/validation arithmetic, actual outline dimensions, qubit precision, manuscript values, and deterministic regeneration of all seven PDFs.')


if __name__ == '__main__':
    main()
