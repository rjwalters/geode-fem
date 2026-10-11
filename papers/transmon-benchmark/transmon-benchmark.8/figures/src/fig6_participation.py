#!/usr/bin/env python3
"""Stiffness participation and port EPR occupy separate axes by definition."""
from figure_style import *

def main():
    d=eigen_data(); p=d['oracles']['palace_target2']; q=d['spurious_mode']; modes=d['modes']
    fig, axes=plt.subplots(1,2,figsize=(6.4,3.1),gridspec_kw={'width_ratios':[1.45,1]})
    a,b=axes
    rows=[(q['f_ghz'],q['participation'],'D',BLUE,'Qubit')]
    rows += [(v['f_ghz'],v['participation'],'s' if k=='junction_lc' else 'o',GRAY if k=='junction_lc' else TEAL,'Port-sheet artifact' if k=='junction_lc' else None) for k,v in modes.items()]
    for f,value,marker,color,label in rows:
        a.vlines(f,0,value,color=color,lw=1.2)
        a.plot(f,value,marker,color=color,ms=5,label=label)
    a.set(xlim=(0,28),ylim=(-.04,1.09),xlabel='Frequency (GHz)',ylabel=r'GEODE stiffness participation $p$',title='(a) Port-surface energy')
    a.legend(loc='center right',frameon=True,facecolor='white',edgecolor='none',framealpha=1,fontsize=8.5)
    vals=[p['qubit_port_epr'],p['junction_lc_port_epr']]
    for i,(v,color,marker) in enumerate(zip(vals,[BLUE,GRAY],['D','s'])):
        b.plot(i,v,marker,color=color,ms=6)
        mantissa, exponent = f'{v:.2e}'.split('e')
        label=f'{v:.5f}' if i==0 else fr'${mantissa}\times10^{{{int(exponent)}}}$'
        b.annotate(label,(i,v),xytext=(0,-18 if i==0 else 10),textcoords='offset points',ha='center',fontsize=9)
    b.set_yscale('log'); b.set(xlim=(-.5,1.5),ylim=(1e-9,3),ylabel='Palace port-voltage EPR',title='(b) Port-voltage energy')
    b.set_xticks([0,1],['Qubit\n3.45 GHz','Port-sheet\n17.49 GHz'])
    b.set_yticks([1,1e-4,1e-8]); b.grid(axis='y',color='#ECEFF1')
    fig.subplots_adjust(left=.095,right=.98,bottom=.25,top=.87,wspace=.65)
    fig.text(.5,.035,'Different observables: large surface energy need not imply a port voltage.',ha='center',fontsize=9)
    save(fig,'fig6-participation.pdf')

if __name__=='__main__': main()
