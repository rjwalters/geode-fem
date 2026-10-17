"""Read-only artifact check: python3 verify.py (no FEM solve or dependencies)."""
from pathlib import Path
import hashlib
import json
import math
import re
import statistics as st

V = Path(__file__).resolve().parent
D = V / 'figures/src/data'
manifest = json.loads((V / 'evidence-manifest.json').read_text())
for item in manifest['files']:
    assert hashlib.sha256((V / item['snapshot']).read_bytes()).hexdigest() == item['sha256'], item
ref = json.loads((D / 'reference_data.json').read_text())['designs']['initial']['torque_Nm_for_modeled_sector_by_current_then_angle']
summary = json.loads((D / 'summary.json').read_text())
numbers = json.loads((V / 'verified-numbers.json').read_text())
total = 0
for level in range(4):
    run = json.loads((D / f'L{level}_table_default_mesh.json').read_text())
    assert len(run['rows']) == 60
    total += len(run['rows'])
    for q in range(3):
        rows = sorted((r for r in run['rows'] if r['current_index'] == q), key=lambda r:r['angle_deg'])
        assert [r['angle_deg'] for r in rows] == list(range(20))
        assert all(r['status'] == 'tolerance' for r in rows)
        values = [r['T_arkkio'] for r in rows]
        s = summary['comparison_20_angles'][f'I{q}_L{level}']
        for a,b in [(st.mean(values),s['mean']), (st.pstdev(values),s['std']), (st.mean(ref[q]),s['ref_mean']), (st.pstdev(ref[q]),s['ref_std'])]:
            assert math.isclose(a,b,rel_tol=1e-12,abs_tol=1e-14)
        if level == 3:
            assert math.isclose(100*(st.pstdev(values)/st.pstdev(ref[q])-1),numbers['ripple_relative_error_percent'][q],abs_tol=1e-10)
            if q:
                assert math.isclose(100*(st.mean(values)/st.mean(ref[q])-1),numbers['mean_relative_error_percent'][q-1],abs_tol=1e-10)
            assert f'{st.mean(values):.5f}' in (V/'tables/torque.tex').read_text()
            assert f'{st.pstdev(values):.5f}' in (V/'tables/torque.tex').read_text()
assert total == numbers['baseline_solves'] == 240
runs = [json.loads(p.read_text()) for p in D.glob('L*.json')]
assert sum(len(r['rows']) for r in runs) == 500
assert all(row['status']=='tolerance' for r in runs for row in r['rows'])
tex = (V/'main.tex').read_text(); bib=(V/'refs.bib').read_text()
keys = set(re.findall(r'@\w+\{([^,]+),',bib))
for group in re.findall(r'\\cite(?:p|t|yearpar)?\{([^}]+)\}',tex):
    assert set(group.split(',')) <= keys, group
for fig in re.findall(r'\\includegraphics(?:\[[^]]*\])?\{([^}]+)\}',tex):
    assert (V/fig).is_file(), fig
assert (V/'main.pdf').is_file() and (V/'main.bbl').is_file()
print('PASS: evidence hashes, 500 solve statuses, baseline statistics, headline values, table values, citations and figure assets.')
print('This verifies recorded artifacts, not a fresh FEM run or manuscript scientific review.')
