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
import closed_capture
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
     'verdict = reboot_attestation.observations(p, history, observed, failed)',"verdict = evidence.get('reboot_verdict')",
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
    ('N9a trust recorded retry issues','quiet_history',"issues = quiet_refusals(snapshot, digest)","issues = record.get('issues')",QUIET+'test_history_refuses_independent_replay_forgeries'),
    ('N9f remove history length cap','quiet_history','1 <= len(records) <= 10','1 <= len(records)',QUIET+'test_history_refuses_independent_replay_forgeries'),
    ('N9g remove history elapsed cap','quiet_history','0 <= elapsed <= 600','0 <= elapsed',QUIET+'test_history_refuses_independent_replay_forgeries'),
    ('N9h remove attempt numbering','quiet_history',"record.get('attempt') == number and ",'',QUIET+'test_history_refuses_independent_replay_forgeries'),
    ('allow stripped history','quiet_history',"require(not required, 'missing quiet check history')",'pass',ARCHIVE+'test_replay_refuses_removed_history_after_repin'),
    ('allow truncated retry prefix','quiet_history',"require(number != 1 or elapsed < 60, 'quiet check history starts after first interval')",'pass',ARCHIVE+'test_replay_refuses_truncated_renumbered_history_after_repin'),
    ('N8a allow failing final attempt','quiet_history','bool(issues) == (number < len(records))','(number == len(records) or bool(issues))',QUIET+'test_history_refuses_failing_final_or_clean_nonfinal_attempt'),
    ('N8b allow clean nonfinal attempt','quiet_history','bool(issues) == (number < len(records))','(number < len(records) or not issues)',QUIET+'test_history_refuses_failing_final_or_clean_nonfinal_attempt'),
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
    ('omit archive quiet history','_reboot',"reboot_attestation.quiet_history(observed, m['capture_tool_sha256']['closed_capture.py'],\n            required=m['capture_tool_sha256']['reboot_attestation.py'] != legacy_no_retry)",'pass' ,ARCHIVE+'test_replay_rechecks_quiet_refusal_history_after_repin'),
]
CASES += [
    ('omit quiet boot equality','host/boot changed while waiting for quiet',QUIET+'test_changed_boot_aborts_without_retrying_new_boot'),
    ('omit quiet time budget','quiet check time budget exhausted',QUIET+'test_time_budget_is_bounded'),
    ('omit retry ordering','invalid quiet check ordering',QUIET+'test_history_replays_refusals_order_host_and_final_snapshot'),
    ('omit retry refusal equality','quiet check refusal log differs',QUIET+'test_history_replays_refusals_order_host_and_final_snapshot'),
    ('omit retry final equality','quiet check final snapshot differs',QUIET+'test_history_replays_refusals_order_host_and_final_snapshot'),
]

