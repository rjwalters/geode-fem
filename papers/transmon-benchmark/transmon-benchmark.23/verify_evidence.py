#!/usr/bin/env python3
"""Check the v23 manuscript against committed artifacts (Python >= 3.11; run as python3.14 -I).

v23 is the reading-flow revision of v22 (the flow review of 2026-10-23 and the two deferred #1231 Judge nits): no
number, result, claim scope, title, author, DOI or reference changed, and no computation was run. Paragraphs, one
float and one section moved, so no whole-file diff against v22 is possible; check_v23() pins instead that the
multiset of numerals in the document body (and in the preamble) equals v22's exactly, that the title, author block,
date-and-DOI block, citation set and refs.bib are v22's, the section and subsection order, the position of each
moved block, one copy of each applied wording, the Computed tier, the Figure 1 caption exemption, the precision
qualifier, and v22 main.tex unchanged by SHA-256. Earlier checks are updated where the text moved or was reworded.


v22 is v21 with the operator's short title, "An Independent Finite-Element Cross-Check of the DeviceLayout.jl
Transmon" (geode-fem#1231). Beyond v21's checks (the title pin updated) it pins: the new title; v21 main.tex
unchanged by SHA-256, and v22 equal to it apart from the header comment and the title. The v20 -> v21 comparison
is made on v21's own main.tex.


v21 is the v1 Zenodo release of v20: each author's own affiliation (Robb Walters, 2AM Logic; Crutcher Dunnavant,
ZSpaceLabs.ai) and the paper's own Zenodo DOI (10.5281/zenodo.23580545) under the date. Beyond v20's checks (the
author pin updated) it pins: the date-and-DOI block; no pending marker and no "Independent Researcher"; v20 main.tex
unchanged, and v21 equal to it apart from the header comment, the author block and the date block.


v20 is a clarity revision of v19 for the coauthor's review, with no number changed and no new computation. Beyond
v19's checks (updated where the text moved) it pins: the coauthor's affiliation in the author block, with no
pending marker left; the qubit frequency named in the first paragraph of Section 1 as the linear eigenfrequency of
the junction's (plasma) mode, with the f01 footnote kept; every "x % above" sentence naming the frequency, not "the
qubits"; the paragraph "Where the shift comes from" in Section 9 (geode-fem in f64, from the run records; Palace
from reference/palace/docker/Dockerfile, which sets no single-precision option; the 2e-4 % same-mesh agreement;
a frequency-domain finite-element eigensolver, not FDTD; the error in the discretization), pointed to from
Section 1; the Figure 1 caption's colour rule; v19 main.tex unchanged.


v19 is a reader-clarity revision of v18 for a coauthor outside qubit and FEM work (pre-read lens, MUST 1-7 and
SHOULD 8-17), with no new results. Beyond v18's checks (updated where the text moved, mostly "order" -> p and r and
"level" -> pass) it pins: element order p and convergence rate r, defined once, with no "order 1/2" for a rate or a
level; one meaning each for bound, cap and band ("fitted ranges" for L_s and k); the Palace run, step and probe at
the head of Section 7; the extrapolation-rule disclosure at the first "consistent" in Section 1; Table tab:meshes
against the artifacts (8x tetrahedra per gmsh split, family b on initial/final/step 2, five adaptive passes); c and L
from runs/fd_initial.toml, whose velocity bands Figure 1 draws; no derivative credit in the abstract, the
introduction or the conclusion; the "x % low" gaps cut, so every gap is over the published value; the 17-39 % band
only in Table 6; the coauthor block (second author, literal affiliation marker, no email, no equal-contribution
note), the author-contributions statement by initials and the shorter acknowledgment; v18 main.tex unchanged.


v18 revises v17 against the Opus (42/44) and Fable (41/44) reviews of v17. Beyond v17's checks (updated where the text
moved) it pins: ONE two-node circuit, Section 3.3's, computed here from the formula for both the static route (which
reproduces the committed grounded and natural bands) and the published reading ([98.51, 98.64] fF, 15.5-16.0 %);
the factor split that multiplies back (1.145 x 1.9-2.1 %); Table 2's cap trimming both initial-qubit bands; the
adaptive levels inside family a's band and below family b's; the film comparison scoped to the uniform meshes, with
the adaptive runs' four refinements (an estimate of about 0.75 um); the companion sentence with its artifact, its
target and the budget-censored lower bound; Figure 2's caption naming the natural-box band it draws; "about 0.3 MHz";
the page cuts (two-decimal ceiled upper bounds, no ulp aside); v17 main.tex unchanged.

v17 revises v16 against the Opus (41/44), Fable (40/44) and Codex reviews of v16, with two new committed results:
the grounded-box C_isl upper bound (benchmarks/transmon_diffopt/c_sigma_bracket_grounded_box.toml) and Palace's
adaptive run on the published final design ([amr.final] of the controls artifact). Beyond v16's checks (updated where
the text moved) it pins: the grounded-box bracket [79.56, 79.77] fF and C_Sigma [85.06, 85.27] fF, the same-mesh
box-model difference (an estimate, 8e-5) and its bound [0, 0.21] fF, the grounded-box static route; E_C and the
15.6-16.1 % / 16.6-17.0 % comparisons against the grounded-box bound, with the factor split computed (no "about
half"); Table 6; the final-design adaptive levels, its finest level +8.0 % (measured, not extrapolated), the stop on
the 1e7 size limit, the ill-conditioned fit; Table 2's adaptive column; the abstract's Palace-only clause; the
itemized band rule with its six initial-qubit values; the GCI numbers; the family-b gaps behind 6.2-8.4 %; the
companion pointer and its generic bibliography entry; the sheet-edge lengths against film thickness, from the mesh;
the one-element-wide strips and Ye et al. in the practitioner paragraph; the Burn year; v16 main.tex unchanged.

v16 revises v15 against the Opus (37/44) and Fable (39/44) reviews of v15. Beyond v15's checks (updated where the
text moved) it pins: Eq. (3) with the one-sided box-flux term and the natural-box condition, tied to the source of
certify_impl and the bracket example; the lower bound's transfer to the grounded box by monotonicity, and which model
the upper bound and the dressed range belong to; the band ends recomputed on all 16 ladders (high end = the order-1
three-level value at its observed order, borrowed by the order-2 pair; low end = order-1 lambda, order-2 lambda on
family-b resonators); Richardson and Roache's GCI cited, with the Crossref DOIs; E_C/h bounded by [0.2271, 0.2278] GHz
from the unrounded and the rounded brackets, and the >= 16.6 % overstatement of C_Sigma; Table 2's family-b column;
the 17-39 % band in the abstract and the conclusion; both capacitor mappings for both families; the whiteroom L2
wording; the observed orders recomputed from the ladder (0.62 initial, 0.61 final); 4.2 % rounded, not floored;
six-decimal resonator agreement in place of "0.0000 %"; the search scope of the absence claim and the litsearch log;
Ye et al.'s convergence practice; the quasi-lumped citation; Reproducibility after the conclusion; the Figure 2 label
placement; v15 main.tex unchanged.

v15 revises v14 against the Opus and Fable reviews of v14 (both 37/44). Beyond v14's
checks it pins: the island capacitance renamed C_isl, with C_Sigma = C_isl + C_J in
Koch's convention wherever a frequency or E_C is formed, and the Bounded entry
carrying C_Sigma in [85.06, 85.26] fF (so 4.63 GHz cannot be formed from it);
"on uniform meshes" wherever the order-1 rate is stated; the linearized (plasma)
qubit frequency against f01, with the 6.6 % and 0.19-0.22 GHz arithmetic; why h/2
is the same discrete problem (Palace's h/2 logs: no bisection, element and vertex
counts = geode-fem's + the crack duplicates); like-for-like Table 1 counts; the
cap in the abstract; "highest uniform level, order 2 on h/2" in place of "finest";
the Nedelec/grad-P1 wording of the from-below heuristic; the environment
difference and "not diagnosed"; the final geometry's second family (#1135 T4) and
PRIMA at three trust radii (T5); SQuADDS's own "validated design database" quoted
and attributed; the surface-participation citations; the bibliography notes; the
L1-L4 and Squad definitions; the acknowledgment marker removed; v14 main.tex
unchanged.

v14 revised v13 against the Opus and Fable reviews of v13, under the operator's
rulings (title, derivative-scope paragraphs, short Section 8). Beyond v13's
checks it pins: one Palace-run counting rule (probes included, so the
Palace-only Newton is 4 runs: its 3 recorded runs plus Broyden's L probe
dl_bp2 that sets its delta); the identical-discrete-problem claim only at h/2,
with Palace's crack bisection traced to its log and to
benchmarks/transmon_eigen/palace_crack/; "code verification" banned; the
remeshed first-order difference against hf_mean (a mean of endpoint
derivatives) and the Broyden probe step; the extrapolation-free bracket
[4.4297, 4.477] GHz; the qubit-from-below mechanism (1.67 against 1.66-1.68);
the E_C/anharmonicity translation; the two delta values (2.6x); the L1 bound of
the lower bound; the EPR and transmon citations; the solver settings and mesh
sizes of Section 9. v13's checks follow:

- the same-input-mesh agreement (Table 1) and the refinement shifts of the fixture;
- the cross-check (Table 2): published pairs, Palace at DeviceLayout's
  discretization, the extrapolation rule recomputed from the ladder cells, bands,
  the circuit cap, same-mesh agreement (qubit and resonator stated separately),
  the rate wording (at best O(h), so order-1 estimates lean low);
- the bounded island capacitance: scalar and tensor brackets, the bounded
  scalar-tensor gap, the conforming levels, the coarse-mesh overstatement, the
  static route and the model-dependent cap;
- the re-optimization (Tables 3 and 4): the controls in one Palace environment
  (PRIMA 10, three Newton variants 3, Broyden 5; published PRIMA 9 as context),
  the explanation of the shape-derivative first step from the Jacobian entries,
  the second refinement family at step 2, and the capacitor band recentred on
  the 3.0 GHz target and reconciled with the two-family union;
- the evidence tiers, the derivative-advantage paragraphs and the attribution
  (no run count or design credited to the shape derivative);
- the citation set against refs.bib (no uncited entry, no url beside a DOI, the
  v13 additions), the eigenvalue-derivative citation, forbidden prior-art claims;
- the operator's rulings (title, solo author, validation wording, Section 8 of
  150-250 words with the whiteroom citation and the AI-model statement and no
  commit counts, acknowledgment, repository URL) and the two TODO(operator)
  comments; v10 to v13 main.tex are unchanged;
- voice and audience rules: no revision narrative, no issue numbers, no virtue
  labels, no em-dash, extrapolations never called converged values.

Bounds, bands and percentages are recomputed from the committed values with
their rounding direction. Lower bounds, the extrapolated circuit
terms and the ladder cells are transcribed from the artifacts, not recomputed.
It is not a physics re-run or an independent paper audit. Run from any
working directory.
"""
from pathlib import Path
import hashlib
import json
import math
import re
import sys
import tomllib

HERE = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
REPO = HERE.parents[2]
TEX = (HERE / "main.tex").read_text()
NORM = re.sub(r"\s+", " ", TEX)
BODY = "\n".join(line for line in TEX[TEX.index("\\begin{document}"):].splitlines()
                 if not line.lstrip().startswith("%"))
LOWBODY = re.sub(r"\s+", " ", BODY.lower())
DIFFOPT = "benchmarks/transmon_diffopt/"
CROSS = "benchmarks/transmon_devicelayout_crosscheck/results.toml"
OPT = "benchmarks/transmon_devicelayout_optimize/"
CTL = "benchmarks/transmon_devicelayout_controls/results.toml"


def read(name):
    path = REPO / name
    print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}")
    return tomllib.loads(path.read_text())


def read_text(name):
    path = REPO / name
    print(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}")
    return path.read_text()


def contains(value):
    """The value must appear in the manuscript (source line wrapping ignored)."""
    assert re.sub(r"\s+", " ", value) in NORM, f"Missing or mismatched manuscript value: {value}"


phrase = contains


def absent(value, text=None):
    assert value not in (TEX if text is None else text), f"Stale or forbidden text present: {value}"


def pin(value, count):
    """Exact occurrence count, so that editing one copy cannot hide behind the others."""
    n = NORM.count(re.sub(r"\s+", " ", value))
    assert n == count, (value, n, count)


def floor_to(x, digits):
    scale = 10 ** digits
    return math.floor(x * scale + 1e-9) / scale


def ceil_to(x, digits):
    scale = 10 ** digits
    return math.ceil(x * scale - 1e-9) / scale


def pct(x):
    return 100 * x


def section(title, nxt):
    return NORM[NORM.index(title):NORM.index(nxt)]


# --------------------------------------------------------------------------
# Section 4: same-input-mesh agreement
# --------------------------------------------------------------------------

def check_agreement():
    eigen = read("benchmarks/transmon_eigen/results.toml")
    pal = eigen["oracles"]["palace_target2"]
    names = {"qubit": "qubit & transmon qubit", "resonator": "resonator & readout resonator",
             "mode_2": "mode 2 & box/cavity", "port_sheet_artifact": "port sheet & port-sheet artifact",
             "mode_4": "mode 4 & box/cavity", "mode_5": "mode 5 & box/cavity", "mode_6": "mode 6 & box/cavity"}
    worst = 0.0
    for key, label in names.items():
        m = eigen["modes"][key]
        if key == "qubit":
            row = f"{label} & {m['f_ghz']:.6f} & {m['palace_f_ghz']:.6f} & {m['rel_err_pct']:.5f} \\\\"
        else:
            worst = max(worst, m["rel_err_pct"])
            row = f"{label} & {m['f_ghz']:.3f} & {m['palace_f_ghz']:.3f} & {m['rel_err_pct']:.3f} \\\\"
            assert f"{abs(100 * (m['f_ghz'] / m['palace_f_ghz'] - 1)):.3f}" == f"{m['rel_err_pct']:.3f}", key
        contains(row)
    assert worst == 0.032 and ceil_to(worst, 3) <= 0.033
    # Opus N-2: one outward-rounded bound only.
    pin("$\\leq 0.033\\%$", 1)   # v17: Section 4 only (the intro no longer repeats it)
    absent("worst case $0.032")
    assert pal["qubit_geode_vs_palace_pct"] == 5e-5 and pal["target_ghz"] == 2.0
    pin("$5\\times10^{-5}\\,\\%$", 1)   # Section 4 (v17: not repeated in Section 1)
    assert eigen["modes"]["qubit"]["participation"] == 0.9942
    contains(f"port EPR ${pal['qubit_port_epr']:.5f}$ in Palace")
    assert f"{pal['port_sheet_artifact_port_epr']:.2e}" == "2.49e-08"
    contains("port EPR of $2.49\\times10^{-8}$")
    meta = eigen["meta"]
    assert meta["n_nedelec_dofs"] == 156863 and meta["n_interior_dofs"] == 133108
    log = read_text("reference/fixtures/transmon_palace/results_p1/palace_run_v22.log")
    assert "ND (p = 1): 183891" in log
    # Fable S4: Table 1's counts are like for like (all edges, PEC sheet edges included).
    contains("Counted the same way, as all first-order N\\'ed\\'elec unknowns (edges) including those on PEC sheets, "
             "geode-fem's mesh has $156{,}863$ ($133{,}108$ interior) and Palace's has $183{,}891$")
    absent("solves $183{,}891$")
    contains("Both codes are printed to geode-fem's archived three decimals (six for the qubit); $\\Delta$ is the "
             "archived relative difference, computed from Palace's full-precision value.")
    # Palace's crack bisection (MF-2): the rule, its log counts, and where the flagged edges sit.
    assert "Added 4913 elements in 2 iterations of local bisection for under-resolved interior boundaries" in log
    assert "Added 6014 duplicate vertices for interior boundaries" in log
    assert re.search(r"vertices\s+\d+\s+\d+\s+\d+\s+29705", log)
    crack = read("benchmarks/transmon_eigen/palace_crack/fixture_crack_edges.toml")
    by = crack["flagged_edges_by_conductor"]
    assert crack["mesh"] == eigen["meta"]["fixture"] and crack["flagged_edges"] == sum(by.values()) == 1007
    assert (by["ground_and_resonator"], by["feedline"], by["island"]) == (741, 252, 14)
    # Exact agreement with Palace's log (metal cracked, lumped-port sheet excluded as in AddInterfaceBdrElements):
    added_vertices = 29705 - meta["n_nodes"] - 6014       # Palace's bisection midpoints, one per split edge
    assert added_vertices == crack["flagged_edges"]
    assert "Added 15098 duplicate boundary elements" in log
    assert crack["crack_triangles"] + 2 * crack["flagged_edges"] == 15098
    assert "2 iterations of local bisection" in log
    # v16 (Fable S5, Opus SF-4): the census leaves the Section 4 prose for the Table 1 caption, with the total and the
    # resonator-plus-feedline share; the per-sheet split stays in the artifact (asserted above).
    assert by["ground_and_resonator"] + by["feedline"] == 993
    contains("adds $4{,}913$ elements on them by bisection, triggered by $1{,}007$ edges, $993$ of them on the "
             "resonator's ground plane and the feedline.")
    # v19 (NIT 18): the cracking paragraph is three sentences.
    contains("Palace cracks each metal sheet into two sides by duplicating its vertices, which leaves the PEC problem "
             "unchanged, and where a sheet is one element wide, almost all on the resonator and feedline sheets, it "
             "also bisects the elements on both sides")
    for stale in ["one per vertex Palace adds", "$741$ on the ground-plane sheet"]:
        absent(stale)
    contains("Only the bisection changes the unknowns (Table~\\ref{tab:agreement}")
    absent("sit away from the island")
    contains("``local bisection for under-resolved interior boundaries'' in its log; \\texttt{palace/utils/geodata.cpp}")
    contains("after it cracks the metal sheets and adds $4{,}913$ elements on them by bisection")
    # Opus MF-1 / Fable S3: why cracking without bisection leaves the PEC problem unchanged.
    absent("Cracking alone leaves the PEC problem unchanged: each volume edge")   # v19: the mechanism is in Table 1's caption and the artifact
    # Fable S7: the Target semantics are cited to Palace's configuration schema (scripts/schema/config-schema.json
    # at fba6a5b: "(Nonzero) frequency target above which to search for eigenvalues, GHz.").
    contains("Palace's configuration schema defines \\texttt{Target} as the frequency above which its eigensolver "
             "searches \\citep{palace}")
    absent("Palace's source shows that its eigensolver")
    # The EPR is defined and cited where it is used.
    contains("the energy-participation ratio of the junction, the fraction of the mode's inductive energy stored in it "
             "\\citep{minev2021energy}")
    contains("Both codes use the isotropic sheet term of Eq.~\\eqref{eq:port} here.")
    lc = 1 / (2 * math.pi * math.sqrt(meta["junction_inductance_nh"] * 1e-9 * meta["junction_capacitance_ff"] * 1e-15))
    assert f"{lc / 1e9:.2f}" == "17.60"
    contains("$f_{LC} = 17.60$\\,GHz")
    conv = read("benchmarks/transmon_loop_inductance/results.toml")["convergence"]
    assert conv["global_labels"] == ["h", "h2", "h2_gmsh", "h_p2"]
    fq, fr = conv["global_f_q_ghz"], conv["global_f_r_ghz"]
    assert f"{fq[0]:.4f}" == "3.4527" and abs(fq[0] - pal["qubit_f_ghz"]) < 1e-6
    for k, label in [(1, "at $h_F/2$"), (3, "at $p = 2$ on $h_F$")]:
        contains(f"${fq[k]:.4f}$\\,GHz {label} ($+{pct(fq[k] / fq[0] - 1):.1f}\\%$)")
    contains(f"the resonator by $+{pct(fr[1] / fr[0] - 1):.1f}\\%$ at $h_F/2$")
    a2 = eigen["port_voltage_a2"]
    assert a2["status"] == "measured" and a2["max_abs_gt_b"] == 0.0
    phrase("on the fixture it moves the qubit by $6\\times10^{-5}\\,\\%$, keeps the other physical modes to four decimals")   # v17 (Opus N-3)
    # The h/2 qubit residual is the junction-model difference (rank-1 shunt against the isotropic sheet).
    assert a2["bulk_modes_ghz"][0] == 3.452753 and eigen["modes"]["qubit"]["f_ghz"] == 3.452751
    assert f"{pct(a2['bulk_modes_ghz'][0] / eigen['modes']['qubit']['f_ghz'] - 1):.0e}" == "6e-05"
    pin("$6\\times10^{-5}\\,\\%$", 1)   # v17: stated once, in Section 3.3
    phrase("This mechanism is our inference, supported by a synthetic test")
    return pal


# --------------------------------------------------------------------------
# Section 5: the DeviceLayout cross-check
# --------------------------------------------------------------------------

def order1(f1, f2):
    return 2 * f2 - f1


def order1_lambda(f1, f2):
    return math.sqrt(2 * f2 ** 2 - f1 ** 2)


def observed_order(p1):
    """Observed order of a three-level order-1 series in f (refinement ratio 2)."""
    return math.log((p1[1] - p1[0]) / (p1[2] - p1[1])) / math.log(2)


def rule(levels):
    """The part-1 estimate from five levels (p1 h, h/2, h/4; p2 h, h/2)."""
    return (order1(levels[1], levels[2]) + order1(levels[3], levels[4])) / 2


