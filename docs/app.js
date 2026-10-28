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
    sysBadge: 'WINDOWS • MACOS • LINUX // 100% OFFLINE & PRIVATE',
    hdrProjectsBtn: '🚀 MY OTHER PROJECTS',
    heroTag: 'AI VIRTUAL STUDIO CAMERA // WINDOWS • MACOS • LINUX',
    heroTitle: 'SMARTCAM VIRTUAL STUDIO v2.0',
    heroDesc: 'Turn your webcam or smartphone into a professional AI studio camera on Windows, macOS, and Linux. Blur or replace your background without a green screen, stream live HD video from your phone over Wi-Fi with a QR code (no phone app needed), and use SmartCam in Zoom, Microsoft Teams, Google Meet, Discord, and OBS Studio — 100% offline, private, and ultra-lightweight.',
    fltAll: 'ALL',
    fltStudio: 'BACKGROUND & BLUR',
    fltMobile: 'SMARTPHONE WI-FI',
    fltPlatforms: 'WIN / MAC / LINUX',
    fltPrivacy: 'PRIVACY & SPEED',
    tbTag: 'INSTANT BROWSER CAMERA PREVIEW',
    tbTitle: 'LIVE DUAL-CAMERA TESTBENCH (PHYSICAL WEBCAM VS SMARTCAM)',
    btnStartCam: '▶ ACTIVATE WEBCAMS',
    creatorTag: 'CURIOUS ABOUT WHO BUILT SMARTCAM?',
    creatorTitle: 'DISCOVER ALL PROJECTS BY RUBENS RAINELLI',
    creatorDesc: 'SmartCam is crafted with passion by Rubens Rainelli. Want to see what else I design and build? Explore my interactive showcase of software, AI tools, and creative engineering projects!',
    btnCreatorPortfolio: '🚀 EXPLORE ALL MY PROJECTS',
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
    sysBadge: 'WINDOWS • MACOS • LINUX // 100% OFFLINE E PRIVATA',
    hdrProjectsBtn: '🚀 I MIEI ALTRI PROGETTI',
    heroTag: 'TELECAMERA STUDIO AI // WINDOWS • MACOS • LINUX',
    heroTitle: 'SMARTCAM VIRTUAL STUDIO v2.0',
    heroDesc: 'Trasforma la tua webcam o il tuo smartphone in una telecamera da studio professionale con AI su Windows, macOS e Linux. Sfoca o sostituisci lo sfondo senza telo verde, usa la fotocamera del telefono in Wi-Fi inquadrando un QR Code (senza installare app sul cellulare) e usa SmartCam su Zoom, Microsoft Teams, Google Meet, Discord e OBS Studio — 100% offline, privata e leggerissima.',
    fltAll: 'TUTTI',
    fltStudio: 'SFONDO & SFOCATURA',
    fltMobile: 'SMARTPHONE WI-FI',
    fltPlatforms: 'WIN / MAC / LINUX',
    fltPrivacy: 'PRIVACY & VELOCITÀ',
    tbTag: 'ANTEPRIMA IMMEDIATA NEL BROWSER',
    tbTitle: 'TESTBENCH LIVE DOPPIA WEBCAM (WEBCAM FISICA VS SMARTCAM)',
    btnStartCam: '▶ ATTIVA LE 2 WEBCAM',
    creatorTag: 'CURIOSO DI SCOPRIRE CHI HA CREATO SMARTCAM?',
    creatorTitle: 'SCOPRI TUTTI GLI ALTRI PROGETTI DI RUBENS RAINELLI',
    creatorDesc: 'SmartCam è sviluppata con passione da Rubens Rainelli. Vuoi scoprire chi sono e quali altri progetti, strumenti AI e applicazioni ho realizzato? Visita la vetrina interattiva con tutti i miei lavori!',
    btnCreatorPortfolio: '🚀 SCOPRI TUTTI I MIEI PROGETTI',
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
    id: '01 // STUDIO',
    cat: 'studio',
    title: {
      en: 'Background Blur & Custom Studio Scenes',
      it: 'Sfondo Sfocato e Ambienti Studio Personalizzati'
    },
    desc: {
      en: 'Hide a messy room with natural portrait blur, pick from built-in executive office scenes, upload your own photos, or use a physical Green Screen with automatic one-click color calibration.',
      it: 'Nascondi il disordine della stanza con una sfocatura ritratto naturale, scegli tra gli sfondi da ufficio inclusi, carica le tue foto o usa un telo verde con calibrazione automatica in un clic.'
    },
    tags: ['PORTRAIT BLUR', 'CUSTOM PHOTO', 'AUTO GREEN SCREEN']
  },
  {
    id: '02 // MOBILE',
    cat: 'mobile',
    title: {
      en: 'Smartphone as Wireless HD Webcam (Zero App)',
      it: 'Smartphone come Webcam HD Wi-Fi (Zero App)'
    },
    desc: {
      en: 'Scan the QR code displayed inside SmartCam with your iPhone or Android camera to stream crystal-clear HD video over your home Wi-Fi. No app installation required on your phone, complete with OLED battery-saver mode.',
      it: 'Inquadra il QR Code mostrato in SmartCam con la fotocamera del tuo iPhone o Android per trasmettere video HD tramite il Wi-Fi di casa. Nessuna app da installare sul telefono e modalità risparmio OLED inclusa.'
    },
    tags: ['QR CODE WI-FI', 'NO PHONE APP', 'OLED SAVER']
  },
  {
    id: '03 // PLATFORMS',
    cat: 'platforms',
    title: {
      en: 'Native on Windows, macOS & Linux',
      it: 'Nativa su Windows, macOS e Linux'
    },
    desc: {
      en: 'Ready-to-run installers for everyone: Windows 10/11 (.exe installer, portable .zip, or WinGet), macOS Universal (.dmg for Apple Silicon M1–M4 & Intel), and Linux (portable .AppImage, Ubuntu/Debian .deb, and .tar.gz).',
      it: 'Pacchetti pronti per ogni sistema: Windows 10/11 (installer .exe, .zip portatile o WinGet), macOS Universal (.dmg per Apple Silicon M1–M4 e Intel) e Linux (.AppImage universale, .deb per Ubuntu/Debian e .tar.gz).'
    },
    tags: ['WINDOWS 10/11', 'MACOS UNIVERSAL', 'LINUX APPIMAGE & DEB']
  },
  {
    id: '04 // APPS',
    cat: 'platforms',
    title: {
      en: 'Works Instantly with Zoom, Teams, Meet & OBS',
      it: 'Subito Pronta su Zoom, Teams, Meet e OBS'
    },
    desc: {
      en: 'Appears directly as the "SmartCam" camera inside Zoom, Microsoft Teams, Google Meet, Discord, Skype, OBS Studio, and modern web browsers (Chrome, Edge, Firefox, Safari).',
      it: 'Compare direttamente come fotocamera "SmartCam" dentro Zoom, Microsoft Teams, Google Meet, Discord, Skype, OBS Studio e in tutti i browser moderni (Chrome, Edge, Firefox, Safari).'
    },
    tags: ['ZOOM & TEAMS', 'GOOGLE MEET', 'DISCORD & OBS']
  },
  {
    id: '05 // FRAMING',
    cat: 'studio',
    title: {
      en: 'Smart Framing, Social Formats & Mirroring',
      it: 'Inquadratura Intelligente, Formati Social e Specchio'
    },
    desc: {
      en: 'Switch effortlessly between Widescreen (16:9), Classic (4:3), Square (1:1), and Vertical Social (9:16 for TikTok/Reels/Shorts), with independent horizontal and vertical background mirroring.',
      it: 'Passa in un clic tra formato Widescreen (16:9), Classico (4:3), Quadrato (1:1) e Verticale Social (9:16 per TikTok/Reels/Shorts), con specchiatura indipendente dello sfondo.'
    },
    tags: ['16:9 / 9:16 / 1:1', 'AUTO FRAMING', 'MIRROR H/V']
  },
  {
    id: '06 // PRIVACY',
    cat: 'privacy',
    title: {
      en: '100% Offline, Private & Powered by Pure Rust',
      it: '100% Offline, Privata e Leggerissima (Pure Rust)'
    },
    desc: {
      en: 'Built 100% in pure Rust for maximum speed and minimal battery/CPU usage. Every single frame stays on your computer — zero cloud uploads, zero accounts, zero subscriptions, and zero watermarks.',
      it: 'Scritta al 100% in Rust puro per offrire massima velocità e consumo minimo di batteria e CPU. Ogni fotogramma resta sul tuo computer: zero cloud, zero account, zero abbonamenti e zero filigrane.'
    },
    tags: ['100% OFFLINE', 'ZERO CLOUD', '100% PURE RUST']
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
  const setTxt = (id, val) => {
    const el = document.getElementById(id);
    if (el && val) el.textContent = val;
  };
  setTxt('sys-badge', t.sysBadge);
  setTxt('hdr-projects-btn', t.hdrProjectsBtn);
  setTxt('hero-tag', t.heroTag);
  setTxt('hero-title', t.heroTitle);
  setTxt('hero-desc', t.heroDesc);
  setTxt('flt-all', t.fltAll);
  setTxt('flt-studio', t.fltStudio);
  setTxt('flt-mobile', t.fltMobile);
  setTxt('flt-platforms', t.fltPlatforms);
  setTxt('flt-privacy', t.fltPrivacy);
  setTxt('tb-tag', t.tbTag);
  setTxt('tb-title', t.tbTitle);
  setTxt('btn-start-cam', t.btnStartCam);
  setTxt('creator-tag', t.creatorTag);
  setTxt('creator-title', t.creatorTitle);
  setTxt('creator-desc', t.creatorDesc);
  setTxt('btn-creator-portfolio', t.btnCreatorPortfolio);
  setTxt('eula-title', t.eulaTitle);
  setTxt('eula-text', t.eulaText);
  setTxt('btn-accept', t.btnAccept);
  setTxt('btn-decline', t.btnDecline);
  setTxt('lang-btn', currentLang === 'en' ? '[ EN* | IT ]' : '[ EN | IT* ]');
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
      <div class="card-meta"><span>${m.id}</span><span>SMARTCAM_V2</span></div>
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
