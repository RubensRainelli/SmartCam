#!/usr/bin/env bash
# Personal and Educational Free License (PEF License / PEFL)
# Version 1.0 - 2026-05-26
# Copyright (c) 2026 Rubens Rainelli
#
# Contact Details:
#   Email: rubens@rainelli.it
#   GitHub: https://github.com/RubensRainelli/
#   Tel: (+39) 342 811 5882
#
# Personal and educational free use permitted.
# Corporate, business or enterprise use prohibited without written authorization and fee.
# The full license can be found in the LICENSE file.

set -euo pipefail

APPIMAGE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LINUX_DIR="$(cd "${APPIMAGE_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${LINUX_DIR}/../.." && pwd)"
BUILD_ROOT="${LINUX_DIR}/build"
APPDIR="${BUILD_ROOT}/SmartCam.AppDir"
DIST_DIR="${REPO_ROOT}/dist"
VERSION="1.0.0"
ARCH="x86_64"
OUTPUT_APPIMAGE="${DIST_DIR}/SmartCam-v${VERSION}-${ARCH}.AppImage"

ACCEPT_PEFL=0
RUN_TESTS=1
CLEAN_BUILD=0

for arg in "$@"; do
    case "$arg" in
        --accept-pefl)
            ACCEPT_PEFL=1
            ;;
        --skip-tests)
            RUN_TESTS=0
            ;;
        --clean)
            CLEAN_BUILD=1
            ;;
    esac
done

# ==============================================================================
# 1. PEFL v1.0 Clause 3 Mandatory Startup Verification
# ==============================================================================
verify_pefl_clause3() {
    local cache_file="${APPIMAGE_DIR}/.pefl_accepted_build_appimage"
    local now_ts
    now_ts="$(date +%s)"
    local max_age=$((7 * 24 * 60 * 60))

    if (( ACCEPT_PEFL == 1 )); then
        echo "$now_ts" > "$cache_file" 2>/dev/null || true
        return 0
    fi

    if [[ -f "$cache_file" ]]; then
        local last_ts
        last_ts="$(head -n 1 "$cache_file" 2>/dev/null | tr -d '[:space:]' || echo 0)"
        if [[ "$last_ts" =~ ^[0-9]+$ ]]; then
            local age=$((now_ts - last_ts))
            if (( age >= 0 && age < max_age )); then
                return 0
            fi
        fi
    fi

    echo "============================================================"
    echo "        Personal and Educational Free License (PEFL) v1.0"
    echo "============================================================"
    echo "Copyright (c) 2026 Rubens Rainelli"
    echo "Official Specification: https://github.com/RubensRainelli/pefl-license-spec"
    echo "Author Contact: rubens@rainelli.it | (+39) 342 811 5882"
    echo ""
    echo "TERMS OF USE:"
    echo "  * FREE for Personal & Educational use."
    echo "  * STRICTLY PROHIBITED for Corporate, Professional, or Business use"
    echo "    without written authorization and fee payment."
    echo "  * Excludes places of worship and religious organizations (corporate use)."
    echo "============================================================"
    read -r -p "Do you accept the terms of the PEFL v1.0 license? (y/N): " choice
    case "${choice,,}" in
        y|yes|s|si)
            echo "$now_ts" > "$cache_file" 2>/dev/null || true
            echo "[SmartCam Builder] License accepted."
            ;;
        *)
            echo "[SmartCam Builder] License rejected. Exiting."
            exit 1
            ;;
    esac
}

verify_pefl_clause3

if (( CLEAN_BUILD == 1 )); then
    echo "[SmartCam AppImage] Cleaning previous Linux build directories..."
    rm -rf "${BUILD_ROOT}/out_avx2" "${BUILD_ROOT}/out_avx512" "${APPDIR}"
fi

mkdir -p "${BUILD_ROOT}/out_avx2" "${BUILD_ROOT}/out_avx512" "${DIST_DIR}"

CMAKE_GEN=()
if command -v ninja >/dev/null 2>&1; then
    CMAKE_GEN=(-G Ninja)
fi

# ==============================================================================
# 2. Build C++17 SIMD Engine + V4L2 Bridge (Variant 1: out_avx2)
# ==============================================================================
echo "============================================================"
echo "[1/4] Building C++17 Engine & V4L2 Bridge: out_avx2 (AVX2 Baseline)"
echo "============================================================"
cmake "${CMAKE_GEN[@]}" \
    -S "${LINUX_DIR}" \
    -B "${BUILD_ROOT}/out_avx2" \
    -DCMAKE_BUILD_TYPE=Release \
    -DSMARTCAM_ENABLE_AVX512=OFF \
    -DSMARTCAM_ENABLE_TESTS=ON

cmake --build "${BUILD_ROOT}/out_avx2" --config Release --parallel

if (( RUN_TESTS == 1 )) && [[ -x "${BUILD_ROOT}/out_avx2/test_engine_cpp" ]]; then
    echo "[SmartCam AppImage] Running C++ unit tests (out_avx2)..."
    "${BUILD_ROOT}/out_avx2/test_engine_cpp"
