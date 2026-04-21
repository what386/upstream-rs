#!/usr/bin/env pwsh
#Requires -Version 5.1

$ErrorActionPreference = "Stop"

$GITHUB_USER = "what386"
$GITHUB_REPO = "upstream-rs"

function Install-PowerShellCompletion {
    $completionUrl = "https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/latest/download/completions.ps1"
    $completionDir = Join-Path $env:USERPROFILE ".config\powershell\Completions"
    $completionFile = Join-Path $completionDir "upstream.ps1"
    $profilePath = $PROFILE.CurrentUserAllHosts
    $profileLine = ". `"$completionFile`""

    New-Item -ItemType Directory -Path $completionDir -Force | Out-Null
    Invoke-WebRequest -Uri $completionUrl -OutFile $completionFile -UseBasicParsing

    $profileDir = Split-Path -Parent $profilePath
    if (![string]::IsNullOrEmpty($profileDir)) {
        New-Item -ItemType Directory -Path $profileDir -Force | Out-Null
    }

    if (!(Test-Path $profilePath)) {
        New-Item -ItemType File -Path $profilePath -Force | Out-Null
    }

    $profileContent = Get-Content -Path $profilePath -Raw -ErrorAction SilentlyContinue
    if ($null -eq $profileContent) {
        $profileContent = ""
    }

    if ($profileContent -notlike "*$profileLine*") {
        Add-Content -Path $profilePath -Value "`n$profileLine"
    }

    Write-Host "Installed PowerShell completion to: $completionFile"
}

Install-PowerShellCompletion
