import json, re, sys, unicodedata
R = json.load(open(sys.argv[1]))
norm = lambda s: re.sub(r'[^a-z0-9]', '', unicodedata.normalize('NFKD', re.sub(r'\\[a-zA-Z]+|[{}\\"\'`^~]', '', s or '')).encode('ascii','ignore').decode().lower())
for k, r in R.items():
    b = r['bib']; c = r.get('crossref')
    if not c: print(f'{k}: NO CROSSREF ({r.get("crossref_error","no doi")[:60]}) openalex={r.get("openalex")}'); continue
    issues = []
    if norm(b.get('title'))[:60] != norm(c['title'])[:60]: issues.append(f"title bib={b.get('title')[:70]!r} cr={c['title'][:70]!r}")
    yrs = {c['year'], c['print_year']}
    if int(b.get('year', 0)) not in yrs: issues.append(f"year bib={b.get('year')} cr={yrs}")
    if b.get('volume') and c['volume'] and b['volume'] != c['volume']: issues.append(f"vol {b['volume']} vs {c['volume']}")
    if b.get('number') and c['issue'] and b['number'].replace('--','-') != c['issue']: issues.append(f"issue {b['number']} vs {c['issue']}")
    bp = (b.get('pages') or '').replace('--', '-'); cp = (c['page'] or '')
    if isinstance(cp, list): cp = cp[0]
    if bp and cp and bp != cp: issues.append(f"pages {bp} vs {cp}")
    fa = norm(re.split(r',| and ', b.get('author', ''))[0]); cfa = norm(c['authors'][0]) if c['authors'] else ''
    if cfa and fa != cfa: issues.append(f"first author {fa} vs {cfa}")
    na = len(re.split(r'\s+and\s+', b.get('author', ''))); 
    if 'others' not in b.get('author','') and c['authors'] and na != len(c['authors']): issues.append(f"n authors {na} vs {len(c['authors'])}")
    print(f'{k}: ' + ('OK' if not issues else ' | '.join(issues)))
