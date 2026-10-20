# Disposable quiet-process rehearsal

`run_quiet_rehearsal.sh` executes a prepared `quiet_rehearsal.py` package on a
new EC2 instance. It produces a process-classification report using the same AMI
and instance type as the source. It does not run the study or authenticate study
results. Obtain operator authorization for the disposable launch before use.

Prepare the package with `quiet_rehearsal.py` using retained instance/image JSON,
the reviewed repository and pinned producer commit (see `--help`). Inspect its
`preparation.json`, `launch.json` and payload hashes, then run:

```sh
bash benchmarks/motor_optimization/final_refinement/run_quiet_rehearsal.sh \
  --execute-authorized-rehearsal /absolute/preparation /absolute/new-output
```

The controller requires Bash, Python 3, AWS CLI, OpenSSH and EC2 Instance
Connect permissions. Every AWS call uses profile `geode-fem-bench` and region
`us-east-1`. The output directory must not exist. Preserve it even on failure;
it contains the launch request, invocation client token, instance description,
controller/remote logs, termination response and fetched report when available.
These operational records can contain host addresses; inspect them before sharing.
Ephemeral SSH keys live in a private temporary directory outside the repository
and are removed by cleanup.

Before launching, the controller checks payload member hashes, study instance and
volume exclusions, AMI/type, a single instance/root disk, delete-on-termination,
required tags and forbidden launch options. After launch it checks the returned
instance identity, AMI/type and volume attachments before transferring the payload.
The first successful SSH command must arm a 75-minute shutdown cap. The launch
request makes that shutdown terminate the disposable instance.

Exit and signal handlers terminate the instance and wait for instance termination
and, when its ID was observed, root-volume deletion. Cleanup ignores further INT
and TERM signals so they cannot interrupt those waits. Remote execution, report
fetch and cleanup failures produce a nonzero exit.

If the launch response is lost, cleanup searches the rehearsal tags and requires
exactly one instance with this invocation's recorded client token, expected
AMI/type, tags and permitted root volume. It never terminates a tag match alone.
If discovery cannot establish that identity, cleanup exits nonzero and retains
`launch-recovery.json` and `client-token.txt` for operator reconciliation. This is
a best-effort fallback: EC2 eventual consistency, API failures, controller death
or SIGKILL can prevent cleanup. The shutdown cap is not armed until SSH succeeds.

Run the isolated fake-cloud tests with:

```sh
python3.12 -I -B -m unittest discover \
  -s benchmarks/motor_optimization/final_refinement \
  -p 'test_reboot_launcher.py' -v
```

The tests cover malformed preparations, unexpected live attachments/identity,
lost launch responses, shutdown-cap refusal, signals, remote/report failures and
cleanup verification. They call fake AWS/SSH tools and launch no cloud resources.
