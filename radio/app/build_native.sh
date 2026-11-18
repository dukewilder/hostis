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
# The C inside (opus) is mapped the same way, or its file names keep the cargo folder's path.
MAP="-ffile-prefix-map=$(pwd)=/pyrate -ffile-prefix-map=${CARGO_HOME:-$HOME/.cargo}=/cargo"
export CFLAGS_aarch64_linux_android="${CFLAGS_aarch64_linux_android:-} $MAP" CXXFLAGS_aarch64_linux_android="${CXXFLAGS_aarch64_linux_android:-} $MAP"
cargo build --release -p pyrate-ffi --target aarch64-linux-android
mkdir -p app/android/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/release/libpyrate_ffi.so app/android/app/src/main/jniLibs/arm64-v8a/
# Dart writes one absolute path into the app (its plugin registrant's), so the app is built from
# /pyrate/app where that is this tree (a bind mount: mount --bind <this folder> /pyrate), and the
# path it carries is the same on every machine. Any further arguments go to flutter (e.g.
# --android-skip-build-dependency-validation).
APP=app
if [ /pyrate/app -ef app ]; then APP=/pyrate/app; else echo "note: /pyrate is not this tree, so the app carries this folder's path" >&2; fi
cd "$APP" && flutter build apk --release --target-platform android-arm64 "$@"
