import type { SystemInfoResponse, BandwidthSummary, MetricsSnapshot, AlertConfig } from "./types";

export interface Alert {
  id: string;
  type: "cert-expiry" | "quota-threshold" | "handshake-spike";
  severity: "warning" | "critical";
  message: string;
}

export function evaluateAlerts(
  systemInfo: SystemInfoResponse | null | undefined,
  bandwidthSummary: BandwidthSummary | null | undefined,
  metrics: MetricsSnapshot | null | undefined,
  alertConfig: AlertConfig | null | undefined
): Alert[] {
  if (!alertConfig) return [];
  const alerts: Alert[] = [];

  // Cert expiry alert
  if (systemInfo?.cert_expiry_days != null) {
    if (systemInfo.cert_expiry_days <= 0) {
      alerts.push({
        id: "cert-expired",
        type: "cert-expiry",
        severity: "critical",
        message: "TLS certificate has expired!",
      });
    } else if (systemInfo.cert_expiry_days <= alertConfig.cert_expiry_days) {
      alerts.push({
        id: "cert-expiring",
        type: "cert-expiry",
        severity: systemInfo.cert_expiry_days <= 7 ? "critical" : "warning",
        message: `TLS certificate expires in ${systemInfo.cert_expiry_days} days`,
      });
    }
  }

  // Quota threshold alert
  if (bandwidthSummary?.clients) {
    for (const client of bandwidthSummary.clients) {
      if (client.quota_bytes > 0) {
        const usedPercent = (client.quota_used / client.quota_bytes) * 100;
        if (usedPercent >= alertConfig.quota_warn_percent) {
          alerts.push({
            id: `quota-${client.client_id}`,
            type: "quota-threshold",
            severity: usedPercent >= 95 ? "critical" : "warning",
            message: `Client ${client.client_name ?? client.client_id} quota at ${usedPercent.toFixed(0)}%`,
          });
        }
      }
    }
  }

  // Handshake spike alert
  if (metrics && metrics.handshake_failures >= alertConfig.handshake_spike_threshold) {
    alerts.push({
      id: "handshake-spike",
      type: "handshake-spike",
      severity: "critical",
      message: `Handshake failures: ${metrics.handshake_failures} (threshold: ${alertConfig.handshake_spike_threshold})`,
    });
  }

  return alerts;
}
