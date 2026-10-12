# GRAIL: grain- and line-art-aware video quality (beta)

`grail` scores an encode against its source the way a viewer judges grainy films and 2D line art. It reports
**GRAILS**, the flavour of GRAIL built on SSIMULACRA2. This is the first Rust beta: a standalone command-line tool
that reproduces the Python reference implementation (GRAIL model v2.1, 2026-10-12) on a CUDA GPU.

```
$ grail score --ref source.mkv --dist encode.mkv --crop 486,284,1920,1080
GRAILS 87.49  (full mode, GRAIL model 2.1, grail 0.1.0-beta.1)
  SSIMULACRA2          70.42
  structure            97.21  (path coded)
  grain                -7.02
  line art              0.00  (a 0.00, p_L 1.767)
  g 0.864  sigma_g 0.672  frames 79  window [486, 284, 1920, 1080]
```

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
- `S_struct`: structure with the grain taken out. A fixed measurement denoiser (flowdegrain2, motion-compensated,
  GPU) splits source and encode into structure and residual. For AV1 encodes that carry film-grain parameters,
  `S_struct` compares the denoised source with the grain-free decode; for every other encode, it compares the two
  denoised pictures. A grain-dependent offset puts both paths on one scale, capped at 100. Blur, blocking, smear and
  ringing still count; grain counts neither for nor against.
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
`--json`; the flavour, the mode and the GRAIL model version. A separate, uncalibrated "standard-decoder seed view"
rates the AV1 grain seeds for players that tile the grain; it never enters the score.

## Flavour naming rule

GRAIL is the method; a flavour is GRAIL on one base metric, named by a letter after it: **GRAILS** = SSIMULACRA2
base. A score is always written with its flavour letter ("GRAILS 87.3", never "GRAIL 87.3"), and with the mode when
it is not the full mode ("GRAILS 87.3 (fast)"). Scores of different flavours are on different scales and must not
be compared. Only GRAILS is released: the composite was also fitted on XPSNR-Y, VMAF-NEG, CVVDP and Butteraugli, and
none agrees clearly better with the verdicts (tables below). A Butteraugli flavour (likely "GRAILB") is the first
candidate for a second release once more verdicts exist; the letter keeps the two apart.

## Install and build

Requirements (Linux; this beta is tested on an RTX 3090 with CUDA 13.4 and driver 615):

- an NVIDIA GPU and driver; the CUDA toolkit (`nvcc`, 12.x or 13.x) at build time only;
- Rust 1.80 or newer (`cargo`);
- `ffmpeg` and `ffprobe` on `PATH`, built with libdav1d (and NVDEC for hardware decoding of 4K sources);
- **libvship** 5.1 (CUDA build), the library under FFVship, for SSIMULACRA2: found as `libvship.so` on the library
  path, `/usr/local/lib/libvship.so`, or `GRAIL_VSHIP=/path/to/libvship.so`;
- for AV1 encodes with film grain: the **av1-fullgrain** build of libdav1d (full-frame grain rendering), given with
  `--fullgrain-lib DIR` or `GRAIL_FULLGRAIN_LIB=DIR` (the directory holding its `libdav1d.so.7`). Without it, grail
  refuses to score such encodes instead of scoring the standard tiled grain.

```
cargo build --release                         # kernels for sm_75, 80, 86, 89, 90, 120 (+ PTX)
GRAIL_CUDA_ARCHS=86 cargo build --release     # one GPU generation only (faster build)
NVCC=/opt/cuda/bin/nvcc cargo build --release # nvcc not on PATH (also found under $CUDA_PATH, /opt/cuda, /usr/local/cuda)
```

The binary is `target/release/grail`. The kernels are embedded; the CUDA driver (`libcuda.so.1`) and libvship are
loaded at run time.

### AMD GPUs (HIP): compile-checked, not run

The kernels are plain CUDA C++ that hipcc compiles unchanged (`kernels/portable.h` maps the one warp shuffle; hipify
only adds the HIP runtime include). The host side loads either the CUDA driver API or the HIP module API at run time
(`GRAIL_BACKEND=cuda|hip`), so one binary can carry both. Build with ROCm installed:

```
cargo build --release --no-default-features --features hip            # HIP only
cargo build --release --features hip                                   # CUDA and HIP in one binary
GRAIL_HIP_ARCHS=gfx1100,gfx1030 cargo build --release --features hip   # chosen AMD targets
```

