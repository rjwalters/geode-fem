"""Synthetic planning checks; no FEM or optimization-performance evidence."""
import copy
from dataclasses import replace
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from check_plan import make_plan  # noqa: E402
from selection import Design, Validation, select  # noqa: E402


def manifest():
    settings = {"fixed_current_a": 9.6777, "material": "synthetic-fixture"}
    theta0 = Design.from_values({"x": 0.0}, settings)
    candidate = Design.from_values({"x": 1.0}, settings)
    row = Validation(
        "attempt", "a" * 64, "A", "a", 0, 1, 0, candidate,
        10.0, 120.0, 0.001, 1.01, eligible_run=True,
        host_contaminated=False, completed=True, geometry_admissible=True,
        discrete_pass=True, mesh_pass=True,
    )
    return select([row], f0=1.0, mean0=1.0, theta0=theta0)


def multiple_settings_manifest():
    scalar={'motion':'scalar','current_A':9.6777}
    cp={'motion':'cp','current_A':9.6777}
    initial=Design.from_values({'x':0.0},scalar)
    cp_initial=Design.from_values({'x':0.0},cp)
    row=Validation('scalar-attempt','a'*64,'A','a',0,1,0,
        Design.from_values({'x':1.0},scalar),10.,120.,.001,1.01,
        eligible_run=True,host_contaminated=False,completed=True,
        geometry_admissible=True,discrete_pass=True,mesh_pass=True)
    cp_row=replace(row,run_id='cp-attempt',space='b',design=Design.from_values({'x':1.0},cp))
    return select([row,cp_row],f0=1.,mean0=1.,theta0=initial,theta0_references=[cp_initial])


