# figures/setup_schematic — source stub

Placeholder spec for the setup schematic (rendered to
`figures/setup_schematic.pdf` by `paper-figures`). No real geometry render
exists yet; this file documents what the schematic must show so the figurer can
draw it (TikZ or a mesh screenshot from the `bent_conformal` fixture).

Contents to depict (all quantities dimensionless / natural units, as recorded in
`benchmarks/patch_antenna_conformal/conformal_results.toml`):

- An FR-4 + PEC patch/ground slab wrapped around a cylinder of bend radius 40
  (about the y-axis) -> genuinely CURVED conformal metal.
- A matched box-UPML absorber shell of thickness 8 surrounding the radiator.
- A pinned-feed lumped port on the x=0 plane (port resistance 50).
- The 73 free patch-conductor nodes moving along their radial normals (the
  design DOFs); PML shell, port plane, ground plane, and outer boundary held
  fixed.

Do NOT label any frequency or dimension in GHz or mm-as-physical — keep the
natural-unit / dimensionless convention of the artifact.
