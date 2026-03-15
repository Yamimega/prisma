import { getToken, clearToken } from "./auth";

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const token = getToken();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(init?.headers as Record<string, string>),
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(path, {
    ...init,
    headers,
  });
  if (res.status === 401) {
    // Token invalid — clear and redirect
    clearToken();
    if (typeof window !== "undefined") {
      window.location.href = "/login/";
    }
    throw new Error("Unauthorized");
  }
  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }
  const text = await res.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

export const api = {
  getHealth: () => apiFetch<import("./types").HealthResponse>("/api/health"),
  getMetrics: () => apiFetch<import("./types").MetricsSnapshot>("/api/metrics"),
  getMetricsHistory: (period?: string) =>
    apiFetch<import("./types").MetricsSnapshot[]>(`/api/metrics/history${period ? `?period=${period}` : ""}`),
  getConnections: () => apiFetch<import("./types").ConnectionInfo[]>("/api/connections"),
  disconnectConnection: (id: string) =>
    apiFetch<void>(`/api/connections/${id}`, { method: "DELETE" }),
  getClients: () => apiFetch<import("./types").ClientInfo[]>("/api/clients"),
  createClient: (name?: string) =>
    apiFetch<import("./types").CreateClientResponse>("/api/clients", {
      method: "POST",
      body: JSON.stringify({ name }),
    }),
  updateClient: (id: string, data: { name?: string; enabled?: boolean }) =>
    apiFetch<void>(`/api/clients/${id}`, {
      method: "PUT",
      body: JSON.stringify(data),
    }),
  deleteClient: (id: string) =>
    apiFetch<void>(`/api/clients/${id}`, { method: "DELETE" }),
  getConfig: () => apiFetch<import("./types").ConfigResponse>("/api/config"),
  patchConfig: (data: Record<string, unknown>) =>
    apiFetch<void>("/api/config", {
      method: "PATCH",
      body: JSON.stringify(data),
    }),
  getTlsInfo: () => apiFetch<import("./types").TlsInfoResponse>("/api/config/tls"),
  getForwards: () => apiFetch<import("./types").ForwardInfo[]>("/api/forwards"),
  getRoutes: () => apiFetch<import("./types").RoutingRule[]>("/api/routes"),
  createRoute: (data: Omit<import("./types").RoutingRule, "id">) =>
    apiFetch<import("./types").RoutingRule>("/api/routes", {
      method: "POST",
      body: JSON.stringify(data),
    }),
  updateRoute: (id: string, data: Omit<import("./types").RoutingRule, "id">) =>
    apiFetch<void>(`/api/routes/${id}`, {
      method: "PUT",
      body: JSON.stringify(data),
    }),
  deleteRoute: (id: string) =>
    apiFetch<void>(`/api/routes/${id}`, { method: "DELETE" }),

  // System
  getSystemInfo: () =>
    apiFetch<import("./types").SystemInfoResponse>("/api/system/info"),

  // Bandwidth
  getClientBandwidth: (id: string) =>
    apiFetch<import("./types").ClientBandwidthInfo>(`/api/clients/${id}/bandwidth`),
  updateClientBandwidth: (id: string, data: { upload_bps?: number; download_bps?: number }) =>
    apiFetch<import("./types").ClientBandwidthInfo>(`/api/clients/${id}/bandwidth`, {
      method: "PUT",
      body: JSON.stringify(data),
    }),
  getClientQuota: (id: string) =>
    apiFetch<import("./types").ClientQuotaInfo>(`/api/clients/${id}/quota`),
  updateClientQuota: (id: string, data: { quota_bytes?: number }) =>
    apiFetch<void>(`/api/clients/${id}/quota`, {
      method: "PUT",
      body: JSON.stringify(data),
    }),
  getBandwidthSummary: () =>
    apiFetch<import("./types").BandwidthSummary>("/api/bandwidth/summary"),

  // Backups
  listBackups: () =>
    apiFetch<import("./types").BackupInfo[]>("/api/config/backups"),
  createBackup: () =>
    apiFetch<import("./types").BackupInfo>("/api/config/backup", { method: "POST" }),
  getBackup: (name: string) =>
    apiFetch<string>(`/api/config/backups/${encodeURIComponent(name)}`),
  restoreBackup: (name: string) =>
    apiFetch<void>(`/api/config/backups/${encodeURIComponent(name)}/restore`, { method: "POST" }),
  deleteBackup: (name: string) =>
    apiFetch<void>(`/api/config/backups/${encodeURIComponent(name)}`, { method: "DELETE" }),
  diffBackup: (name: string) =>
    apiFetch<import("./types").BackupDiff>(`/api/config/backups/${encodeURIComponent(name)}/diff`),

  // Alerts
  getAlertConfig: () =>
    apiFetch<import("./types").AlertConfig>("/api/alerts/config"),
  updateAlertConfig: (data: import("./types").AlertConfig) =>
    apiFetch<import("./types").AlertConfig>("/api/alerts/config", {
      method: "PUT",
      body: JSON.stringify(data),
    }),
};
