#!/bin/sh
# Builds libvship (Vulkan backend) for macOS, where it runs on MoltenVK, and installs it as PREFIX/lib/libvship.dylib.
#
#   tools/build_libvship_macos.sh [PREFIX]          # default PREFIX: $HOME/.local
#
# Needs: Homebrew molten-vk, vulkan-loader, vulkan-headers; git; clang; slangc (the Slang shader compiler, for two
# shaders: https://github.com/shader-slang/slang/releases, macos-aarch64; set SLANGC=/path/to/slangc if it is not on
# PATH).
#
# Two of libvship's shaders compute byte addresses with uint8_t pointer arithmetic. MoltenVK's SPIR-V -> MSL
# translation (SPIRV-Cross) drops the address-of in that pattern, the Metal compiler rejects the generated shader and
# libvship fails at init ("Error -3 while creating compute pipeline"). This script rewrites the arithmetic on uint64_t
# addresses (same addresses, same loads) and recompiles those two shaders; nothing else changes. Worth reporting
# upstream; then this script reduces to a plain `make build BACKEND=Vulkan`.
#
# libvship is not GPL software (MIT licence plus non-AI restrictions; README, "Third-party components"): it is built
# here from its own repository and loaded at run time, never bundled.
set -eu

PREFIX=${1:-$HOME/.local}
VSHIP_REPO=${VSHIP_REPO:-https://codeberg.org/Line-fr/Vship}
VSHIP_REF=${VSHIP_REF:-v5.1.2}
SLANGC=${SLANGC:-slangc}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

command -v "$SLANGC" >/dev/null 2>&1 || { echo "slangc not found (set SLANGC)"; exit 1; }
BREW=$(brew --prefix)

git clone -q --branch "$VSHIP_REF" --depth 1 "$VSHIP_REPO" "$WORK/vship"
cd "$WORK/vship"

P=src/Vulkan/util/shaders/Planed.slang
A=src/Vulkan/gpuColorToLinear/shaders/anyDepthToFloat.slang
sed -i '' 's/((float\*)((uint8_t\*)src + i\*stride))\[j\]/((float*)((uint64_t)src + i*stride))[j]/g' "$P"
sed -i '' 's/uint8_t\* baseAdress = source_plane+line\*stride + byteSample\*column;/const uint64_t addr = (uint64_t)source_plane+line*stride + byteSample*column;\
    uint8_t* baseAdress = (uint8_t*)addr;/' "$A"
sed -i '' 's/return \*((float\*)baseAdress);/return *((float*)addr);/; s/return \*((__half\*)baseAdress);/return *((__half*)addr);/; s/return \*((uint16_t\*)baseAdress)/return *((uint16_t*)addr)/' "$A"
grep -q '(uint64_t)src + i\*stride' "$P" && grep -q 'uint8_t\* baseAdress = (uint8_t\*)addr;' "$A" \
    || { echo "libvship $VSHIP_REF: the shaders changed, patch not applied"; exit 1; }

"$SLANGC" "$P" -O2 -target spirv -profile spirv_1_3 -emit-spirv-directly -fvk-use-entrypoint-name \
    -entry StrideEliminator -entry StrideAdder -o libvshipSpvShaders/Planed.spv
"$SLANGC" "$A" -O2 -target spirv -profile spirv_1_3 -emit-spirv-directly -fvk-use-entrypoint-name \
    -entry convertToFloatPlane -o libvshipSpvShaders/anyDepthToFloat.spv

make build BACKEND=Vulkan CXX=clang++ CXXFLAGS="-I$BREW/include" LDFLAGS="-L$BREW/lib" RMCMD="rm -f"
mkdir -p "$PREFIX/lib"
cp libvship.so "$PREFIX/lib/libvship.dylib"
echo "installed $PREFIX/lib/libvship.dylib (libvship $VSHIP_REF, Vulkan backend); export GRAIL_VSHIP=$PREFIX/lib/libvship.dylib"
