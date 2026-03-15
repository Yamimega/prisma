# Prisma

**简体中文** | [English](./README_EN.md)

基于 Rust 构建的下一代加密代理基础设施套件。Prisma 实现了 **PrismaVeil v4** 线路协议，融合现代密码学、多种传输方式和高级抗审查特性。

## 特性亮点

- **PrismaVeil v4 协议** — 1-RTT 握手、0-RTT 恢复，X25519 + BLAKE3 + ChaCha20/AES-256-GCM/Transport-Only
- **6 种传输方式** — QUIC v2、TCP、WebSocket、gRPC、XHTTP、XPorta（CDN 兼容）
- **TUN 模式** — 通过虚拟网络接口实现系统级代理（Windows/Linux/macOS）
- **GeoIP 路由** — 基于 v2fly geoip.dat 的国家级智能分流，客户端和服务端均支持
- **PrismaTLS** — 替代 REALITY 的主动探测防御，浏览器指纹模拟 + 动态掩护服务器池
- **流量整形** — 桶填充、时序抖动、杂音注入、帧合并，抵御封装 TLS 指纹识别
- **抗审查** — Salamander UDP 混淆、HTTP/3 伪装、端口跳跃、TLS 伪装、熵伪装
- **端口转发** — 通过加密隧道实现类 frp 的反向代理
- **Web 仪表板** — 基于 Next.js + shadcn/ui 的实时监控
- **智能 DNS** — Fake IP、隧道、智能（GeoSite）和直连模式

## 快速开始

### 安装

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/scripts/install.sh | bash -s -- --setup

# Windows (PowerShell)
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Yamimega/prisma/master/scripts/install.ps1))) -Setup
```

`--setup` 参数会自动生成凭证、TLS 证书和示例配置文件。

安装脚本还支持以下选项：

| 选项 | 描述 |
|------|------|
| `--version v0.2.1` | 安装指定版本 |
| `--dir ~/.local/bin` | 自定义安装目录 |
| `--config-dir DIR` | `--setup` 的配置文件输出目录 |
| `--uninstall` | 卸载 prisma |
| `--no-verify` | 跳过 SHA256 校验和验证 |
| `--force` | 覆盖已有安装而不提示 |
| `--quiet` | 静默模式，不输出信息 |

完整选项请运行 `install.sh --help` 或 `install.ps1 -Help`。

### 运行

```bash
# 启动服务端
prisma server -c server.toml

# 启动客户端
prisma client -c client.toml

# 测试代理
curl --socks5 127.0.0.1:1080 https://httpbin.org/ip
```

### 从源码构建

```bash
git clone https://github.com/Yamimega/prisma.git && cd prisma
cargo build --release
```

## 项目结构

```
prisma/
├── prisma-core/       # 共享库：加密、协议、配置、DNS、路由、GeoIP
├── prisma-server/     # 代理服务端（TCP、QUIC、CDN 入站）
├── prisma-client/     # 代理客户端（SOCKS5、HTTP CONNECT、TUN 入站）
├── prisma-mgmt/       # 管理 API（基于 axum 的 REST + WebSocket）
├── prisma-cli/        # CLI 工具：密钥/证书生成、初始化、校验
├── prisma-dashboard/  # Web 仪表板（Next.js + shadcn/ui）
├── prisma-docs/       # 文档站点（Docusaurus）
└── scripts/           # 安装脚本和基准测试
```

## 文档

完整文档请访问 **[yamimega.github.io/prisma](https://yamimega.github.io/prisma/)**，包括：

- [快速入门](https://yamimega.github.io/prisma/docs/getting-started) — 第一个代理会话教程
- [安装指南](https://yamimega.github.io/prisma/docs/installation) — 全平台、Docker、Cargo
- [服务端配置](https://yamimega.github.io/prisma/docs/configuration/server) — 完整配置参考
- [客户端配置](https://yamimega.github.io/prisma/docs/configuration/client) — 完整配置参考
- [路由规则](https://yamimega.github.io/prisma/docs/features/routing-rules) — 客户端/服务端路由 + GeoIP
- [PrismaTLS](https://yamimega.github.io/prisma/docs/features/prisma-tls) — 主动探测防御
- [流量整形](https://yamimega.github.io/prisma/docs/features/traffic-shaping) — 抗指纹识别
- [TUN 模式](https://yamimega.github.io/prisma/docs/features/tun-mode) — 系统级代理配置
- [配置示例](https://yamimega.github.io/prisma/docs/deployment/config-examples) — 8 种场景即用模板
- [PrismaVeil 协议](https://yamimega.github.io/prisma/docs/security/prismaveil-protocol) — 线路协议规范
- [仪表板](https://yamimega.github.io/prisma/docs/features/dashboard) — Web UI 配置
- [管理 API](https://yamimega.github.io/prisma/docs/features/management-api) — REST/WebSocket API 参考

## 开发

```bash
# 运行测试
cargo test --workspace

# 代码检查
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# 构建仪表板
cd prisma-dashboard && npm ci && npm run build

# 构建文档
cd prisma-docs && npm install && npm start
```

## 许可证

GPLv3.0