fi

# ==============================================================================
# 3. Build C++17 SIMD Engine + V4L2 Bridge (Variant 2: out_avx512)
# ==============================================================================
echo "============================================================"
echo "[2/4] Building C++17 Engine & V4L2 Bridge: out_avx512 (AVX-512 High-Throughput)"
echo "============================================================"
cmake "${CMAKE_GEN[@]}" \
    -S "${LINUX_DIR}" \
    -B "${BUILD_ROOT}/out_avx512" \
    -DCMAKE_BUILD_TYPE=Release \
    -DSMARTCAM_ENABLE_AVX512=ON \
    -DSMARTCAM_ENABLE_TESTS=ON

cmake --build "${BUILD_ROOT}/out_avx512" --config Release --parallel

if (( RUN_TESTS == 1 )) && [[ -x "${BUILD_ROOT}/out_avx512/test_engine_cpp" ]]; then
    if grep -q "avx512f" /proc/cpuinfo 2>/dev/null; then
        echo "[SmartCam AppImage] Running C++ unit tests (out_avx512)..."
        "${BUILD_ROOT}/out_avx512/test_engine_cpp"
    fi
fi

# ==============================================================================
# 4. Build Native Application (`SmartCam`)
# ==============================================================================
echo "============================================================"
echo "[3/4] Building Native Application (SmartCam)"
echo "============================================================"
export PATH="$HOME/.cargo/bin:$PATH"
if command -v cargo >/dev/null 2>&1 && [[ -f "${REPO_ROOT}/app/Cargo.toml" ]]; then
    (cd "${REPO_ROOT}/app" && cargo build --release) ||
        echo "[SmartCam AppImage] WARNING: 'cargo build --release' failed; packaging a previous app/target/release/SmartCam if present, otherwise the CMake-built C++17 binary."
else
    echo "[SmartCam AppImage] Using CMake-built native C++17 Linux binary (${BUILD_ROOT}/out_avx2/SmartCam)."
fi

# ==============================================================================
# 5. Populate SmartCam.AppDir Hierarchy
# ==============================================================================
echo "============================================================"
echo "[4/4] Assembling SmartCam.AppDir & Packaging AppImage"
echo "============================================================"
rm -rf "${APPDIR}"
mkdir -p \
    "${APPDIR}/usr/bin" \
    "${APPDIR}/usr/lib" \
    "${APPDIR}/usr/share/applications" \
    "${APPDIR}/usr/share/icons/hicolor/256x256/apps" \
    "${APPDIR}/usr/share/smartcam/config" \
    "${APPDIR}/usr/share/smartcam/assets" \
    "${APPDIR}/usr/share/smartcam/web_preview" \
    "${APPDIR}/config" \
    "${APPDIR}/assets"

# Copy AppRun & Desktop Entry
cp -f "${APPIMAGE_DIR}/AppRun" "${APPDIR}/AppRun"
chmod +x "${APPDIR}/AppRun"
cp -f "${APPIMAGE_DIR}/smartcam.desktop" "${APPDIR}/smartcam.desktop"
cp -f "${APPIMAGE_DIR}/smartcam.desktop" "${APPDIR}/usr/share/applications/smartcam.desktop"

# Copy v4l2loopback helper script into usr/bin
cp -f "${LINUX_DIR}/scripts/setup_v4l2loopback.sh" "${APPDIR}/usr/bin/setup_v4l2loopback.sh"
chmod +x "${APPDIR}/usr/bin/setup_v4l2loopback.sh"

# Copy Rust or C++17 native Linux binary
if [[ -f "${REPO_ROOT}/app/target/release/SmartCam" ]]; then
    cp -f "${REPO_ROOT}/app/target/release/SmartCam" "${APPDIR}/usr/bin/SmartCam"
elif [[ -f "${REPO_ROOT}/app/target/release/smartcam" ]]; then
    cp -f "${REPO_ROOT}/app/target/release/smartcam" "${APPDIR}/usr/bin/SmartCam"
elif [[ -f "${BUILD_ROOT}/out_avx2/SmartCam" ]]; then
    cp -f "${BUILD_ROOT}/out_avx2/SmartCam" "${APPDIR}/usr/bin/SmartCam"
elif [[ -f "${BUILD_ROOT}/out_avx2/test_engine_cpp" ]]; then
    cp -f "${BUILD_ROOT}/out_avx2/test_engine_cpp" "${APPDIR}/usr/bin/SmartCam"
fi
chmod +x "${APPDIR}/usr/bin/SmartCam"

# Copy Dual-Variant Shared Libraries (out_avx2 & out_avx512)
cp -f "${BUILD_ROOT}/out_avx2/libsmartcam_engine_avx2.so"     "${APPDIR}/usr/lib/libsmartcam_engine_avx2.so"
cp -f "${BUILD_ROOT}/out_avx2/libsmartcam_engine.so"          "${APPDIR}/usr/lib/libsmartcam_engine.so"
cp -f "${BUILD_ROOT}/out_avx2/libsmartcam_linux_bridge.so"    "${APPDIR}/usr/lib/libsmartcam_linux_bridge.so"
cp -f "${BUILD_ROOT}/out_avx512/libsmartcam_engine_avx512.so" "${APPDIR}/usr/lib/libsmartcam_engine_avx512.so"

