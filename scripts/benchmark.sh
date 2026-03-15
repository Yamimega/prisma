#!/usr/bin/env bash
# Benchmark script: PrismaVeil v4 vs Xray-core
# Measures throughput, latency, concurrency, and memory via loopback SOCKS5 proxy.
set -euo pipefail

RESULTS_DIR="benchmark-results"
mkdir -p "$RESULTS_DIR"
RESULTS_DIR="$(cd "$RESULTS_DIR" && pwd)"

PRISMA_BIN="${PRISMA_BIN:-./prisma}"
XRAY_BIN="${XRAY_BIN:-./xray/xray}"
HTTP_PORT=18888
TEST_SIZE_MB=100
CONCURRENCY=4
PIDS=()

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
fingerprint = "chrome"
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
skip_cert_verify = true
protocol_version = "v4"
fingerprint = "chrome"

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
quic_version = "v1"

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

    # 1-byte file for latency measurement (minimize transfer time)
    echo -n "x" > "$RESULTS_DIR/ping"

    log "Starting HTTP server on port $HTTP_PORT..."
    python3 -m http.server $HTTP_PORT -d "$RESULTS_DIR" > /dev/null 2>&1 &
    PIDS+=($!)
    wait_for_port $HTTP_PORT
}

# ---------------------------------------------------------------------------
# Measurement primitives
# ---------------------------------------------------------------------------

# Single-stream download throughput (Mbps).
measure_download() {
    local socks_port=$1
    local speed
    speed=$(curl -o /dev/null -s -w '%{speed_download}' \
        --connect-timeout 10 --max-time 120 \
        --socks5-hostname "127.0.0.1:$socks_port" \
        "http://127.0.0.1:$HTTP_PORT/testdata" 2>/dev/null || echo "0")
    python3 -c "print(f'{float($speed) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0"
}

# Time-to-first-byte latency in ms (fetches a 1-byte file).
measure_latency() {
    local socks_port=$1
    local total=0 count=5
    for _ in $(seq 1 $count); do
        local ttfb
        ttfb=$(curl -o /dev/null -s -w '%{time_starttransfer}' \
            --connect-timeout 5 --max-time 10 \
            --socks5-hostname "127.0.0.1:$socks_port" \
            "http://127.0.0.1:$HTTP_PORT/ping" 2>/dev/null || echo "0")
        total=$(python3 -c "print(f'{$total + $ttfb * 1000:.1f}')" 2>/dev/null || echo "$total")
    done
    python3 -c "print(f'{$total / $count:.1f}')" 2>/dev/null || echo "0"
}

# Aggregate throughput with N parallel downloads (Mbps).
measure_concurrent() {
    local socks_port=$1 n=${2:-$CONCURRENCY}
    local tmpdir
    tmpdir=$(mktemp -d)

    for i in $(seq 1 "$n"); do
        curl -o /dev/null -s -w '%{speed_download}' \
            --connect-timeout 10 --max-time 120 \
            --socks5-hostname "127.0.0.1:$socks_port" \
            "http://127.0.0.1:$HTTP_PORT/testdata" \
            > "$tmpdir/$i" 2>/dev/null &
    done
    wait

    # Sum individual speeds → aggregate Mbps
    python3 -c "
import os, glob
total = sum(float(open(f).read().strip() or '0') for f in glob.glob('$tmpdir/*'))
print(f'{total * 8 / 1_000_000:.1f}')
" 2>/dev/null || echo "0"

    rm -rf "$tmpdir"
}

# ---------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------

# Direct download (no proxy) for baseline reference.
run_baseline() {
    log "=== Baseline (no proxy) ==="

    local speed
    speed=$(curl -o /dev/null -s -w '%{speed_download}' \
        --connect-timeout 10 --max-time 120 \
        "http://127.0.0.1:$HTTP_PORT/testdata" 2>/dev/null || echo "0")
    local dl_mbps
    dl_mbps=$(python3 -c "print(f'{float($speed) * 8 / 1_000_000:.1f}')" 2>/dev/null || echo "0")

    local ttfb
    ttfb=$(curl -o /dev/null -s -w '%{time_starttransfer}' \
        --connect-timeout 5 --max-time 10 \
        "http://127.0.0.1:$HTTP_PORT/ping" 2>/dev/null || echo "0")
    local latency_ms
    latency_ms=$(python3 -c "print(f'{$ttfb * 1000:.1f}')" 2>/dev/null || echo "0")

    log "  Download: ${dl_mbps} Mbps  |  Latency: ${latency_ms} ms"

    python3 -c "
import json
json.dump({
    'label': 'baseline',
    'download_mbps': $dl_mbps,
    'latency_ms': $latency_ms,
    'concurrent_mbps': 0,
    'memory_idle_kb': 0,
    'memory_load_kb': 0
}, open('$RESULTS_DIR/baseline.json', 'w'))
"
}

