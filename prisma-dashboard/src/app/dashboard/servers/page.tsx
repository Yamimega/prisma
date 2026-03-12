"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { formatDuration } from "@/lib/utils";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { TlsInfo } from "@/components/settings/tls-info";

export default function ServersPage() {
  const { data: health, isLoading: healthLoading } = useQuery({
    queryKey: ["health"],
    queryFn: api.getHealth,
    refetchInterval: 10000,
  });

  const { data: config, isLoading: configLoading } = useQuery({
    queryKey: ["config"],
    queryFn: api.getConfig,
  });

  const { data: tls, isLoading: tlsLoading } = useQuery({
    queryKey: ["tls"],
    queryFn: api.getTlsInfo,
  });

  if (healthLoading || configLoading || tlsLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">Loading server info...</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {health && (
        <Card>
          <CardHeader>
            <CardTitle>Health</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">Status:</span>
              <Badge className="bg-green-500/15 text-green-700 dark:text-green-400">
                {health.status}
              </Badge>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Version</p>
              <p className="text-sm font-mono">{health.version}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Uptime</p>
              <p className="text-sm font-mono">{formatDuration(health.uptime_secs)}</p>
            </div>
          </CardContent>
        </Card>
      )}

      {config && (
        <Card>
          <CardHeader>
            <CardTitle>Server Configuration</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div>
              <p className="text-sm text-muted-foreground">Listen Address</p>
              <p className="text-sm font-mono">{config.listen_addr}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">QUIC Listen Address</p>
              <p className="text-sm font-mono">{config.quic_listen_addr}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Max Connections</p>
              <p className="text-sm font-mono">{config.max_connections}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Connection Timeout</p>
              <p className="text-sm font-mono">{config.connection_timeout_secs}s</p>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">Port Forwarding:</span>
              <Badge
                className={
                  config.port_forwarding_enabled
                    ? "bg-green-500/15 text-green-700 dark:text-green-400"
                    : "bg-red-500/15 text-red-700 dark:text-red-400"
                }
              >
                {config.port_forwarding_enabled ? "Enabled" : "Disabled"}
              </Badge>
            </div>
            {config.port_forwarding_enabled && (
              <div>
                <p className="text-sm text-muted-foreground">Port Forwarding Range</p>
                <p className="text-sm font-mono">{config.port_forwarding_range}</p>
              </div>
            )}
            <div>
              <p className="text-sm text-muted-foreground">Logging Level</p>
              <p className="text-sm font-mono">{config.logging_level}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Logging Format</p>
              <p className="text-sm font-mono">{config.logging_format}</p>
            </div>
          </CardContent>
        </Card>
      )}

      {tls && <TlsInfo tls={tls} />}
    </div>
  );
}
