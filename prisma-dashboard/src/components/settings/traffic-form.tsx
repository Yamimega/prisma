"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useI18n } from "@/lib/i18n";
import type { ExpandedConfigResponse } from "@/lib/types";

function KeyValue({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right">{value}</span>
    </div>
  );
}

export function TrafficForm() {
  const { t } = useI18n();
  const { data: config, isLoading } = useQuery({
    queryKey: ["config"],
    queryFn: api.getConfig,
  });

  if (isLoading || !config) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      </div>
    );
  }

  const expanded = config as unknown as ExpandedConfigResponse;

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("trafficShaping.title")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <KeyValue
            label={t("trafficShaping.paddingMode")}
            value={
              <span className="font-mono text-xs">
                {expanded.traffic_shaping_mode || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="Padding Range"
            value={
              <span className="font-mono text-xs">
                {expanded.padding_min}\u2013{expanded.padding_max} bytes
              </span>
            }
          />
          <KeyValue
            label={t("trafficShaping.bucketSizes")}
            value={
              <span className="font-mono text-xs">
                {expanded.traffic_shaping_bucket_sizes?.join(", ") || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label={t("trafficShaping.jitter")}
            value={
              <span className="font-mono text-xs">
                {expanded.traffic_shaping_jitter_ms} ms
              </span>
            }
          />
          <KeyValue
            label={t("trafficShaping.chaff")}
            value={
              <Badge
                className={
                  expanded.traffic_shaping_chaff_enabled
                    ? "bg-green-500/15 text-green-700 dark:text-green-400"
                    : "bg-zinc-500/15 text-zinc-700 dark:text-zinc-400"
                }
              >
                {expanded.traffic_shaping_chaff_enabled ? "Enabled" : "Disabled"}
              </Badge>
            }
          />
          <KeyValue
            label="Coalescing Window"
            value={
              <span className="font-mono text-xs">
                {expanded.traffic_shaping_coalescing_window_ms} ms
              </span>
            }
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Congestion Control</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <KeyValue
            label="Mode"
            value={
              <span className="font-mono text-xs">
                {expanded.congestion_mode || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="Target Bandwidth"
            value={
              <span className="font-mono text-xs">
                {expanded.congestion_target_bandwidth || "\u2014"}
              </span>
            }
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Port Hopping</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <KeyValue
            label="Status"
            value={
              <Badge
                className={
                  expanded.port_hopping_enabled
                    ? "bg-green-500/15 text-green-700 dark:text-green-400"
                    : "bg-zinc-500/15 text-zinc-700 dark:text-zinc-400"
                }
              >
                {expanded.port_hopping_enabled ? "Enabled" : "Disabled"}
              </Badge>
            }
          />
          <KeyValue
            label="Interval"
            value={
              <span className="font-mono text-xs">
                {expanded.port_hopping_interval_secs}s
              </span>
            }
          />
          <KeyValue
            label="Ports"
            value={
              <span className="font-mono text-xs">
                {expanded.port_hopping_ports || "\u2014"}
              </span>
            }
          />
        </CardContent>
      </Card>

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>DNS</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <KeyValue
              label="Upstream"
              value={
                <span className="font-mono text-xs">
                  {expanded.dns_upstream || "\u2014"}
                </span>
              }
            />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Anti-RTT</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <KeyValue
              label="Status"
              value={
                <Badge
                  className={
                    expanded.anti_rtt_enabled
                      ? "bg-green-500/15 text-green-700 dark:text-green-400"
                      : "bg-zinc-500/15 text-zinc-700 dark:text-zinc-400"
                  }
                >
                  {expanded.anti_rtt_enabled ? "Enabled" : "Disabled"}
                </Badge>
              }
            />
            <KeyValue
              label="Normalization"
              value={
                <span className="font-mono text-xs">
                  {expanded.anti_rtt_normalization_ms} ms
                </span>
              }
            />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
