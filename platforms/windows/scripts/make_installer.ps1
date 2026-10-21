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

<#
.SYNOPSIS
    SmartCam One-Command Installer Builder ("makeInstaller").
.DESCRIPTION
    1. Verifies PEFL v1.0 Clause 3 license acceptance.
    2. Ensures both C++ SIMD builds (out_avx2 and out_avx512) and the native Rust app (SmartCam.exe) are compiled.
    3. Packages the complete distribution into:
       - dist\SmartCam_Setup_v1.0.0_x64.exe (Standalone Single-File Graphical Installer for Windows 11)
       - dist\SmartCam_Portable_v1.0.0_x64.zip (Portable Zero-Install Archive)
       - dist\SHA256SUMS.txt (Cryptographic SHA-256 verification manifest)
#>

param(
    [switch]$Rebuild = $false,
    [switch]$NonInteractiveAcceptPefl = $false,
    [string]$Version = "1.0.0"
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path

function Show-PeflLicenseAndPrompt {
    if ($NonInteractiveAcceptPefl) { return }

    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    if ([string]::IsNullOrEmpty($ScriptDir)) { $ScriptDir = "." }
    $ScriptStem = [System.IO.Path]::GetFileNameWithoutExtension($MyInvocation.MyCommand.Name)
    $CacheFile = Join-Path $ScriptDir ".pefl_accepted_$ScriptStem"

    $CurrentTime = [int]([datetimeoffset]::Now.ToUnixTimeSeconds())
    $MaxAge = 7 * 24 * 60 * 60

    if (Test-Path $CacheFile) {
        try {
            $LastAcceptedText = Get-Content -Path $CacheFile -ErrorAction SilentlyContinue
            if ($LastAcceptedText -match '^\d+$') {
                $LastAccepted = [int]$LastAcceptedText
                $Age = $CurrentTime - $LastAccepted
                if ($Age -ge 0 -and $Age -lt $MaxAge) {
                    return
                }
            }
        } catch {}
    }

    Write-Host "============================================================" -ForegroundColor Red
    Write-Host "        Personal and Educational Free License (PEFL) v1.0" -ForegroundColor White
    Write-Host "============================================================" -ForegroundColor Red
    Write-Host "Copyright (c) 2026 Rubens Rainelli"
    Write-Host "Official Specification: https://github.com/RubensRainelli/pefl-license-spec"
    Write-Host "Author Contact: rubens@rainelli.it`n"
    Write-Host "TERMS OF USE:" -ForegroundColor Yellow
    Write-Host "  * FREE for Personal & Educational use."
    Write-Host "  * STRICTLY PROHIBITED for Corporate, Professional, or Business use"
    Write-Host "    without written authorization and fee payment."
    Write-Host "  * Excludes places of worship and religious organizations (corporate use).`n"
    Write-Host "For full terms, see: https://github.com/RubensRainelli/pefl-license-spec"
    Write-Host "============================================================" -ForegroundColor Red

    $Choice = Read-Host "Do you accept the terms of the PEFL v1.0 license? (y/N)"
    if ($Choice -in "y", "yes") {
        Write-Host "License accepted." -ForegroundColor Green
        $Remember = Read-Host "Do you want to remember this approval for the next 7 days in this folder? (y/N)"
        if ($Remember -in "y", "yes") {
            try {
                $CurrentTime | Out-File -FilePath $CacheFile -Encoding ascii -Force
            } catch {}
        }
    } else {
        Write-Host "License rejected. Exiting." -ForegroundColor Red
        exit 1
    }
}

Show-PeflLicenseAndPrompt

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "  SmartCam Installer Builder (makeInstaller v$Version — PEFL v1.0)" -ForegroundColor Cyan
Write-Host "======================================================================" -ForegroundColor Cyan

# ----------------------------------------------------------------------------
# Step 1: Verify / Compile C++ SIMD Variants (out_avx2 & out_avx512)
# ----------------------------------------------------------------------------
$BuildRoot = Join-Path $RepoRoot "platforms\windows\build"
$Avx2EngineDll = Join-Path $BuildRoot "out_avx2\smartcam_engine.dll"
$Avx2MfDll     = Join-Path $BuildRoot "out_avx2\smartcam_mf_source.dll"
$Avx512EngineDll = Join-Path $BuildRoot "out_avx512\smartcam_engine.dll"
$Avx512MfDll     = Join-Path $BuildRoot "out_avx512\smartcam_mf_source.dll"

if ($Rebuild -or -not (Test-Path $Avx2EngineDll) -or -not (Test-Path $Avx512EngineDll)) {
    Write-Host "[1/5] Compiling C++ SIMD Engines & Media Foundation Virtual Camera DLLs (AVX2 + AVX-512)..." -ForegroundColor Yellow
    $BuildScriptArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $BuildRoot "build_windows.ps1"), "-Variant", "All", "-NonInteractiveAcceptPefl")
    if ($Rebuild) {
        $BuildScriptArgs += "-Clean"
    }
    & powershell.exe @BuildScriptArgs
    if ($LASTEXITCODE -ne 0) { throw "C++ dual-variant build failed." }
} else {
    Write-Host "[1/5] Verified C++ SIMD binaries (out_avx2 & out_avx512 are up-to-date)." -ForegroundColor Green
}

