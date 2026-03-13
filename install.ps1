# Prisma installer for Windows (PowerShell)
# Usage:
#   irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1 | iex
#   .\install.ps1 -Setup
#   $env:PRISMA_INSTALL_DIR = "C:\tools\prisma"; .\install.ps1
param(
    [switch]$Setup,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Write-Host "Usage: install.ps1 [-Setup] [-Help]"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  -Setup    Generate credentials, TLS certificate, and example configs"
    Write-Host ""
    Write-Host "Environment variables:"
    Write-Host "  PRISMA_INSTALL_DIR   Install directory (default: %LOCALAPPDATA%\prisma)"
    Write-Host "  PRISMA_CONFIG_DIR    Config output directory for -Setup (default: current dir)"
    exit 0
}

$Repo = "Yamimega/prisma"
$InstallDir = if ($env:PRISMA_INSTALL_DIR) { $env:PRISMA_INSTALL_DIR } else { "$env:LOCALAPPDATA\prisma" }
$Binary = "prisma.exe"

# Detect architecture
$Arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq [System.Runtime.InteropServices.Architecture]::Arm64) {
    "arm64"
} elseif ([Environment]::Is64BitOperatingSystem) {
    "amd64"
} else {
    Write-Host "Error: 32-bit Windows is not supported."
    exit 1
}

$DownloadUrl = "https://github.com/$Repo/releases/latest/download/prisma-windows-${Arch}.exe"

Write-Host "==> Detected platform: windows/${Arch}"
Write-Host "==> Downloading prisma from ${DownloadUrl}..."

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$OutPath = Join-Path $InstallDir $Binary
Invoke-WebRequest -Uri $DownloadUrl -OutFile $OutPath -UseBasicParsing

Write-Host "==> Installed to ${OutPath}"

# Add to user PATH if not present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "==> Added $InstallDir to user PATH (restart your terminal to take effect)"
}

# Optional setup
if ($Setup) {
    $ConfigDir = if ($env:PRISMA_CONFIG_DIR) { $env:PRISMA_CONFIG_DIR } else { Get-Location }
    $Prisma = $OutPath

    Write-Host ""
    Write-Host "==> Running initial setup in ${ConfigDir}..."

    Write-Host "    Generating client credentials..."
    & $Prisma gen-key | Out-File (Join-Path $ConfigDir ".prisma-credentials") -Encoding utf8

    Write-Host "    Generating TLS certificate..."
    & $Prisma gen-cert --output "$ConfigDir" --cn prisma-server

    # Download example configs if not present
    $ServerToml = Join-Path $ConfigDir "server.toml"
    if (-not (Test-Path $ServerToml)) {
        try {
            Invoke-WebRequest -Uri "https://raw.githubusercontent.com/$Repo/master/server.example.toml" -OutFile $ServerToml -UseBasicParsing
            Write-Host "    Created server.toml from example"
        } catch { }
    }
    $ClientToml = Join-Path $ConfigDir "client.toml"
    if (-not (Test-Path $ClientToml)) {
        try {
            Invoke-WebRequest -Uri "https://raw.githubusercontent.com/$Repo/master/client.example.toml" -OutFile $ClientToml -UseBasicParsing
            Write-Host "    Created client.toml from example"
        } catch { }
    }

    Write-Host ""
    Write-Host "Setup complete!"
    Write-Host "  Credentials: $(Join-Path $ConfigDir '.prisma-credentials')"
    Write-Host "  TLS cert:    $(Join-Path $ConfigDir 'prisma-cert.pem')"
    Write-Host "  TLS key:     $(Join-Path $ConfigDir 'prisma-key.pem')"
    Write-Host ""
    Write-Host "Next steps:"
    Write-Host "  1. Edit server.toml - paste the client ID and auth secret from .prisma-credentials"
    Write-Host "  2. Edit client.toml - set server_addr and paste the same credentials"
    Write-Host "  3. Run: prisma server -c server.toml"
    Write-Host "  4. Run: prisma client -c client.toml"
}

Write-Host ""
try { & $OutPath --version } catch { Write-Host "Run 'prisma --help' to get started." }
