import re, sys
from pathlib import Path
P=Path(sys.argv[1])
ver=''.join((P/f).read_text() for f in ('verify.py','verify_exp2.py','verify_exp2b.py','verify_exp2c.py'))
for f in ('main.tex','optimization.tex'):
    lines=(P/f).read_text().splitlines()
    for i,l in enumerate(lines,1):
        if l.lstrip().startswith('%') or '\\href' in l or 'includegraphics' in l: continue
        for m in re.finditer(r'(?<![\w.{])(\d[\d,]*\.\d+|\d{2,}[\d,]*)(?![\w}])', l):
            tok=m.group(1)
            if tok in ver or tok.replace(',', '{,}') in ver: continue
            if re.fullmatch(r'(19|20)\d\d',tok): continue
            print(f'{f}:{i}: {tok}  | {l.strip()[:110]}')
