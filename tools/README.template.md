# GRAIL: grain- and line-art-aware video quality (beta)

`grail` scores an encode against its source the way a viewer judges grainy films and 2D line art. It reports
**GRAILS**, the flavour of GRAIL built on SSIMULACRA2. This is the first Rust beta: a standalone command-line tool
that reproduces the Python reference implementation (GRAIL model v2.1, 2026-10-12) on a CUDA GPU, and on Apple GPUs
through Metal (macOS).

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

### macOS (Apple silicon): Metal

The `grail-metal` crate implements the stages on Apple GPUs: the kernels ported to Metal Shading Language
(`kernels/metal/`), compiled from source when grail starts (about 0.25 s; macOS caches the result), and the host-side
orchestration of `grail-gpu`, step for step. SSIMULACRA2 is libvship's Vulkan build on MoltenVK. Tested on an M1 Max
(32-core GPU, 64 GB) with macOS 26.6 and Rust 1.97; exactness and speed below.

Requirements:

- Rust 1.80 or newer and the Xcode command-line tools (no Xcode: the kernels compile at run time);
- `brew install ffmpeg molten-vk vulkan-loader vulkan-headers meson ninja`;
- **libvship**, Vulkan build: `tools/build_libvship_macos.sh [PREFIX]` (needs `slangc`, the Slang compiler, for two
  shaders that MoltenVK cannot translate as shipped; see the script) installs `PREFIX/lib/libvship.dylib`; point
  `GRAIL_VSHIP` at it. grail turns MoltenVK's fast math off (`MVK_CONFIG_FAST_MATH_ENABLED=0`, unless the variable is
  set): with it on, SSIMULACRA2 moves by up to 0.05 per frame against the CUDA build, without it by at most 0.046 on
  the 13 cases and 0.024 on a window mean;
