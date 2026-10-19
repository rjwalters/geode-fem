"""Run explicit unsafe guard mutations in memory, retaining complete test output.

No repository source is rewritten. This complements the independently repinned
bad-evidence cases; it is author regression evidence, not independent review.
"""
import argparse
import hashlib
import inspect
import io
import json
from pathlib import Path
import sys
import unittest
from unittest import mock

HERE=Path(__file__).resolve().parent;sys.path.insert(0,str(HERE))
import reboot_attestation as proof
import refinement_bundle
import capture_archive
import test_regenerate_platform

UNIT='test_reboot_attestation.AttestationTests.'
PROCESS='test_reboot_attestation.ProcessAdapterTests.'
CASES=[
    ('omit B1 launch boundary','B1 launch does not fit boot history',UNIT+'test_b1_launch_must_follow_b0_end_in_both_cloud_records'),
    ('omit B1 boot tail','B1 snapshot omitted or inserted a boot',UNIT+'test_b1_snapshot_cannot_hide_an_intermediate_boot'),
    ('omit B2 boot tail','missing, extra or unobserved intermediate boot',UNIT+'test_b2_tail_check_has_its_own_refusal'),
    ('omit older boot history equality','boot history changed since recovery',UNIT+'test_b2_requires_unchanged_older_boot_history'),
    ('omit torn journal guard','open/torn process journal',PROCESS+'test_unfinished_spawn_missing_receipt_and_torn_journal_refused'),
    ('ignore external policy digest','policy differs from independent digest',UNIT+'test_independent_policy_witness_and_inventory_digests_required'),
    ('ignore original inventory digest','inventory differs from independent digest',UNIT+'test_independent_policy_witness_and_inventory_digests_required'),
    ('ignore independent B1 witness','changed B1 witness',UNIT+'test_independent_policy_witness_and_inventory_digests_required'),
    ('omit B1 recovery-only statement','missing recovery-only witness statement',UNIT+'test_recovery_statement_processes_and_journal_bindings_rechecked_after_repin'),
    ('allow B1 work process','B1 work process present',UNIT+'test_recovery_statement_processes_and_journal_bindings_rechecked_after_repin'),
    ('omit B1 journal identity binding','B1 journal binding changed',UNIT+'test_recovery_statement_processes_and_journal_bindings_rechecked_after_repin'),
    ('allow file newer than B0','source file is malformed or newer than study boot end',UNIT+'test_one_nanosecond_after_study_end_refused'),
    ('omit complete source census','missing or extra source file',UNIT+'test_census_missing_extra_changed_members_refused'),
    ('allow unfinished tracked spawn','unfinished tracked spawn',PROCESS+'test_unfinished_spawn_missing_receipt_and_torn_journal_refused'),
    ('allow absent exit receipt','missing or malformed exit receipt',PROCESS+'test_unfinished_spawn_missing_receipt_and_torn_journal_refused'),
    ('hide malformed old-boot identity','malformed or changed original process identity',PROCESS+'test_old_boot_must_not_hide_malformed_or_changed_identity'),
    ('accept other producer refusals','producer refusal is not exclusively the attested boot change',PROCESS+'test_unfinished_spawn_missing_receipt_and_torn_journal_refused'),
    ('allow live B2 work process','live evaluator/session/analysis worker:',UNIT+'test_live_study_interpreters_and_workers_refused'),
    ('unpin collector identity/source','collector identity/source differs',UNIT+'test_collector_exception_is_source_identity_and_command_bound'),
    ('ignore collector command','collector command differs',UNIT+'test_collector_exception_is_source_identity_and_command_bound'),
    ('allow nonzero session exit','session exit was not successful',UNIT+'test_successful_exit_must_precede_study_shutdown'),
    ('allow exit after shutdown','session exit outside original study boot',UNIT+'test_successful_exit_must_precede_study_shutdown'),
    ('allow receipt before session completion','launcher receipt predates session completion',UNIT+'test_terminal_order_uses_receipt_file_precision_not_truncated_text'),
    ('omit terminal shutdown evidence','no terminal shutdown evidence',UNIT+'test_missing_malformed_and_changed_shutdown_evidence_refused'),
    ('ignore instance/state identity','changed EC2 instance/state',UNIT+'test_host_machine_instance_volume_and_launch_changes_refused'),
]

