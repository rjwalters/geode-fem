"""Inspect the actual plotted series using synthetic data only."""
import csv
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import render
from test_prepare import fixture, preview_fixture
from prepare import prepare, METHODS


class RenderTests(unittest.TestCase):
    def test_both_rendered_repeats_keep_every_method_and_automatic_watermark(self):
        source=preview_fixture()
        unavailable=next(a for a in source['attempts'] if a['job']['start_index']==4)
        unavailable['validation_unavailable_events']=[{'kind':'validation_unavailable','error':'synthetic failure'}]
        data=prepare(source,2.)
        figures=[]
        original=render.plt.subplots
        def capture(*args,**kwargs):
            fig,axes=original(*args,**kwargs);figures.append((fig,axes));return fig,axes
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp)/'render'
            with mock.patch.object(render.plt,'subplots',side_effect=capture):
                render.render(data,out)  # No --synthetic: watermark must survive.
            self.assertEqual(len(figures),2)
            for repeat,(fig,axes) in enumerate(figures,1):
                self.assertIn('SYNTHETIC',fig._suptitle.get_text())
                for start in range(5):
                    for col,space in enumerate(('a','b')):
                        expected={r['run_id']:r for r in data['runs'] if
                            (r['start_index'],r['space'],r['repeat'])==(start,space,repeat) and r['points']}
                        lines={l.get_label():l for l in axes[start,col].lines if l.get_label() in expected}
                        self.assertEqual(set(lines),set(expected))
                        if start!=4:self.assertEqual({r['method'] for r in expected.values()},set(METHODS))
                        for rid,line in lines.items():
                            self.assertEqual(list(zip(line.get_xdata(),line.get_ydata())),[tuple(p) for p in expected[rid]['points']])
                if repeat==1:
                    self.assertIn('Validation unavailable: A','\n'.join(t.get_text() for t in axes[4,0].texts))
            saved=json.loads((out/'plot-data.json').read_text())
            self.assertTrue(saved['synthetic']);self.assertEqual(saved['runs'],data['runs'])
            with (out/'target-attainment.csv').open() as stream:cells=list(csv.DictReader(stream))
            self.assertEqual(len(cells),300)
            unavailable_cells=[r for r in cells if r['run_id']==unavailable['run_id']]
            self.assertEqual(len(unavailable_cells),3)
            self.assertTrue(all(r['validation_unavailable_count']=='1' and r['validation_state']=='unavailable' for r in unavailable_cells))

    def test_absent_validation_labels_distinguish_unavailable_rejected_and_unobserved(self):
        for state,label in [('unavailable','Validation unavailable'),('rejected','No feasible validation'),
                            ('unavailable_and_rejected','Unavailable / rejected'),('not_observed','No validation observed')]:
            self.assertEqual(render.missing_label({'validation_state':state}),label)

    def test_panel_refuses_pooled_repeats_and_missing_method(self):
        data=prepare(fixture(),2.)
        data['runs']=[r for r in data['runs'] if r['method']!='C']
        with self.assertRaisesRegex(ValueError,'all five methods'):render.panel_rows(data,0,'a',1)


if __name__=='__main__':unittest.main()
