#!/usr/bin/env python3
"""Verify the v11 figures (unchanged from v10): inputs, the numbers they show, layout, and regeneration.

Python >= 3.11 with NumPy and Matplotlib; Poppler's pdftotext and pdfinfo.
Run from any working directory. This checks author-side transcription and
rendering, not the physics; no field solve is run.

1. Every repository input in figures/src/data/provenance.json has its recorded
   SHA-256, every artifact path a script names is in the manifest, and the data
   directory holds no snapshot.
2. The four figure scripts run; each one's save() enforces the layout rules of
   figure_style.py (width, fonts >= 8 pt at print size, no text collisions).
   The regenerated PDFs must be byte-identical to the committed ones, every
   figure PDF and script must be byte-identical to v10's, and Figure 1 must be
   byte-identical to v9's.
3. Figure-specific content, from the PDFs' extracted text and the committed
   artifacts: the cross-check estimates and their labels (Figure 2), the
   certified scalar and tensor brackets and the gap, with no design target
   drawn (Figure 3), the re-optimization's labels and the PRIMA end point
   (Figure 4). Estimates are labelled as not bounds wherever they appear.
4. main.tex includes each figure exactly once, in printed order, and the
   captions carry the wording the figures depend on.
"""
from pathlib import Path
import hashlib
import json
import math
import re
import subprocess
import sys
import tomllib

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
FIG = HERE / 'figures'
SRC = FIG / 'src'
DATA = SRC / 'data'
NAMES = ['fig1-geometry', 'fig2-crosscheck', 'fig3-convergence', 'fig4-reoptimization']
LABELS = ['geometry', 'crosscheck', 'convergence', 'reopt']
MAX_WIDTH_PT = 6.3 * 72


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def toml(rel):
    return tomllib.loads((REPO / rel).read_text())


def text(name):
    raw = subprocess.check_output(['pdftotext', '-raw', str(FIG / f'{name}.pdf'), '-'], text=True)
    return re.sub(r'\s+', ' ', raw)


def check_inputs():
    manifest = json.loads((DATA / 'provenance.json').read_text())
    assert manifest['files'] == {}, 'v11 has no snapshot inputs'
    for name, expected in manifest['repository_inputs'].items():
        assert sha(REPO / name) == expected, f'input changed since the figures were drawn: {name}'
    assert sorted(p.name for p in DATA.iterdir()) == ['provenance.json']
    sources = '\n'.join(p.read_text() for p in SRC.glob('*.py'))
    named = set(re.findall(r"'((?:benchmarks|crates)/[^']+\.(?:toml|json|jsonl|msh))'", sources))
    named |= {a + b for a, b in re.findall(r"'((?:benchmarks|crates)/[^']+/)'\s*\n?\s*'([^']+\.(?:toml|json|jsonl))'", sources)}
    for rel in named:
        assert rel in manifest['repository_inputs'], rel
    assert 'benchmarks/transmon_devicelayout_optimize/opt_log.jsonl' in manifest['repository_inputs']


def check_regeneration():
    pdfs = sorted(FIG.glob('*.pdf'))
    assert [p.stem for p in pdfs] == sorted(NAMES), [p.stem for p in pdfs]
    scripts = sorted(SRC.glob('fig[1-9]_*.py'))
    assert [s.name.split('_')[0] for s in scripts] == ['fig1', 'fig2', 'fig3', 'fig4']
    before = {p.name: sha(p) for p in pdfs}
    for script in scripts:
        subprocess.run([sys.executable, str(script)], check=True, stdout=subprocess.DEVNULL)
    after = {p.name: sha(p) for p in sorted(FIG.glob('*.pdf'))}
    assert before == after, 'regenerated figure bytes differ from the committed PDFs'
    v10 = HERE.parent / 'transmon-benchmark.10' / 'figures'
    mine = sorted(p.relative_to(FIG) for p in FIG.rglob('*') if p.is_file() and '__pycache__' not in p.parts)
    theirs = sorted(p.relative_to(v10) for p in v10.rglob('*') if p.is_file() and '__pycache__' not in p.parts)
    assert mine == theirs, 'v11 figure files differ from v10'
    for rel in mine:
        assert sha(FIG / rel) == sha(v10 / rel), f'v11 figure file differs from v10: {rel}'
    v9 = HERE.parent / 'transmon-benchmark.9' / 'figures'
    assert sha(FIG / 'fig1-geometry.pdf') == sha(v9 / 'fig1-geometry.pdf')
    for name in ['fig1_geometry.py', 'geometry_mesh.py', 'figure_style.py']:
        assert (SRC / name).read_bytes() == (v9 / 'src' / name).read_bytes(), name
    for name in NAMES:
        info = subprocess.check_output(['pdfinfo', str(FIG / f'{name}.pdf')], text=True)
        w = float(re.search(r'Page size:\s+([\d.]+) x', info).group(1))
        assert w <= MAX_WIDTH_PT + 0.01, (name, w)


