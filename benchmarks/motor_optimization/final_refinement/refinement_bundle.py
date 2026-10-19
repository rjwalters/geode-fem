"""Prepare portable exact requests after verified closed-study selection.

No executable is built or launched. A separately reviewed executor build/runtime
and witnessed post-run execution remain necessary before interpreting results.
"""
import argparse
import copy
import hashlib
import io
import json
from pathlib import Path
import re
import sys
import tarfile

if __name__=='__main__':sys.path.insert(0,str(Path(__file__).resolve().parent))
import capture_archive
from closed_capture import census, directory, _copy_file, _write
from design_evidence import RegisteredDesigns
import native_results
import producer_source
from selection import Design, _exact
import study_selection
from trace_archive import _json

ARCHIVE='benchmarks/motor_optimization/final_refinement/smoke/native_matrix_63b3e3d8.tar.gz'
MESH='l3-mesh/icem_initial_sector_L3.msh'
REPORT='l3-mesh/icem_initial_sector_L3_report.json'
GEOMETRY_SOURCES=('benchmarks/motor_reference/geometry/build_sector_mesh.py',
                  'benchmarks/motor_reference/geometry/geopdes_nurbs.py',
                  'benchmarks/motor_reference/sources.json',
                  'benchmarks/motor_reference/reference_data.json')


def _digest(raw):return hashlib.sha256(raw).hexdigest()


def _l3(repo, commit, *, producer_commit, reference, l3_mesh):
    if type(commit) is not str or re.fullmatch('[0-9a-f]{40}',commit) is None:
        raise ValueError('independently reviewed full mesh-evidence commit required')
    hashes={}
    for path in GEOMETRY_SOURCES:
        raw=producer_source.git(repo,'show',commit+':'+path)
        if raw!=producer_source.git(repo,'show',producer_commit+':'+path):
            raise ValueError('L3 construction sources differ from the study')
        hashes[path]=_digest(raw)
    raw=producer_source.git(repo,'show',commit+':'+ARCHIVE)
    with tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') as archive:
        contents={}
        for name in (REPORT,):
            members=[m for m in archive.getmembers() if m.name==name]
            if len(members)!=1 or not members[0].isfile():raise ValueError('missing or redirected reviewed L3 evidence')
            contents[name]=archive.extractfile(members[0]).read()
    mesh_path=Path(l3_mesh)
    if mesh_path.is_symlink() or not mesh_path.is_file():raise ValueError('missing or redirected external L3 mesh')
    contents[MESH]=mesh_path.read_bytes()
    report=_json(contents[REPORT]);provenance=report['provenance']
    sources=_json(producer_source.git(repo,'show',commit+':benchmarks/motor_reference/sources.json'))
    exports={f['path']:f['sha256'] for f in sources['files']}
    wanted={p:exports[p] for p in ('results/plots/final/RotorOpt0.txt','results/plots/final/StatorOpt0.txt')}
    if (report.get('level')!=3 or report.get('msh_file')!=Path(MESH).name
            or report.get('msh_sha256')!=_digest(contents[MESH]) or report.get('msh_bytes')!=len(contents[MESH])
            or report.get('gmsh_version')!='4.15.2' or report.get('coupling_segments_N')!=960
            or provenance.get('generator')!=GEOMETRY_SOURCES[0]
            or provenance.get('upstream_commit')!=reference['source_commit']
            or provenance.get('exports_sha256')!=wanted):
        raise ValueError('reviewed L3 mesh report disagrees with initial geometry/provenance')
    evidence={'commit':commit,'archive':ARCHIVE,'archive_sha256':_digest(raw),
              'construction_source_sha256':hashes,'mesh_sha256':_digest(contents[MESH]),
              'report_sha256':_digest(contents[REPORT]),
              'scope':'External mesh bytes checked against independently reviewed L3 construction report and unchanged study geometry sources.'}
    return contents[MESH],contents[REPORT],evidence


def _designs(result):
    designs=copy.deepcopy(result['designs'])
    for value in designs.values():
        for key in ('design','theta0'):value[key]=Design(value[key])
    return designs


def _schedule(result,definitions,meshes):
    prepared=native_results.requests(result['selection'],_designs(result),definitions,meshes,include_conforming=True)
    if _exact(prepared['plan'])!=_exact(result['plan']):raise ValueError('request plan differs from frozen selection')
    jobs=[];geometry=[]
    for key,value in sorted(result['designs'].items()):
        geometry.append({'design_key':key,'space':value['settings']['design_space'],
            'request_file':'geometry/'+key+'.json',
            'request':{'id':1,'op':'geometry','x_bits':value['x_bits']},
            'cost_scope':'post_run','requires_fem':False})
    for job in result['plan']['jobs']:
        key=job['design_key'];check=job['spec']['name'];request=prepared['requests'][job['job_id']]
        jobs.append({'job_id':job['job_id'],'design_key':key,'check':check,'space':request['space'],
            'request_file':'requests/'+key+'/'+check+'.json','request':request,
            'mesh_file':'inputs/L'+str(job['spec']['level'])+'.msh',
            'geometry_request_file':'geometry/'+key+'.json',
            'output_directory':'results/'+key+'/'+check})
    return jobs,geometry