def check_crosscheck():
    cc = read(CROSS)
    pub = cc["published"]
    assert pub["initial_f_ghz"] == [4.14, 5.591] and pub["final_f_ghz"] == [3.005, 3.982]
    assert pub["initial_capacitor_length_um"] == 620 and pub["initial_total_length_um"] == 5000
    assert pub["final_capacitor_length_um"] == 1217.678 and pub["final_total_length_um"] == 7243.955
    assert pub["targets_ghz"] == [3.0, 4.0]
    contains("($c = 620\\,\\mu$m, $L = 5000\\,\\mu$m) at $[4.14, 5.591]$\\,GHz")
    contains("($c = 1217.678\\,\\mu$m, $L = 7243.955\\,\\mu$m) at $[3.005, 3.982]$\\,GHz")
    contains("sets the targets to $[3.0, 4.0]$\\,GHz")
    rows = {}
    for geom in ["initial", "final"]:
        conv, dl, pal = cc["convergence"][geom], cc["devicelayout_run"][geom], cc["palace"][geom]
        assert conv["p1_f_q_ghz"] == pal["p1_f_q_ghz"] and conv["p2_f_q_ghz"] == pal["p2_f_q_ghz"]
        for mode in ["q", "r"]:
            p1, p2 = pal[f"p1_f_{mode}_ghz"], pal[f"p2_f_{mode}_ghz"]
            assert all(b > a for a, b in zip(p1, p1[1:])) and p2[1] > p2[0]
            a = order1(p1[1], p1[2])
            b = order1(p2[0], p2[1])
            assert abs(a - conv[f"p1_order1_f_{mode}_ghz"]) < 2e-6 and abs(b - conv[f"p2_order1_f_{mode}_ghz"]) < 2e-6
            assert abs(order1_lambda(p1[1], p1[2]) - conv[f"p1_order1_lambda_f_{mode}_ghz"]) < 2e-6
            assert abs(order1_lambda(p2[0], p2[1]) - conv[f"p2_order1_lambda_f_{mode}_ghz"]) < 2e-6
            est = (a + b) / 2
            assert abs(est - conv[f"f_{mode}_ghz"]) < 1e-4
            six = [conv[f"p1_order1_f_{mode}_ghz"], conv[f"p2_order1_f_{mode}_ghz"],
                   conv[f"p1_order1_lambda_f_{mode}_ghz"], conv[f"p2_order1_lambda_f_{mode}_ghz"],
                   conv[f"p1_richardson_f_{mode}_ghz"], conv[f"p2_richardson_f_{mode}_ghz"]]
            lo, hi = conv[f"f_{mode}_band_ghz"]
            assert abs(min(six) - lo) < 1e-4 and abs(max(six) - hi) < 1e-4
            published = dl[f"published_f_{mode}_ghz"]
            assert published < lo, (geom, mode)
            over = pct(conv[f"f_{mode}_ghz"] / published - 1)
            assert abs(over - conv[f"f_{mode}_over_published_pct"]) < 2e-3
            dl_over = pct(dl[f"f_{mode}_ghz"] / published - 1)
            assert abs(dl_over - dl[f"f_{mode}_over_published_pct"]) < 2e-3
            rows[(geom, mode)] = (published, dl[f"f_{mode}_ghz"], dl_over, conv[f"f_{mode}_ghz"], over, lo, hi)
        assert conv["published_below_band"] == [True, True]
    for geom in ["initial", "final"]:
        hist = cc["convergence"][geom]["rule_history"]
        assert "estimate 4.63 GHz" in hist and "after the independent static route" in hist, geom
    assert 4.63 > cc["static_cross_check"]["initial"]["circuit_cap_f_q_ghz"]
    # v17 (Fable N3): "the rule used before" dropped.
    # v19 (lens SHOULD 10): the disclosure sits at the first "consistent", in Section 1, and Section 5 points back to it.
    # v23 (flow P11, P15): the caveat is stated in full once, in Section 1; Section 5 keeps the short form and the pointer.
    phrase("The rule was fixed after that static route was known (Section~\\ref{sec:intro}); extrapolation at the "
           "observed rate alone would put the initial qubit at $4.63$\\,GHz, above the cap, so the observed-rate "
           "values enter only as the band's high side.")
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    first = intro.index("is consistent with that shift")
    disc = ("The extrapolation rule was chosen after that capacitance route had been computed, so their agreement is "
            "a consistency check, not a blind prediction.")
    assert disc in intro and 0 < intro.index(disc) - first < 200, "disclosure follows the first 'consistent'"
    assert "consistent" not in NORM[NORM.index("\\begin{abstract}"):NORM.index("\\end{abstract}")]
    absent("the rule used before")
    # The rate: the cited theory allows at best O(h), so the order-1 rule leans low.
    phrase("on uniform meshes the eigenvalue error decays as $h^{2s}$ at any element order "
           "\\citep{boffi2010finite}: $r < 1$.")
    absent("and slower")
    # Fable M2: the rate statement is qualified "on uniform meshes" everywhere it is made.
    phrase("$r = 1$ is the fastest rate the theory allows on uniform meshes, so the rule makes the smallest correction "
           "consistent with it, and its estimates lean low")
    phrase("Extrapolated assuming error $\\propto h$, the fastest rate those edges allow on uniform meshes")
    phrase("The extrapolations assume the rate the sheet-edge singularity allows on uniform meshes")
    for m in re.finditer(r"fastest rate|rate the sheet-edge singularity allows|rate the sheet edges allow", NORM):
        assert "on uniform meshes" in NORM[m.start():m.start() + 120], NORM[m.start():m.start() + 120]
    phrase("for the $p = 2$ pair the rate is taken from theory, because two levels give no observed rate")
    # v16 Opus MF-1: the high side's order is the order-1 ladder's observed order, borrowed by the order-2 pair, and
    # the text says which value sets each end. Recomputed on every ladder with a band (both families, both published
    # designs, step 2 and, for family a, steps 1 and 2 and the scaling-law step).
    ini = cc["convergence"]["initial"]
    # Observed orders from the ladder levels themselves, not from the artifact's 4-decimal value: the initial qubit's
    # 0.615008 rounds to 0.62 (the stored 0.6150 would double-round to 0.61), the final's 0.612786 to 0.61.
    obs = {g: observed_order(cc["palace"][g]["p1_f_q_ghz"]) for g in ["initial", "final"]}
    assert abs(obs["initial"] - ini["p1_observed_order_q"]) < 1e-4
    assert (f"{obs['initial']:.2f}", f"{obs['final']:.2f}") == ("0.62", "0.61"), obs
    # v17 (Fable S1): the six band values as an itemized list, with the initial qubit's values printed.
    phrase("at the observed rate of the $p = 1$ ladder's three levels ($0.62$ and $0.61$ for the family-a qubits), "
           "borrowed by the $p = 2$ pair on the assumption that both element orders share the pre-asymptotic rate, "
           "which lie higher")
    phrase("the $p = 1$ value sets the low end, except on family-b resonators, where the $p = 2$ value does;")
    phrase("the $p = 1$ value sets the high end.")
    v0 = band_values(cc["palace"]["initial"]["p1_f_q_ghz"] + cc["palace"]["initial"]["p2_f_q_ghz"])
    phrase(f"(${v0['p1_order1']:.3f}$ and ${v0['p2_order1']:.3f}$\\,GHz)")
    phrase(f"(${v0['p1_lambda']:.3f}$ and ${v0['p2_lambda']:.3f}$\\,GHz)")
    phrase(f"(${v0['p1_observed']:.3f}$ and ${v0['p2_borrowed']:.3f}$\\,GHz)")
    assert "the same pairs at the observed order, which set the high side" not in NORM
    lo_i, hi_i = ini["f_q_band_ghz"]
    assert abs(ini["p1_order1_lambda_f_q_ghz"] - lo_i) < 1e-4 and abs(ini["p1_richardson_f_q_ghz"] - hi_i) < 1e-4
    assert lo_i < ini["p2_richardson_f_q_ghz"] < hi_i
    assert (f"{lo_i:.3f}", f"{hi_i:.3f}", f"{ini['p2_richardson_f_q_ghz']:.3f}") == ("4.415", "4.677", "4.582")
    # v16 Opus MF-3: Richardson and Roache's GCI named and cited; a GCI at the observed order would rest on the high
    # end, which the cap excludes for the initial qubit.
    assert hi_i > cc["static_cross_check"]["initial"]["circuit_cap_f_q_ghz"]
    # v17 (Opus N-2): the GCI comparison with numbers: fine-grid index, safety factor 1.25, on the order-1 ladder.
    a1, b1, c1 = cc["palace"]["initial"]["p1_f_q_ghz"]
    half = 1.25 * (c1 - b1) / (2 ** observed_order([a1, b1, c1]) - 1)
    assert (f"{c1:.3f}", f"{half:.2f}", f"{c1 + half:.2f}") == ("4.155", "0.65", "4.81"), (c1, half)
    assert c1 + half > cc["static_cross_check"]["initial"]["circuit_cap_f_q_ghz"]
    phrase("The rule is Richardson extrapolation \\citep{richardson1911approximate} at the asymptotic rate rather "
           "than the observed one; Roache's grid convergence index \\citep{roache1994perspective} with a factor of "
           "$1.25$, on the initial qubit's $p = 1$ ladder, is centred on $4.155$\\,GHz with a half-width of "
           "$0.65$\\,GHz, which reaches $4.81$\\,GHz, above the circuit-model cap.")   # v19: GCI paragraph shortened
    absent("The asymptotic rate is therefore $O(h)$")
    # The finest uniform levels (Palace order 2 on h/2) against the published qubits, floored.
    finest = [pct(cc["palace"][g]["p2_f_q_ghz"][1] / cc["devicelayout_run"][g]["published_f_q_ghz"] - 1)
              for g in ["initial", "final"]]
    assert all(max(cc["palace"][g]["p1_f_q_ghz"][-1], cc["palace"][g]["p2_f_q_ghz"][-1])
               == cc["palace"][g]["p2_f_q_ghz"][1] for g in ["initial", "final"])
    # v16 (Opus N-1): rounded to nearest, not floored: 4.311844 / 4.14 = +4.151 % prints as 4.2 %.
    fin = f"${round(finest[0], 1):.1f}\\%$ and ${round(finest[1], 1):.1f}\\%$"
    assert fin == "$4.2\\%$ and $4.7\\%$", fin
    absent("$4.1\\%$ and $4.7\\%$")
    absent("$+4.1\\%$ and $+4.7\\%$")
    # Opus SF-3: "finest uniform level" is ambiguous (order 1 on h/4 has more unknowns); name the level.
    assert "finest uniform" not in NORM
    nd_p2, nd_h4 = cc["palace"]["initial"]["p2_nd"][1], cc["palace"]["initial"]["p1_nd"][2]
    assert round(nd_p2 / 1e6, 1) == 7.0 and nd_h4 > nd_p2
    assert all(cc["palace"][g]["p1_nd"][2] > cc["palace"][g]["p2_nd"][1] for g in ["initial", "final"])
    phrase("With no extrapolation, the highest uniform levels, $p = 2$ on $h/2$ ($7.0$ million unknowns on the initial "
           f"geometry; $p = 1$ on $h/4$ has more unknowns and lower values), already lie {fin} above the published "
           "qubit frequencies")
    contains("adaptive qubit frequencies $+7.0\\%$ and $+8.0\\%$, still rising")
    # Table 2, row by row.
    st = cc["static_cross_check"]["initial"]
    cap = st["circuit_cap_f_q_ghz"]
    labels = {("initial", "q"): "Initial qubit", ("initial", "r"): "Initial resonator",
              ("final", "q"): "Final qubit", ("final", "r"): "Final resonator"}
    # v16 (Opus SF-3, Fable S2): family b has its own column, recomputed from its ladder levels (shared h cells).
    fam_b = read(CTL)["mesh_family"]
    for key, label in labels.items():
        published, dlf, dlo, est, over, lo, hi = rows[key]
        star = "^{\\ast}" if key == ("initial", "q") else ""   # v18 (Fable M1): on both initial-qubit bands
        geom, mode = key
        fb_ = fam_b[geom]
        lv = fb_[f"family_b_levels_f_{mode}_ghz"]
        if mode == "q" or geom == "final":
            assert lv[0] == cc["palace"][geom][f"p1_f_{mode}_ghz"][0]
        est_b = rule(lv)
        assert abs(est_b - fb_[f"family_b_f_{mode}_ghz"]) < 1e-4, (key, est_b)
        blo, bhi = fb_[f"family_b_f_{mode}_band_ghz"]
        assert lo <= blo and bhi <= hi, key            # family b's band inside family a's
        assert published < blo, key
        over_b = pct(est_b / published - 1)
        # v17 (T7): the adaptive run's finest level, a measured value, for both geometries.
        amr = read(CTL)["amr"][geom][f"f_{mode}_ghz"][-1]
        amr_over = pct(amr / published - 1)
        contains(f"{label} & {published:g} & {dlf:.4f} & {amr:.4f} ($+{amr_over:.1f}\\%$) & {est:.3f} ($+{over:.1f}\\%$) "
                 f"$[{lo:.3f}, {hi:.3f}]{star}$ & {est_b:.3f} ($+{over_b:.1f}\\%$) $[{blo:.3f}, {bhi:.3f}]{star}$ \\\\")
    absent("The second refinement family gives, in the same order")
    capped = st["f_q_band_capped_ghz"]
    assert capped[0] == cc["convergence"]["initial"]["f_q_band_ghz"][0] and capped[1] == cap
    fbi = read(CTL)["mesh_family"]["initial"]["family_b_f_q_band_ghz"]
    assert fbi[0] < cap < fbi[1]   # family b's band also crosses the cap
    contains(f"Under the circuit model of Section~\\ref{{sec:static}}, the cap of ${cap:.3f}$\\,GHz trims the bands to "
             f"$[{capped[0]:.3f}, {capped[1]:.3f}]$ and $[{fbi[0]:.3f}, {cap:.3f}]$.")
    q_i, q_f = rows[("initial", "q")][4], rows[("final", "q")][4]
    r_i, r_f = rows[("initial", "r")][4], rows[("final", "r")][4]
    assert f"{q_i:.1f}" == "7.3" and f"{q_f:.1f}" == "8.7" and f"{r_i:.1f}" == f"{r_f:.1f}" == "2.7"
    assert 7 <= q_i and q_f <= 9
    pin("$7.3\\%$ and $8.7\\%$", 1)
    absent("$7$--$9\\%$")
    # #1135 T4: the qubit range over two refinement families, 7.3 % (initial, a) to 9.6 % (final, b).
    ctl = read(CTL)
    fi, ff = ctl["mesh_family"]["initial"], ctl["mesh_family"]["final"]
    q_over = [pct(fi["family_a_f_q_ghz"] / 4.14 - 1), pct(fi["family_b_f_q_ghz"] / 4.14 - 1),
              ff["f_q_over_published_pct"][0], ff["f_q_over_published_pct"][1]]
    assert f"{min(q_over):.1f}" == "7.3" and f"{max(q_over):.1f}" == "9.6", q_over
    r_over = [pct(fi["family_a_f_r_ghz"] / 5.591 - 1), pct(fi["family_b_f_r_ghz"] / 5.591 - 1),
              ff["f_r_over_published_pct"][0], ff["f_r_over_published_pct"][1]]
    assert f"{min(r_over):.1f}" == "2.7" and f"{max(r_over):.1f}" == "3.3", r_over
    pin("$7.3$--$9.6\\%$", 4)   # abstract, Section 1, conclusion  # v16: + the first contribution bullet (Opus N-9)
    contains("two refinement families put both designs' qubit and resonator frequencies $7.3$--$9.6\\%$ and "
             "$2.7$--$3.3\\%$ above their published values")   # v23 (flow P2): the baseline named, not "higher"
    contains("in two refinement families (two ways of subdividing the same starting mesh), extrapolate to "
             "$7.3$--$9.6\\%$ above them")
    contains("Palace reproduces them within $0.7\\%$ at the example's own discretization, and the independent solver "
             "geode-fem agrees on shared meshes to")   # v23 (flow P12)
    # Fable S2: what differs in the environment is stated, and the gap is declared undiagnosed.
    gaps = [rows[k][2] for k in rows]
    assert f"{min(gaps):.2f}" == "0.34" and f"{max(gaps):.2f}" == "0.68" and min(gaps) > 0
    assert "fba6a5b" in cc["meta"]["compute_host"] and "gmsh_jll 4.15.2" in cc["meta"]["geometry_generator"]
    # v16: the environment is stated once, in the Reproducibility section, and pointed to here.
    contains("The published runs' Palace version, mesher build and platform are not recorded (ours are in "
             "Section~\\ref{sec:repro})")
    repro = section("\\section{Reproducibility}", "\\paragraph{Acknowledgments.}")
    for env in ["Palace \\texttt{fba6a5b}", "gmsh 4.15.2", "AWS Graviton4 hosts"]:
        assert env in repro, env
    contains("but which difference causes the $0.34$--$0.68\\%$ gap is not diagnosed")
    assert max(rows[k][2] for k in rows) < 0.7
    contains(f"$[{rows[('initial','q')][1]:.4f}, {rows[('initial','r')][1]:.4f}]$\\,GHz for the initial design")
    contains(f"$[{rows[('final','q')][1]:.4f}, {rows[('final','r')][1]:.4f}]$\\,GHz for the final one")
    contains("$0.68\\%$ and $0.40\\%$")
    contains("$0.38\\%$ and $0.34\\%$")
    bm = max(abs(cc["devicelayout_run"][g][f"boundary_model_f_{m}_pct"]) for g in ["initial", "final"] for m in "qr")
    assert ceil_to(bm, 3) == 0.003
    contains("differ from the configuration-file runs by at most $0.003\\%$")
    under = [cc["convergence"][g]["devicelayout_run_under_converged_pct"] for g in ["initial", "final"]]
    # v17 (Fable S4): both families, so the conclusion's 6.2-8.4 % has its antecedent in Section 5.
    fam_r_under = [pct(1 - cc["devicelayout_run"]["initial"]["f_r_ghz"] / fi["family_b_f_r_ghz"]),
                   -ctl["mesh_family"]["final"]["devicelayout_run_under_family_b_pct"][1]]
    q_i_b = pct(1 - cc["devicelayout_run"]["initial"]["f_q_ghz"] / fi["family_b_f_q_ghz"])
    q_f_b = -ctl["mesh_family"]["final"]["devicelayout_run_under_family_b_pct"][0]
    r_all = [-under[0][1], -under[1][1]] + fam_r_under
    # v19 (lens SHOULD 11): one denominator for the published-vs-estimate gap. The "x % low" figures, relative to the
    # estimate, are cut; every gap in the text is now "above the published value". The artifact values stay asserted.
    assert (f"{-under[0][0]:.1f}", f"{q_f_b:.1f}") == ("6.2", "8.4"), (under, q_f_b)
    absent("low on the two qubits")
    assert math.floor(-under[0][0]) == 6 and math.ceil(-under[1][0]) == 8
    dl_under = [-under[0][0], -under[1][0], pct(1 - cc["devicelayout_run"]["initial"]["f_q_ghz"] / fi["family_b_f_q_ghz"]),
                -ctl["mesh_family"]["final"]["devicelayout_run_under_family_b_pct"][0]]
    assert f"{min(dl_under):.1f}" == "6.2" and f"{max(dl_under):.1f}" == "8.4", dl_under
    for stale in ["$6.2$--$8.4", "$6.2$--$6.8", "$7.6$--$8.4"]:
        absent(stale)
    nd = max(cc["palace"]["final"]["p1_nd"])
    assert round(nd / 1e6, 1) == 12.6
    pin("$12.6$ million unknowns", 1)
    orders_q = [observed_order(cc["palace"][g]["p1_f_q_ghz"]) for g in ["initial", "final"]]
    orders_r = [observed_order(cc["palace"][g]["p1_f_r_ghz"]) for g in ["initial", "final"]]
    assert f"{min(orders_q):.2f}" == "0.61" and f"{max(orders_q):.2f}" == "0.62", orders_q   # v16: was "0.61" only
    assert f"{min(orders_r):.2f}" == "0.70" and f"{max(orders_r):.2f}" == "0.73"
    contains("pre-asymptotic rates of $0.61$--$0.62$ (qubit) and $0.70$--$0.73$ (resonator)")
    absent("orders of $0.61$ (qubit)")
    # Same-mesh agreement on the ladder: the qubit to 1e-4 %, the resonator 0.03 % at h and 0 at h/2.
    for g in ["initial", "final"]:
        sm = cc["same_mesh"][g]
        assert all(abs(x) <= 1e-4 for x in sm["geode_over_palace_f_q_pct"])
        assert round(sm["geode_over_palace_f_r_pct"][0], 2) == 0.03 and sm["geode_over_palace_f_r_pct"][1] == 0
    note = cc["same_mesh"]["initial"]["note"]
    assert "4946 initial, 5944 final" in note
    contains("geode-fem and Palace agree on the qubit to $10^{-4}\\,\\%$ on both geometries. On the resonator they "
             "differ by $0.03\\%$ at $h$")
    contains("Palace adds $4{,}946$ (initial) and $5{,}944$ (final) elements")
    # MF-2: the identical-discrete-problem claim is made only for h/2, where Palace adds no elements.
    for g in ["initial", "final"]:
        assert cc["same_mesh"][g]["geode_over_palace_f_r_pct"][1] == 0
    sentences = re.split(r"(?<=[.;:])\s", NORM[NORM.index("\\begin{document}"):])
    hits = [x for x in sentences if "same discrete problem" in x]
    assert hits and all("$h/2$" in x for x in hits), hits
    absent("assemble and solve the same discrete problem")
    # v16 (Opus N-3, Fable N1): "0.0000 %" replaced by what the archives show: the two h/2 resonator values are equal
    # to all six archived decimals on both geometries.
    for g in ["initial", "final"]:
        assert f"{cc['geode'][g]['f_r_ghz'][1]:.6f}" == f"{cc['palace'][g]['p1_f_r_ghz'][1]:.6f}", g
    contains("on the $h/2$ meshes of Section~\\ref{sec:crosscheck} the two codes solve the same discrete problem "
             "apart from the junction term.")
    contains("and not at all in the six archived decimals at $h/2$, where Palace's logs report no bisection and the "
             "two codes differ only in the junction term")
    absent("$0.0000")
    # MF-1 (Opus) / S3 (Fable): at h/2 Palace bisects nothing; its counts are geode-fem's plus the crack duplicates.
    assert "the refined meshes need no bisection" in note
    base = "reference/fixtures/transmon_palace/results_crosscheck_1092/"
    dup = {}
    for g, cell in [("initial", "initial_r1_p1"), ("final", "final_r1_p1")]:
        log = read_text(base + cell + "/palace_run.log")
        assert "local bisection" not in log, cell
        run = tomllib.loads(read_text(f"benchmarks/transmon_devicelayout_crosscheck/geode_runs/{g}_h2.toml"))["run"]
        nv = int(re.search(r"Added (\d+) duplicate vertices", log).group(1))
        verts, edges, elems = (int(re.search(rf"^ {k}\s+\d+\s+\d+\s+\d+\s+(\d+)", log, re.M).group(1))
                               for k in ("vertices", "edges", "elements"))
        assert elems == run["n_tets"] and verts == run["n_nodes"] + nv, (cell, verts, elems)
        assert edges > run["n_edges"] and f"ND (p = 1): {edges}" in log
        dup[g] = (nv, edges, run["n_edges"])
    fmt = lambda n: f"{n:,}".replace(",", "{,}")
    # v16 (Opus SF-4, Fable S5): the claim stays in the prose; the six counts, recomputed above from the logs and the
    # run records, leave the text for the committed logs.
    assert dup == {"initial": (23106, 1325370, 1249916), "final": (28414, 1624470, 1531492)}, dup
    contains("(Section~\\ref{sec:junction}; the counts are in the logs)")
    for n in ["23{,}106", "1{,}325{,}370", "1{,}249{,}916"]:
        absent(n)
    contains("every uniform level we could afford lies below those estimates")
    # The final qubit rests on family a only; family b came out higher wherever it was run.
    # #1135 T4: the final geometry now has both families; the "one family" caveat is gone everywhere.
    for stale in ["family~a only", "rests on one family", "has family a only"]:
        absent(stale)
    contains("Its qubit estimates lie $0.64\\%$ and $0.83\\%$ above family~a's, and every family-b band lies inside "
             "the family-a band (Table~\\ref{tab:crosscheck})")
    # v16 (Opus N-2): the abstract covers every shared mesh, the re-optimization ladders included (2e-4 %).
    contains("agrees on shared meshes to $2\\times10^{-4}\\%$ (qubit) and $0.03\\%$ (resonator)")   # v23 wording
    contains("reproduces every level of those ladders it can reach, the two coarsest meshes with linear elements, to "
             "$10^{-4}\\%$ on the qubit")
    # v17 (Fable M2): the abstract says the extrapolations rest on Palace's levels (v19: shortened, 200-word cap).
    contains("estimates that lean low and rest on Palace's finer levels.")   # v23 wording
    absent("reproduces the two ladder levels it can reach to $10^{-4}\\%$,")
    assert f"{cc['palace']['initial']['p1_f_q_ghz'][0]:.4f}" == "3.4520"
    contains("($3.4527$ against $3.4520$\\,GHz, $0.02\\%$)")
    mo_q = max(abs(cc["devicelayout_run"][g]["mesh_order_f_q_pct"]) for g in ["initial", "final"])
    mo_r = max(abs(cc["devicelayout_run"][g]["mesh_order_f_r_pct"]) for g in ["initial", "final"])
    assert mo_q <= 0.0001 and round(mo_r, 2) == 0.05
    contains("curving the faces changes the qubit by at most $0.0001\\%$ and the resonator by about $0.05\\%$")
    contains(f"peaked at ${cc['geode']['initial']['peak_rss_gb'][1]:.0f}$ and "
             f"${cc['geode']['final']['peak_rss_gb'][1]:.0f}$\\,GB")
    return cc


# --------------------------------------------------------------------------
# Section 6: bounded island capacitance and the static route
# --------------------------------------------------------------------------

