/*
 * ============================================================================
 *  Project: SmartCam - AI-Powered Virtual Camera & Aspect Ratio Studio
 *  File:    app/web_preview/pefl_web_guard.js
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

    const PEFL_STORAGE_KEY = "smartcam_pefl_v1_approval";
    const PEFL_TTL_MS = 7 * 24 * 60 * 60 * 1000; // 7 giorni

    const PEFL_LICENSE_TEXT = `PERSONAL, EDUCATIONAL & FAIR-USE LICENSE v1.0 (PEFL v1.0)
Copyright (c) 2026 Rubens Rainelli (Rubens). Tutti i diritti riservati.

CLAUSOLA 1 — USO PERSONALE, EDUCATIVO E DI RICERCA GRATUITO
È concessa gratuitamente a chiunque una licenza non esclusiva per utilizzare, studiare, modificare e testare SmartCam (inclusa questa Web Preview Testbench Side-by-Side) per scopi strettamente personali, didattici, accademici o di ricerca non commerciale.

CLAUSOLA 2 — SOGLIA D'USO COMMERCIALE (FAIR-USE)
L'utilizzo in ambito professionale o aziendale è consentito gratuitamente solo a ditte individuali, liberi professionisti o enti con fatturato o finanziamento annuo lordo inferiore a EUR 10.000,00. Oltre tale soglia è obbligatoria una licenza commerciale scritta concordata con l'Autore (Rubens Rainelli).

CLAUSOLA 3 — APPROVAZIONE INTERATTIVA OBBLIGATORIA IN PRIMO PIANO
Ogni esecuzione dell'applicazione o apertura dell'interfaccia web deve presentare in primo piano la presente schermata di approvazione della licenza prima di abilitare l'accesso ai flussi hardware/virtuali della webcam. L'utente può memorizzare l'accettazione localmente sul proprio browser per un massimo di 7 giorni.

CLAUSOLA 4 — ESCLUSIONE DI GARANZIA
Il software è fornito "COSÌ COM'È" ("AS IS"), senza garanzie di alcun tipo, esplicite o implicite. L'elaborazione video e il confronto tra flussi avvengono interamente in locale nel browser dell'utente (nessun dato video viene trasmesso a server esterni).`;

    function hasValidSavedApproval() {
        try {
            const raw = window.localStorage.getItem(PEFL_STORAGE_KEY);
            if (!raw) return false;
            const parsed = JSON.parse(raw);
            if (!parsed || !parsed.accepted || typeof parsed.expiresAt !== "number") {
                return false;
            }
            if (Date.now() < parsed.expiresAt) {
                return true;
            }
            window.localStorage.removeItem(PEFL_STORAGE_KEY);
            return false;
        } catch (err) {
            return false;
        }
    }

    function saveApproval(rememberFor7Days) {
        try {
            if (rememberFor7Days) {
                const payload = {
                    accepted: true,
                    license: "PEFL-1.0",
                    acceptedAt: new Date().toISOString(),
                    expiresAt: Date.now() + PEFL_TTL_MS
                };
                window.localStorage.setItem(PEFL_STORAGE_KEY, JSON.stringify(payload));
            } else {
                window.localStorage.removeItem(PEFL_STORAGE_KEY);
            }
        } catch (err) {
            console.warn("[PEFL Guard] Impossibile salvare lo stato in localStorage:", err);
        }
    }

    function resetApproval() {
        try {
            window.localStorage.removeItem(PEFL_STORAGE_KEY);
        } catch (err) {
            // Ignora errori storage
        }
    }

    function getSavedApprovalStatus() {
        try {
            const raw = window.localStorage.getItem(PEFL_STORAGE_KEY);
            if (!raw) return { active: false, expiresAt: null };
            const parsed = JSON.parse(raw);
            if (parsed && parsed.accepted && Date.now() < parsed.expiresAt) {
                return {
                    active: true,
                    expiresAt: new Date(parsed.expiresAt)
                };
            }
        } catch (err) {
            // Ignora
        }
        return { active: false, expiresAt: null };
    }

    function enforcePeflLicense(onApprovedCallback, forceShow = false) {
        const modalOverlay = document.getElementById("pefl-modal-overlay");
        const licenseTextEl = document.getElementById("pefl-license-text");
        const rememberCheckbox = document.getElementById("pefl-remember-7days");
        const acceptBtn = document.getElementById("pefl-accept-btn");
        const declineBtn = document.getElementById("pefl-decline-btn");
        const declinedBanner = document.getElementById("pefl-declined-banner");
        const appMain = document.getElementById("app-main-container");
        const licenseBadge = document.getElementById("pefl-status-badge");

        if (licenseTextEl) {
            licenseTextEl.textContent = PEFL_LICENSE_TEXT;
        }

        function updateBadge() {
            if (!licenseBadge) return;
            const st = getSavedApprovalStatus();
            if (st.active && st.expiresAt) {
                const dateStr = st.expiresAt.toLocaleDateString("it-IT", {
                    day: "2-digit",
                    month: "2-digit",
                    year: "numeric"
                });
                licenseBadge.textContent = `PEFL v1.0 Attiva (fino al ${dateStr})`;
                licenseBadge.classList.add("badge-verified");
            } else {
                licenseBadge.textContent = "PEFL v1.0 (Sessione Corrente)";
                licenseBadge.classList.add("badge-verified");
            }
        }

        if (!forceShow && hasValidSavedApproval()) {
            if (modalOverlay) modalOverlay.classList.add("hidden");
            if (appMain) appMain.classList.remove("app-locked");
            updateBadge();
            if (typeof onApprovedCallback === "function") {
                onApprovedCallback();
            }
            return;
        }

        // Mostra il modale in primo piano e blocca l'interfaccia sottostante
        if (modalOverlay) modalOverlay.classList.remove("hidden");
        if (declinedBanner) declinedBanner.classList.add("hidden");
        if (appMain) appMain.classList.add("app-locked");

        if (acceptBtn) {
            acceptBtn.onclick = function () {
                const remember = rememberCheckbox ? Boolean(rememberCheckbox.checked) : false;
                saveApproval(remember);
                if (modalOverlay) modalOverlay.classList.add("hidden");
                if (appMain) appMain.classList.remove("app-locked");
                updateBadge();
                if (typeof onApprovedCallback === "function") {
                    onApprovedCallback();
                }
            };
        }

        if (declineBtn) {
            declineBtn.onclick = function () {
                resetApproval();
                if (declinedBanner) {
                    declinedBanner.classList.remove("hidden");
                }
            };
        }
    }

    window.PEFLWebGuard = {
        enforce: enforcePeflLicense,
        reset: resetApproval,
        getStatus: getSavedApprovalStatus
    };
})();
