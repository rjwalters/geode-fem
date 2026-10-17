# transmon-benchmark.18 changelog

Revision of v17 against two independent reviews of v17 held outside the thread
(no `.17.review/` sibling): Opus (42/44) and Fable (41/44), no critical flags.
No solver and no `cargo` was run; text, table, caption and
verifier changes only. Figures 1–5 are v16's bytes. The operator's rulings
stand.

## Must-fixes

| Source | Resolution |
|---|---|
| Opus M1 (circuit mismatch) | One circuit, §3.3's: bare qubit $1/(2\pi\sqrt{(L_J+L_s)C_\Sigma})$, $C_\Sigma = C_{\rm isl} + C_J$, Maxwell-form coupling $k$. §3.3 now says it is used both ways (forward from the bound, backward from a dressed pair). The published reading is recomputed with it by exact inversion of [4.14, 5.591] GHz over the $L_s$, $k$ corners: $C_\Sigma$ in **[98.51, 98.64] fF**, **15.5–16.0 %** above the grounded bound (was [98.59, 98.72] fF and 15.6–16.1 %, from the `transmon_loop_inductance [anchor]` topology with $C_J$ across $L_J$ alone). $E_C/h$ for the published two-node reading is 0.196–0.197 GHz (Table 6). The static route was already in §3.3's circuit; the verifier now recomputes both from the formula and checks the committed grounded and natural static bands against it. The abstract carries none of these numbers. |
| Opus M2 (film scope) | "more than ten times a 0.2 µm film, on every uniform mesh. Palace's adaptive runs refine at most four times, so their sheet edges can reach about 0.75 µm, still above the film (an estimate: no committed record holds those meshes' minimum edge)." Both adaptive runs have five solves (four refinements) in `[amr.*].iterations`. |
| Fable M1 (Table 2 cap) | Asterisk on both initial-qubit bands; footnote "trims the bands to [4.415, 4.477] and [4.458, 4.477]". |
| Fable M2 + Opus SF-3 (companion) | Artifact named in the text (`benchmarks/patch_antenna_conformal/tuned/tuned_summary.toml`); the target is the worst-of-band reflection $|S_{11}|$ over 2.400–2.4835 GHz; "runs that exhausted their budget before reaching the target bound the ratio from below at 2.3–3"; cut to one framing sentence and one result sentence. |

## Should-fixes

| Source | Resolution |
|---|---|
| Opus SF-1 | §5: both finest adaptive levels lie "inside family a's band and below family b's" (4.4297 < 4.458; 3.2468 < 3.278). |
| Opus SF-2 | §9 split restated multiplicatively: 1.145 × the remaining 1.9–2.1 % (86.87/85.27 to 86.87/85.06), which multiplies back to the 1.166–1.169 factor; the prose no longer repeats Table 6's per-row percentages. |
| Opus N-1, Fable S2 | Figure 2 caption: "(natural box; the grounded bound lowers its low end by about 0.3 MHz)", matching the plotted v16 band. |
| Fable S3 | "about 0.3 MHz" in §6 and the Figure 2 caption (0.26 MHz unrounded; the printed four-decimal bands differ by 0.2). |
| Fable S6 | Table 6 caption: bare rows omit $L_s$ and $k$, so their $C_\Sigma$ lies above the bound. |
| Opus SF-4, Fable S1 (cuts) | Six-decimal upper bounds → two decimals, ceiled; ulp aside removed; the §4 sentence repeating Table 1's caption cut; the "Neither the blog…" sentence made a clause; the $L_s$, $k$ artifact paths moved to §3.3; §7 derivative and δ paragraphs trimmed; the surface-loss sentence cut to one clause; the intro's linear-model sentence shortened. |
| Opus N-2, N-3, N-5 / Fable N3, N4, N8 | "the adaptive runs check only the qubit"; contribution 2 "the eigenproblem's PEC box grounded (a floating box lies inside the same bracket)"; the box sentence split, with "nested"; "from its ladder's $h$ mesh"; "A first check for a copied layout". |
| Opus N-7 | The source header comment no longer names reviews or scores. |

Skipped: KQCircuits/Elmer (not verified); §11 as a table and the commit list (Fable S5; the Zenodo deposit pins the state); §8 position (Fable N7, ruled section); the faer and Braess bibliography checks (Fable N1, N2; no network verification done this round); Figure 2 cap/bar overlap.

## Length

19 pages (v17: 19), references from page 18. The v18 additions (circuit sentence, film scope, artifact path) are paid for by the cuts above; 18 pages would need cutting a table (for example Table 4, whose values Figure 5(c, d) shows only graphically) or §11's settings, which no review asked to remove.

## Judge round (Doctor)

| Source | Resolution |
|---|---|
| Judge M1 (adaptive film estimate) | The 0.75 µm figure used the median outline edge. `sheet_edge_lengths.py` on both DeviceLayout $h$ meshes gives a shortest edge of 1.000 µm (the junction leads) and 1.833/1.877 µm next; four halvings take them to 0.06–0.11 µm. §9 now says the uniform meshes' edges are mostly ten or more times the film; near the junction leads the adaptive meshes are estimated to reach edges comparable to or below the film; there the zero-thickness model is an approximation whose effect is not quantified (listed under Threats and limitations). "still above the film" and its "therefore" are gone. `check_v18()` reads the minimum, the lead edges and p10 from the meshes. |
| Judge S1 | 136.69 fF (ceiled) in both places. |
| Judge S2 | "(h/2, h/4)" and "(h, h/2)" restored in the common-limit fit. |
| Judge S3 | New `two_node_reading.py` writes `two_node_reading.toml` (both directions of §3.3's circuit); Table 6 cites it and says the `transmon_loop_inductance` anchor uses another topology. The verifier re-runs the script and fails on a stale TOML. |
| Judge nits | "the −9.5 dB target"; the censored lower bound floored to 2.2 (minimum 2.29); "Of that excess, taken as a factor". |
| Length | To stay at 19 pages: the practitioner paragraph and §11 are trimmed, and five bibliography entries with seven or more authors list the first author and "and others" (eriksson2025automated, mfem, shanto2024squadds, sommers2025open, koch2007charge); the verifier checks that only the author field changed and that the first author is kept. |
