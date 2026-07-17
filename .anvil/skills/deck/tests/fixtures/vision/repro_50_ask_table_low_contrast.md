---
marp: true
size: 16:9
theme: anvil-deck
---

<!-- _class: ask -->

# Raising $2M seed

## Use of funds (18 months · ~$111K/mo blended burn)

| %   | Bucket               | Detail                                                    |
|-----|----------------------|-----------------------------------------------------------|
| 50% | Founding team        | Hardware, ID, electromech, ML, content                    |
| 20% | Otium v0 prototype   | Industrial design locked + electromech proof of concept   |
| 15% | Aldus Mobile alpha   | App + ingest pipeline + early reader telemetry            |
| 10% | Concierge pilot      | 8 households at \$5k cap, dedicated success               |
| 5%  | Display partnership  | Joint dev with reflective display vendor                  |

**Series A gate**: Otium pilot delivered · app at 100K MAU · pre-orders covering Yr 1 production · display partnership executed.

<!--
Repro for #50: if a future theme change removes the `section.ask table`
overrides in `anvil-deck.css`, the data cells render white-on-white on
the navy ask background. The imported Marp `default` theme paints
data-cell backgrounds light, the `section.ask` cascade recolors text to
`#ffffff`, and without the `section.ask table th/td` overrides setting
`background: transparent` the funding-breakdown rows become invisible.

A vision critic SHOULD score `palette_adherence` low (0–1) and emit a
finding describing low-contrast (white-on-white) table cells on the
`_class: ask` slide. The fix points back at the `section.ask table` CSS
overrides shipped in PR #55.

Companion to `test_ask_table_css.py`, which guards the CSS source side
(rule presence in `anvil-deck.css`); this fixture exercises the
rendered-side detection path via the deck-vision critic.
-->
