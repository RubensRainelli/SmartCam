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
    SmartCam Virtual Device Manager for Windows 11 (`platforms/windows/scripts/manage_device.ps1`).
.DESCRIPTION
    Provides complete 100% Zero-Python management of the SmartCam Windows 11 Media Foundation
    Virtual Camera (`CLSID {9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}`).
    - Enforces PEFL v1.0 Clause 3 foreground startup verification (with 7-day local cache).
    - Delegates to the native Rust desktop application `SmartCam.exe` when compiled, or runs the
      built-in 100% Native Windows Forms (.NET) Dark Desktop GUI (`-NativeGui`) if `SmartCam.exe`
      is still being compiled or `-NativeGui` is explicitly requested.
#>

param(
    [switch]$NativeGui = $false,
    [switch]$Cli = $false,
    [switch]$Status = $false,
    [switch]$RegisterDriver = $false,
    [switch]$UnregisterDriver = $false,
    [ValidateSet("out_avx2", "out_avx512", "out")]
    [string]$BuildVariant = "out_avx2",
    [switch]$OpenWebTestbench = $false,
    [switch]$NonInteractiveAcceptPefl = $false
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($ScriptDir)) { $ScriptDir = "." }
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..\..")).Path
$ConfigPath = Join-Path $RepoRoot "config\default_config.yaml"
$InstallScript = Join-Path $ScriptDir "install_driver.ps1"
$UninstallScript = Join-Path $ScriptDir "uninstall_driver.ps1"
$WebPreviewScript = Join-Path $ScriptDir "start_web_preview.ps1"
$SmartCamClsid = "{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}"

# ============================================================================
# 1. PEFL v1.0 Clause 3 Foreground License Check (Console & GUI Modal)
# ============================================================================

function Test-PeflCacheValid {
    param([string]$CacheFile)
    $CurrentTime = [int]([datetimeoffset]::Now.ToUnixTimeSeconds())
    $MaxAge = 7 * 24 * 60 * 60
    if (Test-Path $CacheFile) {
        try {
            $LastAcceptedText = (Get-Content -Path $CacheFile -ErrorAction SilentlyContinue | Select-Object -First 1).Trim()
            if ($LastAcceptedText -match '^\d+$') {
                $Age = $CurrentTime - [int]$LastAcceptedText
                if ($Age -ge 0 -and $Age -lt $MaxAge) {
                    return $true
                }
            }
        } catch {}
    }
    return $false
}

