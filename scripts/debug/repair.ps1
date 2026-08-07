#!/usr/bin/env pwsh
#Requires -Version 5.1

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$Repository = "what386/upstream-rs"
$ReleaseBase = "https://github.com/$Repository/releases/latest/download"

function Get-Architecture {
    try {
        switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
            "X64" { return "x86_64" }
            "Arm64" { return "aarch64" }
        }
    } catch {}

    $architecture = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }
    switch ($architecture) {
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        default { throw "Unsupported Windows architecture: $architecture" }
    }
}

function Confirm-Checksum([string]$Artifact, [string]$Manifest, [string]$AssetName) {
    $expected = $null
    foreach ($line in Get-Content -LiteralPath $Manifest) {
        if ($line -match '^([A-Fa-f0-9]{64})\s+\*?(.+)$' -and $Matches[2] -eq $AssetName) {
            $expected = $Matches[1]
            break
        }
    }
    if (-not $expected) { throw "No checksum found for $AssetName." }
    $actual = (Get-FileHash -LiteralPath $Artifact -Algorithm SHA256).Hash
    if (-not $actual.Equals($expected, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Checksum verification failed for $AssetName."
    }
}

function Invoke-Upstream([string]$Binary, [string[]]$Arguments) {
    Write-Host "Running: upstream $($Arguments -join ' ')"
    & $Binary @Arguments
    if ($LASTEXITCODE -ne 0) { throw "upstream $($Arguments -join ' ') failed." }
}

function Test-Upstream([string]$Binary) {
    if (-not $Binary -or -not (Test-Path -LiteralPath $Binary -PathType Leaf)) { return $false }
    try {
        $packages = @(& $Binary list --json 2>$null | ConvertFrom-Json)
        if ($LASTEXITCODE -ne 0) { return $false }
        $package = $packages | Where-Object name -eq "upstream" | Select-Object -First 1
        return [bool]($package -and $package.exec_path -and (Test-Path -LiteralPath $package.exec_path -PathType Leaf))
    } catch {
        return $false
    }
}

function Complete-Repair([string]$Binary) {
    Invoke-Upstream $Binary @("hooks", "init")
    Invoke-Upstream $Binary @("doctor", "--fix")
}

$command = Get-Command upstream -CommandType Application -ErrorAction SilentlyContinue
$installedBinary = if ($command) { $command.Source } else { $null }
if (Test-Upstream $installedBinary) {
    try {
        Invoke-Upstream $installedBinary @("--yes", "reinstall", "upstream", "--force")
        Complete-Repair $installedBinary
        Write-Host "Repair complete."
        exit 0
    } catch {
        Write-Warning "In-place repair failed: $($_.Exception.Message)"
        Write-Host "Falling back to a clean bootstrap."
    }
}

$temporary = New-Item -ItemType Directory -Path ([IO.Path]::Combine([IO.Path]::GetTempPath(), [IO.Path]::GetRandomFileName()))
try {
    $asset = "upstream-$(Get-Architecture)-pc-windows-msvc.exe"
    $bootstrap = Join-Path $temporary "upstream.exe"
    $checksums = Join-Path $temporary "SHA256SUMS.txt"
    Invoke-WebRequest "$ReleaseBase/$asset" -OutFile $bootstrap -UseBasicParsing
    Invoke-WebRequest "$ReleaseBase/SHA256SUMS.txt" -OutFile $checksums -UseBasicParsing
    Confirm-Checksum $bootstrap $checksums $asset

    try {
        Invoke-Upstream $bootstrap @("--yes", "remove", "upstream", "--force")
    } catch {
        Write-Warning "Forced removal did not complete: $($_.Exception.Message)"
        Write-Host "Continuing with a fresh install attempt."
    }
    Invoke-Upstream $bootstrap @("--yes", "install", $Repository, "upstream", "-k", "win-exe")
    Complete-Repair $bootstrap
    Write-Host "Repair complete. Restart separately launched shells before testing upstream."
} finally {
    if (Test-Path $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
}
