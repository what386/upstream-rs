# Run with either Windows PowerShell 5.1 or pwsh. No downloads or elevation occur.
$ErrorActionPreference = "Stop"
$installerPath = Join-Path $PSScriptRoot "../../scripts/install/install.ps1"
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $installerPath, [ref]$tokens, [ref]$parseErrors
)
if ($parseErrors.Count -gt 0) {
    throw "Installer parse errors: $parseErrors"
}

# Load function definitions without running Main or changing the user's setup.
foreach ($statement in $ast.EndBlock.Statements) {
    if ($statement -is [System.Management.Automation.Language.FunctionDefinitionAst]) {
        . ([scriptblock]::Create($statement.Extent.Text))
    }
}

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Reset-Scenario {
    $script:AlreadyInstalled = $false
    $script:DetectedAfterInstall = $true
    $script:RuntimeChecks = @()
    $script:Downloads = @()
    $script:Processes = @()
    $script:DownloadFailure = $false
    $script:ElevationFailure = $false
    $script:SignatureStatus = "Valid"
    $script:SignerSubject = "CN=Microsoft Corporation, O=Microsoft Corporation, C=US"
    $script:RuntimeExitCode = 0
    $script:Messages = @()
}

function Write-ColorOutput {
    param([string]$Message, [string]$Color)
    $script:Messages += $Message
}

function Test-VisualCppRuntimeInstalled {
    param([string]$Architecture)
    $script:RuntimeChecks += $Architecture
    if ($script:RuntimeChecks.Count -eq 1) { return $script:AlreadyInstalled }
    return $script:DetectedAfterInstall
}

function Invoke-WebRequest {
    param([string]$Uri, [string]$OutFile, [switch]$UseBasicParsing)
    $script:Downloads += @{ Uri = $Uri; Path = $OutFile }
    if ($script:DownloadFailure) { throw "Download failed" }
    Set-Content -LiteralPath $OutFile -Value "Mock installer"
}

function Get-AuthenticodeSignature {
    param([string]$LiteralPath)
    return [pscustomobject]@{
        Status = $script:SignatureStatus
        SignerCertificate = [pscustomobject]@{ Subject = $script:SignerSubject }
    }
}

function Start-Process {
    param(
        [string]$FilePath, [string[]]$ArgumentList, [string]$Verb,
        [switch]$Wait, [switch]$PassThru, [string]$ErrorAction
    )
    $script:Processes += @{
        Path = $FilePath; Arguments = $ArgumentList; Verb = $Verb
        Wait = $Wait.IsPresent; PassThru = $PassThru.IsPresent
    }
    if ($script:ElevationFailure) { throw "Administrator approval was declined" }
    return [pscustomobject]@{ ExitCode = $script:RuntimeExitCode }
}

function Assert-TemporaryFilesRemoved {
    foreach ($download in $script:Downloads) {
        $directory = Split-Path -Parent $download.Path
        Assert-True (-not (Test-Path -LiteralPath $directory)) "Temporary runtime directory was not removed: $directory"
    }
}

Reset-Scenario
$script:AlreadyInstalled = $true
Install-VisualCppRuntimeIfMissing -Architecture "x86_64"
Assert-True ($script:Downloads.Count -eq 0) "An installed runtime should skip downloading"
Assert-True ($script:Processes.Count -eq 0) "An installed runtime should skip elevation"

foreach ($architecture in @("x86_64", "aarch64")) {
    Reset-Scenario
    Install-VisualCppRuntimeIfMissing -Architecture $architecture
    $runtimeArchitecture = if ($architecture -eq "x86_64") { "x64" } else { "arm64" }
    Assert-True ($script:Downloads.Count -eq 1) "Expected one runtime download"
    Assert-True ($script:Downloads[0].Uri -eq "https://aka.ms/vc14/vc_redist.$runtimeArchitecture.exe") "Wrong runtime download for $architecture"
    Assert-True ($script:Processes.Count -eq 1) "Expected only the runtime installer to launch"
    $process = $script:Processes[0]
    Assert-True ($process.Path -eq $script:Downloads[0].Path) "Wrong executable launched"
    Assert-True (($process.Arguments -join ' ') -eq "/install /passive /norestart") "Runtime must install without restarting Windows"
    Assert-True ($process.Verb -eq "RunAs" -and $process.Wait -and $process.PassThru) "Runtime installation must request elevation and wait for its exit code"
    Assert-True (($script:RuntimeChecks -join ',') -eq "$architecture,$architecture") "Runtime must be checked again after installation"
    Assert-TemporaryFilesRemoved
}

Reset-Scenario
$script:RuntimeExitCode = 3010
Install-VisualCppRuntimeIfMissing -Architecture "x86_64"
Assert-True ([bool]($script:Messages -match 'restart is required')) "Restart-required success should explain the restart"
Assert-TemporaryFilesRemoved

foreach ($failure in @("download", "signature", "publisher", "unsigned", "elevation", "exit-code", "missing-runtime")) {
    Reset-Scenario
    switch ($failure) {
        "download" { $script:DownloadFailure = $true }
        "signature" { $script:SignatureStatus = "HashMismatch" }
        "publisher" { $script:SignerSubject = "CN=Someone Else, O=Microsoft Corporation" }
        "unsigned" { $script:SignatureStatus = "NotSigned"; $script:SignerSubject = $null }
        "elevation" { $script:ElevationFailure = $true }
        "exit-code" { $script:RuntimeExitCode = 1603 }
        "missing-runtime" { $script:DetectedAfterInstall = $false }
    }
    $caughtError = $null
    try {
        Install-VisualCppRuntimeIfMissing -Architecture "x86_64"
    } catch {
        $caughtError = $_
    }
    Assert-True ($null -ne $caughtError) "Expected $failure to stop installation"
    Assert-True ($caughtError.ToString().Contains("https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist")) "Expected manual installation guidance after $failure"
    if ($failure -in @("download", "signature", "publisher", "unsigned")) {
        Assert-True ($script:Processes.Count -eq 0) "Must not launch an unverified installer after $failure"
    }
    Assert-TemporaryFilesRemoved
}

Write-Host "Windows installer tests passed."
