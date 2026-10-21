# ============================================================================
#  Project: SmartCam - AI-Powered Virtual Camera & Aspect Ratio Studio
#  File:    platforms/windows/scripts/start_web_preview.ps1
#
#  Copyright (c) 2026 Rubens Rainelli (Rubens). All rights reserved.
#
#  Licensed under the Personal, Educational & Fair-Use License v1.0 (PEFL v1.0).
#  - Free for personal, academic, educational, and non-commercial research use.
#  - Commercial use requires a separate written commercial license from the
#    copyright holder if annual revenue/funding exceeds EUR 10,000.
#  - Redistribution or public deployment must retain this header, the interactive
#    Clause 3 EULA approval modal on first startup, and the LICENSE.md file.
# ============================================================================

<#
.SYNOPSIS
    Avvia il banco di prova Web Preview Side-by-Side di SmartCam su http://localhost:8080 (100% Zero-Python).
.DESCRIPTION
    Esegue un server HTTP locale nativo basato su PowerShell/.NET (`System.Net.HttpListener` con
    fallback automatico su `System.Net.Sockets.TcpListener` se la porta o le ACL di HttpListener
    non sono disponibili), senza alcuna dipendenza da Python.
    Garantisce un contesto sicuro (`http://localhost`) per `navigator.mediaDevices.getUserMedia`
    e apre automaticamente il browser su `http://localhost:8080/index.html`.
#>

param(
    [int]$Port = 8080,
    [switch]$NoBrowser = $false,
    [switch]$NonInteractiveAcceptPefl = $false
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($ScriptDir)) { $ScriptDir = "." }
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..\..")).Path
$WebPreviewDir = Join-Path $RepoRoot "app\web_preview"

if (-not (Test-Path $WebPreviewDir)) {
    Write-Error "Cartella Web Preview non trovata: $WebPreviewDir"
    exit 1
}

# ============================================================================
# 1. PEFL v1.0 Clause 3 Foreground License Check (7-day cache)
# ============================================================================
function Invoke-PeflStartupCheck {
    if ($NonInteractiveAcceptPefl) { return }
    $CacheFile = Join-Path $ScriptDir ".pefl_accepted_start_web_preview"
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
    Write-Host "Copyright (c) 2026 Rubens Rainelli — rubens@rainelli.it"
    Write-Host "Specification: https://github.com/RubensRainelli/pefl-license-spec`n"
    Write-Host "TERMS OF USE (Clause 3 Foreground Verification):" -ForegroundColor Yellow
    Write-Host "  * FREE for Personal & Educational use."
    Write-Host "  * STRICTLY PROHIBITED for Corporate, Professional, or Business use"
    Write-Host "    without written authorization and fee payment.`n"
    Write-Host "============================================================" -ForegroundColor Red

    $Choice = Read-Host "Do you accept the terms of the PEFL v1.0 license? (y/N)"
    if ($Choice -in @("y", "yes", "s", "si")) {
        $Remember = Read-Host "Remember this approval for the next 7 days in this folder? (y/N)"
        if ($Remember -in @("y", "yes", "s", "si")) {
            try { $CurrentTime | Out-File -FilePath $CacheFile -Encoding ascii -Force } catch {}
        }
    } else {
        Write-Host "License rejected. Exiting." -ForegroundColor Red
        exit 1
    }
}

Invoke-PeflStartupCheck

$MimeTypes = @{
    ".html" = "text/html; charset=utf-8"
    ".css"  = "text/css; charset=utf-8"
    ".js"   = "application/javascript; charset=utf-8"
    ".json" = "application/json; charset=utf-8"
    ".png"  = "image/png"
    ".jpg"  = "image/jpeg"
    ".jpeg" = "image/jpeg"
    ".svg"  = "image/svg+xml"
    ".ico"  = "image/x-icon"
}

