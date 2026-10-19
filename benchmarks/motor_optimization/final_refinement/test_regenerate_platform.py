"""The historical mesh platform gate must precede every write or subprocess."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest import mock

PATH=Path(__file__).resolve().parent/'smoke/regenerate_meshes.py'
SPEC=importlib.util.spec_from_file_location('historical_mesh_regeneration',PATH)
regeneration=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(regeneration)


class RegenerationPlatformTests(unittest.TestCase):
    def test_unsupported_platform_refuses_before_creating_output_or_running_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            for system,machine in [('Linux','x86_64'),('Linux','aarch64'),('Darwin','x86_64')]:
                with self.subTest(system=system,machine=machine):
                    out=Path(temporary)/(system+'-'+machine)
                    with mock.patch.object(regeneration.platform,'system',return_value=system), \
                         mock.patch.object(regeneration.platform,'machine',return_value=machine), \
                         mock.patch.object(regeneration.subprocess,'check_output') as command:
                        with self.assertRaisesRegex(ValueError,'requires macOS arm64'):
                            regeneration.regenerate(out)
                        command.assert_not_called()
                    self.assertFalse(out.exists())


if __name__=='__main__':unittest.main()
