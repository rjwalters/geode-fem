### Set-up

Torque factor ScaleR^2 * LENGTH / 0.1 = 1.0477172208^2 * 0.0800005401 / 0.1 = 0.8781750281. GEODE current 11.048494105 A (unscaled), load angle -11.646272734 deg, harmonic coupling n = 3:6:45 (tied ends).

| family | level | nodes | triangles | group-1 area (mm^2) | vs history area | Newton its (max) | solve wall (s) |
|---|---|---|---|---|---|---|---|
| moved | L0 | 3153 | 5938 | 38.993222 | -1.40e-02 | 11 | 3.9 |
| moved | L1 | 9937 | 19163 | 39.406971 | -3.49e-03 | 12 | 13.1 |
| moved | L2 | 36083 | 70757 | 39.503392 | -1.05e-03 | 12 | 129.5 |
| moved | L3 | 139268 | 275736 | 39.534422 | -2.64e-04 | 12 | 741.5 |
| fresh scalar | L0 | 3046 | 5732 | 38.998413 | -1.38e-02 | 11 | 2.6 |
| fresh scalar | L1 | 9493 | 18283 | 39.379414 | -4.18e-03 | 12 | 12.1 |
| fresh scalar | L2 | 35467 | 69537 | 39.503144 | -1.06e-03 | 12 | 123.2 |
| fresh scalar | L3 | 137103 | 271428 | 39.534669 | -2.58e-04 | 12 | 751.2 |
| fresh full | L0 | 3020 | 5680 | 38.998413 | -1.38e-02 | 11 | 2.4 |
| fresh full | L1 | 9497 | 18291 | 39.379414 | -4.18e-03 | 12 | 12.1 |
| fresh full | L2 | 35462 | 69529 | 39.503144 | -1.06e-03 | 12 | 118.4 |
| fresh full | L3 | 136892 | 271006 | 39.534669 | -2.58e-04 | 12 | 762.8 |

The history's group-1 area for iteration 120 is 39.544881 mm^2 (the reference's quadrature of its own geometry). The group-1 winding is not touched by the free-form motion, so all three families carry the same group geometry; the differences are the O(h^2) polygon error of the mesh.

### ScaleR similarity check (moved L0)

Mesh scaled by ScaleR, current 11.575697538 A, coupling torque times LENGTH/0.1, against the unscaled solve times ScaleR^2 LENGTH/0.1: max |dT| = 7.838e-14 N m over 60 solves (5.81e-14 of the largest torque).

### Optimized design vs the saved arrays, coupling torque, L0-L3

