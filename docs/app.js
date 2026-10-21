/*
 * Personal and Educational Free License (PEF License / PEFL)
 * Version 1.0 - 2026-05-26
 * Copyright (c) 2026 Rubens Rainelli
 *
 * Contact Details:
 *   Email: rubens@rainelli.it
 *   GitHub: https://github.com/RubensRainelli/
 *   Tel: (+39) 342 811 5882
 *
 * Personal and educational free use permitted.
 * Corporate, business or enterprise use prohibited without written authorization and fee.
 * The full license can be found in the LICENSE file.
 */

const PEFL_STORAGE_KEY = 'pefl-accepted-date';
const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;

let currentLang = (navigator.language || 'en').toLowerCase().startsWith('it') ? 'it' : 'en';
let currentFilter = 'all';

const I18N = {
  en: {
    heroTag: 'NATIVE RUST + C++17 SIMD + MULTI-VENDOR GPU VIRTUAL CAMERA',
    heroTitle: 'SMARTCAM VIRTUAL STUDIO',
    heroDesc: 'Zero-Python native virtual camera for Windows 11 (Media Foundation COM) and Linux (V4L2 + v4l2loopback AppImage). Features 5-Point Guided Face Template Tracking (NCC + 2D Kalman), AVX-512/AVX2 & Vulkan/OpenCL/CUDA/ROCm acceleration, 64-bit HDR source processing, and a single-port HTTPS :8080 OLED Smartphone Camera WebApp.',
    tbTitle: 'LIVE DUAL-CAMERA TESTBENCH (RAW UVC VS SMARTCAM)',
    btnStartCam: '▶ ACTIVATE WEBCAMS',
    eulaTitle: 'PERSONAL AND EDUCATIONAL FREE LICENSE (PEFL v1.0)',
    btnAccept: 'ACCEPT PEFL v1.0 (7-DAY CACHE)',
    btnDecline: 'DECLINE',
    eulaText: `Personal and Educational Free License (PEF License / PEFL) — Version 1.0 (2026-05-26)
Copyright (c) 2026 Rubens Rainelli (rubens@rainelli.it | https://github.com/RubensRainelli/)

1. FREE FOR PERSONAL, EDUCATIONAL, ACADEMIC & NON-PROFIT USE:
   You are free to use, study, modify, and redistribute this software exclusively for personal, educational, student, academic, or non-profit research purposes.

2. CORPORATE / COMMERCIAL / ENTERPRISE USE PROHIBITED WITHOUT WRITTEN LICENSE:
   Any use in a corporate, commercial, business, enterprise, or revenue-generating environment (including religious organizations and places of worship) is strictly prohibited without prior written authorization and fee payment to the author (Rubens Rainelli).

3. MANDATORY CLAUSE 3 VERIFICATION:
   Explicit user acceptance is required before running the application and is cached locally for 7 days.`
  },
  it: {
    heroTag: 'WEBCAM VIRTUALE NATIVA RUST + C++17 SIMD + GPU MULTI-VENDOR',
    heroTitle: 'SMARTCAM VIRTUAL STUDIO',
    heroDesc: 'Webcam virtuale 100% nativa (zero Python) per Windows 11 (Media Foundation COM) e Linux (V4L2 + v4l2loopback AppImage). Include Tracciamento Volto Calibrato a 5 Punti (NCC + Kalman 2D), accelerazione AVX-512/AVX2 e GPU Vulkan/OpenCL/CUDA/ROCm, pipeline HDR a 64-bit e WebApp Fotocamera Smartphone OLED su singola porta HTTPS :8080.',
    tbTitle: 'TESTBENCH LIVE DOPPIA WEBCAM (UVC FISICA VS SMARTCAM)',
    btnStartCam: '▶ ATTIVA LE 2 WEBCAM',
    eulaTitle: 'LICENZA GRATUITA PERSONALE E DIDATTICA (PEFL v1.0)',
    btnAccept: 'ACCETTO LA LICENZA PEFL v1.0 (7 GIORNI)',
    btnDecline: 'RIFIUTO',
    eulaText: `Personal and Educational Free License (PEF License / PEFL) — Versione 1.0 (2026-05-26)
Copyright (c) 2026 Rubens Rainelli (rubens@rainelli.it | https://github.com/RubensRainelli/)

1. USO GRATUITO PERSONALE, DIDATTICO E NO-PROFIT:
   È consentito utilizzare, studiare, modificare e ridistribuire il software esclusivamente per scopi personali, didattici, accademici o di ricerca non-profit.

2. DIVIETO DI USO AZIENDALE / COMMERCIALE SENZA AUTORIZZAZIONE:
   Qualsiasi utilizzo aziendale, commerciale, professionale o d'impresa (inclusi enti religiosi e luoghi di culto) è severamente vietato senza preventiva autorizzazione scritta e corresponsione di compenso all'autore (Rubens Rainelli).

3. VERIFICA OBBLIGATORIA CLAUSOLA 3:
   L'accettazione esplicita della licenza è obbligatoria all'avvio e viene memorizzata per 7 giorni.`
  }
};

