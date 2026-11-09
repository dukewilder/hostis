#!/usr/bin/env bash
# Builds the node for Android and puts it where Gradle packs it.
set -euo pipefail
cd "$(dirname "$0")/.."
NDK=${ANDROID_NDK_HOME:-/opt/android-ndk-r28c}
TC=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
export CC_aarch64_linux_android=$TC/aarch64-linux-android28-clang
export CXX_aarch64_linux_android=$TC/aarch64-linux-android28-clang++
export AR_aarch64_linux_android=$TC/llvm-ar
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$TC/aarch64-linux-android28-clang
export CMAKE_TOOLCHAIN_FILE_aarch64_linux_android=$NDK/build/cmake/android.toolchain.cmake
cargo build --release -p pyrate-ffi --target aarch64-linux-android
mkdir -p app/android/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/release/libpyrate_ffi.so app/android/app/src/main/jniLibs/arm64-v8a/
cd app && flutter build apk --release --target-platform android-arm64
