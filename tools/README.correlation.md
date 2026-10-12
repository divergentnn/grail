## How well GRAIL agrees with viewers

*Calibration v2.1, 2026-10-12. Cite as: GRAIL v2.1 (2026-10-12), 574 blind showings, one reviewer, held out by film.*

### Dataset

One reviewer judged every encode blind, in 30 review rounds held 2026-10-10 to 2026-10-12: each showing is one
encode of one scene, played 1:1 (a 1920x1080 window of 4K sources) next to its source, with a verdict of pass /
borderline / fail. Encodes come from several encoders and settings (SVT-AV1 with synthetic film grain at CRF ladders
and grain strengths, x265, x264, SVT-AV1 without grain synthesis, near-lossless controls).

574 judged showings, 13 films, 48 scenes, 30 blind rounds; verdicts pass 259 / borderline 89 / fail 226.

| film | scenes | showings |
|---|---|---|
| Your Name (2016) | 3 | 121 |
| Black Swan (2010) | 5 | 81 |
| Inglourious Basterds (2009) | 4 | 76 |
| The Lighthouse (2019) | 3 | 54 |
| The Hunt (2012) | 3 | 48 |
| Coco (2017) | 3 | 41 |
| The Brutalist (2024) | 7 | 29 |
| Up (2009) | 5 | 27 |
| Nightcrawler (2014) | 3 | 25 |
| Dune (2021) | 4 | 25 |
| 1917 (2019) | 3 | 22 |
| The Super Mario Bros. Movie (2023) | 3 | 16 |
| The Wild Robot (2024) | 2 | 9 |

Every number below is **held out by film**: the weights (and the grain-dependent structure lift) are fitted on 12
films and scored on the 13th, for each film in turn. Columns: held-out log loss of an ordinal pass / borderline /
fail model on the score (lower is better; knowing only the verdict shares gives 1.023),
cross-scene concordance (does a higher score mean a better verdict for showings of DIFFERENT scenes: the property a
quality target needs), within-scene concordance (ranking versions of one scene), and AUC of pass vs not pass.
"Fast" = the `--fast` mode (section below) with the full mode's weights.

### Agreement with the verdicts

