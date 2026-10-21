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
    One-time elevated (UAC) installer and registrator for the SmartCam Windows 11 Media Foundation
    Virtual Camera COM MediaSource (`CLSID_SmartCamMfVirtualCameraSource`).
.DESCRIPTION
    Registers `smartcam_mf_source.dll` under HKLM\SOFTWARE\Classes\CLSID\{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}
    and configures Windows 11 FrameServer permissions so the native SmartCam runtime (`SmartCam.exe`) can
    operate at standard user privileges without requiring UAC on every launch.
#>

param(
    [string]$DllPath = "",
    [switch]$NonInteractiveAcceptPefl = $false
)

$ErrorActionPreference = "Stop"

function Show-PeflLicenseAndPrompt {
    if ($NonInteractiveAcceptPefl) { return }
    $ScriptDir = if (-not [string]::IsNullOrEmpty($PSScriptRoot)) { $PSScriptRoot } else { "." }
    $ScriptStem = if (-not [string]::IsNullOrEmpty($PSCommandPath)) { [System.IO.Path]::GetFileNameWithoutExtension($PSCommandPath) } else { "install_driver" }
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

# Check for Administrator privileges; self-elevate via UAC if running as standard user
$CurrentIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()
$Principal = New-Object Security.Principal.WindowsPrincipal($CurrentIdentity)
$IsAdmin = $Principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

# Resolve smartcam_mf_source.dll path
if ([string]::IsNullOrEmpty($DllPath) -or -not (Test-Path $DllPath)) {
    $CandidatePaths = @(
        (Join-Path $PSScriptRoot "..\build\out_avx2\smartcam_mf_source.dll"),
        (Join-Path $PSScriptRoot "..\..\..\bin\smartcam_mf_source.dll"),
        (Join-Path $PSScriptRoot "..\build\out_avx512\smartcam_mf_source.dll"),
        (Join-Path $PSScriptRoot "smartcam_mf_source.dll"),
        (Join-Path $PSScriptRoot "..\build\out\Release\smartcam_mf_source.dll")
    )
    foreach ($Cand in $CandidatePaths) {
        if (Test-Path $Cand) {
            $DllPath = (Resolve-Path $Cand).Path
            break
        }
    }
}

$Clsid = "{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}"
$DShowCat = "{860BB310-5D01-11d0-BD3B-00A0C911CE86}"
$FilterData = [byte[]](
    0x02,0x00,0x00,0x00, 0x00,0x00,0x20,0x00, 0x01,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,
    0x30,0x70,0x69,0x33, 0x08,0x00,0x00,0x00, 0x00,0x00,0x00,0x00, 0x01,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00, 0x00,0x00,0x00,0x00, 0x30,0x74,0x79,0x33, 0x00,0x00,0x00,0x00,
    0x38,0x00,0x00,0x00, 0x48,0x00,0x00,0x00, 0x76,0x69,0x64,0x73, 0x00,0x00,0x10,0x00,
    0x80,0x00,0x00,0xaa, 0x00,0x38,0x9b,0x71, 0x00,0x00,0x00,0x00, 0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00, 0x00,0x00,0x00,0x00
)

function Register-SmartCamHive([string]$HivePrefix, [string]$TargetDll) {
    $RegClsidPath = "${HivePrefix}:\Software\Classes\CLSID\$Clsid"
    $RegInprocPath = "$RegClsidPath\InprocServer32"
    $RegDShowPath = "${HivePrefix}:\Software\Classes\CLSID\$DShowCat\Instance\$Clsid"

    New-Item -Path $RegClsidPath -Force | Out-Null
    Set-ItemProperty -Path $RegClsidPath -Name "(Default)" -Value "SmartCam"
    New-Item -Path $RegInprocPath -Force | Out-Null
    Set-ItemProperty -Path $RegInprocPath -Name "(Default)" -Value $TargetDll
    Set-ItemProperty -Path $RegInprocPath -Name "ThreadingModel" -Value "Both"

    New-Item -Path $RegDShowPath -Force | Out-Null
    New-ItemProperty -Path $RegDShowPath -Name "FriendlyName" -Value "SmartCam" -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $RegDShowPath -Name "CLSID" -Value $Clsid -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $RegDShowPath -Name "DevicePath" -Value "smartcam://virtual-camera-0" -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $RegDShowPath -Name "FilterData" -Value $FilterData -PropertyType Binary -Force | Out-Null
}

$EffectiveDll = if (-not [string]::IsNullOrEmpty($DllPath)) { $DllPath } else { (Join-Path $PSScriptRoot "..\build\out_avx2\smartcam_mf_source.dll") }

# 1. Always register in HKCU first (works immediately for Chromium, Edge, Firefox, OBS, Zoom, Teams without UAC)
Register-SmartCamHive -HivePrefix "HKCU" -TargetDll $EffectiveDll
Write-Host "[SmartCam Setup] Registered 'SmartCam' in HKCU (DirectShow + Media Foundation): $EffectiveDll" -ForegroundColor Green

# 2. If running as Administrator, also register system-wide in HKLM and run regsvr32
if ($IsAdmin) {
    if (Test-Path $EffectiveDll) {
        & regsvr32.exe /s "$EffectiveDll"
    }
    Register-SmartCamHive -HivePrefix "HKLM" -TargetDll $EffectiveDll
    $FrameServerKey = "HKLM:\SOFTWARE\Microsoft\Windows Media Foundation\Platform"
    if (Test-Path $FrameServerKey) {
        Set-ItemProperty -Path $FrameServerKey -Name "EnableFrameServerMode" -Value 1 -Type DWord -ErrorAction SilentlyContinue
    }
    Write-Host "[SmartCam Setup] Registered 'SmartCam' system-wide in HKLM." -ForegroundColor Green
}

Write-Host "[SmartCam Setup] SmartCam Virtual Camera CLSID $Clsid registered successfully!" -ForegroundColor Green
Write-Host "[SmartCam Setup] You can now select 'SmartCam' in Chromium, Chrome, Edge, Teams, Zoom, Meet, and OBS." -ForegroundColor Green
