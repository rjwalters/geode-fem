# transmon-benchmark.20 changelog

v19 → v20 is a clarity revision from coauthor Crutcher Dunnavant's review of v19. It covers four items.

- No number changed.
- No solver, no `cargo` and no new computation was run.
- The only new evidence the text reads is two committed files: `reference/palace/docker/Dockerfile` and the `float_dtype` field of the cross-check run records. The verifier now checks both.

## Items

| Item | Resolution |
|---|---|
| 1. Affiliation | **Done.** The author block now reads `Crutcher Dunnavant \\ {\normalsize Independent Researcher / ZSpaceLabs.ai}`. This is the exact string, in the class's `\normalsize` affiliation line, formatted like the first author's. No `[PENDING …]` marker is left; the verifier asserts that `PENDING` does not appear in the source. |
| 2. What "a qubit above" means | **Done.** Section 1's first paragraph now defines the term: "Its frequency, which we call the *qubit frequency*, is the linear eigenfrequency of the junction's (plasma) mode." The f01 footnote moved to that sentence, reworded as "Not the transmon's 0–1 transition f01, which lies below the plasma frequency by about the charging energy E_C/h". Every sentence that put "qubits" above a published value now names the frequency, with "qubit frequency" at most once per sentence. The full list is below. The abstract is 199 words, within its 200-word cap. |
| 3. Figure colours | **Done.** Figures 1, 2 and 5 were recoloured; Figures 3 and 4 needed no change and are v19's bytes. See "Figure colours" below. |
| 4. Where the shift comes from | **Done.** New Discussion paragraph, "Where the shift comes from". Section 1 points to it with "Section 9 explains why this agreement places the shift in the mesh, not in either solver or its arithmetic." See below. |

## Item 2: sentences changed

| Where | v19 | v20 |
|---|---|---|
| Abstract | "We reproduce them with Palace within 0.7% and cross-check Palace with geode-fem, an independent solver;" | "Palace reproduces them within 0.7%, and geode-fem, an independent solver, cross-checks Palace:" (the two words this frees pay for the two below) |
| Abstract | "Adaptive Palace runs, still rising, put the qubits 7.0% and 8.0% above their published values without extrapolation." | "… put the qubit frequencies 7.0% and 8.0% above their published values …" |
| Abstract | "… two refinement families put the qubits 7.3–9.6% and the resonators 2.7–3.3% higher;" | "… two refinement families put the qubit and resonator frequencies 7.3–9.6% and 2.7–3.3% higher;" |
| §1 ¶1 | "Its frequency is set mostly by that capacitance, …" | "Its frequency, which we call the *qubit frequency*, is the linear eigenfrequency of the junction's (plasma) mode.[f01 footnote] It is set mostly by that capacitance, …" |
| §1 ¶2 | "Like every frequency here, these are linear eigenfrequencies.[footnote]" | The same sentence; the footnote moved to ¶1. |
| §1 ¶3 | "Palace's adaptive refinement reaches qubits 7.0% and 8.0% above the two published values without extrapolation, and its uniform ladders, … extrapolate to qubits 7.3–9.6% above them;" | "Palace's adaptive refinement puts the two qubit frequencies 7.0% and 8.0% above their published values without extrapolation, and its uniform ladders, … extrapolate to 7.3–9.6% above them;" |
| §1 ¶4 | — | Added: "Section 9 explains why this agreement places the shift in the mesh, not in either solver or its arithmetic." |
| §1, contribution 1 | "… which puts its qubits 7.3–9.6% above their published values …" | "… which puts its qubit frequencies 7.3–9.6% above their published values …" |
| §5 Result | "The initial qubit extrapolates to 4.444 GHz and the final qubit to 3.265 GHz, 7.3% and 8.7% above the published values; both resonators extrapolate to 2.7% above theirs" | "The qubit frequency extrapolates to 4.444 GHz on the initial design and 3.265 GHz on the final one, 7.3% and 8.7% above the published values; both resonator frequencies extrapolate to 2.7% above theirs" |
| §5 Result | "… already lie 4.2% and 4.7% above the published qubit values." | "… above the published qubit frequencies." |
| §5 family b | "We therefore quote the initial qubit as 4.444–4.472 GHz, 7.3–8.0% above the published value, and the final qubit as 3.265–3.292 GHz, …" | "We therefore quote the initial qubit frequency as 4.444–4.472 GHz, 7.3–8.0% above the published value, and the final one as 3.265–3.292 GHz, …" |
| §5 adaptive | "… the qubit rises from 4.1683 to 4.4297 GHz, …" | "… the qubit frequency rises from 4.1683 to 4.4297 GHz, …" |
| §5 adaptive | "… the qubit rises from 3.0164 to 3.2468 GHz, 8.0% above the published 3.005 GHz, …" | "… the qubit frequency rises from 3.0164 to 3.2468 GHz, 8.0% above the published 3.005 GHz, …" |
| Table 6, Measured | "(adaptive qubits +7.0% and +8.0%, still rising)" | "(adaptive qubit frequencies +7.0% and +8.0%, still rising)" |
| §10 Conclusion | "… that discretization reads the qubits low: adaptive runs put the two qubits 7.0% and 8.0% above their published values …" | "… that discretization reads the qubit frequencies low: adaptive runs put them 7.0% and 8.0% above their published values …" |

