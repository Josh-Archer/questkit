# CMake toolchain used by build.sh for crates that build C/C++ with cmake for
# Android (the Khronos OpenXR loader from openxr-sys, and Opus).
#
# - Points CMake at the NDK ($ANDROID_NDK_HOME), which the plain cmake-rs
#   "-DCMAKE_SYSTEM_NAME=Android" setup cannot find on its own.
# - Defines CMAKE_INSTALL_INCLUDEDIR: OpenXR-SDK only includes GNUInstallDirs on Android when
#   INSTALL_TO_ARCHITECTURE_PREFIXES is set, so otherwise its header install step targets "/openxr".
# The ABI build.sh builds for (APP_ABI): arm64-v8a for the Quest, x86_64 for the emulator.
if(DEFINED ENV{APP_ABI} AND NOT "$ENV{APP_ABI}" STREQUAL "")
    set(ANDROID_ABI $ENV{APP_ABI})
else()
    set(ANDROID_ABI arm64-v8a)
endif()
set(ANDROID_PLATFORM android-30)
set(CMAKE_INSTALL_INCLUDEDIR include)
include("$ENV{ANDROID_NDK_HOME}/build/cmake/android.toolchain.cmake")
