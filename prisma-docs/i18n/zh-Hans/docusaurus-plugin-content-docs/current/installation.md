---
sidebar_position: 3
---

# 安装

## 一键安装

自动检测操作系统和架构，最快的安装方式。

**Linux / macOS：**

```bash
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash
```

**Windows（PowerShell）：**

```powershell
irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1 | iex
```

### 安装 + 初始化

添加 `--setup` 参数同时生成凭证、TLS 证书和示例配置文件：

**Linux / macOS：**

```bash
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash -s -- --setup
```

**Windows（PowerShell）：**

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Yamimega/prisma/master/install.ps1))) -Setup
```

生成的文件：
- `.prisma-credentials` — 客户端 ID 和认证密钥
- `prisma-cert.pem` / `prisma-key.pem` — TLS 证书和私钥
- `server.toml` / `client.toml` — 示例配置文件（如果不存在）

### 自定义安装目录

设置 `PRISMA_INSTALL_DIR` 指定安装位置：

```bash
PRISMA_INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash
```

## 各平台手动下载

**Linux (x86_64)：**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-amd64 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**Linux (aarch64)：**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-arm64 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**Linux (ARMv7 / 树莓派)：**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-linux-armv7 -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**macOS（Apple Silicon / Intel）：**

```bash
curl -fsSL https://github.com/Yamimega/prisma/releases/latest/download/prisma-darwin-$(uname -m | sed s/x86_64/amd64/) -o /usr/local/bin/prisma && chmod +x /usr/local/bin/prisma
```

**Windows（x64，PowerShell）：**

```powershell
New-Item -Force -ItemType Directory "$env:LOCALAPPDATA\prisma" | Out-Null; Invoke-WebRequest -Uri "https://github.com/Yamimega/prisma/releases/latest/download/prisma-windows-amd64.exe" -OutFile "$env:LOCALAPPDATA\prisma\prisma.exe"; [Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$env:LOCALAPPDATA\prisma", "User")
```

**Windows（ARM64，PowerShell）：**

```powershell
New-Item -Force -ItemType Directory "$env:LOCALAPPDATA\prisma" | Out-Null; Invoke-WebRequest -Uri "https://github.com/Yamimega/prisma/releases/latest/download/prisma-windows-arm64.exe" -OutFile "$env:LOCALAPPDATA\prisma\prisma.exe"; [Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$env:LOCALAPPDATA\prisma", "User")
```

**FreeBSD (x86_64)：**

```bash
fetch -o /usr/local/bin/prisma https://github.com/Yamimega/prisma/releases/latest/download/prisma-freebsd-amd64 && chmod +x /usr/local/bin/prisma
```

## 通过 Cargo 安装

适用于任何安装了 Rust 工具链的平台：

```bash
cargo install --git https://github.com/Yamimega/prisma.git prisma-cli
```

或从本地克隆安装：

```bash
cargo install --path prisma-cli
```

## Docker

```bash
docker run --rm -v $(pwd):/config ghcr.io/yamimega/prisma server -c /config/server.toml
```

或本地构建：

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
docker build -t prisma .
docker run --rm -v $(pwd):/config prisma server -c /config/server.toml
```

## 从源码构建

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
cargo build --release
```

二进制文件将生成在 `target/release/` 目录下。将 `prisma` 二进制文件复制到 `$PATH` 中的某个位置：

```bash
sudo cp target/release/prisma /usr/local/bin/
```

## 预编译二进制文件

以下目标平台的预编译二进制文件通过 GitHub Releases 提供：

| 平台 | 架构 |
|------|------|
| Linux | x86_64, aarch64, ARMv7 |
| macOS | x86_64 (Intel), aarch64 (Apple Silicon) |
| Windows | x86_64, ARM64 |
| FreeBSD | x86_64 |

请查看 [GitHub Releases](https://github.com/Yamimega/prisma/releases) 页面获取最新构建。

## 验证安装

```bash
prisma --version
prisma --help
```

## 下一步

- [快速开始](./getting-started.md) — 运行您的第一个代理会话
- [Linux systemd 部署](./deployment/linux-systemd.md) — 部署为系统服务
