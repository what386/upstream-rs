#!/usr/bin/env pwsh
#Requires -Version 5.1

# Stop on errors
$ErrorActionPreference = "Stop"

# Colors for output
$RED = "Red"
$GREEN = "Green"
$YELLOW = "Yellow"

$GITHUB_USER = "what386"
$GITHUB_REPO = "upstream-rs"
$BINARY_NAME = "upstream"
$UPSTREAM_DIR = Join-Path $HOME ".upstream"

function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "",
        [switch]$NoNewline
    )

    $params = @{}
    if ($Color) {
        $params["ForegroundColor"] = $Color
    }
    if ($NoNewline) {
        $params["NoNewline"] = $true
    }

    Write-Host $Message @params
}

function Detect-Arch {
    # Try multiple methods to detect architecture

    # Method 1: Using RuntimeInformation (PowerShell Core)
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

    switch ($arch) {
        ([System.Runtime.InteropServices.Architecture]::X64)   { return "x86_64" }
        ([System.Runtime.InteropServices.Architecture]::Arm64) { return "aarch64" }
        ([System.Runtime.InteropServices.Architecture]::Arm)   { return "armv7" }
        ([System.Runtime.InteropServices.Architecture]::X86)   { return "i686" }
    }

    $envArch = $env:PROCESSOR_ARCHITECTURE

    switch ($envArch) {
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        "ARM"   { return "armv7" }
        "x86"   { return "i686" }
    }

    switch ($wmiArch) {
        9  { return "x86_64" }  # x64
        12 { return "aarch64" } # ARM64
        5  { return "armv7" }   # ARM
        0  { return "i686" }    # x86
    }

    return "unknown"
}

function Select-ExistingDataAction {
    if (-not (Test-Path $UPSTREAM_DIR)) {
        return "new"
    }

    if (-not (Test-Path $UPSTREAM_DIR -PathType Container)) {
        throw "'$UPSTREAM_DIR' exists but is not a directory."
    }

    if ($env:UPSTREAM_EXISTING_DATA) {
        switch ($env:UPSTREAM_EXISTING_DATA.ToLowerInvariant()) {
            "keep" { return "keep" }
            "replace" { return "replace" }
            default { throw "UPSTREAM_EXISTING_DATA must be 'keep' or 'replace'." }
        }
    }

    if ([Console]::IsInputRedirected) {
        Write-ColorOutput "Existing '$UPSTREAM_DIR' found; no interactive input available, keeping it." $YELLOW
        return "keep"
    }

    while ($true) {
        $answer = Read-Host "Existing '$UPSTREAM_DIR' found. Keep it and refresh hooks, or replace it? [K/r]"
        switch ($answer.ToLowerInvariant()) {
            "" { return "keep" }
            "k" { return "keep" }
            "keep" { return "keep" }
            "r" { return "replace" }
            "replace" { return "replace" }
            default { Write-Host "Please answer 'keep' or 'replace'." }
        }
    }
}

function Invoke-UpstreamCommand {
    param(
        [string]$Binary,
        [string[]]$Arguments
    )

    Write-ColorOutput "Running: " $YELLOW -NoNewline
    Write-Host "upstream $($Arguments -join ' ')"
    & $Binary @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed: upstream $($Arguments -join ' ')"
    }
}

function Test-UpstreamPackageInstalled {
    param(
        [string]$Binary
    )

    & $Binary @("list", "upstream", "--json") *> $null
    return $LASTEXITCODE -eq 0
}

function Install-UpstreamIfMissing {
    param(
        [string]$Binary
    )

    if (Test-UpstreamPackageInstalled -Binary $Binary) {
        Write-ColorOutput "Managed upstream package already present; skipping package install." $GREEN
    } else {
        Invoke-UpstreamCommand -Binary $Binary -Arguments @("--yes", "install", "what386/upstream-rs", "upstream", "-k", "win-exe")
    }
}

function Main {
    Write-ColorOutput "Starting installation..." $GREEN

    $ARCH = Detect-Arch

    if ($ARCH -eq "unknown") {
        Write-ColorOutput "Error: Unsupported architecture" $RED
        Write-Host "Debug info:"
        Write-Host "  PROCESSOR_ARCHITECTURE: $env:PROCESSOR_ARCHITECTURE"
        Write-Host "  PROCESSOR_ARCHITEW6432: $env:PROCESSOR_ARCHITEW6432"
        try {
            Write-Host "  RuntimeInformation: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
        } catch {
            Write-Host "  RuntimeInformation: Not available"
        }
        exit 1
    }

    Write-Host "Detected Architecture: $ARCH"

    $DOWNLOAD_URL = "https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/latest/download/${BINARY_NAME}-${ARCH}-pc-windows-msvc.exe"

    Write-Host "Downloading from: $DOWNLOAD_URL"

    # Create temporary directory
    $TMP_DIR = New-Item -ItemType Directory -Path ([System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), [System.IO.Path]::GetRandomFileName()))
    $TMP_FILE = Join-Path $TMP_DIR "${BINARY_NAME}.exe"

    try {
        # Download file
        Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile $TMP_FILE -UseBasicParsing

        Write-ColorOutput "Download complete!" $GREEN

        $existingAction = Select-ExistingDataAction
        if ($existingAction -eq "replace") {
            Write-ColorOutput "Removing existing '$UPSTREAM_DIR'..." $YELLOW
            Remove-Item -Recurse -Force $UPSTREAM_DIR
        } elseif ($existingAction -eq "keep") {
            Write-ColorOutput "Keeping existing '$UPSTREAM_DIR'." $GREEN
        }

        Invoke-UpstreamCommand -Binary $TMP_FILE -Arguments @("hooks", "init")
        Install-UpstreamIfMissing -Binary $TMP_FILE

        Write-ColorOutput "Installation complete!" $GREEN
    }
    catch {
        Write-ColorOutput "Error: $_" $RED
        exit 1
    }
    finally {
        if (Test-Path $TMP_DIR) {
            Remove-Item -Recurse -Force $TMP_DIR
        }
    }
}

# Run main function
Main
