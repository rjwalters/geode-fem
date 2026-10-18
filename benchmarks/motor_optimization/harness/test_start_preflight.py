"""Frozen-start gate rejects wrong meshes/points, remeshing and hidden FEM."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import start_preflight as preflight
from evaluator import to_bits


def reply():
    return {"ok": True, "x_bits": to_bits([0.]), "geometry_ok": True,
            "geometry": {"buildable": True, "admissible": True}, "trigger": None,
            "quality": {"inverted": 0, "min_angle_deg": 5.},
            "charges": [{"forward_solves": 0, "adjoint_solves": 0, "seconds": .1}]}


class PreflightTests(unittest.TestCase):
    def test_rejects_wrong_bits_bad_domain_trigger_quality_and_fem(self):
        self.assertIsNone(preflight.check_reply(reply(), [0.]))
        changes = (
            lambda r: r.update(x_bits=to_bits([1.])),
            lambda r: r["geometry"].update(buildable=False),
            lambda r: r["geometry"].update(admissible=False),
            lambda r: r.update(trigger={"kind": "min_angle"}),
            lambda r: r["quality"].update(inverted=1),
            lambda r: r["quality"].update(min_angle_deg=4.999),
            lambda r: r["quality"].update(min_angle_deg=float("nan")),
            lambda r: r["charges"][0].update(forward_solves=1),
            lambda r: r.update(charges=[]),
        )
        for change in changes:
            r = reply(); change(r)
            self.assertIsNotNone(preflight.check_reply(r, [0.]))

    def test_failed_preflight_cannot_dispatch_calibration_or_main_jobs(self):
        import session
        with tempfile.TemporaryDirectory() as td:
            binary = Path(td) / "eval"
            binary.write_bytes(b"fixture")
            with patch.object(session, "host_record", return_value={}), \
                 patch.object(session, "provenance", return_value={}), \
                 patch("build_provenance.verify", return_value={"verified": True}), \
                 patch.object(session, "sha256", return_value="fixture-hash"), \
                 patch.object(preflight, "verify", return_value={"status": "failed"}), \
                 patch.object(session, "gated_calibration", side_effect=AssertionError("calibration must not begin")) as gate, \
                 patch.object(session, "calibrate") as calibrate, \
                 patch.object(session, "schedule") as schedule:
                with self.assertRaisesRegex(SystemExit, "preflight failed"):
                    session.main(["--main", "--procs", "1", "--binary", str(binary),
                                  "--out", str(Path(td) / "session")])
                gate.assert_not_called()
                calibrate.assert_not_called()
                schedule.assert_not_called()

    def test_all_twenty_checks_retained_failed_start_never_replaced(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            binary, starts, out = root / "binary", root / "starts.json", root / "preflight.json"
            binary.write_bytes(b"fixture")
            spaces = {sp: {"free": ["x"], "lower": [-1.], "upper": [1.], "x0": [0.]} for sp in "ab"}
            starts.write_text(json.dumps({"spaces": spaces,
                "starts": {st: {sp: [0.] for sp in "ab"} for st in preflight.STARTS}}))
            meshes = {k: root / f"L{k}.msh" for k in (0, 1)}
            for p in meshes.values(): p.write_bytes(str(p).encode())
            original = starts.read_bytes()
            instances = []
            class Fake:
                def __init__(self, argv, **kw):
                    opts = dict(zip(argv[1::2], argv[2::2]))
                    self.space, self.level = opts["--space"], int(opts["--train-level"])
                    self.ready = {"space": spaces[self.space], "train_msh_sha256": preflight.sha(meshes[self.level])}
                    self.n, self.closed = 0, False
                    instances.append(self)
                def request(self, op, **kw):
                    self.n += 1
                    assert op == "check"
                    r = reply()
                    if self.space == "b" and self.level == 1 and self.n == 2:
                        r["trigger"] = {"kind": "fresh_mesh_would_be_needed"}
                    return r
                def close(self): self.closed = True
                def resources(self): return {"fixture": "retained"}
            result = preflight.verify(binary, starts, meshes, out, evaluator_factory=Fake)
            self.assertEqual(result["status"], "failed")
            checks = [c for s in result["sessions"] for c in s["checks"]]
            self.assertEqual(len(checks), 20)
            self.assertEqual(sum(c["status"] == "failed" for c in checks), 1)
            self.assertEqual(starts.read_bytes(), original)
            self.assertTrue(all(e.closed and e.n == 5 for e in instances))
            self.assertEqual(json.loads(out.read_text()), result)
            raw = out.read_bytes()
            with self.assertRaises(FileExistsError):
                preflight.verify(binary, starts, meshes, out, evaluator_factory=Fake)
            self.assertEqual(out.read_bytes(), raw)


if __name__ == "__main__":
    unittest.main(verbosity=2)
