#!/usr/bin/env bash
# geode-worker.sh — a small AWS worker pool for geode-fem builds and solver runs.
#
# Moves heavy `cargo build` / solver / Meep jobs off the local Mac onto
# tagged EC2 workers that stop themselves when idle. See README.md next to
# this file for the full guide.
#
#   scripts/aws/geode-worker.sh launch    <name> <type>        # e.g. pool1 r8g.4xlarge
#   scripts/aws/geode-worker.sh bootstrap <name>               # toolchain + clone
#   scripts/aws/geode-worker.sh ssh       <name> [cmd ...]     # keyless (Instance Connect)
#   scripts/aws/geode-worker.sh sync-out  <name> <remote> <local>
#   scripts/aws/geode-worker.sh start|stop|terminate <name>
#   scripts/aws/geode-worker.sh list | cost | allow-ip | ami [type]
#
# Identity: AWS_PROFILE defaults to geode-fem-bench (never another project's profile),
# region defaults to us-east-1. Every resource is tagged project=geode-fem,
# which is the only tag the IAM user may start/stop/terminate/connect to.
# shellcheck disable=SC2016  # JMESPath backticks inside single quotes are intended
set -euo pipefail

export AWS_PROFILE="${AWS_PROFILE:-geode-fem-bench}"
export AWS_REGION="${AWS_REGION:-us-east-1}"
export AWS_PAGER=""

SG_NAME="geode-fem-worker"
NAME_PREFIX="geode-worker-"
ALARM_PREFIX="geode-fem-idle-"
ROOT_GB="${GEODE_WORKER_ROOT_GB:-200}"
IDLE_MINUTES="${GEODE_WORKER_IDLE_MINUTES:-30}"
IDLE_CPU_PCT="${GEODE_WORKER_IDLE_CPU_PCT:-5}"
OS_USER=ubuntu
CANONICAL_OWNER=099720109477

# On-demand Linux prices, USD/hour, us-east-1. The IAM user is not allowed
# to call pricing:GetProducts, so these are copied from the public price
# list on the date below; re-check before quoting a budget.
PRICE_DATE="2026-10-12"
price_per_hour() {
  case "$1" in
    r8g.4xlarge)  echo 0.94256 ;;
    r8g.8xlarge)  echo 1.88512 ;;
    r8g.48xlarge) echo 11.31072 ;;
    r6i.4xlarge)  echo 1.008 ;;
    m6i.4xlarge)  echo 0.768 ;;
    g6e.xlarge)   echo 1.861 ;;
    g6e.2xlarge)  echo 2.24208 ;;
    g6e.4xlarge)  echo 3.00424 ;;
    *)            echo "" ;;
  esac
}
# gp3 storage, USD per GB-month (billed whether the instance runs or not).
GP3_GB_MONTH=0.08

die()  { echo "geode-worker: $*" >&2; exit 1; }
info() { echo "geode-worker: $*" >&2; }

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

# ---------------------------------------------------------------- helpers

arch_for_type() {
  # Graviton families end in "g" (r8g, m7g, c7g, ...) or "gd"/"gn".
  case "$1" in
    *[0-9]g.*|*[0-9]gd.*|*[0-9]gn.*) echo arm64 ;;
    *) echo amd64 ;;
  esac
}

# Latest Ubuntu 24.04 (noble) AMI for an architecture. Tries the SSM public
# parameter first; the geode-fem-bench policy does not grant ssm:GetParameters
# today, so it falls back to describe-images on Canonical's owner id, which
# resolves the same image.
resolve_ami() {
  local arch="$1" ami
  ami=$(aws ssm get-parameters \
          --names "/aws/service/canonical/ubuntu/server/24.04/stable/current/${arch}/hvm/ebs-gp3/ami-id" \
          --query 'Parameters[0].Value' --output text 2>/dev/null || true)
  if [[ -n "$ami" && "$ami" != None ]]; then
    echo "$ami"; return
  fi
  info "ssm:GetParameters not permitted; resolving AMI via describe-images (owner ${CANONICAL_OWNER})"
  ami=$(aws ec2 describe-images --owners "$CANONICAL_OWNER" \
          --filters "Name=name,Values=ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-${arch}-server-*" \
                    Name=state,Values=available \
          --query 'sort_by(Images,&CreationDate)[-1].ImageId' --output text)
  [[ -n "$ami" && "$ami" != None ]] || die "could not resolve an Ubuntu 24.04 ${arch} AMI"
  echo "$ami"
}