def check_static(cc):
    conv = read(DIFFOPT + "c_sigma_convergence.toml")
    scal = read(DIFFOPT + "c_sigma_bracket.toml")
    tens = read(DIFFOPT + "c_sigma_bracket_tensor.toml")
    assert tens["meta"]["sapphire_rotation_is_identity"] is True
    assert tens["meta"]["sapphire_lab_max_abs_deviation_from_diag"] == 0.0
    contains("which leaves a uniaxial tensor unchanged, so the lab tensor is diagonal")
    table = {}
    for row in conv["x0"]:
        table.setdefault((row["order"], row["level"]), row["c_sigma_ff"])
    levels = sorted({lv for _, lv in table})
    t1 = conv["tensor_eps"]
    t2 = {r["mesh"]: r["upper_ff"] for r in tens["uniform_x0_tensor_p2"]}
    assert t2 == {"uniform h/1": 91.717846, "uniform h/2": 85.008425}
    hi_s, lo_s = table[("P1", levels[0])], table[("P2", levels[1])]
    # The coarsest and finest uniform values quoted (the extremes of each model's series).
    assert hi_s == max(table.values()) and lo_s == min(table.values())
    assert t1["c_sigma_ff"][0] == max(t1["c_sigma_ff"] + list(t2.values()))
    assert t2["uniform h/2"] == min(t1["c_sigma_ff"] + list(t2.values()))
    # v18 (Fable S1): two decimals, ceiled (they are upper bounds); the six decimals stay in the artifacts.
    contains(f"from ${ceil_to(hi_s, 2):.2f}$ to ${ceil_to(lo_s, 2):.2f}$\\,fF (scalar) and from "
             f"${ceil_to(t1['c_sigma_ff'][0], 2):.2f}$ to ${ceil_to(t2['uniform h/2'], 2):.2f}$\\,fF (tensor), ceiled.")
    sb, tb = scal["bracket_x0"], tens["bracket_x0_tensor"]
    assert (sb["lower_ff"], sb["upper_ff"]) == (77.40, 77.61) and (tb["lower_ff"], tb["upper_ff"]) == (79.56, 79.76)
    best_t = max(max(v for k, v in r.items() if k.startswith("lower_")) for r in tens["adaptive_x0_tensor"])
    assert floor_to(best_t, 2) == tb["lower_ff"]
    assert ceil_to(min(r["upper_ff"] for r in tens["adaptive_x0_tensor"]), 2) == tb["upper_ff"]
    pin("$[79.56, 79.76]$\\,fF", 2)   # Section 6, Table 5 (v19: contribution 2 quotes only the grounded bracket)
    pin("$[77.40, 77.61]$\\,fF", 2)
    gap = tens["gap_x0"]
    t_up = min(r["upper_ff"] for r in tens["adaptive_x0_tensor"])
    assert floor_to(best_t - gap["scalar_upper_ff"], 2) == gap["tensor_minus_scalar_lower_ff"] == 1.95
    assert ceil_to(t_up - gap["scalar_lower_ff"], 2) == gap["tensor_minus_scalar_upper_ff"] == 2.35
    lo = min(gap["tensor_minus_scalar_relative"][0], gap["tensor_minus_scalar_lower_ff"] / gap["scalar_upper_ff"])
    hi = max(gap["tensor_minus_scalar_relative"][1], gap["tensor_minus_scalar_upper_ff"] / gap["scalar_lower_ff"])
    shown = f"${floor_to(pct(lo), 1):.1f}$--${ceil_to(pct(hi), 1):.1f}\\%$"
    assert shown == "$2.5$--$3.1\\%$", shown
    contains(f"is $1.95$--$2.35$\\,fF, or {shown} of the scalar value")
    pin(shown, 2)
    t1h = t1["c_sigma_ff"][0]
    assert floor_to(t1h - tb["upper_ff"], 2) == 56.92
    assert floor_to(pct((t1h - tb["upper_ff"]) / tb["upper_ff"]), 1) == 71.3
    assert floor_to(pct((hi_s - sb["upper_ff"]) / sb["upper_ff"]), 1) == 77.4
    contains("overstates the tensor $\\Cisl$ by at least $56.92$\\,fF, which is at least $71.3\\%$ of the exact "
             "value, and the scalar one by at least $77.4\\%$")
    st = cc["static_cross_check"]["initial"]
    assert st["c_sigma_bracket_ff"] == [tb["lower_ff"], tb["upper_ff"]]
    assert st["inside_eigen_band"] is True
    lo, hi = st["dressed_f_q_band_ghz"]
    contains(f"initial dressed qubit in $[{floor_to(lo, 3):.3f}, {ceil_to(hi, 3):.3f}]$\\,GHz")
    cap = 1 / (2 * math.pi * math.sqrt(14.860e-9 * (tb["lower_ff"] + 5.5) * 1e-15)) / 1e9
    assert abs(cap - st["circuit_cap_f_q_ghz"]) < 1e-4 and f"{cap:.3f}" == "4.477"
    pin("$4.477$\\,GHz", 6)   # abstract, Section 1, Section 5 rule, Table 2 note, Section 6 bracket, Table 5 (twice)
    # SF-1: the extrapolation-free bracket, adaptive level to model-dependent cap.
    amr_top = read(CTL)["amr"]["initial"]["f_q_ghz"][-1]
    pub_q = cc["published"]["initial_f_ghz"][0]
    lo_b, hi_b = pct(amr_top / pub_q - 1), pct(cap / pub_q - 1)
    assert f"{lo_b:.1f}" == "7.0" and f"{hi_b:.1f}" == "8.1" and amr_top < cap
    contains(f"With no extrapolation, the finest adaptive pass and the cap put the initial qubit between ${amr_top:.4f}$ and "
             f"${cap:.3f}$\\,GHz, $+{lo_b:.1f}\\%$ to $+{hi_b:.1f}\\%$ over the published value; the lower end is a "
             "computed value, the upper end holds only under the circuit model.")   # v23: the Computed tier
    contains(f"with the finest adaptive pass, $[{amr_top:.4f}, {cap:.3f}]$\\,GHz")
    # Opus SF-2: the extrapolation-free cap is in the abstract.
    ab = NORM[NORM.index("\\begin{abstract}"):NORM.index("\\end{abstract}")]
    assert f"caps the qubit at ${cap:.3f}$\\,GHz ($+{hi_b:.1f}\\%$)" in ab, ab
    # S1: why the qubit is approached from below; the fixture's capacitance ratio against its f^2 ratio.
    p1h = conv["tensor_eps"]["c_sigma_ff"][0]
    c_ratio = [(p1h + 5.5) / (tb[k] + 5.5) for k in ("upper_ff", "lower_ff")]
    assert [f"{x:.2f}" for x in c_ratio] == ["1.67", "1.67"], c_ratio
    fix_q = read("benchmarks/transmon_eigen/results.toml")["oracles"]["palace_target2"]["qubit_f_ghz"]
    fam = read(CTL)["mesh_family"]["initial"]["estimate_range_f_q_ghz"]
    f_ratio = [(f / fix_q) ** 2 for f in fam]
    assert [f"{x:.2f}" for x in f_ratio] == ["1.66", "1.68"], f_ratio
    assert f"{ceil_to(p1h, 2):.2f}" == "136.69"   # v18 Judge S1: ceiled, as in "The unmodified device"
    contains("so the computed qubit should approach its limit from below")
    # Opus SF-4: the comparison is Nedelec/grad-P1, like for like.
    contains("the quasi-static part of a first-order N\\'ed\\'elec field is the gradient of a P1 potential, which "
             "can only overstate the capacitance")
    absent("which a conforming discretization can only overstate")
    contains(f"on the fixture the P1 bound (${ceil_to(p1h, 2):.2f}$\\,fF, with $C_J$ added) overstates $C_\\Sigma$ by a factor of "
             "$1.67$, and the family estimates exceed the fixture's $p = 1$ qubit by $1.66$--$1.68$ in $f^2$. This is a "
             "heuristic, not a proof")
    # SF-3: the L1 terms are bounded above, not integrated by quadrature of an absolute value.
    contains("Each $L^1$ norm is bounded above element by element and face by face, "
             "$\\lVert p\\rVert_{L^1(K)} \\le |K|^{1/2}\\lVert p\\rVert_{L^2(K)}$, with "
             "$\\lVert p\\rVert_{L^2}^2$ integrated exactly by quadrature.")
    src = read_text("crates/geode-core/src/assembly/electrostatic_dual.rs")
    # v16 Fable M1: Eq. (3) matches the implementation. The box is natural (zero normal flux; the bracket example's
    # ground set is the metal sheets only), and certify_impl penalizes the one-sided flux on boundary faces that carry
    # no conductor, beside the interior jumps and the divergence.
    assert "the outer boundary is natural (zero normal flux) except\n//! where a conductor lies on it" in src
    assert "boundary faces (one side)" in src
    assert "NONE if topo.face_tets[fi][1] == NONE => bnd_l1 += l1," in src
    assert "numerator -= interior_jump_l1 + boundary_flux_l1 + divergence_l1;" in src
    ex = read_text("crates/geode-core/examples/transmon_c_sigma_bracket.rs")
    assert '("ground", ConductorRole::Ground),' in ex and '("feedline", ConductorRole::Floating),' in ex
    assert "exterior_boundary" not in ex[ex.index("fn dual(&self)"):ex.index("fn dual(&self)") + 600]
    contains("feedline floating with zero net charge, and zero normal flux on the vacuum box)")
    contains("- \\lVert [D\\cdot n]\\rVert_{L^1(\\mathcal F)} - \\lVert D\\cdot n\\rVert_{L^1(\\mathcal B)},")
    contains("and $D\\cdot n$ on the box faces $\\mathcal B$ is the one-sided flux, which the zero-flux condition "
             "requires of the exact field but not of $D$.")
    # The bound transfers to the eigenproblem's grounded (PEC) box by monotonicity; the upper bound and the dressed
    # range do not, and the text says which model each belongs to.
    # v17 (Opus MF-1, Codex M1): the grounded-box upper bound is now computed; the two-sided grounded-box bracket
    # is checked in check_v17().
    contains("Constraining the potential on the box, to zero or to a floating constant, shrinks the set of admissible "
             "potentials, so it can only raise $\\Cisl$: the natural-box lower bound, and the cap below that uses only "
             "it, hold for the grounded box too.")
    absent("was not computed")
    absent("because the range belongs to the natural-box model")
    contains("PEC (perfect-conductor) conditions hold on the metal sheets and on the box walls, which therefore sit at "
             "ground potential")
    assert "let tet_q = duffy_tet(k + 3);" in src and "let tri_q = duffy_tri(k + 2);" in src
    assert "exact for total degree `2n − 3`" in src and "exact for\n/// total degree `2n − 2`" in src
    assert src.count("int2.max(0.0).sqrt()") == 1 and src.count("div2.max(0.0).sqrt()") == 1
    # Operator ruling (#1129): no "certified" in v14; bounds are "bounded by [a, b]", with one stated caveat.
    assert "certif" not in TEX.lower(), [m.start() for m in re.finditer("certif", TEX.lower())]
    # The ban on "validated" covers our own claims. The one allowed use is SQuADDS's own wording, quoted and
    # attributed to its authors (its title: "A validated design database ...").
    quoted = "what its authors call a ``validated design database''"
    pin(quoted, 1)
    assert "validated design database" in refs_entry("shanto2024squadds")
    assert "curated" not in LOWBODY
    for word in ["validated", "demonstrated"]:
        assert word not in LOWBODY.replace(quoted.lower(), ""), word
    contains("The two bounds hold by theorem, for the exact arithmetic of the discretization on the polygonal "
             "geometry of the mesh, with lower bounds floored and upper bounds ceiled; the implementation is checked "
             "by tests, not formally verified.")
    contains("initial design is bounded by $[77.40, 77.61]$\\,fF with the scalar $\\bar\\varepsilon$ and by "
             "$[79.56, 79.76]$\\,fF with the sapphire tensor")
    contains("Bounded: a two-sided bound that holds by theorem for the meshed polygonal geometry.")
    # Fable M1: C_Sigma follows Koch et al. (it includes C_J); the island capacitance is C_isl. Nowhere may a
    # frequency be formed from C_isl alone, and the Bounded entry carries C_Sigma = C_isl + C_J explicitly.
    csig = [round(tb["lower_ff"] + 5.5, 2), round(tb["upper_ff"] + 5.5, 2)]
    assert csig == [85.06, 85.26]
    cap_c = 1 / (2 * math.pi * math.sqrt(14.860e-9 * csig[0] * 1e-15)) / 1e9
    assert f"{cap_c:.3f}" == "4.477"
    wrong = 1 / (2 * math.pi * math.sqrt(14.860e-9 * tb["lower_ff"] * 1e-15)) / 1e9
    assert f"{wrong:.2f}" == "4.63"   # the number a reader would get from C_isl alone; it must not be formable
    contains("that is $C_\\Sigma \\ge 85.06$\\,fF, caps it at $1/(2\\pi\\sqrt{L_J C_\\Sigma}) = 4.477$\\,GHz")
    contains("In the convention of \\citet{koch2007charge} the transmon's total capacitance is $C_\\Sigma = \\Cisl + "
             "C_J$, and it sets the charging energy $E_C = e^2/2C_\\Sigma$.")
    contains("$\\Cisl = C_{ii} - C_{if}^2/C_{ff}$, excludes the junction")
    contains("With $C_J$ added, the transmon's $C_\\Sigma = \\Cisl + C_J$ is bounded by $[85.06, 85.27]$\\,fF for the "
             "eigenproblem's grounded box (tensor; $[85.06, 85.26]$\\,fF natural).")
    for stale in ["C_\\Sigma + C_J", "C_\\Sigma = C_{ii}", "C_\\Sigma \\ge 79.56", "C_\\Sigma = \\int",
                  "C_\\Sigma \\;\\ge", "C_\\Sigma$ of the unmodified", "of the $C_\\Sigma$, $L_s$"]:
        absent(stale, BODY)
    for m in re.finditer(r"sqrt\{[^}]*\}", BODY):
        assert "Cisl" not in m.group(0), m.group(0)
    assert "\\newcommand{\\Cisl}{C_{\\mathrm{isl}}}" in TEX
    assert f"{-st['eigen_estimate_minus_static_mid_pct']:.2f}" == "0.22"
    contains("family~a's estimate is $0.22\\%$ below its midpoint")   # v23 (flow P9): family a, not "the first family"
    phrase("midpoint, a consistency check (Section~\\ref{sec:intro})")   # v17 (Fable S7); v19 MUST 5; v23 (P15) shortened
    pin("$0.22\\%$", 2)   # v17: Section 6 and Table 5 (Fable S7)
    established = NORM[NORM.index("\\paragraph{What is established, and how.}"):NORM.index("\\label{tab:tiers}")]
    assert "0.22" not in established.split("\\begin{table}")[0]
    tiers = {row.split(" & ")[0].strip(): row for row in
             NORM[NORM.index("Tier & Results"):NORM.index("\\end{tabular}", NORM.index("Tier & Results"))]
             .split("\\\\")}
    assert [k for k in tiers if "$0.22\\%$" in tiers[k]] == ["Consistency"], tiers.keys()
    assert "not a blind prediction" in tiers["Consistency"]
    contains("Bounded & Island capacitance $\\Cisl$ of the unmodified device, without $C_J$, natural box: "
             "$[77.40, 77.61]$\\,fF (scalar), $[79.56, 79.76]$\\,fF (tensor); grounded box, the eigenproblem's model: "
             "$[79.56, 79.77]$\\,fF (tensor), so $C_\\Sigma = \\Cisl + C_J$ in $[85.06, 85.27]$\\,fF and "
             "$e^2/2hC_\\Sigma$ in $[0.2271, 0.2278]$\\,GHz; "
             "tensor-minus-scalar gap $2.5$--$3.1\\%$ \\\\")
    tier = lambda name: next(v for k, v in tiers.items() if k.endswith(name))
    assert "4.477" not in tier("Bounded") and "4.477" in tier("Model-dependent")
    # Judge nit: the Model-dependent row names C_Sigma; Opus SF-7: the bracket is not repeated in full there.
    contains("Model-dependent & The initial-qubit cap of $4.477$\\,GHz $= 1/(2\\pi\\sqrt{L_J C_\\Sigma})$ at the lower "
             "bound on $C_\\Sigma$: two-node circuit, quasi-static $C_\\Sigma$, bounded on the fixture's polygonal "
             "geometry, not on the ladder meshes; with the finest adaptive pass, $[4.4297, 4.477]$\\,GHz; the same "
             "$e^2/2hC_\\Sigma$ read as the transmon's charging energy $E_C/h$ \\\\")
    contains("Not claimed & Agreement with measurement; a verified hit of $3.0$/$4.0$\\,GHz in the $h \\to 0$ limit; "
             "a global optimum; an advantage of the shape derivative on this problem; eigenmode-participation (EPR) "
             "derivatives, which need eigenvector derivatives \\citep{nelson1976simplified}; a speed advantage over "
             "Palace \\\\")
    phrase("It does not depend on the extrapolation rule.")
    rich = cc["convergence"]["initial"]["p1_richardson_f_q_ghz"]
    assert f"{rich:.3f}" == "4.677" and rich > cap
    contains("($4.677$ and $4.582$\\,GHz)")
    assert "transmon_smoke.msh" in st["source"] and "not on this PR's DeviceLayout mesh" in st["geometry_caveat"]
    contains("rests on the fixture, not on the ladder meshes, which agree with it to $0.02\\%$ at $h$")
    m = scal["method"]["certificate"]
    assert "0 <= phi* <= 1" in m and "Cauchy-Schwarz" in m
    contains("$0 \\le \\phi \\le 1$ by the maximum principle")
    contains("whenever $N(D) > 0$")
    assert "not interval arithmetic" in tens["method"]["rounding"] and "1e-10" in tens["method"]["rounding"]
    contains("not interval arithmetic; the worst-case summation error is about $10^{-10}$ relative")
    contains("fitted ranges of $0.113$--$0.125$\\,nH and $0.0227$--$0.0310$, which are estimates.")   # v19: "band" is the extrapolation range only
    return sb, tb, tens


# --------------------------------------------------------------------------
# Section 7: the re-optimization and its controls
# --------------------------------------------------------------------------

def err(f, t):
    return sum(((a - b) / b) ** 2 for a, b in zip(f, t)) / 2


def shared_limit(f1, f2, g1, g2):
    """Order p at which the two pairs extrapolate to the same limit, and that limit."""
    ex = lambda a, b, p: b + (b - a) / (2 ** p - 1)
    lo, hi = 0.3, 3.0
    for _ in range(200):
        mid = (lo + hi) / 2
        same = (ex(f1, f2, lo) - ex(g1, g2, lo)) * (ex(f1, f2, mid) - ex(g1, g2, mid)) > 0
        lo, hi = (mid, hi) if same else (lo, mid)
    return lo, ex(f1, f2, lo)


def fmt_err(e):
    mant, ex = f"{e:.1e}".split("e")
    return f"${mant}\\times10^{{{int(ex)}}}$"


def check_reopt(cc):
    opt = read(OPT + "results.toml")
    log = [json.loads(line) for line in read_text(OPT + "opt_log.jsonl").splitlines() if line.strip()]
    targets = opt["meta"]["targets_ghz"]
    assert targets == [3.0, 4.0]
    g = opt["gradient_check"]
    assert g["hf_df_dc_ghz_per_um"] == g["fd_df_dc_ghz_per_um"] and g["qubit_rel_diff"] == 0
    assert g["fd_h_um"] == 0.5 and g["velocity_outline_nodes_checked"] == 3146 and g["gradient_s"] == 0.44
    contains(f"${g['hf_df_dc_ghz_per_um'][0] * 1e3:.6f}\\times10^{{-3}}$\\,GHz/$\\mu$m for the qubit")
    contains(f"${g['hf_df_dc_ghz_per_um'][1] * 1e6:.3f}\\times10^{{-6}}$\\,GHz/$\\mu$m for the resonator")
    contains("($3{,}146$ nodes on the initial mesh)")
    contains("for both modes in $0.44$\\,s")
    contains("($\\pm0.5\\,\\mu$m)")
    rn = opt["remesh_noise"]
    contains(f"${rn['c_620_to_650_remesh_fd_ghz_per_um'][0] * 1e3:.2f}\\times10^{{-3}}$\\,GHz/$\\mu$m")
    # v16 (Opus N-11): the densest sentence of Section 7, split after the remeshed value.
    contains("against a fixed-topology derivative of "
             f"${rn['c_620_to_650_hf_mean_ghz_per_um'][0] * 1e3:.2f}\\times10^{{-3}}$ averaged over the interval (the "
             "mean of its end values)")
    # M3 / SF-4: -2.28e-3 is hf_mean, the mean of the fixed-topology derivatives at c = 620 and 650 um.
    g620 = [e for e in log if e.get("tag") == "fd_initial"][0]
    g650 = [e for e in log if e.get("tag") == "calibC_650"][0]
    rc = [e for e in log if e["kind"] == "remesh_check_c"][0]
    assert abs(rc["hf_mean"][0] - (g620["dfdc"][0] + g650["dfdc"][0]) / 2) < 1e-12
    assert abs(rc["fd_remesh"][0] - (g650["f"][0] - g620["f"][0]) / 30) < 1e-12
    assert rc["hf_mean"][0] == rn["c_620_to_650_hf_mean_ghz_per_um"][0]
    fixed = -rc["hf_mean"][0] * 30 / g620["f"][0]
    jump = -(rc["fd_remesh"][0] - rc["hf_mean"][0]) * 30 / g620["f"][0]
    assert f"{pct(fixed):.1f}" == "2.0" and f"{pct(jump):.1f}" == "2.6", (fixed, jump)
    contains("because remeshing alone moves the qubit by about $2.6\\%$, more than the $2.0\\%$ the step moves it.")
    tl = opt["transmission_line"]
    assert round(tl["delta_um"]) == 216
    contains("$\\delta = 216\\,\\mu$m")
    d_of = lambda a, b: (5250 * b - 5000 * a) / (a - b)
    fa, fb = tl["f_r_ghz"]
    assert abs(d_of(fa, fb) - tl["delta_um"]) < 1e-3
    spread = max(abs(d_of(fa * (1 + s * 1.5e-3), fb) - tl["delta_um"]) for s in (1, -1))
    spread = max(spread, max(abs(d_of(fa, fb * (1 + s * 1.5e-3)) - tl["delta_um"]) for s in (1, -1)))
    assert 25 * round(spread / 25) == 175, spread
    phrase("remesh noise of about $0.15\\%$ in either $f_r$ moves it by about $\\pm175\\,\\mu$m")
    phrase("the geode-fem solves of the one-time $\\delta$ calibration are not Palace runs")   # v17 (Codex)
    nd = opt["newton_devicelayout"]
    for k in range(4):
        e = nd["err"][k]
        assert abs(err([nd["f_q_ghz"][k], nd["f_r_ghz"][k]], targets) - e) < 1e-9 + 2e-3 * e
        x1, x2 = math.sqrt(620 / nd["capacitor_length_um"][k]), 5000 / nd["total_length_um"][k]
        assert abs(x1 - nd["x1"][k]) < 1e-6 and abs(x2 - nd["x2"][k]) < 1e-6
    crit = nd["devicelayout_criterion"]
    first = next(k for k, e in enumerate(nd["err"]) if e < crit)
    assert first == nd["criterion_met_at_step"] == 2 and nd["palace_runs_at_criterion"] == first + 1 == 3
    assert nd["prima_palace_runs"] == 9
    contains("meets the criterion at its second step, after 3 Palace runs")
    geode_s = sum(nd["geode_wall_s"][:first + 1])
    assert 65 <= geode_s <= 75
    contains("its three geode-fem solves add about $70$\\,s")
    assert all(200 < s < 270 for s in nd["palace_wall_s"])
    contains("against about $4$\\,min per Palace run")
    contains("one Palace eigensolve at DeviceLayout's discretization, about $4$\\,min on 32 ranks")
    assert min(nd["geode_wall_s"]) >= 19 and max(nd["geode_wall_s"]) <= 26
    contains("gradient ($19$--$25$\\,s)")
    assert f"{nd['err'][3]:.1e}" == "1.4e-07"
    contains(f"$\\mathrm{{err}} = 1.4\\times10^{{-7}}$ at $({nd['capacitor_length_um'][3]:.3f}, "
             f"{nd['total_length_um'][3]:.3f})\\,\\mu$m")
    # Table 5: the steps against the extrapolated frequencies.
    cross_final = cc["convergence"]["final"]
    rows = [("Published final", nd["published_final_um"], cross_final),
            ("Step 1", opt["conv"]["cv1"]["to_um"], opt["conv"]["cv1"]["ladder"]),
            ("Step 2", opt["conv"]["cv2"]["to_um"], opt["conv"]["cv2"]["ladder"])]
    for label, um, lad in rows:
        contains(f"{label} & {um[0]:.3f} & {um[1]:.3f} & {lad['f_q_ghz']:.3f} $[{lad['f_q_band_ghz'][0]:.3f}, "
                 f"{lad['f_q_band_ghz'][1]:.3f}]$ & {lad['f_r_ghz']:.3f} $[{lad['f_r_band_ghz'][0]:.3f}, "
                 f"{lad['f_r_band_ghz'][1]:.3f}]$ \\\\")
    assert opt["conv"]["cv1"]["from_um"] == nd["published_final_um"]
    assert opt["conv"]["cv1"]["from_converged_ghz"] == [cross_final["f_q_ghz"], cross_final["f_r_ghz"]]
    cv1, cv2 = opt["conv"]["cv1"]["ladder"], opt["conv"]["cv2"]["ladder"]
    assert cv1["targets_in_band"] == cv2["targets_in_band"] == [True, True]
    contains(f"overshoots the qubit by ${cv1['over_target_pct'][0]:.2f}\\%$")
    contains(f"lands at $+{cv2['over_target_pct'][0]:.2f}\\%$ and ${cv2['over_target_pct'][1]:.2f}\\%$ of the targets")
    for lad in [cv1, cv2]:
        q = [lad["geode_over_palace_pct"][0], lad["geode_over_palace_pct"][2]]
        assert all(abs(x) <= 2e-4 for x in q)
    contains("agree on the qubit to $2\\times10^{-4}\\,\\%$")
    c2, L2 = opt["conv"]["cv2"]["to_um"]
    c0, L0 = nd["published_final_um"]
    assert opt["conv"]["cv2"]["to_minus_published_final_um"] == [round(c2 - c0, 3), round(L2 - L0, 3)]
    assert f"{pct(c2 / c0 - 1):.1f}" == "18.8" and round(L2 - L0) == 172 and f"{pct(L2 / L0 - 1):.1f}" == "2.4"
    contains("$c = 1446.078\\,\\mu$m, $18.8\\%$ longer than published, with a resonator $2.4\\%$ longer "
             "($+172\\,\\mu$m)")
    step2 = [e for e in log if e["kind"] == "conv_step" and e["step"] == 2][0]
    assert step2["theta"] == [c2, L2] and step2["from"] == opt["conv"]["cv1"]["to_um"]
    slope = step2["S"][0] * step2["dfdc_h"][0]
    assert f"{slope * 1e4:.2f}" == "-9.05"
    contains("= -9.05\\times10^{-4}$\\,GHz/$\\mu$m at the step-1 design, centred on the target")
    absent("centred on the step-2 estimate")
    # The band recentred on the target (not on the step-2 estimate), linear in c.
    b_lo, b_hi = cv2["f_q_band_ghz"]
    c_lo, c_hi = c2 + (b_lo - targets[0]) / -slope, c2 + (b_hi - targets[0]) / -slope
    assert round(c_lo) == 1423 and round(c_hi) == 1690, (c_lo, c_hi)
    p_lo, p_hi = pct(c_lo / c0 - 1), pct(c_hi / c0 - 1)
    assert round(p_lo) == 17 and round(p_hi) == 39, (p_lo, p_hi)
    contains("to $+17\\%$ to $+39\\%$ ($c$ from about $1423$ to $1690\\,\\mu$m)")
    contains("about $+17$ to $+39\\%$ within the band")   # Table 7
    absent("$16$--$38\\%$")
    pq, vq = shared_limit(cv2["p1_f_q_ghz"][1], cv2["p1_f_q_ghz"][2], *cv2["p2_f_q_ghz"])
    pr, vr = shared_limit(cv2["p1_f_r_ghz"][1], cv2["p1_f_r_ghz"][2], *cv2["p2_f_r_ghz"])
    assert f"{vq:.3f}" == "3.039" and f"{vr:.3f}" == "4.019"
    contains(f"puts step~2 at ${vq:.3f}$ and ${vr:.3f}$\\,GHz, would call for a longer capacitor still")
    absent("Each ladder took")
    assert f"{err(nd['published_final_f_ghz'], targets):.2e}" == "1.15e-05"   # Figure 5b, PRIMA published
    return opt, slope


