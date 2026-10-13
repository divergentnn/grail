# macOS backend: handoff report (branch macos-backend)

Working notes for whoever picks this branch up (drop the file before a release, as progress.log was). Machine: Apple
M1 Max (32-core GPU, 64 GB), macOS 26.6, Rust 1.97, Homebrew ffmpeg 9.0, SVT-AV1 4.2, MoltenVK 1.4.2. Test pack:
the 13 cases of port_reference.json and the two bench clips of bench_clips.json (not in the repository).

## Please check first (not verifiable on the Mac)

1. **Linux CUDA / HIP build.** Shared code changed and nvcc is not available here:
   - `crates/grail-gpu/build.rs` returns early when the target is macOS; `src/lib.rs` is `#![cfg(not(target_os = "macos"))]`.
   - `crates/grail/Cargo.toml`: grail-gpu is now a `cfg(not(target_os = "macos"))` dependency, grail-metal a macOS one.
   - `crates/grail/src/main.rs`: `new_backend()` picks the backend; `grail denoise` now runs through the stage traits
     (`Backend::denoiser` + `produce_window` with window = whole frame) instead of grail-gpu's Denoiser directly.
   - `crates/grail/src/score.rs`: the three SSIMULACRA2 pairs run on one worker thread per pair fed by bounded queues
     (4 frames), joined at the end, instead of a thread scope per frame. Same values; it should help CUDA too.
   - `HwDec::VideoToolbox`, `--hwdec videotoolbox`; `-hwaccel` is `videotoolbox` on macOS, `cuda` elsewhere.
   Suggested: `cargo build --release`, `grail check-reference` on the 13 cases (expect the old |dQ| <= 0.0004), the
   bench, and `grail denoise` against flowdegrain2.py (it was bit-exact; the trait path adds only cut_window).
2. **Grain-free decode fix (all platforms, commit 9c593bd).** The Grain::Off decode now sets `DAV1D_GRAIN_FORCE=0`.
   On this Mac Homebrew's libdav1d is an av1-fullgrain build with grain_force on, and `-filmgrain 0` still rendered
   grain: S_Dp off by 31, R huge, the synthetic-grain cases took the coded path (up to 12 points off). The lab boxes
   use the system libdav1d with LD_LIBRARY_PATH substitution only for the full-grain decode, so they were not hit,
   but any machine with a grain-forcing system install was.

## Result

- `crates/grail-metal`: the stage traits on Metal. `kernels/metal/*.metal` are MSL ports of the two .cu files
  (compiled from source at start, ~0.25 s, cached by macOS); the host orchestration mirrors grail-gpu step for step.
  Device pointers are MTLBuffer GPU addresses, each kernel takes its CUDA argument list as one constant struct.
- Apple GPUs have no float64 and no 64-bit atomics: cross-block sums are per-block float partials summed in double on
  the host, counters are 32-bit, corr_d is compensated float-float.
- Exactness, 13 cases, software decode: |dQ| max 0.0171, median 0.0054, Spearman 1.000000, 0 failed. Per stage: S0
  window mean 0.024, worst frame 0.046 (libvship Vulkan/MoltenVK vs CUDA, the bulk of dQ); S_D mean 0.016, one
  frame 1.03 (clean CGI case, fast-math motion search picks another vector); everything from the denoiser on adds
  < 0.02. check-reference flags S0_frame and S_D_frame as OVER (CUDA tolerances kept unchanged on purpose).
  VideoToolbox decoding gives the same scores. Runs are deterministic.