These "above" sentences were left as they were, because each already names a frequency in GHz or another quantity:
- §5 reproduction: "[4.1683, 5.6132] GHz … 0.68% and 0.40% above the published initial pair";
- §5: "the initial qubit at 4.63 GHz, above the cap";
- §5 family b: "Its qubit estimates lie 0.64% and 0.83% above family a's";
- §5: the common-limit fit "above the rule's estimate";
- §7: "3.0203 … GHz for the qubit and 4.0225 GHz for the resonator, 0.52% and 0.60% above the first family";
- §9: the C_Σ sentences.

## Item 3: figure colours

**Figure 1, before → after**

| Object or annotation | v19 | v20 |
|---|---|---|
| Feedline (drawn, label) | teal, teal | teal, teal (unchanged) |
| Resonator outline, label; (d) branch | orange, orange; orange | orange, orange; orange (unchanged) |
| Island (a), island lead (b), island branch of (d) and its labels | blue | blue (unchanged) |
| Junction in (a) | not drawn; label "Junction" black | drawn as the sheet plus a small marker; label purple `#7B3FA0` |
| Junction in (b) | sheet orange, label "1 × 1 µm, 4 triangles" orange | sheet purple, label "junction, 1 × 1 µm, 4 triangles" purple |
| Ground | outline `#5F6B73` in (a); label "Ground" `#4D565C`; (c) sheet GRAY `#72777D`; (d) rail black | one grey `#5F6B73`: outline (a), label, sheet (c), rail and ground symbols (d) |
| Box wall | dashed black, label black | dashed black, label black (unchanged; the label already had its object's colour) |
| c dimension and "c" | blue (the island's colour) | green `#5E8A2E` (annotation colour) |
| Stretched strip | light-blue shading, blue text | light-green shading, green text |
| "translated by Δc" and its arrow | GRAY | green |
| "length L (total_length)" | second line of the orange resonator label | its own green label with a green leader to the meander |
| k in (d) | grey | grey (unchanged; a circuit coupling, not a design parameter) |

The caption adds the qubit island "(blue, as in (b) and (d))" and "the junction (purple, as in (b)) joins the island to ground". It also states the rule: "Each label has the colour of the object it names; the design parameters are annotated in green."

In (d), $L_J$ is the junction's inductance, but it is drawn in the island branch's blue, as the instruction asked ("the island branch of the circuit (d) stays blue").

**Figures 2–5, checked for the same inconsistency**

- **Figure 2: changed.** The legend keyed "r = 1 extrapolation of each pair" with the green p = 2 marker only, while the panels draw the p = 1 extrapolation blue. The key now shows both markers, blue circle and green square.
- **Figure 5: changed.**
  - The shape-derivative Newton is blue against DeviceLayout's discretization. Its steps against the extrapolated frequencies, with their "Step 1/Step 2" labels, were green. The scaling-law method was teal in both places.
  - The step-2 qubit band mapped to c was a translucent green bar in (a). The same band in (c) is the grey of every estimate band.
  - Now the shape-derivative steps and their labels are blue: one colour per method, with marker and line style telling the two objectives apart. The mapped band is the estimate-band grey `#DDE1E4` with a grey edge.
  - The D/E region shading is unchanged.
- **Figure 3: no change.** p = 1 is blue and p = 2 green, as in Figure 2. The published line is orange dashed, the circuit-model cap teal dotted, the bands grey and the estimates black. The adaptive run is black throughout.
- **Figure 4: no change.**
  - Colour encodes the permittivity model (scalar blue, tensor orange) the same way in (a), (b), the bounded bars and the in-panel labels.
  - Marker and line style encode the scalar P1/P2 element order. These are Lagrange elements for the potential, not the Nédélec ladders of Figures 2 and 3, and they are named "P1/P2", not "p = 1/2".
  - The (b) key uses neutral black markers for "upper"/"lower" on purpose.

`figure_style.py` and `geometry_mesh.py` are still v9's bytes; the verifier checks this. The new colours are defined in the figure scripts.

## Item 4: where the shift comes from

The new Discussion paragraph says:
- geode-fem computes in f64. Each cross-check run record has `float_dtype = "F64"`.
- Palace is the build pinned in `reference/palace/docker/Dockerfile`, which sets no single-precision option. I checked this: its CMake options are the `PALACE_WITH_*` set, plus `-O3 -ffp-contract=fast -funroll-loops`, with no precision option. The cross-check artifact records "Palace fba6a5b … built from reference/palace/docker/Dockerfile", and `fba6a5b` is the Dockerfile's default `PALACE_REF`.
- The two codes agree on the qubit frequency to 2×10⁻⁴% on shared meshes (§5, §7). So a loss of precision that mattered would show as a same-mesh disagreement far above that level.
- Palace is used as a frequency-domain finite-element eigensolver, not FDTD. Given the same mesh, geode-fem returns the same modes (Table 2, the crosses in Figure 2).
- The error lies in the discretization: refining from h to h/2 moves both codes' frequencies up together.

The paragraph does not claim that Palace's build is "the default double-precision build". I could not check this in the repository, so it says only what the Dockerfile shows. The paragraph uses no "certified", "proven", "proof" or "guarantee"; the verifier checks this.

## Length

v19 had 20 pages; v20 has **21**. The extra lines are the Discussion paragraph, the definition sentence in §1 and the longer phrases. They push about ten bibliography entries onto page 21. Page 19 ends about a quarter blank before the References heading. Getting back to 20 pages would need about 20 lines cut. I did not cut any, because this revision is clarity-only.

## For the operator

- The title is still the ruled one (see v19's changelog).
- The Zenodo DOI is still a `TODO(operator)`.
- Independent v20 reviews are still pending.
