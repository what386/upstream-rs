#!/usr/bin/env pwsh
#Requires -Version 5.1

[CmdletBinding()]
param([switch]$Fix)

$ErrorActionPreference = "Stop"
$Root = Join-Path $HOME ".upstream"
$CanonicalAliases = Join-Path $Root "state\symlinks"
$LegacyAliases = Join-Path $Root "symlinks"
$PackagesRoot = Join-Path $Root "packages"

function Normalize-PathEntry([string]$Value) {
    if (-not $Value) { return "" }
    return $Value.Trim().Trim('"').TrimEnd('\').Replace('/', '\').ToLowerInvariant()
}

function Get-UserPathInfo {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Environment", $false)
    if (-not $key) { throw "Cannot open HKCU\Environment." }
    try {
        $value = $key.GetValue("Path", $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $kind = if ($null -eq $value) { "Missing" } else { $key.GetValueKind("Path").ToString() }
        [pscustomobject]@{ Value = $value; Kind = $kind }
    } finally { $key.Dispose() }
}

function Show-Diagnostics {
    $path = Get-UserPathInfo
    Write-Host "User PATH registry type: $($path.Kind)"
    $entries = if ($path.Value) { @($path.Value -split ';') } else { @() }
    foreach ($entry in $entries) {
        $normalized = Normalize-PathEntry $entry
        $label = if ($normalized -eq (Normalize-PathEntry $CanonicalAliases)) {
            "canonical"
        } elseif ($normalized.StartsWith((Normalize-PathEntry $PackagesRoot) + '\')) {
            "direct-package (legacy)"
        } elseif ($normalized -eq (Normalize-PathEntry $LegacyAliases)) {
            "legacy-alias"
        } else { "other" }
        Write-Host "PATH [$label] $entry"
    }

    Write-Host "Package database: $(Join-Path $Root 'metadata\packages.db') ($(Test-Path (Join-Path $Root 'metadata\packages.db')))"
    Write-Host "Canonical aliases: $CanonicalAliases ($(Test-Path $CanonicalAliases))"
    Write-Host "Legacy aliases: $LegacyAliases ($(Test-Path $LegacyAliases))"
    $cores = @(Get-ChildItem -LiteralPath $PackagesRoot -Recurse -Filter "upstream.exe" -File -ErrorAction SilentlyContinue)
    foreach ($core in $cores) {
        Write-Host "Managed core: $($core.FullName)"
    }
    if (-not $cores) { Write-Host "No managed upstream.exe was found beneath $PackagesRoot." }
}

function Get-Architecture {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($arch) {
        "X64" { "x86_64" }
        "Arm64" { "aarch64" }
        default { throw "Unsupported Windows architecture: $arch" }
    }
}

function Confirm-Checksum([string]$Artifact, [string]$Manifest, [string]$AssetName) {
    $expected = $null
    foreach ($line in Get-Content -LiteralPath $Manifest) {
        if ($line -match '^([A-Fa-f0-9]{64})\s+\*?(.+)$' -and $Matches[2] -eq $AssetName) {
            $expected = $Matches[1]; break
        }
    }
    if (-not $expected) { throw "No checksum found for $AssetName." }
    $actual = (Get-FileHash -LiteralPath $Artifact -Algorithm SHA256).Hash
    if (-not $actual.Equals($expected, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Checksum verification failed for $AssetName."
    }
}

function Invoke-Core([string]$Core, [string[]]$Arguments) {
    & $Core @Arguments
    if ($LASTEXITCODE -ne 0) { throw "upstream $($Arguments -join ' ') failed." }
}

function Repair-UserPath {
    $info = Get-UserPathInfo
    $entries = if ($info.Value) { @($info.Value -split ';') } else { @() }
    $canonical = Normalize-PathEntry $CanonicalAliases
    $packages = (Normalize-PathEntry $PackagesRoot) + '\'
    $legacy = Normalize-PathEntry $LegacyAliases
    $kept = @($entries | Where-Object {
        $entry = Normalize-PathEntry $_
        $entry -and $entry -ne $canonical -and $entry -ne $legacy -and -not $entry.StartsWith($packages)
    })
    $newValue = (@($CanonicalAliases) + $kept) -join ';'
    $kind = if ($info.Kind -eq "String") {
        [Microsoft.Win32.RegistryValueKind]::String
    } else {
        [Microsoft.Win32.RegistryValueKind]::ExpandString
    }
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Environment", $true)
    if (-not $key) { throw "Cannot open HKCU\Environment for writing." }
    try { $key.SetValue("Path", $newValue, $kind) } finally { $key.Dispose() }
    $env:Path = (@($CanonicalAliases) + @($env:Path -split ';' | Where-Object {
        (Normalize-PathEntry $_) -ne $canonical
    })) -join ';'
}

Show-Diagnostics
if (-not $Fix) {
    Write-Host "Diagnostic-only run complete. No files or registry values were changed. Use -Fix to repair."
    exit 0
}

$temporary = New-Item -ItemType Directory -Path ([IO.Path]::Combine([IO.Path]::GetTempPath(), [IO.Path]::GetRandomFileName()))
try {
    $asset = "upstream-$(Get-Architecture)-pc-windows-msvc.exe"
    $core = Join-Path $temporary $asset
    $checksums = Join-Path $temporary "SHA256SUMS.txt"
    Invoke-WebRequest "https://github.com/what386/upstream-rs/releases/latest/download/$asset" -OutFile $core -UseBasicParsing
    Invoke-WebRequest "https://github.com/what386/upstream-rs/releases/latest/download/SHA256SUMS.txt" -OutFile $checksums -UseBasicParsing
    Confirm-Checksum $core $checksums $asset

    $installed = & $core list --json 2>$null | ConvertFrom-Json | Where-Object name -eq "upstream" | Select-Object -First 1
    $needsInstall = -not $installed
    if ($installed) {
        $execExists = $installed.exec_path -and (Test-Path -LiteralPath $installed.exec_path -PathType Leaf)
        if ($installed.filetype -ne "WinExe" -or -not $execExists) {
            Invoke-Core $core @("--yes", "remove", "upstream", "--force")
            $needsInstall = $true
        }
    }
    if ($needsInstall) {
        Invoke-Core $core @("--yes", "install", "what386/upstream-rs", "upstream", "-k", "win-exe")
    }
    Invoke-Core $core @("hooks", "init")
    Invoke-Core $core @("doctor", "--fix")
    Repair-UserPath
    if (Test-Path $LegacyAliases -PathType Container) {
        Get-ChildItem -LiteralPath $LegacyAliases -File | Where-Object {
            $_.Name -ieq "upstream.exe" -or $_.Name -ieq "upstream"
        } | Remove-Item -Force
    }
    Write-Host "Repair complete. Restart separately launched shells before testing the alias."
} finally {
    if (Test-Path $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
}