function Invoke-PeflVerification {
    param([bool]$UseGuiModal = $false)

    $CacheFile = Join-Path $ScriptDir ".pefl_accepted_manage_device"
    if ($NonInteractiveAcceptPefl) { return }
    if (Test-PeflCacheValid -CacheFile $CacheFile) { return }

    $CurrentTime = [int]([datetimeoffset]::Now.ToUnixTimeSeconds())

    if ($UseGuiModal) {
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing

        $PeflText = @"
Personal and Educational Free License (PEFL) v1.0
Copyright (c) 2026 Rubens Rainelli — rubens@rainelli.it
Specification: https://github.com/RubensRainelli/pefl-license-spec

TERMS OF USE (Clause 1, 2 & 3):
  • FREE OF CHARGE exclusively for Personal and Educational use.
  • STRICTLY PROHIBITED for Corporate, Professional, Commercial, or Business use
    without prior written authorization and fee payment.
  • Places of worship and religious organizations are classified as corporate use.

Do you accept the terms of the PEFL v1.0 license?
(Selecting 'Yes' will also remember your approval for 7 days in this folder.)
"@
        $Result = [System.Windows.Forms.MessageBox]::Show(
            $PeflText,
            "SmartCam — PEFL v1.0 License Verification (Clause 3)",
            [System.Windows.Forms.MessageBoxButtons]::YesNo,
            [System.Windows.Forms.MessageBoxIcon]::Warning
        )
        if ($Result -eq [System.Windows.Forms.DialogResult]::Yes) {
            try { $CurrentTime | Out-File -FilePath $CacheFile -Encoding ascii -Force } catch {}
            return
        } else {
            exit 1
        }
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
    if ($Choice -in "y", "yes", "s", "si") {
        Write-Host "License accepted." -ForegroundColor Green
        $Remember = Read-Host "Do you want to remember this approval for the next 7 days in this folder? (y/N)"
        if ($Remember -in "y", "yes", "s", "si") {
            try { $CurrentTime | Out-File -FilePath $CacheFile -Encoding ascii -Force } catch {}
        }
    } else {
        Write-Host "License rejected. Exiting." -ForegroundColor Red
        exit 1
    }
}

# ============================================================================
# 2. Native Executable Resolver, System Diagnostics & YAML Config Helpers
# ============================================================================

function Find-SmartCamExecutable {
    $CandidatePaths = @(
        (Join-Path $RepoRoot "SmartCam.exe"),
        (Join-Path $RepoRoot "app\SmartCam.exe"),
        (Join-Path $RepoRoot "app\target\release\SmartCam.exe"),
        (Join-Path $RepoRoot "app\target\release\smartcam.exe"),
        (Join-Path $RepoRoot "target\release\SmartCam.exe"),
        (Join-Path $RepoRoot "target\release\smartcam.exe"),
        (Join-Path $RepoRoot "platforms\windows\build\out_avx2\SmartCam.exe"),
        (Join-Path $RepoRoot "platforms\windows\build\out_avx2\Release\SmartCam.exe"),
        (Join-Path $RepoRoot "platforms\windows\build\out_avx512\SmartCam.exe"),
        (Join-Path $RepoRoot "platforms\windows\build\out_avx512\Release\SmartCam.exe"),
        (Join-Path $RepoRoot "platforms\windows\build\out\Release\SmartCam.exe"),
        (Join-Path $RepoRoot "app\target\debug\SmartCam.exe"),
        (Join-Path $RepoRoot "app\target\debug\smartcam.exe")
    )
    foreach ($Cand in $CandidatePaths) {
        if (Test-Path $Cand) {
            return (Resolve-Path $Cand).Path
        }
    }
    return $null
}

function Get-SmartCamDiagnostics {
    $RegClsidPath = "HKLM:\SOFTWARE\Classes\CLSID\$SmartCamClsid"
    $RegInprocPath = "$RegClsidPath\InprocServer32"
    $FrameServerKey = "HKLM:\SOFTWARE\Microsoft\Windows Media Foundation\Platform"

    $ClsidRegistered = Test-Path $RegClsidPath
    $InprocDll = ""
    $ThreadingModel = ""
    if (Test-Path $RegInprocPath) {
        try {
            $InprocDll = (Get-ItemProperty -Path $RegInprocPath -Name "(Default)" -ErrorAction SilentlyContinue)."(Default)"
            $ThreadingModel = (Get-ItemProperty -Path $RegInprocPath -Name "ThreadingModel" -ErrorAction SilentlyContinue)."ThreadingModel"
        } catch {}
    }

    $FrameServerEnabled = "Not Set (Default)"
    if (Test-Path $FrameServerKey) {
        try {
            $Val = (Get-ItemProperty -Path $FrameServerKey -Name "EnableFrameServerMode" -ErrorAction SilentlyContinue)."EnableFrameServerMode"
            if ($null -ne $Val) {
                $FrameServerEnabled = if ($Val -eq 1) { "Enabled (1)" } else { "Disabled ($Val)" }
            }
        } catch {}
    }

    $BuildRoot = Join-Path $RepoRoot "platforms\windows\build"
    $Avx2Dll = Join-Path $BuildRoot "out_avx2\smartcam_mf_source.dll"
    if (-not (Test-Path $Avx2Dll)) { $Avx2Dll = Join-Path $BuildRoot "out_avx2\Release\smartcam_mf_source.dll" }
    $Avx2Eng = Join-Path $BuildRoot "out_avx2\smartcam_engine.dll"
    if (-not (Test-Path $Avx2Eng)) { $Avx2Eng = Join-Path $BuildRoot "out_avx2\common_engine\Release\smartcam_engine.dll" }

    $Avx512Dll = Join-Path $BuildRoot "out_avx512\smartcam_mf_source.dll"
    if (-not (Test-Path $Avx512Dll)) { $Avx512Dll = Join-Path $BuildRoot "out_avx512\Release\smartcam_mf_source.dll" }
    $Avx512Eng = Join-Path $BuildRoot "out_avx512\smartcam_engine.dll"
    if (-not (Test-Path $Avx512Eng)) { $Avx512Eng = Join-Path $BuildRoot "out_avx512\common_engine\Release\smartcam_engine.dll" }

    $Cameras = @()
    try {
        $PnpCams = Get-CimInstance Win32_PnPEntity -ErrorAction SilentlyContinue |
            Where-Object { $_.PNPClass -in @('Camera', 'Image') -and $_.Status -eq 'OK' }
        $Idx = 0
        foreach ($Cam in $PnpCams) {
            $CamName = $Cam.Name
            if ($CamName -match '(^|[\s(_\-/])IR($|[\s)_/\-])|Infrared|Windows\s*Hello') {
                $Cameras += "[#$Idx] $CamName ⚠️ [IR Sensor]"
            } else {
                $Cameras += "[#$Idx] $CamName"
            }
            $Idx++
        }
    } catch {}
    if ($Cameras.Count -eq 0) {
        $Cameras += "[#0] Default Camera / Synthetic 1080p Fallback"
    }

    $NativeExePath = Find-SmartCamExecutable

    return [PSCustomObject]@{
        Clsid              = $SmartCamClsid
        ClsidRegistered    = $ClsidRegistered
        InprocDll          = $InprocDll
        DllExists          = (-not [string]::IsNullOrEmpty($InprocDll) -and (Test-Path $InprocDll))
        ThreadingModel     = $ThreadingModel
        FrameServerStatus  = $FrameServerEnabled
        Avx2MfDll          = $Avx2Dll
        Avx2HasMf          = (Test-Path $Avx2Dll)
        Avx2HasEngine      = (Test-Path $Avx2Eng)
        Avx512MfDll        = $Avx512Dll
        Avx512HasMf        = (Test-Path $Avx512Dll)
        Avx512HasEngine    = (Test-Path $Avx512Eng)
        NativeExe          = if ($null -ne $NativeExePath) { $NativeExePath } else { "Not compiled yet (WinForms Fallback Ready)" }
        HasNativeExe       = ($null -ne $NativeExePath)
        Cameras            = $Cameras
    }
}

function Read-ConfigValue {
    param([string]$Key, [string]$DefaultValue)
    if (-not (Test-Path $ConfigPath)) { return $DefaultValue }
    $Lines = Get-Content -Path $ConfigPath
    foreach ($Line in $Lines) {
        if ($Line -match "^\s*$Key\s*:\s*([^#]+)") {
            $Val = $Matches[1].Trim().Trim('"').Trim("'")
            return $Val
        }
    }
    return $DefaultValue
}

function Write-SmartCamConfig {
    param(
        [int]$DeviceIndex,
        [int]$Width,
        [int]$Height,
        [int]$Fps,
        [string]$AspectRatio,
        [string]$CropStrategy,
        [string]$SegMode,
        [int]$HueMin,
        [int]$HueMax,
        [int]$BlurRadius,
        [bool]$PreferIgpu,
        [string]$SimdTier,
        [string]$BgMode
    )

    $OutW = $Width
    $OutH = $Height
    if ($AspectRatio -eq "4:3") {
        $OutW = [int](($Height * 4) / 3) -band (-bnot 1)
        $OutH = $Height
    } elseif ($AspectRatio -eq "1:1") {
        $OutW = $Height
        $OutH = $Height
    } elseif ($AspectRatio -eq "9:16") {
        $OutW = [int](($Height * 9) / 16) -band (-bnot 1)
        $OutH = $Height
    }

    $IgpuStr = if ($PreferIgpu) { "true" } else { "false" }

    $YamlText = @"
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

camera_input:
  device_index: $DeviceIndex
  device_id: ""
  width: $Width
  height: $Height
  fps: $Fps

framing_output:
  aspect_ratio: "$AspectRatio"           # Options: "native", "16:9", "4:3", "1:1", "9:16", "custom"
  crop_strategy: "$CropStrategy"  # Options: "center_crop", "letterbox"
  output_width: $OutW            # Distortion-free output width
  output_height: $OutH

segmentation:
  mode: "$SegMode"          # Options: "passthrough", "green_screen", "ai_selfie"

  green_screen:
    hue_min: $HueMin                 # HSV Hue lower bound [0..180]
    hue_max: $HueMax                 # HSV Hue upper bound [0..180]
    sat_min: 45                 # HSV Saturation lower bound [0..255]
    sat_max: 255                # HSV Saturation upper bound [0..255]
    val_min: 40                 # HSV Value lower bound [0..255]
    val_max: 255                # HSV Value upper bound [0..255]
    morph_kernel_size: 3        # Morphological cleanup kernel (0 = off, 3 or 5)
    blur_radius: $BlurRadius              # Soft-edge feathering radius in pixels
    spill_suppression: 0.35     # Green halo suppression on subject edges [0.0..1.0]

  ai_selfie:
    model_path: "models/selfie_segmentation.onnx"
    input_width: 256
    input_height: 144
    confidence_threshold: 0.50
    temporal_smoothing: 0.25
    intra_op_threads: 2
    prefer_igpu_directml: $IgpuStr

compositor:
  simd_acceleration: "$SimdTier"     # Options: "auto", "avx512", "avx2", "scalar"
  background_mode: "$BgMode" # Options: "solid_color", "image", "blur"
  background_color_rgb: [24, 34, 56]
  background_image_path: "assets/backgrounds/preset_1_studio_blue.jpg"
  background_blur_radius: 15

virtual_camera:
  friendly_name: "SmartCam"
  backend: "media_foundation"   # Windows 11 native Media Foundation Virtual Camera
  shared_memory_name: "Local\\SmartCam_MF_SharedFrame_v1"
  app_icon_theme: "cyber_lens"
"@
    $YamlText | Set-Content -Path $ConfigPath -Encoding UTF8
}

function Invoke-DriverInstall {
    param([string]$Variant = "out_avx2")
    $Diag = Get-SmartCamDiagnostics
    $TargetDll = if ($Variant -eq "out_avx512") { $Diag.Avx512MfDll } else { $Diag.Avx2MfDll }
    $ArgsList = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$InstallScript`"", "-NonInteractiveAcceptPefl")
    if (Test-Path $TargetDll) {
        $ArgsList += @("-DllPath", "`"$TargetDll`"")
    }
    Start-Process powershell.exe -ArgumentList $ArgsList -Wait
}

