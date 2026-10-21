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

param(
    [ValidateSet("Release", "Debug")]
    [string]$Configuration = "Release",
    [ValidateSet("All", "AVX2", "AVX512")]
    [string]$Variant = "All",
    [switch]$EnableAvx512 = $false,
    [switch]$RunTests = $true,
    [switch]$Clean = $false,
    [Alias("NonInteractiveAcceptPefl")]
    [switch]$AcceptPefl = $false
)

$ErrorActionPreference = "Stop"

function Show-PeflLicenseAndPrompt {
    $ScriptDir = if (-not [string]::IsNullOrEmpty($PSScriptRoot)) { $PSScriptRoot } else { "." }
    $ScriptName = "build_windows"
    $CacheFile = Join-Path $ScriptDir ".pefl_accepted_$ScriptName"

    $CurrentTime = [int]([datetimeoffset]::Now.ToUnixTimeSeconds())
    $MaxAge = 7 * 24 * 60 * 60

    if ($AcceptPefl) {
        try {
            $CurrentTime | Out-File -FilePath $CacheFile -Encoding ascii -Force
        } catch {}
        return
    }

    if (Test-Path $CacheFile) {
        try {
            $LastAcceptedText = (Get-Content -Path $CacheFile -ErrorAction SilentlyContinue | Select-Object -First 1).Trim()
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

function Initialize-BuildToolchain {
    # Refresh PATH from Machine and User environment variables
    $MachinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $UserPath    = [Environment]::GetEnvironmentVariable("Path", "User")
    foreach ($entry in ($MachinePath + ";" + $UserPath).Split(";", [System.StringSplitOptions]::RemoveEmptyEntries)) {
        if (($env:Path -split ";") -notcontains $entry) {
            $env:Path += ";$entry"
        }
    }

    # Check standard CMake & WinLibs / MinGW / LLVM / Visual Studio locations if not yet in PATH
    $CandidateBinDirs = @(
        "C:\Program Files\CMake\bin",
        "C:\Program Files\LLVM\bin",
        "C:\msys64\ucrt64\bin",
        "C:\msys64\mingw64\bin",
        "C:\MinGW\bin"
    )

    $WinGetPackagesDir = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages"
    if (Test-Path $WinGetPackagesDir) {
        $WinLibsBins = Get-ChildItem -Path $WinGetPackagesDir -Directory -Filter "BrechtSanders.WinLibs*" -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path $_.FullName "mingw64\bin" }
        $CMakeBins = Get-ChildItem -Path $WinGetPackagesDir -Directory -Filter "Kitware.CMake*" -ErrorAction SilentlyContinue |
            ForEach-Object { Get-ChildItem -Path $_.FullName -Recurse -Filter "cmake.exe" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty DirectoryName }
        $CandidateBinDirs += $WinLibsBins
        $CandidateBinDirs += $CMakeBins
    }

    foreach ($dir in $CandidateBinDirs) {
        if ($dir -and (Test-Path $dir) -and (($env:Path -split ";") -notcontains $dir)) {
            $env:Path = "$dir;$env:Path"
        }
    }

    if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
        throw "CMake was not found in PATH or standard installation directories."
    }
}

function Unlock-BinaryDirectory {
    param([string]$TargetDir)
    if (-not (Test-Path $TargetDir)) { return }
    Get-Process -Name "SmartCam" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    $oldFiles = @(Get-ChildItem -Path $TargetDir -Filter "*.old_*" -File -ErrorAction SilentlyContinue)
    foreach ($f in $oldFiles) {
        if ($f -and $f.FullName) {
            try { Remove-Item -Path $f.FullName -Force -ErrorAction Stop } catch {}
        }
    }
    $binFiles = @(Get-ChildItem -Path "$TargetDir\*" -File -Include "*.dll", "*.exe" -ErrorAction SilentlyContinue)
    foreach ($f in $binFiles) {
        if (-not $f -or -not $f.FullName) { continue }
        if ($Clean) {
            try {
                Remove-Item -Path $f.FullName -Force -ErrorAction Stop
            } catch {
                $BackupLeaf = "$($f.Name).old_$([guid]::NewGuid().ToString('N').Substring(0,6))"
                try { Rename-Item -Path $f.FullName -NewName $BackupLeaf -Force -ErrorAction Stop } catch {}
            }
        } else {
            try {
                $fs = [System.IO.File]::Open($f.FullName, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
                $fs.Close()
            } catch {
                $BackupLeaf = "$($f.Name).old_$([guid]::NewGuid().ToString('N').Substring(0,6))"
                try { Rename-Item -Path $f.FullName -NewName $BackupLeaf -Force -ErrorAction Stop } catch {}
            }
        }
    }
}

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

function Build-SmartCamVariant {
    param(
        [string]$VariantName,
        [string]$BuildSubDir,
        [bool]$UseAvx512,
        [string]$BuildConfig,
        [bool]$ExecuteTests
    )

    $BuildDir = Join-Path $PSScriptRoot $BuildSubDir
    if (-not (Test-Path $BuildDir)) {
        New-Item -ItemType Directory -Path $BuildDir | Out-Null
    }

    $AvxFlag = if ($UseAvx512) { "ON" } else { "OFF" }
    Write-Host "`n============================================================" -ForegroundColor Cyan
    Write-Host "[SmartCam Build] Variant: $VariantName (Config=$BuildConfig, AVX512=$AvxFlag)" -ForegroundColor Cyan
    Write-Host "                 Output : $BuildDir" -ForegroundColor Cyan
    Write-Host "============================================================" -ForegroundColor Cyan

    $CMakeArgs = @(
        "-S", $PSScriptRoot,
        "-B", $BuildDir,
        "-DCMAKE_BUILD_TYPE=$BuildConfig",
        "-DSMARTCAM_ENABLE_AVX512=$AvxFlag"
    )

    # Prefer Ninja if available and no Visual Studio generator is forced
    if ((Get-Command ninja -ErrorAction SilentlyContinue) -and -not (Get-Command cl -ErrorAction SilentlyContinue)) {
        $CMakeArgs = @("-G", "Ninja") + $CMakeArgs
    }

    & cmake @CMakeArgs | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "CMake configuration failed for variant $VariantName." }

    Unlock-BinaryDirectory -TargetDir $BuildDir

    $BuildCmdArgs = @("--build", $BuildDir, "--config", $BuildConfig, "--parallel")
    if ($Clean) {
        $BuildCmdArgs += "--clean-first"
    }
    & cmake @BuildCmdArgs | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "CMake build failed for variant $VariantName." }

    if ($ExecuteTests) {
        Write-Host "[SmartCam Build] Running C++ unit tests for $VariantName..." -ForegroundColor Yellow
        $TestExe = Join-Path $BuildDir "test_engine_cpp.exe"
        if (Test-Path $TestExe) {
            & $TestExe | Out-Host
            if ($LASTEXITCODE -ne 0) { throw "Unit tests failed for variant $VariantName." }
        } else {
            & ctest --test-dir "$BuildDir/common_engine" -C $BuildConfig --output-on-failure | Out-Host
            if ($LASTEXITCODE -ne 0) { throw "CTest failed for variant $VariantName." }
        }
    }

    return $BuildDir
}

