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

With no grain above the threshold (`sigma_g <= 0.25`, so `g = 0`) and no line art (`a = 0`), GRAILS **is**
SSIMULACRA2, exactly. In between, the score moves smoothly toward the grain-aware form. `Q <= 100` always; it reads
in SSIMULACRA2 points.

Outputs: the GRAILS score; its parts (plain SSIMULACRA2, structure, the grain term `-sum w p`, the line-art term);
the structure path (`synth` or `coded`); `g` and `sigma_g`; every penalty and raw feature; per-frame values with
`--json`; the flavour, the mode and the GRAILS model version (`model_version`). A separate, uncalibrated
"standard-decoder seed view" rates each frame's AV1 grain seed for players that tile the grain: clumps, voids and
blotches in the grain patch it makes, and stripe-degenerate seeds
([example](examples.md#av1-grain-seeds-in-standard-players-black-swan-2010)); it never enters the score. Model 0.2.2
revised it: its table no longer counts how much a seed's patch repeats (the viewer minds clumps and holes, not
repetition) and no longer depends on the frame height (a seed that makes striped patches at some heights is checked
against the actual height when the encode is scored). The JSON report (`std_decoder`) gives `seed_frames`,
`seed_pct_mean`, `seed_pct_p90`, `seed_pct_worst` (0 best .. 100 worst), `seed_bad_share` (frames worse than 90),
`seed_degenerate`, and, informational only and summarised nowhere, `info.template_overlap_share` (frames whose patch
largely repeats one of the previous three frames').

## Flavour naming rule

GRAIL is the method; a flavour is GRAIL on one base metric, named by a letter after it: **GRAILS** = SSIMULACRA2
base. A score is always written with its flavour letter ("GRAILS 87.3", never "GRAIL 87.3"), and with the mode when
it is not the full mode ("GRAILS 87.3 (fast)"). Scores of different flavours are on different scales and must not
be compared. Only GRAILS is released: the composite was also fitted on XPSNR-Y, VMAF-NEG, CVVDP and Butteraugli, and
none agrees clearly better with the verdicts (tables below). A Butteraugli flavour (likely "GRAILB") is the first
candidate for a second release once more verdicts exist; the letter keeps the two apart.

## Version numbers

- **The model.** Every score carries the version of the model that produced it: "GRAILS 87.49 (full mode, GRAILS
  model 0.2.2, grail 0.1.0-beta.2)"; `model_version` in the JSON report; `version` in `crates/grail/data/weights.json`.
  The current model is **0.2.2**: the scores of 0.2.1 = the lab's v2.1 (calibration of 2026-10-12: 574 blind
  showings, 13 films, one viewer), with a revised standard-decoder seed view (below). The next model, fitted in the
  lab as v2.2, will be 0.3.0.
- **The rule.** GRAILS stays below 1.0 while it is in beta, and below 1.0 any change that moves scores bumps the minor
  number (0.2 -> 0.3); a change that leaves every score alone (0.2.1 -> 0.2.2: only the separate seed view) bumps the
  last number. Scores are comparable only within one minor version. 1.0 comes when calibration from other viewers is
  in and the scores are stable.
- **The tool** (the `grail` crate, 0.1.0-beta.2) has its own version. A tool change that is not a model change must
  not move scores: on CUDA a refactor gives identical scores, and other backends stay within `check-reference`'s
  tolerances.

## Limits

- **One reviewer.** The weights encode one person's tolerance (574 blind showings, 13 films). More reviewers are
  needed before a public pass threshold can be stated; until then, use GRAILS to compare encodes of the same source,
  not as an absolute grade.
- **Grainy 2D animation.** The line-art detector looks for ink strokes over flat colour; on cel animation with film
  grain (Spirited Away (2001) in the blind tests) it stays off (`a = 0`), and the grain branch (`g = 0.4` there) can
  prefer an encode that keeps the grain while it rings next to the lines. Riding Haku (a later blind round, not in
  the fit): x265 slow, failed, GRAILS 75.3 / SSIMULACRA2 78.85; SVT-AV1 4.2.0 preset 2, passed, GRAILS 67.6 /
  SSIMULACRA2 78.89. On grainy animation, check plain SSIMULACRA2 and look.
