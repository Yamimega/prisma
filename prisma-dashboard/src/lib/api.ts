const API_BASE = "/api/proxy";

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...init?.headers,
    },
  });
  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }
  return res.json();
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
};
