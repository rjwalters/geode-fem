"""Thin wrapper — delegates to eigensolve_from_sidecar.py --problem sphere-pec.

Deprecated: use ``eigensolve_from_sidecar.py --problem sphere-pec
--backend <tfjava|onnx>`` directly. Preserved for backward compatibility
so CI workflows and external callers referencing this path continue to
work unchanged. All logic lives in the consolidated
``eigensolve_from_sidecar.py`` (issue #144).

The shim defaults ``--backend tfjava`` to match the historical behavior
of this script (sphere-PEC was TF-Java-only when it was first added in
PR #137). Callers that want a different backend may pass
``--backend onnx`` (or any other accepted value) explicitly — the shim
forwards extra arguments verbatim.
"""

import sys
import runpy
from pathlib import Path

extra = sys.argv[1:]
# Inject sensible defaults that the legacy script provided implicitly. We
# only inject them when the caller has NOT already supplied them, so
# explicit overrides from CI / external callers always win.
forwarded = ["--problem", "sphere-pec"]
if "--backend" not in extra:
    forwarded += ["--backend", "tfjava"]

sys.argv = [str(Path(__file__).parent / "eigensolve_from_sidecar.py")] + forwarded + extra
runpy.run_path(sys.argv[0], run_name="__main__")
