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

EXACTNESS_TABLE

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

WHOLESET_LINE

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
