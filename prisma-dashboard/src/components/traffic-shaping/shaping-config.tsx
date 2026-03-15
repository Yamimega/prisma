"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useI18n } from "@/lib/i18n";
import type { ExpandedConfigResponse } from "@/lib/types";

interface ShapingConfigProps {
  config: ExpandedConfigResponse;
}

export function ShapingConfig({ config }: ShapingConfigProps) {
  const { t } = useI18n();

  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium text-muted-foreground">
            {t("trafficShaping.paddingMode")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("trafficShaping.mode")}</span>
              <Badge variant="secondary">{config.traffic_shaping_mode}</Badge>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Padding Range</span>
              <span className="font-mono text-xs">
                {config.padding_min} - {config.padding_max} bytes
              </span>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium text-muted-foreground">
            {t("trafficShaping.jitter")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("trafficShaping.jitterMs")}</span>
              <span className="font-medium">{config.traffic_shaping_jitter_ms} ms</span>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium text-muted-foreground">
            {t("trafficShaping.chaff")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("clients.status")}</span>
              {config.traffic_shaping_chaff_enabled ? (
                <Badge className="bg-green-500/15 text-green-700 dark:text-green-400">
                  {t("trafficShaping.enabled")}
                </Badge>
              ) : (
                <Badge className="bg-red-500/15 text-red-700 dark:text-red-400">
                  {t("trafficShaping.disabled")}
                </Badge>
              )}
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium text-muted-foreground">
            {t("trafficShaping.coalescingWindow")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("trafficShaping.coalescingWindow")}</span>
              <span className="font-medium">
                {config.traffic_shaping_coalescing_window_ms} ms
              </span>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
