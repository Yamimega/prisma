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

export interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
}