function Invoke-DriverUninstall {
    $ArgsList = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$UninstallScript`"", "-NonInteractiveAcceptPefl")
    Start-Process powershell.exe -ArgumentList $ArgsList -Wait
}

function Open-WebTestbench {
    if (Test-Path $WebPreviewScript) {
        Start-Process powershell.exe -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$WebPreviewScript`" -NonInteractiveAcceptPefl"
    } else {
        $WebHtml = Join-Path $RepoRoot "app\web_preview\index.html"
        if (Test-Path $WebHtml) {
            Start-Process $WebHtml
        }
    }
}

# ============================================================================
# 3. Direct CLI / Headless Actions
# ============================================================================

$IsHeadless = ($Status -or $RegisterDriver -or $UnregisterDriver -or $OpenWebTestbench -or $Cli)
Invoke-PeflVerification -UseGuiModal:(-not $IsHeadless)

if ($RegisterDriver) {
    Invoke-DriverInstall -Variant $BuildVariant
    exit 0
}
if ($UnregisterDriver) {
    Invoke-DriverUninstall
    exit 0
}
if ($OpenWebTestbench) {
    Open-WebTestbench
    exit 0
}
if ($Status) {
    $Diag = Get-SmartCamDiagnostics
    Write-Host "======================================================================" -ForegroundColor Cyan
    Write-Host "  SmartCam Virtual Camera Device Manager — Windows 11 Diagnostics" -ForegroundColor White
    Write-Host "======================================================================" -ForegroundColor Cyan
    Write-Host "  * CLSID                  : $($Diag.Clsid)"
    Write-Host "  * Registry Status        : $(if ($Diag.ClsidRegistered) { 'REGISTERED' } else { 'NOT REGISTERED' })"
    Write-Host "  * InprocServer32 DLL     : $($Diag.InprocDll) (Exists=$($Diag.DllExists))"
    Write-Host "  * EnableFrameServerMode  : $($Diag.FrameServerStatus)"
    Write-Host "  * Build out_avx2         : MF_DLL=$($Diag.Avx2HasMf) | Engine_DLL=$($Diag.Avx2HasEngine)"
    Write-Host "  * Build out_avx512       : MF_DLL=$($Diag.Avx512HasMf) | Engine_DLL=$($Diag.Avx512HasEngine)"
    Write-Host "  * Native Rust App        : $($Diag.NativeExe)"
    Write-Host "  * Connected Cameras      : $($Diag.Cameras -join ', ')"
    Write-Host "======================================================================" -ForegroundColor Cyan
    exit 0
}