- Speed (README "Speed", Apple M1 Max table): 1080p 14.7 / 31.8 frames/s full / fast; 4K 3.7 / 8.4 software,
  3.9 / 11.4 VideoToolbox (4K varies 3.0-4.7 / 7.2-11.7 with the laptop's state). Same scores as the 3090 runs to 0.02.

## Decisions and why

- **Metal over Vulkan / wgpu / CPU.** Spike: the motion search, the Lanczos warp and the sigma-9.6 filter, same data,
  same protocol. Per unit (1080p / 4K): raster 0.95 / 0.95 ms Metal, 1.12 Vulkan-MoltenVK, 1.29 wgpu, 8.3 CPU
  (8 threads); warp_y x6 1.47 / 5.93 Metal, 1.72 / 6.28, 1.52 / 6.01, 51 / 161; filter 0.44 / 1.56 Metal,
  1.08 / 3.56, 0.88 / 3.24, 1.04 / 4.65. Projected end to end (kernel ratios applied to the Metal profile, +-30 %):
  Vulkan ~8 / ~20 fps at 1080p, wgpu ~9 / ~22, CPU ~1.8 / ~4.9 (with a CPU SSIMULACRA2). The spike harnesses are
  not in the repository (scratch); the GLSL of the three kernels can be rewritten from the MSL quickly.
- **SSIMULACRA2 stays libvship (Vulkan build on MoltenVK).** It needs `tools/build_libvship_macos.sh`: two shaders do
  uint8_t pointer arithmetic that MoltenVK's SPIRV-Cross translation breaks (pipeline creation fails, error -3); the
  script rewrites it on uint64 addresses and recompiles with slangc. Should go upstream (codeberg.org/Line-fr/Vship;
  the GitHub repo is an empty redirect now). MoltenVK fast math must be off (grail sets
  MVK_CONFIG_FAST_MATH_ENABLED=0): with it on, +0.045 per frame. Cost: ~12 ms per call (latency-bound), about a
  quarter of a run through GPU contention (18.3 fps at 1080p full without it).
  The rust `ssimulacra2` crate (libjxl-faithful) is 0.35-0.60 off with its own YUV conversion, 0.11-0.20 with a
  left-sited bilinear one, and ~280 ms/frame: not usable. A native Metal SSIMULACRA2 was not written: matching
  libvship to 0.01 means following its implementation, whose licence (MIT + non-AI terms) is not GPL-compatible
  beyond the section 7 permission for linking; a clean-room port would need libvship's colour / upsampling / blur
  choices established from outputs only.
- **Kernel changes that are not 1:1 with CUDA** (all in fast-math or order-of-summation territory, checked on the 13
  cases): raster uses one SIMD group per patch, two pixels per lane, `simd_sum` (no barriers; another summation order)
  and takes the neighbours' levels as a pointer array (no stack copies); the Lanczos weights use `sin(pi x)` instead
  of `sinpi` (1/3 faster, 1e-9 relative); the scipy filters are threadgroup-tiled with 8 outputs per thread and the
  weights in the argument struct (same taps, same order, same sums as corr_f); 32-bit element indices.

## Measured on the Mac, not in the README tables

- Decode, 4 threads: 4K HEVC 10-bit 20.5 fps software / 80.4 VideoToolbox; 1080p H.264 153 / 135 (so auto = hardware
  above 2048 px, as on Linux); AV1 full-frame grain 188 (1080p) / 73 (4K) on dav1d.
- CPU "GRAILX fast" quick look (XPSNR, every 4th frame, no denoise; ffmpeg's xpsnr filter): 66 fps 1080p, 21.5 / 56
  fps 4K software / VideoToolbox (decode-bound).
- Next to SVT-AV1 preset 6 (41.1 fps alone, no --film-grain): GRAILS full at normal priority halves the encoder
  (17.9) while doing 8-9.5; under `taskpolicy -c utility --threads 2` the encoder keeps 34.9 and GRAILS does 9.7.
  Fast mode under utility is starved while the encoder runs (decode-bound). Background QoS (`taskpolicy -b`) leaves
  GRAILS at 1.8 fps. A CPU kernel load costs the encoder 22 %, the XPSNR quick look 17 %.
- 1080p full: the host waits 4.3 of 6.0 s on the GPU; encoding 46,000 launches takes 0.06 s. GPU time: the feature
  filters ~ 70-80 %, then raster and the warps.

## Open items

- Linux verification (above).
- AMD / Intel: a Vulkan (or HIP for AMD) path; the motion search assumes 32-lane SIMD groups (AMD wave64, Intel
  SIMD8/16 need the two-level reduction back). Intel Macs untested.
- Windows: library names, dav1d DLL substitution, a run.
- Packaging: signed + notarized arm64 build or Homebrew formula; libvship fix upstream; slangc only needed until then.
- Speed: start the denoise before the active-picture scan ends (1080p scan 0.85 s of 6 s); fewer launches / syncs in
  the features (count_lt read mid-frame); a clean-room Metal SSIMULACRA2 (~+25 %).
- No CPU backend (projected < 2 fps at 1080p full, and no accurate CPU SSIMULACRA2).
