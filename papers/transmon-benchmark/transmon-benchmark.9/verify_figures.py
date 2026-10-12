#!/usr/bin/env python3
"""Verify the v9 figures: inputs, the numbers they show, layout, and regeneration.

Python >= 3.11 with NumPy and Matplotlib; Poppler's pdftotext and pdfinfo.
Run from any working directory. This checks author-side transcription and
rendering, not the physics; no field solve is run.

1. Every repository input in figures/src/data/provenance.json has its recorded
   SHA-256, and the data directory holds no snapshot (v9 removed v8's one).
2. The figure scripts all run; each one's save() enforces the layout rules of
   figure_style.py (width, fonts >= 8 pt at print size, no text collisions).
   The regenerated PDFs must be byte-identical to the committed ones.
3. Figure-specific content, from the PDFs' extracted text and from the
   committed artifacts: the qubit mode and its precision (Figure 2), which
   L-doubling ratios are committed (Figure 3), the current anchor and the
   certified X0 bracket (Figure 5), the 93.95 fF runs with their certified
   brackets, estimates labelled as estimates and the by-construction label
   (Figure 6), the anchor chain against the brackets (Figure 7), the timing
   wording (Figure 8) and the historical framing with the superseded anchor
   (Figure 9). Withdrawn labels and verdicts are absent everywhere.
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
NAMES = ['fig1-geometry', 'fig2-agreement', 'fig3-lscaling', 'fig4-participation', 'fig5-convergence',
         'fig6-refined-optimization', 'fig7-anchor-chain', 'fig8-cpu-wallclock', 'fig9-legacy-diffopt']
LABELS = ['geometry', 'agreement', 'lscaling', 'participation', 'convergence', 'refined', 'anchor-chain',
          'cpu', 'legacy']
MAX_WIDTH_PT = 6.3 * 72
BASE = 'benchmarks/transmon_diffopt/refined_optimization/'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def toml(rel):
    return tomllib.loads((REPO / rel).read_text())


def text(name):
    raw = subprocess.check_output(['pdftotext', '-raw', str(FIG / f'{name}.pdf'), '-'], text=True)
    return re.sub(r'\s+', ' ', raw)


def ceil_to(x, digits):
    scale = 10 ** digits
    return math.ceil(x * scale - 1e-9) / scale


def floor_to(x, digits):
    scale = 10 ** digits
    return math.floor(x * scale + 1e-9) / scale


def check_inputs():
    manifest = json.loads((DATA / 'provenance.json').read_text())
    assert manifest['files'] == {}, 'v9 has no snapshot inputs'
    for name, expected in manifest['repository_inputs'].items():
        assert sha(REPO / name) == expected, f'input changed since the figures were drawn: {name}'
    assert sorted(p.name for p in DATA.iterdir()) == ['provenance.json']
    # Every artifact a script reads is in the manifest.
    sources = '\n'.join(p.read_text() for p in SRC.glob('*.py'))
    for rel in re.findall(r"'((?:benchmarks|crates)/[^']+\.(?:toml|json|msh))'", sources):
        assert rel in manifest['repository_inputs'], rel
    for sub in ['p2_cycles10_anchor_93p95', 'p2_cycles10_anchor_93p95_resolution_corrected']:
        assert BASE + sub + '/design.json' in manifest['repository_inputs'], sub
        assert BASE + sub + '/summary.toml' in manifest['repository_inputs'], sub


def check_regeneration():
    pdfs = sorted(FIG.glob('*.pdf'))
    assert [p.stem for p in pdfs] == sorted(NAMES), [p.stem for p in pdfs]
    scripts = sorted(SRC.glob('fig[1-9]_*.py'))
    assert len(scripts) == 9
    assert [s.name.split('_')[0] for s in scripts] == [f'fig{k}' for k in range(1, 10)]
    before = {p.name: sha(p) for p in pdfs}
    for script in scripts:
        subprocess.run([sys.executable, str(script)], check=True, stdout=subprocess.DEVNULL)
    after = {p.name: sha(p) for p in sorted(FIG.glob('*.pdf'))}
    assert before == after, 'regenerated figure bytes differ from the committed PDFs'
    for name in NAMES:
        info = subprocess.check_output(['pdfinfo', str(FIG / f'{name}.pdf')], text=True)
        w = float(re.search(r'Page size:\s+([\d.]+) x', info).group(1))
        assert w <= MAX_WIDTH_PT + 0.01, (name, w)
        assert text(name).strip(), name


def check_content():
    eigen = toml('benchmarks/transmon_eigen/results.toml')
    pal = eigen['oracles']['palace_target2']
    everything = {name: text(name) for name in NAMES}
    for name, t in everything.items():
        for stale in ['Same-mesh', 'junction_lc', 'physical modes', 'passes', 'passed', 'within 1%', '±1%',
                      '1% working', 'working tolerance', 'Finer mesh', 'spurious']:
            assert stale not in t, (name, stale)

    # Figure 2: the qubit mode is present, at its committed precision.
    t2 = everything['fig2-agreement']
    qubit = 100 * (pal['geode_qubit_f_ghz'] / pal['qubit_f_ghz'] - 1)
    assert f'{qubit:+.5f}%' == '+0.00005%' and '+0.00005%' in t2 and 'Qubit mode' in t2
    assert 'Port-sheet artifact' in t2 and 'Same input mesh' in t2
    for key, mode in eigen['modes'].items():
        if mode['kind'] == 'qubit':
            continue
        signed = 100 * (mode['f_ghz'] / mode['palace_f_ghz'] - 1)
        assert f'{abs(signed):.3f}' == f"{mode['rel_err_pct']:.3f}", key
        assert f'{signed:+.3f}%' in t2, key

    # Figure 3 (v8 review nit): Palace's ratio is computed, geode-fem's are committed.
    t3 = everything['fig3-lscaling']
    ratio = pal['l2_qubit_f_ghz'] / pal['qubit_f_ghz']
    assert f'{ratio:.5f} (Palace, from its committed frequencies)' in t3
    q, sh = eigen['tripwires']['qubit_l_doubling'], eigen['tripwires']['port_sheet_artifact_l_doubling']
    assert f"{q['ratio']} (geode-fem, committed)" in t3 and f"{sh['ratio']} (geode-fem, committed)" in t3
    assert 'Committed ratios' not in t3

    # Figure 4: separate observables, both labels.
    t4 = everything['fig4-participation']
    assert '0.99424' in t4 and 'Port-sheet artifact' in t4 and 'Palace' in t4

    # Figure 5: uniform bounds and band, the certified X0 bracket, the current anchor.
    t5 = everything['fig5-convergence']
    conv = toml('benchmarks/transmon_diffopt/c_sigma_convergence.toml')
    lo, hi = conv['x0_extrapolation']['estimate_band_ff']
    br = toml('benchmarks/transmon_diffopt/c_sigma_bracket.toml')['bracket_x0']
    anchor = toml(BASE + 'p2_cycles10_anchor_93p95/summary.toml')['anchor']['c_sigma_target_ff']
    for value in [f'{lo:.2f}–{hi:.2f} fF (not a bound)', '< 83.02 fF (rigorous)', '< 89.50 fF (rigorous)',
                  f"certified {br['lower_ff']:.2f}–{br['upper_ff']:.2f} fF", f'anchor {anchor:.2f} fF',
                  'Every marker is an upper bound', 'certified two-sided bracket']:
        assert value in t5, value
    assert '89.9 fF' not in t5 and 'anchor 89.9' not in t5

    # Figure 6: the 93.95 fF runs, both bounds, brackets, estimates, the caveat.
    t6 = everything['fig6-refined-optimization']
    prim = toml(BASE + 'p2_cycles10_anchor_93p95/summary.toml')
    corr = toml(BASE + 'p2_cycles10_anchor_93p95_resolution_corrected/summary.toml')
    brk = toml('benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml')
    for run, key in [(prim, 'bracket_anchor_93p95'), (corr, 'bracket_anchor_93p95_resolution_corrected')]:
        rounds = sorted(run['held_out'], key=lambda r: r['round'])
        assert [r['round'] for r in rounds] == list(range(8))
        c = [r['c_ff'] for r in rounds]
        assert all(b < a for a, b in zip(c, c[1:]))
        assert run['optimization']['accepted_steps'] == 4 and len(run['final_design_extrapolation']['slow_rate_fit']) == 2
        b = brk[key]
        assert b['lower_ff'] < c[-1] and ceil_to(c[-1], 2) == b['upper_ff']
        assert f"[{b['lower_ff']:.2f}, {b['upper_ff']:.2f}]" in t6
    for phrase in ['certified lower bound', 'certified bracket', 'estimate (not a bound)', 'slower rates',
                   'upper bound', 'rounds 0–7', f"anchor {anchor:.2f}",
                   f"corr. target {corr['meta']['optimizer_target_ff']:.3f}",
                   f"{prim['optimization']['initial_ff']:.3f}",
                   'contains the anchor by construction', 'independent certificate',
                   f"pad width 24.00 → {prim['design']['pad_width_um']:.2f} / {corr['design']['pad_width_um']:.2f} µm",
                   f"length 625.50 → {prim['design']['island_length_um']:.2f} / {corr['design']['island_length_um']:.2f} µm",
                   'committed extents']:
        assert phrase in t6, phrase
    for stale in ['Fixed-design', 'Palace P2', '89.6628', '< 89.02', '< 90.08', 'anchor 89.9', 'Primary',
                  'No lower bound']:
        assert stale not in t6, stale
    # The boxes are the committed design.json extents.
    for sub, run in [('p2_cycles10_anchor_93p95', prim), ('p2_cycles10_anchor_93p95_resolution_corrected', corr)]:
        d = json.loads((REPO / BASE / sub / 'design.json').read_text())
        (x0, y0, _), (x1, y1, _) = d['island_bbox_um']
        assert math.isclose(y1 - y0, run['design']['island_length_um'], abs_tol=1e-5)
        assert math.isclose(x1 - x0, run['design']['pad_width_um'], abs_tol=1e-5)

    # Figure 7: the anchor chain, estimates labelled, brackets certified.
    t7 = everything['fig7-anchor-chain']
    cpl = toml('benchmarks/transmon_coupling/results.toml')['anchor']
    loop = toml('benchmarks/transmon_loop_inductance/results.toml')['anchor']
    for phrase in [f"{cpl['dressed_c_sigma_ff']:.3f}", f"{cpl['bare_c_sigma_ff']:.3f}", f"{loop['c_sigma_ff']:.3f}",
                   f"−{cpl['shift_ff']:.3f} fF (hybridization)",
                   f"−{loop['shift_from_bare_ff']:.3f} fF (loop inductance)",
                   'target band (estimate)', 'same-mesh k or Ls', 'certified bracket',
                   'certified [92.75, 93.00] fF', 'certified [93.89, 94.14] fF', 'Dressed anchor', 'Bare qubit',
                   'Bare + loop']:
        assert phrase in t7, phrase
    assert cpl['dressed_c_sigma_ff'] - cpl['bare_c_sigma_ff'] == cpl['shift_ff'] or \
        abs(cpl['dressed_c_sigma_ff'] - cpl['bare_c_sigma_ff'] - cpl['shift_ff']) < 1e-5
    # Neither bracket contains the loop-corrected central target; the corrected one contains the dressed.
    d, c = brk['bracket_anchor_93p95'], brk['bracket_anchor_93p95_resolution_corrected']
    assert not (d['lower_ff'] <= loop['c_sigma_ff'] <= d['upper_ff'])
    assert not (c['lower_ff'] <= loop['c_sigma_ff'] <= c['upper_ff'])
    assert c['lower_ff'] <= cpl['dressed_c_sigma_ff'] <= c['upper_ff']

    # Figure 8 and 9: timing wording, historical framing.
    t8 = everything['fig8-cpu-wallclock']
    assert 'Six reference modes' in t8 and '†' in t8
    t9 = everything['fig9-legacy-diffopt']
    for phrase in ['Historical', 'anchor shortfall was a mesh artifact', 'a coarse-mesh artifact',
                   'first tet inversion', 'superseded 89.9 fF']:
        assert phrase in t9, phrase
    assert 'Limited by mesh validity' not in t9


def check_manuscript():
    tex = (HERE / 'main.tex').read_text()
    norm = re.sub(r'\s+', ' ', tex)
    used = re.findall(r'\\anvilfig(?:\[[^]]*\])?\{([^}]+)\.pdf\}', tex)
    assert used == NAMES, used
    assert '\\anvilfig[' not in tex, 'every figure uses the 0.98 default width'
    assert '\\newcommand{\\anvilfig}[3][0.98]' in tex
    for label in LABELS:
        assert tex.count(f'\\label{{fig:{label}}}') == 1 and f'\\ref{{fig:{label}}}' in tex, label
    assert '\\ref{fig:diffopt}' not in tex and 'github.com' not in tex
    # Caption wording each figure depends on (v8 review nit: Figure 8's "six reference modes").
    caps = {m.group(2): m.group(1) for m in re.finditer(r'\\caption\{(.*?)\}\s*\\label\{fig:([^}]+)\}', norm)}
    assert 'six reference modes' in caps['cpu'] and 'physical modes' not in caps['cpu']
    assert 'contains the anchor by construction' in caps['refined'] and 'independent certificate' in caps['refined']
    assert 'extrapolated estimates, not bounds' in caps['refined']
    assert 'The optimized outlines are not committed' in caps['refined']
    assert 'are estimates from the extrapolated coupling and loop inductance, not bounds' in caps['anchor-chain']
    assert 'transfer assumption' in caps['anchor-chain'] and 'certified brackets' in caps['anchor-chain']
    assert 'certified two-sided bracket' in caps['convergence'] and 'superseded' not in caps['convergence']
    assert 'superseded anchor value' in caps['legacy']
    assert 'Palace\'s $0.70810$ is computed from its two committed frequencies' in caps['lscaling']


def main():
    check_inputs()
    check_regeneration()
    check_content()
    check_manuscript()
    print('PASS: input hashes (no snapshot), layout rules and byte-identical regeneration of all nine PDFs, '
          'figure content against the committed artifacts (qubit mode, ratios, certified X0 bracket, '
          '93.95 fF series with certified brackets and estimates, by-construction label, anchor chain), '
          'figure order and caption wording.')


if __name__ == '__main__':
    main()
