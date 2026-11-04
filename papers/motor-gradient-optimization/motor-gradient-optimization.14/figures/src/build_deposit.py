"""Build the deposit-ready raw-trace archive of both motor-paper studies (not uploaded).

Every inventory-listed file is hash-checked against its inventory before it is added.
The tar is deterministic: sorted member order, mtime 0, uid/gid 0, mode 0644/0755,
no user/group names, streamed through zstd -19 --long=27.
"""
import hashlib, io, json, subprocess, sys, tarfile
from pathlib import Path

DATA = Path(sys.argv[1]); OUT = Path(sys.argv[2])
SETS = [('exp2-main', DATA / 'exp2/provenance/exp2-main-inventory-0650f4ee.sha256.txt',
         Path('/Volumes/Stripe/scratch-codex-review/exp2-main-recovery-paper3-20261019'),
         [DATA / 'exp2/provenance' / n for n in ('exp2-main-inventory-0650f4ee.sha256.txt.ots', 'inv.digicert.tsr', 'inv.freetsa.tsr', 'inv.tsq')]),
        ('exp2-fd-ablation', DATA / 'exp2b/provenance/exp2-fd-ablation-inventory-c2bc65ff.sha256.txt',
         Path('/Volumes/Stripe/geode-results/exp2-fd-ablation-20261021/copy'),
         sorted((DATA / 'exp2b/provenance').glob('exp2-fd-ablation-inventory-c2bc65ff.sha256.txt.*'))),
        ('exp2-fd-warm', DATA / 'exp2c/provenance/exp2-fd-warm-inventory-df509322.sha256.txt',
         Path('/Volumes/Stripe/geode-results/exp2-fd-warm-20261021/copy'),
         sorted((DATA / 'exp2c/provenance').glob('exp2-fd-warm-inventory-df509322.sha256.txt.*')))]
ROOT = 'motor-paper-raw-traces'
README = f"""Raw traces of the three optimization sessions of the GEODE motor-gradient-optimization paper.

Layout:
  inventories/<study>/   the SHA-256 inventory taken on each study host before it stopped,
                         with its RFC 3161 tokens (.tsr, request .tsq) and OpenTimestamps receipt (.ots)
  exp2-main/...          the preregistered matched study: 5,453 files listed in
                         exp2-main-inventory-0650f4ee.sha256.txt (digest 0650f4ee779b64c8efc32020571657f3009f3fe46a52c8adc9f8ffdf28ea6e16)
  exp2-fd-ablation/...   the post-hoc finite-difference SLSQP ablation (cold stencils): 2,118 files listed in
                         exp2-fd-ablation-inventory-c2bc65ff.sha256.txt (digest c2bc65ff61fb8485d14a1e2b07dadb3f9c9fa4b040ce98aa1360f46f64788a83)
  exp2-fd-warm/...       the post-hoc warm-stencil arm F_w: 1,180 files listed in
                         exp2-fd-warm-inventory-df509322.sha256.txt (digest df5093225b52b74f622d4bfbd193c58ea24d6116eadb2f0a03b9e9830e8a2559)

Check: from {ROOT}/, `shasum -a 256 -c inventories/exp2-main/exp2-main-inventory-0650f4ee.sha256.txt` and the same for the ablation.
Every file was hash-checked against its inventory when this archive was built. The paper package's
verify_exp2b.py re-derives the ablation extracts from exp2-fd-ablation/session when pointed at it.
"""


def add_bytes(tf, name, data):
    ti = tarfile.TarInfo(name); ti.size = len(data); ti.mtime = 0; ti.mode = 0o644
    tf.addfile(ti, io.BytesIO(data))


zst = subprocess.Popen(['zstd', '-19', '--long=27', '-T0', '-q', '-o', str(OUT), '-'], stdin=subprocess.PIPE)
counts = {}
with tarfile.open(fileobj=zst.stdin, mode='w|', format=tarfile.PAX_FORMAT) as tf:
    add_bytes(tf, f'{ROOT}/README.txt', README.encode())
    for study, inv, base, extras in SETS:
        add_bytes(tf, f'{ROOT}/inventories/{study}/{inv.name}', inv.read_bytes())
        for e in extras:
            add_bytes(tf, f'{ROOT}/inventories/{study}/{e.name}', e.read_bytes())
        rows = sorted(l.split('  ', 1)[::-1] for l in inv.read_text().splitlines())
        for rel, digest in rows:
            p = base / rel; data = p.read_bytes()
            assert hashlib.sha256(data).hexdigest() == digest, rel
            ti = tarfile.TarInfo(f'{ROOT}/{rel}'); ti.size = len(data); ti.mtime = 0
            ti.mode = 0o755 if p.stat().st_mode & 0o111 else 0o644
            tf.addfile(ti, io.BytesIO(data))
        counts[study] = len(rows)
zst.stdin.close(); assert zst.wait() == 0
digest = hashlib.sha256(OUT.read_bytes()).hexdigest() if OUT.stat().st_size < 2**34 else None
(OUT.parent / (OUT.name + '.sha256')).write_text(f'{digest}  {OUT.name}\n')
(OUT.parent / 'deposit-record.json').write_text(json.dumps({'archive': OUT.name, 'bytes': OUT.stat().st_size, 'sha256': digest,
    'files': counts, 'uploaded': False, 'note': 'Deposit-ready; not uploaded. The operator controls deposits.'}, indent=1) + '\n')
print(json.dumps({'bytes': OUT.stat().st_size, 'sha256': digest, 'files': counts}))
