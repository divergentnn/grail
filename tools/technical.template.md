# GRAIL technical notes

GRAIL (Grain- and Ringing-Aware Image Likeness) is the method; GRAILS is its score on the SSIMULACRA2 base. How GRAILS
is computed, how closely this Rust implementation reproduces the Python reference, how fast it runs, and
how well it agrees with viewers. For installing and using GRAIL see [install.md](install.md) and
[usage.md](usage.md); the [README](../README.md) is the short version.

<!-- Generated: tools/readme_tables.py tools/technical.template.md tools/README.correlation.md results/ docs/technical.md -->

## What GRAILS measures

Pixel-comparing metrics (SSIMULACRA2, Butteraugli, VMAF, XPSNR, PSNR) rank a grain-free encode of a grainy film
above a faithful one. Film grain is a random field, so two faithful copies with different grain draws differ
everywhere, while a copy with the grain wiped off is closer to the source pixel for pixel. Viewers fail such encodes
("missing grain", "smoothed skin").

GRAILS keeps SSIMULACRA2 where the source has no grain and replaces the pixel comparison of the grain with a
comparison of its statistics where it does:

    Q = S0 + g * (S_struct - S0 - sum_j w_j p_j)  -  (1 - g) * a * w_L * p_L

- `S0`: SSIMULACRA2 of the source and the encode as watched. AV1 film grain is rendered full-frame, never as the
  standard decoder's 32x32 tiles.
- `S_struct`: structure with the grain taken out. A fixed measurement denoiser
  ([flowdenoise](https://github.com/divergentnn/flowdenoise), formerly flowdegrain2; motion-compensated, GPU) splits
  source and encode into structure and residual. For AV1 encodes that carry film-grain parameters, `S_struct` compares
  the denoised source with the grain-free decode; for every other encode, it compares the two denoised pictures. A
  grain-dependent offset puts both paths on one scale, capped at 100. Blur, blocking, smear and ringing still count;
  grain counts neither for nor against.
- `p_j`: grain penalties measured against the source's own grain, per luma level, in flat areas: too weak or too
  strong (`under_amp`, `over_amp`, `over`), blobby (`blob`), wrong chroma grain (`chroma`), grain frozen across
  frames (`stick`, `stickc`), and lost texture (`texloss`). Each penalty is about 0 for a faithful copy.
- `g`: how grainy the source is (0 to 1, a smooth step of the source's grain rms `sigma_g`; measured on the source
  only, so an encode cannot change it).
- `a`, `p_L`: the anime / line-art path. `a` is the confidence that the source is 2D line art (ink strokes drawn over
  flat colour, from the source only); `p_L` is ringing that the encode adds next to the source's lines.

With no grain and no line art (`g = 0`, `a = 0`), GRAILS **is** SSIMULACRA2, exactly. In between, the score moves
smoothly toward the grain-aware form. `Q <= 100` always; it reads in SSIMULACRA2 points.

Outputs: the GRAILS score; its parts (plain SSIMULACRA2, structure, the grain term `-sum w p`, the line-art term);
the structure path (`synth` or `coded`); `g` and `sigma_g`; every penalty and raw feature; per-frame values with
`--json`; the flavour, the mode and the GRAILS model version (`model_version`). A separate, uncalibrated
"standard-decoder seed view" rates the AV1 grain seeds for players that tile the grain; it never enters the score.

## Flavour naming rule

GRAIL is the method; a flavour is GRAIL on one base metric, named by a letter after it: **GRAILS** = SSIMULACRA2
base. A score is always written with its flavour letter ("GRAILS 87.3", never "GRAIL 87.3"), and with the mode when
it is not the full mode ("GRAILS 87.3 (fast)"). Scores of different flavours are on different scales and must not
be compared. Only GRAILS is released: the composite was also fitted on XPSNR-Y, VMAF-NEG, CVVDP and Butteraugli, and
none agrees clearly better with the verdicts (tables below). A Butteraugli flavour (likely "GRAILB") is the first
candidate for a second release once more verdicts exist; the letter keeps the two apart.

## Version numbers

- **The model.** Every score carries the version of the model that produced it: "GRAILS 87.49 (full mode, GRAILS
  model 0.2.1, grail 0.1.0-beta.1)"; `model_version` in the JSON report; `version` in `crates/grail/data/weights.json`.
  The current model is **0.2.1 = the lab's v2.1** (calibration of 2026-10-12: 574 blind showings, 13 films, one
  viewer). The next model, fitted in the lab as v2.2, will be 0.3.0.