# ----------------------------------------------------------------------------
# Step 2: Verify / Compile Native Rust Desktop Application (SmartCam.exe)
# ----------------------------------------------------------------------------
$RustExe = Join-Path $RepoRoot "SmartCam.exe"
$CargoManifest = Join-Path $RepoRoot "app\Cargo.toml"

if ($Rebuild -or -not (Test-Path $RustExe)) {
    Write-Host "[2/5] Compiling Native Rust Desktop & System Tray App (SmartCam.exe)..." -ForegroundColor Yellow
    $MachinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $UserPath    = [Environment]::GetEnvironmentVariable("Path", "User")
    foreach ($entry in ($MachinePath + ";" + $UserPath).Split(";", [System.StringSplitOptions]::RemoveEmptyEntries)) {
        if (($env:Path -split ";") -notcontains $entry) {
            $env:Path += ";$entry"
        }
    }
    $CargoPaths = @(
        "$env:USERPROFILE\.cargo\bin",
        (Get-ChildItem "C:\Program Files\Rust*\bin" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }),
        (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\Rustlang.Rust.GNU*\*\bin" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }),
        (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT*\mingw64\bin" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    ) | Where-Object { $_ -and (Test-Path $_) }
    foreach ($p in $CargoPaths) {
        if ($env:PATH -notlike "*$p*") { $env:PATH = "$p;$env:PATH" }
    }
    Push-Location (Join-Path $RepoRoot "app")
    try {
        if ($Rebuild) {
            & cargo clean
        }
        & cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "Rust cargo build --release failed." }
        Copy-Item -Path "target\release\SmartCam.exe" -Destination $RustExe -Force
    } finally {
        Pop-Location
    }
} else {
    Write-Host "[2/5] Verified Native Rust Desktop executable (SmartCam.exe is up-to-date)." -ForegroundColor Green
}

# Populate ready-to-use repository root bin\ directory with all compiled binaries
function Copy-ItemSafeOverwrite {
    param([string]$Source, [string]$Destination)
    if (-not (Test-Path $Source)) { return }
    if (Test-Path $Destination) {
        try {
            Copy-Item -Path $Source -Destination $Destination -Force -ErrorAction Stop
            return
        } catch {
            $BackupName = "$Destination.old_$([guid]::NewGuid().ToString('N').Substring(0,6))"
            Rename-Item -Path $Destination -NewName (Split-Path $BackupName -Leaf) -Force -ErrorAction SilentlyContinue
        }
    }
    Copy-Item -Path $Source -Destination $Destination -Force
}

$RepoBinDir = Join-Path $RepoRoot "bin"
if (-not (Test-Path $RepoBinDir)) {
    New-Item -ItemType Directory -Path $RepoBinDir -Force | Out-Null
}
$oldBinFiles = @(Get-ChildItem -Path $RepoBinDir -Filter "*.old_*" -File -ErrorAction SilentlyContinue)
foreach ($f in $oldBinFiles) {
    if ($f -and $f.FullName) {
        try { Remove-Item -Path $f.FullName -Force -ErrorAction Stop } catch {}
    }
}
Copy-ItemSafeOverwrite -Source $RustExe -Destination (Join-Path $RepoBinDir "SmartCam.exe")
Copy-ItemSafeOverwrite -Source (Join-Path $BuildRoot "out_avx2\smartcam_engine_avx2.dll") -Destination (Join-Path $RepoBinDir "smartcam_engine_avx2.dll")
Copy-ItemSafeOverwrite -Source (Join-Path $BuildRoot "out_avx512\smartcam_engine_avx512.dll") -Destination (Join-Path $RepoBinDir "smartcam_engine_avx512.dll")
Copy-ItemSafeOverwrite -Source (Join-Path $BuildRoot "out_avx2\smartcam_engine.dll") -Destination (Join-Path $RepoBinDir "smartcam_engine.dll")
Copy-ItemSafeOverwrite -Source (Join-Path $BuildRoot "out_avx2\smartcam_win_bridge.dll") -Destination (Join-Path $RepoBinDir "smartcam_win_bridge.dll")
Copy-ItemSafeOverwrite -Source (Join-Path $BuildRoot "out_avx2\smartcam_mf_source.dll") -Destination (Join-Path $RepoBinDir "smartcam_mf_source.dll")

# ----------------------------------------------------------------------------
# Step 3: Stage Clean Distribution Package
# ----------------------------------------------------------------------------
$DistDir = Join-Path $RepoRoot "dist"
$StagingRoot = Join-Path $DistDir "staging"
$StageAppDir = Join-Path $StagingRoot "SmartCam"

if (Test-Path $StagingRoot) {
    Remove-Item -Path $StagingRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $StageAppDir -Force | Out-Null

Write-Host "[3/5] Staging SmartCam v$Version distribution files..." -ForegroundColor Cyan

# Copy Root Licenses, Docs & Main Native Executable
Copy-Item -Path (Join-Path $RepoRoot "LICENSE") -Destination $StageAppDir -Force
Copy-Item -Path (Join-Path $RepoRoot "LICENSE.it") -Destination $StageAppDir -Force
Copy-Item -Path (Join-Path $RepoRoot "NOTICE.md") -Destination $StageAppDir -Force
Copy-Item -Path (Join-Path $RepoRoot "README.md") -Destination $StageAppDir -Force
if (Test-Path (Join-Path $RepoRoot "README_EN.md")) {
    Copy-Item -Path (Join-Path $RepoRoot "README_EN.md") -Destination $StageAppDir -Force
}
Copy-Item -Path $RustExe -Destination $StageAppDir -Force

# Copy Ready-to-Use bin\ Directory
$StageBinDir = Join-Path $StageAppDir "bin"
New-Item -ItemType Directory -Path $StageBinDir -Force | Out-Null
Get-ChildItem -Path "$RepoBinDir\*" -File -Include "*.exe", "*.dll" | ForEach-Object {
    Copy-Item -Path $_.FullName -Destination (Join-Path $StageBinDir $_.Name) -Force
}

# Copy Config (exclude machine-specific hardware_benchmark.yaml so fresh installs run first-boot auto-tuning)
$StageConfigDir = Join-Path $StageAppDir "config"
New-Item -ItemType Directory -Path $StageConfigDir -Force | Out-Null
Copy-Item -Path (Join-Path $RepoRoot "config\*") -Destination $StageConfigDir -Recurse -Force
Remove-Item -Path (Join-Path $StageConfigDir "hardware_benchmark.yaml") -Force -ErrorAction SilentlyContinue

# Create External Plugins Directory with C ABI v1 SDK Header
$StagePluginsDir = Join-Path $StageAppDir "plugins"
New-Item -ItemType Directory -Path $StagePluginsDir -Force | Out-Null
if (Test-Path (Join-Path $RepoRoot "common\plugins\iplugin_source.h")) {
    Copy-Item -Path (Join-Path $RepoRoot "common\plugins\iplugin_source.h") -Destination $StagePluginsDir -Force
}
if (Test-Path (Join-Path $RepoRoot "plugins")) {
    Copy-Item -Path (Join-Path $RepoRoot "plugins\*") -Destination $StagePluginsDir -Recurse -Force -ErrorAction SilentlyContinue
}

# Copy Assets (App Icons, Multi-Scale PNG/ICO Themes & Studio Background Presets)
$RepoAssetsDir = Join-Path $RepoRoot "assets"
if (Test-Path $RepoAssetsDir) {
    $StageAssetsDir = Join-Path $StageAppDir "assets"
    New-Item -ItemType Directory -Path $StageAssetsDir -Force | Out-Null
    Copy-Item -Path (Join-Path $RepoAssetsDir "*") -Destination $StageAssetsDir -Recurse -Force
}

# Copy Web Preview Side-by-Side Testbench
$StageWebDir = Join-Path $StageAppDir "app\web_preview"
New-Item -ItemType Directory -Path $StageWebDir -Force | Out-Null
Copy-Item -Path (Join-Path $RepoRoot "app\web_preview\*") -Destination $StageWebDir -Recurse -Force

# Copy Windows Scripts
$StageScriptsDir = Join-Path $StageAppDir "platforms\windows\scripts"
New-Item -ItemType Directory -Path $StageScriptsDir -Force | Out-Null
Copy-Item -Path (Join-Path $RepoRoot "platforms\windows\scripts\*.ps1") -Destination $StageScriptsDir -Force
Copy-Item -Path (Join-Path $RepoRoot "platforms\windows\scripts\*.bat") -Destination $StageScriptsDir -Force

# Copy Compiled Dual-Variant Binaries (out_avx2 & out_avx512)
$StageAvx2Dir = Join-Path $StageAppDir "platforms\windows\build\out_avx2"
$StageAvx512Dir = Join-Path $StageAppDir "platforms\windows\build\out_avx512"
New-Item -ItemType Directory -Path $StageAvx2Dir -Force | Out-Null
New-Item -ItemType Directory -Path $StageAvx512Dir -Force | Out-Null

Copy-Item -Path (Join-Path $BuildRoot "out_avx2\*.dll") -Destination $StageAvx2Dir -Force
Copy-Item -Path (Join-Path $BuildRoot "out_avx512\*.dll") -Destination $StageAvx512Dir -Force

# Ensure no .pefl_accepted* cache files exist in staging, bin, or root so SmartCam.exe always prompts on launch
Get-ChildItem -Path $StageAppDir -Filter ".pefl_accepted*" -Recurse -Force -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
Get-ChildItem -Path $RepoRoot -Filter ".pefl_accepted_SmartCam*" -Recurse -Force -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue

# ----------------------------------------------------------------------------
# Step 4: Create Portable ZIP Archive
# ----------------------------------------------------------------------------
$PortableZipPath = Join-Path $DistDir "SmartCam_Portable_v${Version}_x64.zip"
if (Test-Path $PortableZipPath) {
    Remove-Item -Path $PortableZipPath -Force
}

Write-Host "[4/5] Creating Portable Archive: $PortableZipPath..." -ForegroundColor Cyan
Compress-Archive -Path (Join-Path $StageAppDir "*") -DestinationPath $PortableZipPath -CompressionLevel Optimal

# ----------------------------------------------------------------------------
# Step 5: Compile Standalone Single-File GUI Installer (SmartCam_Setup_v1.0.0_x64.exe)
# ----------------------------------------------------------------------------
$SetupExePath = Join-Path $DistDir "SmartCam_Setup_v${Version}_x64.exe"
if (Test-Path $SetupExePath) {
    Remove-Item -Path $SetupExePath -Force
}

Write-Host "[5/5] Compiling Standalone Single-File Windows GUI Installer: $SetupExePath..." -ForegroundColor Cyan

$InstallerCsSource = @"
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

using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Security.Principal;
using System.Windows.Forms;
using Microsoft.Win32;

[assembly: AssemblyTitle("SmartCam Setup Wizard")]
[assembly: AssemblyDescription("SmartCam Virtual Camera & Real-Time Video Suite Installer (PEFL v1.0)")]
[assembly: AssemblyCompany("Rubens Rainelli")]
[assembly: AssemblyProduct("SmartCam")]
[assembly: AssemblyCopyright("Copyright (c) 2026 Rubens Rainelli - PEFL v1.0")]
[assembly: AssemblyVersion("1.0.0.0")]

namespace SmartCamInstaller
{
    internal static class Program
    {
        [STAThread]
        private static int Main(string[] args)
        {
            bool silent = false;
            bool uninstall = false;
            string customDir = null;

            for (int i = 0; i < args.Length; i++)
            {
                string a = args[i].ToLowerInvariant();
                if (a == "/s" || a == "--silent" || a == "-silent") silent = true;
                else if (a == "/uninstall" || a == "--uninstall") uninstall = true;
                else if ((a == "/dir" || a == "--dir") && i + 1 < args.Length) customDir = args[++i];
            }

            string defaultDir = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "Programs",
                "SmartCam");
            string targetDir = string.IsNullOrEmpty(customDir) ? defaultDir : customDir;

            if (uninstall)
            {
                return RunUninstall(targetDir, silent);
            }

            if (silent)
            {
                return PerformInstallation(targetDir, true, true, false, null);
            }

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            using (SetupForm form = new SetupForm(targetDir))
            {
                Application.Run(form);
                return form.ExitCode;
            }
        }

        internal static int PerformInstallation(
            string installDir,
            bool createDesktopShortcut,
            bool registerDriverUac,
            bool enableAutostart,
            Action<string, int> progressCallback)
        {
            try
            {
                if (progressCallback != null) progressCallback("Creazione cartella di installazione...", 10);
                if (!Directory.Exists(installDir))
                {
                    Directory.CreateDirectory(installDir);
                }

                string tempZip = Path.Combine(Path.GetTempPath(), "SmartCam_Payload_" + Guid.NewGuid().ToString("N") + ".zip");
                if (progressCallback != null) progressCallback("Estrazione pacchetto SmartCam (Rust + C++ SIMD AVX2/AVX-512)...", 30);

                using (Stream resStream = Assembly.GetExecutingAssembly().GetManifestResourceStream("SmartCamPayload.zip"))
                {
                    if (resStream == null)
                        throw new InvalidOperationException("Embedded payload SmartCamPayload.zip not found.");
                    using (FileStream fs = new FileStream(tempZip, FileMode.Create, FileAccess.Write))
                    {
                        resStream.CopyTo(fs);
                    }
                }

                // Extract files overwriting existing ones
                using (ZipArchive archive = ZipFile.OpenRead(tempZip))
                {
                    foreach (ZipArchiveEntry entry in archive.Entries)
                    {
                        string fullPath = Path.Combine(installDir, entry.FullName.Replace('/', Path.DirectorySeparatorChar));
                        if (string.IsNullOrEmpty(entry.Name))
                        {
                            Directory.CreateDirectory(fullPath);
                            continue;
                        }
                        string parent = Path.GetDirectoryName(fullPath);
                        if (!string.IsNullOrEmpty(parent) && !Directory.Exists(parent))
                        {
                            Directory.CreateDirectory(parent);
                        }
                        entry.ExtractToFile(fullPath, true);
                    }
                }
                try { File.Delete(tempZip); } catch { }

                // Copy the installer itself as Uninstaller.exe inside installDir
                string selfPath = Process.GetCurrentProcess().MainModule.FileName;
                string uninstallerPath = Path.Combine(installDir, "Uninstall_SmartCam.exe");
                try { File.Copy(selfPath, uninstallerPath, true); } catch { }

                if (progressCallback != null) progressCallback("Configurazione collegamenti Desktop e Menu Start...", 65);
                string exePath = Path.Combine(installDir, "SmartCam.exe");
                string startMenuDir = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.StartMenu),
                    "Programs",
                    "SmartCam");
                if (!Directory.Exists(startMenuDir)) Directory.CreateDirectory(startMenuDir);
                CreateShortcut(Path.Combine(startMenuDir, "SmartCam.lnk"), exePath, installDir, "SmartCam");
                CreateShortcut(Path.Combine(startMenuDir, "Disinstalla SmartCam.lnk"), uninstallerPath, installDir, "Disinstalla SmartCam", "--uninstall");

                if (createDesktopShortcut)
                {
                    string desktopDir = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                    CreateShortcut(Path.Combine(desktopDir, "SmartCam.lnk"), exePath, installDir, "SmartCam");
                }

                if (enableAutostart)
                {
                    try
                    {
                        using (RegistryKey runKey = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run", true))
                        {
                            if (runKey != null)
                            {
                                runKey.SetValue("SmartCam", "\"" + exePath + "\" --minimized");
                            }
                        }
                    }
                    catch { }
                }

                // Register in Windows "Apps & Features" (HKCU Uninstall)
                try
                {
                    using (RegistryKey uninstKey = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartCam"))
                    {
                        if (uninstKey != null)
                        {
                            uninstKey.SetValue("DisplayName", "SmartCam (PEFL v1.0)");
                            uninstKey.SetValue("DisplayVersion", "1.0.0");
                            uninstKey.SetValue("Publisher", "Rubens Rainelli");
                            uninstKey.SetValue("DisplayIcon", exePath);
                            uninstKey.SetValue("InstallLocation", installDir);
                            uninstKey.SetValue("UninstallString", "\"" + uninstallerPath + "\" --uninstall");
                            uninstKey.SetValue("NoModify", 1, RegistryValueKind.DWord);
                            uninstKey.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                        }
                    }
                }
                catch { }

                // Always register SmartCam COM + DirectShow VideoInputDeviceCategory in HKCU (works without UAC!)
                try
                {
                    string mfDll = Path.Combine(installDir, @"platforms\windows\build\out_avx2\smartcam_mf_source.dll");
                    if (!File.Exists(mfDll))
                    {
                        string binDll = Path.Combine(installDir, @"bin\smartcam_mf_source.dll");
                        if (File.Exists(binDll)) mfDll = binDll;
                    }
                    string clsid = "{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}";
                    string dshowCat = "{860BB310-5D01-11d0-BD3B-00A0C911CE86}";
                    byte[] filterData = new byte[] {
                        0x02,0x00,0x00,0x00, 0x00,0x00,0x20,0x00, 0x01,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,
                        0x30,0x70,0x69,0x33, 0x08,0x00,0x00,0x00, 0x00,0x00,0x00,0x00, 0x01,0x00,0x00,0x00,
                        0x00,0x00,0x00,0x00, 0x00,0x00,0x00,0x00, 0x30,0x74,0x79,0x33, 0x00,0x00,0x00,0x00,
                        0x38,0x00,0x00,0x00, 0x48,0x00,0x00,0x00, 0x76,0x69,0x64,0x73, 0x00,0x00,0x10,0x00,
                        0x80,0x00,0x00,0xaa, 0x00,0x38,0x9b,0x71, 0x00,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,
                        0x00,0x00,0x00,0x00, 0x00,0x00,0x00,0x00
                    };
                    using (RegistryKey ck = Registry.CurrentUser.CreateSubKey(@"Software\Classes\CLSID\" + clsid))
                    {
                        if (ck != null) ck.SetValue("", "SmartCam", RegistryValueKind.String);
                    }
                    using (RegistryKey ik = Registry.CurrentUser.CreateSubKey(@"Software\Classes\CLSID\" + clsid + @"\InprocServer32"))
                    {
                        if (ik != null)
                        {
                            ik.SetValue("", mfDll, RegistryValueKind.String);
                            ik.SetValue("ThreadingModel", "Both", RegistryValueKind.String);
                        }
                    }
                    using (RegistryKey dk = Registry.CurrentUser.CreateSubKey(@"Software\Classes\CLSID\" + dshowCat + @"\Instance\" + clsid))
                    {
                        if (dk != null)
                        {
                            dk.SetValue("FriendlyName", "SmartCam", RegistryValueKind.String);
                            dk.SetValue("CLSID", clsid, RegistryValueKind.String);
                            dk.SetValue("DevicePath", "smartcam://virtual-camera-0", RegistryValueKind.String);
                            dk.SetValue("FilterData", filterData, RegistryValueKind.Binary);
                        }
                    }
                }
                catch { }

                if (registerDriverUac)
                {
                    if (progressCallback != null) progressCallback("Registrazione Driver SmartCam (DirectShow + Media Foundation)...", 85);
                    string scriptPath = Path.Combine(installDir, @"platforms\windows\scripts\install_driver.ps1");
                    if (File.Exists(scriptPath))
                    {
                        ProcessStartInfo psi = new ProcessStartInfo
                        {
                            FileName = "powershell.exe",
                            Arguments = "-NoProfile -ExecutionPolicy Bypass -File \"" + scriptPath + "\" -NonInteractiveAcceptPefl",
                            UseShellExecute = true,
                            Verb = "runas"
                        };
                        try
                        {
                            using (Process p = Process.Start(psi))
                            {
                                if (p != null) p.WaitForExit(30000);
                            }
                        }
                        catch { /* User declined UAC prompt; HKCU registration is already active */ }
                    }
                }

                if (progressCallback != null) progressCallback("Installazione completata con successo!", 100);
                return 0;
            }
            catch (Exception ex)
            {
                if (progressCallback != null) progressCallback("Errore: " + ex.Message, 0);
                return 1;
            }
        }

        private static int RunUninstall(string installDir, bool silent)
        {
            if (!silent)
            {
                DialogResult dr = MessageBox.Show(
                    "Vuoi disinstallare SmartCam e rimuovere la periferica virtuale da Windows?",
                    "Disinstallazione SmartCam",
                    MessageBoxButtons.YesNo,
                    MessageBoxIcon.Question);
                if (dr != DialogResult.Yes) return 0;
            }

            try
            {
                string uninstScript = Path.Combine(installDir, @"platforms\windows\scripts\uninstall_driver.ps1");
                if (File.Exists(uninstScript))
                {
                    ProcessStartInfo psi = new ProcessStartInfo
                    {
                        FileName = "powershell.exe",
                        Arguments = "-NoProfile -ExecutionPolicy Bypass -File \"" + uninstScript + "\" -NonInteractiveAcceptPefl",
                        UseShellExecute = true,
                        Verb = "runas"
                    };
                    try
                    {
                        using (Process p = Process.Start(psi))
                        {
                            if (p != null) p.WaitForExit(20000);
                        }
                    }
                    catch { }
                }

                using (RegistryKey runKey = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run", true))
                {
                    if (runKey != null) runKey.DeleteValue("SmartCam", false);
                }
                Registry.CurrentUser.DeleteSubKeyTree(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartCam", false);

                string desktopShortcut = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory), "SmartCam.lnk");
                if (File.Exists(desktopShortcut)) File.Delete(desktopShortcut);

                string startMenuDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.StartMenu), "Programs", "SmartCam");
                if (Directory.Exists(startMenuDir)) Directory.Delete(startMenuDir, true);

                if (!silent)
                {
                    MessageBox.Show("SmartCam è stato disinstallato correttamente.", "SmartCam", MessageBoxButtons.OK, MessageBoxIcon.Information);
                }
                return 0;
            }
            catch
            {
                return 1;
            }
        }

        private static void CreateShortcut(string shortcutPath, string targetPath, string workingDir, string description, string arguments = "")
        {
            try
            {
                Type shellType = Type.GetTypeFromProgID("WScript.Shell");
                if (shellType == null) return;
                dynamic shell = Activator.CreateInstance(shellType);
                dynamic shortcut = shell.CreateShortcut(shortcutPath);
                shortcut.TargetPath = targetPath;
                shortcut.WorkingDirectory = workingDir;
                shortcut.Description = description;
                if (!string.IsNullOrEmpty(arguments)) shortcut.Arguments = arguments;
                shortcut.Save();
            }
            catch { }
        }
    }

    internal sealed class SetupForm : Form
    {
        public int ExitCode { get; private set; }
        private readonly TextBox _dirBox;
        private readonly CheckBox _chkAcceptPefl;
        private readonly CheckBox _chkDesktop;
        private readonly CheckBox _chkDriverUac;
        private readonly CheckBox _chkAutostart;
        private readonly CheckBox _chkLaunchNow;
        private readonly ProgressBar _progress;
        private readonly Label _statusLabel;
        private readonly Button _installBtn;

        public SetupForm(string defaultInstallDir)
        {
            Text = "Installazione Guidata SmartCam v1.0.0 (PEFL v1.0)";
            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }
            Size = new Size(680, 600);
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.FromArgb(18, 22, 31);
            ForeColor = Color.FromArgb(235, 240, 248);
            Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);

            Label title = new Label
            {
                Text = "SmartCam — Installazione Guidata (1 Clic)",
                Font = new Font("Segoe UI", 15f, FontStyle.Bold),
                ForeColor = Color.FromArgb(96, 178, 255),
                Location = new Point(24, 18),
                Size = new Size(620, 32)
            };
            Controls.Add(title);

            Label subtitle = new Label
            {
                Text = "Webcam Virtuale Nativa Windows 11 • Sfocatura, AI & Telo Verde • Ottimizzata AVX2 / AVX-512",
                ForeColor = Color.FromArgb(165, 178, 198),
                Location = new Point(26, 50),
                Size = new Size(620, 22)
            };
            Controls.Add(subtitle);

            Label licHeader = new Label
            {
                Text = "1. Licenza d'Uso Obbligatoria — PEFL v1.0 (Clausola 3):",
                Font = new Font("Segoe UI", 9.5f, FontStyle.Bold),
                ForeColor = Color.FromArgb(255, 204, 102),
                Location = new Point(24, 80),
                Size = new Size(620, 20)
            };
            Controls.Add(licHeader);

            TextBox licBox = new TextBox
            {
                Multiline = true,
                ReadOnly = true,
                ScrollBars = ScrollBars.Vertical,
                BackColor = Color.FromArgb(12, 15, 22),
                ForeColor = Color.FromArgb(220, 226, 236),
                Font = new Font("Consolas", 8.5f),
                Location = new Point(24, 104),
                Size = new Size(616, 155),
                Text =
                    "Personal and Educational Free License (PEF License / PEFL) v1.0 - 2026-05-26\r\n" +
                    "Copyright (c) 2026 Rubens Rainelli (rubens@rainelli.it | https://github.com/RubensRainelli/)\r\n\r\n" +
                    "• USO GRATUITO: 100% gratuito per uso personale, domestico, didattico, scolastico, universitario e No-Profit.\r\n" +
                    "• DIVIETO DI USO AZIENDALE: Qualsiasi utilizzo in contesti aziendali, professionali, commerciali o su macchine aziendali è VIETATO senza autorizzazione scritta dell'autore e pagamento del relativo canone.\r\n" +
                    "• ESCLUSIONE ENTI RELIGIOSI: Luoghi di culto ed enti religiosi sono equiparati all'Uso Aziendale.\r\n" +
                    "• CLAUSOLA 3: Questa licenza deve essere approvata esplicitamente dall'utente prima dell'utilizzo."
            };
            Controls.Add(licBox);

            _chkAcceptPefl = new CheckBox
            {
                Text = "Ho letto e ACCETTO i termini della licenza PEFL v1.0 (confermo l'Uso Personale/Didattico)",
                Font = new Font("Segoe UI", 9.5f, FontStyle.Bold),
                ForeColor = Color.FromArgb(110, 231, 183),
                Location = new Point(24, 268),
                Size = new Size(616, 24),
                Checked = false
            };
            _chkAcceptPefl.CheckedChanged += (s, e) => { _installBtn.Enabled = _chkAcceptPefl.Checked; };
            Controls.Add(_chkAcceptPefl);

            Label dirLabel = new Label
            {
                Text = "2. Cartella di installazione:",
                Font = new Font("Segoe UI", 9.5f, FontStyle.Bold),
                Location = new Point(24, 302),
                Size = new Size(616, 20)
            };
            Controls.Add(dirLabel);

            _dirBox = new TextBox
            {
                Text = defaultInstallDir,
                BackColor = Color.FromArgb(28, 34, 48),
                ForeColor = Color.White,
                Location = new Point(24, 326),
                Size = new Size(510, 26)
            };
            Controls.Add(_dirBox);

            Button browseBtn = new Button
            {
                Text = "Sfoglia...",
                BackColor = Color.FromArgb(42, 52, 72),
                ForeColor = Color.White,
                FlatStyle = FlatStyle.Flat,
                Location = new Point(542, 324),
                Size = new Size(98, 28)
            };
            browseBtn.Click += (s, e) =>
            {
                using (FolderBrowserDialog fbd = new FolderBrowserDialog { SelectedPath = _dirBox.Text })
                {
                    if (fbd.ShowDialog(this) == DialogResult.OK) _dirBox.Text = fbd.SelectedPath;
                }
            };
            Controls.Add(browseBtn);

            _chkDesktop = new CheckBox
            {
                Text = "Crea icona di SmartCam sul Desktop",
                Location = new Point(24, 366),
                Size = new Size(600, 24),
                Checked = true
            };
            Controls.Add(_chkDesktop);

            _chkDriverUac = new CheckBox
            {
                Text = "Registra subito la Webcam Virtuale di Windows 11 (richiede 1 conferma Amministratore UAC)",
                Location = new Point(24, 394),
                Size = new Size(616, 24),
                Checked = true
            };
            Controls.Add(_chkDriverUac);

            _chkAutostart = new CheckBox
            {
                Text = "Avvia automaticamente SmartCam vicino all'orologio (System Tray) all'accensione del PC",
                Location = new Point(24, 422),
                Size = new Size(616, 24),
                Checked = true
            };
            Controls.Add(_chkAutostart);

            _chkLaunchNow = new CheckBox
            {
                Text = "Avvia SmartCam subito al termine dell'installazione",
                Location = new Point(24, 450),
                Size = new Size(616, 24),
                Checked = true
            };
            Controls.Add(_chkLaunchNow);

            _progress = new ProgressBar
            {
                Location = new Point(24, 486),
                Size = new Size(616, 18),
                Minimum = 0,
                Maximum = 100,
                Value = 0
            };
            Controls.Add(_progress);

            _statusLabel = new Label
            {
                Text = "Spunta la casella di accettazione della licenza PEFL v1.0 per abilitare l'installazione.",
                ForeColor = Color.FromArgb(165, 178, 198),
                Location = new Point(24, 510),
                Size = new Size(410, 36)
            };
            Controls.Add(_statusLabel);

            _installBtn = new Button
            {
                Text = "Installa SmartCam Ora",
                Font = new Font("Segoe UI", 10.5f, FontStyle.Bold),
                BackColor = Color.FromArgb(37, 99, 235),
                ForeColor = Color.White,
                FlatStyle = FlatStyle.Flat,
                Location = new Point(448, 508),
                Size = new Size(192, 38),
                Enabled = false
            };
            _installBtn.Click += OnInstallClick;
            Controls.Add(_installBtn);
        }

        private void OnInstallClick(object sender, EventArgs e)
        {
            _installBtn.Enabled = false;
            _chkAcceptPefl.Enabled = false;

            int code = Program.PerformInstallation(
                _dirBox.Text.Trim(),
                _chkDesktop.Checked,
                _chkDriverUac.Checked,
                _chkAutostart.Checked,
                (msg, pct) =>
                {
                    _statusLabel.Text = msg;
                    _progress.Value = Math.Max(0, Math.Min(100, pct));
                    Application.DoEvents();
                });

            ExitCode = code;
            if (code == 0)
            {
                string exePath = Path.Combine(_dirBox.Text.Trim(), "SmartCam.exe");
                if (_chkLaunchNow.Checked && File.Exists(exePath))
                {
                    try
                    {
                        Process.Start(new ProcessStartInfo
                        {
                            FileName = exePath,
                            WorkingDirectory = _dirBox.Text.Trim(),
                            UseShellExecute = true
                        });
                    }
                    catch { }
                }
                MessageBox.Show(
                    this,
                    "SmartCam è stato installato con successo!\r\n\r\n" +
                    "Trovi l'icona di SmartCam sul Desktop, nel Menu Start e vicino all'orologio di Windows.",
                    "Installazione Completata",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Information);
                Close();
            }
            else
            {
                _installBtn.Enabled = true;
            }
        }
    }
}
"@

$TempCsFile = Join-Path $DistDir "SmartCamInstaller.cs"
$TempEmbeddedZip = Join-Path $DistDir "SmartCamPayload.zip"
Copy-Item -Path $PortableZipPath -Destination $TempEmbeddedZip -Force
$InstallerCsSource | Out-File -FilePath $TempCsFile -Encoding utf8 -Force

# Locate .NET Framework C# compiler (csc.exe, pre-installed on every Windows 10/11 system)
$CscCandidates = @(
    "$env:WINDIR\Microsoft.NET\Framework64\v4.0.30319\csc.exe",
    "$env:WINDIR\Microsoft.NET\Framework\v4.0.30319\csc.exe"
)
$CscExe = $CscCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $CscExe) {
    throw "C# compiler (csc.exe) not found in $env:WINDIR\Microsoft.NET."
}

$SetupIconPath = Join-Path $RepoRoot "assets\icons\smartcam.ico"
$Win32IconArg = if (Test-Path $SetupIconPath) { "/win32icon:`"$SetupIconPath`"" } else { "" }

& $CscExe /nologo /target:winexe /optimize+ `
    /out:"$SetupExePath" `
    $Win32IconArg `
    /r:System.dll `
    /r:System.Core.dll `
    /r:System.Drawing.dll `
    /r:System.Windows.Forms.dll `
    /r:System.IO.Compression.dll `
    /r:System.IO.Compression.FileSystem.dll `
    /r:Microsoft.CSharp.dll `
    /resource:"$TempEmbeddedZip,SmartCamPayload.zip" `
    "$TempCsFile"

if ($LASTEXITCODE -ne 0 -or -not (Test-Path $SetupExePath)) {
    throw "Compilation of SmartCam_Setup_v${Version}_x64.exe failed."
}

Remove-Item -Path $TempCsFile -Force -ErrorAction SilentlyContinue
Remove-Item -Path $TempEmbeddedZip -Force -ErrorAction SilentlyContinue
Remove-Item -Path $StagingRoot -Recurse -Force -ErrorAction SilentlyContinue

# Generate SHA256SUMS.txt
$SumsFile = Join-Path $DistDir "SHA256SUMS.txt"
$SetupHash = (Get-FileHash -Path $SetupExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$ZipHash   = (Get-FileHash -Path $PortableZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$SumsLines = @(
    "# SmartCam v$Version Release Checksums (PEFL v1.0)",
    "$SetupHash  $(Split-Path -Leaf $SetupExePath)",
    "$ZipHash  $(Split-Path -Leaf $PortableZipPath)"
)
foreach ($binFile in @("SmartCam.exe", "smartcam_engine_avx2.dll", "smartcam_engine_avx512.dll", "smartcam_engine.dll", "smartcam_win_bridge.dll", "smartcam_mf_source.dll")) {
    $binPath = Join-Path $RepoBinDir $binFile
    if (Test-Path $binPath) {
        $bHash = (Get-FileHash -Path $binPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $SumsLines += "$bHash  bin/$binFile"
    }
}
$SumsLines | Out-File -FilePath $SumsFile -Encoding ascii -Force

$SetupSizeMb = [math]::Round((Get-Item $SetupExePath).Length / 1MB, 2)
$ZipSizeMb   = [math]::Round((Get-Item $PortableZipPath).Length / 1MB, 2)

Write-Host ""
Write-Host "======================================================================" -ForegroundColor Green
Write-Host "  makeInstaller COMPLETED SUCCESSFULLY!" -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Green
Write-Host "  1. Single-File GUI Installer : $SetupExePath ($SetupSizeMb MB)" -ForegroundColor White
Write-Host "  2. Portable ZIP Package      : $PortableZipPath ($ZipSizeMb MB)" -ForegroundColor White
Write-Host "  3. Ready-to-Use Binaries     : $RepoBinDir" -ForegroundColor White
Write-Host "  4. SHA-256 Manifest          : $SumsFile" -ForegroundColor White
Write-Host "======================================================================" -ForegroundColor Green
