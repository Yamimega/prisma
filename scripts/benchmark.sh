#!/usr/bin/env bash
# Benchmark script: PrismaVeil v4 vs Xray-core vs sing-box
# Measures throughput, latency, concurrency, and memory via loopback SOCKS5 proxy.
set -euo pipefail

RESULTS_DIR="benchmark-results"
mkdir -p "$RESULTS_DIR"
RESULTS_DIR="$(cd "$RESULTS_DIR" && pwd)"

PRISMA_BIN="${PRISMA_BIN:-./prisma}"
XRAY_BIN="${XRAY_BIN:-./xray/xray}"
SINGBOX_BIN="${SINGBOX_BIN:-./sing-box/sing-box}"
HTTP_PORT=18888
PIDS=()

# Mode-aware parameters (set via BENCHMARK_MODE env var)
BENCHMARK_MODE="${BENCHMARK_MODE:-full}"
if [ "$BENCHMARK_MODE" = "quick" ]; then
    TEST_SIZE_MB=64
    CONCURRENCY=2
    WARMUP_RUNS=1
    MEASURE_RUNS=3
    MAX_EXTRA_RUNS=0
    QUALITY_GATE_ENABLED=0
    SUSTAINED_SECS=5
    COOLDOWN_SECS=1
    PAYLOAD_SIZES="single"   # single 64MB payload only
    echo "[BENCH] Running in QUICK mode (64MB, 1 warmup, 3 measure, no quality gate, 5 scenarios)"
else
    TEST_SIZE_MB=256
    CONCURRENCY=4
    WARMUP_RUNS=2
    MEASURE_RUNS=7
    MAX_EXTRA_RUNS=6
    QUALITY_GATE_ENABLED=1
    SUSTAINED_SECS=15
    COOLDOWN_SECS=3
    PAYLOAD_SIZES="multi"    # 1MB + 32MB + 256MB payloads
    echo "[BENCH] Running in FULL mode (3 payloads, 2+7 runs, MAD filtering, sustained resources)"
fi

# Quick mode: representative subset of scenarios
QUICK_SCENARIOS="baseline prisma-quic prisma-tcp xray-vless-xtls singbox-hysteria2"

should_run_scenario() {
    local label=$1
    if [ "$BENCHMARK_MODE" = "full" ]; then
        return 0
    fi
    echo "$QUICK_SCENARIOS" | grep -qw "$label"
}

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[BENCH]${NC} $*"; }
err() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }

cleanup() {
    log "Cleaning up..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

wait_for_port() {
    local port=$1 timeout=${2:-10}
    for _ in $(seq 1 "$timeout"); do
        if ss -tln 2>/dev/null | grep -q ":${port} " ||
           netstat -tln 2>/dev/null | grep -q ":${port} "; then
            return 0
        fi
        sleep 1
    done
    err "Port $port not ready after ${timeout}s"
    return 1
}

get_rss_kb() {
    local pid=$1
    if [ -f "/proc/$pid/status" ]; then
        grep VmRSS "/proc/$pid/status" 2>/dev/null | awk '{print $2}' || echo "0"
    else
        echo "0"
    fi
}

# Total CPU ticks (utime + stime) for a process from /proc/PID/stat.
# Returns 0 on non-Linux or if the process doesn't exist.
get_cpu_ticks() {
    local pid=$1
    if [ -f "/proc/$pid/stat" ]; then
        awk '{print $14 + $15}' "/proc/$pid/stat" 2>/dev/null || echo "0"
    else
        echo "0"
    fi
}

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

generate_certs() {
    log "Generating test certificates..."
    if ! "$PRISMA_BIN" gen-cert --output "$RESULTS_DIR" --cn "benchmark.local" 2>/dev/null; then
        openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
            -keyout "$RESULTS_DIR/prisma-key.pem" -out "$RESULTS_DIR/prisma-cert.pem" \
            -days 1 -nodes -subj "/CN=benchmark.local" 2>/dev/null
    fi
}

generate_configs() {
    log "Generating test configurations..."

    local CLIENT_ID AUTH_SECRET XRAY_UUID XRAY_SS_PASS XRAY_SS2022_KEY
    CLIENT_ID=$(uuidgen 2>/dev/null || python3 -c "import uuid; print(uuid.uuid4())")
    AUTH_SECRET=$(openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | xxd -p -c 64)
    XRAY_UUID=$(uuidgen 2>/dev/null || python3 -c "import uuid; print(uuid.uuid4())")
    XRAY_SS_PASS=$(openssl rand -hex 16 2>/dev/null || head -c 16 /dev/urandom | xxd -p -c 32)
    # SS-2022 requires exactly 16 bytes base64 for aes-128-gcm
    XRAY_SS2022_KEY=$(openssl rand -base64 16 2>/dev/null || head -c 16 /dev/urandom | base64)

    local SINGBOX_UUID SINGBOX_SS_PASS SINGBOX_SS2022_KEY SINGBOX_HYSTERIA_PASS
    SINGBOX_UUID=$(uuidgen 2>/dev/null || python3 -c "import uuid; print(uuid.uuid4())")
    SINGBOX_SS_PASS=$(openssl rand -hex 16 2>/dev/null || head -c 16 /dev/urandom | xxd -p -c 32)
    SINGBOX_SS2022_KEY=$(openssl rand -base64 16 2>/dev/null || head -c 16 /dev/urandom | base64)
    SINGBOX_HYSTERIA_PASS=$(openssl rand -hex 16 2>/dev/null || head -c 16 /dev/urandom | xxd -p -c 32)
    # SS-2022 relay requires 32-byte base64 key (aes-256-gcm) — server:user format
    SINGBOX_SS2022_RELAY_SERVER_KEY=$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)
    SINGBOX_SS2022_RELAY_USER_KEY=$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)

    # --- Prisma QUIC ---------------------------------------------------
    cat > "$RESULTS_DIR/server-quic.toml" <<EOF
listen_addr = "127.0.0.1:18443"
quic_listen_addr = "127.0.0.1:18443"
protocol_version = "v4"

[tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[[authorized_clients]]
id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
name = "bench-client"

[traffic_shaping]
padding_mode = "none"
EOF

    cat > "$RESULTS_DIR/client-quic.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11080"
server_addr = "127.0.0.1:18443"
transport = "quic"
skip_cert_verify = true
protocol_version = "v4"
quic_version = "v1"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Prisma TCP + TLS -------------------------------------------------
    cat > "$RESULTS_DIR/server-tcp.toml" <<EOF
listen_addr = "127.0.0.1:18445"
protocol_version = "v4"

[tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[[authorized_clients]]
id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
name = "bench-client"

[camouflage]
enabled = true
tls_on_tcp = true

[traffic_shaping]
padding_mode = "none"
EOF

    cat > "$RESULTS_DIR/client-tcp.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11082"
server_addr = "127.0.0.1:18445"
transport = "tcp"
tls_on_tcp = true
tls_server_name = "benchmark.local"
skip_cert_verify = true
protocol_version = "v4"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Prisma QUIC + traffic shaping ---------------------------------
    cat > "$RESULTS_DIR/server-shaped.toml" <<EOF
listen_addr = "127.0.0.1:18444"
quic_listen_addr = "127.0.0.1:18444"
protocol_version = "v4"

[tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[[authorized_clients]]
id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
name = "bench-client"

[traffic_shaping]
padding_mode = "random"
EOF

    cat > "$RESULTS_DIR/client-shaped.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11081"
server_addr = "127.0.0.1:18444"
transport = "quic"
skip_cert_verify = true
protocol_version = "v4"
quic_version = "v1"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Prisma QUIC + AES-256-GCM ------------------------------------
    cat > "$RESULTS_DIR/server-quic-aes.toml" <<EOF
listen_addr = "127.0.0.1:18446"
quic_listen_addr = "127.0.0.1:18446"
protocol_version = "v4"

[tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[[authorized_clients]]
id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
name = "bench-client"

[traffic_shaping]
padding_mode = "none"
EOF

    cat > "$RESULTS_DIR/client-quic-aes.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11083"
server_addr = "127.0.0.1:18446"
transport = "quic"
cipher_suite = "aes-256-gcm"
skip_cert_verify = true
protocol_version = "v4"
quic_version = "v1"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Prisma TCP+TLS + Transport-Only cipher -----------------------
    cat > "$RESULTS_DIR/server-transport-only.toml" <<EOF
listen_addr = "127.0.0.1:18447"
protocol_version = "v4"
allow_transport_only_cipher = true

[tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[[authorized_clients]]
id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
name = "bench-client"

[camouflage]
enabled = true
tls_on_tcp = true

[traffic_shaping]
padding_mode = "none"
EOF

    cat > "$RESULTS_DIR/client-transport-only.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11084"
server_addr = "127.0.0.1:18447"
transport = "tcp"
tls_on_tcp = true
tls_server_name = "benchmark.local"
skip_cert_verify = true
protocol_version = "v4"
transport_only_cipher = true

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Prisma WebSocket + TLS (CDN-compatible) ----------------------
    cat > "$RESULTS_DIR/server-ws.toml" <<EOF
listen_addr = "127.0.0.1:18449"
protocol_version = "v4"

[tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[[authorized_clients]]
id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
name = "bench-client"

[cdn]
enabled = true
listen_addr = "127.0.0.1:18460"
ws_tunnel_path = "/ws-tunnel"

[cdn.tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[traffic_shaping]
padding_mode = "none"
EOF

    cat > "$RESULTS_DIR/client-ws.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11086"
server_addr = "127.0.0.1:18460"
transport = "ws"
ws_url = "wss://127.0.0.1:18460/ws-tunnel"
tls_server_name = "benchmark.local"
skip_cert_verify = true
protocol_version = "v4"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Prisma TCP+TLS + bucket padding ------------------------------
    cat > "$RESULTS_DIR/server-bucket.toml" <<EOF
listen_addr = "127.0.0.1:18450"
protocol_version = "v4"

[tls]
cert_path = "$RESULTS_DIR/prisma-cert.pem"
key_path = "$RESULTS_DIR/prisma-key.pem"

[[authorized_clients]]
id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
name = "bench-client"

[camouflage]
enabled = true
tls_on_tcp = true

[traffic_shaping]
padding_mode = "bucket"
bucket_sizes = [128, 256, 512, 1024, 2048, 4096, 8192, 16384]
EOF

    cat > "$RESULTS_DIR/client-bucket.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11087"
server_addr = "127.0.0.1:18450"
transport = "tcp"
tls_on_tcp = true
tls_server_name = "benchmark.local"
skip_cert_verify = true
protocol_version = "v4"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # ===================================================================
    # Xray-core configurations
    # ===================================================================

    # --- Xray VLESS + TLS (TCP) ----------------------------------------
    cat > "$RESULTS_DIR/xray-vless-tls-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28443,
    "protocol": "vless",
    "settings": {
      "clients": [{"id": "$XRAY_UUID"}],
      "decryption": "none"
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {
        "certificates": [{
          "certificateFile": "$RESULTS_DIR/prisma-cert.pem",
          "keyFile": "$RESULTS_DIR/prisma-key.pem"
        }]
      }
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-vless-tls-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21080,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "vless",
    "settings": {
      "vnext": [{
        "address": "127.0.0.1",
        "port": 28443,
        "users": [{"id": "$XRAY_UUID", "encryption": "none"}]
      }]
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {"allowInsecure": true}
    }
  }]
}
XEOF

    # --- Xray VLESS + XTLS-Vision (fastest Xray mode) -----------------
    cat > "$RESULTS_DIR/xray-vless-xtls-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28444,
    "protocol": "vless",
    "settings": {
      "clients": [{"id": "$XRAY_UUID", "flow": "xtls-rprx-vision"}],
      "decryption": "none"
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {
        "certificates": [{
          "certificateFile": "$RESULTS_DIR/prisma-cert.pem",
          "keyFile": "$RESULTS_DIR/prisma-key.pem"
        }]
      }
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-vless-xtls-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21081,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "vless",
    "settings": {
      "vnext": [{
        "address": "127.0.0.1",
        "port": 28444,
        "users": [{"id": "$XRAY_UUID", "encryption": "none", "flow": "xtls-rprx-vision"}]
      }]
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {"allowInsecure": true}
    }
  }]
}
XEOF

    # --- Xray VMess + TLS (TCP) ----------------------------------------
    cat > "$RESULTS_DIR/xray-vmess-tls-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28445,
    "protocol": "vmess",
    "settings": {
      "clients": [{"id": "$XRAY_UUID", "alterId": 0}]
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {
        "certificates": [{
          "certificateFile": "$RESULTS_DIR/prisma-cert.pem",
          "keyFile": "$RESULTS_DIR/prisma-key.pem"
        }]
      }
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-vmess-tls-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21082,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "vmess",
    "settings": {
      "vnext": [{
        "address": "127.0.0.1",
        "port": 28445,
        "users": [{"id": "$XRAY_UUID", "alterId": 0, "security": "auto"}]
      }]
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {"allowInsecure": true}
    }
  }]
}
XEOF

    # --- Xray Trojan + TLS ---------------------------------------------
    cat > "$RESULTS_DIR/xray-trojan-tls-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28446,
    "protocol": "trojan",
    "settings": {
      "clients": [{"password": "$XRAY_SS_PASS"}]
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {
        "certificates": [{
          "certificateFile": "$RESULTS_DIR/prisma-cert.pem",
          "keyFile": "$RESULTS_DIR/prisma-key.pem"
        }]
      }
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-trojan-tls-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21083,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "trojan",
    "settings": {
      "servers": [{
        "address": "127.0.0.1",
        "port": 28446,
        "password": "$XRAY_SS_PASS"
      }]
    },
    "streamSettings": {
      "network": "tcp",
      "security": "tls",
      "tlsSettings": {"allowInsecure": true}
    }
  }]
}
XEOF

    # --- Xray Shadowsocks AEAD (chacha20-ietf-poly1305, no TLS) ---------
    cat > "$RESULTS_DIR/xray-ss-aead-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28447,
    "protocol": "shadowsocks",
    "settings": {
      "method": "chacha20-ietf-poly1305",
      "password": "$XRAY_SS_PASS",
      "network": "tcp"
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-ss-aead-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21084,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "shadowsocks",
    "settings": {
      "servers": [{
        "address": "127.0.0.1",
        "port": 28447,
        "method": "chacha20-ietf-poly1305",
        "password": "$XRAY_SS_PASS"
      }]
    }
  }]
}
XEOF

    # --- Xray Shadowsocks-2022 (blake3-aes-128-gcm, no TLS) -----------
    cat > "$RESULTS_DIR/xray-ss2022-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28450,
    "protocol": "shadowsocks",
    "settings": {
      "method": "2022-blake3-aes-128-gcm",
      "password": "$XRAY_SS2022_KEY",
      "network": "tcp"
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-ss2022-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21087,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "shadowsocks",
    "settings": {
      "servers": [{
        "address": "127.0.0.1",
        "port": 28450,
        "method": "2022-blake3-aes-128-gcm",
        "password": "$XRAY_SS2022_KEY"
      }]
    }
  }]
}
XEOF

    # --- Xray VLESS + WebSocket + TLS (CDN-compatible) -----------------
    cat > "$RESULTS_DIR/xray-vless-ws-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28448,
    "protocol": "vless",
    "settings": {
      "clients": [{"id": "$XRAY_UUID"}],
      "decryption": "none"
    },
    "streamSettings": {
      "network": "ws",
      "security": "tls",
      "wsSettings": {"path": "/ws-tunnel"},
      "tlsSettings": {
        "certificates": [{
          "certificateFile": "$RESULTS_DIR/prisma-cert.pem",
          "keyFile": "$RESULTS_DIR/prisma-key.pem"
        }]
      }
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-vless-ws-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21085,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "vless",
    "settings": {
      "vnext": [{
        "address": "127.0.0.1",
        "port": 28448,
        "users": [{"id": "$XRAY_UUID", "encryption": "none"}]
      }]
    },
    "streamSettings": {
      "network": "ws",
      "security": "tls",
      "wsSettings": {"path": "/ws-tunnel"},
      "tlsSettings": {"allowInsecure": true}
    }
  }]
}
XEOF

    # --- Xray VLESS + gRPC + TLS ---------------------------------------
    cat > "$RESULTS_DIR/xray-vless-grpc-server.json" <<XEOF
{
  "inbounds": [{
    "port": 28449,
    "protocol": "vless",
    "settings": {
      "clients": [{"id": "$XRAY_UUID"}],
      "decryption": "none"
    },
    "streamSettings": {
      "network": "grpc",
      "security": "tls",
      "grpcSettings": {"serviceName": "tunnel"},
      "tlsSettings": {
        "certificates": [{
          "certificateFile": "$RESULTS_DIR/prisma-cert.pem",
          "keyFile": "$RESULTS_DIR/prisma-key.pem"
        }]
      }
    }
  }],
  "outbounds": [{"protocol": "freedom"}]
}
XEOF

    cat > "$RESULTS_DIR/xray-vless-grpc-client.json" <<XEOF
{
  "inbounds": [{
    "port": 21086,
    "protocol": "socks",
    "settings": {"auth": "noauth"}
  }],
  "outbounds": [{
    "protocol": "vless",
    "settings": {
      "vnext": [{
        "address": "127.0.0.1",
        "port": 28449,
        "users": [{"id": "$XRAY_UUID", "encryption": "none"}]
      }]
    },
    "streamSettings": {
      "network": "grpc",
      "security": "tls",
      "grpcSettings": {"serviceName": "tunnel"},
      "tlsSettings": {"allowInsecure": true}
    }
  }]
}
XEOF
    # ===================================================================
    # sing-box configurations
    # ===================================================================

    # --- sing-box VLESS + TLS (TCP) ------------------------------------
    cat > "$RESULTS_DIR/singbox-vless-tls-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "vless",
    "listen": "127.0.0.1",
    "listen_port": 38443,
    "users": [{"uuid": "$SINGBOX_UUID"}],
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-vless-tls-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31080
  }],
  "outbounds": [{
    "type": "vless",
    "server": "127.0.0.1",
    "server_port": 38443,
    "uuid": "$SINGBOX_UUID",
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box VMess + TLS (TCP) ------------------------------------
    cat > "$RESULTS_DIR/singbox-vmess-tls-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "vmess",
    "listen": "127.0.0.1",
    "listen_port": 38444,
    "users": [{"uuid": "$SINGBOX_UUID", "alterId": 0}],
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-vmess-tls-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31081
  }],
  "outbounds": [{
    "type": "vmess",
    "server": "127.0.0.1",
    "server_port": 38444,
    "uuid": "$SINGBOX_UUID",
    "security": "auto",
    "alter_id": 0,
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box Trojan + TLS -----------------------------------------
    cat > "$RESULTS_DIR/singbox-trojan-tls-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "trojan",
    "listen": "127.0.0.1",
    "listen_port": 38445,
    "users": [{"password": "$SINGBOX_SS_PASS"}],
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-trojan-tls-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31082
  }],
  "outbounds": [{
    "type": "trojan",
    "server": "127.0.0.1",
    "server_port": 38445,
    "password": "$SINGBOX_SS_PASS",
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box Shadowsocks AEAD (chacha20-ietf-poly1305) ------------
    cat > "$RESULTS_DIR/singbox-ss-aead-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "shadowsocks",
    "listen": "127.0.0.1",
    "listen_port": 38446,
    "method": "chacha20-ietf-poly1305",
    "password": "$SINGBOX_SS_PASS"
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-ss-aead-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31083
  }],
  "outbounds": [{
    "type": "shadowsocks",
    "server": "127.0.0.1",
    "server_port": 38446,
    "method": "chacha20-ietf-poly1305",
    "password": "$SINGBOX_SS_PASS"
  }]
}
SBEOF

    # --- sing-box Shadowsocks-2022 (blake3-aes-128-gcm) ----------------
    cat > "$RESULTS_DIR/singbox-ss2022-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "shadowsocks",
    "listen": "127.0.0.1",
    "listen_port": 38447,
    "method": "2022-blake3-aes-128-gcm",
    "password": "$SINGBOX_SS2022_KEY"
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-ss2022-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31084
  }],
  "outbounds": [{
    "type": "shadowsocks",
    "server": "127.0.0.1",
    "server_port": 38447,
    "method": "2022-blake3-aes-128-gcm",
    "password": "$SINGBOX_SS2022_KEY"
  }]
}
SBEOF

    # --- sing-box VLESS + WebSocket + TLS ------------------------------
    cat > "$RESULTS_DIR/singbox-vless-ws-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "vless",
    "listen": "127.0.0.1",
    "listen_port": 38448,
    "users": [{"uuid": "$SINGBOX_UUID"}],
    "transport": {
      "type": "ws",
      "path": "/ws-tunnel"
    },
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-vless-ws-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31085
  }],
  "outbounds": [{
    "type": "vless",
    "server": "127.0.0.1",
    "server_port": 38448,
    "uuid": "$SINGBOX_UUID",
    "transport": {
      "type": "ws",
      "path": "/ws-tunnel"
    },
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box Hysteria2 (QUIC-based) -------------------------------
    cat > "$RESULTS_DIR/singbox-hysteria2-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "hysteria2",
    "listen": "127.0.0.1",
    "listen_port": 38449,
    "up_mbps": 10000,
    "down_mbps": 10000,
    "users": [{"password": "$SINGBOX_HYSTERIA_PASS"}],
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-hysteria2-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31086
  }],
  "outbounds": [{
    "type": "hysteria2",
    "server": "127.0.0.1",
    "server_port": 38449,
    "up_mbps": 10000,
    "down_mbps": 10000,
    "password": "$SINGBOX_HYSTERIA_PASS",
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box TUIC v5 (QUIC-based) ---------------------------------
    cat > "$RESULTS_DIR/singbox-tuic-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "tuic",
    "listen": "127.0.0.1",
    "listen_port": 38450,
    "users": [{"uuid": "$SINGBOX_UUID", "password": "$SINGBOX_HYSTERIA_PASS"}],
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-tuic-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31087
  }],
  "outbounds": [{
    "type": "tuic",
    "server": "127.0.0.1",
    "server_port": 38450,
    "uuid": "$SINGBOX_UUID",
    "password": "$SINGBOX_HYSTERIA_PASS",
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box VMess + WS + TLS ------------------------------------
    cat > "$RESULTS_DIR/singbox-vmess-ws-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "vmess",
    "listen": "127.0.0.1",
    "listen_port": 38451,
    "users": [{"uuid": "$SINGBOX_UUID", "alterId": 0}],
    "transport": {
      "type": "ws",
      "path": "/ws-tunnel"
    },
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-vmess-ws-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31088
  }],
  "outbounds": [{
    "type": "vmess",
    "server": "127.0.0.1",
    "server_port": 38451,
    "uuid": "$SINGBOX_UUID",
    "security": "auto",
    "alter_id": 0,
    "transport": {
      "type": "ws",
      "path": "/ws-tunnel"
    },
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box Trojan + WS + TLS -----------------------------------
    cat > "$RESULTS_DIR/singbox-trojan-ws-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "trojan",
    "listen": "127.0.0.1",
    "listen_port": 38452,
    "users": [{"password": "$SINGBOX_SS_PASS"}],
    "transport": {
      "type": "ws",
      "path": "/ws-tunnel"
    },
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-trojan-ws-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31089
  }],
  "outbounds": [{
    "type": "trojan",
    "server": "127.0.0.1",
    "server_port": 38452,
    "password": "$SINGBOX_SS_PASS",
    "transport": {
      "type": "ws",
      "path": "/ws-tunnel"
    },
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box Shadowsocks-2022 Relay (aes-256-gcm) ----------------
    cat > "$RESULTS_DIR/singbox-ss2022-relay-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "shadowsocks",
    "listen": "127.0.0.1",
    "listen_port": 38453,
    "method": "2022-blake3-aes-256-gcm",
    "password": "$SINGBOX_SS2022_RELAY_SERVER_KEY",
    "users": [{"password": "$SINGBOX_SS2022_RELAY_USER_KEY"}]
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-ss2022-relay-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31090
  }],
  "outbounds": [{
    "type": "shadowsocks",
    "server": "127.0.0.1",
    "server_port": 38453,
    "method": "2022-blake3-aes-256-gcm",
    "password": "${SINGBOX_SS2022_RELAY_SERVER_KEY}:${SINGBOX_SS2022_RELAY_USER_KEY}"
  }]
}
SBEOF

    # --- sing-box VLESS + gRPC + TLS ----------------------------------
    cat > "$RESULTS_DIR/singbox-vless-grpc-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "vless",
    "listen": "127.0.0.1",
    "listen_port": 38454,
    "users": [{"uuid": "$SINGBOX_UUID"}],
    "transport": {
      "type": "grpc",
      "service_name": "tunnel"
    },
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-vless-grpc-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31091
  }],
  "outbounds": [{
    "type": "vless",
    "server": "127.0.0.1",
    "server_port": 38454,
    "uuid": "$SINGBOX_UUID",
    "transport": {
      "type": "grpc",
      "service_name": "tunnel"
    },
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF

    # --- sing-box VLESS + HTTP/2 + TLS --------------------------------
    cat > "$RESULTS_DIR/singbox-vless-http2-server.json" <<SBEOF
{
  "inbounds": [{
    "type": "vless",
    "listen": "127.0.0.1",
    "listen_port": 38455,
    "users": [{"uuid": "$SINGBOX_UUID"}],
    "transport": {
      "type": "http",
      "host": ["benchmark.local"],
      "path": "/h2-tunnel"
    },
    "tls": {
      "enabled": true,
      "certificate_path": "$RESULTS_DIR/prisma-cert.pem",
      "key_path": "$RESULTS_DIR/prisma-key.pem"
    }
  }],
  "outbounds": [{"type": "direct"}]
}
SBEOF

    cat > "$RESULTS_DIR/singbox-vless-http2-client.json" <<SBEOF
{
  "inbounds": [{
    "type": "socks",
    "listen": "127.0.0.1",
    "listen_port": 31092
  }],
  "outbounds": [{
    "type": "vless",
    "server": "127.0.0.1",
    "server_port": 38455,
    "uuid": "$SINGBOX_UUID",
    "transport": {
      "type": "http",
      "host": ["benchmark.local"],
      "path": "/h2-tunnel"
    },
    "tls": {
      "enabled": true,
      "insecure": true
    }
  }]
}
SBEOF
}

start_test_server() {
    log "Creating test payloads..."

    # Always create the primary testdata file
    dd if=/dev/urandom of="$RESULTS_DIR/testdata" bs=1M count=$TEST_SIZE_MB 2>/dev/null \
        || dd if=/dev/urandom of="$RESULTS_DIR/testdata" bs=1048576 count=$TEST_SIZE_MB 2>/dev/null \
        || true

    if [ ! -s "$RESULTS_DIR/testdata" ]; then
        err "Failed to create test payload (dd failed). Download tests will report 0."
    else
        local actual_mb
        actual_mb=$(( $(stat -c%s "$RESULTS_DIR/testdata" 2>/dev/null || stat -f%z "$RESULTS_DIR/testdata" 2>/dev/null || echo 0) / 1048576 ))
        log "  Primary payload: ${actual_mb}MB"
    fi

    # Full mode: create additional payload sizes for mixed-size testing
    if [ "$PAYLOAD_SIZES" = "multi" ]; then
        dd if=/dev/urandom of="$RESULTS_DIR/testdata-small" bs=1M count=1 2>/dev/null \
            || dd if=/dev/urandom of="$RESULTS_DIR/testdata-small" bs=1048576 count=1 2>/dev/null \
            || true
        dd if=/dev/urandom of="$RESULTS_DIR/testdata-medium" bs=1M count=32 2>/dev/null \
            || dd if=/dev/urandom of="$RESULTS_DIR/testdata-medium" bs=1048576 count=32 2>/dev/null \
            || true
        # Large payload reuses testdata (256MB)
        ln -sf "$RESULTS_DIR/testdata" "$RESULTS_DIR/testdata-large" 2>/dev/null \
            || cp "$RESULTS_DIR/testdata" "$RESULTS_DIR/testdata-large" 2>/dev/null || true
        log "  Mixed payloads: 1MB (small), 32MB (medium), ${TEST_SIZE_MB}MB (large)"
    fi

    # 1-byte file for latency measurement (minimize transfer time)
    echo -n "x" > "$RESULTS_DIR/ping"

    log "Starting threaded HTTP server on port $HTTP_PORT..."
    python3 -c "
from http.server import SimpleHTTPRequestHandler, HTTPServer
from socketserver import ThreadingMixIn

class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *a, **kw):
        super().__init__(*a, directory='$RESULTS_DIR', **kw)
    def do_POST(self):
        length = int(self.headers.get('Content-Length', 0))
        while length > 0:
            chunk = self.rfile.read(min(length, 65536))
            if not chunk:
                break
            length -= len(chunk)
        self.send_response(200)
        self.send_header('Content-Length', '2')
        self.end_headers()
        self.wfile.write(b'OK')
    def log_message(self, *a):
        pass

class ThreadedHTTPServer(ThreadingMixIn, HTTPServer):
    daemon_threads = True

ThreadedHTTPServer(('', $HTTP_PORT), Handler).serve_forever()
" &
    PIDS+=($!)
    wait_for_port $HTTP_PORT
}

