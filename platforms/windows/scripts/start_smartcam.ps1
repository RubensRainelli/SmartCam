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
    Launches the native SmartCam real-time video pipeline (`SmartCam.exe`) at standard user privileges
    (NO UAC required, 100% Zero-Python).
.DESCRIPTION
    Enforces PEFL v1.0 Clause 3 foreground startup verification (with 7-day local cache),
    locates the compiled Rust/C++ `SmartCam.exe` binary, and launches it with the requested
    configuration parameters. Falls back to the built-in Windows Forms Native Device Manager GUI
    (`manage_device.ps1 -NativeGui`) if `SmartCam.exe` is still being compiled.
#>

param(
    [string]$ConfigPath = "",
    [string]$Mode = "",
    [string]$AspectRatio = "",
    [int]$CameraIndex = -1,
    [switch]$Headless = $false,
    [switch]$NonInteractiveAcceptPefl = $false
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($ScriptDir)) { $ScriptDir = "." }
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..\..")).Path

function Invoke-PeflVerification {
    if ($NonInteractiveAcceptPefl) { return }
    $CacheFile = Join-Path $ScriptDir ".pefl_accepted_start_smartcam"
    $CurrentTime = [int]([datetimeoffset]::Now.ToUnixTimeSeconds())
    $MaxAge = 7 * 24 * 60 * 60

    if (Test-Path $CacheFile) {
        try {
            $LastAcceptedText = (Get-Content -Path $CacheFile -ErrorAction SilentlyContinue | Select-Object -First 1).Trim()
            if ($LastAcceptedText -match '^\d+$') {
                $Age = $CurrentTime - [int]$LastAcceptedText
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
    Write-Host "TERMS OF USE (Clause 3 Foreground Verification):" -ForegroundColor Yellow
    Write-Host "  * FREE for Personal & Educational use."
    Write-Host "  * STRICTLY PROHIBITED for Corporate, Professional, or Business use"
    Write-Host "    without written authorization and fee payment."
    Write-Host "  * Excludes places of worship and religious organizations (corporate use).`n"
    Write-Host "============================================================" -ForegroundColor Red

    $Choice = Read-Host "Do you accept the terms of the PEFL v1.0 license? (y/N)"
    if ($Choice -in @("y", "yes", "s", "si")) {
        Write-Host "License accepted." -ForegroundColor Green
        $Remember = Read-Host "Do you want to remember this approval for the next 7 days in this folder? (y/N)"
        if ($Remember -in @("y", "yes", "s", "si")) {
            try { $CurrentTime | Out-File -FilePath $CacheFile -Encoding ascii -Force } catch {}
        }
    } else {
        Write-Host "License rejected. Exiting." -ForegroundColor Red
        exit 1
    }
}

Invoke-PeflVerification

if ([string]::IsNullOrEmpty($ConfigPath)) {
    $ConfigPath = Join-Path $RepoRoot "config\default_config.yaml"
}

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

$SmartCamExe = Find-SmartCamExecutable

if ($null -ne $SmartCamExe) {
    $ArgsList = @("--config", $ConfigPath)
    if ($Headless) {
        $ArgsList += "--headless"
    }
    if (-not [string]::IsNullOrEmpty($Mode)) {
        $ArgsList += @("--mode", $Mode)
    }
    if (-not [string]::IsNullOrEmpty($AspectRatio)) {
        $ArgsList += @("--aspect-ratio", $AspectRatio)
    }
    if ($CameraIndex -ge 0) {
        $ArgsList += @("--camera-index", "$CameraIndex")
    }

    Write-Host "[SmartCam Launcher] Starting native SmartCam.exe ($SmartCamExe)..." -ForegroundColor Cyan
    & $SmartCamExe @ArgsList
    exit $LASTEXITCODE
} else {
    Write-Host "[SmartCam Launcher] Native binary SmartCam.exe is not compiled yet; launching Windows Forms Native Device Manager GUI..." -ForegroundColor Yellow
    $ManageScript = Join-Path $ScriptDir "manage_device.ps1"
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $ManageScript -NativeGui -NonInteractiveAcceptPefl
    exit $LASTEXITCODE
}
