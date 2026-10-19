# Capture after the documented study/recovery boots

This exception is for a terminal study whose original process identities belong
to study boot B0, followed by a witnessed copy-only recovery boot B1 and **one**
capture boot B2. It does not permit recovery, optimization, or modification of
the original records. The producer's same-boot process verifier still refuses;
the archive retains that refusal alongside a separate termination proof.

The Paper 3 ruling is Squad review 50. The capture code must be independently
approved and merged before B2 starts. Keep the instance stopped while preparing
inputs. A passing synthetic test is not permission to inspect real outcomes.

## Independent inputs, prepared before B2

Retain the producer and capture-tool commits independently. Keep the producer
checkout at its original revision and use a separate directory for the reviewed
capture tool. Do not update the original numerical environment. The ordinary
source, input, build, runtime, study-closure and ownership checks still apply.

Prepare a JSON policy with exactly these fields and retain its SHA-256 outside
the capture. Never obtain the expected digest by accepting the archive's own
policy declaration.

| Field | Meaning |
| --- | --- |
| `schema` | Integer `1` |
| `kind` | `three_boot_closed_capture_policy` |
| `inventory_root` | Original absolute parent containing `main` and `build-record` |
| `inventory_sha256` | Independently retained original SHA inventory digest |
| `session_exit_file` | Exact inventory-relative path to the original main launcher exit receipt |
| `recovery_witness_sha256` | Independently retained B1 witness manifest digest |
| `study_boot`, `recovery_boot` | Original B0/B1 UUIDs, including hyphens |
| `host`, `machine_id_sha256` | Original hostname and saved machine-id digest |
| `instance_id`, `volume_id` | Retained EC2 instance and attached volume identities |
| `producer_commit` | Full independently reviewed producer commit |

The original inventory has GNU SHA-256 lines with a root-name prefix, such as
`exp2-main/main/events.jsonl`. `session_exit_file` omits that prefix. Locate the
main receipt in the pinned inventory; do not create a missing receipt or choose
one from a refused setup attempt. Its original bytes must contain exit code `0`
and an ISO timestamp within B0; its recorded write must follow session completion.
Completion ordering uses the receipt file's exact retained mtime. Whole-second
receipt text is not treated as a subsecond measurement; its timestamp must not
follow the recorded write. The exact B0 mtime cutoff is unchanged.

For the existing Paper 3 recovery, the separately posted inventory digest is
`0650f4ee779b64c8efc32020571657f3009f3fe46a52c8adc9f8ffdf28ea6e16` and the B1 witness
digest is `07ed26743010335e2b1b2b908d28023b7c8ec3ab7ae351589cb1f0aa78eda51c`.
These operational identities are not a closed-capture verdict or a scientific
result. They do not establish that B2 has occurred.

Supply the original `recovery-witness.json` and its nine named files in a
separate recovery-evidence directory. All file hashes/sizes are verified before
semantic checks. The saved B1 statement explicitly limits the observation to
copy-only recovery; its process table is not a complete boot process history.
The only recorded Python process must be the collector parenting `ps`.
B1 retains PID, PPID and command name only. It cannot identify an interpreter
hidden behind a script's command name, distinguish system/session D-Bus, or
establish a complete process history. The saved Paper 3 B1 snapshot contains
`networkd-dispat`, `unattended-upgr`, `dbus-daemon` and `amazon-ssm-agen` under
PID 1; it contains no `ssm-session-worker`. Available B0 monitoring tails are
filtered observations, not a full daemon census. See the operational evidence
in `smoke/doctor56-system-daemons.json`; none of it is a study outcome.

## One bounded capture boot

After approval and merge, arrange cleanup in the same operator session: on both
success and failure stop the instance, retain its volume, restore the idle
alarm's actions, and retain the final state receipts. Do not leave the instance
running while waiting for review or analysis. Do not restart after a refusal to
try a different boot chain; preserve the failure for review.

Within that **same boot**, each before/after process observation may make up to
ten read-only quiet checks, sixty seconds apart, with a 600-second elapsed-time
limit. Every attempt prints its number, elapsed seconds and complete blocking
process list (PID, command name, argv and UID) to the operator log, including the
final failed attempt. A clean pass proceeds immediately. An exhausted check
fails capture and triggers the operator's shutdown cleanup. Malformed evidence,
a changed host/boot or an observation error fails immediately; these are not
transient work. No service is stopped, original record changed, or reboot retried.
Successful captures also retain every snapshot and refusal list in their
before/after evidence; portable replay verifies their ordering, refusals,
same-boot identity and equality of the final snapshot to the capture observation.

Immediately after B2 starts, retain a fresh EC2 observation containing
`InstanceId`, the string `State`, `LaunchTime`, `PrivateDnsName`, and the full
`BlockDeviceMappings`. A useful read-only query is:

```sh
aws ec2 describe-instances --instance-ids "$INSTANCE_ID" \
  --query 'Reservations[0].Instances[0].{InstanceId:InstanceId,State:State.Name,LaunchTime:LaunchTime,PrivateDnsName:PrivateDnsName,PrivateIpAddress:PrivateIpAddress,BlockDeviceMappings:BlockDeviceMappings}' \
  --output json > "$NEW_EC2_OBSERVATION"
```

Transfer the reviewed tool and prepared evidence outside the producer and study
trees. From the original producer working directory, use its original isolated
Python environment with bytecode writes disabled:

