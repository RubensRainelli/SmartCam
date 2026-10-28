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
    [switch]$RunTests = $true,
    [switch]$Clean = $false
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
Push-Location $RepoRoot

try {
    if ($Clean) {
        Write-Host "[SmartCam v2.0.0] Cleaning Cargo workspace..." -ForegroundColor Cyan
        cargo clean
    }

    if ($Configuration -eq "Release") {
        Write-Host "[SmartCam v2.0.0] Building 100% Pure Rust workspace (Release)..." -ForegroundColor Cyan
        cargo build --release --workspace
    } else {
        Write-Host "[SmartCam v2.0.0] Building 100% Pure Rust workspace (Debug)..." -ForegroundColor Cyan
        cargo build --workspace
    }

    if ($RunTests) {
        Write-Host "[SmartCam v2.0.0] Running workspace unit tests..." -ForegroundColor Cyan
        cargo test --workspace
    }

    Write-Host "[SmartCam v2.0.0] Pure Rust build completed successfully!" -ForegroundColor Green
} finally {
    Pop-Location
}
