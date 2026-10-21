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

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 1. Normalize line endings (CRLF -> LF) on all Linux scripts in case repository was cloned on Windows
for sh_file in \
    "${REPO_ROOT}/platforms/linux/appimage/build_appimage.sh" \
    "${REPO_ROOT}/platforms/linux/appimage/AppRun" \
    "${REPO_ROOT}/platforms/linux/scripts/setup_v4l2loopback.sh"; do
    if [[ -f "$sh_file" ]]; then
        sed -i 's/\r$//' "$sh_file" 2>/dev/null || true
        chmod +x "$sh_file" 2>/dev/null || true
    fi
done

# 2. Check basic C++ build dependencies on Linux (cmake, g++, curl, file)
MISSING_PKGS=()
command -v cmake >/dev/null 2>&1 || MISSING_PKGS+=("cmake")
command -v g++   >/dev/null 2>&1 || MISSING_PKGS+=("g++")
command -v curl  >/dev/null 2>&1 || MISSING_PKGS+=("curl")
command -v file  >/dev/null 2>&1 || MISSING_PKGS+=("file")

if (( ${#MISSING_PKGS[@]} > 0 )); then
    echo "[SmartCam] Installazione automatica dipendenze di compilazione mancanti: ${MISSING_PKGS[*]}..."
    if command -v apt-get >/dev/null 2>&1; then
        sudo apt-get update && sudo apt-get install -y build-essential cmake ninja-build curl file libfuse2
    elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y gcc-c++ cmake ninja-build curl file fuse-libs
    elif command -v pacman >/dev/null 2>&1; then
        sudo pacman -Sy --noconfirm base-devel cmake ninja curl file fuse2
    fi
fi

# 3. Run the automated AppImage builder (places SmartCam-v1.0.0-x86_64.AppImage in dist/ and bin/)
exec "${REPO_ROOT}/platforms/linux/appimage/build_appimage.sh" --accept-pefl "$@"
