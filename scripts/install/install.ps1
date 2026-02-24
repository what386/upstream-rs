#!/usr/bin/env pwsh
#Requires -Version 5.1

# Stop on errors
$ErrorActionPreference = "Stop"

# Colors for output
$RED = "`e[0;31m"
$GREEN = "`e[0;32m"
$YELLOW = "`e[1;33m"
$NC = "`e[0m" # No Color

$GITHUB_USER = "what386"
$GITHUB_REPO = "upstream-rs"
$BINARY_NAME = "upstream-rs"

$INSTALL_COMMANDS = @(
    "init",
    "install upstream what386/upstream-rs -k win-exe"
)

function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = $NC
    )
    Write-Host "${Color}${Message}${NC}"
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

        # Run installation commands
        for ($i = 0; $i -lt $INSTALL_COMMANDS.Count; $i++) {
            $cmd = $INSTALL_COMMANDS[$i]
            Write-ColorOutput "Running command $($i + 1)/$($INSTALL_COMMANDS.Count): " $YELLOW -NoNewline
            Write-Host $cmd

            $cmdArgs = $cmd -split ' ', 2
            if ($cmdArgs.Count -eq 1) {
                $process = Start-Process -FilePath $TMP_FILE -ArgumentList $cmdArgs[0] -Wait -NoNewWindow -PassThru
            }
            else {
                $process = Start-Process -FilePath $TMP_FILE -ArgumentList $cmdArgs -Wait -NoNewWindow -PassThru
            }

            if ($process.ExitCode -ne 0) {
                Write-ColorOutput "Error: Command failed: $cmd" $RED
                throw "Command execution failed"
            }
        }

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
