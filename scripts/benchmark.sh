#!/usr/bin/env bash
# Benchmark script: PrismaVeil v4 vs Xray-core
# Measures throughput and memory via loopback SOCKS5 proxy.
set -euo pipefail

RESULTS_DIR="benchmark-results"
mkdir -p "$RESULTS_DIR"
RESULTS_DIR="$(cd "$RESULTS_DIR" && pwd)"

PRISMA_BIN="${PRISMA_BIN:-./prisma}"
XRAY_BIN="${XRAY_BIN:-./xray/xray}"
HTTP_PORT=18888
TEST_SIZE_MB=5120
PIDS=()

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

log() { echo -e "${GREEN}[BENCH]${NC} $*"; }
err() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

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

    local CLIENT_ID AUTH_SECRET XRAY_UUID
    CLIENT_ID=$(uuidgen 2>/dev/null || python3 -c "import uuid; print(uuid.uuid4())")
    AUTH_SECRET=$(openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | xxd -p -c 64)
    XRAY_UUID=$(uuidgen 2>/dev/null || python3 -c "import uuid; print(uuid.uuid4())")

    # --- Prisma QUIC v2 ---------------------------------------------------
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
fingerprint = "chrome"
quic_version = "v2"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Prisma QUIC v2 + traffic shaping ---------------------------------
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
padding_mode = "bucket"
bucket_sizes = [128, 256, 512, 1024, 2048, 4096, 8192, 16384]
EOF

    cat > "$RESULTS_DIR/client-shaped.toml" <<EOF
socks5_listen_addr = "127.0.0.1:11081"
server_addr = "127.0.0.1:18444"
transport = "quic"
skip_cert_verify = true
protocol_version = "v4"
fingerprint = "chrome"
quic_version = "v2"

[identity]
client_id = "$CLIENT_ID"
auth_secret = "$AUTH_SECRET"
EOF

    # --- Xray VLESS + TLS -------------------------------------------------
    cat > "$RESULTS_DIR/xray-server.json" <<XEOF
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

    cat > "$RESULTS_DIR/xray-client.json" <<XEOF
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
}

start_test_server() {
    log "Creating ${TEST_SIZE_MB}MB test payload..."
    dd if=/dev/urandom of="$RESULTS_DIR/testdata" bs=1M count=$TEST_SIZE_MB 2>/dev/null

    log "Starting HTTP server on port $HTTP_PORT..."
    python3 -m http.server $HTTP_PORT -d "$RESULTS_DIR" > /dev/null 2>&1 &
    PIDS+=($!)
    wait_for_port $HTTP_PORT
}

# ---------------------------------------------------------------------------
# Benchmarks
# ---------------------------------------------------------------------------

# Download the test payload through a SOCKS5 proxy and return Mbps.
measure_download() {
    local socks_port=$1
    local speed
    speed=$(curl -o /dev/null -s -w '%{speed_download}' \
        --connect-timeout 10 --max-time 600 \
        --socks5-hostname "127.0.0.1:$socks_port" \
        "http://127.0.0.1:$HTTP_PORT/testdata" 2>/dev/null || echo "0")
    python3 -c "print(f'{float($speed) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0"
}

# Direct download (no proxy) for baseline reference.
run_baseline() {
    log "=== Baseline (no proxy) ==="
    local speed
    speed=$(curl -o /dev/null -s -w '%{speed_download}' \
        --connect-timeout 10 --max-time 600 \
        "http://127.0.0.1:$HTTP_PORT/testdata" 2>/dev/null || echo "0")
    local dl_mbps
    dl_mbps=$(python3 -c "print(f'{float($speed) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")
    log "  Download: ${dl_mbps} Mbps"
    echo "{\"label\":\"baseline\",\"download_mbps\":$dl_mbps,\"memory_kb\":0}" \
        > "$RESULTS_DIR/baseline.json"
}

run_prisma_scenario() {
    local label=$1 server_cfg=$2 client_cfg=$3 socks_port=$4

    log "=== $label ==="

    "$PRISMA_BIN" server -c "$server_cfg" \
        > "$RESULTS_DIR/${label}-server.log" 2>&1 &
    local srv=$!; PIDS+=($srv)
    sleep 2

    "$PRISMA_BIN" client -c "$client_cfg" \
        > "$RESULTS_DIR/${label}-client.log" 2>&1 &
    local cli=$!; PIDS+=($cli)
    wait_for_port "$socks_port"

    # Memory (idle)
    local mem_srv mem_cli mem
    mem_srv=$(get_rss_kb $srv)
    mem_cli=$(get_rss_kb $cli)
    mem=$((mem_srv + mem_cli))

    # Throughput
    log "  Measuring download throughput..."
    local dl_mbps
    dl_mbps=$(measure_download "$socks_port")
    log "  Download: ${dl_mbps} Mbps  |  Memory: ${mem} KB"

    echo "{\"label\":\"$label\",\"download_mbps\":$dl_mbps,\"memory_kb\":$mem}" \
        > "$RESULTS_DIR/${label}.json"

    kill $srv $cli 2>/dev/null || true
    wait $srv $cli 2>/dev/null || true
    sleep 1
}

