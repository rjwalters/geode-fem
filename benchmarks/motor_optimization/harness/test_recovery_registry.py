"""Durable chain identity, real process exclusion and interruption evidence."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from recovery_registry import Registry
from session_journal import Journal, digest, freeze, read_events


def original(root):
    root = root.resolve()
    j = Journal(root, {"fixture": True})
    freeze(root / "bindings.json", {"session_directory": str(root),
                                    "recovery_registry": str(root.with_name(root.name + ".recoveries"))})
    j.emit("session_ready", bindings_sha256=digest(root / "bindings.json"))
    j.close()
    return root


def generation(path, link):
    j = Journal(path, {"fixture": True})
    freeze(path / "bindings.json", {"recovery": link})
    j.emit("session_ready", bindings_sha256=digest(path / "bindings.json"))
    return j


class RegistryTests(unittest.TestCase):
    def test_chain_preserves_original_and_seals_plans_and_prior_generations(self):
        with tempfile.TemporaryDirectory() as td:
            source = original(Path(td) / "source")
            before = {p: p.read_bytes() for p in source.iterdir()}
            with Registry(source) as registry:
                self.assertEqual(registry.generations(), [])
                path, link = registry.reserve({"fixture": "first plan"})
                first = generation(path, link)
                first.emit("session_finished", complete=False)
                first.close()
                registry.finish(1)
                with self.assertRaisesRegex(ValueError, "already recorded"):
                    registry.finish(1)
            with Registry(source) as registry:
                path, link = registry.reserve({"fixture": "second plan"})
                second = generation(path, link); second.close()
                self.assertEqual([e["index"] for e in registry.generations()], [1, 2])
            for path, content in before.items():
                self.assertEqual(path.read_bytes(), content)
            records = read_events(source.with_name("source.recoveries") / "events.jsonl")["events"]
            self.assertEqual([r["kind"] for r in records], ["registry_created", "generation_reserved",
                                                           "generation_finished", "generation_reserved"])

    def test_different_source_location_cannot_reset_registry(self):
        with tempfile.TemporaryDirectory() as td:
            source = original(Path(td) / "source")
            with Registry(source): pass
            moved = source.with_name("renamed")
            source.rename(moved)
            with self.assertRaisesRegex(ValueError, "frozen registry location"):
                Registry(moved)
            self.assertFalse(moved.with_name("renamed.recoveries").exists())

    def test_live_generation_and_missing_reserved_generation_cannot_be_skipped(self):
        for create in (False, True):
            with self.subTest(create=create), tempfile.TemporaryDirectory() as td:
                source = original(Path(td) / "source")
                with Registry(source) as registry:
                    path, link = registry.reserve({"fixture": True})
                    journal = generation(path, link) if create else None
                try:
                    with self.assertRaises((ValueError, FileNotFoundError)):
                        Registry(source)
                finally:
                    if journal: journal.close()

    def test_changed_origin_prior_journal_plan_or_link_refuses_reopen(self):
        for changed in ("origin", "previous", "plan", "link", "unregistered"):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as td:
                source = original(Path(td) / "source")
                with Registry(source) as registry:
                    path, link = registry.reserve({"fixture": True})
                    journal = generation(path, link); journal.close()
                    registry.finish(1)
                    registry_root = registry.root
                if changed == "origin":
                    (source / "events.jsonl").write_text((source / "events.jsonl").read_text() + " ")
                elif changed == "previous":
                    # A new valid event still changes the sealed generation.
                    event = {"sequence": 2, "kind": "changed", "time": 1.}
                    with (path / "events.jsonl").open("a") as stream:
                        stream.write(json.dumps(event) + "\n")
                elif changed == "plan":
                    (registry_root / "plan-0001.json").write_text("{}")
                elif changed == "link":
                    (path / "bindings.json").write_text("{}")
                else:
                    (registry_root / "generation-9999").mkdir()
                with self.assertRaises(ValueError): Registry(source)

    def test_torn_registry_tail_is_retained_and_not_reinitialized(self):
        with tempfile.TemporaryDirectory() as td:
            source = original(Path(td) / "source")
            with Registry(source) as registry: root = registry.root
            path = root / "events.jsonl"
            with path.open("ab") as stream: stream.write(b'{"sequence":1')
            before = path.read_bytes()
            with self.assertRaisesRegex(ValueError, "torn"):
                Registry(source)
            self.assertEqual(path.read_bytes(), before)

    def test_real_process_lock_excludes_second_launcher_and_releases_after_kill(self):
        with tempfile.TemporaryDirectory() as td:
            source = original(Path(td) / "source")
            root = Path(td)
            script = root / "holder.py"
            script.write_text('''import pathlib,sys,time
sys.path.insert(0,sys.argv[1])
from recovery_registry import Registry
with Registry(sys.argv[2]):
    pathlib.Path(sys.argv[3]).touch()
    time.sleep(60)
''')
            ready = root / "ready"
            proc = subprocess.Popen([sys.executable, "-I", str(script), str(HERE), str(source), str(ready)],
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 10
                while not ready.exists() and time.monotonic() < deadline:
                    if proc.poll() is not None: self.fail(proc.communicate()[1].decode())
                    time.sleep(.01)
                self.assertTrue(ready.exists())
                with self.assertRaises(BlockingIOError): Registry(source)
                proc.kill(); proc.communicate(timeout=5)
                with Registry(source) as registry:
                    self.assertEqual(registry.generations(), [])
            finally:
                if proc.poll() is None:
                    proc.kill(); proc.communicate()


if __name__ == "__main__":
    unittest.main(verbosity=2)