ARCHIVE='test_reboot_capture.RebootCaptureTests.'
REPLAY_CASES=[
    ('J15 trust recorded boot verdict','_reboot',
     'verdict = reboot_attestation.observations(p, history, observed)',"verdict = evidence.get('reboot_verdict')",
     ARCHIVE+'test_replay_rechecks_boot_chain_after_archive_repin'),
    ('J16 skip copied helper digest','_tool',
     "if producer_source.digest(reader.path('capture-tool/'+name)) != digest:",
     "if name != 'reboot_attestation.py' and producer_source.digest(reader.path('capture-tool/'+name)) != digest:",
     ARCHIVE+'test_replay_rehashes_copied_reboot_tool_after_repin'),
    ('J18 ignore recorded verdict','_reboot',"evidence.get('reboot_verdict') != verdict",'False',
     ARCHIVE+'test_replay_compares_recorded_reboot_verdict'),
    ('J19 ignore recorded process report','_processes','if comparable(replay) != comparable(recorded):','if False:',
     ARCHIVE+'test_replay_compares_process_reports'),
    ('J21 ignore phase stability','_reboot','if phases[0][key] != phases[1][key]:','if False:',
     ARCHIVE+'test_replay_compares_otherwise_valid_shutdown_observations_between_phases'),
    ('J22 ignore session exit ordering','_processes',
     "reboot_attestation.terminal_exit(events,observed,reboot['policy'])",'pass',
     ARCHIVE+'test_replay_checks_changed_session_finished_time_after_fixture_repin'),
    ('J23 ignore machine identity replay','_reboot','reboot_attestation.machine_observation(observed, p)','pass',
     ARCHIVE+'test_replay_rehashes_machine_identity_bytes'),
]

QUIET='test_reboot_quiet.QuietTests.'
QUIET_CASES=[
    ('allow non-system UID','system_daemon','uid < 1000','uid < 10000',QUIET+'test_daemon_lookalikes_never_gain_exception'),
    ('allow negative UID','system_daemon','0 <= uid','-100 <= uid',QUIET+'test_daemon_lookalikes_never_gain_exception'),
    ('allow bool UID','system_daemon','type(uid) is int','isinstance(uid, int)',QUIET+'test_daemon_lookalikes_never_gain_exception'),
    ('omit daemon parent','system_daemon',"and row.get('ppid') == 1",'',QUIET+'test_daemon_lookalikes_never_gain_exception'),
    ('allow bool parent','system_daemon',"type(row.get('ppid')) is int","isinstance(row.get('ppid'), int)",QUIET+'test_daemon_lookalikes_never_gain_exception'),
    ('allow arbitrary daemon command','system_daemon',"tuple(command['argv']) in SYSTEM_DAEMONS.get(command['comm'], ())",'True',QUIET+'test_daemon_lookalikes_never_gain_exception'),
    ('ignore exact daemon comm','system_daemon',"SYSTEM_DAEMONS.get(command['comm'], ())",'tuple(shape for shapes in SYSTEM_DAEMONS.values() for shape in shapes)',QUIET+'test_daemon_lookalikes_never_gain_exception'),
    ('capture before quiet','wait_for_quiet','if not issues:','if True:',QUIET+'test_busy_then_quiet_retries_inside_same_boot'),
    ('omit refusal log','wait_for_quiet','log(message)','pass',QUIET+'test_ten_busy_checks_log_all_refusals_then_fail'),
    ('stop before tenth quiet check','wait_for_quiet','range(1, 11)','range(1, 10)',QUIET+'test_tenth_clean_check_can_succeed'),
    ('K14 kernel parent not one','quiet_processes',"by_pid[row['pid']].get('ppid') == 2","by_pid[row['pid']].get('ppid') != 1",UNIT+'test_capture_kprobe_kernel_worker_requires_parent_group_and_empty_command'),
    ('K23 case-sensitive classifier','work_process','name, re.I','name, 0',UNIT+'test_live_study_interpreters_and_workers_refused'),
    ('omit archive quiet history','_reboot',"reboot_attestation.quiet_history(observed, m['capture_tool_sha256']['closed_capture.py'])",'pass',ARCHIVE+'test_replay_rechecks_quiet_refusal_history_after_repin'),
]
CASES += [
    ('omit quiet boot equality','host/boot changed while waiting for quiet',QUIET+'test_changed_boot_aborts_without_retrying_new_boot'),
    ('omit quiet time budget','quiet check time budget exhausted',QUIET+'test_time_budget_is_bounded'),
    ('omit retry ordering','invalid quiet check ordering',QUIET+'test_history_replays_refusals_order_host_and_final_snapshot'),
    ('omit retry refusal equality','quiet check refusal log differs',QUIET+'test_history_replays_refusals_order_host_and_final_snapshot'),
    ('omit retry final equality','quiet check final snapshot differs',QUIET+'test_history_replays_refusals_order_host_and_final_snapshot'),
]


def run(names):
    stream=io.StringIO()
    suite=unittest.TestLoader().loadTestsFromNames(names)
    result=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite)
    return {'successful':result.wasSuccessful(),'tests':result.testsRun,
            'failures':len(result.failures),'errors':len(result.errors),'output':stream.getvalue()}


