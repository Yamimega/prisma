"use client";

import { useMemo, useState } from "react";
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
import { Button } from "@/components/ui/button";
import type { MetricsSnapshot } from "@/lib/types";
import { formatBytes } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import { useMetricsHistory, computeRateMbps, type TimeRange } from "@/hooks/use-metrics";

interface TrafficChartProps {
  history: MetricsSnapshot[];
}

interface ChartDataPoint {
  time: string;
  bytes_up: number;
  bytes_down: number;
}

interface MbpsDataPoint {
  time: string;
  uploadMbps: number;
  downloadMbps: number;
}

const TIME_RANGES: { key: TimeRange; i18nKey: string }[] = [
  { key: "1h", i18nKey: "chart.timeRange.1h" },
  { key: "6h", i18nKey: "chart.timeRange.6h" },
  { key: "24h", i18nKey: "chart.timeRange.24h" },
  { key: "7d", i18nKey: "chart.timeRange.7d" },
];

const RESOLUTION_MAP: Record<TimeRange, "1s" | "10s" | "60s"> = {
  "1h": "10s",
  "6h": "60s",
  "24h": "60s",
  "7d": "60s",
};

export function TrafficChart({ history }: TrafficChartProps) {
  const { t } = useI18n();
  const [selectedRange, setSelectedRange] = useState<TimeRange | null>(null);
  const [showMbps, setShowMbps] = useState(false);

  const resolution = selectedRange ? RESOLUTION_MAP[selectedRange] : "10s";
  const { data: historicalData } = useMetricsHistory(
    selectedRange ?? "1h",
    resolution
  );

  const activeHistory = selectedRange && historicalData ? historicalData : history;

  const data = useMemo<ChartDataPoint[]>(() => {
    if (activeHistory.length < 2) return [];

    const latestTs = new Date(activeHistory[activeHistory.length - 1].timestamp).getTime();
    const points: ChartDataPoint[] = [];

    for (let i = 1; i < activeHistory.length; i++) {
      const prev = activeHistory[i - 1];
      const curr = activeHistory[i];

      const bytesUpDiff = Math.max(0, curr.total_bytes_up - prev.total_bytes_up);
      const bytesDownDiff = Math.max(0, curr.total_bytes_down - prev.total_bytes_down);

      const ts = new Date(curr.timestamp);
      const secsAgo = Math.round((latestTs - ts.getTime()) / 1000);

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
  }, [activeHistory]);

  const mbpsData = useMemo<MbpsDataPoint[]>(() => {
    if (!showMbps || activeHistory.length < 2) return [];
    const rates = computeRateMbps(activeHistory);
    const latestTs = rates.length > 0 ? new Date(rates[rates.length - 1].timestamp).getTime() : 0;
    return rates.map((r) => {
      const ts = new Date(r.timestamp);
      const secsAgo = Math.round((latestTs - ts.getTime()) / 1000);
      const time =
        secsAgo <= 120
          ? `${secsAgo}s ago`
          : ts.toLocaleTimeString([], {
              hour: "2-digit",
              minute: "2-digit",
              second: "2-digit",
            });
      return {
        time,
        uploadMbps: Math.round(r.uploadMbps * 100) / 100,
        downloadMbps: Math.round(r.downloadMbps * 100) / 100,
      };
    });
  }, [activeHistory, showMbps]);

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>{t("dashboard.realtimeTraffic")}</CardTitle>
          <div className="flex items-center gap-1.5">
            <Button
              variant={selectedRange === null ? "default" : "outline"}
              size="xs"
              onClick={() => setSelectedRange(null)}
            >
              Live
            </Button>
            {TIME_RANGES.map(({ key, i18nKey }) => (
              <Button
                key={key}
                variant={selectedRange === key ? "default" : "outline"}
                size="xs"
                onClick={() => setSelectedRange(key)}
              >
                {t(i18nKey)}
              </Button>
            ))}
            <div className="ml-2 h-4 w-px bg-border" />
            <Button
              variant={showMbps ? "default" : "outline"}
              size="xs"
              onClick={() => setShowMbps((prev) => !prev)}
            >
              {t("chart.mbps")}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {showMbps ? (
          mbpsData.length === 0 ? (
            <p className="flex h-[300px] items-center justify-center text-sm text-muted-foreground">
              Waiting for data...
            </p>
          ) : (
            <ResponsiveContainer width="100%" height={300}>
              <AreaChart data={mbpsData}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="time"
                  tick={{ fontSize: 12 }}
                  className="text-muted-foreground"
                />
                <YAxis
                  tickFormatter={(value: number) => `${value} Mbps`}
                  tick={{ fontSize: 12 }}
                  className="text-muted-foreground"
                  width={90}
                />
                <Tooltip
                  formatter={(value, name) => [
                    `${Number(value).toFixed(2)} Mbps`,
                    name === "uploadMbps" ? "Upload" : "Download",
                  ]}
                  labelFormatter={(label) => String(label)}
                  contentStyle={{
                    backgroundColor: "hsl(var(--card))",
                    border: "1px solid hsl(var(--border))",
                    borderRadius: "var(--radius)",
                    fontSize: "0.875rem",
                  }}
                />
                <Area
                  type="monotone"
                  dataKey="uploadMbps"
                  name="uploadMbps"
                  stroke="hsl(217, 91%, 60%)"
                  fill="hsl(217, 91%, 60%)"
                  fillOpacity={0.15}
                  strokeWidth={2}
                />
                <Area
                  type="monotone"
                  dataKey="downloadMbps"
                  name="downloadMbps"
                  stroke="hsl(142, 71%, 45%)"
                  fill="hsl(142, 71%, 45%)"
                  fillOpacity={0.15}
                  strokeWidth={2}
                />
              </AreaChart>
            </ResponsiveContainer>
          )
        ) : data.length === 0 ? (
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
              <Tooltip
                formatter={(value, name) => [
                  formatBytes(Number(value)),
                  name === "bytes_up" ? "Upload" : "Download",
                ]}
                labelFormatter={(label) => String(label)}
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