Released: **GRAILS**, the SSIMULACRA2-based GRAIL, in full and `--fast` mode. It equals SSIMULACRA2 exactly on
clean, grain-free content that is not line art, so it reads in SSIMULACRA2 points. For comparison, the same composite
was fitted on four other base metrics (XPSNR-Y, VMAF-NEG, CVVDP, Butteraugli 3-norm); none agrees clearly better with
the verdicts, so they are not released (each would read in its own base's units: dB, VMAF points, JOD, distance).
Plain base metrics are listed last.

**all**

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | 0.975 | 0.682 | 0.806 | 0.714 |
| GRAILS fast | 0.965 | 0.689 | 0.817 | 0.717 |
| XPSNR base full | 1.013 | 0.624 | 0.833 | 0.651 |
| XPSNR base fast | 0.997 | 0.616 | 0.825 | 0.643 |
| VMAF-NEG base full | 0.996 | 0.643 | 0.817 | 0.667 |
| VMAF-NEG base fast | 0.999 | 0.653 | 0.844 | 0.675 |
| CVVDP base full | 1.023 | 0.619 | 0.762 | 0.635 |
| CVVDP base fast | 1.027 | 0.607 | 0.765 | 0.618 |
| Butteraugli base full | 0.969 | 0.690 | 0.806 | 0.718 |
| Butteraugli base fast | 0.965 | 0.696 | 0.803 | 0.716 |
| plain ssimu2 | 1.029 | 0.546 | 0.757 | 0.574 |
| plain xpsnr | 1.036 | 0.508 | 0.816 | 0.531 |
| plain vmaf_neg | 1.028 | 0.553 | 0.814 | 0.574 |
| plain cvvdp | 1.034 | 0.516 | 0.719 | 0.532 |
| plain butter3 | 1.028 | 0.542 | 0.719 | 0.571 |

**clean**

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | 1.130 | 0.657 | 0.874 | 0.754 |
| GRAILS fast | 1.133 | 0.657 | 0.874 | 0.752 |
| XPSNR base full | 1.147 | 0.637 | 0.906 | 0.721 |
| XPSNR base fast | 1.154 | 0.659 | 0.890 | 0.743 |
| VMAF-NEG base full | 1.325 | 0.639 | 0.898 | 0.716 |
| VMAF-NEG base fast | 1.398 | 0.665 | 0.913 | 0.758 |
| CVVDP base full | 1.334 | 0.609 | 0.764 | 0.685 |
| CVVDP base fast | 1.306 | 0.617 | 0.748 | 0.684 |
| Butteraugli base full | 1.102 | 0.636 | 0.850 | 0.744 |
| Butteraugli base fast | 1.104 | 0.642 | 0.850 | 0.747 |
| plain ssimu2 | 1.131 | 0.658 | 0.874 | 0.754 |
| plain xpsnr | 1.148 | 0.636 | 0.906 | 0.720 |
| plain vmaf_neg | 1.329 | 0.640 | 0.898 | 0.717 |
| plain cvvdp | 1.343 | 0.612 | 0.764 | 0.689 |
| plain butter3 | 1.102 | 0.638 | 0.850 | 0.746 |

**grainy**

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | 0.886 | 0.684 | 0.823 | 0.710 |
| GRAILS fast | 0.840 | 0.698 | 0.861 | 0.728 |
| XPSNR base full | 1.175 | 0.585 | 0.873 | 0.666 |
| XPSNR base fast | 1.070 | 0.613 | 0.886 | 0.678 |
| VMAF-NEG base full | 0.987 | 0.665 | 0.709 | 0.661 |
| VMAF-NEG base fast | 0.926 | 0.694 | 0.861 | 0.701 |
| CVVDP base full | 0.996 | 0.643 | 0.709 | 0.647 |
| CVVDP base fast | 0.950 | 0.650 | 0.848 | 0.667 |
| Butteraugli base full | 0.860 | 0.728 | 0.873 | 0.771 |
| Butteraugli base fast | 0.862 | 0.735 | 0.873 | 0.770 |
| plain ssimu2 | 1.034 | 0.641 | 0.772 | 0.682 |
| plain xpsnr | 0.916 | 0.600 | 0.861 | 0.667 |
| plain vmaf_neg | 1.065 | 0.691 | 0.873 | 0.732 |
| plain cvvdp | 1.288 | 0.641 | 0.747 | 0.670 |
| plain butter3 | 1.113 | 0.630 | 0.734 | 0.680 |

**anime**

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | nan | 0.903 | 0.860 | 0.933 |
| GRAILS fast | nan | 0.905 | 0.860 | 0.933 |
| XPSNR base full | nan | 0.841 | 0.895 | 0.884 |
| XPSNR base fast | nan | 0.920 | 0.907 | 0.939 |
| VMAF-NEG base full | nan | 0.703 | 0.895 | 0.799 |
| VMAF-NEG base fast | nan | 0.852 | 0.907 | 0.917 |
| CVVDP base full | nan | 0.695 | 0.791 | 0.770 |
| CVVDP base fast | nan | 0.732 | 0.756 | 0.782 |
| Butteraugli base full | nan | 0.834 | 0.895 | 0.924 |
| Butteraugli base fast | nan | 0.845 | 0.895 | 0.927 |
| plain ssimu2 | nan | 0.903 | 0.860 | 0.933 |
| plain xpsnr | nan | 0.841 | 0.895 | 0.884 |
| plain vmaf_neg | nan | 0.703 | 0.895 | 0.799 |
| plain cvvdp | nan | 0.695 | 0.791 | 0.770 |
| plain butter3 | nan | 0.834 | 0.895 | 0.924 |

The "anime" rows are the line-art film (Your Name (2016)) without the line-art path; with it, GRAILS's held-out log
loss there drops from 1.02 to 0.73-0.75 (leave-one-scene-out / leave-one-round-out; it cannot be held out by film,
being the only line-art film).

Differences against GRAILS (full), 95 % intervals from a paired bootstrap over the 48 scenes (log loss: lower is
better; concordance and AUC: higher is better):

| score | log loss difference | cross-scene concordance difference | AUC difference |
|---|---|---|---|
| GRAILS fast | -0.010 [-0.034, +0.011] | [-0.025, +0.036] | [-0.030, +0.033] |
| XPSNR base full | +0.038 [-0.009, +0.081] | [-0.110, +0.008] | [-0.108, +0.004] |
| XPSNR base fast | +0.022 [-0.012, +0.049] | [-0.123, -0.000] | [-0.124, -0.009] |
| VMAF-NEG base full | +0.021 [-0.015, +0.049] | [-0.088, +0.031] | [-0.098, +0.030] |
| VMAF-NEG base fast | +0.024 [-0.016, +0.056] | [-0.095, +0.038] | [-0.103, +0.037] |
| CVVDP base full | +0.048 [+0.014, +0.072] | [-0.123, -0.007] | [-0.131, -0.021] |
| CVVDP base fast | +0.052 [+0.011, +0.082] | [-0.137, +0.001] | [-0.157, -0.010] |
| Butteraugli base full | -0.006 [-0.030, +0.016] | [-0.019, +0.033] | [-0.028, +0.036] |
| Butteraugli base fast | -0.010 [-0.049, +0.026] | [-0.044, +0.067] | [-0.062, +0.062] |
| plain ssimu2 | +0.054 [+0.012, +0.086] | [-0.192, -0.083] | [-0.195, -0.081] |
| plain xpsnr | +0.061 [+0.015, +0.097] | [-0.251, -0.096] | [-0.257, -0.090] |
| plain vmaf_neg | +0.053 [+0.007, +0.085] | [-0.201, -0.051] | [-0.215, -0.050] |
| plain cvvdp | +0.059 [+0.012, +0.092] | [-0.226, -0.106] | [-0.236, -0.112] |
| plain butter3 | +0.053 [+0.009, +0.083] | [-0.196, -0.085] | [-0.201, -0.078] |

Two later validation rounds (pairs, votes only; the score agrees when it rates the voted encode higher):

| score | Black Swan (2010): target encode vs earlier encode | Inglourious Basterds (2009): two full encodes |
|---|---|---|
| Butteraugli base fast | 8/8 | 8/8 |
| Butteraugli base full | 8/8 | 8/8 |
| CVVDP base fast | 8/8 | 3/8 |
| CVVDP base full | 7/8 | 7/8 |
| GRAILS fast | 8/8 | 8/8 |
| GRAILS full | 7/8 | 8/8 |
| VMAF-NEG base fast | 8/8 | 8/8 |
| VMAF-NEG base full | 8/8 | 8/8 |
| XPSNR base fast | 8/8 | 8/8 |
| XPSNR base full | 7/8 | 8/8 |

### How the flavours relate: correlation over all showings and subsets

Models fitted on all showings, full and fast mode, Pearson and Spearman. GRAILS and the Butteraugli-based composite
agree closely; the XPSNR- and VMAF-NEG-based composites rank encodes differently, and fit the verdicts worse.

Pearson, all (574 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 0.96 | 0.70 | 0.71 | 0.73 | 0.71 | 0.74 | 0.70 | 0.97 | 0.92 |
| GRAILS fast | 0.96 | 1.00 | 0.63 | 0.69 | 0.73 | 0.76 | 0.71 | 0.78 | 0.94 | 0.97 |
| XPSNR base full | 0.70 | 0.63 | 1.00 | 0.98 | 0.68 | 0.63 | 0.64 | 0.54 | 0.68 | 0.59 |
| XPSNR base fast | 0.71 | 0.69 | 0.98 | 1.00 | 0.72 | 0.70 | 0.67 | 0.63 | 0.68 | 0.65 |
| VMAF-NEG base full | 0.73 | 0.73 | 0.68 | 0.72 | 1.00 | 0.98 | 0.80 | 0.78 | 0.64 | 0.63 |
| VMAF-NEG base fast | 0.71 | 0.76 | 0.63 | 0.70 | 0.98 | 1.00 | 0.79 | 0.83 | 0.62 | 0.67 |
| CVVDP full | 0.74 | 0.71 | 0.64 | 0.67 | 0.80 | 0.79 | 1.00 | 0.91 | 0.69 | 0.64 |
| CVVDP fast | 0.70 | 0.78 | 0.54 | 0.63 | 0.78 | 0.83 | 0.91 | 1.00 | 0.65 | 0.73 |
| Butter. full | 0.97 | 0.94 | 0.68 | 0.68 | 0.64 | 0.62 | 0.69 | 0.65 | 1.00 | 0.95 |
| Butter. fast | 0.92 | 0.97 | 0.59 | 0.65 | 0.63 | 0.67 | 0.64 | 0.73 | 0.95 | 1.00 |

Spearman, all (574 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 0.96 | 0.67 | 0.68 | 0.66 | 0.65 | 0.70 | 0.64 | 0.97 | 0.90 |
| GRAILS fast | 0.96 | 1.00 | 0.57 | 0.64 | 0.64 | 0.66 | 0.66 | 0.70 | 0.94 | 0.96 |
| XPSNR base full | 0.67 | 0.57 | 1.00 | 0.95 | 0.80 | 0.75 | 0.82 | 0.64 | 0.60 | 0.47 |
| XPSNR base fast | 0.68 | 0.64 | 0.95 | 1.00 | 0.82 | 0.83 | 0.82 | 0.72 | 0.61 | 0.55 |
| VMAF-NEG base full | 0.66 | 0.64 | 0.80 | 0.82 | 1.00 | 0.96 | 0.80 | 0.72 | 0.59 | 0.54 |
| VMAF-NEG base fast | 0.65 | 0.66 | 0.75 | 0.83 | 0.96 | 1.00 | 0.78 | 0.76 | 0.59 | 0.59 |
| CVVDP full | 0.70 | 0.66 | 0.82 | 0.82 | 0.80 | 0.78 | 1.00 | 0.88 | 0.65 | 0.59 |
| CVVDP fast | 0.64 | 0.70 | 0.64 | 0.72 | 0.72 | 0.76 | 0.88 | 1.00 | 0.60 | 0.68 |
| Butter. full | 0.97 | 0.94 | 0.60 | 0.61 | 0.59 | 0.59 | 0.65 | 0.60 | 1.00 | 0.94 |
| Butter. fast | 0.90 | 0.96 | 0.47 | 0.55 | 0.54 | 0.59 | 0.59 | 0.68 | 0.94 | 1.00 |

Pearson, clean (187 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 1.00 | 0.71 | 0.72 | 0.67 | 0.69 | 0.75 | 0.75 | 0.97 | 0.97 |
| GRAILS fast | 1.00 | 1.00 | 0.72 | 0.72 | 0.68 | 0.69 | 0.75 | 0.75 | 0.97 | 0.97 |
| XPSNR base full | 0.71 | 0.72 | 1.00 | 0.99 | 0.74 | 0.73 | 0.67 | 0.66 | 0.65 | 0.65 |
| XPSNR base fast | 0.72 | 0.72 | 0.99 | 1.00 | 0.77 | 0.78 | 0.69 | 0.68 | 0.65 | 0.65 |
| VMAF-NEG base full | 0.67 | 0.68 | 0.74 | 0.77 | 1.00 | 0.98 | 0.89 | 0.87 | 0.51 | 0.51 |
| VMAF-NEG base fast | 0.69 | 0.69 | 0.73 | 0.78 | 0.98 | 1.00 | 0.91 | 0.90 | 0.54 | 0.54 |
| CVVDP full | 0.75 | 0.75 | 0.67 | 0.69 | 0.89 | 0.91 | 1.00 | 1.00 | 0.62 | 0.62 |
| CVVDP fast | 0.75 | 0.75 | 0.66 | 0.68 | 0.87 | 0.90 | 1.00 | 1.00 | 0.62 | 0.62 |
| Butter. full | 0.97 | 0.97 | 0.65 | 0.65 | 0.51 | 0.54 | 0.62 | 0.62 | 1.00 | 1.00 |
| Butter. fast | 0.97 | 0.97 | 0.65 | 0.65 | 0.51 | 0.54 | 0.62 | 0.62 | 1.00 | 1.00 |

Spearman, clean (187 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 1.00 | 0.52 | 0.56 | 0.38 | 0.47 | 0.61 | 0.71 | 0.96 | 0.96 |
| GRAILS fast | 1.00 | 1.00 | 0.53 | 0.57 | 0.39 | 0.48 | 0.62 | 0.72 | 0.96 | 0.96 |
| XPSNR base full | 0.52 | 0.53 | 1.00 | 0.95 | 0.82 | 0.78 | 0.89 | 0.83 | 0.42 | 0.42 |
| XPSNR base fast | 0.56 | 0.57 | 0.95 | 1.00 | 0.76 | 0.84 | 0.82 | 0.78 | 0.50 | 0.51 |
| VMAF-NEG base full | 0.38 | 0.39 | 0.82 | 0.76 | 1.00 | 0.83 | 0.80 | 0.71 | 0.26 | 0.27 |
| VMAF-NEG base fast | 0.47 | 0.48 | 0.78 | 0.84 | 0.83 | 1.00 | 0.64 | 0.60 | 0.44 | 0.44 |
| CVVDP full | 0.61 | 0.62 | 0.89 | 0.82 | 0.80 | 0.64 | 1.00 | 0.95 | 0.49 | 0.49 |
| CVVDP fast | 0.71 | 0.72 | 0.83 | 0.78 | 0.71 | 0.60 | 0.95 | 1.00 | 0.60 | 0.61 |
| Butter. full | 0.96 | 0.96 | 0.42 | 0.50 | 0.26 | 0.44 | 0.49 | 0.60 | 1.00 | 1.00 |
| Butter. fast | 0.96 | 0.96 | 0.42 | 0.51 | 0.27 | 0.44 | 0.49 | 0.61 | 1.00 | 1.00 |

Pearson, grainy (109 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 0.95 | 0.70 | 0.72 | 0.91 | 0.88 | 0.78 | 0.85 | 0.97 | 0.92 |
| GRAILS fast | 0.95 | 1.00 | 0.64 | 0.73 | 0.88 | 0.92 | 0.65 | 0.89 | 0.92 | 0.98 |
| XPSNR base full | 0.70 | 0.64 | 1.00 | 0.98 | 0.59 | 0.56 | 0.56 | 0.58 | 0.73 | 0.66 |
| XPSNR base fast | 0.72 | 0.73 | 0.98 | 1.00 | 0.62 | 0.64 | 0.53 | 0.66 | 0.75 | 0.75 |
| VMAF-NEG base full | 0.91 | 0.88 | 0.59 | 0.62 | 1.00 | 0.97 | 0.76 | 0.83 | 0.86 | 0.83 |
| VMAF-NEG base fast | 0.88 | 0.92 | 0.56 | 0.64 | 0.97 | 1.00 | 0.67 | 0.86 | 0.83 | 0.88 |
| CVVDP full | 0.78 | 0.65 | 0.56 | 0.53 | 0.76 | 0.67 | 1.00 | 0.82 | 0.74 | 0.59 |
| CVVDP fast | 0.85 | 0.89 | 0.58 | 0.66 | 0.83 | 0.86 | 0.82 | 1.00 | 0.79 | 0.84 |
| Butter. full | 0.97 | 0.92 | 0.73 | 0.75 | 0.86 | 0.83 | 0.74 | 0.79 | 1.00 | 0.93 |
| Butter. fast | 0.92 | 0.98 | 0.66 | 0.75 | 0.83 | 0.88 | 0.59 | 0.84 | 0.93 | 1.00 |

Spearman, grainy (109 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 0.96 | 0.87 | 0.92 | 0.91 | 0.91 | 0.82 | 0.84 | 0.98 | 0.94 |
| GRAILS fast | 0.96 | 1.00 | 0.79 | 0.92 | 0.88 | 0.93 | 0.77 | 0.89 | 0.93 | 0.99 |
| XPSNR base full | 0.87 | 0.79 | 1.00 | 0.93 | 0.79 | 0.74 | 0.68 | 0.65 | 0.88 | 0.78 |
| XPSNR base fast | 0.92 | 0.92 | 0.93 | 1.00 | 0.84 | 0.86 | 0.72 | 0.81 | 0.92 | 0.92 |
| VMAF-NEG base full | 0.91 | 0.88 | 0.79 | 0.84 | 1.00 | 0.98 | 0.85 | 0.82 | 0.86 | 0.85 |
| VMAF-NEG base fast | 0.91 | 0.93 | 0.74 | 0.86 | 0.98 | 1.00 | 0.82 | 0.87 | 0.86 | 0.90 |
| CVVDP full | 0.82 | 0.77 | 0.68 | 0.72 | 0.85 | 0.82 | 1.00 | 0.89 | 0.74 | 0.72 |
| CVVDP fast | 0.84 | 0.89 | 0.65 | 0.81 | 0.82 | 0.87 | 0.89 | 1.00 | 0.78 | 0.86 |
| Butter. full | 0.98 | 0.93 | 0.88 | 0.92 | 0.86 | 0.86 | 0.74 | 0.78 | 1.00 | 0.93 |
| Butter. fast | 0.94 | 0.99 | 0.78 | 0.92 | 0.85 | 0.90 | 0.72 | 0.86 | 0.93 | 1.00 |

Pearson, anime (121 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 1.00 | 0.89 | 0.90 | 0.77 | 0.78 | 0.87 | 0.88 | 0.92 | 0.92 |
| GRAILS fast | 1.00 | 1.00 | 0.90 | 0.91 | 0.77 | 0.78 | 0.88 | 0.89 | 0.91 | 0.91 |
| XPSNR base full | 0.89 | 0.90 | 1.00 | 0.95 | 0.94 | 0.85 | 0.91 | 0.91 | 0.69 | 0.70 |
| XPSNR base fast | 0.90 | 0.91 | 0.95 | 1.00 | 0.85 | 0.91 | 0.81 | 0.82 | 0.79 | 0.80 |
| VMAF-NEG base full | 0.77 | 0.77 | 0.94 | 0.85 | 1.00 | 0.83 | 0.87 | 0.85 | 0.51 | 0.52 |
| VMAF-NEG base fast | 0.78 | 0.78 | 0.85 | 0.91 | 0.83 | 1.00 | 0.68 | 0.69 | 0.66 | 0.67 |
| CVVDP full | 0.87 | 0.88 | 0.91 | 0.81 | 0.87 | 0.68 | 1.00 | 0.99 | 0.66 | 0.67 |
| CVVDP fast | 0.88 | 0.89 | 0.91 | 0.82 | 0.85 | 0.69 | 0.99 | 1.00 | 0.69 | 0.69 |
| Butter. full | 0.92 | 0.91 | 0.69 | 0.79 | 0.51 | 0.66 | 0.66 | 0.69 | 1.00 | 1.00 |
| Butter. fast | 0.92 | 0.91 | 0.70 | 0.80 | 0.52 | 0.67 | 0.67 | 0.69 | 1.00 | 1.00 |

Spearman, anime (121 showings)

| | GRAILS full | GRAILS fast | XPSNR base full | XPSNR base fast | VMAF-NEG base full | VMAF-NEG base fast | CVVDP full | CVVDP fast | Butter. full | Butter. fast |
|---|---|---|---|---|---|---|---|---|---|---|
| GRAILS full | 1.00 | 1.00 | 0.87 | 0.97 | 0.71 | 0.87 | 0.71 | 0.75 | 0.88 | 0.89 |
| GRAILS fast | 1.00 | 1.00 | 0.88 | 0.98 | 0.72 | 0.86 | 0.73 | 0.76 | 0.88 | 0.89 |
| XPSNR base full | 0.87 | 0.88 | 1.00 | 0.91 | 0.94 | 0.73 | 0.89 | 0.90 | 0.62 | 0.64 |
| XPSNR base fast | 0.97 | 0.98 | 0.91 | 1.00 | 0.75 | 0.89 | 0.72 | 0.75 | 0.84 | 0.85 |
| VMAF-NEG base full | 0.71 | 0.72 | 0.94 | 0.75 | 1.00 | 0.54 | 0.94 | 0.92 | 0.40 | 0.42 |
| VMAF-NEG base fast | 0.87 | 0.86 | 0.73 | 0.89 | 0.54 | 1.00 | 0.45 | 0.49 | 0.81 | 0.82 |
| CVVDP full | 0.71 | 0.73 | 0.89 | 0.72 | 0.94 | 0.45 | 1.00 | 0.99 | 0.44 | 0.45 |
| CVVDP fast | 0.75 | 0.76 | 0.90 | 0.75 | 0.92 | 0.49 | 0.99 | 1.00 | 0.49 | 0.51 |
| Butter. full | 0.88 | 0.88 | 0.62 | 0.84 | 0.40 | 0.81 | 0.44 | 0.49 | 1.00 | 1.00 |
| Butter. fast | 0.89 | 0.89 | 0.64 | 0.85 | 0.42 | 0.82 | 0.45 | 0.51 | 1.00 | 1.00 |

### Fast mode against full mode

`--fast` scores every 4th frame plus its successor and does not denoise AV1 synthetic-grain candidates. Same
weights; the score difference per showing, and how many showings (scenes, by scene mean) would change side at the
median score. Agreement with the verdicts is unchanged (table above), but use the full mode as an encode target on
heavy, coarse grain. The GRAILS drift comes from AV1 synthetic-grain candidates on heavy, coarse grain: fast mode does
not denoise them, so their grain is measured exactly while the source's grain is measured through the denoiser,
which captures only part of it on such sources.

| flavour | mean abs diff | max abs diff | mean abs diff, grainy | Pearson | Spearman | showings flipped at the median | scenes flipped (scene means) |
|---|---|---|---|---|---|---|---|
| GRAILS | 1.61 | 20.31 | 5.74 | 0.964 | 0.963 | 4.9 % | 6.2 % |
| XPSNR base | 0.88 | 3.74 | 1.01 | 0.979 | 0.949 | 10.5 % | 10.4 % |
| VMAF-NEG base | 0.57 | 5.90 | 1.35 | 0.980 | 0.963 | 7.0 % | 4.2 % |
| CVVDP base | 0.07 | 0.82 | 0.23 | 0.913 | 0.877 | 8.7 % | 4.2 % |
| Butteraugli base | 0.08 | 1.02 | 0.30 | 0.949 | 0.941 | 7.0 % | 4.2 % |

### Speed

Python reference on an RTX 3090 and a 12-thread CPU (machine otherwise idle), one AV1 film-grain encode against its
source, 88 frames of the whole active picture (1080p: 1920x792; 4K: 3840x1600 HDR), GRAILS. Decode = source,
candidate with full-frame grain, and the candidate's grain-free decode. Software decode at 1080p, NVDEC for the 4K
HEVC source: GRAILS full 13.8 / fast 22.4 frames/s at
1080p, 5.3 / 8.8 at 4K.

1-minute load at start 2.57, at end 3.25

| picture | mode | decode | frames/s | decode share | denoise s | metrics s | features s |
|---|---|---|---|---|---|---|---|
| 1080p | full | software | 13.8 | 31 % | 1.94 | 0.54 | 1.91 |
| 1080p | fast | software | 22.4 | 46 % | 1.00 | 0.18 | 0.96 |
| 1080p | full | NVDEC ref | 14.4 | 29 % | 1.98 | 0.55 | 1.82 |
| 1080p | fast | NVDEC ref | 22.3 | 46 % | 0.96 | 0.18 | 0.97 |
| 4K | full | software | 4.8 | 28 % | 4.48 | 1.88 | 6.71 |
| 4K | fast | software | 7.5 | 44 % | 2.24 | 0.65 | 3.62 |
| 4K | full | NVDEC ref | 5.3 | 22 % | 4.53 | 1.86 | 6.65 |
| 4K | fast | NVDEC ref | 8.8 | 34 % | 2.27 | 0.65 | 3.63 |

Fast mode is faster on both decode paths even with decoding counted (about 1.6x), but decoding and the source's
denoise (needed in every mode) bound it: they are 71 % of a fast run at 1080p. NVDEC helps only for the 4K HEVC
source (software 41 frames/s, NVDEC 167); for 1080p AV1, software dav1d (369 frames/s) beats NVDEC (125). The
release decodes in software up to 1080p and in hardware above.
