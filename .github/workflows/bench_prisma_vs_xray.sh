#!/usr/bin/env bash
# =============================================================================
# bench_prisma_vs_xray.sh
# Benchmark: Prisma (PrismaVeil v3) vs Xray-core (VLESS+REALITY)
#
# Prerequisites (install before running):
#   sudo apt install -y iperf3 curl hyperfine hey proxychains-ng socat ab
#   pip install matplotlib numpy   # for optional plots
#
# Usage:
#   chmod +x bench_prisma_vs_xray.sh
#   ./bench_prisma_vs_xray.sh [--skip-setup] [--suite throughput|latency|concurrency|resources|all]
#
# Edit PRISMA_SOCKS, XRAY_SOCKS, and IPERF_TARGET to match your config.
# =============================================================================

set -euo pipefail

# ── Config ────────────────────────────────────────────────────────────────────
PRISMA_SOCKS="127.0.0.1:1080"
XRAY_SOCKS="127.0.0.1:10800"
IPERF_TARGET="127.0.0.1"
IPERF_PORT=5201
HTTP_TARGET="http://127.0.0.1:5300"
RESULTS_DIR="./bench_results_$(date +%Y%m%d_%H%M%S)"
RUNS=5           # median over N runs
WARMUP_SECS=10   # warm-up before each timed run
IPERF_DURATION=30

# Colours
RED='\033[0;31m'; GRN='\033[0;32m'; YLW='\033[1;33m'
BLU='\033[0;34m'; CYN='\033[0;36m'; RST='\033[0m'

mkdir -p "$RESULTS_DIR"
LOG="$RESULTS_DIR/bench.log"
exec > >(tee -a "$LOG") 2>&1

# ── Helpers ───────────────────────────────────────────────────────────────────
log()  { echo -e "${BLU}[INFO]${RST}  $*"; }
warn() { echo -e "${YLW}[WARN]${RST}  $*"; }
fail() { echo -e "${RED}[FAIL]${RST}  $*"; exit 1; }
pass() { echo -e "${GRN}[PASS]${RST}  $*"; }
header() { echo -e "\n${CYN}══════════════════════════════════════════${RST}"; \
           echo -e "${CYN}  $*${RST}"; \
           echo -e "${CYN}══════════════════════════════════════════${RST}"; }

check_tool() {
    command -v "$1" &>/dev/null || fail "Required tool '$1' not found. Install it first."
}

wait_for_proxy() {
    local host="${1%%:*}" port="${1##*:}"
    local retries=20
    while ! nc -z "$host" "$port" 2>/dev/null; do
        sleep 0.5
        ((retries--)) || fail "Proxy $1 did not become ready in time"
    done
    pass "Proxy $1 is up"
}

median_of() {
    # usage: median_of val1 val2 val3 ...
    printf '%s\n' "$@" | sort -n | awk 'NR==int((NF+1)/2)'
}

# ── Preflight ─────────────────────────────────────────────────────────────────
header "Preflight checks"
for tool in iperf3 curl hyperfine socat; do check_tool "$tool"; done
command -v hey      &>/dev/null || warn "'hey' not found — latency HTTP tests will be skipped"
command -v proxychains4 &>/dev/null || warn "'proxychains4' not found — using curl SOCKS5 instead"

# Verify both proxies are up
log "Checking Prisma SOCKS5 at $PRISMA_SOCKS ..."
wait_for_proxy "$PRISMA_SOCKS"
log "Checking Xray SOCKS5 at $XRAY_SOCKS ..."
wait_for_proxy "$XRAY_SOCKS"

# Spin up iperf3 server if not already running
if ! pgrep -x iperf3 &>/dev/null; then
    log "Starting iperf3 server on port $IPERF_PORT ..."
    iperf3 -s -p "$IPERF_PORT" -D
    sleep 1
fi

# Spin up minimal HTTP server if not running
if ! nc -z 127.0.0.1 5300 2>/dev/null; then
    log "Starting HTTP server on port 5300 ..."
    python3 -m http.server 5300 --directory /tmp &>/dev/null &
    HTTP_PID=$!
    sleep 1
    trap 'kill $HTTP_PID 2>/dev/null' EXIT
