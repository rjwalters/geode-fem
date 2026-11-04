"""Portable request preparation on a synthetic closed study; no native execution."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

HERE=Path(__file__).resolve().parent;REPO=HERE.parents[2];sys.path.insert(0,str(HERE))
import refinement_bundle as bundle
from mesh_test_support import CACHE, hydrate
from closed_capture import census

MESH_COMMIT='df83693b6a7af6badb5a8e18dcd4b27393d17c14'


class RefinementBundleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp=tempfile.TemporaryDirectory();cls.base=Path(cls.temp.name).resolve()
        archive=HERE/'smoke/synthetic_scientific_selection.tar.gz'
        assert hashlib.sha256(archive.read_bytes()).hexdigest()=='c08325dc9e1ab75e3739aab08a266330023dbac1d75b35eebf7b3ab8eb6b41a2'
        with tarfile.open(archive) as tar:
            assert all(m.isfile() and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts and not m.mode&0o7000 for m in tar.getmembers())
            tar.extractall(cls.base/'fixture',filter='tar')
        hydrate(cls.base/'fixture')
        cls.capture=cls.base/'fixture/capture'
        ids=json.loads((cls.base/'fixture/identities.json').read_text());ids.pop('scope')
        cls.args={**ids,'git_repo':REPO,'mesh_evidence_commit':MESH_COMMIT,'l3_mesh':CACHE/'614bf98c36e105adca2b6f811f09ebe4aa1c4d1c0a456eb8c3041bf81bdd584a'}
        cls.before=census(cls.capture)
        cls.receipt=bundle.prepare(cls.capture,out=cls.base/'prepared',**cls.args)
        assert census(cls.capture)==cls.before
    @classmethod
    def tearDownClass(cls):cls.temp.cleanup()
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve()/'bundle';shutil.copytree(self.base/'prepared',self.root)
        self.sha=self.receipt['sha256']
    def verify(self):return bundle.verify(self.root,manifest_sha256=self.sha)
    def mutate(self,name,change):
        path=self.root/name;value=json.loads(path.read_text());change(value);path.write_text(json.dumps(value)+'\n')
    def repin(self):
        # Intentional fixture-only new preparation receipt to reach semantic checks.
        path=self.root/'bundle.json';m=json.loads(path.read_text());inv=census(self.root);inv['files'].pop('bundle.json')
        m['inventory']=inv;path.write_text(json.dumps(m)+'\n');self.sha=hashlib.sha256(path.read_bytes()).hexdigest()
    def test_public_preparation_emits_all_exact_checks_and_separate_geometry_without_execution(self):
        m=self.verify();self.assertEqual(len(m['jobs']),21);self.assertEqual(len(m['geometry']),3)
        self.assertEqual(set(j['check'] for j in m['jobs']),{'primary','L3','g40','tight','shifted','withheld','conforming'})
        self.assertFalse(m['can_launch']);self.assertFalse((self.root/'results').exists())
        self.assertEqual(m['mesh_evidence']['mesh_sha256'],'614bf98c36e105adca2b6f811f09ebe4aa1c4d1c0a456eb8c3041bf81bdd584a')
        self.assertEqual(census(self.capture),self.before)
        self.assertTrue(all(g['requires_fem'] is False for g in m['geometry']))
        self.assertEqual(len({g['space'] for g in m['geometry']}),2)
    def test_request_change_rejected_by_census(self):
        m=self.verify();p=self.root/m['jobs'][0]['request_file'];p.write_text('{}')
        with self.assertRaisesRegex(ValueError,'census'):self.verify()
    def test_missing_scheduled_check_cannot_be_replaced(self):
        self.mutate('bundle.json',lambda m:m['jobs'].pop());self.repin()
        with self.assertRaisesRegex(ValueError,'frozen jobs'):self.verify()
    def test_wrong_native_point_refused_even_after_fixture_repin(self):
        m=self.verify();name=m['jobs'][0]['request_file']
        self.mutate(name,lambda r:r['point_bits'].__setitem__(0,0));self.repin()
        with self.assertRaisesRegex(ValueError,'request differs'):self.verify()
    def test_geometry_query_must_match_full_design(self):
        m=self.verify();name=m['geometry'][0]['request_file'];self.mutate(name,lambda r:r.update(id=True));self.repin()
        with self.assertRaisesRegex(ValueError,'request differs'):self.verify()
    def test_mesh_and_definition_hashes_rechecked_after_fixture_repin(self):
        (self.root/'inputs/L3.msh').write_bytes(b'wrong mesh');self.repin()
        with self.assertRaises(ValueError):self.verify()
    def test_extra_file_and_wrong_receipt_refused(self):
        with self.assertRaisesRegex(ValueError,'receipt'):bundle.verify(self.root,manifest_sha256='a'*64)
        (self.root/'extra').write_text('not in inventory')
        with self.assertRaisesRegex(ValueError,'census'):self.verify()
    def test_preparation_refuses_existing_output_and_inside_capture(self):
        with self.assertRaisesRegex(ValueError,'new directory'):bundle.prepare(self.capture,out=self.root,**self.args)
        with self.assertRaisesRegex(ValueError,'new directory'):bundle.prepare(self.capture,out=self.capture/'new',**self.args)
    def test_mesh_evidence_revision_is_independently_pinned(self):
        with self.assertRaisesRegex(ValueError,'full mesh-evidence commit'):
            bundle._l3(REPO,'HEAD',producer_commit=self.args['reviewed_commit'],reference={},l3_mesh=self.args['l3_mesh'])
    def test_changed_l3_construction_source_refused(self):
        original=bundle.producer_source.git
        def altered(repo,*args):
            raw=original(repo,*args)
            if args==('show',MESH_COMMIT+':'+bundle.GEOMETRY_SOURCES[0]):return raw+b'\n# altered source\n'
            return raw
        with mock.patch.object(bundle.producer_source,'git',side_effect=altered):
            with self.assertRaisesRegex(ValueError,'construction sources differ'):
                bundle._l3(REPO,MESH_COMMIT,producer_commit=self.args['reviewed_commit'],reference={},l3_mesh=self.args['l3_mesh'])

    def test_l3_report_must_match_study_upstream_reference(self):
        with self.assertRaisesRegex(ValueError,'initial geometry/provenance'):
            bundle._l3(REPO,MESH_COMMIT,producer_commit=self.args['reviewed_commit'],reference={'source_commit':'0'*40},l3_mesh=self.args['l3_mesh'])

    def test_external_l3_mesh_must_match_reviewed_report_bytes(self):
        reference=json.loads((REPO/'benchmarks/motor_reference/reference_data.json').read_text())
        bad=Path(self.temp.name)/'bad.msh';bad.write_bytes(b'not the reviewed mesh')
        with self.assertRaisesRegex(ValueError,'initial geometry/provenance'):
            bundle._l3(REPO,MESH_COMMIT,producer_commit=self.args['reviewed_commit'],reference=reference,l3_mesh=bad)
        # Reach the digest comparison independently of the byte-count guard.
        changed=bytearray(self.args['l3_mesh'].read_bytes());changed[len(changed)//2]^=1
        bad.write_bytes(changed)
        self.assertEqual(bad.stat().st_size,self.args['l3_mesh'].stat().st_size)
        with self.assertRaisesRegex(ValueError,'initial geometry/provenance'):
            bundle._l3(REPO,MESH_COMMIT,producer_commit=self.args['reviewed_commit'],reference=reference,l3_mesh=bad)
        link=Path(self.temp.name)/'linked.msh';link.symlink_to(self.args['l3_mesh'])
        with self.assertRaisesRegex(ValueError,'redirected external L3 mesh'):
            bundle._l3(REPO,MESH_COMMIT,producer_commit=self.args['reviewed_commit'],reference=reference,l3_mesh=link)

    def test_report_only_archive_binds_supplied_external_mesh(self):
        original=bundle.producer_source.git
        def stripped(repo,*args):
            if args==('show',MESH_COMMIT+':'+bundle.ARCHIVE):return (REPO/bundle.ARCHIVE).read_bytes()
            return original(repo,*args)
        reference=json.loads((REPO/'benchmarks/motor_reference/reference_data.json').read_text())
        with mock.patch.object(bundle.producer_source,'git',side_effect=stripped):
            mesh,_,evidence=bundle._l3(REPO,MESH_COMMIT,producer_commit=self.args['reviewed_commit'],
                reference=reference,l3_mesh=self.args['l3_mesh'])
        self.assertEqual(hashlib.sha256(mesh).hexdigest(),evidence['mesh_sha256'])
        self.assertEqual(evidence['archive_sha256'],hashlib.sha256((REPO/bundle.ARCHIVE).read_bytes()).hexdigest())

    def test_cli_builds_same_request_bundle(self):
        out=Path(self.temp.name).resolve()/'cli'
        args=['--capture',str(self.capture),'--out',str(out)]
        for k,v in self.args.items():args.extend(['--'+k.replace('_','-'),str(v)])
        result=subprocess.run([sys.executable,'-I',str(HERE/'refinement_bundle.py'),*args],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)
        receipt=json.loads(result.stdout);m=bundle.verify(out,manifest_sha256=receipt['sha256'])
        self.assertEqual(m['jobs'],self.verify()['jobs']);self.assertEqual(m['geometry'],self.verify()['geometry'])
        self.assertFalse(receipt['can_launch'])


if __name__=='__main__':unittest.main(verbosity=2)
