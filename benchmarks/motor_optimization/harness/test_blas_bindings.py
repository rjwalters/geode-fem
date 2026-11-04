"""BLAS/LAPACK identity must remain explicit and frozen during recovery."""
import copy
import json
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import recovery_bindings as bindings
import runtime_fingerprint as runtime
from test_recovery_bindings import fixture


def config():
    return {"Build Dependencies": {kind: {"name": "fixture-blas", "version": "1.0",
             "found": True, "has ilp64": False} for kind in ("blas", "lapack")}}


class BlasTests(unittest.TestCase):
    def test_actual_installed_stack_has_all_peer_replay_keys(self):
        import numpy, scipy
        row = runtime.linear_algebra(['numpy', 'scipy'])
        fp = row['fingerprint']
        self.assertEqual(fp['numpy'],numpy.__version__)
        self.assertEqual(fp['scipy'],scipy.__version__)
        for name, module in [('numpy',numpy),('scipy',scipy)]:
            actual=module.show_config(mode='dicts')['Build Dependencies']
            for kind in ('blas','lapack'):
                self.assertEqual(fp[name+'_'+kind],actual[kind]['name'])
                self.assertEqual(fp[name+'_'+kind+'_version'],actual[kind]['version'])
                self.assertEqual(row['build_dependencies'][name][kind],actual[kind])
        self.assertEqual(set(fp), {'system','machine','macos_version','numpy','scipy',
            'numpy_blas','numpy_blas_version','numpy_lapack','numpy_lapack_version',
            'scipy_blas','scipy_blas_version','scipy_lapack','scipy_lapack_version'})
        mesher=runtime.linear_algebra(['numpy','gmsh'])
        self.assertEqual(set(mesher['build_dependencies']),{'numpy'})
        self.assertNotIn('scipy',mesher['fingerprint'])

    def test_capture_records_backend_and_abi_despite_unchanged_payload(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);(root/'module.py').write_text('same installed payload')
            class Dist:
                version='1.0'
                files=[Path('module.py')]
                def locate_file(self,entry):return root/entry
            cfg=config();module=SimpleNamespace(__version__='1.0',show_config=lambda **kw:copy.deepcopy(cfg))
            with patch.object(runtime.importlib.metadata,'distribution',return_value=Dist()), \
                 patch.object(runtime.importlib,'import_module',return_value=module):
                first=runtime.capture(['numpy'])
                self.assertIn('linear_algebra',first)
                cfg['Build Dependencies']['blas']['has ilp64']=True
                second=runtime.capture(['numpy'])
                self.assertEqual(first['distributions'],second['distributions'])
                self.assertEqual(first['linear_algebra']['fingerprint'],second['linear_algebra']['fingerprint'])
                self.assertNotEqual(first['linear_algebra'],second['linear_algebra'])

    def test_backend_version_abi_and_os_changes_refuse_recovery(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);session=fixture(root);cfg=config()
            module=SimpleNamespace(__version__='1.0',show_config=lambda **kw:copy.deepcopy(cfg))
            with patch.object(runtime.importlib,'import_module',return_value=module):
                kwargs={'repo':root,'source_probe':lambda _: {'tree_clean':True},
                        'runtime_probe':lambda _:runtime.linear_algebra(['numpy','scipy'])}
                frozen=bindings.capture(session,**kwargs)
                for kind in ('blas','lapack'):
                    for key,value in [('name','other-blas'),('version','2.0'),('has ilp64',True)]:
                        old=cfg['Build Dependencies'][kind][key];cfg['Build Dependencies'][kind][key]=value
                        with self.subTest(kind=kind,key=key),self.assertRaisesRegex(ValueError,'runtime'):
                            bindings.verify(frozen,session,**kwargs)
                        cfg['Build Dependencies'][kind][key]=old
                with patch.object(runtime.platform,'mac_ver',return_value=('changed','','')):
                    with self.assertRaisesRegex(ValueError,'runtime'):
                        bindings.verify(frozen,session,**kwargs)
                self.assertTrue(bindings.verify(frozen,session,**kwargs)['verified'])

    def test_missing_or_error_config_never_forms_valid_fingerprint(self):
        for bad in [{},{'Build Dependencies':{}}, {'Build Dependencies':{'blas':{'name':'x'}}}]:
            module=SimpleNamespace(__version__='1.0',show_config=lambda **kw:bad)
            with patch.object(runtime.importlib,'import_module',return_value=module),self.assertRaises(ValueError):
                runtime.linear_algebra(['numpy'])
        module=SimpleNamespace(__version__='1.0',show_config=lambda **kw: (_ for _ in ()).throw(RuntimeError('probe failed')))
        with patch.object(runtime.importlib,'import_module',return_value=module),self.assertRaises(RuntimeError):
            runtime.linear_algebra(['numpy'])

    def test_accelerate_unknown_version_still_binds_os_and_named_backend(self):
        cfg=config()
        for dep in cfg['Build Dependencies'].values():dep.update(name='Accelerate',version='unknown')
        module=SimpleNamespace(__version__='1.0',show_config=lambda **kw:cfg)
        with patch.object(runtime.importlib,'import_module',return_value=module), \
             patch.object(runtime.platform,'mac_ver',return_value=('26.0','','')):
            row=runtime.linear_algebra(['numpy'])
        self.assertEqual(row['fingerprint']['numpy_blas'],'Accelerate')
        self.assertEqual(row['fingerprint']['macos_version'],'26.0')

    def test_backend_override_environment_is_in_frozen_allowlist(self):
        for name in ('OPENBLAS_CORETYPE','GOTO_NUM_THREADS','BLIS_ARCH_TYPE',
                     'LD_LIBRARY_PATH','LD_PRELOAD','DYLD_LIBRARY_PATH','DYLD_INSERT_LIBRARIES'):
            self.assertIn(name,bindings.NUMERICAL_ENV)


if __name__=='__main__':unittest.main(verbosity=2)
