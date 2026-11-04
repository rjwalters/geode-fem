import json, sys, time, urllib.request, urllib.parse
R = json.load(open(sys.argv[1])); out = {}
UA = {'User-Agent': 'anvil-paper-audit/1.0'}  # no email, no mailto, no identifier
for k in sys.argv[3].split(','):
    doi = R[k]['bib'].get('doi')
    try:
        req = urllib.request.Request('https://api.openalex.org/works/doi:' + urllib.parse.quote(doi), headers=UA)
        o = json.loads(urllib.request.urlopen(req, timeout=30).read())
        inv = o.get('abstract_inverted_index') or {}
        words = sorted(((p, w) for w, ps in inv.items() for p in ps))
        out[k] = ' '.join(w for _, w in words)
    except Exception as e: out[k] = 'ERROR ' + repr(e)
    time.sleep(0.3)
json.dump(out, open(sys.argv[2], 'w'), indent=1, ensure_ascii=False)
for k, v in out.items(): print(f'### {k}: {v[:900]}\n')
