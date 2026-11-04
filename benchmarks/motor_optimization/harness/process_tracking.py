"""Durable spawn intent before fork, identity after fork, and terminal receipt."""
import os
import process_identity


class Spawn:
    def __init__(self, journal, command, *, role, child_journal=None, attempt_id=None):
        self.journal, self.started_identity, self.finished = journal, None, False
        self.id = None
        if journal is not None:
            self.id = journal.emit("process_spawn_intent", role=role, command=list(command),
                parent_identity=process_identity.identify(os.getpid()),
                child_journal=child_journal, attempt_id=attempt_id)

    def started(self, pid):
        if self.journal is not None:
            self.started_identity = process_identity.identify(pid)
            self.journal.emit("process_spawn_started", spawn_id=self.id, identity=self.started_identity)

    def failed(self, error):
        """Only an ordinary Popen OSError proves that no child was returned.

        An asynchronous interruption between fork and publication leaves the
        intent unresolved; recovery must not infer that nothing was launched.
        """
        if self.journal is not None and isinstance(error, OSError):
            self.journal.emit("process_spawn_failed", spawn_id=self.id,
                              error=f"{type(error).__name__}: {error}")
            self.finished = True

    def reaped(self, receipt):
        if self.journal is not None and not self.finished and receipt.get("state") == "reaped":
            self.journal.emit("process_spawn_reaped", spawn_id=self.id, receipt=receipt)
            self.finished = True
