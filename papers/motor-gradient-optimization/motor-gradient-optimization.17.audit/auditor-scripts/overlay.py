import os, sys, json
src, dst, victim = sys.argv[1], sys.argv[2], sys.argv[3]
for root, dirs, files in os.walk(src):
    rel = os.path.relpath(root, src); d = os.path.join(dst, rel); os.makedirs(d, exist_ok=True)
    for f in files:
        s = os.path.join(root, f); t = os.path.join(d, f)
        if os.path.relpath(s, src) == victim:
            j = json.load(open(s))
            # perturb the first float found in a 'result'-like top-level numeric field
            def bump(o):
                if isinstance(o, dict):
                    for k, v in o.items():
                        if k != 'config':
                            if isinstance(v, float): o[k] = v * 1.001; return k
                            r = bump(v)
                            if r: return k + '.' + r
                elif isinstance(o, list):
                    for i, v in enumerate(o):
                        if isinstance(v, float): o[i] = v * 1.001; return str(i)
                        r = bump(v)
                        if r: return str(i) + '.' + r
            print('perturbed', bump(j)); json.dump(j, open(t, 'w'), indent=2)
        else:
            os.symlink(s, t)
