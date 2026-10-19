"""Daemon identity and same-boot retry regressions; synthetic processes only."""
import copy
import json
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reboot_attestation as proof
from test_reboot_attestation import fixture, B1


def process(obs, comm, argv, *, uid=0, parent=1, pid=700):
    obs['process_table'].append({'pid': pid, 'ppid': parent, 'pgid': pid, 'start': 'system boot', 'state': 'S'})
    obs['process_commands'].append({'pid': pid, 'comm': comm, 'argv': list(argv), 'uid': uid, 'disappeared': False})
    return obs


class QuietTests(unittest.TestCase):
    def setUp(self):
        self.observed = fixture()[3]
        self.digest = 'd'*64

    def quiet(self, observed):
        return proof.quiet_processes(observed, capture_script_sha256=self.digest)

    def test_exact_system_daemon_shapes_accepted(self):
        for name, shapes in proof.SYSTEM_DAEMONS.items():
            for argv in shapes:
                for uid in (0, 999):
                    with self.subTest(name=name, uid=uid):
                        self.assertTrue(self.quiet(process(copy.deepcopy(self.observed), name, argv, uid=uid)))
        # Explicitly pin both Ubuntu shapes independently of the allowlist.
        self.assertTrue(self.quiet(process(copy.deepcopy(self.observed), 'unattended-upgr',
            ['/usr/bin/python3', '/usr/share/unattended-upgrades/unattended-upgrade-shutdown', '--wait-for-signal'])))
        self.assertTrue(self.quiet(process(copy.deepcopy(self.observed), 'networkd-dispat',
            ['/usr/bin/python3', '/usr/bin/networkd-dispatcher', '--run-startup-triggers'])))

    def test_daemon_lookalikes_never_gain_exception(self):
        for name, shapes in proof.SYSTEM_DAEMONS.items():
            for shape in shapes:
                for uid in (None, True, -1, 1000, 1001):
                    with self.subTest(name=name, uid=uid):
                        with self.assertRaisesRegex(ValueError, 'live evaluator'):
                            self.quiet(process(copy.deepcopy(self.observed), name, shape, uid=uid))
                for parent in (0, 2, True, None, 500):
                    with self.subTest(name=name, parent=parent):
                        with self.assertRaisesRegex(ValueError, 'live evaluator'):
                            self.quiet(process(copy.deepcopy(self.observed), name, shape, parent=parent))
                bad = [list(shape)+['extra'], list(shape[:-1]), ['/tmp/'+Path(shape[0]).name, *shape[1:]]]
                if len(shape)>1:
                    bad += [[shape[0], path, *shape[2:]] for path in
                            ('/usr/bin/other-script', '/home/ubuntu/networkd-dispatcher', '/tmp/networkd-dispatcher')]
                for argv in bad:
                    with self.subTest(name=name, argv=argv):
                        with self.assertRaisesRegex(ValueError, 'live evaluator'):
                            self.quiet(process(copy.deepcopy(self.observed), name, argv))
                with self.assertRaisesRegex(ValueError, 'live evaluator'):
                    self.quiet(process(copy.deepcopy(self.observed), name+'-lookalike', shape))

    def test_refusal_contains_every_busy_process(self):
        obs=copy.deepcopy(self.observed)
        process(obs,'python3',['/usr/bin/python3','/tmp/a'],pid=700)
        process(obs,'jupyter',['/usr/bin/jupyter','lab'],pid=701)
        issues=proof.quiet_refusals(obs,self.digest)
        self.assertEqual([r['pid'] for r in issues],[700,701])
        self.assertEqual(issues[0]['argv'],['/usr/bin/python3','/tmp/a'])

    def wait(self, snapshots, times):
        observe=mock.Mock(side_effect=snapshots);sleep=mock.Mock();logs=[]
        result=proof.wait_for_quiet(observe,self.digest,sleep=sleep,
                                   clock=mock.Mock(side_effect=times),log=logs.append)
        return result,observe,sleep,logs

    def test_busy_then_quiet_retries_inside_same_boot(self):
        busy=process(copy.deepcopy(self.observed),'python3',['python3','/tmp/task'])
        result,observe,sleep,logs=self.wait([busy,self.observed],[0,0,60])
        self.assertEqual(observe.call_count,2);sleep.assert_called_once_with(60)
        self.assertEqual(len(logs[0]['issues']),1);self.assertEqual(logs[1]['issues'],[])
        self.assertEqual(len(result['quiet_checks']),2)
        proof.quiet_history(result,self.digest)
        self.assertNotIn('quiet_checks',self.observed)

    def test_clean_first_check_never_sleeps(self):
        result,observe,sleep,logs=self.wait([self.observed],[0,0])
        sleep.assert_not_called();self.assertEqual(observe.call_count,1)
        self.assertEqual(logs[0]['attempt'],1);self.assertEqual(logs[0]['issues'],[])
        proof.quiet_history(result,self.digest)

    def test_tenth_clean_check_can_succeed(self):
        busy=process(copy.deepcopy(self.observed),'python3',['python3'])
        result,observe,sleep,logs=self.wait([busy]*9+[self.observed],[0]+list(range(0,600,60)))
        self.assertEqual(observe.call_count,10);self.assertEqual(sleep.call_count,9)
        self.assertEqual(len(logs),10);proof.quiet_history(result,self.digest)

    def test_ten_busy_checks_log_all_refusals_then_fail(self):
        obs=process(copy.deepcopy(self.observed),'python3',['python3'],pid=700)
        process(obs,'jupyter',['jupyter'],pid=701)
        observe=mock.Mock(return_value=obs);sleep=mock.Mock();logs=[]
        with self.assertRaisesRegex(ValueError,'quiet check exhausted'):
            proof.wait_for_quiet(observe,self.digest,sleep=sleep,
                clock=mock.Mock(side_effect=[0]+list(range(0,600,60))),log=logs.append)
        self.assertEqual(observe.call_count,10);self.assertEqual(sleep.call_count,9)
        self.assertEqual([r['attempt'] for r in logs],list(range(1,11)))
        self.assertTrue(all([r['pid'] for r in item['issues']]==[700,701] for item in logs))

    def test_changed_boot_aborts_without_retrying_new_boot(self):
        busy=process(copy.deepcopy(self.observed),'python3',['python3'])
        changed=copy.deepcopy(self.observed);changed['host']['boot']=B1
        with self.assertRaisesRegex(ValueError,'host/boot changed while waiting'):
            self.wait([busy,changed],[0,0,60])

    def test_time_budget_is_bounded(self):
        with self.assertRaisesRegex(ValueError,'quiet check time budget exhausted'):
            self.wait([self.observed],[0,601])
        busy=process(copy.deepcopy(self.observed),'python3',['python3'])
        observe=mock.Mock(return_value=busy);sleep=mock.Mock();logs=[]
        with self.assertRaisesRegex(ValueError,'quiet check exhausted'):
            proof.wait_for_quiet(observe,self.digest,sleep=sleep,
                clock=mock.Mock(side_effect=[0,550]),log=logs.append)
        sleep.assert_not_called();self.assertEqual(len(logs),1)

    def test_malformed_or_failed_observation_is_not_retried(self):
        bad=copy.deepcopy(self.observed);bad['collector']['script_sha256']='e'*64
        for value in (bad, OSError('process observation failed')):
            observe=mock.Mock(side_effect=[value]);sleep=mock.Mock()
            with self.assertRaises((ValueError,OSError)):
                proof.wait_for_quiet(observe,self.digest,sleep=sleep,clock=lambda:0,log=lambda _:None)
            self.assertEqual(observe.call_count,1);sleep.assert_not_called()

    def test_history_replays_refusals_order_host_and_final_snapshot(self):
        busy=process(copy.deepcopy(self.observed),'python3',['python3'])
        original=self.wait([busy,self.observed],[0,0,60])[0]
        for change,message in [
            (lambda r:r['quiet_checks'][0].update(issues=[]),'refusal log differs'),
            (lambda r:r['quiet_checks'][1].update(issues=[{'fake':True}]),'refusal log differs'),
            (lambda r:r['quiet_checks'][1].update(elapsed_seconds=59),'ordering'),
            (lambda r:r['quiet_checks'][0]['snapshot']['host'].update(boot=B1),'host/boot changed'),
            (lambda r:r['quiet_checks'][1]['snapshot']['process_commands'][0].update(uid=10),'final snapshot differs'),
            (lambda r:r.update(quiet_checks=[]),'invalid quiet check history'),
        ]:
            with self.subTest(message=message):
                r=copy.deepcopy(original);change(r)
                with self.assertRaisesRegex(ValueError,message):proof.quiet_history(r,self.digest)


if __name__=='__main__':unittest.main()