- **The rule.** GRAILS stays below 1.0 while it is in beta, and below 1.0 any change that moves scores bumps the minor
  number (0.2 -> 0.3). Scores are comparable only within one model version. 1.0 comes when calibration from other
  viewers is in and the scores are stable.
- **The tool** (the `grail` crate, 0.1.0-beta.1) has its own version. A tool change that is not a model change must
  not move scores: on CUDA a refactor gives identical scores, and other backends stay within `check-reference`'s
  tolerances.

## Limits

- **One reviewer.** The weights encode one person's tolerance (574 blind showings, 13 films). More reviewers are
  needed before a public pass threshold can be stated; until then, use GRAILS to compare encodes of the same source,
  not as an absolute grade.
- **A fixed target is not stable across scenes.** The per-scene pass line still spreads by about 14-16 points (24-28
  CRF steps). Per-scene minimums work; one film-wide target does not.
- **Heavy grain: prefer full mode.** Fast mode skips the denoise of AV1 synthetic-grain encodes; on heavy, coarse
  grain the denoiser captures only part of the source's grain, so fast mode can be up to 20 points off. Use full mode
  or `--fast --denoise-candidate` there.
- The structure term is distance to the measurement denoiser's output, not to the true clean picture; the line-art
  path rests on one 2D film (Your Name (2016)); the score's top is flat at 100 on heavy grain (differences there come
  from the penalties only). Details: the model notes (`GRAINSCORE.md` in the lab).
- The encode must have the source's resolution and frame rate, or be a crop of it; there is no scaling
  (`--dist-start` aligns differing frame numbers). HDR: PQ (HDR10) only.
- Beta tool limits: Linux with NVIDIA (HIP compile-checked) and macOS on Apple silicon (Metal); frame-accurate seeking
  is by decoding from the start of the file; AV1 film grain needs the
  [av1-fullgrain](https://github.com/divergentnn/av1-fullgrain) dav1d build (a self-contained GPU grain renderer is
  planned).

## Exactness against the Python reference

`grail check-reference` runs the 13 cases of `port_reference.json` (AV1 synthetic grain at 1080p and 4K HDR, coded
grain, synthetic grain on a grainy base, heavy grain at the cap, clean CGI, 2D line art, a window-only encode) and
compares each stage. On the RTX 3090 the port reproduces the reference to rounding:

EXACTNESS_TABLE

- The measurement denoiser is **bit-exact**: every output sample equal to the reference flowdenoise's
  (`flowdegrain2.py`) on a 1080p clip (24 frames) and a 4K HDR active picture (16 frames, flow scale 2). The kernels are
  the reference's own, compiled with the same fast-math flag; the CuPy element-wise steps are ported with the same float
  operations and order statistics.
- The remaining |dQ| (at most 0.0004) comes from the line-art peak: the reference averages the top 0.1 % of the
  ringing values in float32, the port in float64.
- Plain SSIMULACRA2 follows FFVship: each file's chroma siting (top-left on most UHD HDR sources, left when
  unspecified). The reference's denoised pairs, and its fast-mode plain pair, used "left" on both sides instead. On
  the 13 cases this is worth up to 0.73 SSIMULACRA2 points on S0 and 0.1 on Q (a top-left source against an encode
  that does not signal its siting). The port keeps both conventions, as the reference did; `--siting-left` switches
  the plain pair. Open question for the model: which convention to keep.

Fast mode matches the reference's fast-mode inputs (`variants*.jsonl`: S0, structure and features on the
subsample) on the same 13 cases with |dQ| <= 0.0004.

Hardware decoding (NVDEC) of the non-AV1 sources and encodes gives the same scores as software decoding (13 cases,
identical).

WHOLESET_LINE

### On Apple GPUs (Metal)

The Metal backend is not bit-exact: other compilers and fast-math functions (flowdenoise is compiled with fast math,
as on CUDA), no float64 on Apple GPUs (sums across thread groups are float partials added in double on the host, as
the CUDA kernels add them with double atomics; the float64 Sobel accumulation is compensated float-float), SIMD-group
reductions in another order, and libvship's Vulkan build on MoltenVK instead of its CUDA build. The 13 cases of
`port_reference.json` on an M1 Max (`grail check-reference`, software decoding; VideoToolbox gives the same scores):