FOUR='test_reboot_four_boot.FourBootTests.'
FAKE='test_reboot_four_boot.FakeJournalctlTests.'
PLUMB='test_reboot_four_boot.FailedEvidencePlumbingTests.'
FOUR_ARCHIVE='test_reboot_four_boot.FourBootCaptureTests.'
# Four-boot chain (B0 -> B1 -> refused B2 -> B3): one deletion per new guard.
CASES += [
    ('F01 allow four-boot boot reuse','four-boot policy reuses a boot',FOUR+'test_policy_requires_exact_four_boot_fields_and_distinct_boots'),
    ('F02 ignore independent B2 witness','changed B2 witness',FOUR+'test_b2_witness_hash_mismatch_refused'),
    ('F03 accept other B2 witness kind','unsupported failed-capture witness',FOUR+'test_witness_kind_identity_and_statement_required'),
    ('F04 omit B2 file set','missing or extra failed-capture evidence',FOUR+'test_b2_witness_hash_mismatch_refused'),
    ('F05 omit B2 file hashes','changed failed-capture evidence',FOUR+'test_b2_witness_hash_mismatch_refused'),
    ('F06 omit B2 witness identity','B2 witness changed ',FOUR+'test_witness_kind_identity_and_statement_required'),
    ('F07 omit B2 witness statement','missing refused-capture witness statement',FOUR+'test_witness_kind_identity_and_statement_required'),
    ('F08 omit B2 start/stop requests','B2 start/stop requests differ',FOUR+'test_ec2_start_and_stop_records_bind_instance_and_b1_stop'),
    ('F09 omit B1 stop to B2 start link','B2 start does not follow the witnessed B1 stop',FOUR+'test_ec2_start_and_stop_records_bind_instance_and_b1_stop'),
    ('F10 omit B2 attachment','B2 attachment changed',FOUR+'test_ec2_start_and_stop_records_bind_instance_and_b1_stop'),
    ('F11 omit B2 stop record binding','B2 stop record changed launch/attachment',FOUR+'test_ec2_start_and_stop_records_bind_instance_and_b1_stop'),
    ('F12 omit B2 stop reason','missing witnessed B2 stop',FOUR+'test_ec2_start_and_stop_records_bind_instance_and_b1_stop'),
    ('F13 omit EC2 record shape','malformed EC2 instance record',FOUR+'test_ec2_start_and_stop_records_bind_instance_and_b1_stop'),
    ('F14 allow B2 launch before B1 stop','B2 launch/stop ordering differs',FOUR+'test_b2_launch_before_b1_stop_refused'),
    ('F15 omit summary quiet/census/archive','B2 refusal summary shows quiet/census/archive activity',FOUR+'test_refusal_summary_census_quiet_or_archive_refused'),
    ('F16 omit summary retry/stop','B2 refusal summary shows a retry or no stop',FOUR+'test_refusal_summary_census_quiet_or_archive_refused'),
    ('F17 omit summary launch/log binding','B2 refusal summary changed launch or capture log',FOUR+'test_refusal_summary_census_quiet_or_archive_refused'),
    ('F18 omit B2 log refusal line','B2 capture log lacks the journalctl refusal',FOUR+'test_capture_log_requires_terminal_journalctl_refusal'),
    ('F19 omit B2 log quiet/census scan','B2 capture log shows a quiet check or census',FOUR+'test_capture_log_showing_a_quiet_attempt_refused'),
    ('F20 omit policy/evidence chain match','failed-capture evidence does not match the policy boot chain',FOUR+'test_evidence_must_match_policy_kind'),
    ('F21 omit B1 end/B2 launch/B2 first order','B1 stop/B2 launch/B2 first entry ordering differs',FOUR+'test_b2_launch_must_follow_b1_journal_and_precede_b2_first_entry'),
    ('F22 omit B2 stop vs journal end','B2 stop follows its journal end',FOUR+'test_b2_stop_must_not_follow_its_journal'),
    ('F23 omit B2 end/B3 launch order','B2 journal end/B3 launch ordering differs',FOUR+'test_b3_launch_must_follow_b2_journal_end'),
    ('F24 allow B2 journal work process','B2 journal shows evaluator/session/analysis activity',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F25 omit B2 journal field types','malformed B2 journal process field',FOUR+'test_b2_journal_process_fields_must_be_text'),
    ('F26 accept B2 evidence on three-boot policy','four-boot policy requires, and three-boot policy refuses',PLUMB+'test_three_boot_policy_given_b2_evidence_refused'),
    ('F27 accept four-boot policy without B2','four-boot policy requires, and three-boot policy refuses',PLUMB+'test_four_boot_policy_without_b2_evidence_refused'),
    ('F28 allow B3 reuse of B2','capture reused an earlier boot',FOUR+'test_reordered_b2_b3_refused'),
    ('F29 accept B2 host in process report','not the attested capture host',FOUR+'test_process_report_refuses_every_earlier_boot_host'),
    ('F30 allow extra/missing/reordered B2-B3 tail','missing, extra or unobserved intermediate boot',FOUR+'test_extra_boot_refused'),
    ('F31 replay allows B2 journal work process','B2 journal shows evaluator/session/analysis activity',FOUR_ARCHIVE+'test_replay_rechecks_b2_journal_activity_after_repin'),
    ('F32 replay trusts archived B2 bytes','changed failed-capture evidence',FOUR_ARCHIVE+'test_replay_rehashes_failed_capture_files_after_repin'),
]
FOUR_CASES=[
    ('F40 pass hyphenated boot to journalctl',proof,'live_observation',"'-b', boot_id(boot), '-o'","'-b', boot, '-o'",FAKE+'test_live_observation_runs_real_argv_through_systemd_255_parser'),
    ('F41 read only B0/B1 journals',proof,'live_observation','for boot in ended_boots(p)}',"for boot in (p['study_boot'], p['recovery_boot'])}",FAKE+'test_live_observation_runs_real_argv_through_systemd_255_parser'),
    ('F42 drop refusal rc/stderr',proof,'_command',"' (rc=' + str(result.returncode) + '; stderr: ' + detail + ')'","''",FAKE+'test_hyphenated_boot_refusal_reports_exit_code_and_stderr'),
    ('F43 unbounded refusal stderr',proof,'_command','splitlines()[:5]','splitlines()',FAKE+'test_command_refusal_keeps_five_stderr_lines_and_no_stdout'),
    ('F44 classify two B2 cmdline words',proof,'failed_boot_activity','(comm, exe, *argv)','(comm, exe, *argv[:2])',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F45 ignore B2 executable',proof,'failed_boot_activity','(comm, exe, *argv)','(comm, *argv)',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F46 allow user-UID B2 daemons',proof,'failed_boot_activity','system = uid is not None and 0 <= uid < 1000','system = True',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F47 ignore B2 daemon executable',proof,'failed_boot_activity',
     "(argv[0] == '/usr/bin/python3' if interpreter\n                     else PurePosixPath(exe).name == PurePosixPath(argv[0].lstrip('@')).name)",'True',
     FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F48 loose B2 Python service prefix',proof,'failed_boot_activity',"tuple(argv[:2]) == B2_PYTHON_SERVICES[comm]","argv[:1] == ['/usr/bin/python3']",FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F49 collector without -I -B',proof,'failed_boot_activity',"set(argv[1:3]) == {'-I', '-B'}",'True',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F50 collector any option',proof,'failed_boot_activity','all(argv[i] in B2_COLLECTOR_OPTIONS for i in range(4, len(argv), 2))','True',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F51 collector relative script',proof,'failed_boot_activity','PurePosixPath(argv[3]).is_absolute() and ','',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F52 collector any script',proof,'failed_boot_activity',"PurePosixPath(argv[3]).name == 'closed_capture.py'",'True',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F53 collector odd argv',proof,'failed_boot_activity','len(argv) % 2 == 0','True',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F54 daemon comm alone not refused',proof,'failed_boot_activity','if comm in SYSTEM_DAEMONS or any(','if any(',FOUR+'test_b2_journal_with_a_work_process_refused'),
    ('F55 skip B2 shutdown journal',proof,'observations','zip(earlier, boots[-tail:-1])','zip(earlier[:2], boots[-tail:-1][:2])',FOUR+'test_b2_journal_must_be_a_complete_shutdown_journal'),
    ('F56 compare B2 with B1-era history',proof,'observations','older = boots[:len(boots) - (tail - 2)]','older = boots[:-1]',FOUR+'test_complete_four_boot_proof_accepts'),
    ('F57 replay without four-boot file set',capture_archive,'_reboot','if failed_names != expected_failed:','if False:',FOUR_ARCHIVE+'test_replay_refuses_missing_or_extra_failed_capture_file'),
    ('F58 replay three-boot with B2 files',capture_archive,'_reboot',"if failed_names or 'reboot/failed-capture' in m['inventory']['directories']:",'if False:',ARCHIVE+'test_three_boot_replay_refuses_failed_capture_evidence'),
    ('F59 replay three-boot ignoring B2 directory',capture_archive,'_reboot',"if failed_names or 'reboot/failed-capture' in m['inventory']['directories']:",'if failed_names:',ARCHIVE+'test_three_boot_replay_refuses_failed_capture_evidence'),
    ('F60 drop B2 identity from replay result',capture_archive,'load',"if reboot['failed'] is not None:",'if False:',FOUR_ARCHIVE+'test_four_boot_portable_replay_verifies_failed_capture_witness'),
    ('F61 allow B2 evidence without policy',closed_capture,'capture','if failed_capture_evidence is not None and reboot_policy is None:','if False:',PLUMB+'test_capture_refuses_failed_capture_evidence_without_policy'),
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
                            [case[5] for case in FOUR_CASES]+
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
    for label,module,function,before,after,test in FOUR_CASES:
        altered=changed_function(module,function,before,after)
        with mock.patch.object(module,function,altered):result=run([test])
        rows.append({'mutation':label,'killed':not result['successful'],**result})
    sources={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in
             [HERE/'test_reboot_quiet.py',HERE/'reboot_attestation.py',HERE/'capture_archive.py',HERE/'test_reboot_capture.py',
              HERE/'refinement_bundle.py',HERE/'reboot_mutations.py',
              HERE/'smoke/regenerate_meshes.py',HERE/'test_reboot_attestation.py',
              HERE/'test_refinement_bundle.py',HERE/'test_regenerate_platform.py',
              HERE/'closed_capture.py',HERE/'test_reboot_four_boot.py',HERE/'failed_capture_witness.py']}
    record={'scope':'Author synthetic guard-deletion/replacement mutations; no cloud or scientific outcomes.',
            'baseline':baseline,'sources_sha256':sources,'mutations':rows,
            'injected':len(rows),'killed':sum(r['killed'] for r in rows)}
    with args.out.open('x') as stream:json.dump(record,stream,indent=2);stream.write('\n')
    print(json.dumps({'file':str(args.out),'injected':record['injected'],'killed':record['killed'],
                      'sha256':hashlib.sha256(args.out.read_bytes()).hexdigest()}))
    return 0 if record['injected']==record['killed'] else 1


if __name__=='__main__':sys.exit(main())
