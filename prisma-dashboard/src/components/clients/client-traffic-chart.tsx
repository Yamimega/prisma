"use client";

import { useMemo } from "react";
import {
  ResponsiveContainer,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  CartesianGrid,
} from "recharts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { ConnectionInfo } from "@/lib/types";
import { formatBytes } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";

interface ClientTrafficChartProps {
  connections: ConnectionInfo[];
  clientId: string;
}

export function ClientTrafficChart({ connections, clientId }: ClientTrafficChartProps) {
  const { t } = useI18n();

  const data = useMemo(() => {
    const clientConns = connections.filter((c) => c.client_id === clientId);
    if (clientConns.length === 0) return [];

    return clientConns.map((conn) => ({
      peer: conn.peer_addr,
      bytes_up: conn.bytes_up,
      bytes_down: conn.bytes_down,
    }));
  }, [connections, clientId]);

  const totals = useMemo(() => {
    return data.reduce(
      (acc, d) => ({
        bytes_up: acc.bytes_up + d.bytes_up,
        bytes_down: acc.bytes_down + d.bytes_down,
      }),
      { bytes_up: 0, bytes_down: 0 }
    );
  }, [data]);

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("clients.traffic")}</CardTitle>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <p className="flex h-[200px] items-center justify-center text-sm text-muted-foreground">
            {t("common.noData")}
          </p>
        ) : (
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="text-sm">
                <p className="text-muted-foreground">{t("bandwidth.upload")}</p>
                <p className="text-xl font-bold">{formatBytes(totals.bytes_up)}</p>
              </div>
              <div className="text-sm">
                <p className="text-muted-foreground">{t("bandwidth.download")}</p>
                <p className="text-xl font-bold">{formatBytes(totals.bytes_down)}</p>
              </div>
            </div>
            <ResponsiveContainer width="100%" height={200}>
              <AreaChart data={data}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="peer"
                  tick={{ fontSize: 11 }}
                  className="text-muted-foreground"
                />
                <YAxis
                  tickFormatter={(value: number) => formatBytes(value)}
                  tick={{ fontSize: 11 }}
                  className="text-muted-foreground"
                  width={70}
                />
                <Tooltip
                  formatter={(value, name) => [
                    formatBytes(Number(value)),
                    name === "bytes_up" ? t("bandwidth.upload") : t("bandwidth.download"),
                  ]}
                  contentStyle={{
                    backgroundColor: "hsl(var(--card))",
                    border: "1px solid hsl(var(--border))",
                    borderRadius: "var(--radius)",
                    fontSize: "0.875rem",
                  }}
                />
                <Area
                  type="monotone"
                  dataKey="bytes_up"
                  name="bytes_up"
                  stroke="hsl(217, 91%, 60%)"
                  fill="hsl(217, 91%, 60%)"
                  fillOpacity={0.15}
                  strokeWidth={2}
                />
                <Area
                  type="monotone"
                  dataKey="bytes_down"
                  name="bytes_down"
                  stroke="hsl(142, 71%, 45%)"
                  fill="hsl(142, 71%, 45%)"
                  fillOpacity={0.15}
                  strokeWidth={2}
                />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