| stage | worst error over the 13 cases (Metal) |
|---|---|
| plain SSIMULACRA2, window mean | 0.024 |
| plain SSIMULACRA2, worst frame (reference rounded to 0.001) | 0.046 |
| active picture, mismatches | 0 |
| SSIMULACRA2(Ds, Dc), window mean | 0.016 |
| SSIMULACRA2(Ds, Dc), worst frame | 1.03 |
| SSIMULACRA2(Ds, grain-free decode), window mean | 0.014 |
| SSIMULACRA2(Ds, grain-free decode), worst frame | 0.022 |
| source grain rms sigma_g | 9.4e-05 |
| band powers per luma bin (relative) | 0.0080 |
| flat pixels per luma bin (relative) | 7.7e-05 |
| chroma residual power (relative) | 0.0031 |
| stick / stickc correlations | 0.0027 |
| texture energy ratio | 3.4e-07 |
| grainy-base capture ratio R | 6.2e-05 |
| structure path, mismatches | 0 |
| structure term | 0.016 |
| grain weight g | 1.2e-04 |
| weighted penalties (points) | 0.015 |
| line-art confidence a | 0 |
| line-art ringing peak, worst frame | 5.3e-05 |
| line-art penalty p_L | 1.1e-05 |
| GRAILS | 0.017 |

| case | reference Q | Metal Q | dQ |
|---|---|---|---|
| av1_synthetic_1080p | 67.822 | 67.827 | +0.0043 |
| av1_synthetic_1080p | 55.924 | 55.923 | -0.0017 |
| av1_synthetic_4k_hdr | 50.420 | 50.433 | +0.0131 |
| av1_synthetic_4k_hdr | 45.544 | 45.557 | +0.0129 |
| coded_grain | 100.000 | 100.000 | +0.0000 |
| coded_grain | 87.486 | 87.503 | +0.0171 |
| grainy_base_av1 | 67.436 | 67.431 | -0.0055 |
| heavy_grain_capped | 93.364 | 93.363 | -0.0009 |
| clean_cgi | 99.679 | 99.678 | -0.0006 |
| clean_cgi | 87.591 | 87.593 | +0.0021 |
| line_art | 63.520 | 63.526 | +0.0054 |
| line_art | 6.632 | 6.640 | +0.0076 |
| window_only_candidate | 74.053 | 74.062 | +0.0097 |

- The largest part of |dQ| is plain SSIMULACRA2 (libvship on MoltenVK against libvship on CUDA): it enters Q directly
  where the source is not grainy. Everything from the denoiser on adds less than 0.02 to any case.
- The one large per-frame value, 1.03 on SSIMULACRA2(Ds, Dc), is a single frame of a clean CGI case where the
  fast-math motion search picks another vector; its window mean is off by 0.013.
- Runs are deterministic (the same build gives the same scores to the last digit).
- `check-reference` keeps the CUDA port's tolerances: on Metal it flags the two per-frame SSIMULACRA2 rows above
  ("OVER"); every window mean and Q are within them.

## Speed

RTX 3090, the Python reference's benchmark (`bench_grail.py`; this repository's `tools/bench.py` runs the same):
one AV1 film-grain encode against its source, 88 frames of the whole active picture (1080p: Black Swan (2010),
1920x792, 8-bit source; 4K: Inglourious Basterds (2009), 3840x1592, 10-bit HDR source), full and `--fast`.
The whole process runs on 4 cores (8-11), nice 19; frames/s = 88 / time inside `grail score` (decode, denoise,
metrics, features, line art; GPU initialization and process start excluded, as in the reference's numbers). Decode
share = time the pipeline waited for decoded frames (the reference's active-picture scan, which needs every source
frame first, plus stalls), over the total. Python numbers: GRAINSCORE.md, "Flavours" (idle machine).

SPEED_SECTION

Where the time goes (4K full, software decode: 11.6 s): the source's active-picture scan 3.6 s (every source frame is
decoded before anything else can start, since the exact active picture needs all of them; the candidate decoders run
meanwhile), the two measurement denoises with the remaining decoding 4.7 s, grain features 2.7 s (SSIMULACRA2 runs
on helper threads at the same time and adds 0.2 s), line art 0.3 s. Fast mode also skips the denoiser's output for
frames it does not score (their motion pyramid is still built for the neighbours); that changes no score. Next steps
for speed: start the denoise before the scan ends (provisional active picture, checked at the end), pinned host
memory for libvship, more kernel fusion in the features, and one pipeline stage per CUDA stream.