class PlanTests(unittest.TestCase):
    def test_every_selected_design_receives_the_entire_matrix(self):
        source = manifest()
        plan = make_plan(source)
        self.assertEqual(len(source["jobs"]), 2)
        self.assertEqual(len(plan["jobs"]), 12)
        self.assertEqual(len({j["job_id"] for j in plan["jobs"]}), 12)
        for entry in source["jobs"]:
            actual = {j["spec"]["name"] for j in plan["jobs"]
                      if j["design_key"] == entry["design_key"]}
            self.assertEqual(actual, {"primary", "L3", "g40", "tight", "shifted", "withheld"})

    def test_resolution_currents_and_tolerances_are_the_adopted_values(self):
        specs = {s["name"]: s for s in make_plan(manifest())["specs"]}
        for name, spec in specs.items():
            self.assertEqual(spec["level"], 3 if name == "L3" else 2)
            self.assertEqual(spec["harmonic_max"], 93)
            self.assertEqual(spec["newton_rel_tol"], 1e-12 if name == "tight" else 1e-8)
            self.assertEqual(spec["current_fractions"],
                             [0.25, 0.75] if name == "withheld" else [0.0, 0.5, 1.0])
        self.assertEqual(specs["primary"]["angles_deg"], list(range(20)))
        self.assertEqual(specs["tight"]["angles_deg"], list(range(20)))
        self.assertEqual(specs["g40"]["angles_deg"], [i / 2 for i in range(40)])

    def test_shifted_check_is_an_independent_complete_cold_started_sweep(self):
        plan = make_plan(manifest())
        for job in plan["jobs"]:
            spec = job["spec"]
            self.assertIs(spec["requires_complete_sweep"], True)
            self.assertEqual(spec["current_sweep_initialization"], "cold")
            self.assertEqual(spec["successive_angle_initialization"], "previous_angle")
            if spec["name"] == "shifted":
                self.assertEqual(spec["angles_deg"], [i + 0.5 for i in range(20)])
                self.assertNotIn("g40", job["job_id"])

    def test_every_reference_exists_and_matches_the_requested_settings(self):
        plan = make_plan(manifest())
        jobs = {j["job_id"]: j for j in plan["jobs"]}
        anchor = next(c["design_key"] for c in plan["cohort"] if "theta0" in c["roles"])
        for job in jobs.values():
            ref = jobs[job["theta0_job_id"]]
            self.assertEqual(ref["design_key"], anchor)
            self.assertEqual(ref["spec"], job["spec"])
            if job["primary_job_id"] is not None:
                primary = jobs[job["primary_job_id"]]
                self.assertEqual(primary["design_key"], job["design_key"])
                self.assertEqual(primary["spec"]["name"], "primary")
        self.assertTrue(all(j["design_key"] == anchor for j in plan["jobs"][:6]))

    def test_cohort_roles_and_target_credit_are_preserved_without_aliases(self):
        source = manifest()
        before = copy.deepcopy(source)
        plan = make_plan(source)
        self.assertEqual(source, before)
        self.assertEqual(plan["cohort"], source["jobs"])
        self.assertEqual(plan["target_credit_policy"], "preserve_original_common_validation")
        self.assertEqual(plan["replacement_policy"], "never_replace_selected_design")
        plan["cohort"][0]["roles"].clear()
        plan["jobs"][0]["spec"]["angles_deg"].clear()
        self.assertEqual(source, before)
        self.assertEqual(len(plan["specs"][0]["angles_deg"]), 20)

    def test_no_run_or_conforming_success_is_inferred(self):
        plan = make_plan(manifest())
        for job in plan["jobs"]:
            self.assertEqual(job["status"], "not_run")
            self.assertEqual(job["cost_scope"], "post_run")
        self.assertEqual(plan["conforming_checks"],
                         "pending_separate_fresh_mesh_and_supported_rotation_gate")
        json.dumps(plan, allow_nan=False)

    def test_changed_source_evidence_changes_manifest_binding(self):
        source = manifest()
        first = make_plan(source)
        self.assertEqual(first, make_plan(source))
        source["records"][0]["record"]["source_sha256"] = "b" * 64
        second = make_plan(source)
        self.assertNotEqual(first["selection_manifest_sha256"], second["selection_manifest_sha256"])

    def test_each_motion_map_has_its_own_matched_reference_and_no_extra_roles(self):
        source=multiple_settings_manifest();plan=make_plan(source)
        cohort={c['design_key']:c for c in plan['cohort']}
        jobs={j['job_id']:j for j in plan['jobs']}
        references=set(plan['reference_design_keys'])
        self.assertEqual(len(references),2)
        self.assertEqual(len(plan['jobs']),24)
        self.assertEqual(plan['cohort'],source['jobs'])
        for job in jobs.values():
            ref=jobs[job['theta0_job_id']]
            self.assertEqual(ref['spec'],job['spec'])
            self.assertIn(ref['design_key'],references)
            self.assertEqual(cohort[job['design_key']]['theta0_design_key'],ref['design_key'])
            expected=Design(cohort[job['design_key']]['canonical_design']).settings_key
            self.assertEqual(job['spec']['base_settings_sha256'],expected)
        self.assertTrue(all(j['design_key'] in references for j in plan['jobs'][:12]))

    def test_missing_or_cross_map_theta0_binding_is_refused(self):
        for mutation in ('missing','cross_map','self','role'):
            source=multiple_settings_manifest()
            primary=next(j for j in source['jobs'] if 'theta0' in j['roles'])
            secondary=next(j for j in source['jobs'] if 'theta0_reference' in j['roles'])
            candidate=next(j for j in source['jobs'] if 'A/b/representative' in j['roles'])
            if mutation=='missing':candidate.pop('theta0_design_key')
            elif mutation=='cross_map':candidate['theta0_design_key']=primary['design_key']
            elif mutation=='self':candidate['theta0_design_key']=candidate['design_key']
            else:secondary['roles']=['invalid-reference-role']
            with self.subTest(mutation=mutation),self.assertRaisesRegex(ValueError,'theta0 reference'):
                make_plan(source)

    def test_reference_geometry_cannot_change_with_recomputed_design_hash(self):
        source=multiple_settings_manifest()
        secondary=next(j for j in source['jobs'] if 'theta0_reference' in j['roles'])
        old=secondary['design_key'];parts=json.loads(secondary['canonical_design'])
        parts[1][0]=json.loads(Design.from_values({'x':2.0},{'unused':True}).canonical)[1][0]
        altered=Design(json.dumps(parts,separators=(',',':')))
        secondary.update(canonical_design=altered.canonical,design_key=altered.key)
        for entry in source['jobs']:
            if entry['theta0_design_key']==old:entry['theta0_design_key']=altered.key
        with self.assertRaisesRegex(ValueError,'geometry or comparison settings'):make_plan(source)

    def test_missing_anchor_duplicate_and_tampered_design_fail_closed(self):
        for mutation in ("anchor", "duplicate", "tampered", "empty", "schema"):
            with self.subTest(mutation=mutation):
                source = manifest()
                if mutation == "anchor":
                    source["jobs"] = [j for j in source["jobs"] if "theta0" not in j["roles"]]
                elif mutation == "duplicate":
                    source["jobs"].append(copy.deepcopy(source["jobs"][0]))
                elif mutation == "tampered":
                    source["jobs"][0]["canonical_design"] += " "
                elif mutation == "empty":
                    source["jobs"] = []
                else:
                    source["schema_version"] = 2
                with self.assertRaises(ValueError):
                    make_plan(source)


if __name__ == "__main__":
    unittest.main()