def check_content():
    every = {name: text(name) for name in NAMES}
    for name, t in every.items():
        for stale in ['anchor', '93.95', '93.14', '89.9', 'spurious', 'Same-mesh', 'within 1%', 'passed']:
            assert stale not in t, (name, stale)
    cc = toml('benchmarks/transmon_devicelayout_crosscheck/results.toml')
    t2 = every['fig2-crosscheck']
    for geom in ['initial', 'final']:
        conv = cc['convergence'][geom]
        for mode in ['q', 'r']:
            label = f"{conv[f'f_{mode}_ghz']:.3f} GHz ({conv[f'f_{mode}_over_published_pct']:+.1f}% vs published)"
            assert label in t2, label
    for label in ['published (DeviceLayout)', 'Palace, order 1', 'Palace, order 2', 'geode-fem, order 1 (same meshes)',
                  "Palace at DeviceLayout's discretization", 'converged estimate band (not a bound)',
                  'static route (certified', 'circuit cap from the certified bound']:
        assert label in t2, label
    t3 = every['fig3-convergence']
    sb = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')['bracket_x0']
    tens = toml('benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml')
    tb, gap = tens['bracket_x0_tensor'], tens['gap_x0']
    for b in [sb, tb]:
        assert f"{b['lower_ff']:.2f}–{b['upper_ff']:.2f} fF" in t3
    rel = gap['tensor_minus_scalar_relative']
    # Certified, so rounded outward: against the floored/ceiled fF gap and the scalar bracket too.
    lo = min(rel[0], gap['tensor_minus_scalar_lower_ff'] / gap['scalar_upper_ff'])
    hi = max(rel[1], gap['tensor_minus_scalar_upper_ff'] / gap['scalar_lower_ff'])
    shown = (math.floor(1000 * lo) / 10, math.ceil(1000 * hi) / 10)
    assert shown == (2.5, 3.1), shown
    assert f"gap {shown[0]:.1f}–{shown[1]:.1f}%" in t3
    assert 'gap 2.5–3.1%' in t3 and 'P2, sapphire tensor' in t3 and 'certified two-sided brackets' in t3
    assert 'Every marker is an upper bound' in t3
    t4 = every['fig4-reoptimization']
    for label in ['published final (PRIMA)', "Newton, DeviceLayout's discretization", 'Newton, converged frequencies',
                  'qubit band mapped to', '(estimate)', 'estimate band (not a bound)', 'PRIMA, published end point',
                  "DeviceLayout's criterion", 'target (3.0, 4.0 GHz)', '(620, 5000)', 'first leg, from the initial design',
                  'step-2 qubit band mapped to']:
        assert label in t4, label
    # The converged-target steps carry Table 5's names, in (a) and as the (c, d) tick labels.
    assert t4.count('Step 1') == 3 and t4.count('Step 2') == 3, (t4.count('Step 1'), t4.count('Step 2'))
    for stale in ['cv1', 'cv2']:
        assert stale not in t4, stale


def check_manuscript():
    tex = (HERE / 'main.tex').read_text()
    norm = re.sub(r'\s+', ' ', tex)
    order = [m.group(1) for m in re.finditer(r'\\anvilfig\{(fig\d-[a-z-]+)\.pdf\}', tex)]
    assert order == NAMES, order
    labels = re.findall(r'\\label\{fig:([a-z-]+)\}', tex)
    assert labels == LABELS, labels
    for wording in ['its extrapolation band, which is not a bound', 'certified capacitance bracket read through the',
                    'Every marker is a conforming upper bound on the converged value of the committed polygonal geometry',
                    'certified two-sided brackets from adapted meshes', 'magnified in the inset',
                    'mapped linearly to capacitor\\_length (an estimate)', 'since its trajectory is not published',
                    'Converged estimates and extrapolation bands']:
        assert wording in norm, wording


def main():
    check_inputs()
    check_regeneration()
    check_content()
    check_manuscript()
    print('PASS: input hashes, byte-identical regeneration of four figures (all as in v10, Figure 1 as in v9), widths, '
          'figure content against the artifacts, inclusion order and caption wording.')


if __name__ == '__main__':
    main()
