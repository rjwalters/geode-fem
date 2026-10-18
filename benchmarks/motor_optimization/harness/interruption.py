"""Cooperative stop signals for an isolated run, not permission to retry it.

Raise only in declared compute phases. Evidence writes and child cleanup run
outside those phases; a signal there records a pending stop without tearing an
atomic snapshot or interrupting the sole reaper. The first signal is retained.
"""
from contextlib import contextmanager, nullcontext
import signal


class Interrupted(BaseException):
    def __init__(self, signum):
        self.signum = int(signum)
        super().__init__(f"received {signal.Signals(signum).name}")


def evidence(error):
    signum = getattr(error, "signum", signal.SIGINT)
    return {"signal": int(signum), "signal_name": signal.Signals(signum).name,
            "verified_external_interruption": False,
            "reason": "signal observed; operator/host evidence and process verification still required"}


class StopSignals:
    def __init__(self):
        self.requested = None
        self.active = False
        self.previous = {}

    def _receive(self, signum, frame):
        first = self.requested is None
        if first:
            self.requested = Interrupted(signum)
        if first and self.active:
            raise self.requested

    def __enter__(self):
        for sig in (signal.SIGINT, signal.SIGTERM):
            self.previous[sig] = signal.signal(sig, self._receive)
        return self

    def __exit__(self, *exc):
        for sig, handler in self.previous.items():
            signal.signal(sig, handler)

    @contextmanager
    def phase(self):
        self.active = True
        try:
            if self.requested is not None:
                raise self.requested
            yield
        finally:
            self.active = False

    @contextmanager
    def suspend(self):
        active, self.active = self.active, False
        try:
            yield
        finally:
            self.active = active


def phase(signals):
    return signals.phase() if signals is not None else nullcontext()


def suspend(signals):
    return signals.suspend() if signals is not None else nullcontext()


def close_evaluator(ev, error=None):
    """Interruptions do not wait for a busy request before killing its group."""
    if ev is None:
        return
    if isinstance(error, (Interrupted, KeyboardInterrupt)) and hasattr(ev, "kill"):
        ev.kill()
    ev.close()
