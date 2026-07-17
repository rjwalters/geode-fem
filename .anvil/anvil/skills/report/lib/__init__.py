"""Report-skill-local helpers.

These modules implement the small in-skill primitives that the report
commands (`report-promote`, `report-audit`) lean on. They live here
rather than under ``anvil/lib/`` because they are skill-specific (the
ack-file schema is part of the report-promote contract; the URL
classifier serves the report-audit findings table) and because anvil's
v0 policy is to let the framework ``lib/`` emerge from observed
duplication rather than design it up-front.

If a future skill needs the same primitives, the lift to
``anvil/lib/`` is mechanical.
"""
