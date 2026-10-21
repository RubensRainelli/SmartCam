# SmartCam Linux Native Port (`V4L2` + `v4l2loopback`) & AppImage Builder Guide

[![License: PEFL v1.0](https://raw.githubusercontent.com/RubensRainelli/pefl-license-spec/main/badges/pefl-badge-red.svg)](../../LICENSE)

> **Licenza:** [Personal and Educational Free License (PEFL) v1.0](../../LICENSE) • [Traduzione Italiana](../../LICENSE.it)  
> **Target Architettura:** Linux `x86_64` (Kernel $\ge 5.4$, `Video4Linux2` + `v4l2loopback`)

---

## 1. Architettura del Sottosistema Nativo Linux (`platforms/linux/`)

La cartella `platforms/linux/` fornisce l'implementazione **100% nativa C++17** di acquisizione hardware e Virtual Camera Sink per Linux, collegata direttamente al motore matematico condiviso [`common/engine/`](../../common/engine/) (`AVX-512` / `AVX2` / GPU Compute Dispatcher Vulkan/OpenCL/CUDA/ROCm):

```text
platforms/linux/
├── CMakeLists.txt                  # Build CMake per out_avx2 (.so), out_avx512 (.so) e test_engine_cpp
├── v4l2_camera_capture.h/.cpp      # Acquisizione webcam hardware V4L2 (MMAP 4-buffer ring + filtro sensori IR/GREY)
├── v4l2_virtual_camera_sink.h/.cpp # Sink Virtual Camera v4l2loopback (/dev/videoN, YUYV/RGB24/BGR32 + On-Demand)
├── linux_bridge_c_api.h/.cpp       # Bridge C ABI unificato (smartcam_linux_* + alias smartcam_win_* per Rust FFI)
├── v4l2_virtual_camera_stub.h      # Header legacy di compatibilità
├── scripts/
│   └── setup_v4l2loopback.sh       # Script di caricamento e persistenza modulo kernel v4l2loopback ("SmartCam")
└── appimage/
    ├── AppRun                      # Entry-point AppImage (Guardia Clausola 3 PEFL + auto-dispatch AVX-512/AVX2)
    ├── smartcam.desktop            # Manifesto FreeDesktop / AppImage
    └── build_appimage.sh           # Builder automatizzato per generare dist/SmartCam-v1.0.0-x86_64.AppImage
```

### Componenti Principali
1. **`V4l2CameraCapture` ([`v4l2_camera_capture.h`](v4l2_camera_capture.h) / [`v4l2_camera_capture.cpp`](v4l2_camera_capture.cpp)):**
   - Enumera i dispositivi `/dev/video0`..`/dev/video63` tramite `VIDIOC_QUERYCAP`, escludendo automaticamente i nodi virtuali `v4l2loopback`.
   - Rileva automaticamente i sensori a infrarossi (**Infrared / Windows Hello / Howdy / solo formato `V4L2_PIX_FMT_GREY` / `Y16`**) impostando `is_infrared = true`.
   - Acquisisce i fotogrammi in zero-copy dal kernel tramite 4 ring buffer `V4L2_MEMORY_MMAP` (`VIDIOC_REQBUFS`, `VIDIOC_QUERYBUF`, `VIDIOC_QBUF`, `VIDIOC_DQBUF` sincronizzati con `select()`) e converte `YUYV`, `NV12`, `RGB24`, `BGR24`, `GREY` e `BGR32` in buffer `BGRA32` allineati a 64 byte.
2. **`V4l2VirtualCameraSink` ([`v4l2_virtual_camera_sink.h`](v4l2_virtual_camera_sink.h) / [`v4l2_virtual_camera_sink.cpp`](v4l2_virtual_camera_sink.cpp)):**
   - Scopre automaticamente il nodo `/dev/videoN` corrispondente a `v4l2loopback` (`driver == "v4l2 loopback"` o `card == "SmartCam"`).
   - Configura il formato di uscita tramite `ioctl(fd, VIDIOC_S_FMT, &fmt)` su `V4L2_BUF_TYPE_VIDEO_OUTPUT` (`V4L2_PIX_FMT_YUYV` 4:2:2 BT.601 per compatibilità universale con Chromium, Firefox, Zoom, Microsoft Teams e OBS Studio, con fallback a `V4L2_PIX_FMT_RGB24` / `V4L2_PIX_FMT_BGR32`).
   - Implementa `IsConsumerActive()` su Linux ispezionando i file descriptor aperti in `/proc/*/fd/*` verso `/dev/videoN` per abilitare la modalità **On-Demand ("0% CPU Idle")**.
3. **`linux_bridge_c_api` ([`linux_bridge_c_api.h`](linux_bridge_c_api.h) / [`linux_bridge_c_api.cpp`](linux_bridge_c_api.cpp)):**
   - Espone sia le funzioni native `smartcam_linux_*` sia gli alias `smartcam_win_*` così il crate Rust `app/` può caricare `libsmartcam_engine_avx512.so` o `libsmartcam_engine_avx2.so` senza alcuna modifica alla logica di pipeline.

---

## 2. Prerequisiti dell'Ambiente Linux (per Build & Creazione `.AppImage`)

Per compilare i sorgenti C++17/Rust e generare il pacchetto `dist/SmartCam-v1.0.0-x86_64.AppImage` su **Ubuntu / Debian / WSL2 Ubuntu** (o container Linux equivalente), installare i seguenti pacchetti:

```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    cmake \
    ninja-build \
    pkg-config \
    libv4l-dev \
    v4l-utils \
    v4l2loopback-dkms \
    v4l2loopback-utils \
    curl \
    file \
    squashfs-tools \
    desktop-file-utils \
    ca-certificates
```

Assicurarsi inoltre che la toolchain **Rust** (`cargo` / `rustc` $\ge 1.75$) sia installata:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

---

## 3. Configurazione Dispositivo Kernel `v4l2loopback` (`"SmartCam"`)

Prima di avviare SmartCam su un sistema Linux desktop, eseguire lo script di setup che carica il modulo `v4l2loopback` creando `/dev/video10` con nome `"SmartCam"` e flag `exclusive_caps=1` (indispensabile per far comparire la webcam in Chrome, Edge, Firefox, Google Meet, Teams e Zoom):

```bash
chmod +x platforms/linux/scripts/setup_v4l2loopback.sh
./platforms/linux/scripts/setup_v4l2loopback.sh --persist
```

*(L'opzione `--persist` configura anche `/etc/modules-load.d/smartcam.conf` e `/etc/modprobe.d/smartcam-v4l2loopback.conf` per rendere la webcam virtuale permanente ad ogni riavvio).*

---

## 4. Generazione dell'AppImage (`dist/SmartCam-v1.0.0-x86_64.AppImage`)

Dalla radice del repository (dentro un ambiente Linux nativo, WSL2 o container Docker Linux), eseguire:

```bash
chmod +x platforms/linux/appimage/build_appimage.sh platforms/linux/appimage/AppRun
./platforms/linux/appimage/build_appimage.sh
```

Lo script eseguirà automaticamente:
1. Verifica della **Clausola 3 PEFL v1.0** (con cache di 7 giorni o flag `--accept-pefl`).
2. Compilazione di `libsmartcam_engine_avx2.so` (`out_avx2`, 0 istruzioni `ZMM`) ed esecuzione di `test_engine_cpp`.
3. Compilazione di `libsmartcam_engine_avx512.so` (`out_avx512`, registri `ZMM` a 512-bit).
4. Compilazione del binario Rust (`cargo build --release`).
5. Assemblaggio di `platforms/linux/build/SmartCam.AppDir/` e creazione di **`dist/SmartCam-v1.0.0-x86_64.AppImage`** + aggiornamento di **`dist/SHA256SUMS.txt`**.