Status: both kernel files compile with hipcc from ROCm 7.2.4 (the `rocm/dev-ubuntu-24.04:7.2.4` container,
gfx1100 and gfx90a) without warnings, and the code objects contain every kernel. Nothing has run on an AMD GPU:
exactness and speed there are unverified. Expect small score differences (another compiler and fast-math library),
and a libvship HIP build is needed too.

### CPU backend

Not in this beta. The pipeline talks only to the stage interfaces in `crates/grail-stages` (measurement denoiser,
grain features, line-art path, backend); a CPU backend implements those traits and nothing else changes.
libvship also has non-CUDA builds (HIP, Vulkan).

## Usage

```
grail score --ref SOURCE --dist ENCODE [--fast [--denoise-candidate]] [--start N --frames N]
            [--crop x,y,w,h | auto] [--json] [--fullgrain-lib DIR]
```

- `--start N --frames N`: score frames `N ..` of the source (file frame numbers; decoding starts at the beginning of
  the file, so cut long films into scenes first). `--dist-start` if the encode's frame numbers differ.
- `--crop x,y,w,h`: score this window (even coordinates, in source pixels). Default `auto`: the active picture (black
  bars removed). The denoiser always sees the whole active picture; the window is cut out afterwards. If the encode
  is the window itself (a crop encode), it is detected by its size; `--dist-crop` overrides.
- `--fast`: every 4th frame plus its successor; AV1 synthetic-grain encodes are not denoised. About 1.6x faster in
  the Python reference, same agreement with the verdicts, but up to 20 points off on heavy, coarse grain.
  `--fast --denoise-candidate` keeps the denoise (frame subsampling only): use it, or the full mode, as an encode
  target on heavy grain.
- `--json`: everything, including per-frame SSIMULACRA2, structure and line-art values and the stage timings.
- `--colour sdr|pq`: overrides the colour handling read from the source (PQ: BT.2020 + SMPTE 2084).
- `--hwdec auto|none|cuda`: NVDEC for non-AV1 streams (auto: above 2048 px width). AV1 stays on dav1d.
- `--threads N`: ffmpeg threads per decoder (up to three decoders run at once).
- `--siting-left`: plain SSIMULACRA2 with chroma siting "left" on both sides instead of each file's own siting (see
  "Exactness").

Many pairs (one GPU initialization; resumable with `--out`):

```
$ cat pairs.jsonl
{"id": "x265-crf18", "ref": "scene.mkv", "dist": "x265_crf18.mkv", "frames": 96}
{"id": "svt-crf30", "ref": "scene.mkv", "dist": "svt_crf30.mkv", "crop": [960, 540, 1920, 1080], "fast": true}
$ grail score --batch pairs.jsonl --out scores.jsonl
```

Each line takes `ref`, `dist` and optionally `id`, `start`, `dist_start`, `frames`, `crop`, `dist_crop`, `colour`,
`fast`, `denoise_candidate`; the command-line options are the defaults. Each output line holds the id, a label
("GRAILS 87.49", or "GRAILS 87.49 (fast)"), the score and its parts (the full report with `--json`); a failed pair
gets an `error` field and the batch goes on.

Other commands:

- `grail denoise IN.y4m OUT.y4m`: the measurement denoiser alone (10-bit 4:2:0 y4m; `-` for stdin / stdout).
- `grail check-reference port_reference.json`: runs the reference's test cases and compares every intermediate value
  (see below; needs the lab's media).

Environment: `GRAIL_VSHIP`, `GRAIL_FULLGRAIN_LIB`, `GRAIL_BACKEND`, `GRAIL_GPU` (device index), `GRAIL_MEM_MB`
(host memory for decoded source frames, default 4096; above it the source is decoded twice).

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
- Beta tool limits: Linux and NVIDIA only (HIP compile-checked); frame-accurate seeking is by decoding from the start
  of the file; AV1 film grain needs the av1-fullgrain dav1d build (a self-contained GPU grain renderer is planned).

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

- The measurement denoiser is **bit-exact**: every output sample equal to flowdegrain2.py's on a 1080p clip (24
  frames) and a 4K HDR active picture (16 frames, flow scale 2). The kernels are the reference's own, compiled with
  the same fast-math flag; the CuPy element-wise steps are ported with the same float operations and order statistics.
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

