#!/usr/bin/env bash
# Builds the node for Android and puts it where Gradle packs it.
set -euo pipefail
cd "$(dirname "$0")/.."
# Exported: opusic-sys tells CMake the ABI only when it sees ANDROID_NDK_HOME, and builds 32-bit
# ARM without it.
export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-/opt/android-ndk-r28c}
NDK=$ANDROID_NDK_HOME
TC=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
export CC_aarch64_linux_android=$TC/aarch64-linux-android28-clang
export CXX_aarch64_linux_android=$TC/aarch64-linux-android28-clang++
export AR_aarch64_linux_android=$TC/llvm-ar
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$TC/aarch64-linux-android28-clang
export CMAKE_TOOLCHAIN_FILE_aarch64_linux_android=$NDK/build/cmake/android.toolchain.cmake
# The build paths are remapped, as browser/build.sh does for the player, so the library names
# none of the builder's own folders and the same source builds the same bytes anywhere.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$(pwd)=/pyrate --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
cargo build --release -p pyrate-ffi --target aarch64-linux-android
mkdir -p app/android/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/release/libpyrate_ffi.so app/android/app/src/main/jniLibs/arm64-v8a/
# Any further arguments go to flutter (e.g. --android-skip-build-dependency-validation).
cd app && flutter build apk --release --target-platform android-arm64 "$@"
