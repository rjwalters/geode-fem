"""Private-copy mutation checks of read-only archive evidence boundaries."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE=Path(__file__).resolve().parent
MUTATIONS=(
    ('bound_hash_ignored','if expected is not None and observed != _sha(expected):','if False:'),
    ('queue_unchecked','        _queue(jobs)','        pass'),
    ('frozen_unit_changed'," or raw[key] != session[key]",''),
    ('training_mesh_unchecked',"if ready.get('train_msh_sha256') != _sha(model.get('train_mesh_sha256')):",'if False:'),
    ('scientific_failures_dropped',"'attempts': list(attempts.values())", "'attempts': [a for a in attempts.values() if (a.get('trace') or {}).get('stop_reason') != 'error']"),
    ('contaminated_attempts_hidden',"'attempts': list(attempts.values())", "'attempts': [a for a in attempts.values() if not a['host_contaminated']]"),
    ('incomplete_loader_grants_selection',"'can_select': False", "'can_select': True"),
    ('live_owner_unchecked','fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)','None'),
    ('end_of_read_hash_check_omitted','        archive.verify_stable()','        pass'),
    ('copied_source_identity_rewritten',"'original_session_directory': source", "'original_session_directory': str(archive.root)"),
    ('duplicate_json_keys_accepted','if key in out:', 'if False:'),
    ('overflow_json_numbers_accepted','if not math.isfinite(number):', 'if False:'),
)


def main():
    source=(HERE/'trace_archive.py').read_text()
    test=(HERE/'test_trace_archive.py').read_text()
    rows=[]
    for name,before,after in (('baseline',None,None),*MUTATIONS):
        mutated=source
        if before is not None:
            assert source.count(before)==1,name
            mutated=source.replace(before,after)
            compile(mutated,'trace_archive.py','exec')
        with tempfile.TemporaryDirectory(prefix='motor-archive-mutation-') as td:
            root=Path(td);(root/'trace_archive.py').write_text(mutated)
            (root/'test_trace_archive.py').write_text(test)
            r=subprocess.run([sys.executable,'-I',str(root/'test_trace_archive.py')],capture_output=True,text=True,timeout=20)
        passed=r.returncode==0 if before is None else r.returncode!=0 and 'AssertionError' in r.stderr
        rows.append({'name':name,'expected_result':passed,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
    print(json.dumps({'source_sha256':{name:hashlib.sha256((HERE/name).read_bytes()).hexdigest()
                                     for name in ('trace_archive.py','test_trace_archive.py','archive_mutations.py')},
                      'checks':rows},indent=2))
    return 0 if all(r['expected_result'] for r in rows) else 1


if __name__=='__main__':sys.exit(main())
