export interface HealthResponse {
  status: string;
  uptime_secs: number;
  version: string;
}

export interface MetricsSnapshot {
  timestamp: string;
  uptime_secs: number;
  total_connections: number;
  active_connections: number;
  total_bytes_up: number;
  total_bytes_down: number;
  handshake_failures: number;
}

export interface ConnectionInfo {
  session_id: string;
  client_id: string | null;
  client_name: string | null;
  peer_addr: string;
  transport: string;
  mode: string;
  connected_at: string;
  bytes_up: number;
  bytes_down: number;
}

export interface ClientInfo {
  id: string;
  name: string | null;
  enabled: boolean;
}

export interface CreateClientResponse {
  id: string;
  name: string | null;
  auth_secret_hex: string;
}

export interface ConfigResponse {
  listen_addr: string;
  quic_listen_addr: string;
  tls_enabled: boolean;
  max_connections: number;
  connection_timeout_secs: number;
  port_forwarding_enabled: boolean;
  port_forwarding_range: string;
  logging_level: string;
  logging_format: string;
  camouflage_enabled: boolean;
  camouflage_tls_on_tcp: boolean;
  camouflage_fallback_addr: string | null;
  camouflage_alpn: string[];
}

export interface TlsInfoResponse {
  enabled: boolean;
  cert_path: string | null;
  key_path: string | null;
}

export interface ForwardInfo {
  session_id: string;
  peer_addr: string;
  connected_at: string;
  bytes_up: number;
  bytes_down: number;
}

export interface RoutingRule {
  id: string;
  name: string;
  priority: number;
  condition: RuleCondition;
  action: "Allow" | "Block";
  enabled: boolean;
}

export type RuleCondition =
  | { type: "DomainMatch"; value: string }
  | { type: "DomainExact"; value: string }
  | { type: "IpCidr"; value: string }
  | { type: "PortRange"; value: [number, number] }
  | { type: "All"; value: null };

/** Log levels ordered from most verbose to least verbose. */
export const LOG_LEVELS = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"] as const;
export type LogLevel = (typeof LOG_LEVELS)[number];

/** Numeric priority for each log level (higher = more severe). */
export const LOG_LEVEL_PRIORITY: Record<string, number> = Object.fromEntries(
  LOG_LEVELS.map((l, i) => [l, i])
);

export interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
}

// Phase 2: New types for expanded API

export interface SystemInfoResponse {
  version: string;
  platform: string;
  pid: number;
  cpu_usage: number;
  memory_used_mb: number;
  memory_total_mb: number;
  listeners: ListenerInfo[];
  cert_expiry_days: number | null;
}

export interface ListenerInfo {
  addr: string;
  protocol: string;
}

export interface ClientBandwidthInfo {
  client_id: string;
  upload_bps: number;
  download_bps: number;
}

export interface ClientQuotaInfo {
  client_id: string;
  quota_bytes: number;
  used_bytes: number;
  remaining_bytes: number;
}

export interface BandwidthSummary {
  clients: ClientBandwidthSummaryEntry[];
}

export interface ClientBandwidthSummaryEntry {
  client_id: string;
  client_name: string | null;
  upload_bps: number;
  download_bps: number;
  quota_bytes: number;
  quota_used: number;
}

export interface BackupInfo {
  name: string;
  timestamp: string;
  size: number;
}

export interface BackupDiff {
  changes: DiffChange[];
}

export interface DiffChange {
  tag: "equal" | "insert" | "delete";
  old_value: string | null;
  new_value: string | null;
}

export interface AlertConfig {
  cert_expiry_days: number;
  quota_warn_percent: number;
  handshake_spike_threshold: number;
}

export interface ExpandedConfigResponse extends ConfigResponse {
  // Camouflage (full)
  camouflage_salamander_password: string | null;
  camouflage_h3_enabled: boolean;
  // CDN
  cdn_ws_path: string | null;
  cdn_grpc_path: string | null;
  cdn_xhttp_upload_path: string | null;
  cdn_xhttp_download_path: string | null;
  cdn_xhttp_stream_path: string | null;
  cdn_cover_site: string | null;
  cdn_xporta_enabled: boolean;
  // Traffic shaping
  padding_min: number;
  padding_max: number;
  traffic_shaping_mode: string;
  traffic_shaping_bucket_sizes: number[];
  traffic_shaping_jitter_ms: number;
  traffic_shaping_chaff_enabled: boolean;
  traffic_shaping_coalescing_window_ms: number;
  // Congestion
  congestion_mode: string;
  congestion_target_bandwidth: string | null;
  // Port hopping
  port_hopping_enabled: boolean;
  port_hopping_interval_secs: number;
  port_hopping_ports: string | null;
  // DNS
  dns_upstream: string;
  // Anti-RTT
  anti_rtt_enabled: boolean;
  anti_rtt_normalization_ms: number;
  // PrismaTLS
  prisma_tls_enabled: boolean;
  // Transport-only cipher
  allow_transport_only_cipher: boolean;
  // Protocol version
  protocol_version: string;
}
