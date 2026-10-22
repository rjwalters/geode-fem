import json, re, sys, time, urllib.request, urllib.parse
from pathlib import Path
bib = Path(sys.argv[1]).read_text(); out = Path(sys.argv[2])
UA = {'User-Agent': 'anvil-paper-audit/1.0'}   # no email, no mailto, no identifier
ents = re.findall(r'@(\w+)\{([^,]+),(.*?)\n\}', bib, re.S)
res = {}
def get(url):
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=30) as r: return json.loads(r.read())
for typ, key, body in ents:
    f = dict((k.lower(), v.strip()) for k, v in re.findall(r'(\w+)\s*=\s*\{(.*?)\},?\s*$', body, re.M))
    doi = f.get('doi'); rec = {'type': typ, 'bib': f}
    if doi:
        try:
            m = get('https://api.crossref.org/works/' + urllib.parse.quote(doi))['message']
            rec['crossref'] = {'title': (m.get('title') or [''])[0], 'year': (m.get('issued', {}).get('date-parts') or [[None]])[0][0],
                               'print_year': ((m.get('published-print') or {}).get('date-parts') or [[None]])[0][0],
                               'container': (m.get('container-title') or [''])[0], 'volume': m.get('volume'), 'issue': m.get('issue'),
                               'page': m.get('page') or m.get('article-number'), 'authors': [a.get('family') for a in m.get('author', [])]}
        except Exception as e:
            rec['crossref_error'] = repr(e)
            try:
                o = get('https://api.openalex.org/works/doi:' + urllib.parse.quote(doi))
                rec['openalex'] = {'title': o.get('title'), 'year': o.get('publication_year'), 'authors': [a['author']['display_name'] for a in o.get('authorships', [])]}
            except Exception as e2: rec['openalex_error'] = repr(e2)
        time.sleep(0.3)
    res[key] = rec
out.write_text(json.dumps(res, indent=1, ensure_ascii=False))
print(len(res), 'entries;', sum('crossref' in r for r in res.values()), 'crossref ok;', [k for k, r in res.items() if 'crossref_error' in r])