def check_controls(cc, opt, slope):
    """The controls in one Palace environment, the second family at step 2, and the attribution they force."""
    ctl = read(CTL)
    nd = opt["newton_devicelayout"]
    c = ctl["controls"]
    targets = c["targets_ghz"]
    assert c["delta_um"] == opt["transmission_line"]["delta_um"]
    for name in ["scaling", "scaling_palace", "broyden", "prima_this_build"]:
        m = c[name]
        for k, e in enumerate(m["err"]):
            assert abs(err([m["f_q_ghz"][k], m["f_r_ghz"][k]], targets) - e) < 1e-9 + 2e-3 * e, (name, k)
        first = next(k for k, e in enumerate(m["err"]) if e < c["criterion"])
        assert first + 1 == m["palace_runs_at_criterion"], name
        assert m["f_q_ghz"][0] == c["scaling"]["f_q_ghz"][0] and abs(m["f_q_ghz"][0] - nd["f_q_ghz"][0]) < 1e-6
    sc, sp, br, pr = c["scaling"], c["scaling_palace"], c["broyden"], c["prima_this_build"]
    assert c["comparison_1129"]["palace_runs_at_criterion"] == [9, 10, 3, 3, 3, 5]
    assert c["comparison_1129"]["uses_geode_input"] == [False, False, True, True, False, False]
    # Table 4, row by row.
    contains("PRIMA, published & none & none & 9 & unpublished & (1217.678, 7243.955) \\\\")
    contains(f"PRIMA, our environment & none & none & {pr['palace_runs_at_criterion']} & n/a & "
             f"({pr['final_um'][0]:.3f}, {pr['final_um'][1]:.3f}) \\\\")
    # MF-1: one counting rule. Every Palace run a method uses counts, probes included. The Palace-only Newton's
    # delta comes from Broyden's L probe dl_bp2 (Broyden run 3), so it is the recorded 3 runs plus that probe.
    d_of = lambda L1, f1, L2, f2: (L2 * f2 - L1 * f1) / (f1 - f2)
    assert br["c_um"][2] == 620.0 and br["L_um"][2] == 5263.158 and br["L_um"][0] == 5000.0
    d_palace = d_of(br["L_um"][0], br["f_r_ghz"][0], br["L_um"][2], br["f_r_ghz"][2])
    assert abs(d_palace - sp["delta_um"]) < 0.05, d_palace
    sp_runs = sp["palace_runs_at_criterion"] + 1
    assert sp_runs == 4 and br["palace_runs_at_criterion"] == 1 + 2 + 2
    assert f"{sp['delta_um'] / c['delta_um']:.1f}" == "2.6"
    contains("taken instead from a Palace probe ($c$ fixed, $L$ from $5000$ to $5263\\,\\mu$m), so that no geode-fem "
             "input remains, it takes 4 runs: the same two steps, plus the probe. This $\\delta$ is $2.6$ times the "
             "geode-fem one, more than the $\\pm175\\,\\mu$m noise estimate, and costs no extra step.")
    contains("Every Palace run a method uses counts, with the initial design as run~1 and probes included, among them "
             "the probe that sets the Palace $\\delta$ (Broyden's second probe, shared by the two rows); geode-fem "
             "solves are not counted. Each count is one trajectory.")
    contains("We repeat DeviceLayout's optimization against its own discretization, to compare optimizers in one "
             "environment, and against the extrapolated frequencies, to find the design that meets the targets.")
    contains("Palace runs to DeviceLayout's criterion, one trajectory each (Table~\\ref{tab:controls})")
    # #1135 T5: PRIMA at three initial trust radii; 0.4 is not valid on DeviceLayout's box.
    rb = c["prima_rhobeg"]
    assert rb["rhobeg"] == [0.1, 0.2, 0.39999999999999997] and rb["palace_runs_at_criterion"] == [10, 10, 12]
    assert rb["prima_status"] == ["FTARGET_ACHIEVED"] * 3 and rb["newton_runs_at_criterion"] == 3
    assert (1.4 - 0.6) / 2 == 0.39999999999999997 and 0.39999999999999997 < 0.4
    for name, r in [("prima_rhobeg0p1", 0.1), ("prima_rhobeg0p4", 0.39999999999999997)]:
        m = c[name]
        first = next(k for k, e in enumerate(m["err"]) if e < c["criterion"])
        assert first + 1 == m["palace_runs_at_criterion"] == len(m["err"]) and m["rhobeg"] == r, name
        for k, e in enumerate(m["err"]):
            assert abs(err([m["f_q_ghz"][k], m["f_r_ghz"][k]], targets) - e) < 1e-9 + 2e-3 * e, (name, k)
    for label, name in [("\\quad \\texttt{rhobeg} $0.1$", "prima_rhobeg0p1"),
                        ("\\quad \\texttt{rhobeg} $(x_u - x_l)/2$", "prima_rhobeg0p4")]:
        m = c[name]
        contains(f"{label} & none & none & {m['palace_runs_at_criterion']} & n/a & "
                 f"({m['final_um'][0]:.3f}, {m['final_um'][1]:.3f}) \\\\")
    readme = read_text("benchmarks/transmon_devicelayout_controls/README.md")
    assert "MINVAL(XU-XL)/4" in readme and "0.19999999999999998" in readme
    assert "rhoend" not in read_text("benchmarks/transmon_devicelayout_controls/prima_driver.jl")  # PRIMA's default
    # v16 (Opus SF-1, Fable S4): the binary64 aside and the final-radius note move to a footnote; the run counts and
    # the radius stay in the body. Section 7's single paragraph is split in three.
    contains("With \\texttt{rhobeg} at $0.1$ instead of $0.2$, PRIMA also takes 10 runs, and "
             "at the largest radius BOBYQA accepts on DeviceLayout's box, $(x_u - x_l)/2$, it takes 12. Each setting is a "
             "single run.")   # v18 (Fable S1): the ulp aside is in the artifact only
    body_only = re.sub(r"\\footnote\{[^}]*(\{[^}]*\}[^}]*)*\}", "", NORM)
    assert "0.39999999999999997" not in NORM
    for head in ["\\paragraph{Against DeviceLayout's discretization.}", "\\paragraph{PRIMA in our environment.}",
                 "\\paragraph{Where the first steps land.}"]:
        pin(head, 1)
    absent("it again takes 3")
    absent("3 for each Newton variant, 5")
    # The Broyden c probe: step and slope agreement in frequency terms.
    assert br["c_um"][1] == 686.981 and abs(620 / (1 + c["broyden_probe_dx"]) ** 2 - 686.981) < 1e-3
    for label, delta_from, runs, e, um in [
            ("shape derivative", "geode-fem", nd["palace_runs_at_criterion"], nd["err"][1],
             (nd["capacitor_length_um"][2], nd["total_length_um"][2])),
            ("scaling law", "geode-fem", sc["palace_runs_at_criterion"], sc["first_step_err"], sc["final_um"]),
            ("scaling law", "Palace", sp_runs, sp["first_step_err"], sp["final_um"])]:
        contains(f"Newton & {label} & {delta_from} & {runs} & {fmt_err(e)} & ({um[0]:.3f}, {um[1]:.3f}) \\\\")
    contains(f"Broyden & two probes & none & {br['palace_runs_at_criterion']} & {fmt_err(br['first_step_err'])} & "
             f"({br['final_um'][0]:.3f}, {br['final_um'][1]:.3f}) \\\\")
    # The scaling-law first step: err ratio and frequency ratio.
    ratio = nd["err"][1] / sc["first_step_err"]
    assert f"{ratio:.1f}" == "4.7" and f"{math.sqrt(ratio):.1f}" == "2.2"
    # v16: the two err values are Table 3's (checked row by row); the text gives the ratio and points there.
    contains("its first step has an err $4.7$ times lower (Table~\\ref{tab:controls}), about $2.2$ times closer in "
             "frequency")
    absent("times closer (err")
    # The Palace-only Newton: no geode-fem input left.
    assert sp["delta_um"] == c["delta_sensitivity"]["delta_palace_dl_bp2_um"] == 563.879
    assert sp["first_step_um"] == [c["delta_sensitivity"]["first_step_c_um"][2], c["delta_sensitivity"]["first_step_L_um"][2]]
    contains("With $\\delta = 563.879\\,\\mu$m")
    contains("Broyden's method \\citep{broyden1965class} from two forward finite-difference probes, takes 5")
    # PRIMA in our environment.
    assert pr["prima_status"] == "FTARGET_ACHIEVED" and pr["prima_nf"] == pr["palace_runs_at_criterion"] == 10
    assert pr["first_model_step_run"] == 6 and pr["published_palace_runs"] == 9
    assert pr["err"][8] > c["criterion"] and round(pct(pr["err"][8] / c["criterion"] - 1)) == 15
    # v16 (Opus N-5): one more digit, so run 9's 1.150e-4 and the published pair's 1.151e-5 cannot read as a typo.
    contains(f"misses the criterion by $15\\%$ ($\\mathrm{{err}} = {pr['err'][8] * 1e4:.3f}\\times10^{{-4}}$)")
    contains(f"err of its published frequencies, ${err(nd['published_final_f_ghz'], targets) * 1e5:.3f}\\times10^{{-5}}$")
    contains("runs 1--5 are its initial interpolation set of $2n+1 = 5$ points \\citep{powell2009bobyqa}")
    assert f"{pr['published_final_this_build_err']:.1e}" == "1.5e-05" and pr["published_final_this_build_err"] < 1e-4
    contains("In this environment the published final design already meets the criterion ($\\mathrm{err} = "
             "1.5\\times10^{-5}$)")
    contains("In one environment, then, Newton's method in DeviceLayout's variables takes 3 or 4 Palace runs with any "
             "of the three Jacobians of Table~\\ref{tab:controls} (the shape derivative, and the scaling law with "
             "either $\\delta$) and PRIMA 10 to 12; the one run between 3 and 4 is the Palace probe that "
             "geode-fem's $\\delta$ calibration replaces.")   # v23 (flow P7): the three Jacobians are Table 4's Newton rows
    # B3: the abstract and conclusion say which inputs the 3-run count needs, and give the geode-free 4.
    ab = NORM[NORM.index("\\begin{abstract}"):NORM.index("\\end{abstract}")]
    concl = NORM[NORM.index("\\section{Conclusion}"):NORM.index("\\paragraph{Acknowledgments.}")]
    # Opus SF-1: the abstract's last sentence, rewritten; PRIMA over its three trust radii.
    # v19 (lens MUST 2): no derivative credit in the abstract, the introduction or the conclusion; the counting rule
    # is defined at the head of Section 7 and in Table 4's caption.
    assert ("Newton's method in DeviceLayout's variables meets the example's criterion in 3 or 4 Palace runs with any "
            "of three Jacobians, against 10 to 12 for PRIMA; geode-fem's shape derivative gives no advantage "
            "here.") in ab, ab
    assert ("Newton's method in DeviceLayout's variables reaches the criterion in 3 or 4 Palace runs with any of three "
            "Jacobians, against 10 to 12 for PRIMA; geode-fem's shape derivative gives no advantage here.") in concl
    contains("Cost is counted in \\emph{Palace runs}: one Palace eigensolve at DeviceLayout's discretization")
    # Fable M3: the qubit frequency is the linearized (plasma) mode, said at first use in the body.
    assert "All frequencies are linear eigenmodes." in ab
    intro = NORM[NORM.index("\\section{Introduction}"):NORM.index("DeviceLayout.jl \\citep{devicelayout} builds")]
    assert ("is the linear eigenfrequency of the junction's (plasma) mode.\\footnote{Not the transmon's 0--1 "
            "transition $f_{01}$, which lies below the plasma frequency") in intro, intro   # v20: in paragraph 1
    absent("with or without the shape derivative")
    # Why the shape-derivative first step lands farther: the Jacobian entries at the initial design.
    x1 = nd["x1"][0]
    S = nd["f_q_ghz"][0] / nd["geode_f_h_q_ghz"][0]
    j_geode = S * nd["geode_df_dc_q_ghz_per_um"][0] * (-2 * 620 / x1 ** 3)
    j_broyden = (br["f_q_ghz"][1] - br["f_q_ghz"][0]) / c["broyden_probe_dx"]
    j_scaling = nd["f_q_ghz"][0] / x1
    assert f"{j_geode:.2f}" == "3.66" and f"{j_broyden:.2f}" == "3.71" and f"{j_scaling:.3f}" == "4.168"
    assert round(pct(1 - j_geode / j_broyden), 1) == 1.5
    over_probe = (j_broyden - j_geode) * abs(c["broyden_probe_dx"]) / nd["f_q_ghz"][0]
    assert f"{pct(over_probe):.2f}" == "0.07", over_probe
    # v16 (Fable S4 (iii)): the landing paragraph is cut to its slopes; the 0.07 % probe detail and the scaling law's
    # landing point leave the text (asserted above from the artifacts).
    shallow = sorted(round(pct(1 - j / j_scaling)) for j in (j_geode, j_broyden))
    assert shallow == [11, 12], shallow
    chord = (nd["f_q_ghz"][2] - nd["f_q_ghz"][0]) / (nd["x1"][2] - nd["x1"][0])
    assert f"{chord:.1f}" == "4.0" and j_broyden < chord < j_scaling, chord
    contains(f"The shape-derivative and Broyden first steps land at $c = {nd['capacitor_length_um'][1]:.3f}$ and "
             f"${br['c_um'][3]:.3f}\\,\\mu$m, past about $1230\\,\\mu$m at the criterion: both follow a local "
             "slope, $\\partial F_q/\\partial x_1 = 3.66$ and $3.71$\\,GHz, within about $1.5\\%$ of each other and "
             "$11$--$12\\%$ shallower than the scaling law's $F_q/x_1 = 4.168$\\,GHz.")
    absent("\\paragraph{Why the shape-derivative step lands farther.}")   # Fable D9: folded into one paragraph
    at_crit = [nd["capacitor_length_um"][2], sc["final_um"][0], sp["final_um"][0], br["final_um"][0]]
    assert all(abs(x - 1230) < 10 for x in at_crit)
    contains("past about $1230\\,\\mu$m at the criterion")
    pin("$1.5\\%$ of each other", 1)   # v17 (Opus SF-3): stated in Section 7 only; Section 9 points back
    absent("$1.5\\%$ below Broyden's probe")
    # Near-decoupling, from Broyden's two probes.
    own = [abs(pct(br["f_q_ghz"][1] / br["f_q_ghz"][0] - 1)), abs(pct(br["f_r_ghz"][2] / br["f_r_ghz"][0] - 1))]
    other = [abs(pct(br["f_r_ghz"][1] / br["f_r_ghz"][0] - 1)), abs(pct(br["f_q_ghz"][2] / br["f_q_ghz"][0] - 1))]
    assert all(round(o, 1) == 4.5 for o in own) and max(other) < 0.05, (own, other)
    contains("a probe in either variable moves its own mode by $4.5\\%$ and the other by at most $0.05\\%$")
    # The scaling-law converged step (Table 5).
    cs = ctl["conv_step"]["scaling"]
    contains(f"Scaling-law step & {cs['to_um'][0]:.3f} & {cs['to_um'][1]:.3f} & {cs['f_q_ghz']:.3f} "
             f"$[{cs['f_q_band_ghz'][0]:.3f}, {cs['f_q_band_ghz'][1]:.3f}]$ & {cs['f_r_ghz']:.3f} "
             f"$[{cs['f_r_band_ghz'][0]:.3f}, {cs['f_r_band_ghz'][1]:.3f}]$ \\\\")
    short = [-x for x in cs["to_minus_cv2_um"]]
    contains(f"lands ${short[0]:.1f}$ and ${short[1]:.1f}\\,\\mu$m short of step~2")
    contains(f"at $+{cs['over_target_pct'][0]:.2f}\\%$ and $+{cs['over_target_pct'][1]:.2f}\\%$ of the targets")
    assert cs["targets_in_band"] == [True, True]
    # The second refinement family at step 2 (Table 5 row, Section 7, Figure 5).
    fb = ctl["mesh_family"]["cv2"]
    cv2 = opt["conv"]["cv2"]
    assert fb["design_um"] == cv2["to_um"]
    assert fb["family_a_levels_f_q_ghz"] == cv2["ladder"]["p1_f_q_ghz"] + cv2["ladder"]["p2_f_q_ghz"]
    est_q, est_r = rule(fb["family_b_levels_f_q_ghz"]), rule(fb["family_b_levels_f_r_ghz"])
    assert abs(est_q - fb["family_b_f_q_ghz"]) < 1e-4 and abs(est_r - fb["family_b_f_r_ghz"]) < 1e-4
    bq, brr = fb["family_b_f_q_band_ghz"], fb["family_b_f_r_band_ghz"]
    contains(f"Step 2, family b & {cv2['to_um'][0]:.3f} & {cv2['to_um'][1]:.3f} & {est_q:.3f} $[{bq[0]:.3f}, "
             f"{bq[1]:.3f}]$ & {est_r:.3f} $[{brr[0]:.3f}, {brr[1]:.3f}]$ \\\\")
    contains(f"${fb['family_b_f_q_ghz']:.4f}$ $[{bq[0]:.4f}, {bq[1]:.4f}]$\\,GHz for the qubit and "
             f"${fb['family_b_f_r_ghz']:.4f}$\\,GHz for the resonator")
    sp_ = fb["estimate_spread_pct"]
    contains(f"${sp_[0]:.2f}\\%$ and ${sp_[1]:.2f}\\%$ above family~a")   # v23 (flow P9)
    assert fb["targets_in_family_b_band"] == [False, False] and bq[0] > 3.0 and brr[0] > 4.0 and fb["family_b_band_inside_a"]
    contains("both targets lie just below its band, so on family b step 2 is slightly short")
    # The capacitor on target, by the scaling law, from each family's estimate and from the bands.
    law = lambda f: cv2["to_um"][0] * (f / targets[0]) ** 2
    c0 = opt["newton_devicelayout"]["published_final_um"][0]
    on = [law(rule(fb["family_a_levels_f_q_ghz"])), law(est_q)]   # from the unrounded estimates
    assert [round(x, 1) for x in on] == fb["c_on_target_um"], on
    assert [round(pct(x / c0 - 1), 2) for x in on] == fb["c_on_target_over_published_pct"]
    contains(f"call for $c = {on[0]:.0f}$ and ${on[1]:.0f}\\,\\mu$m, $+{pct(on[0] / c0 - 1):.1f}\\%$ and "
             f"$+{pct(on[1] / c0 - 1):.1f}\\%$ over the published length")
    assert fb["union_f_q_band_ghz"] == cv2["ladder"]["f_q_band_ghz"]
    union = [law(x) for x in fb["union_f_q_band_ghz"]]
    assert [round(x, 1) for x in union] == fb["union_c_band_um"]
    up = [pct(x / c0 - 1) for x in union]
    assert [round(x, 2) for x in up] == fb["union_c_band_over_published_pct"]
    bband = [pct(law(x) / c0 - 1) for x in bq]
    assert [round(x, 2) for x in bband] == fb["family_b_c_band_over_published_pct"]
    assert up[0] <= bband[0] and bband[1] <= up[1]       # the union band contains family b's, under the scaling law
    # v16 Opus SF-5: both families mapped the same way, both ways, before containment is claimed.
    lin = lambda f: cv2["to_um"][0] + (f - targets[0]) / -slope      # linear, centred on the target
    a_lin = [pct(lin(x) / c0 - 1) for x in cv2["ladder"]["f_q_band_ghz"]]
    b_lin = [pct(lin(x) / c0 - 1) for x in bq]
    assert [round(x) for x in a_lin] == [17, 39] and [round(x) for x in b_lin] == [19, 28], (a_lin, b_lin)
    assert [round(x) for x in up] == [17, 37] and [round(x) for x in bband] == [19, 27], (up, bband)
    assert a_lin[0] <= b_lin[0] and b_lin[1] <= a_lin[1]    # containment under the linear map
    contains("Their bands map to $+17\\%$ to $+37\\%$ (family a) and $+19\\%$ to $+27\\%$ (family b) under the scaling "
             "law, and to $+17\\%$ to $+39\\%$ ($c$ from about $1423$ to $1690\\,\\mu$m) and $+19\\%$ to $+28\\%$ "
             "linearly")
    contains("Under either mapping the first range contains the second.")
    absent("a range that contains family~b's")
    # The headline capacitor statement: 19-20 % over two families.
    assert [round(x) for x in fb["c_on_target_over_published_pct"]] == [19, 20]
    pin("$19$--$20\\%$", 2)   # abstract, conclusion
    contains("needs $c \\approx 1451$--$1466\\,\\mu$m against the published $1217.678\\,\\mu$m")
    contains("the re-optimized capacitor, $+19$ to $+20\\%$ over two families")
    absent("$+19\\%$ ($+228\\,\\mu$m)")
    # Adaptive refinement on the initial geometry (qubit only).
    a = ctl["amr"]["initial"]
    assert a["nd"][0] == cc["palace"]["initial"]["p2_nd"][0] and a["f_q_ghz"][0] == cc["palace"]["initial"]["p2_f_q_ghz"][0]
    assert all(y > x for x, y in zip(a["f_q_ghz"], a["f_q_ghz"][1:]))
    contains(f"in five passes from ${a['nd'][0] / 1e6:.2f}$ to ${a['nd'][-1] / 1e6:.1f}$ million unknowns, the qubit frequency "
             f"rises from ${a['f_q_ghz'][0]:.4f}$ to ${a['f_q_ghz'][-1]:.4f}$\\,GHz")
    pin(f"${a['f_q_ghz'][-1]:.4f}$\\,GHz", 1)   # Section 5 (v17: the abstract gives percentages)
    assert a["f_q_ghz"][2] > cc["palace"]["initial"]["p2_f_q_ghz"][1] > a["f_q_ghz"][1]
    contains(f"the third pass, at ${a['nd'][2] / 1e6:.1f}$ million unknowns, already exceeds the highest uniform level "
             f"(${cc['palace']['initial']['p2_f_q_ghz'][1]:.4f}$\\,GHz at "
             f"${cc['palace']['initial']['p2_nd'][1] / 1e6:.1f}$ million)")
    assert a["final_in_estimate_band"] and a["final_below_cap"]
    contains(f"${-a['final_minus_estimate_pct']:.2f}\\%$ below the estimate, inside family a's band and below family b's")
    assert a["f_q_ghz"][-1] < read(CTL)["mesh_family"]["initial"]["family_b_f_q_band_ghz"][0]   # v18 (Opus SF-1)
    contains(f"gives ${a['dof_extrapolation_f_q_ghz']:.4f}$\\,GHz ($\\alpha = {a['dof_extrapolation_alpha_q']:.2f}$)")
    af = ctl["amr"]["final"]
    contains(f"moves them by ${1000 * a['dof_extrapolation_f_q_sensitivity_ghz_per_0p1pct_finest']:.0f}$ and "
             f"${1000 * af['dof_extrapolation_f_q_sensitivity_ghz_per_0p1pct_finest']:.0f}$\\,MHz, so they are "
             "ill-conditioned estimates")
    assert not a["final_r_in_estimate_band"] and af["finest_in_union_band"] == [True, False]
    contains("The adaptive resonators rise more slowly and still lie below their bands, so the adaptive runs check the "
             "qubit estimates only.")
    assert f"{a['dof_extrapolation_alpha_r']:.2f}" == "0.25" and not af["dof_extrapolation_r_reliable"]
    contains("(the resonator fit is unreliable)")
    pct7 = pct(a["f_q_ghz"][-1] / cc["published"]["initial_f_ghz"][0] - 1)
    pct8 = pct(af["finest_f_q_ghz"] / af["published_ghz"][0] - 1)
    assert f"{pct7:.1f}" == "7.0" and f"{pct8:.1f}" == "8.0"
    contains("Palace's still-rising adaptive refinement puts the initial and final designs' qubit frequencies "
             "$7.0\\%$ and $8.0\\%$ above their "
             "published values without extrapolation.")
    contains("A further check comes from Palace's adaptive mesh refinement")
    absent("An independent check comes from Palace")
    # The second refinement family on the initial geometry.
    f = ctl["mesh_family"]["initial"]
    assert f["family_a_f_q_ghz"] == round(cc["convergence"]["initial"]["f_q_ghz"], 4)
    assert f["family_b_band_inside_a"] and f["family_a_f_q_band_ghz"] == f["union_f_q_band_ghz"]
    contains(f"Its $h/2$ qubit is ${f['h2_f_q_ghz'][1]:.4f}$\\,GHz against gmsh's ${f['h2_f_q_ghz'][0]:.4f}$\\,GHz")
    contains(f"at $h_F/2$ the two refinements of the fixture give ${f['h2_f_q_ghz'][3]:.4f}$ and ${f['h2_f_q_ghz'][2]:.4f}$\\,GHz")
    rs, mf = f["refinement_scheme_pct"], f["mesh_family_pct"]
    contains(f"accounts for ${min(rs):.1f}$--${max(rs):.1f}\\%$ of the $h/2$ level and the choice of starting mesh for only "
             f"${min(mf):.2f}$--${max(mf):.2f}\\%$")
    bq0, br0 = f["family_b_f_q_band_ghz"], f["family_b_f_r_band_ghz"]
    # v16: family b's estimates and bands are Table 2's column (checked there); the text keeps orders and spreads.
    assert f"{f['family_b_observed_order_q']:.2f}" == "0.91"
    assert f"{f['estimate_spread_pct'][0]:.2f}" == "0.64"
    lo_, hi_ = f["estimate_range_f_q_ghz"]
    rng = f"${lo_:.3f}$--${hi_:.3f}$\\,GHz"
    assert rng == "$4.444$--$4.472$\\,GHz"
    pin(rng, 1)   # v17: Section 5 (Table 5's repeat cut, Fable S7)
    pub = cc["published"]["initial_f_ghz"][0]
    contains(f"{rng}, ${pct(lo_ / pub - 1):.1f}$--${pct(hi_ / pub - 1):.1f}\\%$ above the published value")
    # Table 2 footnote: family b on both geometries, from the unrounded estimates and bands (#1135 T4 for final).
    ff = ctl["mesh_family"]["final"]
    assert ff["design_um"] == [1217.678, 7243.955]
    assert ff["family_a_levels_f_q_ghz"] == cc["palace"]["final"]["p1_f_q_ghz"] + cc["palace"]["final"]["p2_f_q_ghz"]
    assert ff["family_b_levels_f_q_ghz"][0] == ff["family_a_levels_f_q_ghz"][0]   # shared h cells
    vals = []
    for fam_, mode in [(f, "q"), (f, "r"), (ff, "q"), (ff, "r")]:
        if fam_ is ff:
            assert abs(rule(ff[f"family_b_levels_f_{mode}_ghz"]) - ff[f"family_b_f_{mode}_ghz"]) < 1e-4, mode
        b_ = fam_[f"family_b_f_{mode}_band_ghz"]
        vals.append(f"${fam_[f'family_b_f_{mode}_ghz']:.3f}$ $[{b_[0]:.3f}, {b_[1]:.3f}]$")
    assert len(vals) == 4   # v16: these are the Table 2 family-b column, checked row by row in check_crosscheck
    bqf = ff["family_b_f_q_band_ghz"]
    assert ff["family_b_band_inside_a"] and ff["published_below_family_b_band"] == [True, True]
    assert f"{ff['family_b_observed_order_q']:.2f}" == "0.90" and f"{ff['family_a_observed_order_q']:.2f}" == "0.61"
    contains("a plausible reason for its higher observed rates, $0.91$ (initial) and $0.90$ (final) against "
             "family~a's $0.62$ and $0.61$.")
    contains(f"Its qubit estimates lie ${f['estimate_spread_pct'][0]:.2f}\\%$ and "
             f"${ff['estimate_spread_pct'][0]:.2f}\\%$ above family~a's")
    lo_f, hi_f = ff["estimate_range_f_q_ghz"]
    po = ff["f_q_over_published_pct"]
    contains(f"and the final one as ${lo_f:.3f}$--${hi_f:.3f}$\\,GHz, $+{po[0]:.1f}\\%$ to $+{po[1]:.1f}\\%$ "
             "over the published $3.005$\\,GHz.")
    absent(f"the final qubit ${lo_f:.3f}$--${hi_f:.3f}$\\,GHz")   # v17: Table 5's repeat cut
    # Opus SF-6: family b is partly graded toward the sheets.
    contains("so it is graded slightly toward the sheets, a plausible reason for its higher observed rates")
    # The common-limit fits, every first-family series.
    sl = ctl["shared_limit"]
    lim = dict(zip(sl["series"], zip(sl["order"], sl["f_ghz"])))
    ests = {"initial_q": cc["convergence"]["initial"]["f_q_ghz"], "initial_r": cc["convergence"]["initial"]["f_r_ghz"],
            "final_q": cc["convergence"]["final"]["f_q_ghz"], "final_r": cc["convergence"]["final"]["f_r_ghz"],
            "cv2_q": cv2["ladder"]["f_q_ghz"], "cv2_r": cv2["ladder"]["f_r_ghz"],
            "sccv1_q": cs["f_q_ghz"], "sccv1_r": cs["f_r_ghz"]}
    over = [pct(lim[k][1] / v - 1) for k, v in ests.items()]
    assert all(o > 0 for o in over) and f"{min(over):.1f}" == "0.5" and f"{max(over):.1f}" == "1.3", over
    contains("it lies $0.5$--$1.3\\%$ above the rule's estimate on every family-a series, as expected of a rule "
             "that leans low")
    sens = sl["initial_q_sensitivity_ghz_per_0p1pct"][3]
    contains(f"$+0.1\\%$ on the $p = 2$ $h/2$ level alone moves the initial-qubit fit by ${1000 * sens:.0f}$\\,MHz")
    cap = ctl["cap"]["initial"]
    assert cap["tier"].startswith("model-dependent") and cap["f_q_ghz"] == cc["static_cross_check"]["initial"]["circuit_cap_f_q_ghz"]
    exc = lim["initial_q"][1] - cap["f_q_ghz"]
    assert abs(exc - sl["initial_q_excess_over_cap_ghz"]) < 1e-9
    assert sl["initial_q_excess_as_p2_h2_change_pct_exact"] < 0.1   # within the fit's 20 MHz per 0.1 % conditioning
    contains(f"family~a's common-limit fit, ${lim['initial_q'][1]:.4f}$\\,GHz, exceeds it by "
             f"${1000 * exc:.1f}$\\,MHz, within that fit's conditioning")
    assert lim["family_b_initial_q"][1] < cap["f_q_ghz"] and f["all_estimates_below_cap"]
    # Judge nit: "not in the Bounded tier", not "model-dependent, not bounded".
    contains("so it is model-dependent and not in the Bounded tier")
    absent("model-dependent, not bounded")
    # The attribution: no run count or design credited to the shape derivative.
    for stale in ["Cost to the criterion", "price of entry", "bounds implementation error", "makes it cheap",
                  "$7$--$9\\%$ below", "Jacobian comes from", "Gradient-Assisted", "gradient re-optimization",
                  "thanks to the", "because of the shape derivative", "the derivative reduces"]:
        absent(stale, BODY)
    contains("Agreement between two codes is evidence against implementation error")
    assert not re.search(r"both (?:solvers |codes )?converge", LOWBODY)
    assert not re.search(r"two independently implemented solvers (?:converge|reach)", LOWBODY)
    assert not re.search(r"no geode-fem solve", LOWBODY)