run_xray_scenario() {
    if [ ! -f "$XRAY_BIN" ]; then
        log "Xray binary not found at $XRAY_BIN — skipping"
        echo '{"label":"xray-vless","download_mbps":0,"memory_kb":0}' \
            > "$RESULTS_DIR/xray-vless.json"
        return
    fi

    log "=== Xray VLESS+TLS ==="

    "$XRAY_BIN" run -c "$RESULTS_DIR/xray-server.json" \
        > "$RESULTS_DIR/xray-server.log" 2>&1 &
    local srv=$!; PIDS+=($srv)
    if ! wait_for_port 28443 15; then
        err "Xray server failed to start. Log:"
        tail -20 "$RESULTS_DIR/xray-server.log" >&2 || true
        kill $srv 2>/dev/null || true
        echo '{"label":"xray-vless","download_mbps":0,"memory_kb":0}' \
            > "$RESULTS_DIR/xray-vless.json"
        return
    fi

    "$XRAY_BIN" run -c "$RESULTS_DIR/xray-client.json" \
        > "$RESULTS_DIR/xray-client.log" 2>&1 &
    local cli=$!; PIDS+=($cli)
    if ! wait_for_port 21080 15; then
        err "Xray client failed to start. Log:"
        tail -20 "$RESULTS_DIR/xray-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        echo '{"label":"xray-vless","download_mbps":0,"memory_kb":0}' \
            > "$RESULTS_DIR/xray-vless.json"
        return
    fi

    local mem_srv mem_cli mem
    mem_srv=$(get_rss_kb $srv)
    mem_cli=$(get_rss_kb $cli)
    mem=$((mem_srv + mem_cli))

    log "  Measuring download throughput..."
    local dl_mbps
    dl_mbps=$(measure_download 21080)
    log "  Download: ${dl_mbps} Mbps  |  Memory: ${mem} KB"

    echo "{\"label\":\"xray-vless\",\"download_mbps\":$dl_mbps,\"memory_kb\":$mem}" \
        > "$RESULTS_DIR/xray-vless.json"

    kill $srv $cli 2>/dev/null || true
    wait $srv $cli 2>/dev/null || true
    sleep 1
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

read_field() {
    local file="$RESULTS_DIR/$1.json" field=$2
    if [ -f "$file" ]; then
        python3 -c "import json,sys; d=json.load(open('$file')); v=d.get('$field',0); print('-' if v==0 else v)" 2>/dev/null || echo "-"
    else
        echo "-"
    fi
}

generate_summary() {
    log "Generating summary..."
    local date_str
    date_str=$(date -u +"%Y-%m-%d")

    local bl_dl pq_dl ps_dl xr_dl pq_mem ps_mem xr_mem
    bl_dl=$(read_field baseline download_mbps)
    pq_dl=$(read_field prisma-quic download_mbps)
    ps_dl=$(read_field prisma-shaped download_mbps)
    xr_dl=$(read_field xray-vless download_mbps)
    pq_mem=$(read_field prisma-quic memory_kb)
    ps_mem=$(read_field prisma-shaped memory_kb)
    xr_mem=$(read_field xray-vless memory_kb)

    cat > "$RESULTS_DIR/summary.md" <<EOF
## Benchmark Results ($date_str)

**Test:** ${TEST_SIZE_MB}MB transfer over loopback

| Metric | Baseline | Prisma QUIC v2 | Prisma (shaped) | Xray VLESS+TLS |
|--------|----------|----------------|-----------------|----------------|
| Download (Mbps) | $bl_dl | $pq_dl | $ps_dl | $xr_dl |
| Memory (KB) | — | $pq_mem | $ps_mem | $xr_mem |

Generated by PrismaVeil benchmark suite.
EOF

    log "Results written to $RESULTS_DIR/summary.md"
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

    run_baseline
    run_prisma_scenario "prisma-quic" \
        "$RESULTS_DIR/server-quic.toml" "$RESULTS_DIR/client-quic.toml" 11080
    run_prisma_scenario "prisma-shaped" \
        "$RESULTS_DIR/server-shaped.toml" "$RESULTS_DIR/client-shaped.toml" 11081
    run_xray_scenario

    generate_summary

    log "Benchmark complete. Results in $RESULTS_DIR/"
}

main "$@"