Coupling torque -lambda^T d(G^T A)/dbeta (the reference's estimator), converted to the saved arrays' ScaleR and LENGTH. N m per modelled sector; std is the population std over the 20 angles. d % is (GEODE - reference) / reference.

| current | level | moved mean | moved std | fresh scalar mean | fresh scalar std | fresh full mean | fresh full std | ref mean | ref std | moved d mean % | moved d std % | fresh scalar d mean % | fresh scalar d std % | fresh full d mean % | fresh full d std % |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.0 | L0 | -0.00000 | 0.00210 | -0.00000 | 0.00241 | 0.00000 | 0.00061 | 0.00011 | 0.00066 | n/a | +217.3 | n/a | +262.9 | n/a | -8.2 |
| 0.0 | L1 | -0.00000 | 0.00237 | 0.00000 | 0.00249 | -0.00000 | 0.00067 | 0.00011 | 0.00066 | n/a | +256.8 | n/a | +275.7 | n/a | +1.3 |
| 0.0 | L2 | -0.00001 | 0.00236 | -0.00000 | 0.00238 | -0.00000 | 0.00063 | 0.00011 | 0.00066 | n/a | +256.7 | n/a | +258.5 | n/a | -5.3 |
| 0.0 | L3 | -0.00000 | 0.00233 | 0.00000 | 0.00233 | 0.00000 | 0.00061 | 0.00011 | 0.00066 | n/a | +251.6 | n/a | +251.8 | n/a | -7.8 |
| 0.5 | L0 | 0.68061 | 0.00728 | 0.68037 | 0.00794 | 0.75077 | 0.00050 | 0.74975 | 0.00044 | -9.2 | +1563.3 | -9.3 | +1714.2 | +0.1 | +13.3 |
| 0.5 | L1 | 0.67842 | 0.00772 | 0.67840 | 0.00795 | 0.74894 | 0.00055 | 0.74975 | 0.00044 | -9.5 | +1664.2 | -9.5 | +1716.4 | -0.1 | +25.7 |
| 0.5 | L2 | 0.67781 | 0.00770 | 0.67785 | 0.00769 | 0.74866 | 0.00055 | 0.74975 | 0.00044 | -9.6 | +1661.3 | -9.6 | +1658.1 | -0.1 | +25.4 |
| 0.5 | L3 | 0.67773 | 0.00757 | 0.67773 | 0.00757 | 0.74860 | 0.00055 | 0.74975 | 0.00044 | -9.6 | +1631.1 | -9.6 | +1631.4 | -0.2 | +26.4 |
| 1.0 | L0 | 1.33868 | 0.00703 | 1.33847 | 0.00797 | 1.50330 | 0.00043 | 1.50001 | 0.00018 | -10.8 | +3897.8 | -10.8 | +4433.1 | +0.2 | +142.5 |
| 1.0 | L1 | 1.33296 | 0.00744 | 1.33348 | 0.00766 | 1.49937 | 0.00027 | 1.50001 | 0.00018 | -11.1 | +4128.1 | -11.1 | +4256.2 | -0.0 | +55.1 |
| 1.0 | L2 | 1.33202 | 0.00745 | 1.33232 | 0.00748 | 1.49873 | 0.00031 | 1.50001 | 0.00018 | -11.2 | +4139.0 | -11.2 | +4151.4 | -0.1 | +75.8 |
| 1.0 | L3 | 1.33184 | 0.00740 | 1.33201 | 0.00740 | 1.49857 | 0.00035 | 1.50001 | 0.00018 | -11.2 | +4106.5 | -11.2 | +4106.9 | -0.1 | +100.6 |

### Estimators at L3 (mean, std; N m)

Arkkio over the outer half band (44.5-45.0 mm) is valid for every family. The full band (44.0-45.0 mm) assumes the rotor surface is the 44 mm circle, which holds for the moved and fresh-scalar meshes but not for the full iteration-120 rotor (pulled in to 42.6-44.0 mm), so it is not reported there.

| current | family | coupling mean | coupling std | Arkkio outer mean | Arkkio outer std | Arkkio full mean | Arkkio full std |
|---|---|---|---|---|---|---|---|
| 0.0 | moved | -0.00000 | 0.00233 | -0.00013 | 0.00238 | -0.00007 | 0.00235 |
| 0.0 | fresh scalar | 0.00000 | 0.00233 | 0.00003 | 0.00233 | 0.00001 | 0.00233 |
| 0.0 | fresh full | 0.00000 | 0.00061 | 0.00006 | 0.00061 | n/a | n/a |
| 0.5 | moved | 0.67773 | 0.00757 | 0.67436 | 0.00756 | 0.67604 | 0.00757 |
| 0.5 | fresh scalar | 0.67773 | 0.00757 | 0.67778 | 0.00759 | 0.67775 | 0.00758 |
| 0.5 | fresh full | 0.74860 | 0.00055 | 0.74870 | 0.00056 | n/a | n/a |
| 1.0 | moved | 1.33184 | 0.00740 | 1.32575 | 0.00731 | 1.32879 | 0.00735 |
| 1.0 | fresh scalar | 1.33201 | 0.00740 | 1.33208 | 0.00741 | 1.33204 | 0.00741 |
| 1.0 | fresh full | 1.49857 | 0.00035 | 1.49871 | 0.00036 | n/a | n/a |

### L3 band per family (coupling torque)

Band = L3 +- max(|d12|, |d23|), the rule of ../forward and ../forward_harmonic.

| family | current | quantity | L0 | L1 | L2 | L3 | L3 band | reference | ref in band? |
|---|---|---|---|---|---|---|---|---|---|
| moved | 0.0 | std | 0.00210 | 0.00237 | 0.00236 | 0.00233 | [0.00230, 0.00236] | 0.00066 | no |
| moved | 0.5 | mean | 0.68061 | 0.67842 | 0.67781 | 0.67773 | [0.67711, 0.67834] | 0.74975 | no |
| moved | 0.5 | std | 0.00728 | 0.00772 | 0.00770 | 0.00757 | [0.00744, 0.00770] | 0.00044 | no |
| moved | 1.0 | mean | 1.33868 | 1.33296 | 1.33202 | 1.33184 | [1.33090, 1.33278] | 1.50001 | no |
| moved | 1.0 | std | 0.00703 | 0.00744 | 0.00745 | 0.00740 | [0.00734, 0.00745] | 0.00018 | no |
| fresh scalar | 0.0 | std | 0.00241 | 0.00249 | 0.00238 | 0.00233 | [0.00222, 0.00245] | 0.00066 | no |
| fresh scalar | 0.5 | mean | 0.68037 | 0.67840 | 0.67785 | 0.67773 | [0.67717, 0.67828] | 0.74975 | no |
| fresh scalar | 0.5 | std | 0.00794 | 0.00795 | 0.00769 | 0.00757 | [0.00732, 0.00783] | 0.00044 | no |
| fresh scalar | 1.0 | mean | 1.33847 | 1.33348 | 1.33232 | 1.33201 | [1.33084, 1.33317] | 1.50001 | no |
| fresh scalar | 1.0 | std | 0.00797 | 0.00766 | 0.00748 | 0.00740 | [0.00721, 0.00758] | 0.00018 | no |
| fresh full | 0.0 | std | 0.00061 | 0.00067 | 0.00063 | 0.00061 | [0.00057, 0.00065] | 0.00066 | no |
| fresh full | 0.5 | mean | 0.75077 | 0.74894 | 0.74866 | 0.74860 | [0.74831, 0.74888] | 0.74975 | no |
| fresh full | 0.5 | std | 0.00050 | 0.00055 | 0.00055 | 0.00055 | [0.00055, 0.00056] | 0.00044 | no |
| fresh full | 1.0 | mean | 1.50330 | 1.49937 | 1.49873 | 1.49857 | [1.49794, 1.49920] | 1.50001 | no |
| fresh full | 1.0 | std | 0.00043 | 0.00027 | 0.00031 | 0.00035 | [0.00031, 0.00040] | 0.00018 | no |

### Mesh-motion error and geometry effect, per level

Moved - fresh scalar isolates the mesh motion (same 9-parameter geometry, up to the stator bore's sub-micron free-form offsets that the fresh mesh keeps). Fresh full - fresh scalar is the free-form pole shaping that the 9 parameters cannot represent. Values are max over the 20 angles of |dT| and the differences of the means and stds (N m).

| current | level | moved - scalar: max abs dT | d mean | d std | full - scalar: max abs dT | d mean | d std |
|---|---|---|---|---|---|---|---|
| 0.0 | L0 | 0.00045 | -0.00000 | -0.00030 | 0.00277 | +0.00000 | -0.00180 |
| 0.0 | L1 | 0.00021 | -0.00000 | -0.00013 | 0.00280 | -0.00001 | -0.00182 |
| 0.0 | L2 | 0.00003 | -0.00001 | -0.00001 | 0.00267 | +0.00000 | -0.00175 |
| 0.0 | L3 | 0.00003 | -0.00000 | -0.00000 | 0.00263 | -0.00000 | -0.00172 |
| 0.5 | L0 | 0.00129 | +0.00024 | -0.00066 | 0.08398 | +0.07040 | -0.00744 |
| 0.5 | L1 | 0.00048 | +0.00002 | -0.00023 | 0.08388 | +0.07053 | -0.00740 |
| 0.5 | L2 | 0.00014 | -0.00004 | +0.00001 | 0.08389 | +0.07080 | -0.00714 |
| 0.5 | L3 | 0.00005 | +0.00000 | -0.00000 | 0.08384 | +0.07087 | -0.00702 |
| 1.0 | L0 | 0.00235 | +0.00021 | -0.00094 | 0.17966 | +0.16483 | -0.00755 |
| 1.0 | L1 | 0.00122 | -0.00052 | -0.00023 | 0.17981 | +0.16589 | -0.00739 |
| 1.0 | L2 | 0.00047 | -0.00030 | -0.00002 | 0.18002 | +0.16642 | -0.00717 |
| 1.0 | L3 | 0.00024 | -0.00017 | -0.00000 | 0.18011 | +0.16656 | -0.00705 |

### L3 waveforms, full current (N m)

| deg | moved | fresh scalar | fresh full | reference |
|---|---|---|---|---|
| 0 | 1.33433 | 1.33446 | 1.49871 | 1.50000 |
| 1 | 1.33711 | 1.33730 | 1.49847 | 1.49989 |
| 2 | 1.33639 | 1.33661 | 1.49847 | 1.49993 |
| 3 | 1.33377 | 1.33401 | 1.49877 | 1.50015 |
| 4 | 1.33177 | 1.33200 | 1.49895 | 1.50018 |
| 5 | 1.33144 | 1.33164 | 1.49890 | 1.50004 |
| 6 | 1.33256 | 1.33273 | 1.49869 | 1.49987 |
| 7 | 1.33509 | 1.33523 | 1.49858 | 1.49992 |
| 8 | 1.33887 | 1.33898 | 1.49856 | 1.50013 |
| 9 | 1.34231 | 1.34242 | 1.49844 | 1.50025 |
| 10 | 1.34340 | 1.34354 | 1.49803 | 1.50001 |
| 11 | 1.34114 | 1.34132 | 1.49766 | 1.49972 |
| 12 | 1.33573 | 1.33593 | 1.49783 | 1.49980 |
| 13 | 1.32916 | 1.32936 | 1.49848 | 1.50019 |
| 14 | 1.32374 | 1.32392 | 1.49898 | 1.50039 |
| 15 | 1.32021 | 1.32038 | 1.49899 | 1.50016 |
| 16 | 1.31848 | 1.31864 | 1.49875 | 1.49984 |
| 17 | 1.31921 | 1.31934 | 1.49861 | 1.49975 |
| 18 | 1.32312 | 1.32323 | 1.49867 | 1.49989 |
| 19 | 1.32896 | 1.32908 | 1.49881 | 1.50011 |

### Boundary reproduction (check_boundaries.py)

| design | export interface sides | export -> GEODE max (m) | GEODE -> export max (m) | worst place |
|---|---|---|---|---|
| initial | 143 | 6.979e-16 | 7.047e-16 | stator patch 102 side 2 (winding_12_V+/winding_04_U-) |
| optimized | 143 | 1.403e-03 | 5.000e-04 | rotor patch 1 side 4 (rotor_iron/airgap_inner) |

Optimized design, export -> GEODE by region pair (pairs above 1e-14 m):

| region pair | max deviation (m) |
|---|---|
| rotor:airgap_inner/rotor_iron | 1.403e-03 |
| stator:airgap_outer/slot_air | 1.323e-08 |
| stator:airgap_outer/stator_iron | 1.496e-07 |
| stator:slot_air/stator_iron | 3.381e-10 |
| every other pair | <= 6.1e-16 |

### Fixed-topology validity over the parameter box

One parameter at a time from theta0 (others at theta0), 200 steps to each bound. 'valid' is the largest interval around theta0 with no inverted triangle and a constructible geometry; the reason names the first failure beyond it. Min angle is the smallest interior angle over the valid interval (degrees); theta0 has 13.20 deg at every level.

| parameter | box [min, max] (mm) | theta0 (mm) | L | valid [lo, hi] (mm) | min angle over valid range | failure below | failure above |
|---|---|---|---|---|---|---|---|
| MAG | [6, 15] | 7 | L0 | [6, 12.36] | 0.35 | - | 12.4: inverted |
| MH | [2, 12] | 7 | L0 | [2, 12] | 4.01 | - | - |
| MW | [10, 25] | 19 | L0 | [10, 25] | 11.96 | - | - |
| SW1 | [2, 6] | 4 | L0 | [2, 5.72] | 1.55 | - | 5.73: no geometry |
| SW2 | [1, 4] | 2.3 | L0 | [1, 4] | 5.12 | - | - |
| SW3 | [0.5, 1.5] | 1 | L0 | [0.5, 1.5] | 13.20 | - | - |
| SW4 | [2, 20] | 8.25 | L0 | [2.094, 16.77] | 0.24 | 2.062: inverted | 16.83: inverted |
| SD1 | [90, 160] | 135 | L0 | [117.9, 160] | 0.13 | 117.7: inverted | - |
| Sr1 | [0.5, 2] | 1 | L0 | [0.5, 2] | 13.20 | - | - |
| optimized (all 9) | | | L0 | | 13.16 (inverted 0) | | |
| MAG | [6, 15] | 7 | L1 | [6, 12.32] | 0.17 | - | 12.36: inverted |
| MH | [2, 12] | 7 | L1 | [2, 12] | 4.64 | - | - |
| MW | [10, 25] | 19 | L1 | [10, 25] | 8.02 | - | - |
| SW1 | [2, 6] | 4 | L1 | [2, 5.72] | 1.16 | - | 5.73: no geometry |
| SW2 | [1, 4] | 2.3 | L1 | [1, 4] | 4.83 | - | - |
| SW3 | [0.5, 1.5] | 1 | L1 | [0.5, 1.5] | 13.20 | - | - |
| SW4 | [2, 20] | 8.25 | L1 | [2.312, 16.18] | 0.19 | 2.281: inverted | 16.24: inverted |
| SD1 | [90, 160] | 135 | L1 | [119.2, 160] | 0.98 | 119: inverted | - |
| Sr1 | [0.5, 2] | 1 | L1 | [0.5, 2] | 8.60 | - | - |
| optimized (all 9) | | | L1 | | 7.85 (inverted 0) | | |

### Optimized design: extension variants (no remeshing)

| level | edges | stiffening chi | min angle (deg) | max angle (deg) | area ratio [min, max] | inverted |
|---|---|---|---|---|---|---|
| L0 | Slide | 0 | 12.91 | 139.87 | [0.218, 2.644] | 0 |
| L0 | Slide | 1 | 13.16 | 138.85 | [0.168, 2.524] | 0 |
| L0 | Stretch | 0 | 12.14 | 153.06 | [0.165, 2.644] | 0 |
| L1 | Slide | 0 | 2.18 | 170.44 | [0.124, 2.940] | 0 |
| L1 | Slide | 1 | 7.85 | 154.12 | [0.117, 2.728] | 0 |
| L1 | Stretch | 0 | 2.18 | 170.46 | [0.105, 2.940] | 0 |
| L2 | Slide | 0 | 1.33 | 173.92 | [0.097, 3.059] | 0 |
| L2 | Slide | 1 | 6.90 | 153.28 | [0.107, 2.776] | 0 |
| L2 | Stretch | 0 | 0.70 | 178.27 | [-0.027, 3.059] | 1 |
| L3 | Slide | 0 | 5.44 | 164.66 | [0.140, 3.118] | 0 |
| L3 | Slide | 1 | 7.14 | 155.26 | [0.112, 2.966] | 0 |
| L3 | Stretch | 0 | 0.15 | 178.64 | [-0.116, 3.118] | 6 |

### dX/dtheta against central differences of X(theta)

Smallest max-norm discrepancy over h/theta in 1e-3..1e-7, relative to the largest |dX/dtheta| of that parameter.

| level | design | parameter | max abs dX/dtheta | best rel. discrepancy | at h/theta |
|---|---|---|---|---|---|
| L0 | initial | MAG | 1.0000 | 5.2e-13 | 1e-03 |
| L0 | initial | MH | 1.0000 | 5.1e-13 | 1e-03 |
| L0 | initial | MW | 0.5000 | 1.8e-13 | 1e-03 |
| L0 | initial | SW1 | 0.8074 | 1.6e-09 | 1e-05 |
| L0 | initial | SW2 | 0.5002 | 2.3e-09 | 1e-04 |
| L0 | initial | SW3 | 1.0814 | 7.1e-11 | 1e-04 |
| L0 | initial | SW4 | 1.0038 | 8.0e-13 | 1e-03 |
| L0 | initial | SD1 | 0.5019 | 1.3e-13 | 1e-03 |
| L0 | initial | Sr1 | 1.0913 | 6.2e-12 | 1e-03 |
| L0 | optimized | MAG | 1.0000 | 7.7e-13 | 1e-03 |
| L0 | optimized | MH | 1.0000 | 1.3e-12 | 1e-03 |
| L0 | optimized | MW | 0.5000 | 1.6e-13 | 1e-03 |
| L0 | optimized | SW1 | 0.7399 | 4.1e-10 | 1e-04 |
| L0 | optimized | SW2 | 0.5000 | 6.4e-10 | 1e-04 |
| L0 | optimized | SW3 | 1.0671 | 1.7e-10 | 1e-03 |
| L0 | optimized | SW4 | 1.0038 | 1.5e-12 | 1e-03 |
| L0 | optimized | SD1 | 0.5019 | 1.8e-13 | 1e-03 |
| L0 | optimized | Sr1 | 1.0913 | 3.4e-12 | 1e-03 |
| L1 | initial | MAG | 1.0000 | 5.2e-13 | 1e-03 |
| L1 | initial | MH | 1.0000 | 6.7e-13 | 1e-03 |
| L1 | initial | MW | 0.5000 | 1.8e-13 | 1e-03 |
| L1 | initial | SW1 | 0.8172 | 1.6e-09 | 1e-05 |
| L1 | initial | SW2 | 0.5002 | 2.3e-09 | 1e-04 |
| L1 | initial | SW3 | 1.0949 | 7.3e-11 | 1e-04 |
| L1 | initial | SW4 | 1.0038 | 9.1e-13 | 1e-03 |
| L1 | initial | SD1 | 0.5019 | 1.4e-13 | 1e-03 |
| L1 | initial | Sr1 | 1.0913 | 6.5e-12 | 1e-03 |
| L1 | optimized | MAG | 1.0000 | 7.7e-13 | 1e-03 |
| L1 | optimized | MH | 1.0000 | 1.3e-12 | 1e-03 |
| L1 | optimized | MW | 0.5000 | 1.6e-13 | 1e-03 |
| L1 | optimized | SW1 | 0.7477 | 4.1e-10 | 1e-04 |
| L1 | optimized | SW2 | 0.5000 | 6.6e-10 | 1e-04 |
| L1 | optimized | SW3 | 1.0791 | 1.7e-10 | 1e-03 |
| L1 | optimized | SW4 | 1.0038 | 1.5e-12 | 1e-03 |
| L1 | optimized | SD1 | 0.5019 | 1.7e-13 | 1e-03 |
| L1 | optimized | Sr1 | 1.0913 | 3.7e-12 | 1e-03 |