my_ip() {
  local ip
  ip=$(curl -fsS --max-time 10 https://checkip.amazonaws.com | tr -d '[:space:]')
  [[ "$ip" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "could not determine public IP"
  echo "$ip"
}

default_vpc() {
  aws ec2 describe-vpcs --filters Name=is-default,Values=true \
    --query 'Vpcs[0].VpcId' --output text
}

# Security group geode-fem-worker: ssh only from the operator's current IP.
# Creates it on first use; always (re)adds the current IP.
ensure_sg() {
  local sg vpc
  sg=$(aws ec2 describe-security-groups --filters "Name=group-name,Values=${SG_NAME}" \
         --query 'SecurityGroups[0].GroupId' --output text 2>/dev/null || true)
  if [[ -z "$sg" || "$sg" == None ]]; then
    vpc=$(default_vpc)
    info "creating security group ${SG_NAME} in ${vpc}"
    sg=$(aws ec2 create-security-group --group-name "$SG_NAME" --vpc-id "$vpc" \
           --description "geode-fem worker pool: ssh from operator IP only" \
           --tag-specifications "ResourceType=security-group,Tags=[{Key=project,Value=geode-fem},{Key=Name,Value=${SG_NAME}}]" \
           --query GroupId --output text)
  fi
  allow_ip_on "$sg"
  echo "$sg"
}

allow_ip_on() {
  local sg="$1" ip
  ip=$(my_ip)
  if aws ec2 authorize-security-group-ingress --group-id "$sg" \
       --ip-permissions "IpProtocol=tcp,FromPort=22,ToPort=22,IpRanges=[{CidrIp=${ip}/32,Description=geode-worker operator}]" \
       >/dev/null 2>&1; then
    info "ssh allowed from ${ip}/32 on ${sg}"
  else
    info "ssh rule for ${ip}/32 already present on ${sg}"
  fi
}

# Instance id for a worker name (any state but terminated).
instance_id() {
  local id n
  [[ "$1" =~ ^[a-z0-9-]+$ ]] || die "worker name must be [a-z0-9-] (got '$1')"
  id=$(aws ec2 describe-instances \
         --filters "Name=tag:Name,Values=${NAME_PREFIX}$1" "Name=tag:project,Values=geode-fem" \
                   Name=instance-state-name,Values=pending,running,stopping,stopped \
         --query 'Reservations[].Instances[].InstanceId' --output text)
  [[ -n "$id" && "$id" != None ]] || die "no worker named '$1' (see: $0 list)"
  n=$(wc -w <<<"$id")
  (( n == 1 )) || die "worker name '$1' matches ${n} instances (${id}); refusing to guess"
  echo "$id"
}

instance_field() { # id jmespath
  aws ec2 describe-instances --instance-ids "$1" \
    --query "Reservations[0].Instances[0].$2" --output text
}

put_idle_alarm() {
  local id="$1" periods
  periods=$(( IDLE_MINUTES / 5 )); (( periods >= 1 )) || periods=1
  aws cloudwatch put-metric-alarm \
    --alarm-name "${ALARM_PREFIX}${id}" \
    --alarm-description "geode-fem worker ${id}: stop after ${IDLE_MINUTES} min with CPU < ${IDLE_CPU_PCT}%" \
    --namespace AWS/EC2 --metric-name CPUUtilization \
    --dimensions "Name=InstanceId,Value=${id}" \
    --statistic Average --period 300 --evaluation-periods "$periods" \
    --threshold "$IDLE_CPU_PCT" --comparison-operator LessThanThreshold \
    --treat-missing-data notBreaching \
    --alarm-actions "arn:aws:automate:${AWS_REGION}:ec2:stop" \
    --tags "Key=project,Value=geode-fem"
  info "idle alarm ${ALARM_PREFIX}${id}: stop after ${IDLE_MINUTES} min below ${IDLE_CPU_PCT}% CPU"
}

# Push an ephemeral key through EC2 Instance Connect (valid 60 s) and set
# SSH_OPTS / SSH_TARGET for the caller. The key lives in a temp dir that is
# removed on exit; nothing is written under ~/.ssh.
connect_prep() {
  local name="$1" id ip az
  id=$(instance_id "$name")
  [[ "$(instance_field "$id" 'State.Name')" == running ]] || die "worker '$name' ($id) is not running"
  ip=$(instance_field "$id" PublicIpAddress)
  az=$(instance_field "$id" 'Placement.AvailabilityZone')
  [[ "$ip" != None ]] || die "worker '$name' has no public IP"
  KEYDIR=$(mktemp -d "${TMPDIR:-/tmp}/geode-worker.XXXXXX")
  trap 'rm -rf "$KEYDIR"' EXIT
  ssh-keygen -q -t ed25519 -N '' -C "geode-worker-ephemeral" -f "$KEYDIR/key"
  aws ec2-instance-connect send-ssh-public-key --instance-id "$id" \
    --instance-os-user "$OS_USER" --availability-zone "$az" \
    --ssh-public-key "file://$KEYDIR/key.pub" --query Success --output text >/dev/null
  # Public IPs are reassigned on every stop/start, so host keys cannot be
  # pinned; the endpoint is authenticated by the EC2 API lookup above.
  SSH_OPTS=(-i "$KEYDIR/key" -o IdentitiesOnly=yes -o StrictHostKeyChecking=no
            -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ServerAliveInterval=30)
  SSH_TARGET="${OS_USER}@${ip}"
}

wait_for_ssh() { # uses SSH_OPTS/SSH_TARGET
  for _ in $(seq 1 30); do
    if ssh "${SSH_OPTS[@]}" -o ConnectTimeout=5 "$SSH_TARGET" true 2>/dev/null; then return 0; fi
    sleep 5
  done
  die "ssh to ${SSH_TARGET} did not come up"
}

hours_since() { # ISO-8601 timestamp -> decimal hours
  python3 - "$1" <<'PY'
import sys, datetime
t = datetime.datetime.fromisoformat(sys.argv[1].replace("Z", "+00:00"))
now = datetime.datetime.now(datetime.timezone.utc)
print(f"{(now - t).total_seconds() / 3600:.3f}")
PY
}

# ---------------------------------------------------------------- commands

cmd_ami() {
  local type="${1:-r8g.4xlarge}"
  resolve_ami "$(arch_for_type "$type")"
}

cmd_allow_ip() {
  ensure_sg >/dev/null
}

cmd_launch() {
  local name="${1:-}" type="${2:-}"
  [[ -n "$name" && -n "$type" ]] || die "usage: launch <name> <type>"
  [[ "$name" =~ ^[a-z0-9-]+$ ]] || die "name must be [a-z0-9-]"
  if aws ec2 describe-instances \
       --filters "Name=tag:Name,Values=${NAME_PREFIX}${name}" Name=instance-state-name,Values=pending,running,stopping,stopped \
       --query 'Reservations[].Instances[].InstanceId' --output text | grep -q .; then
    die "a worker named '$name' already exists (see: $0 list)"
  fi
  local arch ami sg root_dev id
  arch=$(arch_for_type "$type")
  ami=$(resolve_ami "$arch")
  sg=$(ensure_sg)
  root_dev=$(aws ec2 describe-images --image-ids "$ami" --query 'Images[0].RootDeviceName' --output text)
  info "launching ${NAME_PREFIX}${name}: ${type} (${arch}) ami=${ami} root=${ROOT_GB}G gp3 sg=${sg} region=${AWS_REGION}"
  id=$(aws ec2 run-instances \
         --image-id "$ami" --instance-type "$type" --count 1 \
         --security-group-ids "$sg" \
         --instance-initiated-shutdown-behavior stop \
         --metadata-options HttpTokens=required,HttpEndpoint=enabled \
         --block-device-mappings "[{\"DeviceName\":\"${root_dev}\",\"Ebs\":{\"VolumeSize\":${ROOT_GB},\"VolumeType\":\"gp3\",\"DeleteOnTermination\":true}}]" \
         --tag-specifications \
           "ResourceType=instance,Tags=[{Key=project,Value=geode-fem},{Key=Name,Value=${NAME_PREFIX}${name}},{Key=geode-worker,Value=${name}}]" \
           "ResourceType=volume,Tags=[{Key=project,Value=geode-fem},{Key=Name,Value=${NAME_PREFIX}${name}}]" \
         --query 'Instances[0].InstanceId' --output text)
  info "instance ${id} launched; waiting for running"
  aws ec2 wait instance-running --instance-ids "$id"
  put_idle_alarm "$id"
  echo "${id} $(instance_field "$id" PublicIpAddress)"
}

cmd_ssh() {
  local name="${1:-}"; [[ -n "$name" ]] || die "usage: ssh <name> [cmd ...]"
  shift
  connect_prep "$name"
  if (( $# )); then
    # Run under a login shell so ~/.profile applies (cargo on PATH,
    # CARGO_INCREMENTAL=0); Ubuntu's ~/.bashrc returns early when
    # non-interactive, so a bare remote command would not see them.
    local remote_cmd; remote_cmd=$(printf '%q' "$*")
    # Plain ssh, not exec: exec would skip the EXIT trap that deletes the
    # ephemeral key. Client-side expansion is intended (already %q-quoted).
    # shellcheck disable=SC2029
    ssh "${SSH_OPTS[@]}" "$SSH_TARGET" "bash -lc $remote_cmd" || exit $?
  else
    ssh "${SSH_OPTS[@]}" -t "$SSH_TARGET" || exit $?
  fi
}

cmd_bootstrap() {
  local name="${1:-}"; [[ -n "$name" ]] || die "usage: bootstrap <name>"
  connect_prep "$name"
  wait_for_ssh
  info "bootstrapping ${name} (${SSH_TARGET})"
  ssh "${SSH_OPTS[@]}" "$SSH_TARGET" 'bash -s' <<'REMOTE'
# shellcheck disable=SC2016  # JMESPath backticks inside single quotes are intended
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
LOG=$HOME/geode-worker-bootstrap.txt
ARCH=$(uname -m)
echo "bootstrap started $(date -u +%FT%TZ) on $(hostname) ($ARCH)" | tee "$LOG"

# Wait for cloud-init / unattended-upgrades to release the apt lock.
sudo cloud-init status --wait >/dev/null 2>&1 || true
for _ in $(seq 1 60); do
  if ! sudo fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1; then break; fi
  sleep 5
done

sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends \
  build-essential pkg-config cmake git curl ca-certificates rsync htop \
  libarpack2-dev libopenblas-dev \
  python3 python3-venv python3-pip python3-numpy python3-matplotlib \
  docker.io
sudo apt-get install -y -q --no-install-recommends gmsh || echo "apt gmsh failed" | tee -a "$LOG"
sudo usermod -aG docker "$USER" || true
sudo systemctl enable --now docker >/dev/null 2>&1 || true

# Gmsh: geode fixtures are generated with gmsh 4.15.x. The official 4.15
# Linux SDK is x86-64 only, so x86 workers get the SDK under /opt/gmsh and
# arm64 workers keep the apt build (4.12.x on Ubuntu 24.04) — do not
# regenerate committed fixtures on an arm64 worker.
if [ "$ARCH" = x86_64 ] && [ ! -x /opt/gmsh/bin/gmsh ]; then
  GMSH_VER=4.15.2
  tmp=$(mktemp -d)
  if curl -fsSL "https://gmsh.info/bin/Linux/gmsh-${GMSH_VER}-Linux64-sdk.tgz" -o "$tmp/gmsh.tgz"; then
    sudo mkdir -p /opt/gmsh && sudo tar -xzf "$tmp/gmsh.tgz" -C /opt/gmsh --strip-components=1
    sudo ln -sf /opt/gmsh/bin/gmsh /usr/local/bin/gmsh
    echo "gmsh SDK ${GMSH_VER} installed to /opt/gmsh" | tee -a "$LOG"
  else
    echo "gmsh SDK download failed; using apt gmsh" | tee -a "$LOG"
  fi
  rm -rf "$tmp"
fi

# Rust: rustup stable, CARGO_INCREMENTAL=0 (see memory: shared target dirs
# plus incremental caches bloat; CI builds the same way).
if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --component clippy,rustfmt
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
rustup update stable >/dev/null
for rc in "$HOME/.profile" "$HOME/.bashrc"; do
  grep -q 'CARGO_INCREMENTAL=0' "$rc" 2>/dev/null || printf '\nexport CARGO_INCREMENTAL=0\n' >> "$rc"
  grep -q '.cargo/env' "$rc" 2>/dev/null || printf '. "$HOME/.cargo/env"\n' >> "$rc"
done

if [ ! -d "$HOME/geode-fem/.git" ]; then
  git clone --quiet https://github.com/rjwalters/geode-fem "$HOME/geode-fem"
else
  git -C "$HOME/geode-fem" fetch --quiet origin
fi

{
  echo "rustc:  $(rustc --version)"
  echo "cargo:  $(cargo --version)"
  echo "gmsh:   $(gmsh --version 2>&1 | tail -1)  ($(command -v gmsh))"
  echo "python: $(python3 --version)"
  echo "numpy:  $(python3 -c 'import numpy; print(numpy.__version__)')"
  echo "mpl:    $(python3 -c 'import matplotlib; print(matplotlib.__version__)')"
  echo "docker: $(docker --version)"
  echo "arpack: $(pkg-config --modversion arpack 2>/dev/null || echo 'pkg-config: not found')"
  echo "git:    $(git --version); repo at $(git -C "$HOME/geode-fem" rev-parse --short HEAD)"
  echo "disk:   $(df -h / | awk 'NR==2{print $2" total, "$4" free"}')"
  echo "cpus:   $(nproc), mem: $(free -g | awk '/Mem/{print $2}') GiB"
  echo "bootstrap finished $(date -u +%FT%TZ)"
} | tee -a "$LOG"
REMOTE
}

cmd_sync_out() {
  local name="${1:-}" remote="${2:-}" local_path="${3:-}"
  [[ -n "$name" && -n "$remote" && -n "$local_path" ]] || die "usage: sync-out <name> <remote-path> <local-path>"
  connect_prep "$name"
  mkdir -p "$local_path"
  rsync -az -e "ssh ${SSH_OPTS[*]}" "${SSH_TARGET}:${remote}" "$local_path"
  info "synced ${name}:${remote} -> ${local_path}"
}

cmd_start() {
  local id; id=$(instance_id "${1:-}")
  aws ec2 start-instances --instance-ids "$id" --query 'StartingInstances[0].CurrentState.Name' --output text
  aws ec2 wait instance-running --instance-ids "$id"
  ensure_sg >/dev/null   # refresh the operator IP rule while we are here
  echo "${id} $(instance_field "$id" PublicIpAddress)"
}

cmd_stop() {
  local id; id=$(instance_id "${1:-}")
  aws ec2 stop-instances --instance-ids "$id" --query 'StoppingInstances[0].CurrentState.Name' --output text
  aws ec2 wait instance-stopped --instance-ids "$id"
  info "${id} stopped (root volume kept; billed at gp3 storage rate only)"
}

cmd_terminate() {
  local id; id=$(instance_id "${1:-}")
  aws ec2 terminate-instances --instance-ids "$id" --query 'TerminatingInstances[0].CurrentState.Name' --output text
  aws cloudwatch delete-alarms --alarm-names "${ALARM_PREFIX}${id}" || info "could not delete alarm ${ALARM_PREFIX}${id}"
  aws ec2 wait instance-terminated --instance-ids "$id"
  info "${id} terminated"
}

cmd_list() {
  printf '%-22s %-20s %-14s %-10s %-16s %-12s %s\n' NAME ID TYPE STATE PUBLIC-IP AZ LAUNCHED
  aws ec2 describe-instances --filters "Name=tag:project,Values=geode-fem" \
    --query 'Reservations[].Instances[].[Tags[?Key==`Name`]|[0].Value,InstanceId,InstanceType,State.Name,PublicIpAddress,Placement.AvailabilityZone,LaunchTime]' \
    --output text | sort \
    | while read -r name id type state ip az launched; do
        printf '%-22s %-20s %-14s %-10s %-16s %-12s %s\n' "$name" "$id" "$type" "$state" "$ip" "$az" "$launched"
      done
}

cmd_cost() {
  local rows total=0 id type name launched hrs price cost vol_gb
  rows=$(aws ec2 describe-instances --filters "Name=tag:project,Values=geode-fem" \
           Name=instance-state-name,Values=running \
           --query 'Reservations[].Instances[].[InstanceId,InstanceType,Tags[?Key==`Name`]|[0].Value,LaunchTime]' \
           --output text)
  echo "ESTIMATE ONLY, not a bill: on-demand Linux us-east-1 prices as of ${PRICE_DATE} (pricing API not permitted for this user)."
  printf '%-22s %-20s %-14s %8s %10s %10s\n' NAME ID TYPE HOURS USD/H USD
  if [[ -n "$rows" ]]; then
    while read -r id type name launched; do
      hrs=$(hours_since "$launched")
      price=$(price_per_hour "$type")
      if [[ -z "$price" ]]; then printf '%-22s %-20s %-14s %8s %10s %10s\n' "$name" "$id" "$type" "$hrs" "?" "?"; continue; fi
      cost=$(python3 -c "print(f'{$hrs*$price:.2f}')")
      total=$(python3 -c "print($total+$hrs*$price)")
      printf '%-22s %-20s %-14s %8s %10s %10s\n' "$name" "$id" "$type" "$hrs" "$price" "$cost"
    done <<< "$rows"
  fi
  vol_gb=$(aws ec2 describe-volumes --filters "Name=tag:project,Values=geode-fem" \
             --query 'sum(Volumes[].Size)' --output text 2>/dev/null || echo 0)
  printf 'running-instance spend this uptime: $%.2f\n' "$total"
  python3 -c "print(f'tagged gp3 storage: ${vol_gb} GB -> \${${vol_gb}*${GP3_GB_MONTH}:.2f}/month (\${${vol_gb}*${GP3_GB_MONTH}/730:.4f}/h, stopped or not)')"
  echo "note: uptime is since the last start; earlier sessions are not included."
}

# ---------------------------------------------------------------- dispatch

cmd="${1:-}"; shift || true
case "$cmd" in
  launch)     cmd_launch "$@" ;;
  ssh)        cmd_ssh "$@" ;;
  bootstrap)  cmd_bootstrap "$@" ;;
  sync-out)   cmd_sync_out "$@" ;;
  start)      cmd_start "$@" ;;
  stop)       cmd_stop "$@" ;;
  terminate)  cmd_terminate "$@" ;;
  list)       cmd_list ;;
  cost)       cmd_cost ;;
  allow-ip)   cmd_allow_ip ;;
  ami)        cmd_ami "$@" ;;
  -h|--help|help|"") usage 0 ;;
  *)          echo "unknown command: $cmd" >&2; usage 2 ;;
esac
