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
    "install upstream what386/upstream-rs -k binary"
)

function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = $NC
    )
    Write-Host "${Color}${Message}${NC}"
}

function Detect-Arch {
    # Use PROCESSOR_ARCHITECTURE environment variable for native arch
    $arch = $env:PROCESSOR_ARCHITECTURE

    # Fallback to PROCESSOR_ARCHITEW6432 if running 32-bit process on 64-bit Windows
    if ($env:PROCESSOR_ARCHITEW6432) {
        $arch = $env:PROCESSOR_ARCHITEW6432
    }

    switch ($arch) {
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        "ARM" { return "armv7" }
        "x86" { return "i686" }
        default { return "unknown" }
    }
}

function Main {
    Write-ColorOutput "Starting installation..." $GREEN

    $ARCH = Detect-Arch

    if ($ARCH -eq "unknown") {
        Write-ColorOutput "Error: Unsupported architecture ($ARCH)" $RED
        Write-Host "PROCESSOR_ARCHITECTURE: $env:PROCESSOR_ARCHITECTURE"
        Write-Host "PROCESSOR_ARCHITEW6432: $env:PROCESSOR_ARCHITEW6432"
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
        # Cleanup
        if (Test-Path $TMP_DIR) {
            Remove-Item -Recurse -Force $TMP_DIR
        }
    }
}

# Run main function
Main
