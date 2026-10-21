/*
 * ============================================================================
 *  Project: SmartCam - AI-Powered Virtual Camera & Aspect Ratio Studio
 *  File:    app/web_preview/app.js
 *
 *  Copyright (c) 2026 Rubens Rainelli (Rubens). All rights reserved.
 *
 *  Licensed under the Personal, Educational & Fair-Use License v1.0 (PEFL v1.0).
 *  - Free for personal, academic, educational, and non-commercial research use.
 *  - Commercial use requires a separate written commercial license from the
 *    copyright holder if annual revenue/funding exceeds EUR 10,000.
 *  - Redistribution or public deployment must retain this header, the interactive
 *    Clause 3 EULA approval modal on first startup, and the LICENSE.md file.
 * ============================================================================
 */

(function () {
    "use strict";

    // Riferimenti DOM — Selettori e Video
    const selectDeviceA = document.getElementById("select-device-a");
    const selectDeviceB = document.getElementById("select-device-b");
    const selectPresetA = document.getElementById("select-preset-a");
    const selectPresetB = document.getElementById("select-preset-b");

    const irAlertA = document.getElementById("ir-alert-a");
    const irAlertB = document.getElementById("ir-alert-b");
    const btnSwitchRgbA = document.getElementById("btn-switch-rgb-a");
    const btnSwitchRgbB = document.getElementById("btn-switch-rgb-b");

    const videoA = document.getElementById("video-a");
    const videoB = document.getElementById("video-b");
    const videoWipeA = document.getElementById("video-wipe-a");
    const videoWipeB = document.getElementById("video-wipe-b");

    const placeholderA = document.getElementById("placeholder-a");
    const placeholderB = document.getElementById("placeholder-b");

    const statusBadgeA = document.getElementById("status-badge-a");
    const statusBadgeB = document.getElementById("status-badge-b");
    const devicesCountBadge = document.getElementById("devices-count-badge");
    const cropQualityBadge = document.getElementById("crop-quality-badge");
    const peflStatusBadge = document.getElementById("pefl-status-badge");

    // Riferimenti DOM — Guida Assistita Windows 11 (Punto 7)
    const win11HelpPanel = document.getElementById("win11-help-panel");
    const win11HelpIcon = document.getElementById("win11-help-icon");
    const win11HelpTitle = document.getElementById("win11-help-title");
    const win11HelpSubtitle = document.getElementById("win11-help-subtitle");
    const btnCloseWin11Help = document.getElementById("btn-close-win11-help");
    const btnRetryPermissions = document.getElementById("btn-retry-permissions");

    // Riferimenti DOM — Indicatore Qualità Ritaglio 4:3 vs 16:9 & Preset Rapidi
    const calibrationDot = document.getElementById("calibration-status-dot");
    const calibrationHeadline = document.getElementById("calibration-headline");
    const calibrationDetail = document.getElementById("calibration-detail");
    const presetBtnFhd4x3 = document.getElementById("preset-btn-fhd-4x3");
    const presetBtnHd4x3 = document.getElementById("preset-btn-hd-4x3");
    const presetBtnNative = document.getElementById("preset-btn-native");

    // HUD & Telemetria
    const hudNameA = document.getElementById("hud-name-a");
    const hudNameB = document.getElementById("hud-name-b");
    const hudResA = document.getElementById("hud-res-a");
    const hudResB = document.getElementById("hud-res-b");

    const metricResA = document.getElementById("metric-res-a");
    const metricArA = document.getElementById("metric-ar-a");
    const metricFpsA = document.getElementById("metric-fps-a");
    const metricStateA = document.getElementById("metric-state-a");

    const metricResB = document.getElementById("metric-res-b");
    const metricArB = document.getElementById("metric-ar-b");
    const metricFpsB = document.getElementById("metric-fps-b");
    const metricStateB = document.getElementById("metric-state-b");

    // Pulsanti Azione
    const btnStartBoth = document.getElementById("btn-start-both");
    const btnStopBoth = document.getElementById("btn-stop-both");
    const btnRefreshDevices = document.getElementById("btn-refresh-devices");
    const btnSwapAB = document.getElementById("btn-swap-ab");
    const btnAutoCalibrate = document.getElementById("btn-auto-calibrate");
    const btnRestartA = document.getElementById("btn-restart-a");
    const btnRestartB = document.getElementById("btn-restart-b");

    const btnToggleMirror = document.getElementById("btn-toggle-mirror");
    const btnToggleGrid = document.getElementById("btn-toggle-grid");
    const btnToggleWipe = document.getElementById("btn-toggle-wipe");
    const btnSnapshotPng = document.getElementById("btn-snapshot-png");

    // Elementi Wipe & Griglia
    const gridOverlayA = document.getElementById("grid-overlay-a");
    const gridOverlayB = document.getElementById("grid-overlay-b");
    const wipeGridOverlay = document.getElementById("wipe-grid-overlay");
    const wipeSliderStage = document.getElementById("wipe-slider-stage");
    const wipeLayerBefore = document.getElementById("wipe-layer-before");
    const wipeDividerLine = document.getElementById("wipe-divider-line");
    const wipeRangeSlider = document.getElementById("wipe-range-slider");

    // Stato interno
    const state = {
        streamA: null,
        streamB: null,
        videoInputs: [],
        mirrored: false,
        gridVisible: false,
        wipeMode: false,
        fpsTrackerA: { frames: 0, lastTimestamp: performance.now(), currentFps: 0 },
        fpsTrackerB: { frames: 0, lastTimestamp: performance.now(), currentFps: 0 }
    };

    // Rilevamento Telecamera a Infrarossi (IR / Windows Hello) — Punto 5
    function isInfraredCameraLabel(label) {
        if (!label) return false;
        return /(?:^|[\s(_\-/])IR(?:$|[\s)_/\-])|Infrared|Windows\s*Hello/i.test(label);
    }

    function isVirtualCameraLabel(label) {
        if (!label) return false;
        return /smartcam|virtual|obs/i.test(label);
    }

    function formatDeviceDisplayLabel(rawLabel, idx) {
        const baseLabel = (rawLabel && rawLabel.trim()) ? rawLabel.trim() : `Webcam #${idx + 1}`;
        if (isInfraredCameraLabel(baseLabel)) {
            return `${baseLabel} ⚠️ [Sensore Infrarossi IR]`;
        }
        return baseLabel;
    }

    // Mostra o nasconde il Banner Alert Giallo Non Bloccante sotto il selettore Camera A o Camera B
    function updateIrWarningBanner(slot) {
        const isA = slot === "A";
        const selectEl = isA ? selectDeviceA : selectDeviceB;
        const alertEl = isA ? irAlertA : irAlertB;
        const switchBtn = isA ? btnSwitchRgbA : btnSwitchRgbB;

        if (!selectEl || !alertEl) return;

        const selectedOpt = selectEl.options[selectEl.selectedIndex];
        const optText = selectedOpt ? selectedOpt.text : "";
        const isIr = isInfraredCameraLabel(optText);

        alertEl.classList.toggle("hidden", !isIr);

        if (switchBtn) {
            const hasColorAlt = state.videoInputs.some(
                (d) => !isInfraredCameraLabel(d.label) && !isVirtualCameraLabel(d.label)
            );
            switchBtn.classList.toggle("hidden", !hasColorAlt);
        }
    }

    // Passa rapidamente alla prima webcam HD a colori (non-IR) disponibile
    async function switchSlotToColorCamera(slot) {
        const isA = slot === "A";
        const selectEl = isA ? selectDeviceA : selectDeviceB;

        let rgbIdx = state.videoInputs.findIndex(
            (d) => !isInfraredCameraLabel(d.label) && !isVirtualCameraLabel(d.label)
        );
        if (rgbIdx === -1) {
            rgbIdx = state.videoInputs.findIndex((d) => !isInfraredCameraLabel(d.label));
        }

        if (rgbIdx !== -1 && selectEl.options[rgbIdx]) {
            selectEl.selectedIndex = rgbIdx;
            updateIrWarningBanner(slot);
            await startCameraSlot(slot);
        }
    }

    // Mostra il Pannello Guida Assistita Permessi e Conflitti Windows 11 (Punto 7)
    function showWindows11HelpPanel(err, slot) {
        if (!win11HelpPanel) return;

        const errName = (err && err.name) ? err.name : "";
        win11HelpPanel.classList.remove("hidden");

        if (errName === "NotAllowedError" || errName === "PermissionDeniedError" || errName === "SecurityError") {
            if (win11HelpIcon) win11HelpIcon.textContent = "🔒";
            if (win11HelpTitle) {
                win11HelpTitle.textContent = `Permesso Fotocamera Negato su Camera ${slot} (NotAllowedError)`;
            }
            if (win11HelpSubtitle) {
                win11HelpSubtitle.textContent =
                    "Windows 11 o il tuo browser hanno bloccato l'accesso alla webcam. Segui i 3 passaggi qui sotto o apri direttamente le Impostazioni Privacy di Windows 11.";
            }
        } else if (errName === "NotReadableError" || errName === "TrackStartError" || errName === "AbortError") {
            if (win11HelpIcon) win11HelpIcon.textContent = "⚠️";
            if (win11HelpTitle) {
                win11HelpTitle.textContent = `Webcam Già Occupata da un'Altra Applicazione su Camera ${slot} (NotReadableError)`;
            }
            if (win11HelpSubtitle) {
                win11HelpSubtitle.textContent =
                    "La webcam fisica è già in uso esclusivo (es. da Microsoft Teams, Zoom o dall'app desktop SmartCam). Su Camera B seleziona 'SmartCam' per condividere il flusso senza conflitti!";
            }
        } else {
            if (win11HelpIcon) win11HelpIcon.textContent = "🛡️";
            if (win11HelpTitle) {
                win11HelpTitle.textContent = `Impossibile Avviare Camera ${slot} (${errName || "Errore Dispositivo"})`;
            }
            if (win11HelpSubtitle) {
                win11HelpSubtitle.textContent =
                    "Controlla che la webcam sia collegata, che i permessi Privacy di Windows 11 siano attivi e che nessun'altra app blocchi il sensore.";
            }
        }
    }

    // Calcolo MCD per semplificare l'Aspect Ratio
    function gcd(a, b) {
        return b === 0 ? a : gcd(b, a % b);
    }

    function formatAspectRatio(width, height) {
        if (!width || !height) return "--";
        const ratio = width / height;
        if (Math.abs(ratio - 16 / 9) < 0.03) return "16:9 (Widescreen)";
        if (Math.abs(ratio - 4 / 3) < 0.03) return "4:3 (Standard)";
        if (Math.abs(ratio - 1) < 0.02) return "1:1 (Square)";
        const divisor = gcd(width, height);
        return `${Math.round(width / divisor)}:${Math.round(height / divisor)} (${ratio.toFixed(2)})`;
    }

    // Costruzione vincoli getUserMedia per singolo slot
    function buildMediaConstraints(deviceId, presetValue) {
        const videoConstraints = {};
        if (deviceId) {
            videoConstraints.deviceId = { exact: deviceId };
        }
        if (presetValue && presetValue !== "auto") {
            const parts = presetValue.split("x").map(Number);
            if (parts.length === 2 && parts[0] > 0 && parts[1] > 0) {
                videoConstraints.width = { ideal: parts[0] };
                videoConstraints.height = { ideal: parts[1] };
            }
        }
        return {
            audio: false,
            video: Object.keys(videoConstraints).length > 0 ? videoConstraints : true
        };
    }

    // Richiesta permessi iniziale ed enumerazione delle webcam con filtro intelligente IR
    async function enumerateAndPopulateCameras(autoSelectDefaults = true) {
        if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) {
            alert("Il browser corrente non supporta le API WebRTC MediaDevices.");
            return;
        }

        // Sblocca le etichette dei dispositivi chiedendo un permesso rapido se necessario
        try {
            const initDevices = await navigator.mediaDevices.enumerateDevices();
            const hasLabels = initDevices.some((d) => d.kind === "videoinput" && d.label);
            if (!hasLabels) {
                const tempStream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
                tempStream.getTracks().forEach((t) => t.stop());
            }
        } catch (err) {
            console.warn("[SmartCam Preview] Permesso iniziale webcam non concesso o dispositivo occupato:", err);
            if (err && (err.name === "NotAllowedError" || err.name === "NotReadableError")) {
                showWindows11HelpPanel(err, "A/B");
            }
        }

        const allDevices = await navigator.mediaDevices.enumerateDevices();
        const videoInputs = allDevices.filter((d) => d.kind === "videoinput");
        state.videoInputs = videoInputs;

        if (devicesCountBadge) {
            const irCount = videoInputs.filter((d) => isInfraredCameraLabel(d.label)).length;
            devicesCountBadge.textContent =
                irCount > 0
                    ? `Webcam rilevate: ${videoInputs.length} (${irCount} IR)`
                    : `Webcam rilevate: ${videoInputs.length}`;
        }

        const prevA = selectDeviceA.value;
        const prevB = selectDeviceB.value;

        selectDeviceA.innerHTML = "";
        selectDeviceB.innerHTML = "";

        if (videoInputs.length === 0) {
            selectDeviceA.add(new Option("Nessuna webcam rilevata", ""));
            selectDeviceB.add(new Option("Nessuna webcam rilevata", ""));
            updateIrWarningBanner("A");
            updateIrWarningBanner("B");
            return;
        }

        videoInputs.forEach((device, idx) => {
            const displayLabel = formatDeviceDisplayLabel(device.label, idx);
            selectDeviceA.add(new Option(displayLabel, device.deviceId));
            selectDeviceB.add(new Option(displayLabel, device.deviceId));
        });

        if (!autoSelectDefaults && (prevA || prevB)) {
            if (prevA) selectDeviceA.value = prevA;
            if (prevB) selectDeviceB.value = prevB;
            updateIrWarningBanner("A");
            updateIrWarningBanner("B");
            return;
        }

        // 1. Ricerca automatica di "SmartCam" (o OBS) per Camera B
        let smartCamIndex = videoInputs.findIndex((d) => /smartcam/i.test(d.label));
        if (smartCamIndex === -1) {
            smartCamIndex = videoInputs.findIndex((d) => /virtual|obs/i.test(d.label));
        }

        // 2. Ricerca automatica della prima webcam hardware A COLORI (NON-IR e NON-virtuale) per Camera A
        let colorHardwareIndex = videoInputs.findIndex(
            (d, idx) =>
                idx !== smartCamIndex &&
                !isVirtualCameraLabel(d.label) &&
                !isInfraredCameraLabel(d.label)
        );

        // Fallback se tutte le webcam fisiche sono IR o non etichettate
        if (colorHardwareIndex === -1) {
            colorHardwareIndex = videoInputs.findIndex(
                (d, idx) => idx !== smartCamIndex && !isVirtualCameraLabel(d.label)
            );
        }
        if (colorHardwareIndex === -1) {
            colorHardwareIndex = 0;
        }

        selectDeviceA.selectedIndex = colorHardwareIndex;

        if (smartCamIndex !== -1) {
            selectDeviceB.selectedIndex = smartCamIndex;
        } else {
            // Se SmartCam non è ancora registrata, scegli un'altra webcam non-IR oppure la stessa a colori
            const secondColorIdx = videoInputs.findIndex(
                (d, idx) => idx !== colorHardwareIndex && !isInfraredCameraLabel(d.label)
            );
            selectDeviceB.selectedIndex = secondColorIdx !== -1 ? secondColorIdx : colorHardwareIndex;
        }

        updateIrWarningBanner("A");
        updateIrWarningBanner("B");
    }

    // Ferma uno stream specifico ("A" oppure "B")
    function stopCameraSlot(slot) {
        const isA = slot === "A";
        const currentStream = isA ? state.streamA : state.streamB;
        const videoEl = isA ? videoA : videoB;
        const wipeVideoEl = isA ? videoWipeA : videoWipeB;
        const placeholderEl = isA ? placeholderA : placeholderB;
        const badgeEl = isA ? statusBadgeA : statusBadgeB;
        const hudNameEl = isA ? hudNameA : hudNameB;
        const hudResEl = isA ? hudResA : hudResB;
        const metricResEl = isA ? metricResA : metricResB;
        const metricArEl = isA ? metricArA : metricArB;
        const metricFpsEl = isA ? metricFpsA : metricFpsB;
        const metricStateEl = isA ? metricStateA : metricStateB;

        if (currentStream) {
            currentStream.getTracks().forEach((track) => track.stop());
        }
        if (isA) {
            state.streamA = null;
            state.fpsTrackerA.currentFps = 0;
        } else {
            state.streamB = null;
            state.fpsTrackerB.currentFps = 0;
        }

        videoEl.srcObject = null;
        wipeVideoEl.srcObject = null;
        placeholderEl.classList.remove("hidden");

        badgeEl.textContent = "INATTIVA";
        badgeEl.classList.remove("live", "error");
        hudNameEl.textContent = `CAMERA ${slot}: Non avviata`;
        hudResEl.textContent = "-- × --";
        metricResEl.textContent = "-- × --";
        metricArEl.textContent = "--";
        metricFpsEl.textContent = "-- fps";
        metricStateEl.textContent = "Inattivo";
    }

    // Avvia uno stream specifico ("A" oppure "B")
    async function startCameraSlot(slot) {
        stopCameraSlot(slot);
        updateIrWarningBanner(slot);

        const isA = slot === "A";
        const selectDev = isA ? selectDeviceA : selectDeviceB;
        const selectPreset = isA ? selectPresetA : selectPresetB;
        const videoEl = isA ? videoA : videoB;
        const wipeVideoEl = isA ? videoWipeA : videoWipeB;
        const placeholderEl = isA ? placeholderA : placeholderB;
        const badgeEl = isA ? statusBadgeA : statusBadgeB;
        const hudNameEl = isA ? hudNameA : hudNameB;
        const metricStateEl = isA ? metricStateA : metricStateB;

        const deviceId = selectDev.value;
        const deviceLabel = selectDev.options[selectDev.selectedIndex]
            ? selectDev.options[selectDev.selectedIndex].text
            : `Camera ${slot}`;
        const presetValue = selectPreset.value;

        try {
            badgeEl.textContent = "AVVIO...";
            const constraints = buildMediaConstraints(deviceId, presetValue);
            const stream = await navigator.mediaDevices.getUserMedia(constraints);

            if (isA) {
                state.streamA = stream;
            } else {
                state.streamB = stream;
            }

            videoEl.srcObject = stream;
            wipeVideoEl.srcObject = stream;
            await videoEl.play().catch(() => {});
            await wipeVideoEl.play().catch(() => {});

            placeholderEl.classList.add("hidden");
            badgeEl.textContent = "LIVE";
            badgeEl.classList.remove("error");
            badgeEl.classList.add("live");
            hudNameEl.textContent = `CAMERA ${slot}: ${deviceLabel}`;
            metricStateEl.textContent = "Attivo (Streaming)";

            attachFrameRateCounter(slot, videoEl);
        } catch (err) {
            console.error(`[SmartCam Preview] Errore avvio Camera ${slot}:`, err);
            badgeEl.textContent = "ERRORE";
            badgeEl.classList.remove("live");
            badgeEl.classList.add("error");
            metricStateEl.textContent = `Errore: ${err.name || "Accesso negato"}`;

            // Mostra la Guida Assistita Windows 11 per permessi negati o webcam occupata
            showWindows11HelpPanel(err, slot);
        }
    }

    // Contatore FPS reale tramite requestVideoFrameCallback (con fallback)
    function attachFrameRateCounter(slot, videoEl) {
        const tracker = slot === "A" ? state.fpsTrackerA : state.fpsTrackerB;
        tracker.frames = 0;
        tracker.lastTimestamp = performance.now();

        if ("requestVideoFrameCallback" in HTMLVideoElement.prototype) {
            const onFrame = (now) => {
                const activeStream = slot === "A" ? state.streamA : state.streamB;
                if (!activeStream) return;

                tracker.frames += 1;
                const elapsed = now - tracker.lastTimestamp;
                if (elapsed >= 500) {
                    tracker.currentFps = (tracker.frames * 1000) / elapsed;
                    tracker.frames = 0;
                    tracker.lastTimestamp = now;
                }
                videoEl.requestVideoFrameCallback(onFrame);
            };
            videoEl.requestVideoFrameCallback(onFrame);
        } else {
            const fallbackInterval = setInterval(() => {
                const activeStream = slot === "A" ? state.streamA : state.streamB;
                if (!activeStream) {
                    clearInterval(fallbackInterval);
                    return;
                }
                const track = activeStream.getVideoTracks()[0];
                const settings = track && track.getSettings ? track.getSettings() : {};
                tracker.currentFps = settings.frameRate || 30.0;
            }, 500);
        }
    }

    // Valuta la qualità del ritaglio 4:3 vs 16:9 tra Camera A e Camera B
    function updateCropQualityIndicator() {
        const hasA = Boolean(state.streamA && videoA.videoWidth && videoA.videoHeight);
        const hasB = Boolean(state.streamB && videoB.videoWidth && videoB.videoHeight);

        if (!hasA && !hasB) {
            if (cropQualityBadge) cropQualityBadge.textContent = "Ritaglio: In attesa flussi";
            if (calibrationDot) calibrationDot.className = "calibration-dot";
            if (calibrationHeadline) {
                calibrationHeadline.textContent = "Analisi Geometria & Ritaglio 4:3 vs 16:9";
            }
            if (calibrationDetail) {
                calibrationDetail.textContent =
                    "Avvia i flussi video per verificare l'assenza di stiramento (stretching) tra sorgente 16:9 e output 4:3.";
            }
            return;
        }

        const wA = hasA ? videoA.videoWidth : 0;
        const hA = hasA ? videoA.videoHeight : 0;
        const wB = hasB ? videoB.videoWidth : 0;
        const hB = hasB ? videoB.videoHeight : 0;

        const ratioA = hasA ? wA / hA : 0;
        const ratioB = hasB ? wB / hB : 0;

        const isA16x9 = hasA && Math.abs(ratioA - 16 / 9) < 0.04;
        const isB4x3 = hasB && Math.abs(ratioB - 4 / 3) < 0.04;

        if (isA16x9 && isB4x3) {
            if (cropQualityBadge) cropQualityBadge.textContent = `✓ Center-Crop 4:3 Attivo (${wB}×${hB})`;
            if (calibrationDot) calibrationDot.className = "calibration-dot optimal";
            if (calibrationHeadline) {
                calibrationHeadline.textContent = `Ritaglio Center-Crop 4:3 Ottimale (${wA}×${hA} → ${wB}×${hB})`;
            }
            if (calibrationDetail) {
                calibrationDetail.textContent =
                    "Proporzioni 1:1 preservate: i bordi laterali 16:9 sono stati ritagliati simmetricamente mantenendo la nitidezza verticale.";
            }
        } else if (hasA && hasB) {
            const arAShort = formatAspectRatio(wA, hA).split(" ")[0];
            const arBShort = formatAspectRatio(wB, hB).split(" ")[0];
            if (cropQualityBadge) cropQualityBadge.textContent = `Confronto: ${arAShort} vs ${arBShort}`;
            if (calibrationDot) calibrationDot.className = "calibration-dot optimal";
            if (calibrationHeadline) {
                calibrationHeadline.textContent = `Confronto Attivo: Camera A (${wA}×${hA}, ${arAShort}) vs Camera B (${wB}×${hB}, ${arBShort})`;
            }
            if (calibrationDetail) {
                calibrationDetail.textContent =
                    "Usa i 'Preset Rapidi' a destra o il pulsante 'Griglia Aspect Ratio' per verificare l'inquadratura 4:3 dentro 16:9.";
            }
        } else {
            if (cropQualityBadge) cropQualityBadge.textContent = "Ritaglio: 1 solo flusso attivo";
            if (calibrationDot) calibrationDot.className = "calibration-dot warning";
        }
    }

    // Loop di aggiornamento Telemetria Live
    function startTelemetryLoop() {
        function updateSlotTelemetry(slot) {
            const isA = slot === "A";
            const stream = isA ? state.streamA : state.streamB;
            const videoEl = isA ? videoA : videoB;
            const tracker = isA ? state.fpsTrackerA : state.fpsTrackerB;
            const hudResEl = isA ? hudResA : hudResB;
            const metricResEl = isA ? metricResA : metricResB;
            const metricArEl = isA ? metricArA : metricArB;
            const metricFpsEl = isA ? metricFpsA : metricFpsB;
            const metricStateEl = isA ? metricStateA : metricStateB;

            if (!stream || !videoEl.videoWidth || !videoEl.videoHeight) {
                return;
            }

            const w = videoEl.videoWidth;
            const h = videoEl.videoHeight;
            const arText = formatAspectRatio(w, h);
            const fpsText = tracker.currentFps > 0 ? `${tracker.currentFps.toFixed(1)} fps` : "-- fps";

            hudResEl.textContent = `${w} × ${h} (${arText.split(" ")[0]})`;
            metricResEl.textContent = `${w} × ${h} px`;
            metricArEl.textContent = arText;
            metricFpsEl.textContent = fpsText;

            const track = stream.getVideoTracks()[0];
            if (track) {
                metricStateEl.textContent = track.readyState === "live" ? "Attivo (LIVE)" : track.readyState;
            }
        }

        setInterval(() => {
            updateSlotTelemetry("A");
            updateSlotTelemetry("B");
            updateCropQualityIndicator();
        }, 300);
    }

    // Controlli UI: Mirror, Griglia, Wipe Slider, Swap A/B, Auto-Calibrazione, Snapshot PNG
    function toggleMirror() {
        state.mirrored = !state.mirrored;
        btnToggleMirror.classList.toggle("active", state.mirrored);
        [videoA, videoB, videoWipeA, videoWipeB].forEach((v) => {
            v.classList.toggle("mirrored", state.mirrored);
        });
    }

    function toggleGridOverlay() {
        state.gridVisible = !state.gridVisible;
        btnToggleGrid.classList.toggle("active", state.gridVisible);
        [gridOverlayA, gridOverlayB, wipeGridOverlay].forEach((g) => {
            g.classList.toggle("hidden", !state.gridVisible);
        });
    }

    function toggleWipeMode() {
        state.wipeMode = !state.wipeMode;
        btnToggleWipe.classList.toggle("active", state.wipeMode);
        wipeSliderStage.classList.toggle("hidden", !state.wipeMode);
    }

    function updateWipeSliderPosition(percentValue) {
        const clamped = Math.max(0, Math.min(100, Number(percentValue)));
        const rightInset = 100 - clamped;
        wipeLayerBefore.style.clipPath = `inset(0 ${rightInset}% 0 0)`;
        wipeDividerLine.style.left = `${clamped}%`;
    }

    async function swapCamerasAB() {
        const tempDev = selectDeviceA.value;
        const tempPreset = selectPresetA.value;
        selectDeviceA.value = selectDeviceB.value;
        selectPresetA.value = selectPresetB.value;
        selectDeviceB.value = tempDev;
        selectPresetB.value = tempPreset;

        updateIrWarningBanner("A");
        updateIrWarningBanner("B");
        await Promise.all([startCameraSlot("A"), startCameraSlot("B")]);
    }

    async function applyQuickPreset(presetAVal, presetBVal, enableGrid = false) {
        selectPresetA.value = presetAVal;
        selectPresetB.value = presetBVal;
        if (enableGrid && !state.gridVisible) {
            toggleGridOverlay();
        }
        await Promise.all([startCameraSlot("A"), startCameraSlot("B")]);
    }

    // Esporta uno Snapshot Comparativo Side-by-Side ad alta risoluzione in PNG
    function captureComparativeSnapshot() {
        const panelWidth = 960;
        const panelHeight = 540;
        const headerHeight = 90;
        const footerHeight = 90;
        const gap = 24;
        const totalWidth = panelWidth * 2 + gap * 3;
        const totalHeight = headerHeight + panelHeight + footerHeight;

        const canvas = document.createElement("canvas");
        canvas.width = totalWidth;
        canvas.height = totalHeight;
        const ctx = canvas.getContext("2d");

        // Sfondo Studio
        ctx.fillStyle = "#0b0e14";
        ctx.fillRect(0, 0, totalWidth, totalHeight);

        // Titolo Snapshot
        ctx.fillStyle = "#f1f5f9";
        ctx.font = "bold 24px Segoe UI, sans-serif";
        ctx.fillText("SmartCam Studio — Side-by-Side Comparison Snapshot", gap, 42);

        ctx.fillStyle = "#94a3b8";
        ctx.font = "15px Consolas, monospace";
        ctx.fillText(`Timestamp: ${new Date().toLocaleString("it-IT")} | PEFL v1.0 Licensed`, gap, 68);

        function drawVideoSlot(videoEl, xOffset, title, resText, arText, fpsText, borderColor) {
            ctx.fillStyle = "#000000";
            ctx.fillRect(xOffset, headerHeight, panelWidth, panelHeight);
            ctx.strokeStyle = borderColor;
            ctx.lineWidth = 2;
            ctx.strokeRect(xOffset, headerHeight, panelWidth, panelHeight);

            if (videoEl && videoEl.videoWidth > 0 && videoEl.videoHeight > 0) {
                const vw = videoEl.videoWidth;
                const vh = videoEl.videoHeight;
                const scale = Math.min(panelWidth / vw, panelHeight / vh);
                const drawW = vw * scale;
                const drawH = vh * scale;
                const drawX = xOffset + (panelWidth - drawW) / 2;
                const drawY = headerHeight + (panelHeight - drawH) / 2;

                ctx.save();
                if (state.mirrored) {
                    ctx.translate(drawX + drawW, drawY);
                    ctx.scale(-1, 1);
                    ctx.drawImage(videoEl, 0, 0, drawW, drawH);
                } else {
                    ctx.drawImage(videoEl, drawX, drawY, drawW, drawH);
                }
                ctx.restore();
            } else {
                ctx.fillStyle = "#64748b";
                ctx.font = "18px Segoe UI, sans-serif";
                ctx.fillText("Nessun flusso video attivo", xOffset + panelWidth / 2 - 110, headerHeight + panelHeight / 2);
            }

            const metaY = headerHeight + panelHeight + 32;
            ctx.fillStyle = borderColor;
            ctx.font = "bold 17px Segoe UI, sans-serif";
            ctx.fillText(title, xOffset, metaY);

            ctx.fillStyle = "#e2e8f0";
            ctx.font = "15px Consolas, monospace";
            ctx.fillText(`Risoluzione: ${resText}   |   Aspect Ratio: ${arText}   |   FPS: ${fpsText}`, xOffset, metaY + 26);
        }

        drawVideoSlot(
            videoA,
            gap,
            hudNameA.textContent,
            metricResA.textContent,
            metricArA.textContent,
            metricFpsA.textContent,
            "#f59e0b"
        );

        drawVideoSlot(
            videoB,
            gap * 2 + panelWidth,
            hudNameB.textContent,
            metricResB.textContent,
            metricArB.textContent,
            metricFpsB.textContent,
            "#06b6d4"
        );

        const dataUrl = canvas.toDataURL("image/png");
        const link = document.createElement("a");
        const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
        link.download = `SmartCam_Comparison_${stamp}.png`;
        link.href = dataUrl;
        link.click();
    }

    // Event Listeners
    function bindEvents() {
        btnStartBoth.addEventListener("click", async () => {
            await Promise.all([startCameraSlot("A"), startCameraSlot("B")]);
        });

        btnStopBoth.addEventListener("click", () => {
            stopCameraSlot("A");
            stopCameraSlot("B");
        });

        btnRefreshDevices.addEventListener("click", async () => {
            await enumerateAndPopulateCameras(false);
        });

        btnSwapAB.addEventListener("click", swapCamerasAB);

        if (btnAutoCalibrate) {
            btnAutoCalibrate.addEventListener("click", async () => {
                await enumerateAndPopulateCameras(true);
                await applyQuickPreset("1920x1080", "1440x1080", true);
            });
        }

        btnRestartA.addEventListener("click", () => startCameraSlot("A"));
        btnRestartB.addEventListener("click", () => startCameraSlot("B"));

        selectDeviceA.addEventListener("change", () => {
            updateIrWarningBanner("A");
            startCameraSlot("A");
        });
        selectDeviceB.addEventListener("change", () => {
            updateIrWarningBanner("B");
            startCameraSlot("B");
        });
        selectPresetA.addEventListener("change", () => startCameraSlot("A"));
        selectPresetB.addEventListener("change", () => startCameraSlot("B"));

        if (btnSwitchRgbA) {
            btnSwitchRgbA.addEventListener("click", () => switchSlotToColorCamera("A"));
        }
        if (btnSwitchRgbB) {
            btnSwitchRgbB.addEventListener("click", () => switchSlotToColorCamera("B"));
        }

        if (btnCloseWin11Help && win11HelpPanel) {
            btnCloseWin11Help.addEventListener("click", () => {
                win11HelpPanel.classList.add("hidden");
            });
        }
        if (btnRetryPermissions) {
            btnRetryPermissions.addEventListener("click", async () => {
                if (win11HelpPanel) win11HelpPanel.classList.add("hidden");
                await enumerateAndPopulateCameras(true);
                await Promise.all([startCameraSlot("A"), startCameraSlot("B")]);
            });
        }

        if (presetBtnFhd4x3) {
            presetBtnFhd4x3.addEventListener("click", () => applyQuickPreset("1920x1080", "1440x1080", true));
        }
        if (presetBtnHd4x3) {
            presetBtnHd4x3.addEventListener("click", () => applyQuickPreset("1280x720", "960x720", true));
        }
        if (presetBtnNative) {
            presetBtnNative.addEventListener("click", () => applyQuickPreset("auto", "auto", false));
        }

        btnToggleMirror.addEventListener("click", toggleMirror);
        btnToggleGrid.addEventListener("click", toggleGridOverlay);
        btnToggleWipe.addEventListener("click", toggleWipeMode);
        btnSnapshotPng.addEventListener("click", captureComparativeSnapshot);

        wipeRangeSlider.addEventListener("input", (e) => {
            updateWipeSliderPosition(e.target.value);
        });

        if (peflStatusBadge) {
            peflStatusBadge.addEventListener("click", () => {
                window.PEFLWebGuard.enforce(() => {}, true);
            });
        }

        if (navigator.mediaDevices && navigator.mediaDevices.addEventListener) {
            navigator.mediaDevices.addEventListener("devicechange", () => {
                enumerateAndPopulateCameras(false);
            });
        }
    }

    // Inizializzazione vincolata all'approvazione PEFL v1.0 Clausola 3
    document.addEventListener("DOMContentLoaded", () => {
        bindEvents();
        startTelemetryLoop();

        window.PEFLWebGuard.enforce(async () => {
            await enumerateAndPopulateCameras(true);
            await Promise.all([startCameraSlot("A"), startCameraSlot("B")]);
        });
    });
})();