Show-PeflLicenseAndPrompt
Initialize-BuildToolchain

if ($EnableAvx512 -and $Variant -eq "All") {
    $Variant = "AVX512"
}

$BuiltDirs = @()

if ($Variant -in @("All", "AVX2")) {
    $BuiltDirs += Build-SmartCamVariant -VariantName "Standard Baseline (AVX2 / No AVX-512)" `
        -BuildSubDir "out_avx2" -UseAvx512 $false -BuildConfig $Configuration -ExecuteTests $RunTests
}

if ($Variant -in @("All", "AVX512")) {
    $BuiltDirs += Build-SmartCamVariant -VariantName "High-Throughput SIMD (AVX-512 Enabled)" `
        -BuildSubDir "out_avx512" -UseAvx512 $true -BuildConfig $Configuration -ExecuteTests $RunTests
}

# Mirror primary build artifacts into legacy "out" directory for backward compatibility
$DefaultOutDir = Join-Path $PSScriptRoot "out"
if (-not (Test-Path $DefaultOutDir)) {
    New-Item -ItemType Directory -Path $DefaultOutDir | Out-Null
}
foreach ($dir in $BuiltDirs) {
    Get-ChildItem -Path "$dir\*" -File -Include "*.dll", "*.exe", "*.a", "*.lib" -ErrorAction SilentlyContinue |
        ForEach-Object { Copy-ItemSafeOverwrite -Source $_.FullName -Destination (Join-Path $DefaultOutDir $_.Name) }
}

# Also mirror ready-to-use DLLs into repository root "bin\" directory
$RepoRootDir = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$RepoBinDir = Join-Path $RepoRootDir "bin"
if (-not (Test-Path $RepoBinDir)) {
    New-Item -ItemType Directory -Path $RepoBinDir -Force | Out-Null
}
Unlock-BinaryDirectory -TargetDir $RepoBinDir
$Avx2OutDir = Join-Path $PSScriptRoot "out_avx2"
$Avx512OutDir = Join-Path $PSScriptRoot "out_avx512"
if (Test-Path (Join-Path $Avx2OutDir "smartcam_engine_avx2.dll")) {
    Copy-ItemSafeOverwrite -Source (Join-Path $Avx2OutDir "smartcam_engine_avx2.dll") -Destination (Join-Path $RepoBinDir "smartcam_engine_avx2.dll")
}
if (Test-Path (Join-Path $Avx512OutDir "smartcam_engine_avx512.dll")) {
    Copy-ItemSafeOverwrite -Source (Join-Path $Avx512OutDir "smartcam_engine_avx512.dll") -Destination (Join-Path $RepoBinDir "smartcam_engine_avx512.dll")
}
foreach ($commonDll in @("smartcam_engine.dll", "smartcam_win_bridge.dll", "smartcam_mf_source.dll")) {
    if (Test-Path (Join-Path $Avx2OutDir $commonDll)) {
        Copy-ItemSafeOverwrite -Source (Join-Path $Avx2OutDir $commonDll) -Destination (Join-Path $RepoBinDir $commonDll)
    } elseif (Test-Path (Join-Path $Avx512OutDir $commonDll)) {
        Copy-ItemSafeOverwrite -Source (Join-Path $Avx512OutDir $commonDll) -Destination (Join-Path $RepoBinDir $commonDll)
    }
}

Write-Host "`n[SmartCam Build] All requested variants compiled and verified successfully!" -ForegroundColor Green
foreach ($dir in $BuiltDirs) {
    Write-Host "  -> $dir" -ForegroundColor Green
    Get-ChildItem -Path "$dir\*" -File -Include "*.dll", "*.exe", "*.a", "*.lib" |
        ForEach-Object { Write-Host ("       {0,-32} ({1,8:N1} KB)" -f $_.Name, ($_.Length / 1KB)) }
}