const MODULES = [
  {
    id: 'ID: 001',
    cat: 'ai',
    title: {
      en: '5-Point Guided Face Template Tracker',
      it: 'Tracciamento Volto Guidato a 5 Punti'
    },
    desc: {
      en: 'Interactive snapshot calibration (Right Eye, Left Eye, Nose Tip, Lips, Chin) with Z-Score normalized 32x32 YCbCr template matching (NCC) and 2D Kalman filtering. Supports Face + Upper Body or Face Only cutout.',
      it: 'Calibrazione guidata su foto (Occhio Destro, Occhio Sinistro, Punta del Naso, Labbra, Mento) con template matching fotometrico Z-Score 32x32 (NCC) e filtro di Kalman 2D. Supporta Volto + Busto o Solo Volto.'
    },
    tags: ['NCC PYRAMID', 'KALMAN 2D', 'ZERO PARALLAX']
  },
  {
    id: 'ID: 002',
    cat: 'mobile',
    title: {
      en: 'OLED Smartphone Camera (HTTPS :8080)',
      it: 'Fotocamera Smartphone OLED (HTTPS :8080)'
    },
    desc: {
      en: 'Single-port HTTP/HTTPS multiplexer on port 8080 unlocks mobile browser camera permissions automatically. Features multi-lens dropdown, native 1:1 sensor aspect ratio, FPS & bandwidth scaler, Wake Lock anti-standby, and 0.00% APL pure black fullscreen saver (2-tap or swipe back).',
      it: 'Multiplexer HTTP/HTTPS sulla stessa porta 8080 per sbloccare il permesso fotocamera mobile. Include selettore obiettivi multipli, proporzioni 1:1 native del sensore, scala banda e FPS, anti-standby Wake Lock e schermo nero fullscreen OLED a zero scritte (2 tap o swipe indietro).'
    },
    tags: ['HTTPS :8080', '0% APL OLED', 'WAKE LOCK']
  },
  {
    id: 'ID: 003',
    cat: 'engine',
    title: {
      en: 'AVX-512 / AVX2 SIMD & Multi-Vendor GPU',
      it: 'SIMD AVX-512 / AVX2 & GPU Multi-Vendor'
    },
    desc: {
      en: 'Dynamic runtime dispatch across CPU (AVX-512, AVX2, Scalar) and GPU (Vulkan, OpenCL, NVIDIA CUDA, AMD ROCm/HIP) with real hardware pipeline benchmarking and hardware fingerprint caching.',
      it: 'Dispatch dinamico a runtime su CPU (AVX-512, AVX2, Scalare) e GPU (Vulkan, OpenCL, NVIDIA CUDA, AMD ROCm/HIP) con benchmark reale della pipeline e memorizzazione per fingerprint hardware.'
    },
    tags: ['AVX-512', 'VULKAN / CUDA', 'REAL BENCHMARK']
  },
  {
    id: 'ID: 004',
    cat: 'engine',
    title: {
      en: '64-Bit Deep Color HDR & Mirror Background',
      it: 'Sorgente 64-Bit HDR & Specchiatura Sfondo'
    },
    desc: {
      en: 'Selectable 32-bit (BGRA32) or 64-bit (BGRA64 16-bit per channel) source pipeline with 12-bit fixed-point YCbCr chroma conversion, plus horizontal/vertical background mirroring for apps that mirror webcams.',
      it: 'Pipeline sorgente selezionabile a 32-bit (BGRA32) o 64-bit (BGRA64 16-bit per canale) con conversione cromatica a 12-bit ed opzione di specchiatura orizzontale/verticale dello sfondo.'
    },
    tags: ['64-BIT BGRA64', 'MIRROR H/V', 'SOCIAL CROP']
  },
  {
    id: 'ID: 005',
    cat: 'plugins',
    title: {
      en: 'Modular Plugin System & Multi-Person AI',
      it: 'Sistema Plugin Modulare & Multi-Persona'
    },
    desc: {
      en: 'C ABI v1 plugin architecture (`how-to-make-a-plugin.md`) with built-in Smartphone Camera, Multi-Person Tracker (up to 4 simultaneous subjects), Studio Color & Denoise Enhancer, Teleprompter, and WebRTC/RTSP.',
      it: 'Architettura plugin C ABI v1 (`how-to-make-a-plugin.md`) con Fotocamera Smartphone, Tracking Persone Multiple (fino a 4 soggetti), Ottimizzatore Luce/Colore Studio, Teleprompter e WebRTC/RTSP.'
    },
    tags: ['PLUGIN ABI V1', 'MULTI-PERSON x4', 'STUDIO LUT']
  },
  {
    id: 'ID: 006',
    cat: 'plugins',
    title: {
      en: 'Windows 11 COM & Linux V4L2 AppImage',
      it: 'Windows 11 COM & Linux V4L2 AppImage'
    },
    desc: {
      en: 'Zero-latency 64-bit Seqlock Shared Memory IPC on Windows 11 (`IMFMediaSourceEx`) and native `v4l2loopback` `/dev/videoN` sink on Linux packaged as a self-contained `SmartCam-v1.0.0-x86_64.AppImage`.',
      it: 'IPC Shared Memory Seqlock a 64-bit su Windows 11 (`IMFMediaSourceEx`) e sink nativo `v4l2loopback` `/dev/videoN` su Linux impacchettato in `SmartCam-v1.0.0-x86_64.AppImage`.'
    },
    tags: ['WIN11 MF COM', 'V4L2LOOPBACK', 'APPIMAGE']
  }
];