def changed_function(module,name,before,after):
    source=inspect.getsource(getattr(module,name))
    if source.count(before)!=1:raise ValueError('mutation no longer matches exactly once: '+name)
    namespace=dict(module.__dict__)
    exec(compile(source.replace(before,after),'<explicit unsafe mutation: '+name+'>','exec'),namespace)
    return namespace[name]


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',required=True,type=Path)
    args=parser.parse_args()
    if args.out.exists():raise ValueError('mutation output must be new')
    mesh_test='test_refinement_bundle.RefinementBundleTests.test_external_l3_mesh_must_match_reviewed_report_bytes'
    platform_test='test_regenerate_platform.RegenerationPlatformTests.test_unsupported_platform_refuses_before_creating_output_or_running_commands'
    boot_test=UNIT+'test_missing_extra_intermediate_reused_and_unobserved_boots_refused'
    kernel_b1=UNIT+'test_recovery_kprobe_kernel_worker_requires_exact_name_and_kernel_parent'
    kernel_b2=UNIT+'test_capture_kprobe_kernel_worker_requires_parent_group_and_empty_command'
    argv_test=UNIT+'test_interpreter_argv_cannot_hide_behind_a_script_process_name'
    baseline=run(sorted(set([case[2] for case in CASES]+[case[4] for case in REPLAY_CASES+QUIET_CASES]+
                            [mesh_test,platform_test,boot_test,kernel_b1,kernel_b2,argv_test])))
    if not baseline['successful']:raise RuntimeError('mutation baseline failed:\n'+baseline['output'])
    rows=[];original=proof.require
    for label,prefix,test in CASES:
        def omit(condition,message):
            if not message.startswith(prefix):original(condition,message)
        with mock.patch.object(proof,'require',side_effect=omit):result=run([test])
        rows.append({'mutation':label,'killed':not result['successful'],**result})
    for label,function,before,after,test in REPLAY_CASES:
        altered=changed_function(capture_archive,function,before,after)
        with mock.patch.object(capture_archive,function,altered):result=run([test])
        rows.append({'mutation':label,'killed':not result['successful'],**result})
    altered=changed_function(proof,'quiet_processes',"(name, *row['argv'][:2])",'(name,)')
    with mock.patch.object(proof,'quiet_processes',altered):result=run([argv_test])
    rows.append({'mutation':'ignore interpreter argv','killed':not result['successful'],**result})
    from test_reboot_attestation import B0,B1,B2
    original_boots=proof.boot_list
    def filter_boots(raw):
        rows=[r for r in original_boots(raw) if r['boot'] in {proof.boot_id(b) for b in (B0,B1,B2)}]
        return [dict(row,index=i-len(rows)+1) for i,row in enumerate(rows)]
    with mock.patch.object(proof,'boot_list',side_effect=filter_boots):result=run([boot_test])
    rows.append({'mutation':'hide unobserved intermediate boots','killed':not result['successful'],**result})
    altered=changed_function(refinement_bundle,'_l3',"or report.get('msh_sha256')!=_digest(contents[MESH]) ",'')
    with mock.patch.object(refinement_bundle,'_l3',altered):result=run([mesh_test])
    rows.append({'mutation':'remove external L3 SHA comparison','killed':not result['successful'],**result})
    module=test_regenerate_platform.regeneration
    altered=changed_function(module,'regenerate',"if host!={'system':'Darwin','machine':'arm64'}:",'if False:')
    with mock.patch.object(module,'regenerate',altered):result=run([platform_test])
    rows.append({'mutation':'remove historical regeneration platform guard','killed':not result['successful'],**result})
    for label,function,before,after,test in [
        ('allow B1 userspace kprobe name','recovery',"r[1] == 2 and r[2] == 'kprobe-optimizer'","r[2] == 'kprobe-optimizer'",kernel_b1),
        ('ignore B2 kernel parent','quiet_processes',"and by_pid[row['pid']].get('ppid') == 2",'',kernel_b2),
        ('ignore B2 kernel group','quiet_processes',"and by_pid[row['pid']]['pgid'] == 0",'',kernel_b2),
        ('ignore B2 kernel command','quiet_processes',"and row['argv'] == []",'',kernel_b2),
    ]:
        altered=changed_function(proof,function,before,after)
        with mock.patch.object(proof,function,altered):result=run([test])
        rows.append({'mutation':label,'killed':not result['successful'],**result})
    for label,function,before,after,test in QUIET_CASES:
        module=capture_archive if function=='_reboot' else proof
        altered=changed_function(module,function,before,after)
        with mock.patch.object(module,function,altered):result=run([test])
        rows.append({'mutation':label,'killed':not result['successful'],**result})
    sources={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in
             [HERE/'test_reboot_quiet.py',HERE/'reboot_attestation.py',HERE/'capture_archive.py',HERE/'test_reboot_capture.py',
              HERE/'refinement_bundle.py',HERE/'reboot_mutations.py',
              HERE/'smoke/regenerate_meshes.py',HERE/'test_reboot_attestation.py',
              HERE/'test_refinement_bundle.py',HERE/'test_regenerate_platform.py']}
    record={'scope':'Author synthetic guard-deletion/replacement mutations; no cloud or scientific outcomes.',
            'baseline':baseline,'sources_sha256':sources,'mutations':rows,
            'injected':len(rows),'killed':sum(r['killed'] for r in rows)}
    with args.out.open('x') as stream:json.dump(record,stream,indent=2);stream.write('\n')
    print(json.dumps({'file':str(args.out),'injected':record['injected'],'killed':record['killed'],
                      'sha256':hashlib.sha256(args.out.read_bytes()).hexdigest()}))
    return 0 if record['injected']==record['killed'] else 1


if __name__=='__main__':sys.exit(main())
