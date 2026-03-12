---
sidebar_position: 3
---

# 安装

## 通过 Cargo 安装

安装 Prisma 最快的方式是使用 `cargo install`：

```bash
cargo install --path prisma-cli
```

这将编译并安装 `prisma` 二进制文件到您的 Cargo bin 目录（通常是 `~/.cargo/bin/`）。

## 从源码构建

```bash
git clone <repo-url> && cd prisma
cargo build --release
```

二进制文件将生成在 `target/release/` 目录下。将 `prisma` 二进制文件复制到 `$PATH` 中的某个位置：

```bash
sudo cp target/release/prisma /usr/local/bin/
```

## 预编译二进制文件

以下目标平台的预编译二进制文件将通过 CI 发布提供：

| 平台 | 架构 |
|------|------|
| Linux | x86_64, aarch64 |
| macOS | x86_64, aarch64 |
| Windows | x86_64 |

请查看 GitHub Releases 页面获取最新构建。

## 验证安装

```bash
prisma --help
```

## 下一步

- [快速开始](./getting-started.md) — 运行您的第一个代理会话
- [Linux systemd 部署](./deployment/linux-systemd.md) — 部署为系统服务
