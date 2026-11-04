"""Build the deposit-ready raw-trace archive of the motor-gradient-optimization paper (not uploaded).

v17: besides the three sessions' inventory-listed raw traces, the archive holds the three
final-refinement cohorts (bundle/, tools/, results/, report.json) and the binary-equivalence
record (with the rebuilt binary), so that the package's verifiers can re-derive every run,
timeline and final-check extract from the unpacked archive alone. MANIFEST.sha256 lists the
SHA-256 of every other member; a copy is kept in the package (figures/src/data/deposit/).

Every inventory-listed file is hash-checked against its inventory before it is added.
The tar is deterministic: sorted member order, mtime 0, uid/gid 0, mode 0644/0755,
no user/group names, streamed through zstd -19 --long=27.

Usage: python3 -I build_deposit.py <package>/figures/src/data <out>/motor-paper-raw-traces.tar.zst
"""
import hashlib, io, json, subprocess, sys, tarfile
from pathlib import Path

DATA = Path(sys.argv[1]); OUT = Path(sys.argv[2])
SCR = Path('/Volumes/Stripe/scratch-codex-review')
SETS = [('exp2-main', DATA / 'exp2/provenance/exp2-main-inventory-0650f4ee.sha256.txt',
         SCR / 'exp2-main-recovery-paper3-20261019',
         [DATA / 'exp2/provenance' / n for n in ('exp2-main-inventory-0650f4ee.sha256.txt.ots', 'inv.digicert.tsr', 'inv.freetsa.tsr', 'inv.tsq')]),
        ('exp2-fd-ablation', DATA / 'exp2b/provenance/exp2-fd-ablation-inventory-c2bc65ff.sha256.txt',
         Path('/Volumes/Stripe/geode-results/exp2-fd-ablation-20261021/copy'),
         sorted((DATA / 'exp2b/provenance').glob('exp2-fd-ablation-inventory-c2bc65ff.sha256.txt.*'))),
        ('exp2-fd-warm', DATA / 'exp2c/provenance/exp2-fd-warm-inventory-df509322.sha256.txt',
         Path('/Volumes/Stripe/geode-results/exp2-fd-warm-20261021/copy'),
         sorted((DATA / 'exp2c/provenance').glob('exp2-fd-warm-inventory-df509322.sha256.txt.*')))]
# Final-refinement cohorts: (archive folder, source folder, members). The mesh cache is not
# included: no extract reads it, and the repository's regenerate_meshes.py (commit c0dae78d)
# regenerates its four meshes from source commit 63b3e3d8 and checks their hashes.
# The F_w cohort is the second attempt (...-20261021b); the first produced no results.
REFINE = [('refinement/exp2-main', SCR / 'exp2-refine-direct-20261020', ('bundle', 'tools', 'results', 'report.json')),
          ('refinement/exp2-fd-ablation', SCR / 'exp2-fd-refine-direct-20261021', ('bundle', 'tools', 'results', 'report.json')),
          ('refinement/exp2-fd-warm', SCR / 'exp2-fdwarm-refine-direct-20261021b', ('bundle', 'tools', 'results', 'report.json')),
          ('refinement/binary-equivalence', SCR / 'refine-binary-equivalence-20261021',
           ('equivalence.txt', 'icem_final_refinement.c75fe847', 'one.sh', 'results', 'sample.txt'))]
EXTRA = [('refinement/mesh-cache-regeneration.json', SCR / 'exp2-refine-direct-20261020/mesh-cache/regeneration.json'),
         # the independent selection program's output for each ablation (selection-check.json is derived from it)
         ('refinement/exp2-fd-ablation/selection.json', Path('/Volumes/Stripe/geode-results/exp2-fd-ablation-20261021/selection/selection.json')),
         ('refinement/exp2-fd-warm/selection.json', Path('/Volumes/Stripe/geode-results/exp2-fd-warm-20261021/selection/selection.json'))]
