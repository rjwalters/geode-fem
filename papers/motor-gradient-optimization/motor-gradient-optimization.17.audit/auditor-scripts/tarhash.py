import sys, tarfile, hashlib, json
# usage: zstd -dc archive | python3 -I tarhash.py out.json
out = {}
with tarfile.open(fileobj=sys.stdin.buffer, mode='r|') as tf:
    for m in tf:
        if m.isfile():
            f = tf.extractfile(m); h = hashlib.sha256()
            for b in iter(lambda: f.read(1 << 20), b''): h.update(b)
            out[m.name] = h.hexdigest()
        elif not m.isdir():
            out[m.name] = 'NONREG:' + str(m.type)
json.dump(out, open(sys.argv[1], 'w'), indent=0, sort_keys=True)
print(len(out))