# ---------------------------------------------------------------------------
# Statistical pipeline
# ---------------------------------------------------------------------------

# MAD-based outlier rejection + quality assessment.
# Input:  space-separated raw numeric values
# Output: median cv_pct n_used n_total stable_flag
robust_filter() {
    local raw_values="$1"
    python3 -c "
import math

values = [float(x) for x in '''$raw_values'''.split() if x.strip()]
if not values:
    print('0 0 0 0 0')
    raise SystemExit

n_total = len(values)
values.sort()
median = values[len(values) // 2]

# Compute MAD (Median Absolute Deviation)
abs_devs = sorted(abs(x - median) for x in values)
mad = abs_devs[len(abs_devs) // 2]

if mad > 0:
    # Modified Z-scores (Iglewicz & Hoaglin)
    clean = [x for x in values if abs(0.6745 * (x - median) / mad) <= 3.5]
else:
    # MAD=0 means all values are identical or nearly so — keep all
    clean = list(values)

if not clean:
    clean = list(values)

clean.sort()
n_used = len(clean)
med = clean[len(clean) // 2]
mean = sum(clean) / n_used
if mean > 0:
    sd = math.sqrt(sum((x - mean) ** 2 for x in clean) / n_used)
    cv = sd / mean * 100
else:
    cv = 0.0

stable = 1 if cv < 15 else 0
print(f'{med:.4f} {cv:.1f} {n_used} {n_total} {stable}')
" 2>/dev/null || echo "0 0 0 0 0"
}

# Warmup + collect + filter + quality gate loop.
# Usage: run_pipeline "<command_that_outputs_a_number>"
# Output: median cv_pct n_used n_total stable_flag
run_pipeline() {
    local cmd=$1

    # Stage 1: Warmup (discard results)
    for _ in $(seq 1 "$WARMUP_RUNS"); do
        eval "$cmd" > /dev/null 2>&1 || true
    done

    # Stage 2: Collect measurements
    local values=()
    for _ in $(seq 1 "$MEASURE_RUNS"); do
        local v
        v=$(eval "$cmd" 2>/dev/null) || true
        values+=("${v:-0}")
    done

    # Stage 3+4: Filter + Quality Gate
    local result
    result=$(robust_filter "${values[*]}")
    local stable
    stable=$(echo "$result" | awk '{print $5}')

    # Quality gate: re-run if too many rejected or unstable
    if [ "$QUALITY_GATE_ENABLED" = "1" ]; then
        local extra_done=0
        local total_collected=$MEASURE_RUNS
        local max_total=$((MEASURE_RUNS + MAX_EXTRA_RUNS))
        while [ "$stable" = "0" ] && [ "$extra_done" -lt "$MAX_EXTRA_RUNS" ]; do
            for _ in 1 2 3; do
                local v
                v=$(eval "$cmd" 2>/dev/null) || true
                values+=("${v:-0}")
            done
            extra_done=$((extra_done + 3))
            total_collected=$((total_collected + 3))
            result=$(robust_filter "${values[*]}")
            stable=$(echo "$result" | awk '{print $5}')
            [ "$total_collected" -ge "$max_total" ] && break
        done
    fi

    echo "$result"
}

# ---------------------------------------------------------------------------
# Measurement primitives
# ---------------------------------------------------------------------------

# Single-stream download throughput (Mbps).
# Quick mode: single payload, outputs "median_mbps cv_pct stable_flag"
# Full mode: 3 payload sizes, outputs "composite_mbps small_mbps medium_mbps large_mbps cv_pct stable_flag"
measure_download() {
    local socks_port=$1

    if [ "$PAYLOAD_SIZES" = "single" ]; then
        local cmd="curl -o /dev/null -s -w '%{speed_download}' \
            --connect-timeout 10 --max-time 120 \
            --socks5-hostname '127.0.0.1:$socks_port' \
            'http://127.0.0.1:$HTTP_PORT/testdata'"
        local result
        result=$(run_pipeline "$cmd")
        local raw_median raw_cv raw_stable
        raw_median=$(echo "$result" | awk '{print $1}')
        raw_cv=$(echo "$result" | awk '{print $2}')
        raw_stable=$(echo "$result" | awk '{print $5}')
        # Convert bytes/s to Mbps
        local mbps
        mbps=$(python3 -c "print(f'{float($raw_median) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")
        echo "$mbps 0 0 0 $raw_cv $raw_stable"
        return
    fi

    # Full mode: 3 payload sizes
    local small_mbps=0 medium_mbps=0 large_mbps=0
    local worst_cv=0 all_stable=1

    for size in small medium large; do
        local file="testdata-${size}"
        local timeout=120
        [ "$size" = "small" ] && timeout=30
        [ "$size" = "medium" ] && timeout=60
        local cmd="curl -o /dev/null -s -w '%{speed_download}' \
            --connect-timeout 10 --max-time $timeout \
            --socks5-hostname '127.0.0.1:$socks_port' \
            'http://127.0.0.1:$HTTP_PORT/$file'"
        local result
        result=$(run_pipeline "$cmd")
        local raw_median raw_cv raw_stable
        raw_median=$(echo "$result" | awk '{print $1}')
        raw_cv=$(echo "$result" | awk '{print $2}')
        raw_stable=$(echo "$result" | awk '{print $5}')
        local mbps
        mbps=$(python3 -c "print(f'{float($raw_median) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")

        case "$size" in
            small)  small_mbps=$mbps ;;
            medium) medium_mbps=$mbps ;;
            large)  large_mbps=$mbps ;;
        esac

        # Track worst CV and stability across sizes
        worst_cv=$(python3 -c "print(f'{max(float($worst_cv), float($raw_cv)):.1f}')" 2>/dev/null || echo "$worst_cv")
        [ "$raw_stable" = "0" ] && all_stable=0
    done

    # Weighted composite: 0.3*small + 0.4*medium + 0.3*large
    local composite
    composite=$(python3 -c "
s, m, l = $small_mbps, $medium_mbps, $large_mbps
print(f'{0.3*s + 0.4*m + 0.3*l:.1f}')
" 2>/dev/null || echo "0")

    echo "$composite $small_mbps $medium_mbps $large_mbps $worst_cv $all_stable"
}

# Time-to-first-byte latency in ms. Uses run_pipeline with MAD filtering.
measure_latency() {
    local socks_port=$1
    local cmd="curl -o /dev/null -s -w '%{time_starttransfer}' \
        --connect-timeout 5 --max-time 10 \
        --socks5-hostname '127.0.0.1:$socks_port' \
        'http://127.0.0.1:$HTTP_PORT/ping'"
    local result
    result=$(run_pipeline "$cmd")
    local raw_median
    raw_median=$(echo "$result" | awk '{print $1}')
    # Convert seconds to ms
    python3 -c "print(f'{float($raw_median) * 1000:.1f}')" 2>/dev/null || echo "0"
}

# Handshake time (ms). Uses run_pipeline with MAD filtering.
measure_handshake() {
    local socks_port=$1
    local cmd="curl -o /dev/null -s -w '%{time_connect}' \
        --connect-timeout 5 --max-time 10 \
        --socks5-hostname '127.0.0.1:$socks_port' \
        'http://127.0.0.1:$HTTP_PORT/ping'"
    local result
    result=$(run_pipeline "$cmd")
    local raw_median
    raw_median=$(echo "$result" | awk '{print $1}')
    # Convert seconds to ms
    python3 -c "print(f'{float($raw_median) * 1000:.1f}')" 2>/dev/null || echo "0"
}

# Upload throughput (Mbps). Same structure as measure_download.
# Quick mode: "median_mbps cv_pct stable_flag"
# Full mode: "composite_mbps small_mbps medium_mbps large_mbps cv_pct stable_flag"
measure_upload() {
    local socks_port=$1

    if [ "$PAYLOAD_SIZES" = "single" ]; then
        local cmd="curl -s -w '%{speed_upload}' -o /dev/null \
            --connect-timeout 10 --max-time 120 \
            --data-binary @'$RESULTS_DIR/testdata' \
            --socks5-hostname '127.0.0.1:$socks_port' \
            'http://127.0.0.1:$HTTP_PORT/upload'"
        local result
        result=$(run_pipeline "$cmd")
        local raw_median raw_cv raw_stable
        raw_median=$(echo "$result" | awk '{print $1}')
        raw_cv=$(echo "$result" | awk '{print $2}')
        raw_stable=$(echo "$result" | awk '{print $5}')
        local mbps
        mbps=$(python3 -c "print(f'{float($raw_median) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")
        echo "$mbps 0 0 0 $raw_cv $raw_stable"
        return
    fi

    # Full mode: 3 payload sizes
    local small_mbps=0 medium_mbps=0 large_mbps=0
    local worst_cv=0 all_stable=1

    for size in small medium large; do
        local file="testdata-${size}"
        local timeout=120
        [ "$size" = "small" ] && timeout=30
        [ "$size" = "medium" ] && timeout=60
        local cmd="curl -s -w '%{speed_upload}' -o /dev/null \
            --connect-timeout 10 --max-time $timeout \
            --data-binary @'$RESULTS_DIR/$file' \
            --socks5-hostname '127.0.0.1:$socks_port' \
            'http://127.0.0.1:$HTTP_PORT/upload'"
        local result
        result=$(run_pipeline "$cmd")
        local raw_median raw_cv raw_stable
        raw_median=$(echo "$result" | awk '{print $1}')
        raw_cv=$(echo "$result" | awk '{print $2}')
        raw_stable=$(echo "$result" | awk '{print $5}')
        local mbps
        mbps=$(python3 -c "print(f'{float($raw_median) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")

        case "$size" in
            small)  small_mbps=$mbps ;;
            medium) medium_mbps=$mbps ;;
            large)  large_mbps=$mbps ;;
        esac

        worst_cv=$(python3 -c "print(f'{max(float($worst_cv), float($raw_cv)):.1f}')" 2>/dev/null || echo "$worst_cv")
        [ "$raw_stable" = "0" ] && all_stable=0
    done

    local composite
    composite=$(python3 -c "
s, m, l = $small_mbps, $medium_mbps, $large_mbps
print(f'{0.3*s + 0.4*m + 0.3*l:.1f}')
" 2>/dev/null || echo "0")

    echo "$composite $small_mbps $medium_mbps $large_mbps $worst_cv $all_stable"
}

# Aggregate throughput with N parallel downloads (Mbps).
# Uses run_pipeline with MAD filtering.
# Output: median_mbps cv_pct stable_flag
measure_concurrent() {
    local socks_port=$1 n=${2:-$CONCURRENCY}

    # Build a command that runs N parallel curls and sums their speed
    local cmd="__conc_tmpdir=\$(mktemp -d); \
        for __i in \$(seq 1 $n); do \
            curl -o /dev/null -s -w '%{speed_download}' \
                --connect-timeout 10 --max-time 120 \
                --socks5-hostname '127.0.0.1:$socks_port' \
                'http://127.0.0.1:$HTTP_PORT/testdata' \
                > \"\$__conc_tmpdir/\$__i\" 2>/dev/null & \
        done; \
        wait; \
        python3 -c \"import glob; total = sum(float(open(f).read().strip() or '0') for f in glob.glob('\$__conc_tmpdir/*')); print(f'{total * 8 / 1_000_000:.4f}')\" 2>/dev/null || echo 0; \
        rm -rf \"\$__conc_tmpdir\""

    local result
    result=$(run_pipeline "$cmd")
    local raw_median raw_cv raw_stable
    raw_median=$(echo "$result" | awk '{print $1}')
    raw_cv=$(echo "$result" | awk '{print $2}')
    raw_stable=$(echo "$result" | awk '{print $5}')
    echo "$raw_median $raw_cv $raw_stable"
}