# ============================================================================
# 4. Delegate to Native Rust App (`SmartCam.exe`) if compiled & -NativeGui not forced
# ============================================================================

$SmartCamExe = Find-SmartCamExecutable

if (-not $NativeGui -and $null -ne $SmartCamExe) {
    try {
        if ($Cli) {
            & $SmartCamExe --headless --config $ConfigPath
        } else {
            & $SmartCamExe --config $ConfigPath
        }
        if ($LASTEXITCODE -eq 0) {
            exit 0
        }
    } catch {
        Write-Host "[SmartCam] Native SmartCam.exe launch encountered an error; launching native Windows Forms GUI fallback..." -ForegroundColor Yellow
    }
}

# ============================================================================
# 5. Native Windows Forms (.NET) Dark Desktop GUI (Zero-Python Fallback)
# ============================================================================

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$Form = New-Object System.Windows.Forms.Form
$Form.Text = "SmartCam Virtual Device Manager — Windows 11 Native GUI (PEFL v1.0)"
$Form.Size = New-Object System.Drawing.Size(920, 680)
$Form.StartPosition = "CenterScreen"
$Form.BackColor = [System.Drawing.Color]::FromArgb(15, 23, 42)
$Form.ForeColor = [System.Drawing.Color]::FromArgb(248, 250, 252)
$Form.FormBorderStyle = "FixedSingle"
$Form.MaximizeBox = $false