function checkEulaCompliance() {
  const saved = parseInt(localStorage.getItem(PEFL_STORAGE_KEY) || '0', 10);
  if (!saved || (Date.now() - saved) > SEVEN_DAYS_MS) {
    openEulaModal(true);
  }
}

function openEulaModal(show) {
  const m = document.getElementById('pefl-modal');
  if (show) m.classList.remove('hidden');
  else m.classList.add('hidden');
}

function acceptEula() {
  localStorage.setItem(PEFL_STORAGE_KEY, String(Date.now()));
  openEulaModal(false);
}

function declineEula() {
  localStorage.removeItem(PEFL_STORAGE_KEY);
  alert(currentLang === 'it'
    ? 'Per utilizzare SmartCam è necessario accettare la licenza PEFL v1.0 (Clausola 3).'
    : 'You must accept the PEFL v1.0 license (Clause 3) to use SmartCam.');
}

function toggleLanguage() {
  currentLang = currentLang === 'en' ? 'it' : 'en';
  applyLocalization();
}

function applyLocalization() {
  const t = I18N[currentLang];
  document.getElementById('hero-tag').textContent = t.heroTag;
  document.getElementById('hero-title').textContent = t.heroTitle;
  document.getElementById('hero-desc').textContent = t.heroDesc;
  document.getElementById('tb-title').textContent = t.tbTitle;
  document.getElementById('btn-start-cam').textContent = t.btnStartCam;
  document.getElementById('eula-title').textContent = t.eulaTitle;
  document.getElementById('eula-text').textContent = t.eulaText;
  document.getElementById('btn-accept').textContent = t.btnAccept;
  document.getElementById('btn-decline').textContent = t.btnDecline;
  document.getElementById('lang-btn').textContent = currentLang === 'en' ? '[ EN* | IT ]' : '[ EN | IT* ]';
  renderModules();
}

