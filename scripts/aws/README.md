# scripts/aws — the geode-fem worker pool

`geode-worker.sh` runs heavy `cargo` builds, solver sweeps and (later) Meep
jobs on tagged EC2 workers instead of the local Mac. Workers are plain
Ubuntu 24.04 boxes that stop themselves after 30 min of idle CPU; a stopped
worker keeps its root volume, so its `target/` directory survives between
sessions and the next build is incremental-by-artifact (not by
`CARGO_INCREMENTAL`, which is off).

Operator approval 2026-10-12: a pool of 2–3 `r8g.4xlarge` (16 vCPU Graviton4,
128 GiB) that stop automatically, plus `r8g.8xlarge` / `r8g.48xlarge` for
Meep (#1088).

## Identity

| Setting | Value |
|---|---|
| Profile | `AWS_PROFILE=geode-fem-bench` (the script defaults to it; never `botho-deploy`) |
| Region | `us-east-1` (`AWS_REGION`; change it only if capacity forces you to, and say so) |
| Tag | `project=geode-fem` on every instance, volume and security group — the IAM user can only start/stop/terminate/connect to instances carrying it |
| Security group | `geode-fem-worker`, ssh (tcp/22) from the operator's current public IP only |
| Alarms | `geode-fem-idle-<instance-id>`, the only alarm names the user may manage |
| Access | EC2 Instance Connect with a per-invocation ephemeral key; no key pairs, nothing written under `~/.ssh` |

Known permission gaps of `geode-fem-bench` (the script works around them):

- `ssm:GetParameters` is denied, so the Ubuntu AMI is resolved with
  `describe-images` on Canonical's owner id (`099720109477`) instead of the
  SSM public parameter. Same image, different lookup.
- `pricing:GetProducts` is denied, so `cost` uses a price table copied from
  the public on-demand price list on the date recorded in the script
  (`PRICE_DATE`). Update it when adding an instance type.
- `cloudwatch:DescribeAlarms` is denied; `put-metric-alarm` and
  `delete-alarms` work, so the idle alarm is created but cannot be listed
  from this profile. Check it in the console if in doubt.

## Commands

```sh
W=scripts/aws/geode-worker.sh

$W launch pool1 r8g.4xlarge        # Ubuntu 24.04 arm64, 200 GB gp3, tags, idle alarm
$W bootstrap pool1                 # rustup stable, build deps, gmsh, python, docker, clone
$W ssh pool1                       # interactive shell (Instance Connect)
$W ssh pool1 'cd geode-fem && cargo build --release -p geode-core --examples --features arpack'
$W sync-out pool1 geode-fem/results/ ~/geode-results/pool1/   # rsync back, keep a local copy
$W stop pool1                      # keeps the volume (and target/) — the normal way to finish
$W start pool1                     # new public IP each time; the ssh rule is refreshed
$W terminate pool1                 # deletes the volume and the idle alarm
$W list                            # every project=geode-fem instance
$W cost                            # spend estimate from uptime x on-demand price
$W allow-ip                        # re-open ssh from the IP you are on now
$W ami r8g.4xlarge                 # which AMI launch would use
```

Tunables (environment): `GEODE_WORKER_ROOT_GB` (200), `GEODE_WORKER_IDLE_MINUTES`
(30), `GEODE_WORKER_IDLE_CPU_PCT` (5).

### Working pattern

1. `start` (or `launch` + `bootstrap` once), then `ssh` into `~/geode-fem`
   and `git fetch && git checkout <branch>`.
2. Build and run with `--release`. The box has 16 cores; `cargo` uses them
   all. `CARGO_INCREMENTAL=0` is set in `~/.profile`.
3. Anything worth keeping goes back with `sync-out` **in the same session**
   (never leave a result only on a cloud box), then `stop`.
4. If you forget, the idle alarm stops the box after ~30 min below 5 % CPU.
   A stopped box costs only its gp3 storage (200 GB ≈ $16/month); terminate
   workers you will not reuse.

`ssh <name> '<cmd>'` runs the command under `bash -lc`, so `~/.profile`
applies: `cargo` is on `PATH` and `CARGO_INCREMENTAL=0` is set, exactly as in
an interactive session. Quote the whole remote command as one argument.

## What `bootstrap` installs

Ubuntu 24.04 (noble) via apt: `build-essential`, `pkg-config`, `cmake`, `git`,
`curl`, `rsync`, `libarpack2-dev` (for `--features arpack`), `libopenblas-dev`,
`python3` + `numpy` + `matplotlib` + `venv`, `docker.io`, `gmsh`.
Rust via rustup (stable, `clippy`, `rustfmt`). The public repo is cloned to
`~/geode-fem`. Versions are written to `~/geode-worker-bootstrap.txt`.

**Gmsh version:** geode's committed fixtures are generated with gmsh 4.15.2.
The official 4.15 Linux SDK and the PyPI wheel are x86-64 only, so on
Graviton (arm64) workers bootstrap keeps the apt build, **gmsh 4.12.1**, and
on x86 workers (`r6i`, `m6i`, `g6e`) it installs the 4.15.2 SDK under
`/opt/gmsh`. Regenerate committed fixtures on the Mac or an x86 worker, not
on arm64; solver runs that only read `.msh` files are unaffected.

## Validation record (2026-10-12, `pool1`, r8g.4xlarge, us-east-1d)

AMI `ami-0e1ab5c876cc030e8` (`ubuntu-noble-24.04-arm64-server-20261004`),
rustc 1.99.0, apt gmsh 4.12.1, libarpack 3.9.1, repo at `eb3f23d1`.

| Step | Wall time |
|---|---|
| `launch` (run-instances to running + alarm) | ~40 s |
| `bootstrap` (apt, rustup, clone) | 73 s |
| `cargo build --release -p geode-core --examples --features arpack` (cold) | 155.7 s, maxrss 1.6 GB |
| `cargo test --release -p geode-core --test patch_conformal_2g4_fixture` | 55.3 s (rebuild without `arpack`; the 7 tests run in 0.03 s) |

Nothing needed patching for Linux aarch64: `faer` and the `arpack` FFI
(against the apt `libarpack2-dev`) built clean. `rsync` on macOS is 2.6.9,
so `sync-out` uses only flags that version knows.

## Prices (on-demand Linux, us-east-1, 2026-10-12)

| Type | vCPU | GiB | USD/h |
|---|---|---|---|
| r8g.4xlarge | 16 | 128 | 0.94256 |
| r8g.8xlarge | 32 | 256 | 1.88512 |
| r8g.48xlarge | 192 | 1536 | 11.31072 |
| r6i.4xlarge | 16 | 128 | 1.008 |
| m6i.4xlarge | 16 | 64 | 0.768 |
| g6e.xlarge | 4 | 32 | 1.861 |
| g6e.2xlarge | 8 | 64 | 2.24208 |
| g6e.4xlarge | 16 | 128 | 3.00424 |

gp3 storage: $0.08 per GB-month. The account's on-demand Standard-family
vCPU quota is 300 (an `r8g.48xlarge` is 192 of it).
