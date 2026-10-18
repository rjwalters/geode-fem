"""Verify producer files against an independently chosen Git revision.

The expected commit is a reviewed input, not inferred from a capture's declaration.
This verifies bytes and clean source state. It is not independent review, a signed
build attestation, or proof that a particular process executed those bytes.
"""
from contextlib import contextmanager
import hashlib
import importlib.abc
import importlib.util
from pathlib import Path, PurePosixPath
import subprocess
import sys

SOURCE_PATHS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates',
                'benchmarks/motor_optimization', 'benchmarks/motor_reference',
                'benchmarks/motor_shape_gradient')
HARNESS = 'benchmarks/motor_optimization/harness/'


def git(repo, *args):
    result = subprocess.run(['git', '-C', str(repo), *args], capture_output=True, check=False)
    if result.returncode:
        raise ValueError('producer Git verification failed: ' + result.stderr.decode(errors='replace').strip())
    return result.stdout


def regular(path):
    path = Path(path)
    if not path.is_absolute() or any(p.is_symlink() for p in (path, *path.parents)) or not path.is_file():
        raise ValueError('producer artifact is missing or redirected: ' + str(path))
    return path


def digest(path):
    h = hashlib.sha256()
    with regular(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def committed_files(repo, commit):
    """Hash every registered Git blob, checking modes and complete path census."""
    if (type(commit) is not str or len(commit) != 40
            or any(c not in '0123456789abcdef' for c in commit)):
        raise ValueError('expected reviewed commit must be a full object ID')
    rows = git(repo, 'ls-tree', '-rz', '--full-tree', commit, '--', *SOURCE_PATHS)
    objects = []
    for row in filter(None, rows.split(b'\0')):
        metadata, raw_name = row.split(b'\t', 1)
        mode, kind, oid = metadata.decode().split()
        name = raw_name.decode()
        p = PurePosixPath(name)
        if (mode not in ('100644', '100755') or kind != 'blob' or p.is_absolute()
                or '..' in p.parts or str(p) != name):
            raise ValueError('unsupported producer source path/mode')
        objects.append((name, oid))
    if not objects or len({name for name, _ in objects}) != len(objects):
        raise ValueError('empty or repeated producer source census')
    hashes = {}
    with subprocess.Popen(['git', '-C', str(repo), 'cat-file', '--batch'],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as proc:
        try:
            for name, oid in objects:
                proc.stdin.write((oid+'\n').encode()); proc.stdin.flush()
                header = proc.stdout.readline().decode().strip().split()
                if len(header) != 3 or header[:2] != [oid, 'blob']:
                    raise ValueError('Git blob is missing or changed')
                remaining = int(header[2]); h = hashlib.sha256()
                while remaining:
                    block = proc.stdout.read(min(remaining, 1 << 20))
                    if not block:
                        raise ValueError('truncated Git blob')
                    h.update(block); remaining -= len(block)
                if proc.stdout.read(1) != b'\n':
                    raise ValueError('invalid Git blob boundary')
                hashes[name] = h.hexdigest()
            proc.stdin.close()
            if proc.wait(timeout=30):
                raise ValueError('Git blob reader failed')
        except BaseException:
            proc.kill(); proc.wait()
            raise
    return hashes


def verify(repo, expected, *, reviewed_commit, check_worktree=True):
    """Compare the complete declared source map with trusted Git and live files.

    Offline consumers set check_worktree=False, checking the captured source map
    against Git without requiring their own checkout to be the producer revision.
    """
    repo = Path(repo).resolve(strict=True)
    hashes = committed_files(repo, reviewed_commit)
    if (type(expected) is not dict or expected.get('commit') != reviewed_commit
            or expected.get('tree_clean') is not True or expected.get('untracked_sources') != []
            or expected.get('files') != hashes):
        raise ValueError('declared producer sources differ from reviewed Git revision')
    if check_worktree:
        if (expected.get('repo') != str(repo)
                or git(repo, 'rev-parse', 'HEAD').decode().strip() != reviewed_commit):
            raise ValueError('capture is not on the original reviewed producer checkout')
        indexed = set(filter(None, git(repo, 'ls-files', '-z', '--', *SOURCE_PATHS).decode().split('\0')))
        if indexed != set(hashes):
            raise ValueError('producer index differs from reviewed source census')
        status = git(repo, 'status', '--porcelain', '-z', '--untracked-files=all', '--', *SOURCE_PATHS)
        for entry in filter(None, status.decode().split('\0')):
            if not entry.startswith('?? '):
                raise ValueError('producer has modified tracked sources')
            path = Path(entry[3:])
            if (path.suffix in ('.py', '.rs', '.toml', '.sh', '.geo', '.cfg', '.ini')
                    or path.name in ('Cargo.lock', 'requirements.lock', 'rust-toolchain')):
                raise ValueError('producer has untracked executable source')
        for name, expected_hash in hashes.items():
            if digest(repo/name) != expected_hash:
                raise ValueError('producer file differs from reviewed Git bytes: ' + name)
    return {'reviewed_commit': reviewed_commit, 'files': hashes,
            'worktree_checked': check_worktree, 'scope': 'Git/source byte equality; no execution claim'}


class _Loader(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    def __init__(self, repo, sources):
        self.repo, self.sources = repo, sources

    def find_spec(self, fullname, path=None, target=None):
        if fullname in self.sources:
            return importlib.util.spec_from_loader(fullname, self, origin=str(self.repo/HARNESS/(fullname+'.py')))
        return None

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        name = module.__name__
        path = self.repo/HARNESS/(name+'.py')
        module.__file__ = str(path)
        # Compile the already verified source bytes; timestamp-based .pyc files
        # cannot replace a producer module during verification.
        exec(compile(self.sources[name], str(path), 'exec'), module.__dict__)


@contextmanager
def modules(repo, verified):
    """Import the verified producer's harness from source in a fresh process."""
    repo = Path(repo).resolve(strict=True)
    sources = {}
    for name, expected in verified['files'].items():
        p = PurePosixPath(name)
        if name.startswith(HARNESS) and p.suffix == '.py' and str(p.parent)+'/' == HARNESS:
            raw = regular(repo/name).read_bytes()
            if hashlib.sha256(raw).hexdigest() != expected:
                raise ValueError('producer Python source changed before import')
            sources[p.stem] = raw
    if not sources or set(sources) & sys.modules.keys():
        raise ValueError('producer imports require a fresh isolated process')
    loader = _Loader(repo, sources); old_path = list(sys.path)
    sys.meta_path.insert(0, loader)
    try:
        yield
    finally:
        sys.meta_path.remove(loader)
        sys.path[:] = old_path
        for name in sources:
            sys.modules.pop(name, None)
        for name, raw in sources.items():
            if regular(repo/HARNESS/(name+'.py')).read_bytes() != raw:
                raise ValueError('producer source changed during verification')


@contextmanager
def committed_modules(repo, commit, names):
    """Load a small declared dependency set from reviewed Git, for offline replay."""
    repo = Path(repo).resolve(strict=True)
    sources = {}
    for name in names:
        if not name.isidentifier():
            raise ValueError('invalid producer module name')
        sources[name] = git(repo, 'show', commit+':'+HARNESS+name+'.py')
    if set(sources) & sys.modules.keys():
        raise ValueError('producer replay requires fresh module identities')
    loader = _Loader(repo, sources); old_path = list(sys.path)
    sys.meta_path.insert(0, loader)
    try:
        yield
    finally:
        sys.meta_path.remove(loader)
        sys.path[:] = old_path
        for name in sources:
            sys.modules.pop(name, None)
