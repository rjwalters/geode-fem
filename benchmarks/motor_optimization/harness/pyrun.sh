#!/usr/bin/env bash
# Run a harness script under the locked Python environment (issue #1195):
#
#   benchmarks/motor_optimization/harness/pyrun.sh SCRIPT.py [ARGS...]
#
# The environment is a uv virtualenv (CPython 3.12) with exactly the
# hash-pinned requirements.lock (scipy 1.18.1, numpy 2.5.4), at
# $MOTOR_HARNESS_VENV (default harness/.venv, gitignored). It is rebuilt when
# the lock changes. The interpreter always runs with -I.
set -euo pipefail
H="$(cd "$(dirname "$0")" && pwd)"
VENV="${MOTOR_HARNESS_VENV:-$H/.venv}"
LOCK="$H/requirements.lock"
if command -v sha256sum >/dev/null; then
    LOCK_SHA="$(sha256sum "$LOCK" | cut -d' ' -f1)"
else
    LOCK_SHA="$(shasum -a 256 "$LOCK" | cut -d' ' -f1)"
fi
if [[ ! -x "$VENV/bin/python" || "$(cat "$VENV/.lock-sha256" 2>/dev/null)" != "$LOCK_SHA" ]]; then
    uv venv -q --clear --python 3.12 "$VENV"
    uv pip install -q --python "$VENV/bin/python" --require-hashes --no-deps -r "$LOCK"
    echo "$LOCK_SHA" >"$VENV/.lock-sha256"
fi
export MOTOR_HARNESS_LOCK_SHA256="$LOCK_SHA"
exec "$VENV/bin/python" -I "$@"
