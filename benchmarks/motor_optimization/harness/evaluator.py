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

import resource_usage
import interruption
import process_tracking


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
    def __init__(self, argv, env=None, stderr_path=None, timeout=None, stop=None, process_journal=None):
        self.stop = stop
        interruption.check(stop)
        self.argv = list(argv)
        self.proc = None
        self._err = None
        self.receipt = None
        self.request_resources = []
        self._shutdown_ack = False
        self._buf = b""
        self._id = 0
        self.tracker = None
        try:
            self._err = open(stderr_path, "ab") if stderr_path else subprocess.DEVNULL
            self.tracker = process_tracking.Spawn(process_journal, self.argv, role="evaluator")
            try:
                self.proc = subprocess.Popen(self.argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                             stderr=self._err, env=env, start_new_session=True)
            except OSError as error:
                self.tracker.failed(error)
                raise
            self.tracker.started(self.proc.pid)
            self.ready = self._read(timeout)
            self._record_resources("startup", self.ready)
            if self.ready.get("op") != "ready" or not self.ready.get("ok"):
                raise EvaluatorFailure("start", self.ready.get("error", self.ready))
        except BaseException as exc:
            # Construction may fail before the caller receives this object.
            # Always reap our child and close our handles, retaining evidence
            # on the exception for run/startup accounting.
            try:
                self.kill()
            except BaseException as cleanup_error:
                exc.cleanup_error = f"{type(cleanup_error).__name__}: {cleanup_error}"
            finally:
                self._close_streams()
            exc.evaluator_resources = self.resources()
            raise

    def _record_resources(self, op, reply):
        self.request_resources.append({
            "op": op, "id": reply.get("id"),
            "measurement": reply.get("resources", {
                "available": False, "reason": "evaluator supplied no resource measurement"}),
        })

    def resources(self):
        """Child-specific evidence, never shared-launcher RUSAGE_CHILDREN."""
        return {"requests": self.request_resources, "child_receipt": self.receipt,
                "aggregation": "request totals and child lifetime receipt overlap; do not sum"}

    def _reap(self, *, block=False):
        if self.proc is None:
            return None
        if self.receipt is not None and self.receipt["state"] == "reaped":
            return self.receipt
        receipt = resource_usage.reap_usage(
            self.proc.pid, block=block, descendants_complete=self._shutdown_ack)
        if receipt["state"] == "reaped":
            self.proc.returncode = receipt["returncode"]
            # A nonzero normal exit still fails the clean-shutdown contract.
            receipt["cpu_complete"] &= receipt["returncode"] == 0
        elif receipt["state"] == "unavailable":
            # Unsupported OS, or a receipt lost to an outside reaper: retain
            # explicit unavailability while still completing process cleanup.
            code = self.proc.wait() if block else self.proc.poll()
            if code is not None:
                receipt.update(state="reaped", returncode=code)
        self.receipt = receipt
        try:
            self.tracker.reaped(receipt)
        except BaseException as error:
            receipt["tracking_error"] = f"{type(error).__name__}: {error}"
            raise
        return receipt

    def _close_streams(self):
        if self.proc is not None:
            for stream in (self.proc.stdin, self.proc.stdout):
                if stream is not None and not stream.closed:
                    try:
                        stream.close()
                    except (BrokenPipeError, OSError):
                        pass
        if self._err not in (None, subprocess.DEVNULL) and not self._err.closed:
            self._err.close()

    def request(self, op, timeout=None, **fields):
        interruption.check(self.stop)
        self._id += 1
        msg = dict(fields, op=op, id=self._id)
        try:
            try:
                self.proc.stdin.write((json.dumps(msg) + "\n").encode())
                self.proc.stdin.flush()
            except BrokenPipeError as e:
                raise EvaluatorFailure(op, f"evaluator gone: {e}") from e
            reply = self._read(timeout)
        except BaseException:
            self.request_resources.append({
                "op": op, "id": self._id,
                "measurement": {"available": False,
                                "reason": "request did not return resource measurements"}})
            raise
        self._record_resources(op, reply)
        if reply.get("id") != self._id:
            raise EvaluatorFailure(op, f"reply id {reply.get('id')} != {self._id}")
        return reply

    def _read(self, timeout):
        deadline = None if timeout is None else time.monotonic() + max(timeout, 0.0)
        fd = self.proc.stdout.fileno()
        while b"\n" not in self._buf:
            try:
                interruption.check(self.stop)
            except interruption.Interrupted:
                self.kill()
                raise
            wait = None if deadline is None else deadline - time.monotonic()
            if wait is not None and wait <= 0:
                self.kill()
                raise Aborted()
            if self.stop is not None:
                wait = min(wait, .1) if wait is not None else .1
            ready, _, _ = select.select([fd], [], [], wait)
            if not ready:
                continue
            chunk = os.read(fd, 1 << 16)
            if not chunk:
                self.kill()
                raise EvaluatorFailure("read", f"evaluator exited ({self.proc.returncode})")
            self._buf += chunk
        line, self._buf = self._buf.split(b"\n", 1)
        reply = json.loads(line)
        if not isinstance(reply, dict):
            raise EvaluatorFailure("read", "reply is not a JSON object")
        return reply

    def kill(self):
        """Kill the evaluator and everything it started (a gmsh remesh in
        flight runs in a child process): its whole process group."""
        if self.proc is None:
            return
        receipt = self._reap()
        if receipt["state"] == "reaped":
            return
        try:
            os.killpg(self.proc.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            # Already gone (macOS reports EPERM for the group of a process
            # that has exited but not been reaped).
            pass
        self._reap(block=True)

    def close(self):
        # A requested stop kills a busy evaluator immediately. Disable callback
        # checks during cleanup so they cannot interrupt our sole reaper.
        stopping = self.stop is not None and self.stop() is not None
        self.stop = None
        try:
            if stopping:
                self.kill()
            if self.proc is not None and self._reap()["state"] != "reaped":
                try:
                    reply = self.request("shutdown", timeout=30)
                    self._shutdown_ack = reply.get("op") == "shutdown" and reply.get("ok") is True
                    self.proc.stdin.close()
                    # Give a successful synchronous server time to exit so
                    # wait4 retains a complete receipt, not a forced kill.
                    deadline = time.monotonic() + 0.25
                    while self._reap()["state"] != "reaped" and time.monotonic() < deadline:
                        time.sleep(0.001)
                except (Aborted, EvaluatorFailure, ValueError, OSError):
                    pass
        finally:
            try:
                self.kill()
            finally:
                self._close_streams()