function setFilter(cat, btn) {
  currentFilter = cat;
  document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
  if (btn) btn.classList.add('active');
  renderModules();
}

function renderModules() {
  const q = (document.getElementById('search-input').value || '').toLowerCase().trim();
  const container = document.getElementById('cards-container');
  container.innerHTML = '';

  MODULES.filter(m => {
    if (currentFilter !== 'all' && m.cat !== currentFilter) return false;
    if (!q) return true;
    const text = (m.title[currentLang] + ' ' + m.desc[currentLang] + ' ' + m.tags.join(' ')).toLowerCase();
    return text.includes(q);
  }).forEach(m => {
    const card = document.createElement('article');
    card.className = 'cyber-card';
    card.innerHTML = `
      <div class="card-meta"><span>${m.id}</span><span>MOD_${m.cat.toUpperCase()}</span></div>
      <h3 class="card-title">${m.title[currentLang]}</h3>
      <p class="card-body">${m.desc[currentLang]}</p>
      <div class="card-tags">${m.tags.map(tg => `<span class="tag-badge">${tg}</span>`).join('')}</div>
    `;
    container.appendChild(card);
  });
}

function isIrCameraLabel(label) {
  const l = (label || '').toLowerCase();
  return l.includes('ir ') || l.includes(' ir') || l.includes('infrared') || l.includes('windows hello');
}

async function startDualCameras() {
  if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) return;
  try {
    const temp = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
    temp.getTracks().forEach(t => t.stop());
    const devices = (await navigator.mediaDevices.enumerateDevices()).filter(d => d.kind === 'videoinput');
    const selA = document.getElementById('sel-cam-a');
    const selB = document.getElementById('sel-cam-b');
    selA.innerHTML = '';
    selB.innerHTML = '';
    devices.forEach((d, idx) => {
      const ir = isIrCameraLabel(d.label);
      const lbl = (d.label || `Camera #${idx + 1}`) + (ir ? ' ⚠️ [IR]' : '');
      selA.add(new Option(lbl, d.deviceId));
      selB.add(new Option(lbl, d.deviceId));
    });
    const firstRgb = devices.findIndex(d => !isIrCameraLabel(d.label));
    if (firstRgb >= 0) selA.selectedIndex = firstRgb;
    const smartIdx = devices.findIndex(d => (d.label || '').toLowerCase().includes('smartcam'));
    if (smartIdx >= 0) selB.selectedIndex = smartIdx;
    await switchCam('a');
    await switchCam('b');
  } catch (e) {
    alert('Camera permission denied or busy: ' + e.message);
  }
}

async function switchCam(which) {
  const sel = document.getElementById('sel-cam-' + which);
  const vid = document.getElementById('vid-' + which);
  const res = document.getElementById('res-' + which);
  const warn = document.getElementById('ir-warn-' + which);
  if (!sel.value) return;
  const optText = sel.options[sel.selectedIndex] ? sel.options[sel.selectedIndex].text : '';
  warn.style.display = isIrCameraLabel(optText) ? 'block' : 'none';
  if (vid.srcObject) vid.srcObject.getTracks().forEach(t => t.stop());
  const stream = await navigator.mediaDevices.getUserMedia({
    video: { deviceId: { exact: sel.value } },
    audio: false
  });
  vid.srcObject = stream;
  vid.onloadedmetadata = () => {
    res.textContent = `${vid.videoWidth}x${vid.videoHeight}`;
  };
}

document.addEventListener('DOMContentLoaded', () => {
  applyLocalization();
  checkEulaCompliance();
});