# Copy Icons, Preset Backgrounds, Config & Legal Files
if [[ -f "${REPO_ROOT}/assets/icons/smartcam_logo.png" ]]; then
    cp -f "${REPO_ROOT}/assets/icons/smartcam_logo.png" "${APPDIR}/smartcam.png"
    cp -f "${REPO_ROOT}/assets/icons/smartcam_logo.png" "${APPDIR}/.DirIcon"
    cp -f "${REPO_ROOT}/assets/icons/smartcam_logo.png" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/smartcam.png"
fi

if [[ -d "${REPO_ROOT}/assets" ]]; then
    cp -rf "${REPO_ROOT}/assets/." "${APPDIR}/assets/"
    cp -rf "${REPO_ROOT}/assets/." "${APPDIR}/usr/share/smartcam/assets/"
fi

if [[ -d "${REPO_ROOT}/config" ]]; then
    cp -rf "${REPO_ROOT}/config/." "${APPDIR}/config/"
    cp -rf "${REPO_ROOT}/config/." "${APPDIR}/usr/share/smartcam/config/"
fi

if [[ -d "${REPO_ROOT}/app/web_preview" ]]; then
    cp -rf "${REPO_ROOT}/app/web_preview/." "${APPDIR}/usr/share/smartcam/web_preview/"
fi

for doc in LICENSE LICENSE.it NOTICE.md AI_INSTRUCTIONS.md; do
    if [[ -f "${REPO_ROOT}/${doc}" ]]; then
        cp -f "${REPO_ROOT}/${doc}" "${APPDIR}/${doc}"
    fi
done

# ==============================================================================
# 6. Package into dist/SmartCam-v1.0.0-x86_64.AppImage
# ==============================================================================
APPIMAGETOOL_BIN="${BUILD_ROOT}/appimagetool-x86_64.AppImage"

if command -v appimagetool >/dev/null 2>&1; then
    ARCH="${ARCH}" appimagetool --no-appstream "${APPDIR}" "${OUTPUT_APPIMAGE}"
else
    if [[ ! -x "${APPIMAGETOOL_BIN}" ]]; then
        echo "[SmartCam AppImage] Downloading official appimagetool-x86_64.AppImage..."
        curl -fsSL -o "${APPIMAGETOOL_BIN}" \
            "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
        chmod +x "${APPIMAGETOOL_BIN}"
    fi

    # Support FUSE-less environments (WSL2 / Docker containers) via --appimage-extract-and-run
    ARCH="${ARCH}" APPIMAGE_EXTRACT_AND_RUN=1 "${APPIMAGETOOL_BIN}" --no-appstream "${APPDIR}" "${OUTPUT_APPIMAGE}"
fi

chmod +x "${OUTPUT_APPIMAGE}"

# Also copy ready-to-use AppImage and Linux .so binaries into bin/
BIN_DIR="${REPO_ROOT}/bin"
mkdir -p "${BIN_DIR}"
cp -f "${OUTPUT_APPIMAGE}" "${BIN_DIR}/SmartCam-v${VERSION}-${ARCH}.AppImage"
chmod +x "${BIN_DIR}/SmartCam-v${VERSION}-${ARCH}.AppImage"
cp -f "${BUILD_ROOT}/out_avx2/libsmartcam_engine_avx2.so"     "${BIN_DIR}/libsmartcam_engine_avx2.so"
cp -f "${BUILD_ROOT}/out_avx2/libsmartcam_engine.so"          "${BIN_DIR}/libsmartcam_engine.so"
cp -f "${BUILD_ROOT}/out_avx2/libsmartcam_linux_bridge.so"    "${BIN_DIR}/libsmartcam_linux_bridge.so"
cp -f "${BUILD_ROOT}/out_avx512/libsmartcam_engine_avx512.so" "${BIN_DIR}/libsmartcam_engine_avx512.so"

# Update SHA256SUMS.txt in dist/
(
    cd "${DIST_DIR}"
    if [[ -f SHA256SUMS.txt ]]; then
        grep -v "$(basename "${OUTPUT_APPIMAGE}")" SHA256SUMS.txt > SHA256SUMS.tmp || true
        mv -f SHA256SUMS.tmp SHA256SUMS.txt
    fi
    sha256sum "$(basename "${OUTPUT_APPIMAGE}")" >> SHA256SUMS.txt
)

echo "============================================================"
echo "✅ SmartCam Linux AppImage created successfully!"
echo "   Output (dist/) : ${OUTPUT_APPIMAGE}"
echo "   Output (bin/)  : ${BIN_DIR}/SmartCam-v${VERSION}-${ARCH}.AppImage"
echo "   SHA256         : $(sha256sum "${OUTPUT_APPIMAGE}" | awk '{print $1}')"
echo "============================================================"