- for AV1 encodes with film grain, **av1-fullgrain** (dav1d-fullgrain, branch grain-variants):
  `meson setup build --buildtype=release && ninja -C build`, then `--fullgrain-lib DIR` / `GRAIL_FULLGRAIN_LIB=DIR`
  with DIR holding its `libdav1d.7.dylib` (grail puts DIR first on ffmpeg's `DYLD_LIBRARY_PATH`). If Homebrew's
  libdav1d was replaced by av1-fullgrain (its `packaging/macos/install.sh`), `$(brew --prefix dav1d)/lib` works too.

```
cargo build --release                          # grail-gpu is empty on macOS; grail uses grail-metal
export GRAIL_VSHIP=$HOME/.local/lib/libvship.dylib GRAIL_FULLGRAIN_LIB=/path/to/dav1d-fullgrain/build/src
target/release/grail score --ref source.mkv --dist encode.mkv
target/release/grail denoise in.y4m out.y4m     # the measurement denoiser, 10-bit 4:2:0 y4m ("-" for stdin / stdout)
```

- Hardware decoding: `--hwdec auto` uses VideoToolbox for non-AV1 streams wider than 2048 px (4K HEVC 10-bit: 80
  frames/s against 20 in software on the M1 Max; 1080p H.264 is faster in software, 153 against 135). The scores are
  the same as with software decoding (13 cases). AV1 stays on dav1d (no AV1 decoder in the M1).
- Next to an encoder on the same Mac: `taskpolicy -c utility grail ...` keeps most of the CPU for the encoder (numbers
  under "Speed").
- `GRAIL_METAL_PROFILE=1` prints the GPU time per kernel (each launch timed on its own, so slower overall);
  `GRAIL_METAL_PROFILE=host` the time spent encoding launches and waiting for the GPU.

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
- `--fast`: every 4th frame plus its successor; AV1 synthetic-grain encodes are not denoised. 1.7-2.0x faster
  (table under "Speed"), same agreement with the verdicts, but up to 20 points off on heavy, coarse grain.
  `--fast --denoise-candidate` keeps the denoise (frame subsampling only): use it, or the full mode, as an encode
  target on heavy grain.
- `--json`: everything, including per-frame SSIMULACRA2, structure and line-art values and the stage timings.
- `--colour sdr|pq`: overrides the colour handling read from the source (PQ: BT.2020 + SMPTE 2084).
- `--hwdec auto|none|cuda|videotoolbox`: NVDEC (Linux) or VideoToolbox (macOS) for non-AV1 streams (auto: above
  2048 px width). AV1 stays on dav1d.
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

Environment: `GRAIL_VSHIP`, `GRAIL_FULLGRAIN_LIB`, `GRAIL_BACKEND`, `GRAIL_GPU` (device index), `GRAIL_METAL_PROFILE`
(macOS), `GRAIL_MEM_MB`
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
- The encode must have the source's resolution and frame rate, or be a crop of it; there is no scaling
  (`--dist-start` aligns differing frame numbers). HDR: PQ (HDR10) only.
- Beta tool limits: Linux with NVIDIA (HIP compile-checked) and macOS on Apple silicon (Metal); frame-accurate seeking is by decoding from the start
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

### On Apple GPUs (Metal)

The Metal backend is not bit-exact: other compilers and fast-math functions (flowdegrain2 is compiled with fast math,
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
- **macOS.** `target/release/grail` links only system frameworks and libraries (Metal, Foundation, libSystem); the Metal kernels are
  embedded as source and compiled at start, libvship and the av1-fullgrain libdav1d are loaded at run time. Missing
  for a release: a universal or arm64 build signed and notarized (or a Homebrew formula), a libvship release that runs
  on MoltenVK without `tools/build_libvship_macos.sh` (the fix belongs upstream), and Intel Macs (Metal works there,
  untested; AMD / Intel GPUs have other SIMD widths than the 32 lanes the motion search assumes).

## Third-party components and licences

- **libvship** (SSIMULACRA2; the Vship project, the library under FFVship): "MIT NON-AI License", the MIT licence plus
  restrictions on using the software or its code in machine-learning training and datasets. It is not an
  OSI-approved licence. grail does not include libvship; it loads it at run time, and users must accept its terms
  separately.
- **flowdegrain2** (measurement denoiser kernels, `kernels/flowdegrain2.cu`): copied from the GRAIL lab
  (grain-lab/flowdegrain2); same owner as this repository.
- **FFmpeg** and **dav1d** are run as external programs / libraries (not included). The av1-fullgrain build of
  libdav1d is BSD-2-Clause (as dav1d).
- **MoltenVK** and the **Vulkan loader** (Apache-2.0): libvship's Vulkan build runs on them on macOS; run-time
  dependencies of libvship, not of grail. **Slang** (`slangc`, Apache-2.0 with LLVM exception): build tool for two of
  libvship's shaders in `tools/build_libvship_macos.sh`.
- Rust crates: anyhow, clap, libloading, serde, serde_json (MIT / Apache-2.0); on macOS objc2, objc2-foundation (MIT)
  and objc2-metal (MIT / Apache-2.0 / Zlib), the Metal bindings.

## Licence

**Not chosen yet (placeholder).** The licence for this repository will be decided by the owner; until then no
licence is granted. (Cargo manifests say `UNLICENSED`.)

## Repository layout

- `kernels/`: CUDA C++ kernels (`flowdegrain2.cu`: the measurement denoiser, verbatim from the reference;
  `grail.cu`: CuPy-equivalent steps, order statistics, grain features, line art; `portable.h`: CUDA/HIP shim).
- `crates/grail-stages`: stage interfaces (for other backends).
- `crates/grail-gpu`: driver layer (CUDA driver API / HIP, loaded at run time), denoiser, features, line art.
- `crates/grail-metal`: the same stages on Metal (macOS); kernels in `kernels/metal/` (MSL ports of the CUDA files).
- `crates/grail`: CLI, decoding (ffmpeg), libvship binding, the v2.1 model (`data/weights.json`), seed view
  (`data/seed_scores.npy`), reference checks.
- `tools/`: lab tools (whole-set case file, benchmark; `bench_macos.py`, `build_libvship_macos.sh` for macOS).