- **Ranking, not calibrated probabilities.** Held out by film, with everything refitted in each fold, GRAILS ranks
  encodes clearly better than plain SSIMULACRA2 (cross-scene concordance 0.680 against 0.546, AUC 0.712 against
  0.574), but as a predictor of the pass / borderline / fail verdict it is not clearly better: plain SSIMULACRA2's
  held-out log loss is +0.034 [-0.024, +0.079] against GRAILS's (section "How well GRAIL agrees with viewers").
- **A fixed target is not stable across scenes.** The per-scene pass line still spreads by about 14-16 points (24-28
  CRF steps). Per-scene minimums work; one film-wide target does not.
- **Heavy grain: prefer full mode.** Fast mode skips the denoise of AV1 synthetic-grain encodes; on heavy, coarse
  grain the denoiser captures only part of the source's grain, so fast mode can be up to 20 points off. Use full mode
  or `--fast --denoise-candidate` there.
- The structure term is distance to the measurement denoiser's output, not to the true clean picture; the line-art
  path rests on one 2D film (Your Name (2016)); the score's top is flat at 100 on heavy grain (differences there come
  from the penalties only). Details: the model notes (`GRAINSCORE.md` in the lab).
- The encode must have the source's resolution and frame rate, or be a crop of it; there is no scaling (`--dist-start`
  aligns differing frame numbers). Inputs are checked before scoring, and rejected with a message: frame sizes must be
  even (4:2:0), windows and their origins even; pixel format 8- or 10-bit YUV (4:2:2 and 4:4:4 are converted to 4:2:0 by
  ffmpeg, as the reference did; RGB, grey and deeper formats are rejected); limited range only (full-range video, `pc` /
  `yuvj`, is never accepted); SDR read as BT.709 (matrix and primaries BT.709 or unspecified; BT.709 / BT.601 transfer)
  and HDR as HDR10 (PQ, BT.2020). Other matrices, primaries or transfers (HLG, BT.601 matrices, BT.2020 SDR) are
  rejected unless `--colour sdr|pq` says how to read the file; source and encode must agree. HDR: PQ (HDR10) only.
- Beta tool limits: Linux with NVIDIA (HIP compile-checked) and macOS on Apple silicon (Metal); frame-accurate seeking
  is by decoding from the start of the file; AV1 film grain needs the
  [av1-fullgrain](https://github.com/divergentnn/av1-fullgrain) dav1d build (a self-contained GPU grain renderer is
  planned).

## Exactness against the Python reference

`grail check-reference` runs the 13 cases of `port_reference.json` (AV1 synthetic grain at 1080p and 4K HDR, coded
grain, synthetic grain on a grainy base, heavy grain at the cap, clean CGI, 2D line art, a window-only encode) and
compares each stage. On the RTX 3090 the port reproduces the reference to rounding:

| stage | worst error over the 13 cases |
|---|---|
| plain SSIMULACRA2, window mean | 1.4e-14 |
| plain SSIMULACRA2, worst frame (reference rounded to 0.001) | 5.0e-04 |
| active picture, mismatches | 0 |
| SSIMULACRA2(Ds, Dc), window mean | 1.4e-14 |
| SSIMULACRA2(Ds, Dc), worst frame | 5.0e-04 |
| SSIMULACRA2(Ds, grain-free decode), window mean | 7.1e-15 |
| SSIMULACRA2(Ds, grain-free decode), worst frame | 5.0e-04 |
| source grain rms sigma_g | 9.0e-09 |
| band powers per luma bin (relative) | 2.4e-07 |
| flat pixels per luma bin (relative) | 0.0000 |
| chroma residual power (relative) | 4.9e-08 |
| stick / stickc correlations | 3.7e-08 |
| texture energy ratio | 1.2e-08 |
| grainy-base capture ratio R | 2.9e-06 |
| structure path, mismatches | 0 |
| structure term | 1.9e-07 |
| grain weight g | 4.2e-09 |
| weighted penalties (points) | 1.1e-06 |
| line-art confidence a | 0.0000 |
| line-art ringing peak, worst frame | 5.3e-05 |
| line-art penalty p_L | 1.1e-05 |
| GRAILS | 3.8e-04 |

