---
sidebar_position: 1
---

# Linux systemd 部署

本指南介绍如何在 Linux 上将 Prisma 部署为 systemd 服务。

## 前置要求

- Prisma 二进制文件已构建或安装（参见[安装](../installation.md)）
- root 访问权限

## 1. 创建系统用户

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin prisma
```

## 2. 设置目录

```bash
sudo mkdir -p /etc/prisma
sudo chown prisma:prisma /etc/prisma
sudo chmod 750 /etc/prisma
```

## 3. 安装二进制文件

```bash
sudo cp target/release/prisma /usr/local/bin/prisma
sudo chmod 755 /usr/local/bin/prisma
```

## 4. 添加配置文件

将 `server.toml` 和/或 `client.toml` 复制到 `/etc/prisma/`：

```bash
sudo cp server.toml /etc/prisma/server.toml
sudo cp client.toml /etc/prisma/client.toml
sudo chown prisma:prisma /etc/prisma/*.toml
sudo chmod 640 /etc/prisma/*.toml
```

如果使用 TLS 证书，也需要复制：

```bash
sudo cp prisma-cert.pem prisma-key.pem /etc/prisma/
sudo chown prisma:prisma /etc/prisma/*.pem
sudo chmod 640 /etc/prisma/*.pem
```

更新 `server.toml` 中的路径以引用新位置：

```toml
[tls]
cert_path = "/etc/prisma/prisma-cert.pem"
key_path = "/etc/prisma/prisma-key.pem"
```

## 5. 安装 systemd 服务文件

### 服务端服务

从仓库复制服务文件：

```bash
sudo cp deploy/systemd/prisma-server.service /etc/systemd/system/
```

或创建 `/etc/systemd/system/prisma-server.service`：

```ini
[Unit]
Description=Prisma Proxy Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=prisma
Group=prisma
ExecStart=/usr/local/bin/prisma server -c /etc/prisma/server.toml
Restart=on-failure
RestartSec=5
LimitNOFILE=65535
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true
ReadOnlyPaths=/etc/prisma
WorkingDirectory=/etc/prisma
StandardOutput=journal
StandardError=journal
SyslogIdentifier=prisma-server

[Install]
WantedBy=multi-user.target
```

### 客户端服务

```bash
sudo cp deploy/systemd/prisma-client.service /etc/systemd/system/
```

或创建 `/etc/systemd/system/prisma-client.service`：

```ini
[Unit]
Description=Prisma Proxy Client
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=prisma
Group=prisma
ExecStart=/usr/local/bin/prisma client -c /etc/prisma/client.toml
Restart=on-failure
RestartSec=5
LimitNOFILE=65535
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true
ReadOnlyPaths=/etc/prisma
WorkingDirectory=/etc/prisma
StandardOutput=journal
StandardError=journal
SyslogIdentifier=prisma-client

[Install]
WantedBy=multi-user.target
```

## 6. 启用并启动服务

```bash
# 重新加载 systemd 以识别新的服务文件
sudo systemctl daemon-reload

# 设置服务开机自启
sudo systemctl enable prisma-server

# 启动服务
sudo systemctl start prisma-server

# 检查状态
sudo systemctl status prisma-server
```

对于客户端：

```bash
sudo systemctl daemon-reload
sudo systemctl enable prisma-client
sudo systemctl start prisma-client
sudo systemctl status prisma-client
```

## 7. 查看日志

```bash
# 跟踪服务端日志
sudo journalctl -u prisma-server -f

# 跟踪客户端日志
sudo journalctl -u prisma-client -f

# 查看最近的日志
sudo journalctl -u prisma-server --since "1 hour ago"
```

## 安全加固

提供的服务文件包含多个 systemd 安全指令：

| 指令 | 效果 |
|------|------|
| `ProtectSystem=strict` | 将整个文件系统挂载为只读，除了特定路径 |
| `ProtectHome=true` | 使 `/home`、`/root` 和 `/run/user` 不可访问 |
| `PrivateTmp=true` | 为服务创建私有 `/tmp` 挂载 |
| `NoNewPrivileges=true` | 防止进程获取新的权限 |
| `ReadOnlyPaths=/etc/prisma` | 确保配置文件不能被服务修改 |
| `LimitNOFILE=65535` | 提高文件描述符限制以支持高并发连接 |

## 目录布局总结

```
/usr/local/bin/prisma           # 二进制文件
/etc/prisma/server.toml         # 服务端配置
/etc/prisma/client.toml         # 客户端配置
/etc/prisma/prisma-cert.pem     # TLS 证书
/etc/prisma/prisma-key.pem      # TLS 私钥
/etc/systemd/system/prisma-server.service
/etc/systemd/system/prisma-client.service
```
