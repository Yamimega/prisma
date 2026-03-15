---
sidebar_position: 15
---

# 性能基准测试

Prisma Proxy 包含自动化基准测试套件，将其性能与 [Xray-core](https://github.com/XTLS/Xray-core) 和 [sing-box](https://github.com/SagerNet/sing-box) 在多种传输配置下进行对比。测试结果每周发布，可在[交互式基准测试页面](/benchmarks)查看。

## 测试方法

所有基准测试在 **GitHub Actions**（`ubuntu-latest`）上运行，使用回环网络以消除网络波动。每个场景测量 **5 次**，取**中位数**作为结果。

### 测试参数

| 参数       | 值                             |
|-----------|-------------------------------|
| 负载大小    | 256 MB                        |
| 并发数     | 4 个并行流                      |
| 测量方式    | 5 次运行取中位数                  |
| 网络       | 回环（localhost）               |
| 运行环境    | ubuntu-latest（GitHub Actions）|
| 调度周期    | 每周一 04:00 UTC               |

### 采集指标

- **下载吞吐量**（Mbps）— 通过 SOCKS5 代理的单流下载速度
- **上传吞吐量**（Mbps）— 单流上传速度
- **延迟 / TTFB**（ms）— 通过代理链的首字节时间
- **握手时间**（ms）— 连接建立时间
- **并发吞吐量**（Mbps）— 4 个并行流的总吞吐量
- **CPU 使用率**（%）— 负载下的平均 CPU 使用
- **内存空闲/负载**（KB）— 空闲和负载状态下的常驻集大小
- **稳定性**（CV%）— 多次运行的变异系数

## 测试场景

### Prisma 场景（8 个）

| 场景              | 传输方式     | 加密方式              | 备注                         |
|-------------------|-------------|----------------------|------------------------------|
| Prisma QUIC       | QUIC v2     | ChaCha20-Poly1305    | 默认推荐配置                  |
| Prisma TCP+TLS    | TCP + TLS   | ChaCha20-Poly1305    | 传统 TCP 搭配 TLS 1.3        |
| Prisma（流量整形） | QUIC v2     | ChaCha20-Poly1305    | 启用流量整形                  |
| Prisma QUIC AES   | QUIC v2     | AES-256-GCM          | AES 硬件加速                  |
| Prisma T-Only     | TCP         | 仅传输层              | 最小加密开销                  |
| Prisma WS+TLS     | WebSocket   | ChaCha20-Poly1305    | 兼容 CDN 的 WebSocket         |
| Prisma（桶填充）   | QUIC v2     | ChaCha20-Poly1305    | 启用桶填充                    |

### Xray 场景（8 个）

| 场景              | 协议            | 备注                  |
|-------------------|----------------|----------------------|
| Xray VLESS+TLS    | VLESS          | 标准 TLS 配置          |
| Xray VLESS+XTLS   | VLESS + XTLS   | Vision 流式 splice     |
| Xray VMess+TLS    | VMess          | 传统 VMess 协议        |
| Xray Trojan+TLS   | Trojan         | Trojan 协议            |
| Xray SS AEAD      | Shadowsocks    | AEAD 密码套件          |
| Xray SS-2022      | Shadowsocks    | 2022 版本              |
| Xray VLESS+WS     | VLESS + WS     | WebSocket 传输         |
| Xray VLESS+gRPC   | VLESS + gRPC   | gRPC 传输              |

### sing-box 场景（8 个）

| 场景                  | 协议                          | 备注                           |
|----------------------|-------------------------------|-------------------------------|
| sing-box VLESS+TLS   | VLESS                         | 标准 TLS 配置                  |
| sing-box VMess+TLS   | VMess                         | VMess 协议                     |
| sing-box Trojan+TLS  | Trojan                        | Trojan 协议                    |
| sing-box SS AEAD     | Shadowsocks                   | chacha20-ietf-poly1305         |
| sing-box SS-2022     | Shadowsocks                   | 2022-blake3-aes-128-gcm       |
| sing-box VLESS+WS    | VLESS + WebSocket             | WebSocket 传输                 |
| sing-box Hysteria2   | Hysteria2（QUIC）              | 基于 QUIC，高性能               |
| sing-box TUIC v5     | TUIC v5（QUIC）                | 基于 QUIC 的多路复用            |

### 基线

直接连接（无代理）作为参考基准，用于量化代理开销。

## 安全评分

每个代理场景获得一个综合安全评分（0-100），基于六个维度：

| 维度               | 权重  | 描述                              |
|--------------------|------|-----------------------------------|
| 加密深度            | 25%  | 密码强度和分层                     |
| 前向安全性          | 20%  | 临时密钥交换属性                    |
| 流量分析抵抗力      | 20%  | 对大小/时序指纹识别的抵抗力          |
| 协议检测抵抗力      | 15%  | 识别协议的难度                     |
| 抗重放              | 10%  | 重放攻击防护                       |
| 认证强度            | 10%  | 认证机制的健壮性                    |

### 等级分类

| 等级 | 分数范围  | 标签   |
|------|---------|--------|
| S    | 85-100  | 加固型  |
| A    | 70-84   | 强固型  |
| B    | 55-69   | 中等型  |
| C    | 0-54    | 基础型  |

## 查看结果

- **[交互式基准测试页面](/benchmarks)** — 图表、表格和用例推荐
- **[GitHub Actions](https://github.com/Yamimega/prisma/actions/workflows/benchmark.yml)** — 原始 CI 日志和可下载工件
- **工件** — 每次 CI 运行上传每场景 JSON 文件，便于程序化分析

## 本地运行

你可以在本地运行基准测试套件：

```bash
# 确保 prisma、xray 和 sing-box 二进制文件可用
export PRISMA_BIN=./prisma
export XRAY_BIN=./xray/xray
export SINGBOX_BIN=./sing-box/sing-box

# 运行基准测试
./scripts/benchmark.sh
```

结果写入 `benchmark-results/` 目录，包括每个场景的独立 JSON 文件和 `summary.md` 报告。
