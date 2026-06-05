"""Thin wrapper — delegates to eigensolve_from_sidecar.py --backend tfjava.

Preserved for backward compatibility: CI workflows and any callers referencing
this path continue to work unchanged. All logic lives in the consolidated
``eigensolve_from_sidecar.py`` (issue #127).
"""

import sys
import runpy
from pathlib import Path

sys.argv = [str(Path(__file__).parent / "eigensolve_from_sidecar.py"),
            "--backend", "tfjava"] + sys.argv[1:]
runpy.run_path(sys.argv[0], run_name="__main__")