$FontHeader = New-Object System.Drawing.Font("Segoe UI", 14, [System.Drawing.FontStyle]::Bold)
$FontSection = New-Object System.Drawing.Font("Segoe UI", 10, [System.Drawing.FontStyle]::Bold)
$FontRegular = New-Object System.Drawing.Font("Segoe UI", 9)
$FontMono = New-Object System.Drawing.Font("Consolas", 9)

# Header Banner
$LblTitle = New-Object System.Windows.Forms.Label
$LblTitle.Text = "SmartCam Virtual Camera Device Manager (Windows 11)"
$LblTitle.Font = $FontHeader
$LblTitle.AutoSize = $true
$LblTitle.Location = New-Object System.Drawing.Point(20, 14)
$Form.Controls.Add($LblTitle)

$BtnWeb = New-Object System.Windows.Forms.Button
$BtnWeb.Text = "Open Side-by-Side Web Testbench"
$BtnWeb.Font = $FontSection
$BtnWeb.Size = New-Object System.Drawing.Size(260, 34)
$BtnWeb.Location = New-Object System.Drawing.Point(625, 12)
$BtnWeb.BackColor = [System.Drawing.Color]::FromArgb(2, 132, 199)
$BtnWeb.ForeColor = [System.Drawing.Color]::White
$BtnWeb.FlatStyle = "Flat"
$BtnWeb.Add_Click({ Open-WebTestbench })
$Form.Controls.Add($BtnWeb)

# Diagnostics GroupBox
$GrpDiag = New-Object System.Windows.Forms.GroupBox
$GrpDiag.Text = " Real-Time Driver Status & Media Foundation Diagnostics "
$GrpDiag.Font = $FontSection
$GrpDiag.ForeColor = [System.Drawing.Color]::FromArgb(56, 189, 248)
$GrpDiag.Location = New-Object System.Drawing.Point(20, 58)
$GrpDiag.Size = New-Object System.Drawing.Size(865, 190)
$Form.Controls.Add($GrpDiag)

$TxtDiag = New-Object System.Windows.Forms.TextBox
$TxtDiag.Multiline = $true
$TxtDiag.ReadOnly = $true
$TxtDiag.Font = $FontMono
$TxtDiag.BackColor = [System.Drawing.Color]::FromArgb(30, 41, 59)
$TxtDiag.ForeColor = [System.Drawing.Color]::FromArgb(226, 232, 240)
$TxtDiag.BorderStyle = "None"
$TxtDiag.Location = New-Object System.Drawing.Point(16, 26)
$TxtDiag.Size = New-Object System.Drawing.Size(540, 148)
$GrpDiag.Controls.Add($TxtDiag)

$LblVariant = New-Object System.Windows.Forms.Label
$LblVariant.Text = "Engine Build Variant:"
$LblVariant.Font = $FontRegular
$LblVariant.ForeColor = [System.Drawing.Color]::White
$LblVariant.Location = New-Object System.Drawing.Point(575, 24)
$LblVariant.AutoSize = $true
$GrpDiag.Controls.Add($LblVariant)