fi

# ── Baseline (no proxy) ───────────────────────────────────────────────────────
header "Baseline: direct loopback (no proxy)"
BASELINE_FILE="$RESULTS_DIR/baseline.txt"

log "Running iperf3 baseline — single stream ..."
iperf3 -c "$IPERF_TARGET" -p "$IPERF_PORT" -t "$IPERF_DURATION" -J \
    > "$RESULTS_DIR/baseline_single.json"
BASELINE_MBPS=$(python3 -c "
import json,sys
d=json.load(open('$RESULTS_DIR/baseline_single.json'))
print(round(d['end']['sum_received']['bits_per_second']/1e6,1))
")
log "Baseline single-stream: ${BASELINE_MBPS} Mbps"

log "Running iperf3 baseline — 16 parallel streams ..."
iperf3 -c "$IPERF_TARGET" -p "$IPERF_PORT" -t "$IPERF_DURATION" -P 16 -J \
    > "$RESULTS_DIR/baseline_parallel.json"
BASELINE_PARALLEL_MBPS=$(python3 -c "
import json
d=json.load(open('$RESULTS_DIR/baseline_parallel.json'))
print(round(d['end']['sum_received']['bits_per_second']/1e6,1))
")
log "Baseline 16-stream: ${BASELINE_PARALLEL_MBPS} Mbps"

echo "baseline_single_mbps=$BASELINE_MBPS" > "$BASELINE_FILE"
echo "baseline_parallel_mbps=$BASELINE_PARALLEL_MBPS" >> "$BASELINE_FILE"

# ── Suite 1: Throughput ───────────────────────────────────────────────────────
header "Suite 1: Throughput"
TPUT_FILE="$RESULTS_DIR/throughput.csv"
echo "proxy,transport,streams,run,mbps" > "$TPUT_FILE"

run_iperf_via_socks() {
    local label="$1" socks="$2" streams="${3:-1}"
    local vals=()
    for i in $(seq 1 "$RUNS"); do
        local mbps
        mbps=$(curl -s --socks5 "$socks" \
            "http://$IPERF_TARGET:$IPERF_PORT/__iperf__" 2>/dev/null || true)
        # Fallback: use proxychains if available
        if command -v proxychains4 &>/dev/null; then
            PROXYCHAINS_CONF=$(mktemp)
            echo -e "[ProxyList]\nsocks5 ${socks%%:*} ${socks##*:}" > "$PROXYCHAINS_CONF"
            mbps=$(proxychains4 -f "$PROXYCHAINS_CONF" -q \
                iperf3 -c "$IPERF_TARGET" -p "$IPERF_PORT" \
                -t "$IPERF_DURATION" -P "$streams" -J 2>/dev/null \
                | python3 -c "
import json,sys
d=json.load(sys.stdin)
print(round(d['end']['sum_received']['bits_per_second']/1e6,1))
" 2>/dev/null || echo "0")
            rm -f "$PROXYCHAINS_CONF"
        else
            warn "proxychains4 not found — iperf via proxy skipped. Install proxychains-ng."
            mbps="N/A"
        fi
        vals+=("$mbps")
        echo "$label,tcp,$streams,$i,$mbps" >> "$TPUT_FILE"
        log "  $label stream=$streams run=$i → $mbps Mbps"
    done
}

log "Throughput: Prisma single stream ..."
run_iperf_via_socks "prisma" "$PRISMA_SOCKS" 1

log "Throughput: Prisma 16 parallel streams ..."
run_iperf_via_socks "prisma" "$PRISMA_SOCKS" 16

log "Throughput: Xray-core single stream ..."
run_iperf_via_socks "xray" "$XRAY_SOCKS" 1

log "Throughput: Xray-core 16 parallel streams ..."
run_iperf_via_socks "xray" "$XRAY_SOCKS" 16

pass "Throughput results saved → $TPUT_FILE"

# ── Suite 2: Latency ──────────────────────────────────────────────────────────
header "Suite 2: Latency"
LAT_FILE="$RESULTS_DIR/latency.json"

run_latency() {
    local label="$1" socks="$2" outfile="$3"
    log "Handshake latency for $label (1000 requests) ..."
    hyperfine \
        --warmup 20 \
        --runs 1000 \
        --export-json "$outfile" \
        "curl -s --socks5 $socks $HTTP_TARGET/ -o /dev/null" 2>/dev/null \
    || warn "hyperfine failed for $label — check proxy connectivity"
}

run_latency "prisma"  "$PRISMA_SOCKS" "$RESULTS_DIR/latency_prisma.json"
run_latency "xray"    "$XRAY_SOCKS"   "$RESULTS_DIR/latency_xray.json"

# Extract p50 / p99 from hyperfine JSON
python3 - <<'PYEOF'
import json, os, sys

results_dir = os.environ.get("RESULTS_DIR", ".")
for proxy in ["prisma", "xray"]:
    path = f"{results_dir}/latency_{proxy}.json"
    if not os.path.exists(path):
        print(f"[SKIP] {path} not found")
        continue
    with open(path) as f:
        d = json.load(f)
    times_ms = sorted(t * 1000 for t in d["results"][0]["times"])
    n = len(times_ms)
    p50  = times_ms[int(n * 0.50)]
    p95  = times_ms[int(n * 0.95)]
    p99  = times_ms[int(n * 0.99)]
    mean = sum(times_ms) / n
    print(f"{proxy:12s}  mean={mean:.1f}ms  p50={p50:.1f}ms  p95={p95:.1f}ms  p99={p99:.1f}ms")
PYEOF
export RESULTS_DIR

if command -v hey &>/dev/null; then
    log "HTTP request latency via hey (10k reqs, 100 concurrency) ..."
    hey -n 10000 -c 100 -x "socks5://$PRISMA_SOCKS" "$HTTP_TARGET/" \
        2>/dev/null > "$RESULTS_DIR/hey_prisma.txt" || warn "hey failed for Prisma"
    hey -n 10000 -c 100 -x "socks5://$XRAY_SOCKS"  "$HTTP_TARGET/" \
        2>/dev/null > "$RESULTS_DIR/hey_xray.txt"   || warn "hey failed for Xray"
    grep -E 'Requests/sec|99%|50%' "$RESULTS_DIR/hey_prisma.txt" || true
    grep -E 'Requests/sec|99%|50%' "$RESULTS_DIR/hey_xray.txt"   || true
fi

pass "Latency results saved → $RESULTS_DIR/latency_*.json"

# ── Suite 3: Concurrency ──────────────────────────────────────────────────────
header "Suite 3: Concurrency ramp"
CONC_FILE="$RESULTS_DIR/concurrency.csv"
echo "proxy,concurrency,rps,p99_ms,errors" > "$CONC_FILE"

if command -v hey &>/dev/null; then
    for C in 10 50 100 200 500 1000; do
        for PROXY in "prisma:$PRISMA_SOCKS" "xray:$XRAY_SOCKS"; do
            label="${PROXY%%:*}"
            socks="${PROXY#*:}"
            log "Concurrency $C — $label ..."
            OUT=$(hey -n $((C * 50)) -c "$C" \
                -x "socks5://$socks" "$HTTP_TARGET/" 2>/dev/null || echo "FAILED")
            RPS=$(echo "$OUT"  | grep 'Requests/sec'  | awk '{print $2}' || echo "0")
            P99=$(echo "$OUT"  | grep '99%'           | awk '{print $2}' | tr -d 's' || echo "0")
            ERRS=$(echo "$OUT" | grep 'Error'         | awk '{print $2}' || echo "0")
            echo "$label,$C,$RPS,$P99,$ERRS" >> "$CONC_FILE"
            log "  → RPS=$RPS  p99=${P99}ms  errors=$ERRS"
            # Stop ramping if p99 > 500ms
            if (( $(echo "$P99 > 500" | bc -l 2>/dev/null || echo 0) )); then
                warn "  p99 exceeded 500ms — stopping ramp for $label at C=$C"
                break
            fi
        done
    done
else
    warn "hey not installed — concurrency tests skipped"
fi

pass "Concurrency results saved → $CONC_FILE"

# ── Suite 4: Resource usage ───────────────────────────────────────────────────
header "Suite 4: Resource usage"
RES_FILE="$RESULTS_DIR/resources.csv"
echo "proxy,phase,rss_mb,cpu_pct" > "$RES_FILE"

sample_resources() {
    local label="$1" pid="$2" duration="$3" outfile="$4"
    local rss_vals=() cpu_vals=()
    local end=$((SECONDS + duration))
    while (( SECONDS < end )); do
        local rss cpu
        rss=$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ' || echo 0)
        cpu=$(ps -o %cpu= -p "$pid" 2>/dev/null | tr -d ' ' || echo 0)
        rss_vals+=("$rss")
        cpu_vals+=("$cpu")
        echo "$label,running,$rss,$cpu" >> "$outfile"
        sleep 0.5
    done
}

PRISMA_PID=$(pgrep -f "prisma.*client" 2>/dev/null | head -1 || echo "")
XRAY_PID=$(pgrep -f "xray" 2>/dev/null | head -1 || echo "")

if [[ -n "$PRISMA_PID" ]]; then
    log "Sampling Prisma (PID $PRISMA_PID) idle RSS ..."
    RSS_IDLE=$(ps -o rss= -p "$PRISMA_PID" | tr -d ' ')
    echo "prisma,idle,$RSS_IDLE,0" >> "$RES_FILE"
    log "  Prisma idle RSS: ${RSS_IDLE} kB ($(( RSS_IDLE / 1024 )) MB)"
else
    warn "Prisma process not found — set PRISMA_PID manually if needed"
fi

if [[ -n "$XRAY_PID" ]]; then
    log "Sampling Xray (PID $XRAY_PID) idle RSS ..."
    RSS_IDLE=$(ps -o rss= -p "$XRAY_PID" | tr -d ' ')
    echo "xray,idle,$RSS_IDLE,0" >> "$RES_FILE"
    log "  Xray idle RSS: ${RSS_IDLE} kB ($(( RSS_IDLE / 1024 )) MB)"
else
    warn "Xray process not found — set XRAY_PID manually if needed"
fi

# Sample under load
if command -v hey &>/dev/null; then
    log "Sampling resources under load (60s, 200 concurrency) ..."
    if [[ -n "$PRISMA_PID" ]]; then
        hey -n 999999 -c 200 -x "socks5://$PRISMA_SOCKS" "$HTTP_TARGET/" \
            &>/dev/null & LOAD_PID=$!
        sample_resources "prisma" "$PRISMA_PID" 60 "$RES_FILE"
        kill $LOAD_PID 2>/dev/null || true
    fi
    if [[ -n "$XRAY_PID" ]]; then
        hey -n 999999 -c 200 -x "socks5://$XRAY_SOCKS" "$HTTP_TARGET/" \
            &>/dev/null & LOAD_PID=$!
        sample_resources "xray" "$XRAY_PID" 60 "$RES_FILE"
        kill $LOAD_PID 2>/dev/null || true
    fi
fi

pass "Resource results saved → $RES_FILE"

# ── Summary ───────────────────────────────────────────────────────────────────
header "Summary"
cat <<EOF

Results written to: $RESULTS_DIR/
  baseline_single.json     — direct loopback iperf3 (no proxy)
  baseline_parallel.json   — 16-stream loopback baseline
  throughput.csv           — Mbps per proxy per stream count
  latency_prisma.json      — 1000-connection hyperfine latency
  latency_xray.json        — 1000-connection hyperfine latency
  hey_prisma.txt           — HTTP latency distribution (Prisma)
  hey_xray.txt             — HTTP latency distribution (Xray)
  concurrency.csv          — RPS + p99 at each concurrency level
  resources.csv            — RSS MB + CPU% idle and under load
  bench.log                — full transcript

Overhead formula:
  overhead_pct = (baseline_mbps - proxy_mbps) / baseline_mbps * 100

Baseline single-stream:   $BASELINE_MBPS Mbps
Baseline 16-stream:       $BASELINE_PARALLEL_MBPS Mbps

Fill in the results template (tab 7 of the benchmark widget) with values
from the CSV/JSON files above.
EOF

pass "Benchmark complete."
