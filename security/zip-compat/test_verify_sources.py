#!/usr/bin/env python3
"""Regression: verify_sources must reject omitted_from_vendor orphan entries."""

import json
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run  # noqa: E402


def with_omitted(extra):
    real = json.loads

    def loads(text, *args, **kwargs):
        data = real(text, *args, **kwargs)
        if isinstance(data, dict) and "omitted_from_vendor" in data:
            data["omitted_from_vendor"]["paths"] = (
                list(data["omitted_from_vendor"]["paths"]) + extra)
        return data

    return mock.patch.object(run.json, "loads", loads)


class VerifySourcesOrphans(unittest.TestCase):
    def test_unmodified_tree_passes(self):
        run.verify_sources()

    def test_orphan_entry_rejected(self):
        with with_omitted(["not-a-crate-9.9.9/whatever.yaml"]):
            with self.assertRaisesRegex(RuntimeError, "match no vendored crate"):
                run.verify_sources()

    def test_prefix_substring_of_real_crate_rejected(self):
        with with_omitted(["tch-0.22.01/x.yaml"]):
            with self.assertRaises(RuntimeError):
                run.verify_sources()


if __name__ == "__main__":
    unittest.main()