$CmbVariant = New-Object System.Windows.Forms.ComboBox
$CmbVariant.Font = $FontRegular
$CmbVariant.DropDownStyle = "DropDownList"
[void]$CmbVariant.Items.Add("out_avx2")
[void]$CmbVariant.Items.Add("out_avx512")
$CmbVariant.SelectedIndex = 0
$CmbVariant.Location = New-Object System.Drawing.Point(575, 46)
$CmbVariant.Size = New-Object System.Drawing.Size(270, 26)
$GrpDiag.Controls.Add($CmbVariant)

$BtnInstall = New-Object System.Windows.Forms.Button
$BtnInstall.Text = "Register Virtual Camera Driver (UAC)"
$BtnInstall.Font = $FontSection
$BtnInstall.Location = New-Object System.Drawing.Point(575, 82)
$BtnInstall.Size = New-Object System.Drawing.Size(270, 34)
$BtnInstall.BackColor = [System.Drawing.Color]::FromArgb(37, 99, 235)
$BtnInstall.ForeColor = [System.Drawing.Color]::White
$BtnInstall.FlatStyle = "Flat"
$GrpDiag.Controls.Add($BtnInstall)

$BtnUninstall = New-Object System.Windows.Forms.Button
$BtnUninstall.Text = "Uninstall Virtual Camera Driver (UAC)"
$BtnUninstall.Font = $FontSection
$BtnUninstall.Location = New-Object System.Drawing.Point(575, 124)
$BtnUninstall.Size = New-Object System.Drawing.Size(270, 34)
$BtnUninstall.BackColor = [System.Drawing.Color]::FromArgb(153, 27, 27)
$BtnUninstall.ForeColor = [System.Drawing.Color]::White
$BtnUninstall.FlatStyle = "Flat"
$GrpDiag.Controls.Add($BtnUninstall)

# Configuration GroupBox
$GrpCfg = New-Object System.Windows.Forms.GroupBox
$GrpCfg.Text = " Live Pipeline Configuration (config/default_config.yaml) "
$GrpCfg.Font = $FontSection
$GrpCfg.ForeColor = [System.Drawing.Color]::FromArgb(56, 189, 248)
$GrpCfg.Location = New-Object System.Drawing.Point(20, 260)
$GrpCfg.Size = New-Object System.Drawing.Size(865, 290)
$Form.Controls.Add($GrpCfg)

function Add-ConfigComboRow {
    param($Parent, [string]$LabelText, [int]$X, [int]$Y, [string[]]$Items, [string]$DefaultItem)
    $Lbl = New-Object System.Windows.Forms.Label
    $Lbl.Text = $LabelText
    $Lbl.Font = $FontRegular
    $Lbl.ForeColor = [System.Drawing.Color]::White
    $Lbl.Location = New-Object System.Drawing.Point($X, $Y)
    $Lbl.Size = New-Object System.Drawing.Size(210, 22)
    $Parent.Controls.Add($Lbl)

    $Cmb = New-Object System.Windows.Forms.ComboBox
    $Cmb.Font = $FontRegular
    $Cmb.DropDownStyle = "DropDownList"
    foreach ($It in $Items) { [void]$Cmb.Items.Add($It) }
    $Idx = $Cmb.Items.IndexOf($DefaultItem)
    $Cmb.SelectedIndex = if ($Idx -ge 0) { $Idx } else { 0 }
    $Cmb.Location = New-Object System.Drawing.Point(($X + 215), ($Y - 2))
    $Cmb.Size = New-Object System.Drawing.Size(185, 26)
    $Parent.Controls.Add($Cmb)
    return $Cmb
}

$InitDiag = Get-SmartCamDiagnostics
$CamDropdownItems = if ($InitDiag.Cameras.Count -gt 0) { $InitDiag.Cameras } else { @("[#0] Default Camera") }
$DefaultCamItem = $CamDropdownItems[0]
foreach ($Item in $CamDropdownItems) {
    if ($Item -notmatch "IR Sensor") {
        $DefaultCamItem = $Item
        break
    }
}

$CmbCamIdx = Add-ConfigComboRow $GrpCfg "Hardware Camera:" 20 35 $CamDropdownItems $DefaultCamItem
$CmbRes    = Add-ConfigComboRow $GrpCfg "Capture Resolution:" 20 75 @("1920x1080", "1280x720") "1920x1080"
$CmbFps    = Add-ConfigComboRow $GrpCfg "Capture FPS:" 20 115 @("30", "60") (Read-ConfigValue "fps" "30")
$CmbAspect = Add-ConfigComboRow $GrpCfg "Output Aspect Ratio:" 20 155 @("4:3", "16:9", "1:1", "9:16", "native") (Read-ConfigValue "aspect_ratio" "4:3")
$CmbCrop   = Add-ConfigComboRow $GrpCfg "Framing Strategy:" 20 195 @("center_crop", "letterbox") (Read-ConfigValue "crop_strategy" "center_crop")

