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
    Root shortcut command `.\makeInstaller.ps1` to build the standalone SmartCam Windows 11 Installer.
#>

& (Join-Path $PSScriptRoot "platforms\windows\scripts\make_installer.ps1") @args
exit $LASTEXITCODE