```sh
"$CAPTURE_PYTHON" -I -B "$TOOLS/closed_capture.py" \
  --source "$MAIN_SESSION" --repo "$PRODUCER_SRC" \
  --reviewed-commit "$PRODUCER_COMMIT" --out "$NEW_CAPTURE" \
  --reboot-policy "$POLICY_JSON" --reboot-policy-sha256 "$POLICY_SHA256" \
  --recovery-evidence "$RECOVERY_EVIDENCE" --capture-ec2 "$NEW_EC2_OBSERVATION"
```

The collector reads `journalctl --list-boots` in UTC and the full retained B0/B1
JSON journals. Their exact microsecond boundaries, terminal shutdown records,
the saved B1 observation/stop, and both EC2 launch records must agree. It refuses
missing, extra, reused or unobserved boots. A source-file mtime one nanosecond
after B0's exact boundary refuses; no second-resolution allowance is added.

The complete original inventory is rehashed before and after copying, including
setup/preflight evidence outside the registered main/build trees. Those extra
files are copied under `reboot-source/`; existing main/build files are not
duplicated. Symlinks, special files, changed bytes, changed timestamps and held
owner locks refuse. Every tracked spawn needs its original exit receipt. The
full study/recovery reconciliation still decides whether all jobs are resolved;
the reboot proof does not turn an interrupted study into a closed one.

Current process identities and command observations must exclude live evaluator,
session, analysis and other study-capable interpreter workers. Only the exact
current exporter identity, source hash and `-I -B` command are excepted. Process
names use one shared B1/B2 classifier. B2 also checks the basenames of its first
two command arguments, so an interpreter cannot hide behind a script's process
name. Bounded system-daemon exceptions require an exact command name and entire
argv, an integer effective UID in `[0,1000)`, and integer PPID 1. The enumerated
Ubuntu commands are `/usr/bin/python3` with either
`/usr/share/unattended-upgrades/unattended-upgrade-shutdown --wait-for-signal` or
`/usr/bin/networkd-dispatcher --run-startup-triggers`. D-Bus accepts only
`/usr/bin/dbus-daemon --system|--session --address=systemd: --nofork --nopidfile
--systemd-activation --syslog-only` (one exact mode argument). The truncated
`ssm-session-wor` command accepts only `/usr/bin/ssm-session-worker` with **no**
arguments. This does not allow an active SSM session's document/session arguments,
alternate snap paths, or arbitrary user sessions. Unknown variants, extra flags,
different scripts, user UIDs, and lookalike identities remain refusals. These
checks bind process observations, not executable signatures or malicious-root
resistance. Historical captures without retry logs remain readable.

Observations remain time-bound. No environment values are collected. The
Linux `kprobe-optimizer` kernel worker is recognized by its exact name and
kernel parent (PID 2) in B1; B2 additionally requires process group zero and
an empty command line. A similarly named userspace process still refuses.
The machine-id observation records whether the saved B1 digest included the final
newline; both supported encodings identify the same 32 hexadecimal digits.

Failure leaves a visible partial capture and, once copying has begun, a failure
receipt. Never treat it as complete, overwrite it, rewrite the producer journal,
change its boot IDs, or relax runtime equality to force acceptance.

## Offline replay and downstream preparation

Retain the printed capture-manifest digest separately, then copy the capture to
a new local directory and verify every byte. New exports use schema 2 and bind
all five exporter sources, including `reboot_attestation.py`. Ordinary schema-2
same-boot exports and historical schema-1 captures keep the original verifier
path. The reboot mode additionally requires the external policy digest.

Pass `--reboot-policy-sha256 "$POLICY_SHA256"` to both `study_selection.py` and
`refinement_bundle.py`, in addition to their ordinary independent manifest,
producer/tool and mesh-evidence identities. Missing or different policy digests
refuse before selection or bundle output. Selection and bundle metadata retain
the verified policy, inventory and recovery-witness identities and the explicit
reboot-proof scope. Bundle verification checks that scope against its selection.

Offline replay uses preserved source timestamps, not transport-modified archive
mtimes. It rehashes all mapped original files and replays the genuine producer
refusal under the captured B2 process table. Only enumerated B0/B2 identity
mismatches can be supplemented; malformed identities, unfinished spawns, torn
journals, missing generations/builds and all other verifier issues still refuse.

Authentication does not certify optimization performance or final candidates.
Keep the subsequent final-refinement, scientific reporting and manuscript
review/audit gates from the main runbook.

## Regression checks

`test_reboot_attestation.py` mutates boot, cloud, witness, process, timestamp,
receipt and census evidence. `test_reboot_capture.py` exercises export, portable
replay, tool-source binding and pre/post changes. `test_reboot_pipeline.py` runs
the synthetic scientific selection and refinement preparation through both APIs
and CLIs. Its tool identity is a real temporary Git commit of current source,
explicitly synthetic. Historical fixtures execute their pinned exporter bytes.
No test reads the genuine main-study results or launches FEM/optimization.

With the ordinary regenerated mesh cache available, run deliberate in-memory
guard-deletion/replacement probes with:

```sh
python3 -I benchmarks/motor_optimization/final_refinement/reboot_mutations.py \
  --out "$NEW_MUTATION_REPORT"
```

The report retains each unedited test result and source hashes. It is author
regression evidence; an independent Judge still reviews the change and actual
capture evidence separately.
