# Drawing descriptions — acme-widget-prov.1

Drawing stubs (no rendered figures vendored; descriptions only). Reference
numerals correspond to `spec.tex`.

## FIG. 1 — System block diagram
- Type: block diagram.
- Components shown: sense bridge (10), dummy half-bridge (20), span trim resistor
  (30), split-path excitation network (40), summing node (50), fixed divider (52).
- Spatial relationships: excitation network (40) feeds sense bridge (10); dummy
  half-bridge (20) feeds fixed divider (52) into summing node (50); summing node
  (50) drives span trim resistor (30) to the output.
- Lead-line annotations: each block labeled with its numeral.

## FIG. 2 — Split-path excitation network schematic
- Type: circuit schematic (detail of 40).
- Components shown: constant-current source (42), PTAT current source (44), ratio
  resistor (12) in the PTAT reference leg, summing into the high node of sense
  bridge (10).
- Spatial relationships: sources (42) and (44) sum at the bridge high node; ratio
  resistor (12) sets the PTAT slope.
- Lead-line annotations: 42, 44, 12, 10.

## FIG. 3 — Die-layout plan view
- Type: plan view of the MEMS die (the §4.2 integrated embodiment).
- Components shown: sense bridge (10) on the diaphragm; dummy half-bridge (20)
  adjacent, outside the flexing region.
- Spatial relationships: dummy half-bridge (20) placed immediately adjacent to
  the sense bridge (10) on unstrained die.
- Lead-line annotations: 10, 20.