function Resolve-RequestedFile {
    param([string]$UrlPath)
    $CleanPath = ($UrlPath -split '\?')[0].TrimStart('/')
    if ([string]::IsNullOrWhiteSpace($CleanPath)) {
        $CleanPath = "index.html"
    }
    $SafeRelativePath = $CleanPath -replace '/', '\'
    $FullCandidate = [System.IO.Path]::GetFullPath((Join-Path $WebPreviewDir $SafeRelativePath))
    $RootFull = [System.IO.Path]::GetFullPath($WebPreviewDir)
    if (-not $FullCandidate.StartsWith($RootFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $null
    }
    return $FullCandidate
}

# ============================================================================
# 2. Tentativo Primario: System.Net.HttpListener (con ricerca porta libera)
# ============================================================================
$ActivePort = $Port
$Listener = $null
$UseTcpFallback = $false

for ($Try = 0; $Try -lt 10; $Try++) {
    $CandidatePort = $Port + $Try
    try {
        $TestListener = New-Object System.Net.HttpListener
        $TestListener.Prefixes.Add("http://localhost:$CandidatePort/")
        $TestListener.Start()
        $Listener = $TestListener
        $ActivePort = $CandidatePort
        break
    } catch {
        if ($null -ne $TestListener) {
            try { $TestListener.Close() } catch {}
        }
    }
}

if ($null -eq $Listener) {
    $UseTcpFallback = $true
}

if (-not $UseTcpFallback) {
    $TargetUrl = "http://localhost:$ActivePort/index.html"
    Write-Host "==========================================================================" -ForegroundColor Cyan
    Write-Host "  SmartCam — Side-by-Side Web Preview Testbench (.NET HttpListener)" -ForegroundColor Cyan
    Write-Host "  Modalità          : 100% Nativa PowerShell/.NET (Zero-Python)" -ForegroundColor Gray
    Write-Host "  Directory servita : $WebPreviewDir" -ForegroundColor Gray
    Write-Host "  URL Locale        : $TargetUrl" -ForegroundColor Green
    Write-Host "  Premi CTRL+C per terminare il server." -ForegroundColor Yellow
    Write-Host "==========================================================================" -ForegroundColor Cyan

    if (-not $NoBrowser) {
        Start-Process $TargetUrl
    }

    try {
        while ($Listener.IsListening) {
            $Context = $Listener.GetContext()
            $Request = $Context.Request
            $Response = $Context.Response

            $FilePath = Resolve-RequestedFile -UrlPath $Request.Url.LocalPath

            $Response.Headers.Add("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0")
            $Response.Headers.Add("Permissions-Policy", "camera=(self)")

            if ($null -ne $FilePath -and (Test-Path $FilePath) -and -not (Get-Item $FilePath).PSIsContainer) {
                $Ext = [System.IO.Path]::GetExtension($FilePath).ToLowerInvariant()
                $ContentType = if ($MimeTypes.ContainsKey($Ext)) { $MimeTypes[$Ext] } else { "application/octet-stream" }
                $Response.ContentType = $ContentType
                $Bytes = [System.IO.File]::ReadAllBytes($FilePath)
                $Response.ContentLength64 = $Bytes.Length
                $Response.StatusCode = 200
                $Response.OutputStream.Write($Bytes, 0, $Bytes.Length)
            } else {
                $Response.StatusCode = 404
                $Msg = [System.Text.Encoding]::UTF8.GetBytes("404 Not Found")
                $Response.ContentLength64 = $Msg.Length
                $Response.OutputStream.Write($Msg, 0, $Msg.Length)
            }
            $Response.OutputStream.Close()
        }
    } finally {
        if ($Listener.IsListening) {
            $Listener.Stop()
        }
        $Listener.Close()
        Write-Host "`n[SmartCam Preview] Server HTTP locale (.NET HttpListener) arrestato." -ForegroundColor Yellow
    }
    exit 0
}

# ============================================================================
# 3. Fallback Automatico: System.Net.Sockets.TcpListener (Zero-ACL, Zero-Python)
# ============================================================================
$TcpServer = $null
for ($Try = 0; $Try -lt 10; $Try++) {
    $CandidatePort = $Port + $Try
    try {
        $Endpoint = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Loopback, $CandidatePort)
        $CandidateTcp = New-Object System.Net.Sockets.TcpListener($Endpoint)
        $CandidateTcp.Start()
        $TcpServer = $CandidateTcp
        $ActivePort = $CandidatePort
        break
    } catch {
        if ($null -ne $CandidateTcp) {
            try { $CandidateTcp.Stop() } catch {}
        }
    }
}

