"""Tests hydrate omitted payloads from a separately regenerated, hash-checked cache.

This helper is never used by production capture, preparation, or reporting.
"""
import hashlib
import json
import os
from pathlib import Path

HERE=Path(__file__).resolve().parent
CACHE=Path(os.environ.get('GEODE_REFINEMENT_MESH_CACHE',HERE.parents[2]/'target/final-refinement-meshes'))


def mesh_bytes(sha,size=None):
    path=CACHE/sha
    if not path.is_file() or path.is_symlink():
        raise ValueError('Missing regenerated mesh '+sha+'; run smoke/regenerate_meshes.py --out '+str(CACHE))
    raw=path.read_bytes()
    if hashlib.sha256(raw).hexdigest()!=sha or (size is not None and len(raw)!=size):
        raise ValueError('Regenerated mesh differs from pinned receipt')
    return raw


def hydrate(root):
    for name,item in json.loads((root/'omitted-payloads.json').read_text())['files'].items():
        path=root/name
        if Path(name).is_absolute() or '..' in Path(name).parts or path.exists():raise ValueError('unsafe hydration destination')
        path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(mesh_bytes(item['sha256'],item['bytes']))