### Apple M1 Max (Metal)

Same clips and protocol (`tools/bench_macos.py`, the pack's `bench_clips.json`): 88 frames of the whole active
picture, full and `--fast`, frames/s = 88 / time inside `grail score`. macOS cannot pin a process to cores; the runs
were nice 19 on a laptop in use (desktop apps open, a VM and an Android emulator idle). 1080p numbers are stable to a
few percent; 4K runs vary more from one run to the next (thermal state, other GPU users), so the range over the
session is given after the typical value (runs while the VM was busy, up to 40 % slower, left out).

| picture | mode | decode | M1 Max frames/s | RTX 3090 frames/s | decode share (M1 Max) |
|---|---|---|---|---|---|
| 1080p | full | software | 14.7 | 24.7 | 15 % |
| 1080p | fast | software | 31.8 (27-33) | 41.7 | 34 % |
| 4K | full | software | 3.7 (3.0-3.7) | 7.6 | 18 % |
| 4K | fast | software | 8.4 (7.3-8.4) | 13.5 | 40 % |
| 4K | full | VideoToolbox ref | 3.9 (3.0-4.7) | 8.6 (NVDEC) | 6 % |
| 4K | fast | VideoToolbox ref | 11.4 (7.2-11.7) | 16.9 (NVDEC) | 17 % |

The scores are those of the RTX 3090 runs to within 0.02 (1080p 66.87 / 44.17, 4K 78.28 / 77.48).
Where the time goes (1080p full): the host waits 4.3 of 6.0 s for the GPU (encoding the 46,000 kernel launches takes
0.06 s). Of the GPU time, the scipy-style filters of the grain features are the largest part, then the motion search
and the warps of the denoiser. libvship's SSIMULACRA2 (three pairs per frame, about 12 ms a call on MoltenVK, latency
rather than throughput) runs on worker threads and costs about a quarter of the run through GPU contention: without it
the same run takes 4.8 s (18.3 frames/s).

**Next to an SVT-AV1 encode** (SvtAv1EncApp 4.2 `--preset 6 --crf 30`, 480 frames of a 1080p 10-bit source decoded by
ffmpeg into a pipe, while GRAILS scores the 1080p clip in a loop):

| | SVT-AV1 frames/s | GRAILS frames/s |
|---|---|---|
| alone | 41.1 | 14.7 full / 31.8 fast |
| with GRAILS full (nice 10) | 17.9 | 7.9-9.5 |
| with GRAILS fast (nice 10) | 22.5 | 16-26 |
| with GRAILS full, `taskpolicy -c utility --threads 2` | 34.9 | 9.7 |
| with GRAILS fast, `taskpolicy -c utility --threads 2` | 41.2 | 1.2 while the encode runs |
| with GRAILS, `taskpolicy -b` (background) | 43.7 | 1.8 even alone |

Utility QoS is the useful setting for full mode: the encoder keeps 85 % of its speed and GRAILS two thirds of its own.
Fast mode is decode-bound and is starved under utility QoS while the encoder runs; schedule it between encodes or run
it at normal priority.

## Third-party components and licences

- **libvship** (SSIMULACRA2; the Vship project, the library under FFVship): "MIT NON-AI License", the MIT licence plus
  restrictions on using the software or its code in machine-learning training and datasets. It is not an
  OSI-approved licence. grail does not include libvship; it loads it at run time, and users must accept its terms
  separately.
- **flowdenoise** (the measurement denoiser kernels, `kernels/flowdegrain2.cu`): from
  [flowdenoise](https://github.com/divergentnn/flowdenoise); same owner as this repository.
- **FFmpeg** and **dav1d** are run as external programs / libraries (not included). The
  [av1-fullgrain](https://github.com/divergentnn/av1-fullgrain) build of libdav1d is BSD-2-Clause (as dav1d).
- **MoltenVK** and the **Vulkan loader** (Apache-2.0): libvship's Vulkan build runs on them on macOS; run-time
  dependencies of libvship, not of grail. **Slang** (`slangc`, Apache-2.0 with LLVM exception): build tool for two of
  libvship's shaders in `tools/build_libvship_macos.sh`.
- Rust crates: anyhow, clap, libloading, serde, serde_json (MIT / Apache-2.0); on macOS objc2, objc2-foundation (MIT)
  and objc2-metal (MIT / Apache-2.0 / Zlib), the Metal bindings.

