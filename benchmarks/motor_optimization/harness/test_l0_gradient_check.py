"""FD evidence retention and fixed-step decision checks."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import l0_gradient_check as check


class GradientEvidenceTests(unittest.TestCase):
    def test_a_better_other_step_cannot_hide_failure_at_the_fixed_step(self):
        rows = [{"h": h, "rel_dF": 1e-8, "rel_dmean_full": 1e-8} for h in check.STEPS]
        self.assertTrue(check.assess(rows))
        rows[1]["rel_dF"] = 1e-3
        self.assertFalse(check.assess(rows))
        rows[1]["rel_dF"] = None
        self.assertFalse(check.assess(rows))

    def test_boundary_direction_is_inward_and_scalar_components_unchanged(self):
        space = {"free": ["MAG", "CPR1", "CPS1"], "x0": [.2, 0., 0.],
                 "lower": [0., -.0015, 0.], "upper": [1., 0., .0005]}
        direction = check.inward_direction(space, [-.4, .001, -.0002])
        self.assertEqual(direction, [-.4, -.001, .0002])
        for h in check.STEPS:
            for k in (1, 2):
                for x, d, lo, hi in zip(space["x0"], direction, space["lower"], space["upper"]):
                    self.assertLessEqual(lo, x + k * h * d)
                    self.assertLessEqual(x + k * h * d, hi)

    def test_one_sided_stencil_has_second_order_error_on_a_cubic(self):
        rows = []
        for h in check.STEPS:
            f = lambda x: x + x ** 3
            fd = check.difference(f(0.), f(h), f(2*h), h, True)
            self.assertAlmostEqual(fd, 1. - 2 * h * h, places=14)
            rows.append({"h": h, "fd_dF": fd, "adjoint_dF": 1.,
                         "fd_dmean_full": fd, "adjoint_dmean_full": 1.})
        for order in check.observed_orders(rows):
            self.assertAlmostEqual(order["dF"], 2., places=5)

    def test_raw_failed_replies_survive_and_later_point_is_attempted(self):
        mesh = check.HERE.parents[2] / "benchmarks/motor_reference/geometry/meshes/icem_initial_sector_L0.msh"
        holders = []
        class Fake:
            def __init__(self, *a, **k):
                self.ready = {"space": {"lower": [0., 0.], "upper": [2., 2.], "x0": [.2, .4]},
                              "train_msh_sha256": check.sha(mesh)}
                self.closed = False
                holders.append(self)
            def request(self, op, **kw):
                if op == "check":
                    return {"ok": True, "x_bits": kw["x_bits"], "geometry_ok": True,
                            "geometry": {"buildable": True, "admissible": True}, "trigger": None}
                return {"ok": False, "x_bits": kw["x_bits"], "error": "intentional FEM failure",
                        "resources": {"fixture": "preserve me"}}
            def close(self): self.closed = True
            def resources(self): return {"fixture": "lifetime retained"}
        with tempfile.TemporaryDirectory() as td:
            binary, out = Path(td) / "binary", Path(td) / "record.json"
            binary.write_bytes(b"fixture")
            argv = ["check", "--binary", str(binary), "--out", str(out), "--space", "a", "--newton", "default"]
            with patch.object(sys, "argv", argv), patch.object(check, "Evaluator", Fake):
                self.assertEqual(check.main(), 1)
            data = json.loads(out.read_text())
            self.assertEqual([p["status"] for p in data["points"]], ["failed", "failed"])
            for p in data["points"]:
                self.assertEqual(p["requests"][-1]["reply"]["error"], "intentional FEM failure")
                self.assertEqual(p["resources"], {"fixture": "lifetime retained"})
            self.assertTrue(all(e.closed for e in holders))
            raw = out.read_bytes()
            with patch.object(sys, "argv", argv), patch.object(check, "Evaluator") as ev:
                with self.assertRaisesRegex(SystemExit, "overwrite"):
                    check.main()
                ev.assert_not_called()
            self.assertEqual(raw, out.read_bytes())


if __name__ == "__main__":
    unittest.main(verbosity=2)
