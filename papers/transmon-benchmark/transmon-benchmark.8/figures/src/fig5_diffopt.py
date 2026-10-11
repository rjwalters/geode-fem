#!/usr/bin/env python3
"""Recorded refined P2 design, validation, and actual island surface outlines.
Inputs are immutable snapshots from #1058; provenance.json records source commit.
"""
from figure_style import *
from geometry_mesh import parse_msh41, conductors, outline
from matplotlib.collections import LineCollection

def main():
    d=recorded('refined_results.json'); v=recorded('refined_validation.json'); design=recorded('refined_design.json')
    rows=[r for r in d['history'] if r['kind']=='optimization']
    target=d['target_scalar_capacitance_ff']
    fig=plt.figure(figsize=(6.4,4.8))
    a=fig.add_axes([.10,.54,.34,.34]); b=fig.add_axes([.70,.54,.28,.34]); c=fig.add_axes([.10,.13,.87,.20])
    a.axhline(target,color=ORANGE,ls='--',lw=1)
    a.plot([r['step'] for r in rows],[r['c_ff'] for r in rows],'-o',color=BLUE,ms=5)
    a.annotate(f"{rows[0]['c_ff']:.3f}",(0,rows[0]['c_ff']),xytext=(6,-2),textcoords='offset points',fontsize=9)
    a.annotate(f"{rows[-1]['c_ff']:.4f}",(3,rows[-1]['c_ff']),xytext=(-3,9),textcoords='offset points',ha='right',fontsize=9)
    a.text(.04,.88,'Target: 89.9 fF',transform=a.transAxes,color=ORANGE,fontsize=9)
    a.set(xticks=[0,1,2,3],ylim=(76,94),xlabel='Accepted gradient step',ylabel=r'$C_\Sigma$ (fF)',title='(a) Refined P2 optimization')
    b.axvspan(target*.99,target*1.01,color='#EEF4F6')
    b.axvline(target,color=ORANGE,ls='--',lw=1)
    values=[v['optimized_ff'],v['palace_p2_ff'],v['held_out_ff']]
    for i,(x,col,m) in enumerate(zip(values,[BLUE,TEAL,GRAY],['o','x','s'])):
        b.plot(x,i,m,color=col,ms=6)
        b.annotate(f'{x:.4f}',(x,i),xytext=(5,10),textcoords='offset points',fontsize=9,bbox={'facecolor':'white','edgecolor':'none','pad':1})
    b.set(yticks=[0,1,2],yticklabels=['Optimization','Palace P2','Finer mesh'],xlim=(88.8,91),ylim=(2.5,-.7),xlabel=r'$C_\Sigma$ (fF)',title='(b) Fixed-design checks')
    b.set_xticks([89,90,91]); b.text(.5,-.32,'Shading: ±1% working tolerance',transform=b.transAxes,ha='center',fontsize=8.5)
    initial=conductors(parse_msh41(REPO/'crates/geode-core/tests/fixtures/transmon_smoke.msh')['metal'])['island']
    final=recorded('optimized_island.json')['triangles']
    for triangles,color,ls,label in [(final,BLUE,'-','Optimized'),(initial,GRAY,'--','Initial')]:
        # Rotate the actual top-down outline: horizontal y, vertical x.
        segs=[[(pt[1],pt[0]) for pt in edge] for edge in outline(triangles)]
        c.add_collection(LineCollection(segs,colors=color,linestyles=ls,linewidths=1.3,label=label))
    c.plot([6,6],[-.5,.5],color=ORANGE,lw=2)
    c.set(xlim=(-10,655),ylim=(-43,43),xlabel='Distance along island, y (µm)',ylabel='x (µm)',title='(c) Actual island outlines; junction end held fixed')
    c.set_yticks([-20,0,20]); c.set_aspect('equal',adjustable='box')
    c.legend(loc='upper left',bbox_to_anchor=(.05,1.04),ncol=2,frameon=False,fontsize=8.5)
    fig.text(.54,.035,f"Length: 625.50 → {design['island_length_um']:.2f} µm    Width: 24.00 → {design['pad_width_um']:.2f} µm",ha='center',fontsize=9)
    save(fig,'fig5-diffopt.pdf')

if __name__=='__main__': main()
