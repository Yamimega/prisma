"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type { ExpandedConfigResponse } from "@/lib/types";
import { BucketChart } from "@/components/traffic-shaping/bucket-chart";
import { ShapingConfig } from "@/components/traffic-shaping/shaping-config";

export default function TrafficShapingPage() {
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

  // Cast to ExpandedConfigResponse since the expanded config endpoint returns full shaping data
  const expandedConfig = config as unknown as ExpandedConfigResponse;

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">{t("trafficShaping.title")}</h2>

      <ShapingConfig config={expandedConfig} />

      <BucketChart
        bucketSizes={expandedConfig.traffic_shaping_bucket_sizes ?? []}
      />
    </div>
  );
}
