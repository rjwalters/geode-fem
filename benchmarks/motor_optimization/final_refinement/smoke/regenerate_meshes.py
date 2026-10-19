"""Regenerate omitted mesh bytes from pinned source; no FEM or optimizer runs.

Run with numpy 2.5.4 and gmsh 4.15.2. Output is external to Git and must be new.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

SOURCE = '63b3e3d8bbc30504f845eebb5b78e8be6a2ee12f'
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]


def regenerate(out):
    out=Path(out).resolve();out.mkdir(parents=True,exist_ok=False)
    with tempfile.TemporaryDirectory() as tmp:
        root=Path(tmp)
        raw=subprocess.check_output(['git','-C',str(REPO),'archive',SOURCE,
            'benchmarks/motor_reference','benchmarks/motor_shape_gradient/design'])
        with tarfile.open(fileobj=io.BytesIO(raw)) as archive:archive.extractall(root,filter='data')
        reference=root/'benchmarks/motor_reference'
        sources=json.loads((reference/'sources.json').read_text())
        for item in sources['files']:
            if item['path'] not in ('results/plots/final/RotorOpt0.txt','results/plots/final/StatorOpt0.txt'):continue
            local=REPO/'benchmarks/motor_reference/_upstream'/item['path']
            prefix='https://raw.githubusercontent.com/MichaelWiesheu/ICEMsnapshot/'+sources['commit']+'/'
            if item['url']!=prefix+item['path']:raise ValueError('unpinned upstream URL')
            raw=local.read_bytes() if local.is_file() else urllib.request.urlopen(item['url'],timeout=90).read()
            if len(raw)!=item['bytes'] or hashlib.sha256(raw).hexdigest()!=item['sha256']:raise ValueError('upstream bytes changed')
            dest=reference/'_upstream'/item['path'];dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(raw)
        initial=root/'initial'
        subprocess.run([sys.executable,'-I',str(reference/'geometry/build_sector_mesh.py'),
            '--levels','2','3','--out',str(initial)],check=True)
        records=[]
        def retain(path,expected):
            raw=path.read_bytes();sha=hashlib.sha256(raw).hexdigest()
            if sha!=expected['sha256'] or len(raw)!=expected['bytes']:
                raise ValueError('regenerated mesh differs from recorded bytes: '+str(path))
            shutil.copyfile(path,out/sha);records.append(dict(expected,file=sha))
        retain(initial/'icem_initial_sector_L2.msh',json.loads((HERE/'native_binding_L2.receipt.json').read_text()))
        for archive_name,prefix in [('native_matrix_63b3e3d8.tar.gz','b-lhs1-primary'),
                                    ('native_refinement_63b3e3d8.tar.gz','a-conforming')]:
            with tarfile.open(HERE/archive_name) as archive:
                omissions=json.loads(archive.extractfile('omitted-payloads.json').read())['files']
                plan=root/(prefix+'.json');plan.write_bytes(archive.extractfile(prefix+'/meshes/plan_remesh000.json').read())
                dest=root/prefix
                subprocess.run([sys.executable,'-I',str(root/'benchmarks/motor_shape_gradient/design/build_design_mesh.py'),
                    '--plan',str(plan),'--levels','2','--out',str(dest)],check=True)
                retain(dest/'icem_remesh_remesh000_L2.msh',omissions[prefix+'/meshes/icem_remesh_remesh000_L2.msh'])
                if 'l3-mesh/icem_initial_sector_L3.msh' in omissions:
                    retain(initial/'icem_initial_sector_L3.msh',omissions['l3-mesh/icem_initial_sector_L3.msh'])
        (out/'regeneration.json').write_text(json.dumps({'source_commit':SOURCE,'files':records,
            'scope':'Hash-matched regenerated meshes only. No numerical outputs regenerated.'},indent=2)+'\n')
        print(json.dumps({'mesh_files':len(records),'directory':str(out),'source_commit':SOURCE}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',required=True,type=Path)
    regenerate(parser.parse_args().out)
