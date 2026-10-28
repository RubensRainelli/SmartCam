<div align="center">

<img src="assets/smartcam_cyber_lens.svg" alt="SmartCam Logo" width="128" height="128" />

# SmartCam

**Turn your webcam or smartphone into a professional AI studio camera — 100% offline, private, and ultra-lightweight.**

[![Latest Release](https://img.shields.io/github/v/release/RubensRainelli/SmartCam?style=flat-square&color=00b4d8&label=Release)](https://github.com/RubensRainelli/SmartCam/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11%20%7C%20macOS%20%7C%20Linux-2a9d8f?style=flat-square)](https://github.com/RubensRainelli/SmartCam/releases/latest)
[![License: PEFL v1.0](https://img.shields.io/badge/License-PEFL%20v1.0-f4a261?style=flat-square)](LICENSE)

<h3>🇬🇧 <b>English</b> &nbsp;|&nbsp; <a href="README.it.md">🇮🇹 Italiano</a></h3>

</div>

---

## ⬇️ Download & Instant Installation (v2.0.0)

Choose the package for your operating system from the official **SmartCam v2.0.0** release:

| Operating System | Package Type | Direct Download | Notes |
| :--- | :--- | :--- | :--- |
| **🪟 Windows 10 / 11 (64-bit)** | **Official Installer (`.exe`)** | [**Download SmartCam-Setup-v2.0.0-x64.exe**](https://github.com/RubensRainelli/SmartCam/releases/download/v2.0.0/SmartCam-Setup-v2.0.0-x64.exe) | **Recommended.** Automatically sets up the Virtual Camera and Start Menu shortcuts. |
| **🪟 Windows 10 / 11 (64-bit)** | **Portable Archive (`.zip`)** | [**Download SmartCam-v2.0.0-windows-x64-portable.zip**](https://github.com/RubensRainelli/SmartCam/releases/download/v2.0.0/SmartCam-v2.0.0-windows-x64-portable.zip) | No installation required. Extract anywhere and run `SmartCam.exe`. |
| **🍎 macOS (Apple Silicon & Intel)** | **Universal Disk Image (`.dmg`)** | [**Download SmartCam-v2.0.0-macos-universal.dmg**](https://github.com/RubensRainelli/SmartCam/releases/download/v2.0.0/SmartCam-v2.0.0-macos-universal.dmg) | Universal app bundle for M1/M2/M3/M4 and Intel Macs. |
| **🐧 Linux (Ubuntu / Debian x64)** | **Debian Package (`.deb`)** | [**Download smartcam_2.0.0_amd64.deb**](https://github.com/RubensRainelli/SmartCam/releases/download/v2.0.0/smartcam_2.0.0_amd64.deb) | Install with `sudo apt install ./smartcam_2.0.0_amd64.deb`. |
| **🐧 Linux (All Distros x64)** | **Portable Tarball (`.tar.gz`)** | [**Download SmartCam-v2.0.0-linux-x86_64.tar.gz**](https://github.com/RubensRainelli/SmartCam/releases/download/v2.0.0/SmartCam-v2.0.0-linux-x86_64.tar.gz) | Universal Linux binary archive. |

### ⚡ Install on Windows via WinGet

On Windows 10 and Windows 11, you can also install or update SmartCam directly from the terminal using **Windows Package Manager (WinGet)**:

```powershell
winget install RubensRainelli.SmartCam
```

---

## ✨ What SmartCam Can Do for You

* **🎨 Blur or Replace Your Background (No Green Screen Needed):** Hide a messy room with natural studio portrait blur, replace your background with built-in office scenes or your own photos, or use a physical Green Screen with automatic one-click color calibration.
* **📱 Use Your Smartphone as an HD Webcam:** Don't have a good webcam on your PC? Scan the QR code shown inside SmartCam with your phone's camera to stream live HD video over your home Wi-Fi — **zero apps to install on your phone**.
* **🎥 Works Everywhere Out of the Box:** Appears immediately as the **"SmartCam"** camera inside **Zoom, Microsoft Teams, Google Meet, Discord, Skype, OBS Studio**, and all modern browsers (Chrome, Edge, Firefox) on Windows 10 and Windows 11.
* **🔒 100% Private, Offline & Ultra-Lightweight:** Every video frame is processed directly on your computer. No video ever leaves your device, no account is required, and there are no subscriptions or watermarks.

---

## 🚀 Quick Start in 3 Simple Steps

1. **Install & Open SmartCam:** Run `SmartCam-Setup-v2.0.0-x64.exe` (or launch `SmartCam.exe`). SmartCam will automatically detect your webcam and optimize itself for your computer.
2. **Choose Your Look or Connect Your Phone:** Pick your preferred background effect (**Blur**, **Studio Image**, or **Solid Color**), adjust the framing (`16:9`, `4:3`, `1:1`, or `9:16`), or click **Smartphone Wi-Fi** to scan the QR code with your phone.
3. **Select "SmartCam" in Your Video Call App:** Open **Zoom, Microsoft Teams, Google Meet, Discord, or OBS Studio**, go to your Video Settings, and select **SmartCam** as your camera.

---

## ❓ Frequently Asked Questions (FAQ) & Compatibility

### Which versions of Windows and video apps are supported in SmartCam v2.0?
**SmartCam v2.0** is built exclusively for modern **Windows 10 and Windows 11 (64-bit)** using the native Windows Media Foundation camera platform. This guarantees instant compatibility with Microsoft Teams, Zoom, Google Meet, Chrome, Edge, Firefox, Discord, and OBS Studio.

> **Need support for older legacy software?**
> Support for legacy *DirectShow-only* programs has been retired in v2.0 to keep SmartCam fast, clean, and modern. If you use an older application on a legacy PC that requires DirectShow, you can download **[SmartCam v1.x from the Releases page](https://github.com/RubensRainelli/SmartCam/releases)**.

### Why does Windows show a security prompt when I open the installer?
All SmartCam v2.0.0 executables (`SmartCam.exe`, `smartcam_mf_source.dll`, and `SmartCam-Setup-v2.0.0-x64.exe`) are digitally signed by the author (**Rubens Rainelli**). When you run the installer as Administrator, it automatically registers the publisher certificate on your PC for seamless updates.

### How can I verify the integrity of my download?
Every official GitHub Release includes a [`SHA256SUMS.txt`](https://github.com/RubensRainelli/SmartCam/releases/download/v2.0.0/SHA256SUMS.txt) file containing the cryptographic SHA-256 checksums of all installers and archives. On Windows PowerShell, you can verify your download with:

```powershell
Get-FileHash .\SmartCam-Setup-v2.0.0-x64.exe -Algorithm SHA256
```

---

## 🛠️ For Developers & License

Are you a software engineer or interested in how SmartCam works under the hood? Read the **[Technical Architecture & Engineering Guide (`docs/ARCHITECTURE.md`)](docs/ARCHITECTURE.md)**.

### 📜 License (PEFL v1.0)

SmartCam is licensed under the **Personal and Educational Free License (PEFL) v1.0**:
* **✅ Free for Personal & Educational Use:** Completely free for individuals, students, teachers, and academic research.
* **🏢 Commercial / Corporate Use:** Using SmartCam in a company, business, enterprise, or professional organization requires a written commercial license from the author.

For full license terms, see [`LICENSE`](LICENSE) (English), [`LICENSE.it`](LICENSE.it) (Italian), and [`NOTICE.md`](NOTICE.md).

**Author & Contact:**
* **Rubens Rainelli** — [rubens@rainelli.it](mailto:rubens@rainelli.it) | [GitHub @RubensRainelli](https://github.com/RubensRainelli)