def check_derivative_discussion():
    """The operator's ruling: why the derivative gives no advantage here, and where derivatives pay off."""
    d = section("\\paragraph{Why the shape derivative gives no advantage here.}", "\\paragraph{Threats and limitations.}")
    for text in ["two nearly decoupled parameters", "Newton's method converges with any reasonable Jacobian",
                 "the scaling law $f_q \\propto x_1$, which costs nothing",
                 "a finite-difference gradient costs only $N+1 = 3$ solves",
                 "the coarse-mesh, fixed-topology derivative is the slope of a different discrete problem",
                 "once scaled by the ratio $S$ it gives about what Broyden's probe gives "
                 "(Section~\\ref{sec:reopt})",
                 "a discrete adjoint gives all $N$ derivatives of one objective for about one extra solve",
                 "coupled targets with no per-parameter scaling law", "freeform shapes",
                 "\\citep{walters2026conformal}", "At 38 shape parameters",
                 # v17: the companion's comparison is finished; scoped to its measured ratios.
                 "A companion paper on adjoint shape optimization of a conformal patch antenna",
                 "At 38 shape parameters the adjoint reaches a $-9.5$\\,dB target with about $11$ times fewer solves "
                 "than the fastest gradient-free run that reached it",
                 "a tuned full-covariance CMA-ES comes within $7.7$ times at $-9$\\,dB",
                 "at 4 parameters the ratio is $1.1$ at $-9$\\,dB and $2.4$ at $-9.5$\\,dB."]:
        assert text in d, text
    for claim in ["demonstrates an advantage", "shows the advantage", "outperforms"]:
        assert claim not in d, claim
    absent("a remeshed finite difference of that objective is the more faithful slope", BODY)
    # S2 / SF-2: the designer's quantities.
    e = section("\\paragraph{What the shift means for a design.}", "\\paragraph{What a practitioner can do.}")
    ratio = (4.4297 / 4.14) ** 2
    assert f"{(4.4297 / 4.14):.3f}" == "1.070" and f"{ratio:.3f}" == "1.145" and round(pct(ratio - 1)) == 14
    # v16 (Opus SF-2, Fable S1): E_C from the bound, rounded outward from the unrounded tensor family bounds and from
    # the rounded fF bracket (both give the same printed range), against the published-implied value at fixed L_J.
    e_ch, h_pl, lj, cj = 1.602176634e-19, 6.62607015e-34, 14.860e-9, 5.5
    tens = read(DIFFOPT + "c_sigma_bracket_tensor.toml")
    lo_u = max(max(v for k, v in r.items() if k.startswith("lower_")) for r in tens["adaptive_x0_tensor"])
    hi_u = min(r["upper_ff"] for r in tens["adaptive_x0_tensor"])
    ec_of = lambda c_ff: e_ch ** 2 / (2 * c_ff * 1e-15 * h_pl) / 1e9
    for lo_c, hi_c in [(lo_u, hi_u), (tens["bracket_x0_tensor"]["lower_ff"], tens["bracket_x0_tensor"]["upper_ff"])]:
        assert (floor_to(ec_of(hi_c + cj), 4), ceil_to(ec_of(lo_c + cj), 4)) == (0.2271, 0.2278), (lo_c, hi_c)
    c_of = lambda f: 1 / ((2 * math.pi * f * 1e9) ** 2 * lj) * 1e15     # bare LC at fixed L_J (L_s = k = 0)
    c_pub, c_ad = c_of(4.14), c_of(4.4297)
    assert f"{c_pub:.2f}" == "99.45" and f"{ec_of(c_pub):.3f}" == "0.195" and f"{c_ad:.2f}" == "86.87"
    # v18 (Opus M1): ONE two-node circuit, the one Section 3.3 states: bare qubit 1/(2 pi sqrt((L_J + L_s) C_Sigma)),
    # C_Sigma = C_isl + C_J, Maxwell-form coupling k to the resonator. Forward (static route): dress the bare pair.
    # Backward (published value): invert the dressed pair [4.14, 5.591] GHz exactly, then C_Sigma from the bare qubit.
    # (The transmon_loop_inductance [anchor] uses a different topology, C_J across L_J alone, and is not used.)
    anc = read("benchmarks/transmon_loop_inductance/results.toml")["anchor"]
    assert anc["l_s_band_nh"] == [0.113, 0.125] and anc["k_band"] == [0.0227, 0.031]
    ls_band, k_band = [x * 1e-9 for x in anc["l_s_band_nh"]], anc["k_band"]

    def dressed_lo(fq, fr, k):
        x, y, q = fq * fq, fr * fr, 1 - k * k
        return math.sqrt((x + y - math.sqrt((x + y) ** 2 - 4 * q * x * y)) / (2 * q))

    def bare_lo(flo, fhi, k):
        q = 1 - k * k
        x1, x2 = flo * flo, fhi * fhi
        ss, pp = q * (x1 + x2), q * x1 * x2
        return math.sqrt((ss - math.sqrt(ss * ss - 4 * pp)) / 2)

    two_node_c = lambda fq, ls: 1 / ((lj + ls) * (2 * math.pi * fq) ** 2) * 1e15
    two_node_f = lambda c_ff, ls: 1 / (2 * math.pi * math.sqrt((lj + ls) * c_ff * 1e-15))
    for k in k_band:   # the inversion is the exact inverse of the dressing (f_q^2 f_r^2 = (1 - k^2) f_lo^2 f_hi^2)
        fq = bare_lo(4.14e9, 5.591e9, k)
        fr = math.sqrt((1 - k * k) * (4.14e9 * 5.591e9) ** 2) / fq
        assert abs(dressed_lo(fq, fr, k) - 4.14e9) < 1.0
    pub_c = [two_node_c(bare_lo(4.14e9, 5.591e9, k), ls) for k in k_band for ls in ls_band]
    full = [min(pub_c), max(pub_c)]
    assert (f"{full[0]:.2f}", f"{full[1]:.2f}") == ("98.51", "98.64"), full
    assert (f"{anc['c_sigma_band_low_unrounded_ff'] + cj:.2f}", f"{anc['c_sigma_band_high_unrounded_ff'] + cj:.2f}") \
        == ("98.59", "98.72")   # the other topology, no longer printed
    absent("98.59")
    absent("98.72")
    # The static route from the same formula reproduces the committed grounded and natural bands.
    gbx = read(DIFFOPT + "c_sigma_bracket_grounded_box.toml")
    fr_est = gbx["static_route"]["resonator_f_r_ghz"] * 1e9
    for key, (cl, cu) in [("grounded_dressed_f_q_band_ghz", (79.56, 79.77)), ("natural_dressed_f_q_band_ghz", (79.56, 79.76))]:
        v = [dressed_lo(two_node_f(c + 5.5, ls), fr_est, k) / 1e9 for c in (cl, cu) for ls in ls_band for k in k_band]
        lo_, hi_ = gbx["static_route"][key]
        assert abs(min(v) - lo_) < 1e-4 and abs(max(v) - hi_) < 1e-4, (key, min(v), max(v))
    assert floor_to(pct(c_pub / (hi_u + cj) - 1), 1) == 16.6 and floor_to(pct(c_pub / 85.26 - 1), 1) == 16.6
    st_band = read(CROSS)["static_cross_check"]["initial"]["dressed_f_q_band_ghz"]
    c_st = sorted(c_of(f) for f in st_band)
    assert (f"{c_st[0]:.1f}", f"{c_st[1]:.1f}") == ("85.8", "86.1"), c_st
    over_st = sorted(pct(c_pub / c - 1) for c in c_st)
    assert (f"{over_st[0]:.1f}", f"{over_st[1]:.1f}") == ("15.5", "15.9"), over_st
    over_ad = pct(c_pub / c_ad - 1)
    assert f"{over_ad:.1f}" == "14.5" and c_ad > 85.27 and all(c > 85.27 for c in c_st)
    # v17 (Fable M1): every percentage names its referent, the bound is the grounded box's [85.06, 85.27] fF, and
    # the split of the factor is computed, not "about half" (with 14.5 % unrounded, the adaptive-to-static gap is
    # 1.0-1.4 of 2.2 points, so neither "about half" nor "more than half" holds at both ends).
    gb = read(DIFFOPT + "c_sigma_bracket_grounded_box.toml")["bracket_x0_tensor_grounded_box"]
    g_lo, g_hi = gb["lower_ff"] + cj, gb["upper_ff"] + cj
    assert (round(g_lo, 2), round(g_hi, 2)) == (85.06, 85.27)
    assert (floor_to(ec_of(g_hi), 4), ceil_to(ec_of(g_lo), 4)) == (0.2271, 0.2278)
    tn = [pct(full[0] / g_hi - 1), pct(full[1] / g_lo - 1)]
    assert (floor_to(tn[0], 1), ceil_to(tn[1], 1)) == (15.5, 16.0), tn
    bare = [pct(c_pub / g_hi - 1), pct(c_pub / g_lo - 1)]
    assert (floor_to(bare[0], 1), ceil_to(bare[1], 1)) == (16.6, 17.0), bare
    fac = [c_pub / g_hi, c_pub / g_lo]
    assert (floor_to(fac[0], 3), ceil_to(fac[1], 3), f"{c_pub / c_ad:.3f}") == (1.166, 1.170, "1.145"), fac   # outward
    # v18 (Opus SF-2): the split multiplies back: 1.145 x (adaptive reading over the bound, 1.9-2.1 %) = the factor.
    rest = [c_ad / g_hi, c_ad / g_lo]
    assert (f"{pct(rest[0] - 1):.1f}", f"{pct(rest[1] - 1):.1f}") == ("1.9", "2.1"), rest
    assert all(abs((c_pub / c_ad) * r - f) < 1e-12 for r, f in zip(rest, fac))
    gst = read(DIFFOPT + "c_sigma_bracket_grounded_box.toml")["static_route"]["grounded_dressed_f_q_band_ghz"]
    cg = sorted(c_of(f) for f in gst)
    ad_st = sorted(pct(c_ad / c - 1) for c in cg)
    st_b = [pct(cg[0] / g_hi - 1), pct(cg[1] / g_lo - 1)]
    assert (f"{ad_st[0]:.1f}", f"{ad_st[1]:.1f}") == ("0.9", "1.3"), ad_st
    assert (f"{st_b[0]:.1f}", f"{st_b[1]:.1f}") == ("0.6", "1.2"), st_b
    for text in ["The bound gives $E_C/h = e^2/2hC_\\Sigma$ in $[0.2271, 0.2278]$\\,GHz for the grounded box, Bounded "
                 "as a function of $C_\\Sigma$ and model-dependent as the transmon's charging energy (a single-island "
                 "lumped picture).",
                 "the published $4.14$\\,GHz implies a $C_\\Sigma$ $15.5$--$16.0\\%$ above the bound",
                 "$16.6$--$17.0\\%$ above the bound for the published value (rounded outward).",
                 "Of that excess, taken as a factor, the computed finest adaptive pass accounts for $1.145$, and the remaining $1.9$--$2.1\\%$ "
                 "splits between the still-rising pass and $L_s$ and $k$.",
                 "so it reads every frequency as a larger $C_\\Sigma$",   # Fable N1
                 "$L_J$ can retune the qubit frequency but not $E_C$."]:
        assert text in e, text
    for stale in ["15.6$--$16.1", "0.9$--$1.3", "0.6$--$1.2", "1.166$--$1.170"]:   # v18: Table 6 carries the rest
        assert stale not in e, stale
    assert "about half" not in e and "about $14\\%$" not in e
    assert "at least $16.6\\%$, and" not in e      # the bare-reading figure is not stated as the model's
    # Fable M3: f01 = f_p - E_C/h (Koch), with C at fixed L_J inferred from each linear-mode frequency.
    ec = lambda f: e_ch ** 2 * (2 * math.pi * f) ** 2 * lj / (2 * h_pl)
    ecs = [ec(f * 1e9) / 1e9 for f in (4.14, 4.4297)]
    assert [f"{x:.2f}" for x in ecs] == ["0.19", "0.22"], ecs
    f01 = [f - x for f, x in zip((4.14, 4.4297), ecs)]
    assert f"{pct(f01[1] / f01[0] - 1):.1f}" == "6.6", f01
    assert ("The transition $f_{01}$ rises by a little less than the linear mode, about $6.6\\%$ at the finest adaptive "
            "pass, and") in e
    # v17 (Opus SF-3): the design numbers as Table 6, recomputed.
    contains("Bound & Bounded & -- & $[85.06, 85.27]$ & $[0.2271, 0.2278]$ \\\\")
    contains(f"Static route & Estimated & $[4.449, 4.458]$ & ${cg[0]:.1f}$--${cg[1]:.1f}$ (bare) & "
             f"${ec_of(cg[1]):.3f}$--${ec_of(cg[0]):.3f}$ (bare) \\\\")
    contains(f"Adaptive finest pass & Computed & $4.4297$ & ${c_ad:.2f}$ (bare) & ${ec_of(c_ad):.3f}$ (bare) \\\\")
    contains(f"Published & -- & $4.14$ & $[{full[0]:.2f}, {full[1]:.2f}]$; ${c_pub:.2f}$ (bare) & ${ec_of(full[1]):.3f}$--${ec_of(full[0]):.3f}$; "
             f"${ec_of(c_pub):.3f}$ (bare) \\\\")
    assert (f"{ec_of(full[1]):.3f}", f"{ec_of(full[0]):.3f}") == ("0.196", "0.197")
    contains("At $L_s = k = 0$ this is the plasma frequency $\\sqrt{8E_JE_C}/h$, and the transmon's $f_{01} \\approx "
             "\\sqrt{8E_JE_C}/h - E_C/h$ lies below it.")
    p = section("\\paragraph{What a practitioner can do.}", "\\section{Conclusion}")
    for text in ["graded toward the edges", "$hp$ refinement \\citep{demkowicz2006computing}",
                 "Palace's adaptive runs (Section~\\ref{sec:crosscheck}) and the gap-refined",   # v16, Opus SF-4
                 # v17 (Opus SF-1, Fable S5): Ye et al.'s setting and the one-element-wide strips.
                 "the gap-refined, fourth-order setting of \\citet{ye2025electromagnetic}",
                 "$993$ of the $1{,}007$ edges that trigger Palace's bisection lie where a resonator or feedline strip "
                 "is one element wide",
                 "\\citep{wang2015surface,martinis2022surface}"]:   # v18: one clause
        assert text in p, text


# --------------------------------------------------------------------------
# Citations, operator items, voice and audience
# --------------------------------------------------------------------------

def bib_entries(text):
    out = {}
    for e in re.split(r"\n(?=@)", text):
        m = re.match(r"\s*@\w+\{([^,]+),", e)
        if m:
            out[m.group(1)] = "\n".join(l for l in e.strip().splitlines() if not l.startswith("%")).strip()
    return out


