# Installing and building GRAIL

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
(32-core GPU, 64 GB) with macOS 26.6 and Rust 1.97; exactness and speed in [technical.md](technical.md#exactness-against-the-python-reference).

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
- Next to an encoder on the same Mac: `taskpolicy -c utility grail ...` keeps most of the CPU for the encoder (numbers in
  [technical.md](technical.md#speed)).
- `GRAIL_METAL_PROFILE=1` prints the GPU time per kernel (each launch timed on its own, so slower overall);
  `GRAIL_METAL_PROFILE=host` the time spent encoding launches and waiting for the GPU.


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
  `GRAIL_VSHIP`). Bundling it would put its licence terms (see [technical.md](technical.md#third-party-components-and-licences)) on the package.
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


## Repository layout

- `kernels/`: CUDA C++ kernels (`flowdegrain2.cu`: the measurement denoiser, verbatim from the reference;
  `grail.cu`: CuPy-equivalent steps, order statistics, grain features, line art; `portable.h`: CUDA/HIP shim).
- `crates/grail-stages`: stage interfaces (for other backends).
- `crates/grail-gpu`: driver layer (CUDA driver API / HIP, loaded at run time), denoiser, features, line art.
- `crates/grail-metal`: the same stages on Metal (macOS); kernels in `kernels/metal/` (MSL ports of the CUDA files).
- `crates/grail`: CLI, decoding (ffmpeg), libvship binding, the v2.1 model (`data/weights.json`), seed view
  (`data/seed_scores.npy`), reference checks.
- `tools/`: lab tools (whole-set case file, benchmark; `bench_macos.py`, `build_libvship_macos.sh` for macOS).

Back to the [README](../README.md).
