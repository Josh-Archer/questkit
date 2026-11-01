#!/usr/bin/env bash
set -euo pipefail

# Shared Meta Quest Android APK build and package harness
# Targets 64-bit ARM (arm64-v8a / aarch64-linux-android) on Meta Horizon OS. APP_ABI=x86_64
# builds for an x86_64 Android emulator instead (DeskQuest's start-up smoke test).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="${APP_DIR:?Set APP_DIR to the app crate containing Cargo.toml}"
APP_DIR="$(cd "${APP_DIR}" && pwd)"
APP_LIB_NAME="${APP_LIB_NAME:?Set APP_LIB_NAME to the cdylib name without lib/.so}"
APP_PACKAGE="${APP_PACKAGE:?Set APP_PACKAGE to the Android package id}"
APP_LABEL="${APP_LABEL:-XR Application}"
APP_APK_NAME="${APP_APK_NAME:-${APP_LIB_NAME}}"
APP_ANDROID_DIR="${APP_ANDROID_DIR:-${APP_DIR}/android}"
APP_MANIFEST="${APP_MANIFEST:-${SCRIPT_DIR}/AndroidManifest.xml.in}"
APP_RES_DIR="${APP_RES_DIR:-${APP_ANDROID_DIR}/res}"
APP_JAVA_DIR="${APP_JAVA_DIR:-${APP_ANDROID_DIR}/src}"
APP_ACTIVITY="${APP_ACTIVITY:-android.app.NativeActivity}"
APP_CARGO_FEATURES="${APP_CARGO_FEATURES:-}"
APP_ABI="${APP_ABI:-arm64-v8a}"
case "${APP_ABI}" in
    arm64-v8a) RUST_TARGET=aarch64-linux-android ;;
    x86_64) RUST_TARGET=x86_64-linux-android ;;
    *) echo "ERROR: APP_ABI must be arm64-v8a or x86_64, not ${APP_ABI}" >&2; exit 1 ;;
esac
export APP_ABI

# Detect Android SDK and NDK
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-${HOME}/Android/Sdk}}"
if [[ ! -d "${ANDROID_SDK_ROOT}" ]]; then
    echo "ERROR: Android SDK not found at ${ANDROID_SDK_ROOT}. Set ANDROID_SDK_ROOT." >&2
    exit 1
fi

ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${ANDROID_SDK_ROOT}/ndk/27.2.12479018}"
if [[ ! -d "${ANDROID_NDK_HOME}" ]]; then
    # Fallback to any installed NDK
    ANDROID_NDK_HOME="$(find "${ANDROID_SDK_ROOT}/ndk" -maxdepth 1 -mindepth 1 | sort -V | tail -n1 || true)"
fi

if [[ -z "${ANDROID_NDK_HOME}" || ! -d "${ANDROID_NDK_HOME}" ]]; then
    echo "ERROR: Android NDK not found. Set ANDROID_NDK_HOME." >&2
    exit 1
fi
export ANDROID_NDK_HOME
# audiopus_sys bundles an opus CMakeLists that CMake 4.x rejects without this (#109).
export CMAKE_POLICY_VERSION_MINIMUM="${CMAKE_POLICY_VERSION_MINIMUM:-3.5}"

# Locate build tools
BUILD_TOOLS_DIR="$(find "${ANDROID_SDK_ROOT}/build-tools" -maxdepth 1 -mindepth 1 | sort -V | tail -n1)"
if [[ -z "${BUILD_TOOLS_DIR}" || ! -d "${BUILD_TOOLS_DIR}" ]]; then
    echo "ERROR: Android build-tools not found in ${ANDROID_SDK_ROOT}/build-tools" >&2
    exit 1
fi

AAPT="${BUILD_TOOLS_DIR}/aapt"
D8="${BUILD_TOOLS_DIR}/d8"
ZIPALIGN="${BUILD_TOOLS_DIR}/zipalign"
APKSIGNER="${BUILD_TOOLS_DIR}/apksigner"
ANDROID_JAR="${ANDROID_SDK_ROOT}/platforms/android-34/android.jar"
ADB="${ANDROID_SDK_ROOT}/platform-tools/adb"
if command -v adb &> /dev/null; then
    ADB="$(command -v adb)"
fi

if [[ ! -f "${ANDROID_JAR}" ]]; then
    ANDROID_JAR="$(find "${ANDROID_SDK_ROOT}/platforms" -name "android.jar" | sort -V | tail -n1 || true)"
fi

if [[ ! -f "${D8}" ]]; then
    echo "ERROR: d8 not found in ${BUILD_TOOLS_DIR}." >&2
    exit 1
fi

if [[ ! -f "${ANDROID_JAR}" ]]; then
    echo "ERROR: android.jar not found under ${ANDROID_SDK_ROOT}/platforms" >&2
    exit 1
fi

