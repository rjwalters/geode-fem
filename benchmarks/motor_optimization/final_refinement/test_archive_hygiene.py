"""Prevent compiled executables and derived mesh payloads returning in archives."""
import gzip
from pathlib import Path
import tarfile
import unittest

HERE=Path(__file__).resolve().parent
MAGIC=(b'\x7fELF',b'\xcf\xfa\xed\xfe',b'\xce\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',
       b'\xfe\xed\xfa\xce',b'\xca\xfe\xba\xbe',b'MZ',b'$MeshFormat')


class ArchiveHygieneTests(unittest.TestCase):
    def test_all_smoke_archives_exclude_compiled_binaries_and_meshes(self):
        for path in (HERE/'smoke').glob('*.gz'):
            if path.name.endswith('.tar.gz'):
                with tarfile.open(path) as archive:
                    for member in archive:
                        if not member.isfile():continue
                        with self.subTest(archive=path.name,member=member.name):
                            self.assertFalse(member.name.endswith('.msh'))
                            self.assertFalse(archive.extractfile(member).read(16).startswith(MAGIC))
            else:
                with self.subTest(archive=path.name):
                    self.assertFalse(gzip.decompress(path.read_bytes()).startswith(MAGIC))


if __name__=='__main__':unittest.main()