Whole-set check (PORT_SPEC gate: Spearman >= 0.995): every 4th of the 574 judged showings (144 showings, 13 films, full mode): Spearman between the port's and the reference's Q 0.999993; |dQ| median 1.6e-07, max 0.0004; 0 failed to run.

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

## Packaging notes

- **Linux binary.** `target/release/grail` links only glibc (`libc`, `libm`, `libgcc_s`): the kernels are embedded,
  and the CUDA driver, libvship and (for AV1 grain) the av1-fullgrain libdav1d are loaded at run time. A fully static
  musl build is not possible this way (static musl cannot `dlopen`); a release binary should be built on an old glibc
  (e.g. a manylinux-style container) with the full architecture list (the default `GRAIL_CUDA_ARCHS`).
- **CUDA toolkit and driver.** Building needs nvcc; this beta was built with CUDA 13.4. The fatbin holds SASS for
  sm_75 to sm_120 plus PTX for newer GPUs; the driver must support the toolkit's CUDA version (minor-version
  compatibility within the major version). Building with CUDA 12.x widens the range of drivers that can run it
  (12.x: drivers from the 525 series on); CUDA 13 no longer builds for GPUs older than Turing (sm_75).
- **libvship** must be installed separately (a CUDA build for NVIDIA GPUs); grail loads `libvship.so` (or
  `GRAIL_VSHIP`). Bundling it would put its licence terms (below) on the package.
- **Full-frame AV1 grain** needs the av1-fullgrain libdav1d next to the system ffmpeg (`--fullgrain-lib`). A GPU
  renderer of full-frame grain from ffmpeg's exported film-grain side data would remove this dependency; it must
  match av1-fullgrain's full mode exactly, or the scores of AV1 grain encodes change.
- **AMD (HIP).** The kernels compile with hipcc (ROCm 7.2.4, gfx1100 / gfx90a); missing: a run on an AMD GPU (scores
  against the CUDA build), a libvship HIP build, and CI for `--features hip`.
- **Windows.** Missing: driver and library names for `libloading` (`nvcuda.dll`, `vship.dll`), the av1-fullgrain
  dav1d DLL substitution (PATH order instead of `LD_LIBRARY_PATH`), and a build/test run; nvcc, ffmpeg and the Rust
  code are otherwise portable.
- **macOS.** No CUDA: needs another backend (the stage traits in `crates/grail-stages`: a Metal / Vulkan or CPU
  implementation of the denoiser, features and line art) and libvship's Vulkan build.

## Third-party components and licences

- **libvship** (SSIMULACRA2; the Vship project, the library under FFVship): "MIT NON-AI License", the MIT licence plus
  restrictions on using the software or its code in machine-learning training and datasets. It is not an
  OSI-approved licence. grail does not include libvship; it loads it at run time, and users must accept its terms
  separately.
- **flowdegrain2** (measurement denoiser kernels, `kernels/flowdegrain2.cu`): copied from the GRAIL lab
  (grain-lab/flowdegrain2); same owner as this repository.
- **FFmpeg** and **dav1d** are run as external programs / libraries (not included). The av1-fullgrain build of
  libdav1d is BSD-2-Clause (as dav1d).
- Rust crates: anyhow, clap, libloading, serde, serde_json (MIT / Apache-2.0).

## Licence

**Not chosen yet (placeholder).** The licence for this repository will be decided by the owner; until then no
licence is granted. (Cargo manifests say `UNLICENSED`.)

## Repository layout

- `kernels/`: CUDA C++ kernels (`flowdegrain2.cu`: the measurement denoiser, verbatim from the reference;
  `grail.cu`: CuPy-equivalent steps, order statistics, grain features, line art; `portable.h`: CUDA/HIP shim).
- `crates/grail-stages`: stage interfaces (for other backends).
- `crates/grail-gpu`: driver layer (CUDA driver API / HIP, loaded at run time), denoiser, features, line art.
- `crates/grail`: CLI, decoding (ffmpeg), libvship binding, the v2.1 model (`data/weights.json`), seed view
  (`data/seed_scores.npy`), reference checks.
- `tools/`: lab tools (whole-set case file, benchmark).

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
