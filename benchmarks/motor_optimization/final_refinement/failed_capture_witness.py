"""Build the refused-capture (B2) witness directory for a four-boot policy.

Copies the named operational records of the refused capture boot into a new
directory and writes `failed-capture-witness.json`, naming each file's SHA-256
and size. Prints the manifest digest to retain independently for the policy's
`failed_capture_witness_sha256`. Reads no study data and makes no cloud calls.
The witness statement is the operator's attestation; review it before pinning.
This is preparation only, not a capture verdict or a permission to start B3.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import producer_source
from reboot_attestation import (FAILED_CAPTURE_FILES, FAILED_CAPTURE_KIND, FAILED_CAPTURE_STATEMENT,
                                boot_id, require)
from trace_archive import _sha

SCOPE = ('Operational records of the refused capture boot B2 only; no study outcome, '
         'capture archive, quiet check or source census exists for B2.')


def build(source, out, *, instance_id, volume_id, study_boot, recovery_boot, failed_capture_boot,
          recovery_witness_sha256, statement=FAILED_CAPTURE_STATEMENT):
    source = Path(os.path.abspath(source)); out = Path(os.path.abspath(out))
    for value in (study_boot, recovery_boot, failed_capture_boot):
        boot_id(value)
    require(len({study_boot, recovery_boot, failed_capture_boot}) == 3, 'four-boot policy reuses a boot')
    require(re.fullmatch(r'i-[0-9a-f]+', instance_id) is not None
            and re.fullmatch(r'vol-[0-9a-f]+', volume_id) is not None, 'invalid instance/volume')
    _sha(recovery_witness_sha256)
    require(isinstance(statement, str) and statement.startswith(FAILED_CAPTURE_STATEMENT),
            'missing refused-capture witness statement')
    require(not out.exists() and not out.is_symlink(), 'witness output must be new')
    require(source not in (out, *out.parents) and out not in source.parents, 'witness output overlaps its source')
    raw = {name: producer_source.regular(source/name).read_bytes() for name in sorted(FAILED_CAPTURE_FILES)}
    out.mkdir(parents=True)
    files = {}
    for name, data in raw.items():
        with (out/name).open('xb') as stream:
            stream.write(data); stream.flush(); os.fsync(stream.fileno())
        require((out/name).read_bytes() == data == producer_source.regular(source/name).read_bytes(),
                'failed-capture record changed while copying: ' + name)
        files[name] = {'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}
    manifest = {'schema': 1, 'kind': FAILED_CAPTURE_KIND, 'instance_id': instance_id, 'volume_id': volume_id,
                'study_boot': study_boot, 'recovery_boot': recovery_boot,
                'failed_capture_boot': failed_capture_boot, 'recovery_witness_sha256': recovery_witness_sha256,
                'witness_statement': statement, 'scope': SCOPE, 'files': files}
    encoded = (json.dumps(manifest, sort_keys=True, indent=2) + '\n').encode()
    with (out/'failed-capture-witness.json').open('xb') as stream:
        stream.write(encoded); stream.flush(); os.fsync(stream.fileno())
    return {'directory': str(out), 'manifest': str(out/'failed-capture-witness.json'),
            'sha256': hashlib.sha256(encoded).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path, help='retained B2 operator record directory')
    parser.add_argument('--out', required=True, type=Path, help='new witness directory')
    for name in ('instance-id', 'volume-id', 'study-boot', 'recovery-boot', 'failed-capture-boot',
                 'recovery-witness-sha256'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args()
    print(json.dumps(build(args.source, args.out, instance_id=args.instance_id, volume_id=args.volume_id,
                           study_boot=args.study_boot, recovery_boot=args.recovery_boot,
                           failed_capture_boot=args.failed_capture_boot,
                           recovery_witness_sha256=args.recovery_witness_sha256), sort_keys=True))


if __name__ == '__main__':
    main()