$CmbMode   = Add-ConfigComboRow $GrpCfg "Segmentation Mode:" 445 35 @("green_screen", "ai_selfie", "passthrough") (Read-ConfigValue "mode" "green_screen")
$CmbBg     = Add-ConfigComboRow $GrpCfg "Background Mode:" 445 75 @("solid_color", "blur", "image") (Read-ConfigValue "background_mode" "solid_color")
$CmbSimd   = Add-ConfigComboRow $GrpCfg "SIMD Acceleration:" 445 115 @("auto", "avx512", "avx2", "scalar") (Read-ConfigValue "simd_acceleration" "auto")
$CmbHueMin = Add-ConfigComboRow $GrpCfg "Green Screen Hue Min:" 445 155 @("25", "35", "45") (Read-ConfigValue "hue_min" "35")
$CmbHueMax = Add-ConfigComboRow $GrpCfg "Green Screen Hue Max:" 445 195 @("75", "85", "95") (Read-ConfigValue "hue_max" "85")

$ChkIgpu = New-Object System.Windows.Forms.CheckBox
$ChkIgpu.Text = "Prefer DirectML Integrated GPU (iGPU) for AI Selfie Segmentation"
$ChkIgpu.Font = $FontRegular
$ChkIgpu.ForeColor = [System.Drawing.Color]::White
$ChkIgpu.Checked = $true
$ChkIgpu.Location = New-Object System.Drawing.Point(20, 240)
$ChkIgpu.Size = New-Object System.Drawing.Size(500, 24)
$GrpCfg.Controls.Add($ChkIgpu)

# Bottom Runtime Action Buttons
$Script:StreamProcess = $null

$LblStatus = New-Object System.Windows.Forms.Label
$LblStatus.Text = "Status: Ready (Standard User Mode — 100% Zero-Python)"
$LblStatus.Font = $FontMono
$LblStatus.ForeColor = [System.Drawing.Color]::FromArgb(148, 163, 184)
$LblStatus.Location = New-Object System.Drawing.Point(20, 610)
$LblStatus.Size = New-Object System.Drawing.Size(865, 22)
$Form.Controls.Add($LblStatus)

function Update-DiagnosticsBox {
    $D = Get-SmartCamDiagnostics
    $RegStr = if ($D.ClsidRegistered) { "REGISTERED (Ready)" } else { "NOT REGISTERED (Click Register UAC)" }
    $TxtDiag.Text = (
        "CLSID               : $($D.Clsid)`r`n" +
        "Registry Status     : $RegStr`r`n" +
        "InprocServer32 DLL  : $($D.InprocDll)`r`n" +
        "FrameServer Mode    : $($D.FrameServerStatus)`r`n" +
        "Build out_avx2      : MF_DLL=$($D.Avx2HasMf) | SIMD_Engine=$($D.Avx2HasEngine)`r`n" +
        "Build out_avx512    : MF_DLL=$($D.Avx512HasMf) | SIMD_Engine=$($D.Avx512HasEngine)`r`n" +
        "Native Executable   : $($D.NativeExe)`r`n" +
        "Physical Webcams    : $($D.Cameras -join '; ')"
    )
}

