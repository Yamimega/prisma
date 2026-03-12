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
import type { MetricsSnapshot } from "@/lib/types";
import { formatBytes } from "@/lib/utils";

interface TrafficChartProps {
  history: MetricsSnapshot[];
}

interface ChartDataPoint {
  time: string;
  bytes_up: number;
  bytes_down: number;
}

export function TrafficChart({ history }: TrafficChartProps) {
  const data = useMemo<ChartDataPoint[]>(() => {
    if (history.length < 2) return [];

    const now = Date.now();
    const points: ChartDataPoint[] = [];

    for (let i = 1; i < history.length; i++) {
      const prev = history[i - 1];
      const curr = history[i];

      const bytesUpDiff = Math.max(0, curr.total_bytes_up - prev.total_bytes_up);
      const bytesDownDiff = Math.max(0, curr.total_bytes_down - prev.total_bytes_down);

      const ts = new Date(curr.timestamp);
      const secsAgo = Math.round((now - ts.getTime()) / 1000);

      const time =
        secsAgo <= 120
          ? `${secsAgo}s ago`
          : ts.toLocaleTimeString([], {
              hour: "2-digit",
              minute: "2-digit",
              second: "2-digit",
            });

      points.push({
        time,
        bytes_up: bytesUpDiff,
        bytes_down: bytesDownDiff,
      });
    }

    return points;
  }, [history]);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Traffic</CardTitle>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <p className="flex h-[300px] items-center justify-center text-sm text-muted-foreground">
            Waiting for data...
          </p>
        ) : (
          <ResponsiveContainer width="100%" height={300}>
            <AreaChart data={data}>
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis
                dataKey="time"
                tick={{ fontSize: 12 }}
                className="text-muted-foreground"
              />
              <YAxis
                tickFormatter={(value: number) => formatBytes(value)}
                tick={{ fontSize: 12 }}
                className="text-muted-foreground"
                width={80}
              />
              {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
              <Tooltip
                formatter={(value: any, name: any) => [
                  formatBytes(Number(value)),
                  name === "bytes_up" ? "Upload" : "Download",
                ]}
                labelFormatter={(label: any) => String(label)}
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
        )}
      </CardContent>
    </Card>
  );
}
