#!/usr/bin/env python3
"""Render figures/s11_band.pdf for the conformal-antenna paper.

All values below are transcribed verbatim from the committed artifact
benchmarks/patch_antenna_conformal/conformal_results.toml (branch
feature/issue-650, commit 524db3b). They are MEASURED, not fabricated. Do not
edit the numbers by hand — regenerate the TOML and re-transcribe if the run
changes.

Left panel:  worst-of-band return loss (dB) vs optimization step (trajectory).
Right panel: final per-frequency |S11| (dB) vs omega, against the -10 dB spec.

Frequencies are dimensionless NATURAL units — do NOT relabel as GHz.
"""
import matplotlib.pyplot as plt

# --- trajectory.step: (iter, worst_s11_db) from conformal_results.toml ---
traj_iter = [0, 1, 2, 3, 4, 5, 6]
worst_s11_db = [
    -5.506281,   # iter 0
    -5.586476,   # iter 1
    -5.751564,   # iter 2
    -6.101132,   # iter 3
    -6.882572,   # iter 4
    -8.251119,   # iter 5
    -12.06262,   # iter 6 (terminal: target_reached)
]

# --- s11_band.point: final per-frequency |S11| (dB) ---
omega = [0.30, 0.35, 0.40]
s11_db_final = [-12.06262, -23.92241, -14.42428]

TARGET_DB = -10.0

fig, (ax0, ax1) = plt.subplots(1, 2, figsize=(9.5, 3.6))

# Left: convergence of worst-of-band return loss.
ax0.plot(traj_iter, worst_s11_db, marker="o")
ax0.axhline(TARGET_DB, ls="--", color="0.4", label="$-10$ dB spec")
ax0.set_xlabel("optimization step")
ax0.set_ylabel(r"worst-of-band $|S_{11}|$ (dB)")
ax0.set_title(r"convergence: $-5.51 \to -12.06$ dB")
ax0.legend(loc="upper right")

# Right: final per-frequency return loss vs the spec.
ax1.bar([str(w) for w in omega], s11_db_final)
ax1.axhline(TARGET_DB, ls="--", color="0.4", label="$-10$ dB spec")
ax1.set_xlabel(r"$\omega$ (natural units)")
ax1.set_ylabel(r"final $|S_{11}|$ (dB)")
ax1.set_title("entire band below spec")
ax1.legend(loc="lower right")

fig.tight_layout()
fig.savefig("figures/s11_band.pdf")
fig.savefig("figures/s11_band.png", dpi=200)
print("wrote figures/s11_band.pdf")
