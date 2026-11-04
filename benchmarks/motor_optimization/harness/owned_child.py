"""One launcher's child lifetime, with exclusive wait4 ownership.

Output goes to a file, so waiting cannot deadlock on undrained pipes. Cooperative
cancellation sends SIGTERM to the owned process group, then uses a bounded SIGKILL
fallback. A forced exit never proves that separately sessioned descendants died.
"""
import os
import signal
import subprocess
import time

import resource_usage
import process_tracking


def wait_run(command, *, log, env, stop=None, journal=None, attempt_id=None,
             grace_seconds=35.0, poll_seconds=.05, child_journal=None):
    if grace_seconds < 0 or poll_seconds <= 0:
        raise ValueError("invalid child cleanup timings")
    if stop is not None and stop() is not None:
        from interruption import Interrupted
        raise Interrupted(stop().signum)
    tracker = process_tracking.Spawn(journal, command, role="run" if child_journal else "direct",
                                      child_journal=child_journal, attempt_id=attempt_id)
    try:
        proc = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT,
                                env=env, start_new_session=True)
    except OSError as error:
        tracker.failed(error)
        raise
    started = time.monotonic()
    receipt = None
    signals = []
    stop_time = None
    failure = None

    def emit(kind, **fields):
        if journal is not None:
            journal.emit(kind, attempt_id=attempt_id, **fields)

    def send(sig):
        try:
            os.killpg(proc.pid, sig)
            result = "sent"
        except ProcessLookupError:
            result = "already_exited"
        except PermissionError:
            # Do not invent proof of termination from macOS's possible EPERM
            # on an exited group. The subsequent reaper establishes exit.
            result = "permission_denied"
        record = {"signal": int(sig), "time": time.time(), "result": result}
        signals.append(record)
        return record

    def reap():
        nonlocal receipt
        receipt = resource_usage.reap_usage(proc.pid)
        if receipt["state"] == "reaped":
            proc.returncode = receipt["returncode"]
        elif receipt["state"] == "unavailable":
            code = proc.poll()  # Unavailable remains explicit; no invented CPU.
            if code is not None:
                receipt.update(state="reaped", returncode=code)
        return receipt["state"] == "reaped"

    try:
        tracker.started(proc.pid)
        emit("owned_process_started", pid=proc.pid, pgid=proc.pid, command=list(command),
             launcher_pid=os.getpid(), started_time=time.time())
        while not reap():
            now = time.monotonic()
            if stop_time is None and stop is not None and stop() is not None:
                stop_time = now
                record = send(signal.SIGTERM)
                emit("owned_process_signal", pid=proc.pid, **record)
            if (stop_time is not None and now - stop_time >= grace_seconds
                    and not any(r["signal"] == signal.SIGKILL for r in signals)):
                record = send(signal.SIGKILL)
                emit("owned_process_signal", pid=proc.pid, **record)
            time.sleep(poll_seconds)
    except BaseException as error:
        failure = error
        # Even journal/storage failure must not abandon this child. Keep cleanup
        # independent of the failed evidence sink and retain its receipt on error.
        send(signal.SIGTERM)
        deadline = time.monotonic() + grace_seconds
        while not reap() and time.monotonic() < deadline:
            time.sleep(poll_seconds)
        if receipt["state"] != "reaped":
            send(signal.SIGKILL)
            receipt = resource_usage.reap_usage(proc.pid, block=True)
            if receipt["state"] == "reaped":
                proc.returncode = receipt["returncode"]
            else:
                receipt.update(state="reaped", returncode=proc.wait())
    receipt.update(wall_seconds=time.monotonic() - started, pgid=proc.pid, signals=signals,
                   forced_kill=any(r["signal"] == signal.SIGKILL for r in signals),
                   descendant_cleanup_verified=False,
                   aggregation="child-specific lifetime cross-check; do not add to run/request CPU")
    try:
        tracker.reaped(receipt)
    except BaseException as error:
        receipt["tracking_error"] = f"{type(error).__name__}: {error}"
        if failure is None:
            failure = error
    if failure is not None:
        failure.launcher_receipt = receipt
        raise failure
    try:
        emit("owned_process_reaped", receipt=receipt)
    except BaseException as error:
        error.launcher_receipt = receipt
        raise
    return receipt


def bind_completeness(receipt, trace):
    """Infer accounting completeness only from the child's retained phase receipts.

    Accounting completeness is not orphan-process identity verification. Recovery
    must separately verify ownership/liveness before launching any replacement.
    """
    phases = [trace.get("resources", {})]
    diagnostic = trace.get("post_run_diagnostic")
    if diagnostic is not None:
        phases.append(diagnostic.get("resources", {}))
    status = trace.get("diagnostic_status")
    terminal = ((status == "not_requested" and diagnostic is None)
                or (status in ("ok", "failed", "unavailable", "no incumbent")
                    and diagnostic is not None and diagnostic.get("status") == status))
    complete = (receipt.get("available") is True and receipt.get("returncode") == 0
                and not receipt.get("forced_kill") and trace.get("optimizer_completed") is True
                and terminal and all(p.get("cpu_complete") is True for p in phases))
    return dict(receipt, cpu_complete=complete,
                completeness_basis="normal child exit and complete retained optimizer/diagnostic phase receipts")