def bib_fields(entry):
    kind = re.match(r"\s*@(\w+)\{", entry).group(1).lower()
    fields = {k.lower(): v.strip() for k, v in re.findall(r"^\s*(\w+)\s*=\s*\{(.*)\},?\s*$", entry, re.M)}
    return kind, fields


V13_DOIS = {"foxkapoor1968rates": "10.2514/3.5008", "demkowicz2006computing": "10.1201/9781420011685"}
V13_NEW = {"foxkapoor1968rates", "powell2009bobyqa", "demkowicz2006computing", "walters2026conformal"}
# v14 additions, checked against Crossref (title, venue, volume, article number, year, authors).
V14_DOIS = {"koch2007charge": "10.1103/PhysRevA.76.042319", "minev2021energy": "10.1038/s41534-021-00461-8"}
V15_DOIS = {"wang2015surface": "10.1063/1.4934486", "martinis2022surface": "10.1038/s41534-022-00530-6"}
# v16 (Opus MF-3, Fable S7): Richardson 1911 and Roache 1994 resolved on Crossref 2026-10-15 (title, venue, volume,
# issue, pages, year, author); Minev et al. 2021 on quasi-lumped models read on arXiv and DataCite 2026-10-15
# (arXiv:2103.10344, no journal version; its abstract is the source of the sentence that cites it).
V16_DOIS = {"richardson1911approximate": "10.1098/rsta.1911.0009", "roache1994perspective": "10.1115/1.2910291"}
ARXIV = {"eriksson2025automated", "sommers2025open", "ye2025electromagnetic", "minev2021circuit"}


def check_citations():
    keys = {k.strip() for group in re.findall(r"\\cite\w*(?:\[[^]]*\])*\{([^}]+)\}", TEX) for k in group.split(",")}
    refs = bib_entries((HERE / "refs.bib").read_text())
    assert keys == set(refs), (sorted(keys - set(refs)), sorted(set(refs) - keys))
    # v15: v14's entries plus two Crossref-checked surface-participation papers; the reader-facing notes are reduced
    # to the software version or venue (Opus N-1), and the example's title uses a colon (Fable N6). Every other
    # field is v14's.
    v14 = bib_entries((HERE.parent / "transmon-benchmark.14" / "refs.bib").read_text())
    v15 = bib_entries((HERE.parent / "transmon-benchmark.15" / "refs.bib").read_text())
    new16 = set(V16_DOIS) | {"minev2021circuit"}
    assert set(refs) - set(v15) == new16, sorted(set(refs) - set(v15))
    assert set(refs) - set(v14) == set(V15_DOIS) | new16, sorted(set(refs) - set(v14))
    assert set(v14) <= set(refs) and set(v15) <= set(refs)
    TRUNC = {"eriksson2025automated", "mfem", "shanto2024squadds", "sommers2025open", "koch2007charge"}
    for key in v15:   # v15's entries unchanged, except the v17 updates and v18's "and others" author lists
        if key in ("burn", "walters2026conformal"):
            continue
        f, of = dict(bib_fields(refs[key])[1]), dict(bib_fields(v15[key])[1])
        if key in TRUNC:   # v18: first author kept, the rest "and others" (page length); nothing else changed
            first = of["author"].split(" and ")[0].strip()
            assert f.pop("author") == f"{first} and others", key
            of.pop("author")
        assert f == of, key
    assert "Richardson, Lewis Fry" in refs["richardson1911approximate"] and "307--357" in refs["richardson1911approximate"]
    assert "Roache, Patrick J." in refs["roache1994perspective"] and "405--413" in refs["roache1994perspective"]
    assert "2103.10344" in refs["minev2021circuit"] and "quasi-lumped" in refs["minev2021circuit"]
    assert TEX.count("richardson1911approximate") == TEX.count("roache1994perspective") == 1
    assert TEX.count("minev2021circuit") == 1
    for key, doi in {**V14_DOIS, **V15_DOIS, **V16_DOIS}.items():
        assert bib_fields(refs[key])[1]["doi"] == doi, key
    assert "Wang, C. and Axline" in refs["wang2015surface"] and "162601" in refs["wang2015surface"]
    assert "Martinis, John M." in refs["martinis2022surface"] and "{26}" in refs["martinis2022surface"]
    assert "Minev" in refs["minev2021energy"] and "131" in refs["minev2021energy"]
    assert "Koch, Jens" in refs["koch2007charge"] and "042319" in refs["koch2007charge"]
    for key in ARXIV:   # eprint with its archive prefix
        f = bib_fields(refs[key])[1]
        assert f["archiveprefix"] == "arXiv" and f["eprint"] and f["eprinttype"] == "arxiv", key
    # v17 (Fable N2): Burn v0.21.0 was released 2026-05-07 (GitHub release), so the year is 2026.
    changed = {"burn": {"note": "Software, v0.21.0", "year": "2026"}, "devicelayout": {"note": "Software, v1.15.0"},
               "devicelayout-blog": {"note": "AWS Quantum Technologies Blog"},
               "palace": {"note": "Software, commit fba6a5b"},
               "devicelayout-example": {"title": "{DeviceLayout.jl}: {Single-Transmon Simulation}", "note": None}}
    for key in set(v14) - {"walters2026conformal"} - TRUNC:   # v17: the companion entry is checked in check_v17
        f, of = bib_fields(refs[key])[1], bib_fields(v14[key])[1]
        for field, val in changed.get(key, {}).items():
            of.pop(field, None)
            if val is None:
                assert field not in f, (key, field)
            else:
                assert f.pop(field) == val, (key, field)
        assert f == of, key
    for key, entry in refs.items():
        for stale in ["cited as software", "No journal", "used in this work", "source of the", "Source of the"]:
            assert stale not in entry, (key, stale)
    assert "$\\cdot$" not in refs["devicelayout-example"]
    for key, entry in refs.items():
        _, f = bib_fields(entry)
        assert not ("doi" in f and "url" in f), f"url beside a DOI: {key}"
    for key, doi in V13_DOIS.items():
        assert bib_fields(refs[key])[1]["doi"] == doi, key
    assert bib_fields(refs["powell2009bobyqa"])[1]["number"] == "DAMTP 2009/NA06"
    # v17: the companion's title will change before deposit; the entry carries a generic working title and says it
    # is a Zenodo preprint in preparation (both papers are deposited on Zenodo only).
    cf = bib_fields(refs["walters2026conformal"])[1]
    assert cf["title"] == "Adjoint shape optimization of a conformal patch antenna", cf
    assert cf["note"] == "Working title. Preprint in preparation, to be deposited on Zenodo", cf
    assert "arxiv" not in refs["walters2026conformal"].lower()
    # The eigenvalue derivative is credited to Fox and Kapoor; Nelson only for eigenvector derivatives.
    contains("port EPR $0.99424$ in Palace, the energy-participation ratio of the junction")
    assert TEX.count("\\citep{koch2007charge}") == 2 and TEX.count("\\citet{koch2007charge}") == 1
    assert TEX.count("minev2021energy") == 2 and TEX.count("wang2015surface") == TEX.count("martinis2022surface") == 1
    contains("the classical eigenvalue derivative of a symmetric generalized eigenproblem \\citep{foxkapoor1968rates}")
    assert TEX.count("nelson1976simplified") == 1
    contains("which need eigenvector derivatives \\citep{nelson1976simplified}")   # v17: in the Not-claimed row
    absent("Hellmann--Feynman (Nelson)")
    absent("liang2026adaptive")
    # Prior-art claims.
    for stale in ["uncontested", "none exposes an adjoint", "no derivative path", "elegant", "owned by photonics"]:
        absent(stale, BODY)
    for pattern in [r"\bfirst to\b", r"for the first time", r"\bunprecedented\b", r"\bnovel\b",
                    r"to our knowledge", r"\bno prior work\b",
                    r"\bno (?:other|existing|prior) (?:solver|code|tool|work)s? (?:exposes?|provides?|offers?|computes?|delivers?)",
                    r"\bnone (?:of them )?(?:exposes?|provides?|offers?)", r"\bonly (?:solver|code|tool) (?:that|to|with)\b",
                    r"\bfirst (?:shape|adjoint|gradient|differentiable|sensitivity)"]:
        assert not re.search(pattern, LOWBODY), pattern
    phrase("The method is not new; we apply it")
    # v16 (Opus SF-7): the absence claim carries its search scope, which is the committed litsearch's log.
    lit = re.sub(r"\s+", " ", read_text("papers/transmon-benchmark/transmon-benchmark.9.litsearch/notes.md"))
    assert '"guaranteed capacitance bounds qubit" (nothing found)' in lit and "## Search log (2026-10-12)" in lit
    assert "Prager–Synge, Braess–Schöberl" in lit and "Specogna" in lit
    phrase("A web search in October 2026 of the complementary-bound and qubit-design literature (queries such as "
           "``guaranteed capacitance bounds qubit'') found no two-sided capacitance bound in the qubit-design "
           "literature.")
    absent("we have found no two-sided capacitance bound")
    # v16 (Opus SF-6): how measurement-matching workflows converge, from Ye et al.'s own text (arXiv:2511.09041,
    # Section III: refinement near CPW gaps, fourth-order elements, "within +-0.3% of the extrapolated asymptotic
    # values"; the 0.3 % measurement agreement is on resonator frequencies) and from Minev et al.'s Methods
    # ("Convergence can be verified by plotting p_mj as a function of the simulation adaptive pass number").
    phrase("matching resonator frequencies to within $0.3\\%$ with a mesh refined at the CPW gaps and fourth-order "
           "elements, a setting their convergence study puts within $\\pm0.3\\%$ of extrapolated values.")
    absent("energy-participation analyses likewise check convergence")   # v19: clause cut for length
    absent("at the ${\\sim}0.3\\%$ level")
    # v16 (Fable S7): the two-node circuit against the quasi-lumped route, in the words of Minev et al.'s abstract.
    phrase("The two-node circuit of Section~\\ref{sec:junction} is a minimal instance of the quasi-lumped route "
           "\\citep{minev2021circuit}, which partitions a device into lumped or quasi-distributed cells, simulates each "
           "and reduces the composite to coupled subsystems")
    # The example's own caveat, quoted as DeviceLayout v1.15.0 docs/src/examples/singletransmon.md states it for
    # the order-2 runs (the "quite sensitive" remark there is about the order-1 demo only).
    phrase("They run Palace at second order ``to reduce sensitivity to the discretization'' and note that this "
           "``does not mean that the eigenfrequencies found by Palace will have converged to their true values''")
    absent("the example itself warns that they are sensitive to it")
    absent("quite sensitive")
    # Every figure is cited in the text.
    for lab in ["geometry", "crosscheck", "initialq", "convergence", "reopt"]:
        assert re.search(r"Figure~\\ref\{fig:" + lab + r"\}", BODY), lab
    contains("Table~\\ref{tab:controls} and Figure~\\ref{fig:reopt}(b) compare")
    phrase("the field behaves as $E \\sim r_e^{-1/2}$ in the distance $r_e$ from the edge \\citep{meixner1972behavior}")
    phrase("DeviceLayout's example optimizes with PRIMA \\citep{zhang2023prima}")
    phrase("with bounds only it runs BOBYQA \\citep{powell2009bobyqa}")
    phrase("As of October 2026, neither Palace nor DeviceLayout.jl has a methods publication")
    for k in ["prager1947approximations", "raviart1977mixed", "braess2007equilibrated", "specogna2011complementary",
              "nikolova2004adjoint", "sommers2025open", "ye2025electromagnetic", "boffi2010finite"]:
        assert k in keys, k
    return keys


def refs_entry(key):
    return bib_entries((HERE / "refs.bib").read_text())[key]


def band_values(levels):
    """The six band values of the part-1 rule from five levels (p1 h, h/2, h/4; p2 h, h/2), with their names."""
    a, b, c, d, e = levels
    r = 2 ** observed_order([a, b, c]) - 1
    return {"p1_order1": 2 * c - b, "p2_order1": 2 * e - d,
            "p1_lambda": math.sqrt(2 * c * c - b * b), "p2_lambda": math.sqrt(2 * e * e - d * d),
            "p1_observed": c + (c - b) / r, "p2_borrowed": e + (e - d) / r}


def check_v16():
    """Guards added for the v16 fixes (Opus and Fable reviews of v15)."""
    cc, ctl, opt = read(CROSS), read(CTL), read(OPT + "results.toml")
    # Opus MF-1: on every ladder with a band, the high end is the order-1 three-level value at its observed order,
    # the borrowed-order order-2 value lies inside, and the low end is the order-1 lambda value except on family-b
    # resonators (order-2 lambda). Recomputed from the levels and matched to each artifact's band.
    ladders = []
    for g in ["initial", "final"]:
        for m in "qr":
            ladders.append((("a", g, m), cc["palace"][g][f"p1_f_{m}_ghz"] + cc["palace"][g][f"p2_f_{m}_ghz"],
                            cc["convergence"][g][f"f_{m}_band_ghz"]))
    for k in ["cv1", "cv2"]:
        lad = opt["conv"][k]["ladder"]
        for m in "qr":
            ladders.append((("a", k, m), lad[f"p1_f_{m}_ghz"] + lad[f"p2_f_{m}_ghz"], lad[f"f_{m}_band_ghz"]))
    sc = ctl["conv_step"]["scaling"]
    for m in "qr":
        ladders.append((("a", "scaling", m), sc[f"p1_f_{m}_ghz"] + sc[f"p2_f_{m}_ghz"], sc[f"f_{m}_band_ghz"]))
    for g in ["initial", "cv2", "final"]:
        fam = ctl["mesh_family"][g]
        for m in "qr":
            ladders.append((("b", g, m), fam[f"family_b_levels_f_{m}_ghz"], fam[f"family_b_f_{m}_band_ghz"]))
    assert len(ladders) == 16
    for key, levels, band in ladders:
        v = band_values(levels)
        lo_k, hi_k = min(v, key=v.get), max(v, key=v.get)
        assert hi_k == "p1_observed", key
        assert lo_k == ("p2_lambda" if key[0] == "b" and key[2] == "r" else "p1_lambda"), (key, lo_k)
        assert v[lo_k] < v["p2_borrowed"] < v[hi_k], key
        assert abs(v[lo_k] - band[0]) < 1e-4 and abs(v[hi_k] - band[1]) < 1e-4, (key, v, band)
    # Fable S3: the capacitor's band beside 19-20 % in the abstract and the conclusion.
    ab = NORM[NORM.index("\\begin{abstract}"):NORM.index("\\end{abstract}")]
    concl = section("\\section{Conclusion}", "\\section{Reproducibility}")
    # v19 (lens NIT 20): the 17-39 % band leaves the abstract and the conclusion; it stays in Table 6 (Estimated row).
    assert "$19$--$20\\%$ longer than published." in ab and "17$--$39" not in ab
    assert "about $19$--$20\\%$ longer than published. Against" in concl and "17$--$39" not in concl
    contains("the re-optimized capacitor, $+19$ to $+20\\%$ over two families and about $+17$ to $+39\\%$ within the band")
    # Queued Judge item: the whiteroom L2 is the fusion rotation (L1 unfolded into base algebraic primitives).
    absent("fused primitives")
    phrase("unfold those into compositions of base algebraic primitives")
    # Opus N-7: code and data availability after the conclusion. v23 (flow P4): the contributions section, which no
    # narrative section depends on, follows Reproducibility as the last section.
    order = [m.group(1) for m in re.finditer(r"\\section\{([^}]*)\}", BODY)]
    assert order[-4:] == ["Discussion", "Conclusion", "Reproducibility",
                          "Author Contributions and How geode-fem Was Built"], order
    assert BODY.index("\\section{Reproducibility}") < BODY.index("\\paragraph{Acknowledgments.}")
    # Opus N-9: the first contribution states its result.
    phrase("\\textbf{A refinement study of a published design example}, which puts its qubit frequencies "
           "$7.3$--$9.6\\%$ above their published values")
    # Fable M1: the static model's box is stated where the bound is defined, and the eigenproblem's where it is set up.
    assert NORM.count("zero normal flux on the vacuum box") == 1
    # Opus MF-2: Figure 2 labels the family-a estimate in its own slot column, clear of the ladder series.
    fig2 = (HERE / "figures/src/fig2_crosscheck.py").read_text()
    assert "(XA, .1), xycoords=('data', 'axes fraction')" in fig2 and "xytext=(-22, 0)" not in fig2
    phrase("(a, labelled)")
    print("v16 guards: band ends on 16 ladders, abstract/conclusion band, L2 wording, section order, box model, Fig. 2")


