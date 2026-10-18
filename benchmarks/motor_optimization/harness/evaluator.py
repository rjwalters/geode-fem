"""Client of the JSON-lines evaluator process (`icem_harness_eval`,
crates/geode-core/examples/icem_harness_eval.rs; protocol in
crates/geode-core/tests/common/icem_harness.rs).

One process per optimizer run. `request(op, timeout=...)` writes one
request line and waits for its reply line; when the timeout (the run's
remaining budget) passes first, the process is killed and `Aborted` is
raised, so no request outlives the budget. The process runs in its own
session, and a kill takes its whole process group (gmsh included).
"""
import json
import os
import select
import signal
import struct
import subprocess
import time


def to_bits(x):
    """IEEE-754 bit patterns of floats, as the `x_bits` of a request (exact
    through JSON; the Rust side's decimal parse is not always correctly
    rounded)."""
    return [struct.unpack("<Q", struct.pack("<d", float(v)))[0] for v in x]


def from_bits(b):
    return [struct.unpack("<d", struct.pack("<Q", int(v)))[0] for v in b]


class Aborted(Exception):
    """The reply did not arrive before the deadline; the process is gone."""


class EvaluatorFailure(Exception):
    def __init__(self, op, error):
        super().__init__(f"{op}: {error}")
        self.op = op
        self.error = error


class Evaluator:
    def __init__(self, argv, env=None, stderr_path=None, timeout=None):
        self.argv = list(argv)
        self._err = open(stderr_path, "ab") if stderr_path else subprocess.DEVNULL
        self.proc = subprocess.Popen(self.argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=self._err, env=env, start_new_session=True)
        self._buf = b""
        self._id = 0
        self.ready = self._read(timeout)
        if self.ready.get("op") != "ready" or not self.ready.get("ok"):
            self.kill()
            raise EvaluatorFailure("start", self.ready.get("error", self.ready))

    def request(self, op, timeout=None, **fields):
        self._id += 1
        msg = dict(fields, op=op, id=self._id)
        try:
            self.proc.stdin.write((json.dumps(msg) + "\n").encode())
            self.proc.stdin.flush()
        except BrokenPipeError as e:
            raise EvaluatorFailure(op, f"evaluator gone: {e}") from e
        reply = self._read(timeout)
        if reply.get("id") != self._id:
            raise EvaluatorFailure(op, f"reply id {reply.get('id')} != {self._id}")
        return reply

    def _read(self, timeout):
        deadline = None if timeout is None else time.monotonic() + max(timeout, 0.0)
        fd = self.proc.stdout.fileno()
        while b"\n" not in self._buf:
            wait = None if deadline is None else deadline - time.monotonic()
            if wait is not None and wait <= 0:
                self.kill()
                raise Aborted()
            ready, _, _ = select.select([fd], [], [], wait)
            if not ready:
                continue
            chunk = os.read(fd, 1 << 16)
            if not chunk:
                self.kill()
                raise EvaluatorFailure("read", f"evaluator exited ({self.proc.poll()})")
            self._buf += chunk
        line, self._buf = self._buf.split(b"\n", 1)
        return json.loads(line)

    def kill(self):
        """Kill the evaluator and everything it started (a gmsh remesh in
        flight runs in a child process): its whole process group."""
        try:
            os.killpg(self.proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        self.proc.wait()

    def close(self):
        if self.proc.poll() is None:
            try:
                self.request("shutdown", timeout=30)
            except (Aborted, EvaluatorFailure):
                pass
        self.kill()
        if self._err not in (None, subprocess.DEVNULL):
            self._err.close()