if ($null -eq $TcpServer) {
    Write-Error "Impossibile avviare il server HTTP locale sulle porte $Port..$($Port + 9)."
    exit 1
}

$TargetUrl = "http://localhost:$ActivePort/index.html"
Write-Host "==========================================================================" -ForegroundColor Cyan
Write-Host "  SmartCam — Side-by-Side Web Preview Testbench (.NET TcpListener)" -ForegroundColor Cyan
Write-Host "  Modalità          : 100% Nativa PowerShell/.NET Fallback (Zero-Python)" -ForegroundColor Gray
Write-Host "  Directory servita : $WebPreviewDir" -ForegroundColor Gray
Write-Host "  URL Locale        : $TargetUrl" -ForegroundColor Green
Write-Host "  Premi CTRL+C per terminare il server." -ForegroundColor Yellow
Write-Host "==========================================================================" -ForegroundColor Cyan

if (-not $NoBrowser) {
    Start-Process $TargetUrl
}

try {
    while ($true) {
        $Client = $TcpServer.AcceptTcpClient()
        try {
            $Stream = $Client.GetStream()
            $Reader = New-Object System.IO.StreamReader($Stream, [System.Text.Encoding]::ASCII, $false, 1024, $true)
            $RequestLine = $Reader.ReadLine()
            if (-not [string]::IsNullOrEmpty($RequestLine)) {
                # Consuma gli header HTTP residui
                while ($true) {
                    $HeaderLine = $Reader.ReadLine()
                    if ([string]::IsNullOrEmpty($HeaderLine)) { break }
                }

                $Parts = $RequestLine -split ' '
                $ReqPath = if ($Parts.Length -ge 2) { $Parts[1] } else { "/index.html" }
                $FilePath = Resolve-RequestedFile -UrlPath $ReqPath

                if ($null -ne $FilePath -and (Test-Path $FilePath) -and -not (Get-Item $FilePath).PSIsContainer) {
                    $Ext = [System.IO.Path]::GetExtension($FilePath).ToLowerInvariant()
                    $ContentType = if ($MimeTypes.ContainsKey($Ext)) { $MimeTypes[$Ext] } else { "application/octet-stream" }
                    $BodyBytes = [System.IO.File]::ReadAllBytes($FilePath)
                    $HeaderText = (
                        "HTTP/1.1 200 OK`r`n" +
                        "Content-Type: $ContentType`r`n" +
                        "Content-Length: $($BodyBytes.Length)`r`n" +
                        "Cache-Control: no-store, no-cache, must-revalidate, max-age=0`r`n" +
                        "Permissions-Policy: camera=(self)`r`n" +
                        "Connection: close`r`n`r`n"
                    )
                    $HeaderBytes = [System.Text.Encoding]::ASCII.GetBytes($HeaderText)
                    $Stream.Write($HeaderBytes, 0, $HeaderBytes.Length)
                    $Stream.Write($BodyBytes, 0, $BodyBytes.Length)
                } else {
                    $BodyBytes = [System.Text.Encoding]::UTF8.GetBytes("404 Not Found")
                    $HeaderText = (
                        "HTTP/1.1 404 Not Found`r`n" +
                        "Content-Type: text/plain; charset=utf-8`r`n" +
                        "Content-Length: $($BodyBytes.Length)`r`n" +
                        "Connection: close`r`n`r`n"
                    )
                    $HeaderBytes = [System.Text.Encoding]::ASCII.GetBytes($HeaderText)
                    $Stream.Write($HeaderBytes, 0, $HeaderBytes.Length)
                    $Stream.Write($BodyBytes, 0, $BodyBytes.Length)
                }
                $Stream.Flush()
            }
        } catch {
            # Ignora disconnessioni socket lato client
        } finally {
            $Client.Close()
        }
    }
} finally {
    $TcpServer.Stop()
    Write-Host "`n[SmartCam Preview] Server HTTP locale (.NET TcpListener) arrestato." -ForegroundColor Yellow
}