def check_v17():
    """Guards added for the v17 fixes (Opus, Fable and Codex reviews of v16) and the two new results."""
    cc, ctl = read(CROSS), read(CTL)
    gb = read(DIFFOPT + "c_sigma_bracket_grounded_box.toml")
    tens = read(DIFFOPT + "c_sigma_bracket_tensor.toml")
    # T6: the grounded-box upper bound. Natural-box values are replayed on the same meshes; every grounded value is
    # at least the natural one; the family upper bound is the smallest grounded value, ceiled.
    nat = {r["mesh"]: r["upper_ff"] for r in tens["uniform_x0_tensor_p2"]}
    rows = gb["uniform_x0_grounded_box"] + gb["adaptive_x0_grounded_box"]
    for r in rows:
        assert r["grounded_upper_ff"] >= r["natural_upper_ff"], r["mesh"]
        if r["mesh"] in nat:
            assert r["natural_upper_ff"] == nat[r["mesh"]]
    g_up = ceil_to(min(r["grounded_upper_ff"] for r in rows), 2)
    br = gb["bracket_x0_tensor_grounded_box"]
    assert (br["lower_ff"], br["upper_ff"], g_up) == (79.56, 79.77, 79.77)
    assert br["lower_ff"] == tens["bracket_x0_tensor"]["lower_ff"]
    bd = gb["box_difference"]
    assert bd["converged_difference_bounded_by_ff"] == [0.0, 0.21] and ceil_to(g_up - br["lower_ff"], 2) == 0.21
    last = [r for r in rows if r["mesh"] == bd["finest_mesh"]][0]
    d = last["grounded_upper_ff"] - last["natural_upper_ff"]
    assert f"{d:.4f}" == "0.0065" and abs(d - bd["finest_mesh_difference_ff"]) < 1e-6
    assert f"{bd['finest_mesh_difference_relative']:.0e}" == "8e-05"
    contains("With the box grounded, the tensor $\\Cisl$ is bounded by $[79.56, 79.77]$\\,fF. On the finest mesh, "
             "grounding the box raises the conforming value by $0.0065$\\,fF, $8\\times10^{-5}$ relative, an "
             "estimate of the effect of the box model, whose size is bounded only by $[0, 0.21]$\\,fF. The admissible "
             "sets are nested, so the grounded bracket also covers a floating box.")
    pin("$[79.56, 79.77]$\\,fF", 3)   # contribution 2, Section 6, Table 5
    pin("$[85.06, 85.27]$", 3)          # Section 6, Table 5, Table 6
    contains("in $[79.56, 79.77]$\\,fF with the box walls held at ground, as in the eigenproblem")   # v19: contribution 2
    contains("the box-model effect on $\\Cisl$, $8\\times10^{-5}$")
    sr = gb["static_route"]
    assert sr["natural_dressed_f_q_band_ghz"] == cc["static_cross_check"]["initial"]["dressed_f_q_band_ghz"]
    lo, hi = sr["grounded_dressed_f_q_band_ghz"]
    assert (f"{floor_to(lo, 3):.3f}", f"{ceil_to(hi, 3):.3f}") == ("4.449", "4.458")
    assert sr["dressed_low_end_shift_mhz"] == -0.3
    contains("Reading the grounded-box tensor bound through the two-node circuit")
    # v18 (Fable S3): 0.26 MHz unrounded, "about 0.3"; the printed 4-decimal bands differ by 0.2.
    sh = (sr["natural_dressed_f_q_band_ghz"][0] - sr["grounded_dressed_f_q_band_ghz"][0]) * 1e3
    assert 0.15 < sh < 0.35, sh
    contains("the natural-box bound raises its low end by about $0.3$\\,MHz")
    contains("All values are for the natural box; the grounded box changes only the tensor upper bound, to "
             "$79.77$\\,fF.")      # Figure 4 caption (Codex)
    # v18 (Opus N-1, Fable S2): Figure 2 draws the natural-box band (v16 bytes); the caption says so.
    assert "dressed_f_q_band_ghz" in (HERE / "figures/src/fig2_crosscheck.py").read_text()
    contains("(natural box; the grounded bound lowers its low end by about $0.3$\\,MHz) and the circuit-model cap")
    absent("(grounded box) and the circuit-model cap")
    contains("(for the exact field $D = \\varepsilon\\nabla\\phi$, the negative of the physical displacement, they "
             "are the conductor charges)")                # Codex: sign convention
    # T7: adaptive refinement on the published final design.
    af, ai = ctl["amr"]["final"], ctl["amr"]["initial"]
    assert all(y > x for x, y in zip(af["f_q_ghz"], af["f_q_ghz"][1:]))
    assert af["nd"][0] == ctl["mesh_family"]["final"]["family_b_nd"][3]          # the order-2 h cell
    assert af["finest_f_q_ghz"] == af["f_q_ghz"][-1] and f"{af['finest_f_q_ghz']:.4f}" == "3.2468"
    assert f"{af['finest_over_published_pct'][0]:.2f}" == "8.05"
    assert af["nd"][-1] > 1e7 and ai["nd"][-1] > 1e7 and "MaxSize 1e7" in af["description"]
    assert f"{af['step_change_f_q_pct'][-1]:.2f}" == "0.63" and f"{ai['last_change_f_q_pct']:.2f}" == "0.55"
    m1, m2 = af["finest_minus_family_estimates_f_q_pct"]
    assert (f"{-m1:.2f}", f"{-m2:.2f}") == ("0.56", "1.39") and af["finest_in_union_band"][0]
    p2h2 = cc["palace"]["final"]["p2_f_q_ghz"][1]
    assert af["f_q_ghz"][2] > p2h2 > af["f_q_ghz"][1] and f"{p2h2:.4f}" == "3.1464"
    assert (f"{af['dof_extrapolation_f_q_ghz']:.4f}", f"{af['dof_extrapolation_alpha_q']:.2f}") == ("3.2672", "0.83")
    assert not af["dof_extrapolation_r_reliable"] and "24 ranks" in af_wall_note(ctl)
    contains(f"from ${af['nd'][0] / 1e6:.2f}$ to ${af['nd'][-1] / 1e6:.1f}$ million unknowns, the qubit frequency rises from "
             f"${af['f_q_ghz'][0]:.4f}$ to ${af['finest_f_q_ghz']:.4f}$\\,GHz, $8.0\\%$ above the published "
             "$3.005$\\,GHz")
    contains(f"the third pass again exceeds the highest uniform level (${af['f_q_ghz'][2]:.4f}$ against "
             f"${p2h2:.4f}$\\,GHz)")
    contains("The finest final pass lies $0.56\\%$ and $1.39\\%$ below the two family estimates, again inside "
             "family a's band and below family b's.")
    assert af["finest_f_q_ghz"] < ctl["mesh_family"]["final"]["family_b_f_q_band_ghz"][0]
    contains("Both finest passes are computed values, not extrapolations, and neither run reached a plateau: each "
             "stopped when its last pass exceeded the run's size limit of $10^7$ unknowns, not on its tolerance, and "
             "the last pass still moved the qubit by $+0.55\\%$ (initial) and $+0.63\\%$ (final).")
    contains("gives $4.4552$\\,GHz ($\\alpha = 0.80$) and $3.2672$\\,GHz ($\\alpha = 0.83$)")
    absent("the final-design adaptive run on 24 ranks")   # v19: the hosts sentence is shortened (rank counts stay in the artifacts)
    contains("Palace ran on AWS Graviton4 hosts, and the capacitance brackets on a 28-core workstation and on Graviton4.")
    contains("reads the qubit frequencies low: adaptive runs put them $7.0\\%$ and $8.0\\%$ above their published "
             "values with no extrapolation")   # conclusion
    # The film-thickness comparison (Opus SF-2), from DeviceLayout's initial mesh h.
    sys.path.insert(0, str(HERE))
    from sheet_edge_lengths import outline_edge_lengths
    mesh = "reference/fixtures/transmon_devicelayout/initial_o1.msh"
    read_text(mesh)
    L = outline_edge_lengths(REPO / mesh)
    p10, p90 = L[len(L) // 10], L[9 * len(L) // 10]
    assert round(p10) == 10 and round(p90) == 12, (p10, p90)
    assert p10 / 4 / 0.2 > 10
    rep = read_text("reference/fixtures/transmon_devicelayout/initial_report.txt")
    assert "vacuum             68903" in rep and "substrate          64452" in rep    # 133,355 tets: the ladder's h
    contains("The element edges along the sheet outlines are mostly $10$--$12\\,\\mu$m long on $h$ and so "
             "$2.5$--$3\\,\\mu$m on $h/4$, more than ten times a $0.2\\,\\mu$m film")
    # The companion's ratios, from the committed tuned-baseline summary (#1150): the fastest gradient-free run that
    # reached each level against the adjoint at the same n; runs censored before -9.5 dB bound it only from below.
    ts = read("benchmarks/patch_antenna_conformal/tuned/tuned_summary.toml")
    lv = ts["meta"]["levels_db"]
    i9, i95 = lv.index(-9), lv.index(-9.5)
    sc = {r["n_dofs"]: r for r in ts["scaling"]}
    for n, r in sc.items():   # best_ratio recomputed from the runs themselves
        runs = [x for x in ts["run"] if x["n_dofs"] == n and x["kind"] == "gradient_free"]
        adj = [x for x in ts["run"] if x["n_dofs"] == n and x["kind"] == "adjoint"][0]["solves_to_level"]
        for i in (i9, i95):
            best = min(x["solves_to_level"][i] for x in runs if x["solves_to_level"][i] > 0)
            assert best == r["best_gradient_free_solves_to_level"][i] and abs(best / adj[i] - r["best_ratio"][i]) < 1e-4
    assert f"{sc[38]['best_ratio'][i95]:.0f}" == "11" and f"{sc[38]['best_ratio'][i9]:.1f}" == "7.7"
    assert "fcma" in sc[38]["best_gradient_free_run"][i9]
    assert (f"{sc[4]['best_ratio'][i9]:.1f}", f"{sc[4]['best_ratio'][i95]:.1f}") == ("1.1", "2.4")
    adj38 = sc[38]["adjoint_solves_to_level"][i95]
    early = [x["optimization_solves"] / adj38 for x in ts["run"] if x["n_dofs"] == 38 and x["kind"] == "gradient_free"
             and x["solves_to_level"][i95] < 0 and 0 < x["optimization_solves"] < 200]
    assert early and (f"{min(early):.2f}", f"{max(early):.1f}") == ("2.29", "3.0"), early
    # v18 Judge nit: a lower bound is floored (min 2.29 -> 2.2).
    assert (floor_to(min(early), 1), round(max(early))) == (2.2, 3), early
    contains("runs that exhausted their budget before reaching the $-9.5$\\,dB target bound the ratio from below at "
             "$2.2$--$3$")
    contains("(artifact: \\texttt{benchmarks/patch\\_antenna\\_conformal/tuned/tuned\\_summary.toml})")
    assert "2.400" in read_text("benchmarks/patch_antenna_conformal/tuned/README.md") and \
        "2.4835" in read_text("benchmarks/patch_antenna_conformal/tuned/README.md")
    contains("measures this on the worst-of-band reflection $|S_{11}|$ over $2.400$--$2.4835$\\,GHz")
    # Voice: no version history, and the companion is not called unfinished.
    absent("still being run")
    absent("does not yet show")
    print("v17 guards: grounded-box bracket, box-model estimate, final-design adaptive run, design table, edges")


def check_v18():
    """Guards added for the v18 fixes (Opus and Fable reviews of v17)."""
    ctl = read(CTL)
    # Opus M2: the film comparison is scoped to the uniform meshes; the adaptive runs refined at most four times.
    for g in ["initial", "final"]:
        assert ctl["amr"][g]["iterations"] == 5   # five solves = the h mesh plus four refinements
    # v18 Judge M1: from the mesh, not from the median: the shortest outline edges are the junction leads.
    sys.path.insert(0, str(HERE))
    from sheet_edge_lengths import outline_edge_lengths
    for mesh in ["reference/fixtures/transmon_devicelayout/initial_o1.msh",
                 "reference/fixtures/transmon_devicelayout/final_o1.msh"]:
        L = outline_edge_lengths(REPO / mesh)
        assert abs(L[0] - 1.0) < 1e-6, (mesh, L[0])
        lead = [x for x in L if x < 1.9]                       # 1.000 (twice), 1.833, 1.877: the junction leads
        assert len(lead) >= 3 and max(lead) < 1.9
        assert f"{L[0] / 16:.2f}" == "0.06" and f"{lead[2] / 16:.2f}" == "0.11", (lead[:4])
        assert L[0] / 16 < 0.2 and L[len(L) // 10] / 16 > 0.6   # leads below the film; a typical edge above 0.6 um
    contains("Palace's adaptive runs refine at most four times: a typical outline edge stays above about "
             "$0.6\\,\\mu$m, but the $1.0$--$1.8\\,\\mu$m edges of the junction leads, where the qubit's error "
             "indicator is largest, could reach $0.06$--$0.11\\,\\mu$m, comparable to or below the film (an estimate: "
             "no committed record holds the adaptive meshes' minimum edge). There the zero-thickness model, which every "
             "mesh discretizes, is an approximation whose effect is not quantified")
    absent("still above the film")
    absent("therefore see the zero-thickness edge")
    # v18 Judge S3: the two-node values come from the paper's own script, in Section 3.3's circuit.
    import importlib.util
    spec = importlib.util.spec_from_file_location("two_node_reading", HERE / "two_node_reading.py")
    tnr = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(tnr)
    fresh = tnr.render(tnr.compute())
    assert (HERE / "two_node_reading.toml").read_text() == fresh, "two_node_reading.toml is stale"
    t = tomllib.loads(fresh)
    assert [f"{x:.2f}" for x in t["published_c_sigma_ff"]] == ["98.51", "98.64"]
    assert [f"{x:.4f}" for x in t["static_route_grounded_ghz"]] == \
        [f"{x:.4f}" for x in read(DIFFOPT + "c_sigma_bracket_grounded_box.toml")["static_route"]["grounded_dressed_f_q_band_ghz"]]
    contains("\\texttt{two\\_node\\_reading.toml} in the paper's directory (the "
             "\\texttt{transmon\\_loop\\_inductance} anchor uses another topology)")
    # v18 Judge S2: the common-limit fit's levels are named, so "exactly determined" can be checked (4 levels, 4 unknowns).
    contains("The last check fits the $p = 1$ ($h/2$, $h/4$) and $p = 2$ ($h$, $h/2$) series of each ladder with one "
             "common limit and rate, exactly determined")
    absent("every mesh used here sees the zero-thickness edge")
    # Opus M1 statement in Section 3.3.
    contains("The same circuit is used both ways: forward from a bound on $C_\\Sigma$ to a dressed qubit, and backward "
             "from a dressed pair to $C_\\Sigma$.")
    print("v18 guards: one circuit, film scope, Table 2 cap, adaptive bands, companion artifact, Figure 2 caption")


def check_v19():
    """Guards for v19, the reader-clarity revision (pre-read lens of v18, MUST 1-7, SHOULD 8-17) and the coauthor
    ruling of 2026-10-22."""
    # Terminology (MUST 1): element order is p, the convergence rate is r, the adaptive run has passes; "order" never
    # names a rate or a mesh level.
    for stale in [r"\border[- ][12]\b", r"\bobserved orders?\b", r"\bsecond order on\b", r"\basymptotic order\b",
                  r"\bfidelity ratio\b", r"\bpalace evaluations?\b", r"\badaptive levels?\b", r"\bfinest levels?\b",
                  r"\bthird solve\b", r"\bhigh-fidelity\b"]:
        assert not re.search(stale, LOWBODY), stale
    contains("Element order $p$ is the polynomial degree of the finite elements ($p = 1$: linear, $p = 2$: quadratic); "
             "\\emph{straight-sided} and \\emph{curved} describe the geometry of the mesh")
    contains("We write the discretization error as $\\propto h^{r}$ and call $r$ the \\emph{convergence rate}.")
    # One meaning each for bound, cap and band (MUST 7); "band" is never used for the L_s and k ranges.
    # v23 (flow P3): each word is mapped to its Table 6 tier, and the estimate is defined beside its band.
    contains("A \\emph{bound} holds by theorem and is given only for the island capacitance (the \\emph{Bounded} "
             "tier); a \\emph{cap} is a frequency limit that follows from a bound only under the circuit model "
             "(\\emph{Model-dependent}); an \\emph{estimate} is an extrapolation, quoted with its \\emph{band}, the "
             "range of the extrapolations, which is neither a bound nor a cap (\\emph{Estimated}); and a "
             "\\emph{computed} value is a solve on a stated discretization, with no extrapolation (\\emph{Computed}).")
    for stale in ["$L_s$ and $k$ bands", "bands of $0.113", "$L_s$ and $k$ band"]:
        absent(stale)
    # The cost unit (SHOULD 12): a Palace run, a step and a probe, defined once at the head of Section 7.
    contains("A Newton \\emph{step} places a new design and costs one Palace run; a \\emph{probe} is a Palace run that "
             "is not a step. geode-fem solves are not Palace runs, and their cost is given in seconds.")
    # Figure 1 (MUST 4, MUST 6, SHOULD 8): c and L from the run record the strip is drawn from.
    run = read(OPT + "runs/fd_initial.toml")["run"]
    assert (run["capacitor_length_um"], run["total_length_um"]) == (620, 5000)
    y_min, y0, y1, yt, y_max = run["velocity_bands_um"]
    assert y1 - y0 == run["capacitor_length_um"] - 132 and yt == run["capacitor_length_um"] + 1700
    contains("the published initial design has $c = 620\\,\\mu$m and $L = 5000\\,\\mu$m")
    contains("When $c$ changes, the shaded strip stretches and everything above it moves up by $\\Delta c$")
    contains("(d) The two-node circuit of Section~\\ref{sec:junction}")
    # The mesh inventory (MUST 5): Table tab:meshes, each row against its artifact.
    tab = NORM[NORM.index("\\label{tab:meshes}"):NORM.index("\\subsection{Junction and circuit models}")]
    runs = {g: tomllib.loads(read_text(f"benchmarks/transmon_devicelayout_crosscheck/geode_runs/{g}.toml"))["run"]
            for g in ["initial_h", "initial_h2", "final_h", "final_h2"]}
    for g in ["initial", "final"]:
        assert runs[g + "_h2"]["n_tets"] == 8 * runs[g + "_h"]["n_tets"], g
    assert "gmsh splits each tetrahedron of $h$ into 8" in tab
    ctl = read(CTL)
    assert sorted(ctl["mesh_family"]) == ["cv2", "final", "initial"]
    assert "Family b & initial, final, step 2 & Palace's uniform refinement of $h$" in tab
    assert ctl["amr"]["initial"]["iterations"] == ctl["amr"]["final"]["iterations"] == 5
    assert "Adaptive & initial, final & Palace adaptive refinement from $h$ at $p = 2$ & passes 1--5" in tab
    for row in ["Fixture & initial", "DeviceLayout $h$ & every design", "Family a & every design",
                "DeviceLayout's discretization & every design & DeviceLayout, curved; Palace at $p = 2$"]:
        assert row in tab, row
    # No derivative credit in the abstract, the introduction or the conclusion (MUST 2).
    ab = NORM[NORM.index("\\begin{abstract}"):NORM.index("\\end{abstract}")]
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    concl = section("\\section{Conclusion}", "\\section{Reproducibility}")
    for part in [ab, intro, concl]:
        assert "gives no advantage" in part
        for credit in ["with geode-fem input", "geode-fem-calibrated", "with geode-fem's shape derivative or",
                       "in 4 without"]:
            assert credit not in part, credit
    contains("geode-fem's shape derivative is exact but gives no advantage on this two-parameter problem; its one-time "
             "calibration of the resonator model replaces one Palace probe run")
    # One denominator for the published-vs-estimate gap (SHOULD 11): every percentage is over the published value.
    assert not re.search(r"\\%\$ low on the", NORM)
    # Plain-language entry points (SHOULD 9, MUST 6, SHOULD 8): the LC picture, the natural and grounded box, bare and dressed.
    contains("A transmon qubit is an electrical oscillator")
    contains("this is the \\emph{natural} box, in which no field line leaves the box")
    contains("the \\emph{grounded} box, with the walls held at ground potential")
    print("v19 guards: p and r, bound/cap/band, Palace run, Figure 1 bands, mesh table, no derivative credit, one "
          "denominator, coauthor")


def check_v20():
    """Guards for v20, the coauthor clarity revision of v19: no number changed, no new computation."""
    # Item 2: "a qubit above" names the frequency; the qubit frequency is defined up front.
    for stale in ["put the qubits", "reaches qubits", "extrapolate to qubits", "puts its qubits", "adaptive qubits",
                  "reads the qubits", "two qubits $7", "published qubit values"]:
        absent(stale)
    para1 = NORM[NORM.index("A transmon qubit is an electrical oscillator"):NORM.index("DeviceLayout.jl \\citep{devicelayout} builds")]
    assert ("Its frequency, which we call the \\emph{qubit frequency}, is the linear eigenfrequency of the junction's "
            "(plasma) mode.") in para1
    pin("\\footnote{Not the transmon's 0--1 transition $f_{01}$, which lies below the plasma frequency by about the "
        "charging energy $E_C/h$ (Section~\\ref{sec:junction}).}", 1)
    assert NORM.count("$f_{01}$") == 2 and "$f_{01} \\approx" in NORM   # the footnote, Section 9; Section 3.3
    # Item 4: where the shift comes from, one paragraph in the Discussion, pointed to from Section 1.
    disc = section("\\section{Discussion}", "\\section{Conclusion}")
    where = disc[disc.index("\\paragraph{Where the shift comes from.}"):
                 disc.index("\\paragraph{Why the qubit is approached from below.}")]   # v23: the heuristic follows it
    for text in ["The evidence places the $7$--$8\\%$ shift in the discretization, not in arithmetic precision or in "
                 "the eigensolver.",
                 "geode-fem computes in double precision (f64), and Palace is the build pinned in "
                 "\\path{reference/palace/docker/Dockerfile}, which sets no single-precision option.",
                 # v23 (#1231 Judge nit 1, flow P13): the argument is scoped to the levels both codes ran.
                 "On the levels both codes ran, the two coarsest family-a levels with linear elements, the two codes "
                 "agree on the qubit frequency to $2\\times10^{-4}\\%$, the looser of the ladder and re-optimization values "
                 "(Sections~\\ref{sec:crosscheck} and~\\ref{sec:reopt}); on those levels, a loss of precision large "
                 "enough to matter in either code would appear as a same-mesh disagreement far above that level.",
                 # v23 (flow P12): the frequency-domain statement is motivated, not a bare denial.
                 "frequency-domain finite-element eigensolver, so there is no time stepping or numerical dispersion to "
                 "suspect",
                 "refining the mesh from $h$ to $h/2$ moves both codes' frequencies up together"]:
        assert text in where, text
    for stale in ["certif", "proven", "proof", "guarantee"]:
        assert stale not in where.lower(), stale
    # The evidence behind it: every geode-fem record of the cross-check is f64, Palace is built from the Dockerfile
    # at the committed changeset, and the Dockerfile selects no single precision.
    cc = read(CROSS)
    assert cc["meta"]["float_dtype"] == "F64" and "geode ndarray f64" in cc["meta"]["compute_host"]
    assert "built from reference/palace/docker/Dockerfile" in cc["meta"]["compute_host"]
    for g in ["initial_h", "initial_h2", "final_h", "final_h2"]:
        meta = tomllib.loads(read_text(f"benchmarks/transmon_devicelayout_crosscheck/geode_runs/{g}.toml"))["meta"]
        assert meta["float_dtype"] == "F64", g
    docker = read_text("reference/palace/docker/Dockerfile")
    assert "ARG PALACE_REF=fba6a5b" in docker and "fba6a5b" in cc["meta"]["compute_host"]
    assert not re.search(r"single|precision|float32|MFEM_USE_SINGLE", docker, re.I)
    for k in ["initial", "final"]:
        assert max(cc["same_mesh"][k]["geode_over_palace_f_q_pct"]) <= 2e-4
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    assert ("Section~\\ref{sec:discussion} explains why the same-mesh agreement places the shift in the mesh, not in "
            "either solver or its arithmetic.") in intro   # v23 (flow P1): the referent named
    # Item 1: the affiliation (pinned in AUTHOR).
    assert "{\\normalsize ZSpaceLabs.ai}" in TEX[TEX.index("\\author{"):TEX.index("\\date{")]   # v21: ZSpaceLabs.ai alone
    # Item 3: Figure 1's caption states the colour rule.
    # v23 (#1231 Judge nit 2): the rule exempts the labels drawn in black or grey.
    contains("Each label has the colour of the object it names, except the $k$, Vacuum, Sapphire and Metal-sheets "
             "labels; the design parameters are annotated in green.")
    absent("Each label has the colour of the object it names;")
    contains("the junction (purple, as in (b) and as $L_J$ in (d)) joins the island to ground")
    print("v20 guards: affiliation, the qubit frequency named and defined, where the shift comes from, Figure 1 colours")


def check_v21():
    """v21, the v1 Zenodo release of v20: each author's affiliation and the paper's DOI; nothing else changed."""
    assert TEX[TEX.index("\\date{"):TEX.index("\\begin{document}")] == DATE
    assert "PENDING" not in TEX and "Independent Researcher" not in TEX and TEX.count("zenodo.23580545") == 4
    assert "TODO" not in TEX and "Archival DOI to be added" not in TEX
    v20 = (HERE.parent / "transmon-benchmark.20" / "main.tex").read_text()
    v21 = (HERE.parent / "transmon-benchmark.21" / "main.tex").read_text()   # v22: the v20 -> v21 diff is v21's own
    # The Reproducibility footnote: the deposit placeholder and its TODO comment become the paper's DOI.
    old_fn = ("\\url{https://github.com/rjwalters/geode-fem}. Archival DOI to be added at\ndeposit.}\n"
              "% TODO(operator): Zenodo DOI\n")
    new_fn = ("\\url{https://github.com/rjwalters/geode-fem}. This paper's PDF and \\LaTeX{}\nsource are archived at\n"
              "\\href{https://doi.org/10.5281/zenodo.23580545}{doi:10.5281/zenodo.23580545}.}\n")
    assert v20.count(old_fn) == 1 and TEX.count(new_fn) == 1 and v21.count(new_fn) == 1
    old_block = v20[v20.index("\\author{"):v20.index("\\begin{document}")]
    new_block = v21[v21.index("\\author{"):v21.index("\\begin{document}")]
    old_head = v20[:v20.index("\\documentclass")]
    new_head = v21[:v21.index("\\documentclass")]
    assert old_head.startswith("%% transmon-benchmark.20 ") and new_head.startswith("%% transmon-benchmark.21 ")
    assert new_head.count("\n") == 5 and all(line.startswith("%%") for line in new_head.splitlines()[:4])
    assert v21.replace(new_block, old_block).replace(new_head, old_head, 1).replace(new_fn, old_fn) == v20
    print("v21 guards: each author's affiliation, the paper's DOI (title page and Reproducibility footnote), "
          "no PENDING marker or TODO, v20 otherwise unchanged")


def check_v22():
    """v22: the operator's short title; v21 main.tex otherwise unchanged apart from the header comment."""
    v21 = (HERE.parent / "transmon-benchmark.21" / "main.tex").read_text()
    assert hashlib.sha256(v21.encode()).hexdigest() == V21_SHA, "v21 main.tex changed"
    v22 = (HERE.parent / "transmon-benchmark.22" / "main.tex").read_text()   # v23: the v21 -> v22 diff is v22's own
    assert v21.count(TITLE_V21_SRC) == 1 and v22.count(TITLE_V22_SRC) == 1 and v22.count("\\title{") == 1
    assert TEX.count(TITLE_V22_SRC) == 1 and TEX.count("\\title{") == 1
    for stale in ["Uniform mesh refinement underestimates", "re-optimization}", "transmon: an independent"]:
        assert stale not in TEX and stale not in v22, stale
    old_head = v21[:v21.index("\\documentclass")]
    new_head = v22[:v22.index("\\documentclass")]
    assert old_head.startswith("%% transmon-benchmark.21 ") and new_head.startswith("%% transmon-benchmark.22 ")
    assert new_head.count("\n") == 5 and all(line.startswith("%%") for line in new_head.splitlines()[:4])
    assert v22.replace(new_head, old_head, 1).replace(TITLE_V22_SRC, TITLE_V21_SRC) == v21
    print("v22 guards: the short title, v21 otherwise unchanged")


def check_v23():
    """v23, the reading-flow revision of v22 (flow review of 2026-10-23; #1231 Judge nits). The text was reordered, so
    there is no whole-file diff; instead: the multiset of numerals in the document body equals v22's exactly (the
    preamble's too), the title, author block, date-and-DOI block, citation set and refs.bib are v22's, the sections,
    subsections and moved blocks are in their new order, each applied wording appears once, and v22 is unchanged."""
    from collections import Counter
    v22 = (HERE.parent / "transmon-benchmark.22" / "main.tex").read_text()
    assert hashlib.sha256(v22.encode()).hexdigest() == V22_SHA, "v22 main.tex changed"
    # Hard rule: no number changed. Every numeral token of the body, as a multiset, is v22's; nothing added or dropped.
    num = re.compile(r"-?\d+(?:\{,\}\d+)*(?:\.\d+)?")
    body22 = "\n".join(line for line in v22[v22.index("\\begin{document}"):].splitlines()
                       if not line.lstrip().startswith("%"))
    old, new = Counter(num.findall(body22)), Counter(num.findall(BODY))
    assert old == new, {"only in v22": dict(old - new), "only in v23": dict(new - old)}
    assert sum(new.values()) == 1005, sum(new.values())
    pre = lambda text: Counter(num.findall(text[text.index("\\documentclass"):text.index("\\begin{document}")]))
    assert pre(v22) == pre(TEX)
    # Title, authors, date and DOI are v22's; no reference added or removed; refs.bib is v22's bytes.
    assert TEX.count(TITLE_V22_SRC) == 1 and TEX.count("\\title{") == 1
    assert TEX[TEX.index("\\author{"):TEX.index("\\date{")] == AUTHOR
    assert TEX[TEX.index("\\date{"):TEX.index("\\begin{document}")] == DATE
    keys = lambda text: sorted({k.strip() for m in re.finditer(r"\\cite\w*\{([^}]*)\}", text) for k in m.group(1).split(",")})
    assert keys(TEX) == keys(v22) and len(keys(TEX)) == len(keys(v22))
    assert (HERE / "refs.bib").read_bytes() == (HERE.parent / "transmon-benchmark.22" / "refs.bib").read_bytes()
    assert Counter(re.findall(r"\\cite\w*\{", TEX)) == Counter(re.findall(r"\\cite\w*\{", v22))
    # Header comment: four %% lines naming v23.
    head = TEX[:TEX.index("\\documentclass")]
    assert head.startswith("%% transmon-benchmark.23 ") and head.count("\n") == 5
    assert all(line.startswith("%%") for line in head.splitlines()[:4])
    # Order (P1, P4, P5, P14, P19): sections, subsections and the moved blocks.
    order = [m.group(1) for m in re.finditer(r"\\section\{([^}]*)\}", BODY)]
    assert order == ["Introduction", "Related Work", "Device, Models and Solvers", "Same-Input-Mesh Agreement With Palace",
                     "The DeviceLayout Cross-Check", "A Static Route: A Bounded Island Capacitance",
                     "Re-Optimization and Its Controls", "Discussion", "Conclusion", "Reproducibility",
                     "Author Contributions and How geode-fem Was Built"], order
    subs = [m.group(1) for m in re.finditer(r"\\subsection\{([^}]*)\}", BODY)]
    assert subs == ["Device, meshes and materials", "Junction and circuit models", "geode-fem"], subs
    assert BODY.index("\\label{eq:port}") < BODY.index("\\label{eq:hf-eig}")   # P14: Eq. port precedes its use
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    assert intro.index("Two solvers appear. Palace is the one the example uses; the second is GEODE-FEM") \
        < intro.index("Our main result is that the published frequencies are discretization-limited") \
        < intro.index("Five words sort the evidence") < intro.index("Our contributions are:")
    assert "The second code is" not in BODY
    assert BODY.index("\\paragraph{A second refinement family.}") < BODY.index("\\anvilfig{fig3-initial-qubit.pdf}") \
        < BODY.index("\\paragraph{Adaptive refinement.}")
    assert "\\begin{figure}[t]\n  \\centering\n  \\anvilfig{fig3-initial-qubit.pdf}" in TEX
    assert BODY.index("\\paragraph{Where the shift comes from.}") < BODY.index("\\paragraph{Why the qubit is approached from below.}") \
        < BODY.index("\\paragraph{Why the shape derivative gives no advantage here.}")
    assert BODY.count("Why the qubit is approached from below") == 1
    assert BODY.index("\\paragraph{Acknowledgments.}") < BODY.index("\\section{Author Contributions and How geode-fem Was Built}")
    assert "How geode-fem Was Designed and Built" not in TEX
    # The GCI comparison is a footnote on the rule (P11).
    assert "markers).\\footnote{The rule is Richardson extrapolation" in TEX
    # One copy of each applied wording.
    for text, n in [
        ("which we call the \\emph{final} design", 1),                                   # P8
        ("(one Palace eigensolve each, the unit in which this paper counts optimization cost, Section~\\ref{sec:reopt})", 1),  # P6
        ("(the \\emph{static route}, Section~\\ref{sec:static})", 1),                    # P10
        ("the qubit frequencies of the initial and final designs $7.0\\%$ and $8.0\\%$", 1),   # P2 (Section 1)
        ("the initial and final designs' qubit frequencies $7.0\\%$ and $8.0\\%$", 1),  # P2 (abstract)
        ("above their published values, estimates that lean low", 1),                    # P2: the baseline named
        ("a \\emph{ladder} is one geometry's series of uniformly refined levels", 1),   # P9
        ("The \\emph{band} is the minimum and maximum of six extrapolations", 1),        # P11
        ("Three checks test the estimates: a second refinement family and Palace's adaptive refinement "
         "(Figure~\\ref{fig:initialq}; the adaptive runs check only the qubit), and a common-limit fit.", 1),  # P5
        ("The \\emph{unmodified} device is the initial design as published, before the re-optimization of "
         "Section~\\ref{sec:reopt} changes $c$, meshed as the fixture.", 1),             # P8
        ("not in the Bounded tier of Table~\\ref{tab:tiers}", 1),                        # P3
        ("Newton's Jacobian has one column per design parameter, $c$ and $L$; the next two paragraphs give how each "
         "is obtained.", 1),                                                             # P17
        ("On the ladders of both steps, geode-fem and Palace agree on the qubit to $2\\times10^{-4}\\,\\%$.", 1),  # P13
        ("The headline length we quote is not this design's $c$ itself but the two families' qubit estimates mapped "
         "to $3.0$\\,GHz by the scaling law $f_q \\propto c^{-1/2}$: they call for", 1),  # P18
        ("the shape derivative or the scaling law $f_q \\propto x_1$, which costs nothing, in 3 or 4 runs "
         "(Table~\\ref{tab:controls}), and even Broyden's two-probe secant in one or two runs more.", 1),  # P7
        ("Computed: a solve on a stated discretization.", 1),                            # P16
        ("the final design, to reach its $3.0$ and $4.0$\\,GHz targets, needs a capacitor about $19$--$20\\%$ "
         "longer than published.", 1),                                                   # P8 (conclusion)
        ("a design that reaches the final design's targets, $3.0$ and $4.0$\\,GHz, needs $c \\approx "
         "1451$--$1466\\,\\mu$m", 1),                                                     # P8 (contribution)
        ("not a blind prediction", 2),                                                   # P15: Section 1 and Table 6 only
        ("fixed after", 2),                                                              # P15: Section 1 (chosen after) counts separately
    ]:
        pin(text, n)
    assert NORM.count("chosen after that capacitance route") == 1
    # PR #1245 Judge: geode-fem ran only h and h/2 at p = 1 on family a; the precision argument says so, without numerals.
    pin("On the levels both codes ran, the two coarsest family-a levels with linear elements", 1)
    absent("the only ones both codes ran")
    pin("the two coarsest meshes with linear elements", 1)   # Section 1
    pin("reproduces Palace's qubit on every level both codes ran", 1)   # Conclusion
    absent("rather than a blind prediction")
    absent("a consistency check for the reason given")
    # P9: family a / family b only.
    for stale in ["first family", "first-family", "the second family", "the second refinement family", "second-family"]:
        assert stale not in LOWBODY, stale
    # P16: the Computed tier; "measurement" keeps its laboratory sense; "measured" survives only as a verb.
    assert "Measured" not in BODY and "measured value" not in BODY
    assert LOWBODY.count("measured") == 2 and "Agreement with measurement" in BODY
    assert "$3.0$/$4.0$\\,GHz design" not in BODY                                         # P8: one name per design
    for stale in ["FDTD", "Adaptive Palace runs", "the two qubit frequencies", "Three kinds of interval",
                  "The second code", "which is neither.", "spans six values", "As stated in Section",
                  "and the first family", "7.3$--$9.6\\%$ and $2.7$--$3.3\\%$ higher"]:
        assert stale not in BODY, stale
    # Voice: bounded, never certified.
    assert "certified" not in LOWBODY and "certif" not in LOWBODY
    # Abstract: 200-word cap (also in check_voice), no "consistent".
    ab = TEX[TEX.index("\\begin{abstract}") + 16:TEX.index("\\end{abstract}")]
    assert len(ab.split()) <= 200 and "consistent" not in ab
    print("v23 guards: numerals multiset = v22 (body and preamble), title/authors/DOI/citations/refs.bib = v22, "
          "section and block order, each applied wording once, Computed tier, family a/b, nits, v22 unchanged")


def af_wall_note(ctl):
    return [line for line in read_text(CTL).splitlines() if line.startswith("wall_s = 20847.0")][0]


def check_operator_items():
    for v, digest in [("10", V10_SHA), ("11", V11_SHA), ("12", V12_SHA), ("13", V13_SHA), ("14", V14_SHA),
                      ("15", V15_SHA), ("16", V16_SHA), ("17", V17_SHA), ("18", V18_SHA),
                      ("19", V19_SHA), ("20", V20_SHA), ("21", V21_SHA), ("22", V22_SHA)]:
        text = (HERE.parent / f"transmon-benchmark.{v}" / "main.tex").read_text()
        assert hashlib.sha256(text.encode()).hexdigest() == digest, f"v{v} main.tex changed"
    # Operator (2026-10-22, geode-fem#1231): the short title, which supersedes the 2026-10-15 ruling's title; the
    # convergence finding is stated in the abstract.
    # v22: one manual line break (\\) keeps the last word off a line of its own; it is not part of the title.
    title = re.sub(r"\s+", " ", re.search(r"\\title\{(.*?)\}\n", TEX, re.S).group(1).replace("\\\\", " "))
    assert title == TITLE, title
    low_title = title.lower()
    assert "converged frequencies" not in low_title and "differentiable" not in low_title and "gradient" not in low_title
    assert "uniform mesh refinement underestimates" not in low_title
    # Operator ruling (2026-10-22): Crutcher Dunnavant is the second author, plain order, no equal-contribution
    # footnote, no email; v20: his affiliation as he supplied it, in the class's \normalsize line.
    author = TEX[TEX.index("\\author{"):TEX.index("\\date{")]
    assert author == AUTHOR, author
    for stale in ["contributed equally", "equal contribution", "\\thanks", "affiliation line", "@", AUTHOR_NOTE]:
        assert stale not in author and stale not in BODY[:BODY.index("\\begin{abstract}")], stale
    absent(AUTHOR_NOTE)
    assert TEX.count("\\and ") == 1
    # Ruling: the validation wording, once, in Section 1.
    ruled = ("an independent same-mesh cross-check of the Palace/DeviceLayout.jl transmon stack, complementing the "
             "earlier checks of Palace by SQDMetal \\citep{sommers2025open} and \\citet{ye2025electromagnetic}")
    pin(ruled, 1)
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    assert ruled in intro
    for stale in ["first independent", "third-party", "validation of the palace", "has no publication of its own"]:
        assert stale not in NORM.lower(), stale
    # Ruling (2026-10-15): Section 8 short, 150-250 words; credit and AI-model statement kept; no counts.
    # v23 (flow P4): the section is the last one, after Reproducibility and the acknowledgments, retitled.
    built = section("\\section{Author Contributions and How geode-fem Was Built}", "\\bibliographystyle{plainnat}")
    # v19: the section carries the author-contributions statement by initials; nothing beyond the ruled credit.
    assert built.count("C.D.") == 1 and built.count("R.W.") == 1
    for stale in ["human author", "of Crutcher Dunnavant", "He also gave"]:
        assert stale not in built, stale
    words = len(built.replace("\\section{Author Contributions and How geode-fem Was Built} \\label{sec:built}", "").split())
    assert 150 <= words <= 250, words
    for text in BUILT:
        assert re.sub(r"\s+", " ", text) in built, text
    for stale in ["commits", "pull requests from", "co-author trailer", "Counts from repository history",
                  "earlier full draft", "were corrected or dropped", "Three examples follow", "48 of the 50",
                  "36 commits"]:
        assert stale not in built, stale
    assert not (HERE / "count_provenance.py").exists()
    assert "frontier" not in built and "The reviews of this paper" not in built
    assert len(re.findall(r"\\cite\w*\{dunnavant2026whiteroom\}", TEX)) == 1
    assert "\\url{https://github.com/crutcher/palace_whiteroom}" in refs_entry("dunnavant2026whiteroom")
    assert "\\url{https://github.com/rjwalters/loom}" in refs_entry("loom")
    # v23: the contributions section follows the acknowledgments, so the paragraph ends at that section.
    ack = re.sub(r"\s+", " ", BODY[BODY.index("\\paragraph{Acknowledgments.}"):
                                   BODY.index("\\section{Author Contributions and How geode-fem Was Built}")])
    assert ack == ACK, ack
    assert BODY.count("AI-orchestrated") == 1 and "Claude" not in ack and "Codex" not in ack
    phrase("\\footnote{The geode-fem source is public at \\url{https://github.com/rjwalters/geode-fem}. "
           "This paper's PDF and \\LaTeX{} source are archived at "
           "\\href{https://doi.org/10.5281/zenodo.23580545}{doi:10.5281/zenodo.23580545}.}")   # v21
    absent("Archival DOI to be added")
    assert [line for line in TEX.splitlines() if "TODO" in line] == OPEN_TODOS
    assert "TODO" not in BODY
    assert "PENDING" not in TEX   # v20: the affiliation marker is replaced


def check_repro():
    """SF-6 / S7: solver settings, ranks and mesh sizes of Section 9, from the committed configurations and logs."""
    base = "reference/fixtures/transmon_palace/results_crosscheck_1092/"
    cells = {}
    for line in read_text(base + "cells.txt").splitlines():
        name, ranks = line.split()
        cells[name] = int(ranks)
        cfg = json.loads(read_text(base + name + "/palace_config.json"))
        e, lin = cfg["Solver"]["Eigenmode"], cfg["Solver"]["Linear"]
        assert e["Tol"] == 1e-8 and lin["Tol"] == 1e-9, name
        assert e["Target"] == (3.0 if name.startswith("initial") else 2.0), name
        h1 = name.endswith("_h_p1")
        assert lin["Type"] == ("Default" if h1 else "AMS") and (h1 or lin["KSPType"] == "GMRES"), name
        assert int(ranks) == (16 if h1 else 32), name
    for name in ["dl_initial", "dl_final"]:
        e = json.loads(read_text(base + name + "/palace_config.json"))["Solver"]
        assert (e["Eigenmode"]["Tol"], e["Linear"]["Tol"], e["Eigenmode"]["Target"]) == (1e-6, 1e-7, 2.5)
    assert "Running with 8 MPI processes" in read_text("reference/fixtures/transmon_palace/results_p1/palace_run_v22.log")
    contains("Palace's ladder cells use SLEPc shift-invert at tolerance $10^{-8}$, \\texttt{Target} $3.0$ (initial) or "
             "$2.0$\\,GHz (final), and its default direct solver, SuperLU ($p = 1$ on $h$), or AMS-preconditioned GMRES at relative "
             "tolerance $10^{-9}$, on 16 ($p = 1$ on $h$) or 32 MPI ranks; the fixture ran on 8, and the runs at "
             "DeviceLayout's discretization keep the example's tolerances ($10^{-6}$ and $10^{-7}$, \\texttt{Target} "
             "$2.5$\\,GHz).")
    assert "AMS instead of SuperLU" in tomllib.loads(read_text(CROSS))["same_mesh"]["initial"]["note"]   # Fable N4
    runs = {g: tomllib.loads(read_text(f"benchmarks/transmon_devicelayout_crosscheck/geode_runs/{g}.toml"))["run"]
            for g in ["initial_h", "initial_h2", "final_h", "final_h2"]}
    assert {g: r["sigma_ghz"] for g, r in runs.items()} == {"initial_h": 5, "initial_h2": 5, "final_h": 4, "final_h2": 4}
    assert max(r["max_residual_rel"] for r in runs.values()) < 4e-8
    contains("geode-fem's Lanczos shifts are $5$ and $4$\\,GHz, with relative "
             "residuals below $4\\times10^{-8}$.")
    prov = read_text(base + "provenance.txt")
    for n in ["1,066,840", "8,534,720", "1,307,448", "10,459,584"]:
        assert f"({n} tets)" in prov, n
    assert runs["initial_h"]["n_tets"] == 133355 and runs["final_h"]["n_tets"] == 163431
    assert runs["initial_h2"]["n_tets"] == 1066840 and runs["final_h2"]["n_tets"] == 1307448
    contains("The $h$, $h/2$ and $h/4$ meshes have $133{,}355$, $1{,}066{,}840$ and $8{,}534{,}720$ tetrahedra (initial) "
             "and $163{,}431$, $1{,}307{,}448$ and $10{,}459{,}584$ (final).")
    assert "gmsh_jll 4.15.2, Julia 1.10.11" in prov
    assert "mpich-devel" in read_text("reference/palace/docker/Dockerfile")
    contains("The meshes were made with gmsh 4.15.2 (\\texttt{gmsh\\_jll}) under Julia 1.10.11, and Palace was built "
             "against the MPICH of the Rocky Linux 9 image in \\path{reference/palace/docker/Dockerfile}.")
    fix = read_text("crates/geode-core/tests/fixtures/transmon_smoke.provenance.txt")
    assert "reference/julia/generate_transmon_fixture.jl" in fix and "DeviceLayout.jl v1.15.0" in fix
    contains("written by an earlier DeviceLayout.jl v1.15.0 driver, \\path{reference/julia/generate_transmon_fixture.jl}")


def check_voice():
    assert BODY.count("---") == 0, BODY.count("---")   # no em-dash in the body
    for word in ["honest", "genuine", "transparent", "load-bearing", "load bearing", "emphatically", "elegant",
                 "earlier version", "this version", "withdraw", "Versions 1", "the tracker", "the ruling",
                 "deliberately", "supersede", "v8 ", "v9 ", "v12", "Judge", "Doctor"]:
        assert word.lower() not in BODY.lower(), word
    body_no_path = re.sub(r"\\path\{[^}]*\}|\\texttt\{[^}]*\}|results_\w+", "", BODY)
    assert not re.search(r"\\#\d|#\d{3,}", body_no_path), re.findall(r"\\#\d+|#\d{3,}", body_no_path)
    assert "::" not in re.sub(r"\\texttt\{[^}]*\}", "", BODY)
    ab = TEX[TEX.index("\\begin{abstract}") + 16:TEX.index("\\end{abstract}")]
    assert len(ab.split()) <= 200, len(ab.split())    # v12's ~290 words, cut by a third
    phrase("Our main result is that the published frequencies are discretization-limited")
    pin("is consistent with that shift", 2)   # Section 1, conclusion (the abstract gives the cap itself)
    intro = section("\\section{Introduction}", "\\section{Related Work}")
    concl = section("\\section{Conclusion}", "\\paragraph{Acknowledgments.}")
    assert "is consistent with that shift and caps the initial qubit under that model" in intro
    assert "is consistent with that shift and caps the initial qubit under that model" in concl
    for stale in [r"\bgives? the same shift\b", r"\bsame shift\b(?![-\w])", r"\bby a static route\b",
                  r"\bconfirms?\b", r"\bcorroborat", r"static route[^.]*\bconfirmed\b", r"\bconfirmed by the static\b",
                  r"shift[^.]*\bconfirmed\b", r"\bverifies that shift\b", r"\breproduces that shift\b"]:
        assert not re.search(stale, LOWBODY), stale
    # Extrapolations are estimates, never converged values.
    for stale in [r"converged (?:frequenc|value|estimate|design|resolution|target|island|initial|qubit)",
                  r"\bconv\.", r"the converged "]:
        assert not re.search(stale, LOWBODY), stale
    phrase("The band is an extrapolation range, not a bound")
    phrase("The brackets are the only bounds")
    phrase("The range is an estimate, because $L_s$, $k$ and the resonator estimate are.")   # v16, Fable §6 comment
    assert NORM.lower().count("not a bound") <= 2   # the rule paragraph and a figure legend label at most
    assert "\\appendix" not in TEX and "\\clearpage" not in BODY
    # MF-3: the evidence is code-to-code comparison and solution verification, not code verification.
    assert "code verification" not in LOWBODY
    phrase("code-to-code comparison and solution verification \\citep{oberkampf2010verification}, not validation "
           "against measurement")
    phrase("so the agreement above is a code-to-code comparison on one discretization")


V10_SHA = "16a3d01fb4c0480687882923d859abdaedf60adffeeed71f308d6e4d0969dd88"
V11_SHA = "34f7e6ad083cd1ad6ac6c05eb16512c769f9479f1fb47cba30d9ba29e7fd35fb"
V12_SHA = "55e3797ecdf0771a536aa191b85ea49e113d27d3831236713ec49aaec9671e93"
V13_SHA = "bf0f115d6476c577cbd522702720de72403a94d1795c57781c03890df00ee563"
V14_SHA = "c1dbccd7e7e070b46d64c804a3f902b2972b60b86b7b2569943d8d351cb52440"
V15_SHA = "9eccb81d0efff2cff51ab39e8a7b034fb0954d1ec3a9d4db86f331d93a4ed2b8"
V17_SHA = "1ad6a4aae3abc9703a7f145223e5efc40da0c5d40c1e9844a3d3b631cb743ed7"
V16_SHA = "f09233fa91343bc48fff9f3896917017bd69738e08d006c6058da870e329ceb3"
V18_SHA = "ed3c3f94de93996707e99d616b72fe601f890de9e4971e1519ad942e1f937029"
V19_SHA = "2386297fc2e9e7d25ffd032bea2ee2234faf5c5a2ae8e2718e52c57b55b2887c"
V20_SHA = "9e9dcba018b119d21361565381c6a76b45c0910790ac02f49b06bc77b8920eb6"
V21_SHA = "92db23612f990222bef4947e595241733c5b5fee67de227ad498d978f39ad907"
V22_SHA = "b4fd5ad0d923673f3f514143153129129c93db8ce5fed17b3b8678236b4d492c"
TITLE = "An Independent Finite-Element Cross-Check of the DeviceLayout.jl Transmon"   # v22 (operator, #1231)
TITLE_V21_SRC = ("\\title{Uniform mesh refinement underestimates the frequencies of the\n"
                 "DeviceLayout.jl transmon: an independent finite-element cross-check and\n"
                 "re-optimization}\n")
TITLE_V22_SRC = "\\title{An Independent Finite-Element Cross-Check\\\\of the DeviceLayout.jl Transmon}\n"
assert TITLE_V22_SRC.replace("\\\\", " ") == "\\title{" + TITLE + "}\n"
AUTHOR_NOTE = "% NOTE: Crutcher Dunnavant to be offered coauthorship at review (operator, 2026-10-13)"   # v18; gone in v19
AUTHOR = ("\\author{Robb Walters\\\\\n  {\\normalsize 2AM Logic}\n  \\and Crutcher Dunnavant\\\\\n"
          "  {\\normalsize ZSpaceLabs.ai}}\n")   # v21: each author's own affiliation (v20: the coauthor's affiliation)
DATE = ("\\date{October 2026\\\\[0.5ex]\n"
        "  {\\normalsize DOI: \\href{https://doi.org/10.5281/zenodo.23580545}{10.5281/zenodo.23580545}}}\n\n")   # v21
BUILT = [
    "geode-fem realizes the whiteroom operator specification \\citep{dunnavant2026whiteroom}, whose four layers, "
    "L1--L4, restate Palace's cited source as pure functions, unfold those into compositions of base algebraic "
    "primitives, recast them where possible as global tensor operations, and express them in a small "
    "graph-evaluation calculus.",
    # Operator ruling (2026-10-22): an author-contributions statement by initials. C.D.'s code contributions are the
    # ones v18's Section 8 already stated (BRIEF, v13 rulings); nothing else is added.
    "Author contributions: C.D. wrote that specification, gave design input and review on the solver's operator "
    "layer, and contributed code: the migration to Rust 2024, the Metal (Apple GPU) backend, the present layout of "
    "the core crate and the examples, and the geode-util crate.",
    "R.W. ran the research and the AI agents that produced the rest of the code, the experiments and the text of "
    "this paper, made every scoping and claim decision, and is responsible for the results.",
    "The agents were AI models working in separate roles: Anthropic's Claude (Opus, Fable and Sonnet models, "
    "through Claude Code) and OpenAI's GPT Sol and GPT Astra models (via Codex).",
    "They were coordinated by Loom \\citep{loom}, an issue, worktree and pull-request workflow, and by "
    "Squad, a message room the agents share.",
    "Nearly every change was reviewed by a separate agent session.",
    "The agent reviews of this paper recomputed its numbers from the archived artifacts",
]
ACK = ("\\paragraph{Acknowledgments.} We thank the Palace and DeviceLayout.jl developers for publishing the "
       "SingleTransmon example and its frequencies. Section~\\ref{sec:built} gives the author contributions and the "
       "AI-orchestrated development. ")
# Operator ruling (2026-10-15): the acknowledgment is approved as written; only the DOI marker stays open.
# v21: the DOI is in the Reproducibility footnote, so no TODO stays open.
OPEN_TODOS = []


def main():
    check_operator_items()
    check_agreement()
    cc = check_crosscheck()
    check_static(cc)
    opt, slope = check_reopt(cc)
    check_controls(cc, opt, slope)
    check_derivative_discussion()
    check_citations()
    check_repro()
    check_voice()
    check_v16()
    check_v17()
    check_v18()
    check_v19()
    check_v20()
    check_v21()
    check_v22()
    check_v23()
    print("PASS (v23: reading-flow revision of v22; the body's numerals, title, authors, DOI, citations and refs.bib are "
          "v22's; sections and moved blocks in their new order; the Computed tier; the #1231 nits). (v22: the short title \"" + TITLE + "\"; v21 otherwise unchanged). (v21: each author's affiliation, 2AM Logic and ZSpaceLabs.ai, and the paper's DOI "
          "10.5281/zenodo.23580545; no PENDING marker; v20 otherwise unchanged). (v20: coauthor affiliation, the qubit frequency named and defined in Section 1, where the shift comes "
          "from (f64, the Dockerfile, the same-mesh agreement, the discretization), the Figure 1 colour rule). (v19: terminology, interval words, Palace-run unit, mesh table, Figure 1 bands, no derivative credit, coauthor and contributions; the v18, v17 and v16 checks follow, updated where v19 changed the text). v16: C_isl / C_Sigma convention, uniform-mesh rate, plasma vs f01, h/2 same discrete problem from "
          "Palace's logs, two families on both published designs, PRIMA at three trust radii, quoted SQuADDS wording, "
          "bibliography notes. Carried from v14: one Palace-run counting rule (Palace-only Newton 4 = 3 + the dl_bp2 probe); identical discrete "
          "problem only at h/2, Palace's crack bisection from its log and the fixture analysis; no 'code verification'; "
          "remesh noise against hf_mean and the probe step; the extrapolation-free bracket; the qubit-from-below "
          "mechanism; E_C and anharmonicity; the L1 bound in code; EPR and transmon citations; solver settings and mesh "
          "sizes. Carried from v13: same-mesh agreement and fixture refinement shifts; DeviceLayout cross-check (published pairs, "
          "reproduction, extrapolation rule recomputed from the ladder, bands, cap, same-mesh agreement by mode, the "
          "at-best-O(h) rate); bounded brackets, gap, conforming levels, overstatement, static route and "
          "model-dependent cap; re-optimization and controls in one environment (PRIMA 10, Newton 3 three ways, "
          "Broyden 5), the first-step Jacobian entries, the second family at step 2, the capacitor band recentred on "
          "the target and reconciled with the two-family union; the derivative discussion and the remedy; citations "
          "against refs.bib; the operator's rulings (title, validation wording, Section 8); voice and audience rules.")


if __name__ == "__main__":
    main()