function Save-GuiConfig {
    $ResParts = $CmbRes.SelectedItem.ToString().Split("x")
    $CamStr = $CmbCamIdx.SelectedItem.ToString()
    $ParsedCamIdx = 0
    if ($CamStr -match '^\[#(\d+)\]') {
        $ParsedCamIdx = [int]$Matches[1]
    } elseif ($CamStr -match '^\d+$') {
        $ParsedCamIdx = [int]$CamStr
    }
    Write-SmartCamConfig `
        -DeviceIndex $ParsedCamIdx `
        -Width ([int]$ResParts[0]) `
        -Height ([int]$ResParts[1]) `
        -Fps ([int]$CmbFps.SelectedItem) `
        -AspectRatio $CmbAspect.SelectedItem.ToString() `
        -CropStrategy $CmbCrop.SelectedItem.ToString() `
        -SegMode $CmbMode.SelectedItem.ToString() `
        -HueMin ([int]$CmbHueMin.SelectedItem) `
        -HueMax ([int]$CmbHueMax.SelectedItem) `
        -BlurRadius 2 `
        -PreferIgpu $ChkIgpu.Checked `
        -SimdTier $CmbSimd.SelectedItem.ToString() `
        -BgMode $CmbBg.SelectedItem.ToString()
    $LblStatus.Text = "Status: Saved configuration to $ConfigPath"
}

$BtnSave = New-Object System.Windows.Forms.Button
$BtnSave.Text = "Save Configuration (default_config.yaml)"
$BtnSave.Font = $FontSection
$BtnSave.Location = New-Object System.Drawing.Point(20, 562)
$BtnSave.Size = New-Object System.Drawing.Size(280, 38)
$BtnSave.BackColor = [System.Drawing.Color]::FromArgb(37, 99, 235)
$BtnSave.ForeColor = [System.Drawing.Color]::White
$BtnSave.FlatStyle = "Flat"
$BtnSave.Add_Click({ Save-GuiConfig })
$Form.Controls.Add($BtnSave)

$BtnStart = New-Object System.Windows.Forms.Button
$BtnStart.Text = "Start Virtual Camera Stream"
$BtnStart.Font = $FontSection
$BtnStart.Location = New-Object System.Drawing.Point(315, 562)
$BtnStart.Size = New-Object System.Drawing.Size(280, 38)
$BtnStart.BackColor = [System.Drawing.Color]::FromArgb(16, 185, 129)
$BtnStart.ForeColor = [System.Drawing.Color]::White
$BtnStart.FlatStyle = "Flat"
$Form.Controls.Add($BtnStart)

$BtnStop = New-Object System.Windows.Forms.Button
$BtnStop.Text = "Stop Virtual Camera Stream"
$BtnStop.Font = $FontSection
$BtnStop.Location = New-Object System.Drawing.Point(610, 562)
$BtnStop.Size = New-Object System.Drawing.Size(275, 38)
$BtnStop.BackColor = [System.Drawing.Color]::FromArgb(127, 29, 29)
$BtnStop.ForeColor = [System.Drawing.Color]::White
$BtnStop.FlatStyle = "Flat"
$BtnStop.Enabled = $false
$Form.Controls.Add($BtnStop)

$BtnInstall.Add_Click({
    $Var = $CmbVariant.SelectedItem.ToString()
    $LblStatus.Text = "Status: Elevating via UAC to register driver ($Var)..."
    Invoke-DriverInstall -Variant $Var
    Update-DiagnosticsBox
    $LblStatus.Text = "Status: Driver registration complete ($Var)."
})

$BtnUninstall.Add_Click({
    $LblStatus.Text = "Status: Elevating via UAC to uninstall driver..."
    Invoke-DriverUninstall
    Update-DiagnosticsBox
    $LblStatus.Text = "Status: Driver removal complete."
})

$BtnStart.Add_Click({
    Save-GuiConfig
    $ResolvedExe = Find-SmartCamExecutable
    if ($null -ne $ResolvedExe) {
        $Script:StreamProcess = Start-Process $ResolvedExe -ArgumentList "--headless --config `"$ConfigPath`"" -PassThru -WindowStyle Hidden
        $BtnStart.Enabled = $false
        $BtnStop.Enabled = $true
        $LblStatus.Text = "Status: Streaming LIVE via SmartCam.exe (PID $($Script:StreamProcess.Id)) — Standard User Mode"
    } else {
        [System.Windows.Forms.MessageBox]::Show(
            "Configuration saved to config/default_config.yaml.`nNative binary SmartCam.exe is currently being compiled or not found in build output.",
            "SmartCam Native Stream Launcher",
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Information
        )
    }
})

$BtnStop.Add_Click({
    if ($null -ne $Script:StreamProcess -and -not $Script:StreamProcess.HasExited) {
        Stop-Process -Id $Script:StreamProcess.Id -Force -ErrorAction SilentlyContinue
    }
    $Script:StreamProcess = $null
    $BtnStart.Enabled = $true
    $BtnStop.Enabled = $false
    $LblStatus.Text = "Status: Virtual Camera stream stopped."
})

$Form.Add_FormClosing({
    if ($null -ne $Script:StreamProcess -and -not $Script:StreamProcess.HasExited) {
        Stop-Process -Id $Script:StreamProcess.Id -Force -ErrorAction SilentlyContinue
    }
})

Update-DiagnosticsBox
[void]$Form.ShowDialog()