| case | reference Q | port Q | dQ (full) | dQ (fast, vs the reference's fast inputs) |
|---|---|---|---|---|
| av1_synthetic_1080p | 67.822 | 67.822 | 0.0000 | 0.0000 |
| av1_synthetic_1080p | 55.924 | 55.924 | 0.0000 | 0.0000 |
| av1_synthetic_4k_hdr | 50.420 | 50.420 | 0.0000 | 0.0000 |
| av1_synthetic_4k_hdr | 45.544 | 45.544 | 0.0000 | 0.0000 |
| coded_grain | 100.000 | 100.000 | 0.0000 | 0.0000 |
| coded_grain | 87.486 | 87.486 | 0.0000 | 0.0000 |
| grainy_base_av1 | 67.436 | 67.436 | 0.0000 | 0.0000 |
| heavy_grain_capped | 93.364 | 93.364 | 0.0000 | 0.0000 |
| clean_cgi | 99.679 | 99.679 | 0.0000 | 0.0000 |
| clean_cgi | 87.591 | 87.591 | 0.0000 | 0.0000 |
| line_art | 63.520 | 63.520 | -0.0004 | -0.0004 |
| line_art | 6.632 | 6.632 | -0.0002 | -0.0001 |
| window_only_candidate | 74.053 | 74.053 | 0.0000 | 0.0000 |

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

Two runs of the same CUDA build agree to about 1e-7 in Q (2.6e-8 on a 1080p AV1 case), not to the last bit: the
grain-feature sums are added across thread blocks with float64 atomics, in whatever order the blocks finish. The
measurement denoiser and SSIMULACRA2 are bit-reproducible; on clean content (g = 0, a = 0) Q is exactly S0 anyway.

Whole-set check (PORT_SPEC gate: Spearman >= 0.995): every 4th (144) of the 574 judged showings (13 films, full mode): Spearman between the port's and the reference's Q 0.999993; |dQ| median 1.6e-07, max 0.0004; 0 failed to run.

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
- Under the backend acceptance policy below, these Metal numbers pass: every window mean and Q are within the
  reference tolerances, and the two per-frame SSIMULACRA2 rows (0.046 and 1.03) within the other-backend ones.

### Backend acceptance

`grail check-reference` passes only if every case ran, no value is missing, non-finite or of another length than
the reference's, the standard-decoder seed view agrees, Q <= 100 on every case, Q equals S0 to the last bit on every
clean case (g = 0 and a = 0), the Spearman correlation between the port's and the reference's Q is at least 0.995
(three or more cases), and every stage is within its tolerance; otherwise it exits non-zero.

The tolerances depend on the backend. CUDA is the reference platform: its kernels are the Python reference's own and
its denoiser is bit-exact, so it keeps the tight per-frame tolerances (plain SSIMULACRA2 0.01 per frame; the denoised
pairs 1.0). Every other backend (Metal, HIP, a future CPU backend) must meet the same window-level tolerances and the
same |dQ| <= 0.1, but its per-frame SSIMULACRA2 rows may be off by up to 0.1 (plain pair) and 1.5 (denoised pairs):
another compiler's fast-math functions can move one frame's motion search (Metal: 1.03 on one frame of a clean CGI
case, 0.013 on its window mean), which the window mean and Q absorb. A backend is accepted when `check-reference`
passes under its tolerances on all 13 cases; the other-backend tolerances are not a licence for drift in the window
means.

### Compatibility quirks (GRAILS 0.2.x)

Kept on purpose, because changing them would change scores:

- `texloss` is 0, not infinite, when the texture energy ratio is exactly 0 (the encode has no energy at all in the
  source's textured tiles): the Python reference reads `tex_energy_ratio or 1` (evaluate.py), while the formula in
  PORT_SPEC.md, `max(0, -ln(ratio))`, would be infinite there. The port follows the reference.
- Plain SSIMULACRA2 uses each file's chroma siting, the denoised pairs "left" (section above).

## Speed

RTX 3090, the Python reference's benchmark (`bench_grail.py`; this repository's `tools/bench.py` runs the same):
one AV1 film-grain encode against its source, 88 frames of the whole active picture (1080p: Black Swan (2010),
1920x792, 8-bit source; 4K: Inglourious Basterds (2009), 3840x1592, 10-bit HDR source), full and `--fast`.
The whole process runs on 4 cores (8-11), nice 19; frames/s = 88 / time inside `grail score` (decode, denoise,
metrics, features, line art; GPU initialization and process start excluded, as in the reference's numbers). Decode
share = time the pipeline waited for decoded frames (the reference's active-picture scan, which needs every source
frame first, plus stalls), over the total. Python numbers: GRAINSCORE.md, "Flavours" (idle machine).

| picture | mode | decode | Rust beta frames/s | Python reference frames/s | speed-up | decode share (Rust) |
|---|---|---|---|---|---|---|
| 1080p | full | software | 24.7 | 13.8 | 1.8x | 34 % |
| 1080p | fast | software | 41.7 | 22.4 | 1.9x | 57 % |
| 4K | full | software | 7.6 | 4.8 | 1.6x | 31 % |
| 4K | fast | software | 13.5 | 7.5 | 1.8x | 55 % |
| 4K | full | NVDEC ref | 8.6 | 5.3 | 1.6x | 21 % |
| 4K | fast | NVDEC ref | 16.9 | 8.8 | 1.9x | 43 % |

1-minute load at start 2.84, at end 5.35.

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

## Decoding and inputs

- Decoding runs through ffmpeg with `-fps_mode passthrough` (every decoded frame once, in order: no frame duplicated
  or dropped by a frame-rate guess). Each decoder's delivered frames are counted against its range (`decoded` in the
  JSON report: delivered, expected); a stream with fewer frames than packets gets a warning (`warnings`), a scored
  frame that is missing stops the score.
- AV1 film grain is read from the frame headers of the whole stream (av1-fullgrain's parser, vendored in
  `crates/grail/src/av1/`, no decoding): an encode is a synthetic-grain one if any shown frame applies grain
  (`apply_grain = 1`), the reference's rule. A sequence that allows grain but where no frame applies it is scored as
  coded (with a warning). If the stream cannot be inspected (ffmpeg error, parser error, no sequence header, no
  frames) the score stops; it is never read as "no grain". The grain seeds of the standard-decoder seed view come from
  the same parse.
- An AV1 *source* whose frames apply grain is decoded with its grain rendered full-frame by the pinned av1-fullgrain
  build, like an encode (deterministic, whatever the system libdav1d does); without `--fullgrain-lib` it is refused.
- Every decode starts from an environment without `DAV1D_GRAIN*` variables (av1-fullgrain reads `DAV1D_GRAIN_MODE`,
  `_MODE_FILE`, `_FORCE`, `_TEMPLATES`, `_WARMUP`, `_BANDS`, `_STATS`, `_SIMD`); the full-frame decode then sets
  only `DAV1D_GRAIN_MODE=full` and `DAV1D_GRAIN_FORCE=1`, the grain-free decode only `DAV1D_GRAIN_FORCE=0`.
- Timings in the JSON report: `ssimulacra2_wait` is the time the pipeline waited for the SSIMULACRA2 workers (full
  queues and the final join), not their work; `ssimulacra2_calls` is the wall time inside libvship's calls summed over
  the workers (it runs alongside the other stages).

## Third-party components and licences

- **libvship** (SSIMULACRA2; the Vship project, the library under FFVship): "MIT NON-AI License", the MIT licence plus
  restrictions on using the software or its code in machine-learning training and datasets. It is not an
  OSI-approved licence. grail does not include libvship; it loads it at run time, and users must accept its terms
  separately.
- **flowdenoise** (the measurement denoiser kernels, `kernels/flowdegrain2.cu`): from
  [flowdenoise](https://github.com/divergentnn/flowdenoise); same owner as this repository.
- **FFmpeg** and **dav1d** are run as external programs / libraries (not included). The
  [av1-fullgrain](https://github.com/divergentnn/av1-fullgrain) build of libdav1d is BSD-2-Clause (as dav1d).
- **av1-fullgrain's AV1 header parser** (`crates/grail/src/av1/obu.rs`, `params.rs`, BSD-2-Clause; licence text in
  `crates/grail/src/av1/LICENSE-av1-fullgrain`): vendored, for the frame-level film grain detection.
- **MoltenVK** and the **Vulkan loader** (Apache-2.0): libvship's Vulkan build runs on them on macOS; run-time
  dependencies of libvship, not of grail. **Slang** (`slangc`, Apache-2.0 with LLVM exception): build tool for two of
  libvship's shaders in `tools/build_libvship_macos.sh`.
- Rust crates: anyhow, clap, libloading, serde, serde_json (MIT / Apache-2.0); on macOS objc2, objc2-foundation (MIT)
  and objc2-metal (MIT / Apache-2.0 / Zlib), the Metal bindings.


## How well GRAIL agrees with viewers

*GRAILS model 0.2.2 (the scores of 0.2.1, the lab's v2.1), nested held-out evaluation of 2026-10-12. Cite as: GRAILS
0.2.2 (2026-10-12), 574 blind showings (616 with the line-art round), one reviewer, held out by film.*

### Dataset

One reviewer judged every encode blind, in review rounds held 2026-10-10 to 2026-10-12: each showing is one
encode of one scene, played 1:1 (a 1920x1080 window of 4K sources) next to its source, with a verdict of pass /
borderline / fail. Encodes come from several encoders and settings (SVT-AV1 with synthetic film grain at CRF ladders
and grain strengths, x265, x264, SVT-AV1 without grain synthesis, near-lossless controls).

616 judged showings, 15 films, 54 scenes, 31 blind rounds; verdicts pass 275 / borderline 99 / fail 242.

| film | scenes | showings |
|---|---|---|
| Your Name (2016) | 5 | 135 |
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
| Spirited Away (2001) | 2 | 14 |
| Chainsaw Man - The Movie: Reze Arc (2025) | 2 | 14 |
| The Wild Robot (2024) | 2 | 9 |

The flavour comparison below uses the 574 showings of the first 30 rounds (13 films, 48 scenes); the last round
(42 showings, two more line-art films) enters in "Line art".

### How it is held out

Every number below is **held out by film, nested**: for each film in turn, the whole system is fitted on the other
films only (the structure-path offset, the normalisation scale and grain-dependent lift, the penalty weights, the
line-art parameters, and the ordinal pass / borderline / fail mapping on that system's own training scores), and that
one system scores the held-out film. Log loss: every showing under its own fold's mapping (lower is better; knowing
only the verdict shares gives 1.023). Concordance and AUC pool the held-out scores of all folds, so a cross-film
pair compares scores from two different fold systems. Plain metrics learn only the mapping, fitted on the other
films. Columns: held-out log loss, cross-scene concordance (does a higher score mean a better verdict for showings of
DIFFERENT scenes: the property a quality target needs), within-scene concordance (ranking versions of one scene, shown
side by side in one round), and AUC of pass vs not pass. "Fast" = the `--fast` mode (section below), fitted the same way.

Earlier versions of these tables (before 2026-10-12) fitted the path offset, the scale and the mapping partly on all
showings, so the held-out film had leaked in. The nested numbers replace them. GRAILS full: log loss 0.975 -> 0.995,
cross-scene 0.682 -> 0.680, within-scene 0.806 -> 0.806, AUC 0.714 -> 0.712; the other composites lose up to 0.06 in
log loss; plain metrics are unchanged (1.029 / 0.546 / 0.757 / 0.574 for SSIMULACRA2). Subset rows now use the
whole-set mapping for plain metrics too.

### Agreement with the verdicts

Released: **GRAILS**, the SSIMULACRA2-based GRAIL, in full and `--fast` mode. It equals SSIMULACRA2 exactly on
clean, grain-free content that is not line art, so it reads in SSIMULACRA2 points. For comparison, the same composite
was fitted on four other base metrics (XPSNR-Y, VMAF-NEG, CVVDP, Butteraugli 3-norm); none agrees clearly better with
the verdicts, so they are not released (each would read in its own base's units: dB, VMAF points, JOD, distance).
Plain base metrics are listed last.

**All showings** (574 showings)

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | 0.995 | 0.680 | 0.806 | 0.712 |
| GRAILS fast | 0.984 | 0.688 | 0.817 | 0.716 |
| XPSNR base full | 1.072 | 0.623 | 0.833 | 0.650 |
| XPSNR base fast | 1.066 | 0.614 | 0.822 | 0.642 |
| VMAF-NEG base full | 1.060 | 0.645 | 0.814 | 0.667 |
| VMAF-NEG base fast | 1.061 | 0.652 | 0.839 | 0.674 |
| CVVDP base full | 1.073 | 0.627 | 0.779 | 0.643 |
| CVVDP base fast | 1.056 | 0.613 | 0.765 | 0.624 |
| Butteraugli base full | 0.987 | 0.686 | 0.801 | 0.714 |
| Butteraugli base fast | 0.978 | 0.693 | 0.803 | 0.713 |
| plain ssimu2 | 1.029 | 0.546 | 0.757 | 0.574 |
| plain xpsnr | 1.036 | 0.508 | 0.816 | 0.531 |
| plain vmaf_neg | 1.028 | 0.553 | 0.814 | 0.574 |
| plain cvvdp | 1.034 | 0.516 | 0.719 | 0.532 |
| plain butter3 | 1.028 | 0.542 | 0.719 | 0.571 |

**Clean (source grain sigma below 0.3)** (187 showings)

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | 1.109 | 0.657 | 0.874 | 0.754 |
| GRAILS fast | 1.113 | 0.657 | 0.874 | 0.752 |
| XPSNR base full | 1.214 | 0.637 | 0.906 | 0.721 |
| XPSNR base fast | 1.262 | 0.659 | 0.890 | 0.743 |
| VMAF-NEG base full | 1.230 | 0.639 | 0.898 | 0.716 |
| VMAF-NEG base fast | 1.262 | 0.665 | 0.913 | 0.758 |
| CVVDP base full | 1.240 | 0.609 | 0.764 | 0.685 |
| CVVDP base fast | 1.212 | 0.617 | 0.748 | 0.684 |
| Butteraugli base full | 1.096 | 0.636 | 0.850 | 0.744 |
| Butteraugli base fast | 1.095 | 0.642 | 0.850 | 0.747 |
| plain ssimu2 | 1.057 | 0.658 | 0.874 | 0.754 |
| plain xpsnr | 1.090 | 0.636 | 0.906 | 0.720 |
| plain vmaf_neg | 1.079 | 0.640 | 0.898 | 0.717 |
| plain cvvdp | 1.050 | 0.612 | 0.764 | 0.689 |
| plain butter3 | 1.051 | 0.638 | 0.850 | 0.746 |

**Grainy (source grain sigma 0.8 or more)** (109 showings)

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | 0.903 | 0.672 | 0.823 | 0.695 |
| GRAILS fast | 0.851 | 0.694 | 0.861 | 0.725 |
| XPSNR base full | 1.084 | 0.571 | 0.873 | 0.657 |
| XPSNR base fast | 0.989 | 0.596 | 0.873 | 0.664 |
| VMAF-NEG base full | 0.918 | 0.673 | 0.696 | 0.668 |
| VMAF-NEG base fast | 0.883 | 0.692 | 0.848 | 0.701 |
| CVVDP base full | 0.929 | 0.668 | 0.785 | 0.675 |
| CVVDP base fast | 0.912 | 0.680 | 0.848 | 0.698 |
| Butteraugli base full | 0.859 | 0.712 | 0.848 | 0.752 |
| Butteraugli base fast | 0.830 | 0.725 | 0.873 | 0.761 |
| plain ssimu2 | 0.968 | 0.641 | 0.772 | 0.682 |
| plain xpsnr | 0.970 | 0.600 | 0.861 | 0.667 |
| plain vmaf_neg | 0.961 | 0.691 | 0.873 | 0.732 |
| plain cvvdp | 1.007 | 0.641 | 0.747 | 0.670 |
| plain butter3 | 0.968 | 0.630 | 0.734 | 0.680 |

**Your Name (2016), line art, without the line-art path** (121 showings)

| score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|
| GRAILS full | 1.074 | 0.903 | 0.860 | 0.933 |
| GRAILS fast | 1.097 | 0.905 | 0.860 | 0.933 |
| XPSNR base full | 1.304 | 0.841 | 0.895 | 0.884 |
| XPSNR base fast | 1.383 | 0.920 | 0.907 | 0.939 |
| VMAF-NEG base full | 1.136 | 0.703 | 0.895 | 0.799 |
| VMAF-NEG base fast | 1.193 | 0.852 | 0.907 | 0.917 |
| CVVDP base full | 1.192 | 0.695 | 0.791 | 0.770 |
| CVVDP base fast | 1.190 | 0.732 | 0.756 | 0.782 |
| Butteraugli base full | 1.057 | 0.834 | 0.895 | 0.924 |
| Butteraugli base fast | 1.081 | 0.845 | 0.895 | 0.927 |
| plain ssimu2 | 1.085 | 0.903 | 0.860 | 0.933 |
| plain xpsnr | 1.144 | 0.841 | 0.895 | 0.884 |
| plain vmaf_neg | 1.119 | 0.703 | 0.895 | 0.799 |
| plain cvvdp | 1.072 | 0.695 | 0.791 | 0.770 |
| plain butter3 | 1.077 | 0.834 | 0.895 | 0.924 |

The Your Name (2016) rows are without the line-art path: on these 574 showings no other line-art film is left to
learn it from when Your Name is held out. With the line-art path, see "Line art" below.

Differences against GRAILS (full), 95 % intervals from a paired bootstrap over the 48 scenes (log loss: lower is
better; concordance and AUC: higher is better). The bootstrap resamples scenes with every fold's fitted system held
fixed, so the intervals leave out the uncertainty of fitting; and the concordance and AUC differences compare pooled
held-out scores from different fold systems.

| score | log loss difference | cross-scene concordance difference | AUC difference |
|---|---|---|---|
| GRAILS fast | -0.011 [-0.038, +0.010] | [-0.021, +0.036] | [-0.026, +0.035] |
| XPSNR base full | +0.077 [-0.008, +0.139] | [-0.109, +0.004] | [-0.110, -0.001] |
| XPSNR base fast | +0.071 [-0.017, +0.148] | [-0.125, +0.004] | [-0.125, -0.005] |
| VMAF-NEG base full | +0.065 [+0.005, +0.131] | [-0.088, +0.031] | [-0.097, +0.029] |
| VMAF-NEG base fast | +0.066 [+0.004, +0.134] | [-0.094, +0.040] | [-0.104, +0.035] |
| CVVDP base full | +0.078 [+0.031, +0.124] | [-0.113, +0.000] | [-0.126, -0.013] |
| CVVDP base fast | +0.061 [+0.014, +0.100] | [-0.124, +0.002] | [-0.142, -0.009] |
| Butteraugli base full | -0.009 [-0.033, +0.015] | [-0.018, +0.024] | [-0.028, +0.026] |
| Butteraugli base fast | -0.017 [-0.061, +0.021] | [-0.041, +0.060] | [-0.057, +0.059] |
| plain ssimu2 | +0.034 [-0.024, +0.079] | [-0.194, -0.081] | [-0.196, -0.076] |
| plain xpsnr | +0.041 [-0.016, +0.085] | [-0.243, -0.091] | [-0.250, -0.090] |
| plain vmaf_neg | +0.033 [-0.022, +0.075] | [-0.198, -0.048] | [-0.208, -0.044] |
| plain cvvdp | +0.039 [-0.026, +0.090] | [-0.228, -0.096] | [-0.242, -0.100] |
| plain butter3 | +0.033 [-0.025, +0.081] | [-0.201, -0.083] | [-0.206, -0.079] |

Clearly worse than GRAILS full (interval excludes zero) in cross-scene concordance: every plain metric; in AUC:
every plain metric and XPSNR base full, XPSNR base fast, CVVDP base full and CVVDP base fast; in log loss: VMAF-NEG
base full, VMAF-NEG base fast, CVVDP base full and CVVDP base fast. No plain metric is clearly worse in log loss
after the nested fix (plain SSIMULACRA2 +0.034 [-0.024, +0.079]; before it, every plain metric was): GRAILS's gain
over plain metrics is in ranking, not in calibration of the verdict levels. Nothing is clearly better than GRAILS
full.

Two later validation rounds (pairs, votes only; the score agrees when it rates the voted encode higher; models fitted
on all 574 showings, which these rounds are not part of):

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

### Line art

2D line art gets an extra term for ringing and halos next to the ink lines (detector and penalty from the source;
GRAILS only). The released design (grain model and line-art path) is evaluated here on 616 showings, refitted in
each fold: the 574 above plus one more blind round (42 showings) with Spirited Away (2001), Chainsaw Man - The
Movie: Reze Arc (2025) and Your Name, so three line-art films with 163 showings. Released GRAILS (grain model and
line-art path), nested, held out by film, against plain SSIMULACRA2 with its mapping fitted on the other films:

| showings | score | held-out log loss | cross-scene | within-scene | AUC (pass) |
|---|---|---|---|---|---|
| all 616 | GRAILS | 0.936 | 0.706 | 0.829 | 0.737 |
| all 616 | SSIMULACRA2 | 1.032 | 0.548 | 0.776 | 0.571 |
| the 574 above | GRAILS | 0.933 | 0.704 | 0.822 | 0.733 |
| the 574 above | SSIMULACRA2 | 1.027 | 0.546 | 0.757 | 0.574 |
| line art (163) | GRAILS | 0.817 | 0.830 | 0.911 | 0.887 |
| line art (163) | SSIMULACRA2 | 1.082 | 0.753 | 0.881 | 0.813 |

On the 574 showings the released system does better than the flavour table's GRAILS full (0.995 / 0.680) because the
line-art path, learned from the other two line-art films, now also applies when Your Name is held out.

Line-art log loss with the held-out unit changed (everything refitted in each fold): by film 0.817, by scene 0.787,
by review round 0.791; base rate 1.055; plain SSIMULACRA2 1.082. Line art against everything else, cross-scene:
0.739. Pair votes (which of two encodes looks closer to the source): 11 of 14 agree with the reviewer (4/4 in the
last round, each pair scored by the system that never saw its film; 6/9 and 1/1 in two later rounds, scored by the
system fitted on all 616 showings).

By content class (the same held-out scores; "clean" = source grain sigma at most 0.25; within-scene pairs in
brackets):

| content | showings | films | GRAILS log loss | SSIMULACRA2 log loss | GRAILS cross-scene | SSIMULACRA2 cross-scene | GRAILS within-scene | SSIMULACRA2 within-scene | GRAILS AUC | SSIMULACRA2 AUC |
|---|---|---|---|---|---|---|---|---|---|---|
| clean live action + CGI | 34 | 4 | 1.064 | 1.063 | 0.743 | 0.743 | 0.852 (23/27) | 0.852 (23/27) | 0.792 | 0.792 |
| grainy live action + CGI | 419 | 11 | 0.972 | 1.010 | 0.668 | 0.535 | 0.783 (198/253) | 0.711 (180/253) | 0.688 | 0.556 |
| clean line art | 135 | 1 | 0.777 | 1.074 | 0.889 | 0.844 | 0.922 (94/102) | 0.863 (88/102) | 0.926 | 0.903 |
| grainy line art | 28 | 2 | 1.009 | 1.120 | 0.695 | 0.670 | 0.879 (29/33) | 0.939 (31/33) | 0.817 | 0.744 |

**Grainy line art is GRAILS's weak spot.** On grainy 2D animation (28 showings: two scenes each of Spirited Away and
Chainsaw Man) GRAILS fits the verdicts better than plain SSIMULACRA2 across scenes (log loss 1.009 against 1.120,
AUC 0.82 against 0.74) but ranks encodes of the same scene worse: 29 of 33 within-scene pairs, against 31 of 33 for
SSIMULACRA2. The line-art detector reads the source, where grain hides the lines (on Spirited Away it does not
fire), so there GRAILS is SSIMULACRA2 plus the grain model, and the grain model credits an encode that keeps the
source's grain as coded detail. The same round's Spirited Away scenes, scored with the released GRAILS 0.2.1 (fitted
before that round), show it plainly: the failed x265 encode scored highest on one scene (75.3, against 67.6 for the
passing SVT-AV1 preset 2 encode) and level with preset 2 on the other (75.7 against 75.9), while plain SSIMULACRA2
ranked no failed encode above a passed one on either. To rank encodes of grainy 2D animation, use plain SSIMULACRA2
until a model fixes this.

Two fixes were tested for 0.3.0, nested on the same 616 showings, and not adopted: (1) damping the grain model by a
line-art detector run on the denoised source (g -> g (1 - a)): grainy line art within scene ties SSIMULACRA2
(0.939), but the detector also fires on 16 of 25 Dune (2021) showings, grainy live action drops (within
scene 0.783 -> 0.763) and line-art log loss rises (0.817 ->
0.826); (2) the same plus the grain-robust ringing measure (detector on the denoised source,
magnitudes on a PQ scale for SDR): line-art log loss 0.927. A live-action veto could not be built
from the detector's statistics: Dune's scenes reach the same ink and flat-area values as the anime scenes.

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
