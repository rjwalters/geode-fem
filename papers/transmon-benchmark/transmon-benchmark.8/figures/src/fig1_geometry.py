#!/usr/bin/env python3
"""Actual conductor outlines plus junction detail and a labeled material schematic."""
from figure_style import *
from geometry_mesh import parse_msh41, conductors, bbox, outline
from matplotlib.collections import PolyCollection, LineCollection
from matplotlib.patches import Rectangle

def main():
    tris=parse_msh41(REPO/'crates/geode-core/tests/fixtures/transmon_smoke.msh')
    parts=conductors(tris['metal'])
    fig=plt.figure(figsize=(6.4,4.8))
    a=fig.add_axes([.10,.20,.52,.68]); b=fig.add_axes([.72,.54,.25,.33]); c=fig.add_axes([.69,.12,.29,.22])
    a.add_collection(PolyCollection(parts['ground'],facecolors='#E8EDF0',edgecolors='face',linewidths=.1))
    a.add_collection(LineCollection(outline(parts['ground']),colors='#697780',linewidths=.45))
    for role,color in [('feedline',TEAL),('island',BLUE)]:
        a.add_collection(PolyCollection(parts[role],facecolors=color,edgecolors=color,linewidths=.3))
    a.add_patch(Rectangle((-2000,-707),4000,3700,fill=False,ls='--',ec=GRAY,lw=.7))
    def label(text, xy, xytext, color='black'):
        a.annotate(text,xy,xytext=xytext,color=color,fontsize=9,
                   arrowprops={'arrowstyle':'-','color':color,'lw':.7},
                   bbox={'facecolor':'white','edgecolor':'none','pad':1.2})
    label('Feedline',(-750,2267),(-1900,2660),TEAL)
    label('Readout\nresonator',(180,1600),(650,1780))
    label('Qubit island',(0,350),(-1750,680),BLUE)
    label('Junction',(0,6),(550,-430),ORANGE)
    a.text(850,700,'Ground',fontsize=9,color=GRAY)
    for key in ['port_1','port_2']:
        x0,x1,y0,y1=bbox(tris[key]); a.plot((x0+x1)/2,(y0+y1)/2,'s',mfc='white',mec=TEAL,ms=4)
    a.set(xlim=(-2150,2150),ylim=(-820,3100),xlabel='x (µm)',ylabel='y (µm)',title='(a) Device layout')
    a.set_aspect('equal'); a.set_xticks([-2000,0,2000]); a.set_yticks([0,1000,2000,3000])
    for role,color in [('ground','#E8EDF0'),('island',BLUE)]:
        b.add_collection(PolyCollection(parts[role],facecolors=color,edgecolors='face',linewidths=.1))
    b.add_collection(PolyCollection(tris['lumped_element'],facecolors=ORANGE,edgecolors='white',linewidths=.5))
    b.set(xlim=(-2.2,2.2),ylim=(4,9),xlabel='x (µm)',ylabel='y (µm)',title='(b) Junction sheet')
    b.set_aspect('equal'); b.set_xticks([-2,0,2]); b.set_yticks([4,6,8])
    b.annotate('1 × 1 µm',(0,6),xytext=(.5,4.3),fontsize=9,color=ORANGE,
               arrowprops={'arrowstyle':'-','color':ORANGE,'lw':.7})
    c.add_patch(Rectangle((0,0),1,.42,fc='#D7E5EE',ec='none'))
    c.add_patch(Rectangle((0,.42),1,.55,fc='#F5F7F8',ec='none'))
    c.plot([0,.43],[.42,.42],color=GRAY,lw=3); c.plot([.57,1],[.42,.42],color=BLUE,lw=3)
    c.text(.5,.69,'Vacuum',ha='center'); c.text(.5,.18,'Sapphire',ha='center')
    c.annotate('Metal sheets',(.12,.42),xytext=(.05,1.02),ha='left',fontsize=9,
               arrowprops={'arrowstyle':'-','lw':.7})
    c.set(xlim=(0,1),ylim=(0,1.25)); c.axis('off')
    c.set_title('(c) Cross-section',fontsize=10,pad=7)
    fig.text(.835,.06,'Schematic; not to scale',ha='center',fontsize=8.5,color=GRAY)
    save(fig,'fig1-geometry.pdf')

if __name__=='__main__': main()
