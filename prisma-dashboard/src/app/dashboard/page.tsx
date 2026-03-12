"use client";

import { useMetrics } from "@/hooks/use-metrics";
import { useConnections, useDisconnect } from "@/hooks/use-connections";
import { MetricsCards } from "@/components/dashboard/metrics-cards";
import { TrafficChart } from "@/components/dashboard/traffic-chart";
import { ConnectionTable } from "@/components/dashboard/connection-table";

export default function OverviewPage() {
  const { current, history } = useMetrics();
  const { data: connections, isLoading: connectionsLoading } = useConnections();
  const disconnect = useDisconnect();

  return (
    <div className="space-y-6">
      <MetricsCards metrics={current} />
      <TrafficChart history={history} />
      {connectionsLoading ? (
        <div className="flex items-center justify-center py-12">
          <p className="text-sm text-muted-foreground">Loading connections...</p>
        </div>
      ) : (
        <ConnectionTable
          connections={connections ?? []}
          onDisconnect={(sessionId) => disconnect.mutate(sessionId)}
        />
      )}
    </div>
  );
}
