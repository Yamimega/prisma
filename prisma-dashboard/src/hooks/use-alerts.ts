"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { api } from "@/lib/api";
import type { AlertConfig, SystemInfoResponse, BandwidthSummary, MetricsSnapshot } from "@/lib/types";

export function useAlertConfig() {
  return useQuery({
    queryKey: ["alert-config"],
    queryFn: api.getAlertConfig,
  });
}

export function useUpdateAlertConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (data: AlertConfig) => api.updateAlertConfig(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["alert-config"] });
    },
  });
}

export interface Alert {
  id: string;
  type: "cert-expiry" | "quota-threshold" | "handshake-spike";
  severity: "warning" | "critical";
  message: string;
}

export function useActiveAlerts(
  systemInfo: SystemInfoResponse | null | undefined,
  bandwidthSummary: BandwidthSummary | null | undefined,
  metrics: MetricsSnapshot | null | undefined,
  alertConfig: AlertConfig | null | undefined
) {
  return useMemo<Alert[]>(() => {
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
  }, [systemInfo, bandwidthSummary, metrics, alertConfig]);
}
