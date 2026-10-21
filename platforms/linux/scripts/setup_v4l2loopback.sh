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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CACHE_FILE="${SCRIPT_DIR}/.pefl_accepted_setup_v4l2loopback"
VIDEO_NR="${SMARTCAM_VIDEO_NR:-10}"
CARD_LABEL="${SMARTCAM_CARD_LABEL:-SmartCam}"
PERSIST_BOOT=0
ACCEPT_PEFL=0

for arg in "$@"; do
    case "$arg" in
        --persist)
            PERSIST_BOOT=1
            ;;
        --accept-pefl)
            ACCEPT_PEFL=1
            ;;
        --video-nr=*)
            VIDEO_NR="${arg#*=}"
            ;;
    esac
done

# ==============================================================================
# 1. PEFL v1.0 Clause 3 Mandatory Startup Verification
# ==============================================================================
verify_pefl_license() {
    local now_ts
    now_ts="$(date +%s)"
    local max_age=$((7 * 24 * 60 * 60))

    if (( ACCEPT_PEFL == 1 )); then
        echo "$now_ts" > "$CACHE_FILE" 2>/dev/null || true
        return 0
    fi

    if [[ -f "$CACHE_FILE" ]]; then
        local last_ts
        last_ts="$(head -n 1 "$CACHE_FILE" 2>/dev/null | tr -d '[:space:]' || echo 0)"
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
            echo "$now_ts" > "$CACHE_FILE" 2>/dev/null || true
            echo "[SmartCam] License accepted."
            ;;
        *)
            echo "[SmartCam] License rejected. Exiting."
            exit 1
            ;;
    esac
}

verify_pefl_license

# ==============================================================================
# 2. Check & Install v4l2loopback-dkms if missing
# ==============================================================================
SUDO=""
if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
    SUDO="sudo"
fi

if ! modinfo v4l2loopback >/dev/null 2>&1; then
    echo "[SmartCam] Kernel module 'v4l2loopback' not found. Attempting package installation..."
    if command -v apt-get >/dev/null 2>&1; then
        $SUDO apt-get update && $SUDO apt-get install -y v4l2loopback-dkms v4l2loopback-utils v4l-utils
    elif command -v dnf >/dev/null 2>&1; then
        $SUDO dnf install -y v4l2loopback v4l-utils
    elif command -v pacman >/dev/null 2>&1; then
        $SUDO pacman -Sy --noconfirm v4l2loopback-dkms v4l-utils
    else
        echo "[ERROR] Please install 'v4l2loopback-dkms' using your distribution package manager." >&2
        exit 1
    fi
fi

# ==============================================================================
# 3. Load v4l2loopback with "SmartCam" label and exclusive_caps=1 for WebRTC
# ==============================================================================
if lsmod | grep -q "^v4l2loopback"; then
    echo "[SmartCam] Reloading existing v4l2loopback module..."
    $SUDO modprobe -r v4l2loopback || true
fi

echo "[SmartCam] Loading v4l2loopback on /dev/video${VIDEO_NR} (card_label=\"${CARD_LABEL}\", exclusive_caps=1)..."
$SUDO modprobe v4l2loopback \
    devices=1 \
    video_nr="${VIDEO_NR}" \
    card_label="${CARD_LABEL}" \
    exclusive_caps=1

DEV_NODE="/dev/video${VIDEO_NR}"
if [[ -e "$DEV_NODE" ]]; then
    $SUDO chmod 0666 "$DEV_NODE" || true
    echo "[SmartCam] Virtual Camera device ready: ${DEV_NODE} -> \"${CARD_LABEL}\""
else
    echo "[WARNING] ${DEV_NODE} was not found immediately; check 'v4l2-ctl --list-devices'." >&2
fi

# ==============================================================================
# 4. Optional Persistent Configuration Across Reboots (--persist)
# ==============================================================================
if (( PERSIST_BOOT == 1 )); then
    echo "[SmartCam] Writing persistent boot configuration to /etc/modules-load.d/smartcam.conf..."
    echo "v4l2loopback" | $SUDO tee /etc/modules-load.d/smartcam.conf >/dev/null
    echo "options v4l2loopback devices=1 video_nr=${VIDEO_NR} card_label=\"${CARD_LABEL}\" exclusive_caps=1" \
        | $SUDO tee /etc/modprobe.d/smartcam-v4l2loopback.conf >/dev/null
    echo "[SmartCam] Persistent boot configuration installed."
fi
