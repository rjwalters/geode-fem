#!/usr/bin/env python3
"""Render figures/runtime_scaling.pdf for the conformal-antenna paper.

All values below are transcribed verbatim from the committed artifact
benchmarks/fdtd_density_baseline/meep_runtime_scaling.json (on `main`). They are
MEASURED (Meep 1.34.0, AWS m6i.4xlarge, 16 vCPU / 61 GB), not fabricated. Do NOT
edit the numbers by hand — regenerate the JSON and re-transcribe if the
measurement changes.

Left panel:  measured seconds/timestep vs resolution R (cells/mm), log-log, with
             the fitted ~R^2.92 power law; a forward solve adds a factor ~R (CFL
             steps) -> ~R^4.
Right panel: measured peak RAM (GB) vs R, with the 61 GB box limit; the shaded
             projected region (R>=13) shows R>=14 does not fit.

R is resolution in cells/mm; frequencies elsewhere are dimensionless NATURAL
units — do NOT relabel as GHz.
"""
import numpy as np
import matplotlib.pyplot as plt

# --- measured_perstep from meep_runtime_scaling.json ---
R = np.array([4, 6, 8, 10, 12], dtype=float)
s_per_step = np.array([0.233343, 0.742044, 1.714985, 3.386214, 5.761447])
peak_rss_gb = np.array([1.573, 4.918, 11.338, 21.836, 37.417])

# fitted power law s/step ~= 0.00418 * R^2.92 (from scaling_law in the JSON)
R_fit = np.linspace(4, 16, 100)
s_fit = 0.00418 * R_fit**2.92

# projected RAM (peak_rss ~ R^2.89); anchors R=14 -> ~60 GB, R=16 -> ~83 GB (JSON projection)
R_proj = np.linspace(12, 16, 40)
rss_proj = peak_rss_gb[-1] * (R_proj / 12.0) ** 2.89

BOX_RAM_GB = 61.0  # AWS m6i.4xlarge

fig, (ax0, ax1) = plt.subplots(1, 2, figsize=(9.5, 3.6))

# Left: per-step time, log-log, measured points + fitted power law.
ax0.loglog(R, s_per_step, marker="o", ls="none", label="measured")
ax0.loglog(R_fit, s_fit, ls="--", color="0.4",
           label=r"$\sim R^{2.92}$ fit")
ax0.set_xlabel("resolution $R$ (cells/mm)")
ax0.set_ylabel("seconds / timestep")
ax0.set_title(r"per-step $\sim R^{2.92}$; forward solve $\sim R^4$")
ax0.legend(loc="upper left")

# Right: peak RAM vs R, measured + projected, with the box limit.
ax1.plot(R, peak_rss_gb, marker="o", label="measured")
ax1.plot(R_proj, rss_proj, ls=":", color="0.5", label="projected")
ax1.axhline(BOX_RAM_GB, ls="--", color="0.3", label="61 GB box limit")
ax1.set_xlabel("resolution $R$ (cells/mm)")
ax1.set_ylabel("peak RAM (GB)")
ax1.set_title(r"RAM wall: $R\geq14$ does not fit")
ax1.legend(loc="upper left")

fig.tight_layout()
fig.savefig("figures/runtime_scaling.pdf")
fig.savefig("figures/runtime_scaling.png", dpi=200)
print("wrote figures/runtime_scaling.pdf")