ROOT = 'motor-paper-raw-traces'
PKG = 'https://github.com/rjwalters/geode-fem/tree/main/papers/motor-gradient-optimization/motor-gradient-optimization.17'
README = f"""Raw traces and final-refinement results of the GEODE motor-gradient-optimization paper.

Paper package (verifiers, extracts, inventories): {PKG}

Layout:
  MANIFEST.sha256        SHA-256 of every other file in this archive (shasum -a 256 -c MANIFEST.sha256)
  inventories/<study>/   the SHA-256 inventory taken on each study host before it stopped,
                         with its RFC 3161 tokens (.tsr, request .tsq) and OpenTimestamps receipt (.ots)
  exp2-main/...          the preregistered matched study: 5,453 files listed in
                         exp2-main-inventory-0650f4ee.sha256.txt (digest 0650f4ee779b64c8efc32020571657f3009f3fe46a52c8adc9f8ffdf28ea6e16)
  exp2-fd-ablation/...   the post-hoc finite-difference SLSQP ablation (cold stencils): 2,118 files listed in
                         exp2-fd-ablation-inventory-c2bc65ff.sha256.txt (digest c2bc65ff61fb8485d14a1e2b07dadb3f9c9fa4b040ce98aa1360f46f64788a83)
  exp2-fd-warm/...       the post-hoc warm-stencil arm F_w: 1,180 files listed in
                         exp2-fd-warm-inventory-df509322.sha256.txt (digest df5093225b52b74f622d4bfbd193c58ea24d6116eadb2f0a03b9e9830e8a2559)
  refinement/exp2-main/, refinement/exp2-fd-ablation/, refinement/exp2-fd-warm/
                         the final-check cohort of each session: bundle/ (cohort.json and the requests),
                         tools/ (prepare, run and report scripts), results/ (one folder per design and check)
                         and report.json; for the two ablations also selection.json, the output of the
                         independent selection program
  refinement/binary-equivalence/
                         the rebuilt refinement binary (icem_final_refinement.c75fe847, built from
                         commit 63b3e3d8), the script that reran twelve checks of the
                         exp2-fd-ablation cohort with it, its results and the field comparison
  refinement/mesh-cache-regeneration.json
                         digests of the four meshes the refinement program reads; the meshes are not
                         included (no extract reads them); the GEODE repository regenerates them from
                         source commit 63b3e3d8 and checks their hashes with
                         benchmarks/motor_optimization/final_refinement/smoke/regenerate_meshes.py
                         (see REGENERATE.md there; commit c0dae78d)

Paths inside the tools/ scripts and one.sh are those of the machine that ran them.

Re-derivation: unpack this archive, download the paper package, and from the package run
  MOTOR_RAW_ROOT=<unpacked>/{ROOT} python3 -I -B verify.py
The verifiers then check every file here against MANIFEST.sha256 and re-derive, exactly, the run
extracts, evaluation timelines and final-check extracts of all three sessions, the two ablations'
selection checks and the binary-equivalence comparison from these raw traces and results.
"""


def add_bytes(tf, name, data, mode=0o644):
    ti = tarfile.TarInfo(name); ti.size = len(data); ti.mtime = 0; ti.mode = mode
    tf.addfile(ti, io.BytesIO(data))


def tree(base, names):
    for n in names:
        p = base / n
        assert not p.is_symlink(), p
        if p.is_dir():
            for q in sorted(p.rglob('*')):
                assert not q.is_symlink(), q
                if q.is_file():
                    yield q
        else:
            yield p


# Pass 1: every member's path, source and digest; the manifest is written first.
members = []  # (archive path, source path or bytes, digest)
members.append((f'{ROOT}/README.txt', README.encode(), hashlib.sha256(README.encode()).hexdigest()))
counts = {}
for study, inv, base, extras in SETS:
    for e in [inv] + list(extras):
        members.append((f'{ROOT}/inventories/{study}/{e.name}', e, hashlib.sha256(e.read_bytes()).hexdigest()))
    rows = sorted(l.split('  ', 1)[::-1] for l in inv.read_text().splitlines())
    members += [(f'{ROOT}/{rel}', base / rel, digest) for rel, digest in rows]
    counts[study] = len(rows)
for dst, base, names in REFINE:
    files = list(tree(base, names))
    members += [(f'{ROOT}/{dst}/{q.relative_to(base).as_posix()}', q, hashlib.sha256(q.read_bytes()).hexdigest()) for q in files]
    counts[dst] = len(files)
for dst, src in EXTRA:
    members.append((f'{ROOT}/{dst}', src, hashlib.sha256(src.read_bytes()).hexdigest()))
names = [m[0] for m in members]; assert len(names) == len(set(names))
manifest = ''.join(f'{d}  {n[len(ROOT) + 1:]}\n' for n, _, d in sorted(members, key=lambda m: m[0])).encode()

zst = subprocess.Popen(['zstd', '-19', '--long=27', '-T0', '-q', '-f', '-o', str(OUT), '-'], stdin=subprocess.PIPE)
with tarfile.open(fileobj=zst.stdin, mode='w|', format=tarfile.PAX_FORMAT) as tf:
    add_bytes(tf, f'{ROOT}/MANIFEST.sha256', manifest)
    for name, src, digest in members:
        if isinstance(src, bytes):
            add_bytes(tf, name, src); continue
        data = src.read_bytes()
        assert hashlib.sha256(data).hexdigest() == digest, name  # inventory rows: checked here, against the inventory
        add_bytes(tf, name, data, 0o755 if src.stat().st_mode & 0o111 else 0o644)
zst.stdin.close(); assert zst.wait() == 0
h = hashlib.sha256()
with open(OUT, 'rb') as fh:
    for chunk in iter(lambda: fh.read(1 << 24), b''):
        h.update(chunk)
digest = h.hexdigest()
(OUT.parent / (OUT.name + '.sha256')).write_text(f'{digest}  {OUT.name}\n')
(OUT.parent / 'MANIFEST.sha256').write_bytes(manifest)
(OUT.parent / 'deposit-record.json').write_text(json.dumps({'archive': OUT.name, 'bytes': OUT.stat().st_size, 'sha256': digest,
    'files': counts, 'members': len(members) + 1, 'manifest_sha256': hashlib.sha256(manifest).hexdigest(),
    'uploaded': False, 'note': 'Deposit-ready; not uploaded. The operator controls deposits.'}, indent=1) + '\n')
print(json.dumps({'bytes': OUT.stat().st_size, 'sha256': digest, 'files': counts, 'members': len(members) + 1}))