# Honor isolated build directories, including relative paths, before changing directory.
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${APP_DIR}/target}"
if [[ "${CARGO_TARGET_DIR}" != /* ]]; then
    CARGO_TARGET_DIR="${PWD}/${CARGO_TARGET_DIR}"
fi
export CARGO_TARGET_DIR

# Parse arguments
BUILD_MODE="debug"
CARGO_PROFILE_FLAG=""
TARGET_DIR="${CARGO_TARGET_DIR}/${RUST_TARGET}/debug"
INSTALL_ON_DEVICE=false
RUN_ON_DEVICE=false
LAUNCH_ACTIVITY="${APP_LAUNCH_ACTIVITY:-${APP_ACTIVITY}}"

for arg in "$@"; do
    case "$arg" in
        --release)
            BUILD_MODE="release"
            CARGO_PROFILE_FLAG="--release"
            TARGET_DIR="${CARGO_TARGET_DIR}/${RUST_TARGET}/release"
            ;;
        --install)
            INSTALL_ON_DEVICE=true
            ;;
        --run)
            INSTALL_ON_DEVICE=true
            RUN_ON_DEVICE=true
            ;;
        --vr)
            LAUNCH_ACTIVITY="${APP_ACTIVITY}"
            ;;
        -h|--help)
            echo "Usage: $0 [--release] [--install] [--run] [--vr]"
            echo "  --release  Build optimized release APK"
            echo "  --install  Install APK on connected Meta Quest via adb"
            echo "  --run      Install and launch APK on connected Meta Quest"
            echo "  --vr       With --run, launch immersive VR instead of the Home panel"
            exit 0
            ;;
    esac
done

echo "========================================================"
echo "${APP_LABEL} Quest APK Packaging Harness"
echo "Build Mode:       ${BUILD_MODE}"
echo "Android SDK:      ${ANDROID_SDK_ROOT}"
echo "Android NDK:      ${ANDROID_NDK_HOME}"
echo "Build Tools:      ${BUILD_TOOLS_DIR}"
echo "Android Platform: ${ANDROID_JAR}"
echo "========================================================"

# cmake-built dependencies (OpenXR loader, Opus) for this target use the NDK toolchain wrapper.
export CMAKE_TOOLCHAIN_FILE_aarch64_linux_android="${CMAKE_TOOLCHAIN_FILE_aarch64_linux_android:-${SCRIPT_DIR}/ndk-cmake-toolchain.cmake}"
export CMAKE_TOOLCHAIN_FILE_x86_64_linux_android="${CMAKE_TOOLCHAIN_FILE_x86_64_linux_android:-${SCRIPT_DIR}/ndk-cmake-toolchain.cmake}"

# Step 1: Compile native shared library with cargo-ndk
echo "[1/6] Compiling native cdylib with cargo-ndk (${APP_ABI}, API 30)..."
CARGO_ARGS=()
if [[ -n "${CARGO_PROFILE_FLAG}" ]]; then CARGO_ARGS+=("${CARGO_PROFILE_FLAG}"); fi
if [[ -n "${APP_CARGO_FEATURES}" ]]; then CARGO_ARGS+=(--features "${APP_CARGO_FEATURES}"); fi
cargo ndk -t "${APP_ABI}" -P 30 build "${CARGO_ARGS[@]}" --locked --manifest-path "${APP_DIR}/Cargo.toml"

SO_FILE="${TARGET_DIR}/lib${APP_LIB_NAME}.so"
if [[ ! -f "${SO_FILE}" ]]; then
    echo "ERROR: Native library not found at ${SO_FILE}" >&2
    exit 1
fi

# Step 2: Prepare packaging directories
OUT_DIR="${APP_APK_OUT_DIR:-${APP_DIR}/target/android-apk}"
mkdir -p "${OUT_DIR}/lib/${APP_ABI}"
cp "${SO_FILE}" "${OUT_DIR}/lib/${APP_ABI}/lib${APP_LIB_NAME}.so"

# The Quest APK keeps its name; another ABI's is named after it.
APK_STEM="${APP_APK_NAME}-${BUILD_MODE}"
[[ "${APP_ABI}" != arm64-v8a ]] && APK_STEM="${APK_STEM}-${APP_ABI}"
UNALIGNED_APK="${OUT_DIR}/${APK_STEM}-unaligned.apk"
ALIGNED_APK="${OUT_DIR}/${APK_STEM}-aligned.apk"
FINAL_APK="${OUT_DIR}/${APK_STEM}.apk"

rm -f "${UNALIGNED_APK}" "${ALIGNED_APK}" "${FINAL_APK}"

# The Quest library reads the mipmap launcher icon. Refresh it from the branding
# master so a desktop logo change cannot leave the headset on an older picture.
STAGE_RES="${OUT_DIR}/staged-res"
rm -rf "${STAGE_RES}"
mkdir -p "${STAGE_RES}/mipmap-nodpi"
cp -a "${APP_RES_DIR}/." "${STAGE_RES}/"
MASTER_LOGO="${APP_MASTER_LOGO:-}"
LAUNCHER_ICON="${STAGE_RES}/mipmap-nodpi/ic_launcher.png"
if [[ -f "${MASTER_LOGO}" ]] && command -v ffmpeg >/dev/null 2>&1; then
    ffmpeg -v error -y -i "${MASTER_LOGO}" \
        -vf scale=512:512:flags=lanczos -frames:v 1 -update 1 \
        "${LAUNCHER_ICON}"
fi
if [[ ! -f "${LAUNCHER_ICON}" ]]; then
    echo "ERROR: Quest launcher icon missing at ${LAUNCHER_ICON}" >&2
    exit 1
fi

# Step 3: Package APK with aapt
echo "[2/6] Packaging APK manifest and resources with aapt..."
if [[ "${APP_MANIFEST}" == *.in ]]; then
    export APP_PACKAGE APP_LABEL APP_LIB_NAME APP_ACTIVITY
    export APP_HAS_CODE=false
    if [[ -d "${APP_JAVA_DIR}" ]]; then APP_HAS_CODE=true; fi
    python3 "${SCRIPT_DIR}/render-manifest.py" "${APP_MANIFEST}" "${OUT_DIR}/AndroidManifest.xml"
    APP_MANIFEST="${OUT_DIR}/AndroidManifest.xml"
fi
"${AAPT}" package -F "${UNALIGNED_APK}" \
    -M "${APP_MANIFEST}" \
    -S "${STAGE_RES}" \
    -I "${ANDROID_JAR}" \
    -0 so \
    -f

if [[ -d "${APP_JAVA_DIR}" ]]; then
    # Compile all activity/IME sources and nested classes.
    JAVA_OUT="${OUT_DIR}/java-classes"
    rm -rf "${JAVA_OUT}"
    mkdir -p "${JAVA_OUT}"
    mapfile -d '' JAVA_SOURCES < <(find "${APP_JAVA_DIR}" -name '*.java' -print0)
    javac -encoding UTF-8 --release 8 -classpath "${ANDROID_JAR}" \
        -d "${JAVA_OUT}" "${JAVA_SOURCES[@]}"
    mapfile -d '' JAVA_CLASSES < <(find "${JAVA_OUT}" -name '*.class' -print0)
    "${D8}" --lib "${ANDROID_JAR}" --min-api 30 --output "${OUT_DIR}" \
        "${JAVA_CLASSES[@]}"
    cd "${OUT_DIR}"
    "${AAPT}" add "${UNALIGNED_APK}" classes.dex
    cd "${APP_DIR}"
fi

# Step 4: Add native libraries to APK (uncompressed for direct page-alignment)
echo "[3/6] Adding native shared libraries to APK..."
cd "${OUT_DIR}"
"${AAPT}" add -0 so "${UNALIGNED_APK}" "lib/${APP_ABI}/lib${APP_LIB_NAME}.so"
cd "${APP_DIR}"

# Step 5: Align APK with zipalign (4-byte alignment, page-align shared libs)
echo "[4/6] Aligning APK with zipalign..."
"${ZIPALIGN}" -f -p 4 "${UNALIGNED_APK}" "${ALIGNED_APK}"

# Step 6: Create the debug keystore if absent and sign the APK.
# The keystore lives per user, outside the repo, so every checkout signs with the same key and
# `adb install -r` can upgrade in place. Override with APP_KEYSTORE.
KEYSTORE="${APP_KEYSTORE:-${XDG_DATA_HOME:-${HOME}/.local/share}/${APP_PACKAGE}/debug.keystore}"
if [[ ! -f "${KEYSTORE}" ]]; then
    echo "[5/6] Generating debug signing keystore at ${KEYSTORE} (reused by later builds)..."
    mkdir -p "$(dirname "${KEYSTORE}")"
    keytool -genkey -v \
        -keystore "${KEYSTORE}" \
        -storepass android \
        -alias androiddebugkey \
        -keypass android \
        -keyalg RSA \
        -keysize 2048 \
        -validity 10000 \
        -dname "CN=Android Debug,O=Android,C=US" > /dev/null 2>&1
    chmod 600 "${KEYSTORE}"
else
    echo "[5/6] Using debug keystore ${KEYSTORE}..."
fi

echo "[6/6] Signing APK with apksigner..."
"${APKSIGNER}" sign \
    --ks "${KEYSTORE}" \
    --ks-pass pass:android \
    --key-pass pass:android \
    --out "${FINAL_APK}" \
    "${ALIGNED_APK}"

"${APKSIGNER}" verify --verbose "${FINAL_APK}" > /dev/null

echo "========================================================"
echo "SUCCESS: Built and signed Quest 3 APK:"
echo "  -> ${FINAL_APK}"
ls -lh "${FINAL_APK}"
echo "========================================================"

# Optional install & run
if [[ "${INSTALL_ON_DEVICE}" == "true" ]]; then
    echo "Installing APK to connected device..."
    "${ADB}" install -r "${FINAL_APK}"
fi

if [[ "${RUN_ON_DEVICE}" == "true" ]]; then
    echo "Launching linux-quest client on Meta Quest..."
    "${ADB}" shell am start -a android.intent.action.MAIN -n "${APP_PACKAGE}/${LAUNCH_ACTIVITY}"
fi
