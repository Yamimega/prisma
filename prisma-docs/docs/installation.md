---
sidebar_position: 3
---

# Installation

## One-Line Install

The fastest way to get Prisma running. Automatically detects your OS and architecture.

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1 | iex
```

### Install + Setup

Add `--setup` to also generate credentials, TLS certificates, and example config files:

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash -s -- --setup
```

**Windows (PowerShell):**

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1))) -Setup
```

This creates:
- `.prisma-credentials` — client ID and auth secret
- `prisma-cert.pem` / `prisma-key.pem` — TLS certificate and key
- `server.toml` / `client.toml` — example config files (if not already present)

### Custom install directory

Set `PRISMA_INSTALL_DIR` to install to a different location:

```bash
PRISMA_INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash
```

```powershell
$env:PRISMA_INSTALL_DIR = "C:\tools\prisma"; irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1 | iex
```

## Platform-Specific Downloads

If you prefer to download the binary directly:

**Linux (x86_64):**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-amd64 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**Linux (aarch64):**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-arm64 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**Linux (ARMv7 / Raspberry Pi):**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-armv7 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**macOS (Apple Silicon / Intel):**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-darwin-$(uname -m | sed s/x86_64/amd64/) -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**Windows (x64, PowerShell):**

```powershell
New-Item -Force -ItemType Directory "$env:LOCALAPPDATA\prisma" | Out-Null; Invoke-WebRequest -Uri "https://github.com/Yamimega/prisma/releases/latest/download/prisma-windows-amd64.exe" -OutFile "$env:LOCALAPPDATA\prisma\prisma.exe"; [Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$env:LOCALAPPDATA\prisma", "User")
```

**Windows (ARM64, PowerShell):**

```powershell
New-Item -Force -ItemType Directory "$env:LOCALAPPDATA\prisma" | Out-Null; Invoke-WebRequest -Uri "https://github.com/Yamimega/prisma/releases/latest/download/prisma-windows-arm64.exe" -OutFile "$env:LOCALAPPDATA\prisma\prisma.exe"; [Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$env:LOCALAPPDATA\prisma", "User")
```

**FreeBSD (x86_64):**

```bash
fetch -o /usr/local/bin/prisma https://github.com/Yamimega/prisma/releases/latest/download/prisma-freebsd-amd64 && chmod +x /usr/local/bin/prisma
```

## Install via Cargo

Works on any platform with a Rust toolchain:

```bash
cargo install --git https://github.com/Yamimega/prisma.git prisma-cli
```

Or from a local clone:

```bash
cargo install --path prisma-cli
```

## Docker

```bash
docker run --rm -v $(pwd):/config ghcr.io/yamimega/prisma server -c /config/server.toml
```

Or build locally:

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
docker build -t prisma .
docker run --rm -v $(pwd):/config prisma server -c /config/server.toml
```

## Build from Source

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
cargo build --release
```

Binaries are placed in `target/release/`. Copy the `prisma` binary to a location on your `$PATH`:

```bash
sudo cp target/release/prisma /usr/local/bin/
```

## Pre-built Binaries

Pre-built binaries are available for the following targets via GitHub Releases:

| Platform | Architectures |
|----------|--------------|
| Linux | x86_64, aarch64, ARMv7 |
| macOS | x86_64 (Intel), aarch64 (Apple Silicon) |
| Windows | x86_64, ARM64 |
| FreeBSD | x86_64 |

Check the [GitHub Releases](https://github.com/Yamimega/prisma/releases) page for the latest builds.

## Verify Installation

```bash
prisma --version
prisma --help
```

## Next Steps

- [Getting Started](./getting-started.md) — run your first proxy session
- [Linux systemd deployment](./deployment/linux-systemd.md) — deploy as a system service