def prepare(capture_root, *, manifest_sha256, git_repo, reviewed_commit, capture_tool_commit,
            mesh_evidence_commit, l3_mesh, out):
    root=directory(capture_root);repo=Path(git_repo).resolve(strict=True)
    out=Path(out).absolute();directory(out.parent)
    if out.exists() or out.is_symlink() or any(out==p or p in out.parents for p in (root,repo)):
        raise ValueError('bundle must be a new directory outside capture and repository')
    result=study_selection.load(root,manifest_sha256=manifest_sha256,git_repo=repo,
                reviewed_commit=reviewed_commit,capture_tool_commit=capture_tool_commit)
    reader=capture_archive.Reader(root,manifest_sha256)
    inputs=reader.manifest['inputs']
    path=lambda name:reader.path(inputs[name]['file'])
    definitions=RegisteredDesigns.load(path('reference_data'),path('starts'),
        reference_sha256=inputs['reference_data']['sha256'],starts_sha256=inputs['starts']['sha256'])
    l3,report,evidence=_l3(repo,mesh_evidence_commit,producer_commit=reviewed_commit,
                         reference=_json(path('reference_data').read_bytes()),l3_mesh=l3_mesh)
    out.mkdir();(out/'inputs').mkdir()
    try:
        for source,dest in (('reference_data','reference_data.json'),('starts','starts.json'),('validation_mesh','L2.msh')):
            _copy_file(path(source),out/'inputs'/dest,inputs[source]['sha256'])
        (out/'inputs/L3.msh').write_bytes(l3);(out/'inputs/L3-report.json').write_bytes(report)
        meshes={2:{'path':out/'inputs/L2.msh','sha256':inputs['validation_mesh']['sha256']},
                3:{'path':out/'inputs/L3.msh','sha256':evidence['mesh_sha256']}}
        jobs,geometry=_schedule(result,definitions,meshes)
        _write(out/'selection.json',result)
        for row in jobs+geometry:
            target=out/row['request_file'];target.parent.mkdir(parents=True,exist_ok=True)
            _write(target,row['request'])
        reader.stable()
        metadata={'schema':1,'kind':'final_refinement_input_bundle','capture_manifest_sha256':manifest_sha256,
            'reviewed_producer_commit':reviewed_commit,'capture_tool_commit':capture_tool_commit,
            'mesh_evidence':evidence,'reference_sha256':definitions.reference_sha256,
            'starts_sha256':definitions.starts_sha256,'L2_sha256':meshes[2]['sha256'],
            'jobs':[{k:v for k,v in row.items() if k!='request'} for row in jobs],
            'geometry':[{k:v for k,v in row.items() if k!='request'} for row in geometry],
            'inventory':census(out),'can_launch':False,
            'scope':'Exact inputs for every frozen design/check and a separate geometry query. Executor build/runtime/process receipts and scientific reporting remain required.'}
        _write(out/'bundle.json',metadata)
        digest=producer_source.digest(out/'bundle.json')
        verify(out,manifest_sha256=digest)
        return {'directory':str(out),'sha256':digest,'designs':len(geometry),'checks':len(jobs),'can_launch':False}
    except Exception as error:
        # Partial files stay visible; never reuse them as a complete bundle.
        _write(out/'bundle-failed.json',{'error':str(error),'can_launch':False})
        raise


def verify(root, *, manifest_sha256):
    """Check a retained bundle using its independently recorded preparation digest.

    This authenticates the preparation artifact, not an executor or remote run.
    """
    root=directory(root)
    raw=(root/'bundle.json').read_bytes()
    if _digest(raw)!=manifest_sha256:raise ValueError('bundle manifest differs from preparation receipt')
    metadata=_json(raw)
    if metadata.get('schema')!=1 or metadata.get('kind')!='final_refinement_input_bundle' or metadata.get('can_launch') is not False:
        raise ValueError('invalid input bundle')
    observed=census(root);observed['files'].pop('bundle.json',None)
    if observed!=metadata['inventory']:raise ValueError('bundle file census changed')
    result=_json((root/'selection.json').read_bytes())
    if (result.get('capture_manifest_sha256')!=metadata['capture_manifest_sha256']
            or result.get('source_commit')!=metadata['reviewed_producer_commit']
            or result.get('can_select') is not True or result.get('can_launch') is not False):
        raise ValueError('bundle changed the captured study')
    definitions=RegisteredDesigns.load(root/'inputs/reference_data.json',root/'inputs/starts.json',
        reference_sha256=metadata['reference_sha256'],starts_sha256=metadata['starts_sha256'])
    meshes={2:{'path':root/'inputs/L2.msh','sha256':metadata['L2_sha256']},
            3:{'path':root/'inputs/L3.msh','sha256':metadata['mesh_evidence']['mesh_sha256']}}
    jobs,geometry=_schedule(result,definitions,meshes)
    for label,rows in (('jobs',jobs),('geometry',geometry)):
        if _exact(metadata[label])!=_exact([{k:v for k,v in row.items() if k!='request'} for row in rows]):
            raise ValueError('bundle changed frozen '+label)
        for row in rows:
            if _exact(_json((root/row['request_file']).read_bytes()))!=_exact(row['request']):
                raise ValueError('bundle request differs from complete frozen design/plan')
    after=census(root);manifest=after['files'].pop('bundle.json')
    if manifest['sha256']!=manifest_sha256 or after!=metadata['inventory']:
        raise ValueError('bundle changed during verification')
    return metadata


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for key in ('capture','manifest-sha256','git-repo','reviewed-commit','capture-tool-commit','mesh-evidence-commit','l3-mesh','out'):
        parser.add_argument('--'+key,required=True)
    args=vars(parser.parse_args());root=args.pop('capture')
    print(json.dumps(prepare(root,**args)))


if __name__=='__main__':main()
