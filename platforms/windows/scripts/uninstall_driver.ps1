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
    [switch]$NonInteractiveAcceptPefl = $false
)

$ErrorActionPreference = "Stop"

$Clsid = "{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}"
$DShowCat = "{860BB310-5D01-11d0-BD3B-00A0C911CE86}"

foreach ($Hive in @("HKCU", "HKLM")) {
    $DShowInst = "${Hive}:\Software\Classes\CLSID\$DShowCat\Instance\$Clsid"
    $ClsidKey  = "${Hive}:\Software\Classes\CLSID\$Clsid"
    if (Test-Path $DShowInst) {
        Remove-Item -Path $DShowInst -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $ClsidKey) {
        Remove-Item -Path $ClsidKey -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "[SmartCam Uninstall] Removed SmartCam Virtual Camera ($Clsid) from Windows Registry." -ForegroundColor Green
