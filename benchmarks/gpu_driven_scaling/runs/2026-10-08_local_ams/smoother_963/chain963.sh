#!/usr/bin/env bash
# Issue #963: the driven AMS iteration-growth investigation, as run.
# One ams_cpu_sweep.sh tree per (build, variant, omega); AMS legs only,
# single-threaded, sizes n = 6 9 12 15 20 24 (1 854 -> 102 024 edges).
# usage: chain963.sh <main-checkout> <branch-checkout>
#   CARGO_TARGET_DIR_MAIN / CARGO_TARGET_DIR_BRANCH: the two target dirs.
# Writes trees next to this script; chain.out has one "<tree> rc=<rc>" line each.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
MAIN=$1 BRANCH=$2
SIZES="6 9 12 15 20 24"
export RAYON_NUM_THREADS=1 LEGS=ams SKIP_DIRECT=1
run() {  # run <tree> <checkout> <target> [VAR=value ...]
  local tree=$1 dir=$2 tgt=$3; shift 3
  env GEODE_DIR="$dir" CARGO_TARGET_DIR="$tgt" "$@" \
    "$BRANCH/benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh" "$here/$tree" $SIZES > /dev/null
  echo "$tree rc=$?" >> "$here/chain.out"
}
B() { run "$1" "$BRANCH" "$CARGO_TARGET_DIR_BRANCH" "${@:2}"; }
for w in 0p10 0p05 0p20; do
  om=${w/p/.}
  run "main_w$w" "$MAIN" "$CARGO_TARGET_DIR_MAIN" GEODE_SCALING_OMEGA=$om
  B "default_w$w"         GEODE_SCALING_OMEGA=$om
  B "pi_direct_w$w"       GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_PI_COARSE=direct
  B "pi_sgs16_w$w"        GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_PI_COARSE=sgs:16
  B "pi_amg4_w$w"         GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_PI_COARSE=amg:4
  if [ "$w" = 0p10 ]; then
    B "pi_sgs8_w$w"         GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_PI_COARSE=sgs:8
    B "pi_amg2_w$w"         GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_PI_COARSE=amg:2
    B "pi_amg6_w$w"         GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_PI_COARSE=amg:6
    B "sm_jacobi2_w$w"      GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_SMOOTHER=jacobi:2
    B "sm_l1jacobi_w$w"     GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_SMOOTHER=l1jacobi:1
    B "sm_chebyshev2_w$w"   GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_SMOOTHER=chebyshev:2
    B "sm_chebyshev3_w$w"   GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_SMOOTHER=chebyshev:3
    B "sm_sgs1_w$w"         GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_SMOOTHER=sgs:1
    B "cyc_mult_w$w"        GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_CYCLE=multiplicative
    B "cyc_mult_pi_direct_w$w" GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_CYCLE=multiplicative GEODE_DRIVEN_AMS_PI_COARSE=direct
    B "cyc_mult_pi_amg4_w$w"   GEODE_SCALING_OMEGA=$om GEODE_DRIVEN_AMS_CYCLE=multiplicative GEODE_DRIVEN_AMS_PI_COARSE=amg:4
  fi
done
echo CHAIN_DONE >> "$here/chain.out"