# Sustained resource measurement: CPU + memory sampling under continuous load.
# Usage: measure_resources_sustained <socks_port> <pid1> [pid2 ...]
# Output: mem_idle_kb mem_load_kb mem_load_sd_kb cpu_avg_pct cpu_sd_pct
measure_resources_sustained() {
    local socks_port=$1
    shift
    local pids=("$@")
    local duration=$SUSTAINED_SECS

    # Phase 1: Idle measurement (5 samples, 1s apart)
    local idle_samples=()
    for _ in $(seq 1 5); do
        local total=0
        for pid in "${pids[@]}"; do
            local rss
            rss=$(get_rss_kb "$pid")
            total=$((total + rss))
        done
        idle_samples+=("$total")
        sleep 1
    done

    # Phase 2: Start sustained download + sample during load
    local load_file="testdata"
    [ "$PAYLOAD_SIZES" = "multi" ] && load_file="testdata-large"
    curl -o /dev/null -s --max-time "$duration" \
        --socks5-hostname "127.0.0.1:$socks_port" \
        "http://127.0.0.1:$HTTP_PORT/$load_file" &
    local curl_pid=$!

    local load_mem_samples=() cpu_ticks_data=()
    # Initial CPU ticks baseline
    local prev_ticks=0
    for pid in "${pids[@]}"; do
        local t
        t=$(get_cpu_ticks "$pid")
        prev_ticks=$((prev_ticks + t))
    done
    local prev_time
    prev_time=$(date +%s%N)

    for _ in $(seq 1 "$duration"); do
        sleep 1
        # Memory sample
        local total=0
        for pid in "${pids[@]}"; do
            local rss
            rss=$(get_rss_kb "$pid")
            total=$((total + rss))
        done
        load_mem_samples+=("$total")

        # CPU sample (delta since last interval)
        local curr_ticks=0
        for pid in "${pids[@]}"; do
            local t
            t=$(get_cpu_ticks "$pid")
            curr_ticks=$((curr_ticks + t))
        done
        local curr_time
        curr_time=$(date +%s%N)
        cpu_ticks_data+=("$((curr_ticks - prev_ticks)):$((curr_time - prev_time))")
        prev_ticks=$curr_ticks
        prev_time=$curr_time
    done

    wait $curl_pid 2>/dev/null || true

    # Compute results with Python
    python3 -c "
import math

clk_tck = $(getconf CLK_TCK 2>/dev/null || echo 100)

# Idle memory
idle = sorted(int(x) for x in '${idle_samples[*]}'.split())
mem_idle = idle[len(idle) // 2]

# Load memory
load_mem = [int(x) for x in '${load_mem_samples[*]}'.split() if x.strip()]
if load_mem:
    load_sorted = sorted(load_mem)
    mem_load = load_sorted[len(load_sorted) // 2]
    mem_mean = sum(load_mem) / len(load_mem)
    mem_sd = math.sqrt(sum((x - mem_mean) ** 2 for x in load_mem) / len(load_mem))
else:
    mem_load = mem_idle
    mem_sd = 0

# CPU percentages per interval
cpu_raw = '${cpu_ticks_data[*]}'.split()
cpu_pcts = []
for entry in cpu_raw:
    if ':' not in entry:
        continue
    dt_str, dw_str = entry.split(':')
    dt = int(dt_str)
    dw = int(dw_str)
    if dw > 0:
        cpu_pcts.append((dt / clk_tck) / (dw / 1e9) * 100)

if cpu_pcts:
    cpu_avg = sum(cpu_pcts) / len(cpu_pcts)
    cpu_sd = math.sqrt(sum((x - cpu_avg) ** 2 for x in cpu_pcts) / len(cpu_pcts))
else:
    cpu_avg = 0
    cpu_sd = 0

print(f'{mem_idle} {mem_load} {mem_sd:.0f} {cpu_avg:.1f} {cpu_sd:.1f}')
" 2>/dev/null || echo "0 0 0 0.0 0.0"
}

# ---------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------

# Direct download (no proxy) for baseline reference.
run_baseline() {
    log "=== Baseline (no proxy) ==="

    # Download throughput (pipeline-based)
    log "  Measuring baseline download..."
    local dl_cmd="curl -o /dev/null -s -w '%{speed_download}' \
        --connect-timeout 10 --max-time 120 \
        'http://127.0.0.1:$HTTP_PORT/testdata'"
    local dl_result
    dl_result=$(run_pipeline "$dl_cmd")
    local dl_raw dl_cv
    dl_raw=$(echo "$dl_result" | awk '{print $1}')
    dl_cv=$(echo "$dl_result" | awk '{print $2}')
    local dl_mbps
    dl_mbps=$(python3 -c "print(f'{float($dl_raw) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")

    # Latency (pipeline-based, no proxy)
    log "  Measuring baseline latency..."
    local lat_cmd="curl -o /dev/null -s -w '%{time_starttransfer}' \
        --connect-timeout 5 --max-time 10 \
        'http://127.0.0.1:$HTTP_PORT/ping'"
    local lat_result
    lat_result=$(run_pipeline "$lat_cmd")
    local lat_raw
    lat_raw=$(echo "$lat_result" | awk '{print $1}')
    local latency_ms
    latency_ms=$(python3 -c "print(f'{float($lat_raw) * 1000:.1f}')" 2>/dev/null || echo "0")

    # Handshake (pipeline-based, no proxy)
    log "  Measuring baseline handshake..."
    local hs_cmd="curl -o /dev/null -s -w '%{time_connect}' \
        --connect-timeout 5 --max-time 10 \
        'http://127.0.0.1:$HTTP_PORT/ping'"
    local hs_result
    hs_result=$(run_pipeline "$hs_cmd")
    local hs_raw
    hs_raw=$(echo "$hs_result" | awk '{print $1}')
    local handshake_ms
    handshake_ms=$(python3 -c "print(f'{float($hs_raw) * 1000:.1f}')" 2>/dev/null || echo "0")

    # Upload throughput (pipeline-based)
    log "  Measuring baseline upload..."
    local ul_cmd="curl -s -w '%{speed_upload}' -o /dev/null \
        --connect-timeout 10 --max-time 120 \
        --data-binary @'$RESULTS_DIR/testdata' \
        'http://127.0.0.1:$HTTP_PORT/upload'"
    local ul_result
    ul_result=$(run_pipeline "$ul_cmd")
    local ul_raw
    ul_raw=$(echo "$ul_result" | awk '{print $1}')
    local ul_mbps
    ul_mbps=$(python3 -c "print(f'{float($ul_raw) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")

    log "  Download: ${dl_mbps} Mbps  |  Upload: ${ul_mbps} Mbps  |  Latency: ${latency_ms} ms"

    python3 -c "
import json
json.dump({
    'label': 'baseline',
    'download_mbps': $dl_mbps,
    'upload_mbps': $ul_mbps,
    'latency_ms': $latency_ms,
    'handshake_ms': $handshake_ms,
    'concurrent_mbps': 0,
    'memory_idle_kb': 0,
    'memory_load_kb': 0,
    'cpu_avg_pct': 0,
    'download_cv_pct': $dl_cv,
    'concurrent_cv_pct': 0,
    'download_small_mbps': 0,
    'download_medium_mbps': 0,
    'download_large_mbps': 0,
    'upload_small_mbps': 0,
    'upload_medium_mbps': 0,
    'upload_large_mbps': 0,
    'cpu_sd_pct': 0,
    'memory_load_sd_kb': 0,
    'download_stable': True,
    'upload_stable': True,
    'concurrent_stable': True
}, open('$RESULTS_DIR/baseline.json', 'w'))
"
}

write_empty_result() {
    local label=$1
    python3 -c "
import json
json.dump({'label':'$label','download_mbps':0,'upload_mbps':0,'latency_ms':0,
           'handshake_ms':0,'concurrent_mbps':0,'memory_idle_kb':0,
           'memory_load_kb':0,'cpu_avg_pct':0,'download_cv_pct':0,
           'concurrent_cv_pct':0,
           'download_small_mbps':0,'download_medium_mbps':0,'download_large_mbps':0,
           'upload_small_mbps':0,'upload_medium_mbps':0,'upload_large_mbps':0,
           'cpu_sd_pct':0,'memory_load_sd_kb':0,
           'download_stable':True,'upload_stable':True,'concurrent_stable':True},
          open('$RESULTS_DIR/${label}.json','w'))
"
}

# Warm-up: verify end-to-end connectivity through SOCKS5 (tunnel may not
# be established even though the listener is up).
wait_for_tunnel() {
    local socks_port=$1 timeout=${2:-15}
    for _ in $(seq 1 "$timeout"); do
        if curl -o /dev/null -s --connect-timeout 2 --max-time 3 \
            --socks5-hostname "127.0.0.1:$socks_port" \
            "http://127.0.0.1:$HTTP_PORT/ping" 2>/dev/null; then
            return 0
        fi
        sleep 1
    done
    err "Tunnel not ready on SOCKS5 port $socks_port after ${timeout}s"
    return 1
}

# Warmup: make 3 small requests to warm TLS cache, congestion window, buffers.
warmup_tunnel() {
    local socks_port=$1
    for _ in 1 2 3 4 5; do
        curl -o /dev/null -s --connect-timeout 3 --max-time 5 \
            --socks5-hostname "127.0.0.1:$socks_port" \
            "http://127.0.0.1:$HTTP_PORT/ping" 2>/dev/null || true
    done
}

run_prisma_scenario() {
    local label=$1 server_cfg=$2 client_cfg=$3 socks_port=$4

    log "=== $label ==="

    "$PRISMA_BIN" server -c "$server_cfg" \
        > "$RESULTS_DIR/${label}-server.log" 2>&1 &
    local srv=$!; PIDS+=($srv)
    sleep 3

    "$PRISMA_BIN" client -c "$client_cfg" \
        > "$RESULTS_DIR/${label}-client.log" 2>&1 &
    local cli=$!; PIDS+=($cli)

    if ! wait_for_port "$socks_port" 15; then
        err "$label: client failed to start. Log:"
        tail -20 "$RESULTS_DIR/${label}-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        write_empty_result "$label"
        return
    fi

    if ! wait_for_tunnel "$socks_port" 15; then
        err "$label: tunnel not functional. Server log:"
        tail -10 "$RESULTS_DIR/${label}-server.log" >&2 || true
        err "Client log:"
        tail -10 "$RESULTS_DIR/${label}-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        write_empty_result "$label"
        return
    fi

    # Warmup
    warmup_tunnel "$socks_port"

    # Latency (TTFB)
    log "  Measuring latency..."
    local latency_ms
    latency_ms=$(measure_latency "$socks_port")

    # Handshake time
    log "  Measuring handshake time..."
    local handshake_ms
    handshake_ms=$(measure_handshake "$socks_port")

    # Download throughput (mixed payloads in full mode)
    log "  Measuring download throughput..."
    local dl_result dl_mbps dl_small dl_medium dl_large dl_cv dl_stable
    dl_result=$(measure_download "$socks_port")
    dl_mbps=$(echo "$dl_result" | awk '{print $1}')
    dl_small=$(echo "$dl_result" | awk '{print $2}')
    dl_medium=$(echo "$dl_result" | awk '{print $3}')
    dl_large=$(echo "$dl_result" | awk '{print $4}')
    dl_cv=$(echo "$dl_result" | awk '{print $5}')
    dl_stable=$(echo "$dl_result" | awk '{print $6}')

    # Upload throughput (mixed payloads in full mode)
    log "  Measuring upload throughput..."
    local ul_result ul_mbps ul_small ul_medium ul_large ul_cv ul_stable
    ul_result=$(measure_upload "$socks_port")
    ul_mbps=$(echo "$ul_result" | awk '{print $1}')
    ul_small=$(echo "$ul_result" | awk '{print $2}')
    ul_medium=$(echo "$ul_result" | awk '{print $3}')
    ul_large=$(echo "$ul_result" | awk '{print $4}')
    ul_cv=$(echo "$ul_result" | awk '{print $5}')
    ul_stable=$(echo "$ul_result" | awk '{print $6}')

    # Sustained resource measurement (CPU + memory under load)
    log "  Measuring sustained resources (${SUSTAINED_SECS}s)..."
    local res_result mem_idle mem_load mem_load_sd cpu_pct cpu_sd
    res_result=$(measure_resources_sustained "$socks_port" $srv $cli)
    mem_idle=$(echo "$res_result" | awk '{print $1}')
    mem_load=$(echo "$res_result" | awk '{print $2}')
    mem_load_sd=$(echo "$res_result" | awk '{print $3}')
    cpu_pct=$(echo "$res_result" | awk '{print $4}')
    cpu_sd=$(echo "$res_result" | awk '{print $5}')

    # Concurrent throughput
    log "  Measuring concurrent throughput (${CONCURRENCY}x parallel)..."
    local conc_result concurrent_mbps conc_cv conc_stable
    conc_result=$(measure_concurrent "$socks_port")
    concurrent_mbps=$(echo "$conc_result" | awk '{print $1}')
    conc_cv=$(echo "$conc_result" | awk '{print $2}')
    conc_stable=$(echo "$conc_result" | awk '{print $3}')

    log "  Download: ${dl_mbps} Mbps (±${dl_cv}%)  |  Upload: ${ul_mbps} Mbps"
    log "  ${CONCURRENCY}x: ${concurrent_mbps} Mbps (±${conc_cv}%)  |  Handshake: ${handshake_ms} ms"
    log "  Latency: ${latency_ms} ms  |  CPU: ${cpu_pct}% (±${cpu_sd})  |  Mem idle: ${mem_idle} KB"

    python3 -c "
import json
json.dump({
    'label': '$label',
    'download_mbps': $dl_mbps,
    'upload_mbps': $ul_mbps,
    'latency_ms': $latency_ms,
    'handshake_ms': $handshake_ms,
    'concurrent_mbps': $concurrent_mbps,
    'memory_idle_kb': $mem_idle,
    'memory_load_kb': $mem_load,
    'cpu_avg_pct': $cpu_pct,
    'download_cv_pct': $dl_cv,
    'concurrent_cv_pct': $conc_cv,
    'download_small_mbps': $dl_small,
    'download_medium_mbps': $dl_medium,
    'download_large_mbps': $dl_large,
    'upload_small_mbps': $ul_small,
    'upload_medium_mbps': $ul_medium,
    'upload_large_mbps': $ul_large,
    'cpu_sd_pct': $cpu_sd,
    'memory_load_sd_kb': $mem_load_sd,
    'download_stable': $( [ "$dl_stable" = "1" ] && echo "True" || echo "False" ),
    'upload_stable': $( [ "$ul_stable" = "1" ] && echo "True" || echo "False" ),
    'concurrent_stable': $( [ "$conc_stable" = "1" ] && echo "True" || echo "False" )
}, open('$RESULTS_DIR/${label}.json', 'w'))
"

    kill $srv $cli 2>/dev/null || true
    wait $srv $cli 2>/dev/null || true
    sleep $COOLDOWN_SECS
}

# Generic Xray scenario runner.
# Usage: run_xray_scenario <label> <server_json> <client_json> <server_port> <socks_port>
run_xray_scenario() {
    local label=$1 server_cfg=$2 client_cfg=$3 server_port=$4 socks_port=$5

    if [ ! -f "$XRAY_BIN" ]; then
        log "Xray binary not found at $XRAY_BIN — skipping $label"
        write_empty_result "$label"
        return
    fi

    log "=== $label ==="

    "$XRAY_BIN" run -c "$server_cfg" \
        > "$RESULTS_DIR/${label}-server.log" 2>&1 &
    local srv=$!; PIDS+=($srv)
    if ! wait_for_port "$server_port" 15; then
        err "$label: Xray server failed to start. Log:"
        tail -20 "$RESULTS_DIR/${label}-server.log" >&2 || true
        kill $srv 2>/dev/null || true
        write_empty_result "$label"
        return
    fi

    "$XRAY_BIN" run -c "$client_cfg" \
        > "$RESULTS_DIR/${label}-client.log" 2>&1 &
    local cli=$!; PIDS+=($cli)
    if ! wait_for_port "$socks_port" 15; then
        err "$label: Xray client failed to start. Log:"
        tail -20 "$RESULTS_DIR/${label}-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        write_empty_result "$label"
        return
    fi

    if ! wait_for_tunnel "$socks_port" 15; then
        err "$label: tunnel not functional. Server log:"
        tail -10 "$RESULTS_DIR/${label}-server.log" >&2 || true
        err "Client log:"
        tail -10 "$RESULTS_DIR/${label}-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        write_empty_result "$label"
        return
    fi

    # Warmup
    warmup_tunnel "$socks_port"

    # Latency (TTFB)
    log "  Measuring latency..."
    local latency_ms
    latency_ms=$(measure_latency "$socks_port")

    # Handshake time
    log "  Measuring handshake time..."
    local handshake_ms
    handshake_ms=$(measure_handshake "$socks_port")

    # Download throughput (mixed payloads in full mode)
    log "  Measuring download throughput..."
    local dl_result dl_mbps dl_small dl_medium dl_large dl_cv dl_stable
    dl_result=$(measure_download "$socks_port")
    dl_mbps=$(echo "$dl_result" | awk '{print $1}')
    dl_small=$(echo "$dl_result" | awk '{print $2}')
    dl_medium=$(echo "$dl_result" | awk '{print $3}')
    dl_large=$(echo "$dl_result" | awk '{print $4}')
    dl_cv=$(echo "$dl_result" | awk '{print $5}')
    dl_stable=$(echo "$dl_result" | awk '{print $6}')

    # Upload throughput (mixed payloads in full mode)
    log "  Measuring upload throughput..."
    local ul_result ul_mbps ul_small ul_medium ul_large ul_cv ul_stable
    ul_result=$(measure_upload "$socks_port")
    ul_mbps=$(echo "$ul_result" | awk '{print $1}')
    ul_small=$(echo "$ul_result" | awk '{print $2}')
    ul_medium=$(echo "$ul_result" | awk '{print $3}')
    ul_large=$(echo "$ul_result" | awk '{print $4}')
    ul_cv=$(echo "$ul_result" | awk '{print $5}')
    ul_stable=$(echo "$ul_result" | awk '{print $6}')

    # Sustained resource measurement (CPU + memory under load)
    log "  Measuring sustained resources (${SUSTAINED_SECS}s)..."
    local res_result mem_idle mem_load mem_load_sd cpu_pct cpu_sd
    res_result=$(measure_resources_sustained "$socks_port" $srv $cli)
    mem_idle=$(echo "$res_result" | awk '{print $1}')
    mem_load=$(echo "$res_result" | awk '{print $2}')
    mem_load_sd=$(echo "$res_result" | awk '{print $3}')
    cpu_pct=$(echo "$res_result" | awk '{print $4}')
    cpu_sd=$(echo "$res_result" | awk '{print $5}')

    # Concurrent throughput
    log "  Measuring concurrent throughput (${CONCURRENCY}x parallel)..."
    local conc_result concurrent_mbps conc_cv conc_stable
    conc_result=$(measure_concurrent "$socks_port")
    concurrent_mbps=$(echo "$conc_result" | awk '{print $1}')
    conc_cv=$(echo "$conc_result" | awk '{print $2}')
    conc_stable=$(echo "$conc_result" | awk '{print $3}')

    log "  Download: ${dl_mbps} Mbps (±${dl_cv}%)  |  Upload: ${ul_mbps} Mbps"
    log "  ${CONCURRENCY}x: ${concurrent_mbps} Mbps (±${conc_cv}%)  |  Handshake: ${handshake_ms} ms"
    log "  Latency: ${latency_ms} ms  |  CPU: ${cpu_pct}% (±${cpu_sd})  |  Mem idle: ${mem_idle} KB"

    python3 -c "
import json
json.dump({
    'label': '$label',
    'download_mbps': $dl_mbps,
    'upload_mbps': $ul_mbps,
    'latency_ms': $latency_ms,
    'handshake_ms': $handshake_ms,
    'concurrent_mbps': $concurrent_mbps,
    'memory_idle_kb': $mem_idle,
    'memory_load_kb': $mem_load,
    'cpu_avg_pct': $cpu_pct,
    'download_cv_pct': $dl_cv,
    'concurrent_cv_pct': $conc_cv,
    'download_small_mbps': $dl_small,
    'download_medium_mbps': $dl_medium,
    'download_large_mbps': $dl_large,
    'upload_small_mbps': $ul_small,
    'upload_medium_mbps': $ul_medium,
    'upload_large_mbps': $ul_large,
    'cpu_sd_pct': $cpu_sd,
    'memory_load_sd_kb': $mem_load_sd,
    'download_stable': $( [ "$dl_stable" = "1" ] && echo "True" || echo "False" ),
    'upload_stable': $( [ "$ul_stable" = "1" ] && echo "True" || echo "False" ),
    'concurrent_stable': $( [ "$conc_stable" = "1" ] && echo "True" || echo "False" )
}, open('$RESULTS_DIR/${label}.json', 'w'))
"

    kill $srv $cli 2>/dev/null || true
    wait $srv $cli 2>/dev/null || true
    sleep $COOLDOWN_SECS
}

# Check if a UDP port is listening (for Hysteria2/TUIC).
wait_for_udp_port() {
    local port=$1 timeout=${2:-10}
    for _ in $(seq 1 "$timeout"); do
        if ss -uln 2>/dev/null | grep -q ":${port} "; then
            return 0
        fi
        sleep 1
    done
    err "UDP port $port not ready after ${timeout}s"
    return 1
}

# Generic sing-box scenario runner.
# Usage: run_singbox_scenario <label> <server_json> <client_json> <server_port> <socks_port> [udp]
run_singbox_scenario() {
    local label=$1 server_cfg=$2 client_cfg=$3 server_port=$4 socks_port=$5 transport=${6:-tcp}

    if [ ! -f "$SINGBOX_BIN" ]; then
        log "sing-box binary not found at $SINGBOX_BIN — skipping $label"
        write_empty_result "$label"
        return
    fi

    log "=== $label ==="

    "$SINGBOX_BIN" run -c "$server_cfg" \
        > "$RESULTS_DIR/${label}-server.log" 2>&1 &
    local srv=$!; PIDS+=($srv)
    if [ "$transport" = "udp" ]; then
        if ! wait_for_udp_port "$server_port" 15; then
            err "$label: sing-box server failed to start (UDP). Log:"
            tail -20 "$RESULTS_DIR/${label}-server.log" >&2 || true
            kill $srv 2>/dev/null || true
            write_empty_result "$label"
            return
        fi
    else
        if ! wait_for_port "$server_port" 15; then
            err "$label: sing-box server failed to start. Log:"
            tail -20 "$RESULTS_DIR/${label}-server.log" >&2 || true
            kill $srv 2>/dev/null || true
            write_empty_result "$label"
            return
        fi
    fi

    "$SINGBOX_BIN" run -c "$client_cfg" \
        > "$RESULTS_DIR/${label}-client.log" 2>&1 &
    local cli=$!; PIDS+=($cli)
    if ! wait_for_port "$socks_port" 15; then
        err "$label: sing-box client failed to start. Log:"
        tail -20 "$RESULTS_DIR/${label}-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        write_empty_result "$label"
        return
    fi

    if ! wait_for_tunnel "$socks_port" 15; then
        err "$label: tunnel not functional. Server log:"
        tail -10 "$RESULTS_DIR/${label}-server.log" >&2 || true
        err "Client log:"
        tail -10 "$RESULTS_DIR/${label}-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        write_empty_result "$label"
        return
    fi

    # Warmup
    warmup_tunnel "$socks_port"

    # Latency (TTFB)
    log "  Measuring latency..."
    local latency_ms
    latency_ms=$(measure_latency "$socks_port")

    # Handshake time
    log "  Measuring handshake time..."
    local handshake_ms
    handshake_ms=$(measure_handshake "$socks_port")

    # Download throughput (mixed payloads in full mode)
    log "  Measuring download throughput..."
    local dl_result dl_mbps dl_small dl_medium dl_large dl_cv dl_stable
    dl_result=$(measure_download "$socks_port")
    dl_mbps=$(echo "$dl_result" | awk '{print $1}')
    dl_small=$(echo "$dl_result" | awk '{print $2}')
    dl_medium=$(echo "$dl_result" | awk '{print $3}')
    dl_large=$(echo "$dl_result" | awk '{print $4}')
    dl_cv=$(echo "$dl_result" | awk '{print $5}')
    dl_stable=$(echo "$dl_result" | awk '{print $6}')

    # Upload throughput (mixed payloads in full mode)
    log "  Measuring upload throughput..."
    local ul_result ul_mbps ul_small ul_medium ul_large ul_cv ul_stable
    ul_result=$(measure_upload "$socks_port")
    ul_mbps=$(echo "$ul_result" | awk '{print $1}')
    ul_small=$(echo "$ul_result" | awk '{print $2}')
    ul_medium=$(echo "$ul_result" | awk '{print $3}')
    ul_large=$(echo "$ul_result" | awk '{print $4}')
    ul_cv=$(echo "$ul_result" | awk '{print $5}')
    ul_stable=$(echo "$ul_result" | awk '{print $6}')

    # Sustained resource measurement (CPU + memory under load)
    log "  Measuring sustained resources (${SUSTAINED_SECS}s)..."
    local res_result mem_idle mem_load mem_load_sd cpu_pct cpu_sd
    res_result=$(measure_resources_sustained "$socks_port" $srv $cli)
    mem_idle=$(echo "$res_result" | awk '{print $1}')
    mem_load=$(echo "$res_result" | awk '{print $2}')
    mem_load_sd=$(echo "$res_result" | awk '{print $3}')
    cpu_pct=$(echo "$res_result" | awk '{print $4}')
    cpu_sd=$(echo "$res_result" | awk '{print $5}')

    # Concurrent throughput
    log "  Measuring concurrent throughput (${CONCURRENCY}x parallel)..."
    local conc_result concurrent_mbps conc_cv conc_stable
    conc_result=$(measure_concurrent "$socks_port")
    concurrent_mbps=$(echo "$conc_result" | awk '{print $1}')
    conc_cv=$(echo "$conc_result" | awk '{print $2}')
    conc_stable=$(echo "$conc_result" | awk '{print $3}')

    log "  Download: ${dl_mbps} Mbps (±${dl_cv}%)  |  Upload: ${ul_mbps} Mbps"
    log "  ${CONCURRENCY}x: ${concurrent_mbps} Mbps (±${conc_cv}%)  |  Handshake: ${handshake_ms} ms"
    log "  Latency: ${latency_ms} ms  |  CPU: ${cpu_pct}% (±${cpu_sd})  |  Mem idle: ${mem_idle} KB"

    python3 -c "
import json
json.dump({
    'label': '$label',
    'download_mbps': $dl_mbps,
    'upload_mbps': $ul_mbps,
    'latency_ms': $latency_ms,
    'handshake_ms': $handshake_ms,
    'concurrent_mbps': $concurrent_mbps,
    'memory_idle_kb': $mem_idle,
    'memory_load_kb': $mem_load,
    'cpu_avg_pct': $cpu_pct,
    'download_cv_pct': $dl_cv,
    'concurrent_cv_pct': $conc_cv,
    'download_small_mbps': $dl_small,
    'download_medium_mbps': $dl_medium,
    'download_large_mbps': $dl_large,
    'upload_small_mbps': $ul_small,
    'upload_medium_mbps': $ul_medium,
    'upload_large_mbps': $ul_large,
    'cpu_sd_pct': $cpu_sd,
    'memory_load_sd_kb': $mem_load_sd,
    'download_stable': $( [ "$dl_stable" = "1" ] && echo "True" || echo "False" ),
    'upload_stable': $( [ "$ul_stable" = "1" ] && echo "True" || echo "False" ),
    'concurrent_stable': $( [ "$conc_stable" = "1" ] && echo "True" || echo "False" )
}, open('$RESULTS_DIR/${label}.json', 'w'))
"

    kill $srv $cli 2>/dev/null || true
    wait $srv $cli 2>/dev/null || true
    sleep $COOLDOWN_SECS
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

generate_summary() {
    # Write the summary script, then run it — avoids all heredoc/quoting issues
    cat > "$RESULTS_DIR/_summary.py" <<'PYEOF'
import json, os, sys

RESULTS = os.environ["RESULTS_DIR"]
CONCURRENCY = int(os.environ["CONCURRENCY"])
TEST_MB = int(os.environ["TEST_SIZE_MB"])
DATE = os.environ["BENCH_DATE"]

scenarios = [
    ("baseline",         "Baseline"),
    ("prisma-quic",      "Prisma QUIC"),
    ("prisma-tcp",       "Prisma TCP+TLS"),
    ("prisma-shaped",    "Prisma (shaped)"),
    ("prisma-quic-aes",  "Prisma QUIC AES"),
    ("prisma-tonly",     "Prisma T-Only"),
    ("prisma-ws",        "Prisma WS+TLS"),
    ("prisma-bucket",    "Prisma (bucket)"),
    ("xray-vless-tls",   "Xray VLESS+TLS"),
    ("xray-vless-xtls",  "Xray VLESS+XTLS"),
    ("xray-vmess-tls",   "Xray VMess+TLS"),
    ("xray-trojan-tls",  "Xray Trojan+TLS"),
    ("xray-ss-aead",     "Xray SS AEAD"),
    ("xray-ss2022",      "Xray SS-2022"),
    ("xray-vless-ws",    "Xray VLESS+WS"),
    ("xray-vless-grpc",  "Xray VLESS+gRPC"),
    ("singbox-vless-tls",   "sing-box VLESS+TLS"),
    ("singbox-vmess-tls",   "sing-box VMess+TLS"),
    ("singbox-trojan-tls",  "sing-box Trojan+TLS"),
    ("singbox-ss-aead",     "sing-box SS AEAD"),
    ("singbox-ss2022",      "sing-box SS-2022"),
    ("singbox-vless-ws",    "sing-box VLESS+WS"),
    ("singbox-hysteria2",   "sing-box Hysteria2"),
    ("singbox-tuic",        "sing-box TUIC v5"),
    ("singbox-vmess-ws",    "sing-box VMess+WS"),
    ("singbox-trojan-ws",   "sing-box Trojan+WS"),
    ("singbox-ss2022-relay","sing-box SS2022 Relay"),
    ("singbox-vless-grpc",  "sing-box VLESS+gRPC"),
    ("singbox-vless-http2", "sing-box VLESS+H2"),
]

fields = [
    ("download_mbps",   "Download (Mbps)"),
    ("upload_mbps",     "Upload (Mbps)"),
    ("latency_ms",      "Latency TTFB (ms)"),
    ("handshake_ms",    "Handshake (ms)"),
    ("concurrent_mbps", f"{CONCURRENCY}x Concurrent (Mbps)"),
    ("cpu_avg_pct",     "CPU under load (%)"),
    ("memory_idle_kb",  "Memory idle (KB)"),
    ("memory_load_kb",  "Memory load (KB)"),
]

# Fields that show ±CV% deviation indicator
cv_fields = {
    "download_mbps": "download_cv_pct",
    "concurrent_mbps": "concurrent_cv_pct",
}

# ── Security Scoring ───────────────────────────────────────────────────
# Six dimensions, each rated 0-10
SECURITY_WEIGHTS = {
    "enc": 0.25,   # Encryption Depth
    "fs": 0.20,    # Forward Secrecy
    "tar": 0.20,   # Traffic Analysis Resistance
    "pdr": 0.15,   # Protocol Detection Resistance
    "ar": 0.10,    # Anti-Replay
    "auth": 0.10,  # Auth Strength
}

SECURITY_LABELS = {
    "enc": "Encryption",
    "fs": "Fwd Secrecy",
    "tar": "Traffic Res.",
    "pdr": "Detection Res.",
    "ar": "Anti-Replay",
    "auth": "Auth",
}

SECURITY_DB = {
    "prisma-quic":     {"enc": 10, "fs": 10, "tar": 3, "pdr": 8, "ar": 10, "auth": 10},
    "prisma-tcp":      {"enc": 10, "fs": 10, "tar": 3, "pdr": 7, "ar": 10, "auth": 10},
    "prisma-shaped":   {"enc": 10, "fs": 10, "tar": 6, "pdr": 8, "ar": 10, "auth": 10},
    "prisma-quic-aes": {"enc": 10, "fs": 10, "tar": 3, "pdr": 8, "ar": 10, "auth": 10},
    "prisma-tonly":    {"enc": 5,  "fs": 10, "tar": 3, "pdr": 7, "ar": 10, "auth": 10},
    "prisma-ws":       {"enc": 10, "fs": 10, "tar": 3, "pdr": 9, "ar": 10, "auth": 10},
    "prisma-bucket":   {"enc": 10, "fs": 10, "tar": 9, "pdr": 7, "ar": 10, "auth": 10},
    "xray-vless-tls":  {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 2,  "auth": 3},
    "xray-vless-xtls": {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 4, "ar": 2,  "auth": 3},
    "xray-vmess-tls":  {"enc": 8,  "fs": 7,  "tar": 1, "pdr": 6, "ar": 5,  "auth": 5},
    "xray-trojan-tls": {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 2,  "auth": 4},
    "xray-ss-aead":    {"enc": 6,  "fs": 3,  "tar": 1, "pdr": 3, "ar": 4,  "auth": 4},
    "xray-ss2022":     {"enc": 7,  "fs": 5,  "tar": 1, "pdr": 4, "ar": 7,  "auth": 6},
    "xray-vless-ws":   {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 6, "ar": 2,  "auth": 3},
    "xray-vless-grpc": {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 2,  "auth": 3},
    "singbox-vless-tls":   {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 2,  "auth": 3},
    "singbox-vmess-tls":   {"enc": 8,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 3,  "auth": 5},
    "singbox-trojan-tls":  {"enc": 7,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 2,  "auth": 3},
    "singbox-ss-aead":     {"enc": 6,  "fs": 3,  "tar": 1, "pdr": 3, "ar": 3,  "auth": 5},
    "singbox-ss2022":      {"enc": 6,  "fs": 4,  "tar": 1, "pdr": 3, "ar": 4,  "auth": 5},
    "singbox-vless-ws":    {"enc": 5,  "fs": 7,  "tar": 1, "pdr": 3, "ar": 2,  "auth": 3},
    "singbox-hysteria2":   {"enc": 8,  "fs": 9,  "tar": 4, "pdr": 7, "ar": 7,  "auth": 7},
    "singbox-tuic":        {"enc": 8,  "fs": 9,  "tar": 3, "pdr": 6, "ar": 7,  "auth": 7},
    "singbox-vmess-ws":    {"enc": 8,  "fs": 7,  "tar": 1, "pdr": 6, "ar": 3,  "auth": 5},
    "singbox-trojan-ws":   {"enc": 7,  "fs": 7,  "tar": 1, "pdr": 6, "ar": 2,  "auth": 3},
    "singbox-ss2022-relay":{"enc": 7,  "fs": 5,  "tar": 1, "pdr": 4, "ar": 5,  "auth": 6},
    "singbox-vless-grpc":  {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 2,  "auth": 3},
    "singbox-vless-http2": {"enc": 3,  "fs": 7,  "tar": 1, "pdr": 5, "ar": 2,  "auth": 3},
}

def compute_security_score(key):
    dims = SECURITY_DB.get(key)
    if not dims:
        return 0
    return round(sum(dims[d] * SECURITY_WEIGHTS[d] for d in SECURITY_WEIGHTS) * 10)

def security_tier(score):
    if score >= 85:
        return "S"
    if score >= 70:
        return "A"
    if score >= 55:
        return "B"
    return "C"

TIER_NAMES = {"S": "Hardened", "A": "Strong", "B": "Moderate", "C": "Basic"}

profiles = {
    "Personal VPN":        {"download_mbps": 15, "upload_mbps": 5, "latency_ms": 20, "handshake_ms": 5, "concurrent_mbps": 10, "cpu_avg_pct": 5, "memory_idle_kb": 5, "tput_per_mb": 10, "security_score": 25},
    "Multi-Tenant SaaS":   {"download_mbps": 10, "upload_mbps": 10, "latency_ms": 10, "handshake_ms": 5, "concurrent_mbps": 20, "cpu_avg_pct": 10, "memory_idle_kb": 10, "tput_per_mb": 10, "security_score": 15},
    "Edge / IoT":          {"download_mbps": 10, "upload_mbps": 5, "latency_ms": 5, "handshake_ms": 5, "concurrent_mbps": 10, "cpu_avg_pct": 15, "memory_idle_kb": 20, "tput_per_mb": 15, "security_score": 15},
    "CDN / Bulk Transfer": {"download_mbps": 25, "upload_mbps": 10, "latency_ms": 5, "handshake_ms": 5, "concurrent_mbps": 20, "cpu_avg_pct": 10, "memory_idle_kb": 5, "tput_per_mb": 10, "security_score": 10},
    "High-Security":       {"download_mbps": 5, "upload_mbps": 5, "latency_ms": 5, "handshake_ms": 5, "concurrent_mbps": 5, "cpu_avg_pct": 5, "memory_idle_kb": 5, "tput_per_mb": 5, "security_score": 60},
}

# Load results — only include scenarios that have a result file
data = {}
present = []
for key, name in scenarios:
    path = os.path.join(RESULTS, f"{key}.json")
    try:
        d = json.load(open(path))
        data[key] = d
        present.append((key, name))
    except Exception:
        pass

if not present:
    print("  No results found.")
    sys.exit(0)

def val(key, field):
    if field == "security_score":
        return float(compute_security_score(key))
    v = data.get(key, {}).get(field, 0)
    try:
        v = float(v)
    except (TypeError, ValueError):
        return 0.0
    return v

def fmt(v, dash_zero=False):
    if dash_zero and v == 0:
        return "\u2014"
    if v == int(v):
        return f"{int(v):,}"
    return f"{v:,.1f}"

def fmt_cv(key, field):
    """Return ±CV% string for throughput fields, empty string otherwise."""
    cv_field = cv_fields.get(field)
    if not cv_field:
        return ""
    cv = val(key, cv_field)
    if cv > 0:
        return f"\u00b1{cv:.0f}%"
    return ""

# ── Colors ──────────────────────────────────────────────────────────────
G = "\033[0;32m"
C = "\033[0;36m"
Y = "\033[0;33m"
R = "\033[0;31m"
B = "\033[1m"
N = "\033[0m"

col_w = 18
label_w = 24
skip_bl = {"concurrent_mbps", "cpu_avg_pct", "memory_idle_kb", "memory_load_kb",
           "upload_mbps", "handshake_ms", "download_cv_pct", "concurrent_cv_pct"}

# ── Terminal table ──────────────────────────────────────────────────────
bar = "\u2500" * (label_w + col_w * len(present))
print()
print(f"  {G}{bar}{N}")
print(f"  {B}Benchmark Results \u2014 {DATE}{N}")
print(f"  {TEST_MB}MB payload \u00B7 {CONCURRENCY}x concurrent \u00B7 loopback")
print(f"  Measurements: MAD-filtered median (throughput), pipeline-filtered (latency), sustained sampling (CPU/memory)")
print(f"  {G}{bar}{N}")
print()

hdrs = "".join(name.rjust(col_w) for _, name in present)
print(f"  {'':<{label_w}}{hdrs}")
print(f"  {bar}")

for field, label in fields:
    cells = []
    for key, _ in present:
        v = val(key, field)
        if field in skip_bl and key == "baseline":
            cells.append("\u2014".rjust(col_w))
        elif v == 0:
            cells.append("-".rjust(col_w))
        else:
            cv_str = fmt_cv(key, field)
            cell = f"{fmt(v)}{cv_str}"
            cells.append(cell.rjust(col_w))
    print(f"  {label:<{label_w}}{''.join(cells)}")

# Security Score row
sec_cells = []
for key, _ in present:
    if key == "baseline":
        sec_cells.append("\u2014".rjust(col_w))
    else:
        sc = compute_security_score(key)
        tier = security_tier(sc)
        sec_cells.append(f"{sc} ({tier})".rjust(col_w))
print(f"  {'Security Score':<{label_w}}{''.join(sec_cells)}")

print(f"  {bar}")

# ── Security Properties Table ──────────────────────────────────────────
proxy_keys = [k for k, _ in present if k != "baseline"]
proxy_names = {k: n for k, n in present if k != "baseline"}

if proxy_keys:
    sec_col = max(len(proxy_names[k]) for k in proxy_keys) + 2
    sec_bar = "\u2500" * (16 + sec_col * len(proxy_keys))

    print()
    print(f"  {C}{B}Security Properties{N}")
    print(f"  {sec_bar}")
    sec_hdr = "".join(proxy_names[k].rjust(sec_col) for k in proxy_keys)
    print(f"  {'':16}{sec_hdr}")
    print(f"  {sec_bar}")

    for dim, dim_label in SECURITY_LABELS.items():
        parts = []
        for k in proxy_keys:
            dims = SECURITY_DB.get(k, {})
            v = dims.get(dim, 0)
            parts.append(f"{v}/10".rjust(sec_col))
        print(f"  {dim_label:16}{''.join(parts)}")

    # Score + Tier row
    score_parts = []
    for k in proxy_keys:
        sc = compute_security_score(k)
        tier = security_tier(sc)
        label_str = f"{sc} ({tier})"
        score_parts.append(label_str.rjust(sec_col))
    print(f"  {sec_bar}")
    print(f"  {'Score (Tier)':16}{''.join(score_parts)}")
    print(f"  {sec_bar}")

    print()
    print(f"  {C}Security Dimensions:{N}")
    print(f"    Encryption   = Double (app+transport) vs single vs MAC-only")
    print(f"    Fwd Secrecy  = Ephemeral key exchange (X25519/ECDHE/PSK)")
    print(f"    Traffic Res. = Padding resistance to traffic analysis")
    print(f"    Detection    = Protocol detection resistance (WS/QUIC/TLS/raw)")
    print(f"    Anti-Replay  = Replay attack protection mechanism")
    print(f"    Auth         = Authentication strength (HMAC+challenge/UUID)")
    print(f"    Tiers: {G}S(85+)=Hardened{N}  A(70-84)=Strong  {Y}B(50-69)=Moderate{N}  {R}C(<50)=Basic{N}")
    print()

# ── Verdict ─────────────────────────────────────────────────────────────
def best(field, lower_is_better=False):
    cands = [(k, val(k, field)) for k in proxy_keys if val(k, field) > 0]
    if not cands:
        return None, 0
    cands.sort(key=lambda x: x[1], reverse=not lower_is_better)
    return cands[0]

def efficiency(key):
    dl = val(key, "download_mbps")
    mem = val(key, "memory_load_kb")
    return dl / (mem / 1024) if mem else 0

def cpu_efficiency(key):
    dl = val(key, "concurrent_mbps")
    cpu = val(key, "cpu_avg_pct")
    return dl / cpu if cpu > 0 else 0

def compute_scores():
    raw = {}
    for k in proxy_keys:
        raw[k] = {
            "download_mbps": val(k, "download_mbps"),
            "upload_mbps": val(k, "upload_mbps"),
            "latency_ms": val(k, "latency_ms"),
            "handshake_ms": val(k, "handshake_ms"),
            "concurrent_mbps": val(k, "concurrent_mbps"),
            "cpu_avg_pct": val(k, "cpu_avg_pct"),
            "memory_idle_kb": val(k, "memory_idle_kb"),
            "tput_per_mb": efficiency(k),
            "security_score": float(compute_security_score(k)),
        }

    higher_better = {"download_mbps", "upload_mbps", "concurrent_mbps", "tput_per_mb", "security_score"}
    lower_better = {"latency_ms", "handshake_ms", "cpu_avg_pct", "memory_idle_kb"}

    norm = {}
    for metric in list(higher_better) + list(lower_better):
        values = [raw[k][metric] for k in proxy_keys if raw[k][metric] > 0]
        if not values:
            for k in proxy_keys:
                norm.setdefault(k, {})[metric] = 0
            continue
        max_v = max(values)
        min_v = min(values)
        for k in proxy_keys:
            v = raw[k][metric]
            if v <= 0:
                norm.setdefault(k, {})[metric] = 0
            elif metric in higher_better:
                norm.setdefault(k, {})[metric] = v / max_v
            else:
                norm.setdefault(k, {})[metric] = min_v / v

    results = {}
    for profile_name, weights in profiles.items():
        scores = {}
        for k in proxy_keys:
            total = sum(weights[m] * norm[k][m] for m in weights)
            scores[k] = round(total, 1)
        results[profile_name] = scores
    return results

bdk, bdv = best("download_mbps")
buk, buv = best("upload_mbps")
blk, blv = best("latency_ms", lower_is_better=True)
bhk, bhv = best("handshake_ms", lower_is_better=True)
bck, bcv = best("concurrent_mbps")
bpk, bpv = best("cpu_avg_pct", lower_is_better=True)
bmk, bmv = best("memory_idle_kb", lower_is_better=True)
eff = sorted(
    [(k, efficiency(k)) for k in proxy_keys if efficiency(k) > 0],
    key=lambda x: -x[1],
)
bek = eff[0][0] if eff else None
cpu_eff = sorted(
    [(k, cpu_efficiency(k)) for k in proxy_keys if cpu_efficiency(k) > 0],
    key=lambda x: -x[1],
)
bcek = cpu_eff[0][0] if cpu_eff else None

# Best security
sec_ranked = sorted(
    [(k, compute_security_score(k)) for k in proxy_keys],
    key=lambda x: -x[1],
)
bsk = sec_ranked[0][0] if sec_ranked else None
bsv = sec_ranked[0][1] if sec_ranked else 0

# Best security/speed trade-off (security_score * download_mbps)
sec_speed = sorted(
    [(k, compute_security_score(k) * val(k, "download_mbps")) for k in proxy_keys if val(k, "download_mbps") > 0],
    key=lambda x: -x[1],
)
bssk = sec_speed[0][0] if sec_speed else None

print()
print(f"  {C}{B}Verdict{N}")
print(f"  {'\u2500' * 60}")
if bdk:
    print(f"  {G}\u25A0{N} Fastest download     {B}{proxy_names[bdk]}{N}  ({fmt(bdv)} Mbps)")
if buk:
    print(f"  {G}\u25A0{N} Fastest upload       {B}{proxy_names[buk]}{N}  ({fmt(buv)} Mbps)")
if blk:
    print(f"  {G}\u25A0{N} Lowest latency       {B}{proxy_names[blk]}{N}  ({fmt(blv)} ms)")
if bhk:
    print(f"  {G}\u25A0{N} Fastest handshake    {B}{proxy_names[bhk]}{N}  ({fmt(bhv)} ms)")
if bck:
    print(f"  {G}\u25A0{N} Best concurrency     {B}{proxy_names[bck]}{N}  ({fmt(bcv)} Mbps)")
if bpk:
    print(f"  {G}\u25A0{N} Lowest CPU           {B}{proxy_names[bpk]}{N}  ({fmt(bpv)}%)")
if bmk:
    print(f"  {G}\u25A0{N} Lowest memory        {B}{proxy_names[bmk]}{N}  ({fmt(bmv)} KB idle)")
if bek:
    print(f"  {Y}\u2605{N} Best cost-effective  {B}{proxy_names[bek]}{N}  ({fmt(eff[0][1])} Mbps/MB RAM)")
if bcek:
    print(f"  {Y}\u2605{N} Best CPU-efficient   {B}{proxy_names[bcek]}{N}  ({fmt(cpu_eff[0][1])} Mbps/%CPU)")
if bsk:
    tier = security_tier(bsv)
    print(f"  {Y}\u2605{N} Most secure          {B}{proxy_names[bsk]}{N}  ({bsv}/100, Tier {tier}: {TIER_NAMES[tier]})")
if bssk:
    ss_sc = compute_security_score(bssk)
    ss_dl = val(bssk, "download_mbps")
    print(f"  {Y}\u2605{N} Best security/speed  {B}{proxy_names[bssk]}{N}  (Sec:{ss_sc} + {fmt(ss_dl)} Mbps)")

# Head-to-head comparisons (Prisma vs Xray, Prisma vs sing-box, sing-box vs Xray)
xray_keys = [k for k in proxy_keys if k.startswith("xray-")]
prisma_keys = [k for k in proxy_keys if k.startswith("prisma-")]
singbox_keys = [k for k in proxy_keys if k.startswith("singbox-")]

def head_to_head(name_a, keys_a, name_b, keys_b):
    if not keys_a or not keys_b:
        return
    best_a_dl = max((val(k, "download_mbps"), k) for k in keys_a)
    best_b_dl = max((val(k, "download_mbps"), k) for k in keys_b)
    adl, ak = best_a_dl
    bdl, bk = best_b_dl
    if adl > 0 and bdl > 0:
        dl_ratio = adl / bdl
        print(f"  {'\u2500' * 60}")
        if dl_ratio >= 1:
            print(f"  {G}\u25A0{N} {proxy_names[ak]} is {B}{dl_ratio:.1f}x{N} faster than {proxy_names[bk]}")
        else:
            print(f"  {proxy_names[bk]} is {B}{1/dl_ratio:.1f}x{N} faster than {proxy_names[ak]}")

        amem = val(ak, "memory_idle_kb")
        bmem = val(bk, "memory_idle_kb")
        if amem > 0 and bmem > 0:
            mem_ratio = bmem / amem
            if mem_ratio >= 1:
                print(f"  {G}\u25A0{N} {name_a} uses {B}{mem_ratio:.1f}x{N} less memory than {name_b}")
            else:
                print(f"  {name_b} uses {B}{1/mem_ratio:.1f}x{N} less memory than {name_a}")

head_to_head("Prisma", prisma_keys, "Xray", xray_keys)
head_to_head("Prisma", prisma_keys, "sing-box", singbox_keys)
head_to_head("sing-box", singbox_keys, "Xray", xray_keys)
print()

# ── Use-Case Scores ───────────────────────────────────────────────────
if proxy_keys:
    scores = compute_scores()
    profile_names = list(profiles.keys())

    # Sort columns by average score across all profiles (best first)
    avg_scores = {k: sum(scores[p][k] for p in profile_names) / len(profile_names) for k in proxy_keys}
    sorted_keys = sorted(proxy_keys, key=lambda k: avg_scores[k], reverse=True)

    sc_col = max(len(proxy_names[k]) for k in sorted_keys) + 2
    sc_bar = "\u2500" * (22 + sc_col * len(sorted_keys))

    print(f"  {C}{B}Use-Case Scores (weighted 0\u2013100){N}")
    print(f"  {sc_bar}")
    sc_hdr = "".join(proxy_names[k].rjust(sc_col) for k in sorted_keys)
    print(f"  {'':22}{sc_hdr}")
    print(f"  {sc_bar}")

    for pname in profile_names:
        row_scores = scores[pname]
        best_k = max(sorted_keys, key=lambda k: row_scores[k])
        parts = []
        for k in sorted_keys:
            s = f"{row_scores[k]:.1f}"
            if k == best_k:
                pad = sc_col - len(s) - 2  # 2 = star + space
                parts.append(" " * max(pad, 0) + f"{Y}\u2605 {s}{N}")
            else:
                parts.append(s.rjust(sc_col))
        print(f"  {pname:22}{''.join(parts)}")

    print(f"  {sc_bar}")
    print()

# ── Markdown file ───────────────────────────────────────────────────────
md = []
md.append(f"## Benchmark Results ({DATE})")
md.append("")
md.append(f"**Test:** {TEST_MB}MB payload, {CONCURRENCY}x concurrent streams, loopback")
md.append(f"**Method:** MAD-filtered median (throughput, 3 payload sizes), pipeline-filtered (latency/handshake), sustained sampling (CPU/memory)")
md.append("")

hdr = "| Metric |" + "".join(f" {n} |" for _, n in present)
sep = "|--------|" + "".join(f" {'---':>{len(n)}} |" for _, n in present)
md.append(hdr)
md.append(sep)

for field, label in fields:
    row = f"| {label} |"
    for key, name in present:
        v = val(key, field)
        if field in skip_bl and key == "baseline":
            s = "\u2014"
        elif v == 0:
            s = "-"
        else:
            cv_str = fmt_cv(key, field)
            s = f"{fmt(v)}{cv_str}"
        row += f" {s:>{len(name)}} |"
    md.append(row)

# Security Score row in metrics table
sec_row = "| Security Score |"
for key, name in present:
    if key == "baseline":
        s = "\u2014"
    else:
        sc = compute_security_score(key)
        tier = security_tier(sc)
        s = f"{sc} ({tier})"
    sec_row += f" {s:>{len(name)}} |"
md.append(sec_row)

md.append("")

# Security Properties Table
if proxy_keys:
    md.append("### Security Properties")
    md.append("")
    sec_hdr = "| Dimension |" + "".join(f" {proxy_names[k]} |" for k in proxy_keys)
    sec_sep = "|-----------|" + "".join(f" {'---':>{len(proxy_names[k])}} |" for k in proxy_keys)
    md.append(sec_hdr)
    md.append(sec_sep)

    for dim, dim_label in SECURITY_LABELS.items():
        row = f"| {dim_label} |"
        for k in proxy_keys:
            dims = SECURITY_DB.get(k, {})
            v = dims.get(dim, 0)
            row += f" {f'{v}/10':>{len(proxy_names[k])}} |"
        md.append(row)

    score_row = "| **Score (Tier)** |"
    for k in proxy_keys:
        sc = compute_security_score(k)
        tier = security_tier(sc)
        s = f"**{sc} ({tier})**"
        score_row += f" {s:>{len(proxy_names[k])}} |"
    md.append(score_row)
    md.append("")
    md.append("**Tiers:** S (85+) Hardened, A (70-84) Strong, B (50-69) Moderate, C (<50) Basic")
    md.append("")

md.append("### Verdict")
md.append("")
if bdk:
    md.append(f"- **Fastest download:** {proxy_names[bdk]} ({fmt(bdv)} Mbps)")
if buk:
    md.append(f"- **Fastest upload:** {proxy_names[buk]} ({fmt(buv)} Mbps)")
if blk:
    md.append(f"- **Lowest latency:** {proxy_names[blk]} ({fmt(blv)} ms)")
if bhk:
    md.append(f"- **Fastest handshake:** {proxy_names[bhk]} ({fmt(bhv)} ms)")
if bck:
    md.append(f"- **Best concurrency:** {proxy_names[bck]} ({fmt(bcv)} Mbps)")
if bpk:
    md.append(f"- **Lowest CPU:** {proxy_names[bpk]} ({fmt(bpv)}%)")
if bmk:
    md.append(f"- **Lowest memory:** {proxy_names[bmk]} ({fmt(bmv)} KB idle)")
if bek:
    md.append(f"- **Best cost-effective:** {proxy_names[bek]} ({fmt(eff[0][1])} Mbps/MB RAM)")
if bcek:
    md.append(f"- **Best CPU-efficient:** {proxy_names[bcek]} ({fmt(cpu_eff[0][1])} Mbps/%CPU)")
if bsk:
    tier = security_tier(bsv)
    md.append(f"- **Most secure:** {proxy_names[bsk]} ({bsv}/100, Tier {tier}: {TIER_NAMES[tier]})")
if bssk:
    ss_sc = compute_security_score(bssk)
    ss_dl = val(bssk, "download_mbps")
    md.append(f"- **Best security/speed:** {proxy_names[bssk]} (Sec:{ss_sc} + {fmt(ss_dl)} Mbps)")

def md_head_to_head(name_a, keys_a, name_b, keys_b):
    if not keys_a or not keys_b:
        return
    best_a = max((val(k, "download_mbps"), k) for k in keys_a)
    best_b = max((val(k, "download_mbps"), k) for k in keys_b)
    adl, ak = best_a
    bdl, bk = best_b
    if adl > 0 and bdl > 0:
        ratio = adl / bdl
        if ratio >= 1:
            md.append(f"{proxy_names[ak]} is **{ratio:.1f}x** faster than {proxy_names[bk]}.")
        else:
            md.append(f"{proxy_names[bk]} is **{1/ratio:.1f}x** faster than {proxy_names[ak]}.")

md.append("")
md_head_to_head("Prisma", prisma_keys, "Xray", xray_keys)
md_head_to_head("Prisma", prisma_keys, "sing-box", singbox_keys)
md_head_to_head("sing-box", singbox_keys, "Xray", xray_keys)

if proxy_keys:
    md.append("")
    md.append("### Use-Case Scores (weighted 0\u2013100)")
    md.append("")
    sc_hdr = "| Use Case |" + "".join(f" {proxy_names[k]} |" for k in sorted_keys)
    sc_sep = "|----------|" + "".join(f" {'---':>{len(proxy_names[k])}} |" for k in sorted_keys)
    md.append(sc_hdr)
    md.append(sc_sep)
    for pname in profile_names:
        row_scores = scores[pname]
        best_k = max(sorted_keys, key=lambda k: row_scores[k])
        row = f"| {pname} |"
        for k in sorted_keys:
            s = f"{row_scores[k]:.1f}"
            if k == best_k:
                s = f"**{s}** \u2605"
            row += f" {s:>{len(proxy_names[k])}} |"
        md.append(row)
    md.append("")
    for pname in profile_names:
        row_scores = scores[pname]
        best_k = max(sorted_keys, key=lambda k: row_scores[k])
        md.append(f"- **{pname}:** {proxy_names[best_k]} ({row_scores[best_k]:.1f}/100)")

md.append("")
md.append("Generated by PrismaVeil benchmark suite.")

with open(os.path.join(RESULTS, "summary.md"), "w") as f:
    f.write("\n".join(md) + "\n")
PYEOF

    RESULTS_DIR="$RESULTS_DIR" \
    CONCURRENCY="$CONCURRENCY" \
    TEST_SIZE_MB="$TEST_SIZE_MB" \
    BENCH_DATE="$(date -u +%Y-%m-%d)" \
    python3 "$RESULTS_DIR/_summary.py"

    rm -f "$RESULTS_DIR/_summary.py"
    log "Results written to $RESULTS_DIR/summary.md"
}

# ---------------------------------------------------------------------------
# Package results (exclude test data to reduce size)
# ---------------------------------------------------------------------------

package_results() {
    log "Removing test data from results directory..."
    rm -f "$RESULTS_DIR/testdata" "$RESULTS_DIR/testdata-small" "$RESULTS_DIR/testdata-medium" "$RESULTS_DIR/testdata-large"
    rm -f "$RESULTS_DIR/ping" "$RESULTS_DIR/upload"

    # Remove log files (can be large) — keep only JSON results and summary
    # Uncomment the next line to also strip logs:
    # rm -f "$RESULTS_DIR"/*.log

    local archive="benchmark-results-$(date -u +%Y%m%d-%H%M%S).tar.gz"
    log "Packaging results into $archive ..."
    tar -czf "$archive" \
        --exclude='testdata' \
        --exclude='ping' \
        --exclude='prisma-cert.pem' \
        --exclude='prisma-key.pem' \
        -C "$(dirname "$RESULTS_DIR")" "$(basename "$RESULTS_DIR")"
    log "Archive ready: $archive ($(du -sh "$archive" | cut -f1))"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    log "PrismaVeil v4 Benchmark Suite"
    log "=============================="

    if [ ! -f "$PRISMA_BIN" ]; then
        err "Prisma binary not found at $PRISMA_BIN"
        exit 1
    fi

    generate_certs
    generate_configs
    start_test_server

    # ── Prisma scenarios ──────────────────────────────────────────
    if should_run_scenario "baseline"; then run_baseline; fi
    if should_run_scenario "prisma-quic"; then
        run_prisma_scenario "prisma-quic" \
            "$RESULTS_DIR/server-quic.toml" "$RESULTS_DIR/client-quic.toml" 11080
    fi
    if should_run_scenario "prisma-tcp"; then
        run_prisma_scenario "prisma-tcp" \
            "$RESULTS_DIR/server-tcp.toml" "$RESULTS_DIR/client-tcp.toml" 11082
    fi
    if should_run_scenario "prisma-shaped"; then
        run_prisma_scenario "prisma-shaped" \
            "$RESULTS_DIR/server-shaped.toml" "$RESULTS_DIR/client-shaped.toml" 11081
    fi
    if should_run_scenario "prisma-quic-aes"; then
        run_prisma_scenario "prisma-quic-aes" \
            "$RESULTS_DIR/server-quic-aes.toml" "$RESULTS_DIR/client-quic-aes.toml" 11083
    fi
    if should_run_scenario "prisma-tonly"; then
        run_prisma_scenario "prisma-tonly" \
            "$RESULTS_DIR/server-transport-only.toml" "$RESULTS_DIR/client-transport-only.toml" 11084
    fi
    if should_run_scenario "prisma-ws"; then
        run_prisma_scenario "prisma-ws" \
            "$RESULTS_DIR/server-ws.toml" "$RESULTS_DIR/client-ws.toml" 11086
    fi
    if should_run_scenario "prisma-bucket"; then
        run_prisma_scenario "prisma-bucket" \
            "$RESULTS_DIR/server-bucket.toml" "$RESULTS_DIR/client-bucket.toml" 11087
    fi

    # ── Xray scenarios ────────────────────────────────────────────
    if should_run_scenario "xray-vless-tls"; then
        run_xray_scenario "xray-vless-tls" \
            "$RESULTS_DIR/xray-vless-tls-server.json" \
            "$RESULTS_DIR/xray-vless-tls-client.json" 28443 21080
    fi

    if should_run_scenario "xray-vless-xtls"; then
        run_xray_scenario "xray-vless-xtls" \
            "$RESULTS_DIR/xray-vless-xtls-server.json" \
            "$RESULTS_DIR/xray-vless-xtls-client.json" 28444 21081
    fi

    if should_run_scenario "xray-vmess-tls"; then
        run_xray_scenario "xray-vmess-tls" \
            "$RESULTS_DIR/xray-vmess-tls-server.json" \
            "$RESULTS_DIR/xray-vmess-tls-client.json" 28445 21082
    fi

    if should_run_scenario "xray-trojan-tls"; then
        run_xray_scenario "xray-trojan-tls" \
            "$RESULTS_DIR/xray-trojan-tls-server.json" \
            "$RESULTS_DIR/xray-trojan-tls-client.json" 28446 21083
    fi

    if should_run_scenario "xray-ss-aead"; then
        run_xray_scenario "xray-ss-aead" \
            "$RESULTS_DIR/xray-ss-aead-server.json" \
            "$RESULTS_DIR/xray-ss-aead-client.json" 28447 21084
    fi

    if should_run_scenario "xray-ss2022"; then
        run_xray_scenario "xray-ss2022" \
            "$RESULTS_DIR/xray-ss2022-server.json" \
            "$RESULTS_DIR/xray-ss2022-client.json" 28450 21087
    fi

    if should_run_scenario "xray-vless-ws"; then
        run_xray_scenario "xray-vless-ws" \
            "$RESULTS_DIR/xray-vless-ws-server.json" \
            "$RESULTS_DIR/xray-vless-ws-client.json" 28448 21085
    fi

    if should_run_scenario "xray-vless-grpc"; then
        run_xray_scenario "xray-vless-grpc" \
            "$RESULTS_DIR/xray-vless-grpc-server.json" \
            "$RESULTS_DIR/xray-vless-grpc-client.json" 28449 21086
    fi

    # ── sing-box scenarios ────────────────────────────────────────
    if should_run_scenario "singbox-vless-tls"; then
        run_singbox_scenario "singbox-vless-tls" \
            "$RESULTS_DIR/singbox-vless-tls-server.json" \
            "$RESULTS_DIR/singbox-vless-tls-client.json" 38443 31080
    fi

    if should_run_scenario "singbox-vmess-tls"; then
        run_singbox_scenario "singbox-vmess-tls" \
            "$RESULTS_DIR/singbox-vmess-tls-server.json" \
            "$RESULTS_DIR/singbox-vmess-tls-client.json" 38444 31081
    fi

    if should_run_scenario "singbox-trojan-tls"; then
        run_singbox_scenario "singbox-trojan-tls" \
            "$RESULTS_DIR/singbox-trojan-tls-server.json" \
            "$RESULTS_DIR/singbox-trojan-tls-client.json" 38445 31082
    fi

    if should_run_scenario "singbox-ss-aead"; then
        run_singbox_scenario "singbox-ss-aead" \
            "$RESULTS_DIR/singbox-ss-aead-server.json" \
            "$RESULTS_DIR/singbox-ss-aead-client.json" 38446 31083
    fi

    if should_run_scenario "singbox-ss2022"; then
        run_singbox_scenario "singbox-ss2022" \
            "$RESULTS_DIR/singbox-ss2022-server.json" \
            "$RESULTS_DIR/singbox-ss2022-client.json" 38447 31084
    fi

    if should_run_scenario "singbox-vless-ws"; then
        run_singbox_scenario "singbox-vless-ws" \
            "$RESULTS_DIR/singbox-vless-ws-server.json" \
            "$RESULTS_DIR/singbox-vless-ws-client.json" 38448 31085
    fi

    if should_run_scenario "singbox-hysteria2"; then
        run_singbox_scenario "singbox-hysteria2" \
            "$RESULTS_DIR/singbox-hysteria2-server.json" \
            "$RESULTS_DIR/singbox-hysteria2-client.json" 38449 31086 udp
    fi

    if should_run_scenario "singbox-tuic"; then
        run_singbox_scenario "singbox-tuic" \
            "$RESULTS_DIR/singbox-tuic-server.json" \
            "$RESULTS_DIR/singbox-tuic-client.json" 38450 31087 udp
    fi

    if should_run_scenario "singbox-vmess-ws"; then
        run_singbox_scenario "singbox-vmess-ws" \
            "$RESULTS_DIR/singbox-vmess-ws-server.json" \
            "$RESULTS_DIR/singbox-vmess-ws-client.json" 38451 31088
    fi

    if should_run_scenario "singbox-trojan-ws"; then
        run_singbox_scenario "singbox-trojan-ws" \
            "$RESULTS_DIR/singbox-trojan-ws-server.json" \
            "$RESULTS_DIR/singbox-trojan-ws-client.json" 38452 31089
    fi

    if should_run_scenario "singbox-ss2022-relay"; then
        run_singbox_scenario "singbox-ss2022-relay" \
            "$RESULTS_DIR/singbox-ss2022-relay-server.json" \
            "$RESULTS_DIR/singbox-ss2022-relay-client.json" 38453 31090
    fi

    if should_run_scenario "singbox-vless-grpc"; then
        run_singbox_scenario "singbox-vless-grpc" \
            "$RESULTS_DIR/singbox-vless-grpc-server.json" \
            "$RESULTS_DIR/singbox-vless-grpc-client.json" 38454 31091
    fi

    if should_run_scenario "singbox-vless-http2"; then
        run_singbox_scenario "singbox-vless-http2" \
            "$RESULTS_DIR/singbox-vless-http2-server.json" \
            "$RESULTS_DIR/singbox-vless-http2-client.json" 38455 31092
    fi

    # ── Check for silent failures ─────────────────────────────────
    local fail_count=0
    for result_file in "$RESULTS_DIR"/*.json; do
        local fname
        fname=$(basename "$result_file" .json)
        [ "$fname" = "baseline" ] && continue
        [ "$fname" = "summary" ] && continue
        [[ "$fname" == *-server ]] && continue
        [[ "$fname" == *-client ]] && continue
        if python3 -c "
import json, sys
d = json.load(open('$result_file'))
if d.get('download_mbps', 0) == 0:
    print(f'  WARNING: {\"$fname\"} reported 0 Mbps download — possible silent failure')
    sys.exit(1)
" 2>/dev/null; then
            : # ok
        else
            warn "Scenario '$fname' may have silently failed (0 Mbps download)"
            fail_count=$((fail_count + 1))
        fi
    done
    if [ "$fail_count" -gt 0 ]; then
        warn "$fail_count scenario(s) reported 0 Mbps — check logs above for errors"
    fi

    # ── Results ───────────────────────────────────────────────────
    generate_summary
    package_results

    log "Benchmark complete. Results in $RESULTS_DIR/"
}

main "$@"