write_empty_result() {
    local label=$1
    python3 -c "
import json
json.dump({'label':'$label','download_mbps':0,'latency_ms':0,
           'concurrent_mbps':0,'memory_idle_kb':0,'memory_load_kb':0},
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

    # Memory (idle)
    local mem_idle_srv mem_idle_cli mem_idle
    mem_idle_srv=$(get_rss_kb $srv)
    mem_idle_cli=$(get_rss_kb $cli)
    mem_idle=$((mem_idle_srv + mem_idle_cli))

    # Latency (TTFB, average of 5 requests)
    log "  Measuring latency..."
    local latency_ms
    latency_ms=$(measure_latency "$socks_port")

    # Single-stream throughput
    log "  Measuring single-stream throughput..."
    local dl_mbps
    dl_mbps=$(measure_download "$socks_port")

    # Memory (under load — measure during concurrent test)
    log "  Measuring concurrent throughput (${CONCURRENCY}x parallel)..."
    local concurrent_mbps
    concurrent_mbps=$(measure_concurrent "$socks_port")

    # Memory under load (sample right after concurrent finishes while RSS is high)
    local mem_load_srv mem_load_cli mem_load
    mem_load_srv=$(get_rss_kb $srv)
    mem_load_cli=$(get_rss_kb $cli)
    mem_load=$((mem_load_srv + mem_load_cli))

    log "  Download: ${dl_mbps} Mbps  |  ${CONCURRENCY}x: ${concurrent_mbps} Mbps"
    log "  Latency: ${latency_ms} ms  |  Mem idle: ${mem_idle} KB  |  Mem load: ${mem_load} KB"

    python3 -c "
import json
json.dump({
    'label': '$label',
    'download_mbps': $dl_mbps,
    'latency_ms': $latency_ms,
    'concurrent_mbps': $concurrent_mbps,
    'memory_idle_kb': $mem_idle,
    'memory_load_kb': $mem_load
}, open('$RESULTS_DIR/${label}.json', 'w'))
"

    kill $srv $cli 2>/dev/null || true
    wait $srv $cli 2>/dev/null || true
    sleep 1
}

run_xray_scenario() {
    if [ ! -f "$XRAY_BIN" ]; then
        log "Xray binary not found at $XRAY_BIN — skipping"
        python3 -c "
import json
json.dump({'label':'xray-vless','download_mbps':0,'latency_ms':0,
           'concurrent_mbps':0,'memory_idle_kb':0,'memory_load_kb':0},
          open('$RESULTS_DIR/xray-vless.json','w'))
"
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
        python3 -c "
import json
json.dump({'label':'xray-vless','download_mbps':0,'latency_ms':0,
           'concurrent_mbps':0,'memory_idle_kb':0,'memory_load_kb':0},
          open('$RESULTS_DIR/xray-vless.json','w'))
"
        return
    fi

    "$XRAY_BIN" run -c "$RESULTS_DIR/xray-client.json" \
        > "$RESULTS_DIR/xray-client.log" 2>&1 &
    local cli=$!; PIDS+=($cli)
    if ! wait_for_port 21080 15; then
        err "Xray client failed to start. Log:"
        tail -20 "$RESULTS_DIR/xray-client.log" >&2 || true
        kill $srv $cli 2>/dev/null || true
        python3 -c "
import json
json.dump({'label':'xray-vless','download_mbps':0,'latency_ms':0,
           'concurrent_mbps':0,'memory_idle_kb':0,'memory_load_kb':0},
          open('$RESULTS_DIR/xray-vless.json','w'))
"
        return
    fi

    local mem_idle_srv mem_idle_cli mem_idle
    mem_idle_srv=$(get_rss_kb $srv)
    mem_idle_cli=$(get_rss_kb $cli)
    mem_idle=$((mem_idle_srv + mem_idle_cli))

    log "  Measuring latency..."
    local latency_ms
    latency_ms=$(measure_latency 21080)

    log "  Measuring single-stream throughput..."
    local dl_mbps
    dl_mbps=$(measure_download 21080)

    log "  Measuring concurrent throughput (${CONCURRENCY}x parallel)..."
    local concurrent_mbps
    concurrent_mbps=$(measure_concurrent 21080)

    local mem_load_srv mem_load_cli mem_load
    mem_load_srv=$(get_rss_kb $srv)
    mem_load_cli=$(get_rss_kb $cli)
    mem_load=$((mem_load_srv + mem_load_cli))

    log "  Download: ${dl_mbps} Mbps  |  ${CONCURRENCY}x: ${concurrent_mbps} Mbps"
    log "  Latency: ${latency_ms} ms  |  Mem idle: ${mem_idle} KB  |  Mem load: ${mem_load} KB"

    python3 -c "
import json
json.dump({
    'label': 'xray-vless',
    'download_mbps': $dl_mbps,
    'latency_ms': $latency_ms,
    'concurrent_mbps': $concurrent_mbps,
    'memory_idle_kb': $mem_idle,
    'memory_load_kb': $mem_load
}, open('$RESULTS_DIR/xray-vless.json', 'w'))
"

    kill $srv $cli 2>/dev/null || true
    wait $srv $cli 2>/dev/null || true
    sleep 1
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
    ("baseline",      "Baseline"),
    ("prisma-quic",   "Prisma QUIC"),
    ("prisma-tcp",    "Prisma TCP+TLS"),
    ("prisma-shaped", "Prisma (shaped)"),
    ("xray-vless",    "Xray VLESS+TLS"),
]

fields = [
    ("download_mbps",   "Download (Mbps)"),
    ("latency_ms",      "Latency TTFB (ms)"),
    ("concurrent_mbps", f"{CONCURRENCY}x Concurrent (Mbps)"),
    ("memory_idle_kb",  "Memory idle (KB)"),
    ("memory_load_kb",  "Memory load (KB)"),
]

# Load results
data = {}
for key, _ in scenarios:
    path = os.path.join(RESULTS, f"{key}.json")
    try:
        data[key] = json.load(open(path))
    except Exception:
        data[key] = {}

def val(key, field):
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

# ── Colors ──────────────────────────────────────────────────────────────
G = "\033[0;32m"
C = "\033[0;36m"
Y = "\033[0;33m"
B = "\033[1m"
N = "\033[0m"

col_w = 18
label_w = 24
skip_bl = {"concurrent_mbps", "memory_idle_kb", "memory_load_kb"}

# ── Terminal table ──────────────────────────────────────────────────────
bar = "\u2500" * (label_w + col_w * len(scenarios))
print()
print(f"  {G}{bar}{N}")
print(f"  {B}Benchmark Results \u2014 {DATE}{N}")
print(f"  {TEST_MB}MB payload \u00B7 {CONCURRENCY}x concurrent \u00B7 loopback")
print(f"  {G}{bar}{N}")
print()

hdrs = "".join(name.rjust(col_w) for _, name in scenarios)
print(f"  {'':<{label_w}}{hdrs}")
print(f"  {bar}")

for field, label in fields:
    cells = []
    for key, _ in scenarios:
        v = val(key, field)
        if field in skip_bl and key == "baseline":
            cells.append("\u2014".rjust(col_w))
        elif v == 0:
            cells.append("-".rjust(col_w))
        else:
            cells.append(fmt(v).rjust(col_w))
    print(f"  {label:<{label_w}}{''.join(cells)}")

print(f"  {bar}")

# ── Verdict ─────────────────────────────────────────────────────────────
proxy_keys = [k for k, _ in scenarios if k != "baseline"]
proxy_names = {k: n for k, n in scenarios if k != "baseline"}

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

bdk, bdv = best("download_mbps")
blk, blv = best("latency_ms", lower_is_better=True)
bck, bcv = best("concurrent_mbps")
bmk, bmv = best("memory_idle_kb", lower_is_better=True)
eff = sorted(
    [(k, efficiency(k)) for k in proxy_keys if efficiency(k) > 0],
    key=lambda x: -x[1],
)
bek = eff[0][0] if eff else None

print()
print(f"  {C}{B}Verdict{N}")
print(f"  {'\u2500' * 60}")
if bdk:
    print(f"  {G}\u25A0{N} Fastest download     {B}{proxy_names[bdk]}{N}  ({fmt(bdv)} Mbps)")
if blk:
    print(f"  {G}\u25A0{N} Lowest latency       {B}{proxy_names[blk]}{N}  ({fmt(blv)} ms)")
if bck:
    print(f"  {G}\u25A0{N} Best concurrency     {B}{proxy_names[bck]}{N}  ({fmt(bcv)} Mbps)")
if bmk:
    print(f"  {G}\u25A0{N} Lowest memory        {B}{proxy_names[bmk]}{N}  ({fmt(bmv)} KB idle)")
if bek:
    print(f"  {Y}\u2605{N} Best cost-effective  {B}{proxy_names[bek]}{N}  ({fmt(eff[0][1])} Mbps/MB RAM)")

# Prisma vs Xray head-to-head
xdl = val("xray-vless", "download_mbps")
pdl = val("prisma-quic", "download_mbps")
xmem = val("xray-vless", "memory_idle_kb")
pmem = val("prisma-quic", "memory_idle_kb")

if xdl > 0 and pdl > 0:
    dl_ratio = pdl / xdl
    print(f"  {'\u2500' * 60}")
    if dl_ratio >= 1:
        print(f"  {G}\u25A0{N} Prisma QUIC is {B}{dl_ratio:.1f}x{N} faster than Xray VLESS")
    else:
        print(f"  Xray VLESS is {B}{1/dl_ratio:.1f}x{N} faster than Prisma QUIC")
    if xmem > 0 and pmem > 0:
        mem_ratio = xmem / pmem
        if mem_ratio >= 1:
            print(f"  {G}\u25A0{N} Prisma uses {B}{mem_ratio:.1f}x{N} less memory than Xray")
        else:
            print(f"  Xray uses {B}{1/mem_ratio:.1f}x{N} less memory than Prisma")
print()

# ── Markdown file ───────────────────────────────────────────────────────
md = []
md.append(f"## Benchmark Results ({DATE})")
md.append("")
md.append(f"**Test:** {TEST_MB}MB payload, {CONCURRENCY}x concurrent streams, loopback")
md.append("")

hdr = "| Metric |" + "".join(f" {n} |" for _, n in scenarios)
sep = "|--------|" + "".join(f" {'---':>{len(n)}} |" for _, n in scenarios)
md.append(hdr)
md.append(sep)

for field, label in fields:
    row = f"| {label} |"
    for key, name in scenarios:
        v = val(key, field)
        if field in skip_bl and key == "baseline":
            s = "\u2014"
        elif v == 0:
            s = "-"
        else:
            s = fmt(v)
        row += f" {s:>{len(name)}} |"
    md.append(row)

md.append("")
md.append("### Verdict")
md.append("")
if bdk:
    md.append(f"- **Fastest download:** {proxy_names[bdk]} ({fmt(bdv)} Mbps)")
if blk:
    md.append(f"- **Lowest latency:** {proxy_names[blk]} ({fmt(blv)} ms)")
if bck:
    md.append(f"- **Best concurrency:** {proxy_names[bck]} ({fmt(bcv)} Mbps)")
if bmk:
    md.append(f"- **Lowest memory:** {proxy_names[bmk]} ({fmt(bmv)} KB idle)")
if bek:
    md.append(f"- **Best cost-effective:** {proxy_names[bek]} ({fmt(eff[0][1])} Mbps/MB RAM)")

if xdl > 0 and pdl > 0:
    md.append("")
    if dl_ratio >= 1:
        md.append(f"Prisma QUIC is **{dl_ratio:.1f}x** faster than Xray VLESS+TLS.")
    else:
        md.append(f"Xray VLESS+TLS is **{1/dl_ratio:.1f}x** faster than Prisma QUIC.")

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
    run_prisma_scenario "prisma-tcp" \
        "$RESULTS_DIR/server-tcp.toml" "$RESULTS_DIR/client-tcp.toml" 11082
    run_prisma_scenario "prisma-shaped" \
        "$RESULTS_DIR/server-shaped.toml" "$RESULTS_DIR/client-shaped.toml" 11081
    run_xray_scenario

    generate_summary

    log "Benchmark complete. Results in $RESULTS_DIR/"
}

main "$@"
