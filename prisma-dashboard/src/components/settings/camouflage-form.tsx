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

export function CamouflageForm() {
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
          <CardTitle>{t("settings.camouflage")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <KeyValue
            label="Status"
            value={
              <Badge
                className={
                  expanded.camouflage_enabled
                    ? "bg-green-500/15 text-green-700 dark:text-green-400"
                    : "bg-red-500/15 text-red-700 dark:text-red-400"
                }
              >
                {expanded.camouflage_enabled ? "Enabled" : "Disabled"}
              </Badge>
            }
          />
          <KeyValue
            label="Fallback Address"
            value={
              <span className="font-mono text-xs">
                {expanded.camouflage_fallback_addr || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="TLS on TCP"
            value={expanded.camouflage_tls_on_tcp ? "Yes" : "No"}
          />
          <KeyValue
            label="ALPN"
            value={
              <span className="font-mono text-xs">
                {expanded.camouflage_alpn?.join(", ") || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="Salamander Password"
            value={
              <span className="font-mono text-xs">
                {expanded.camouflage_salamander_password ? "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022" : "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="HTTP/3 Enabled"
            value={expanded.camouflage_h3_enabled ? "Yes" : "No"}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>CDN</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <KeyValue
            label="WebSocket Path"
            value={
              <span className="font-mono text-xs">
                {expanded.cdn_ws_path || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="gRPC Path"
            value={
              <span className="font-mono text-xs">
                {expanded.cdn_grpc_path || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="XHTTP Upload Path"
            value={
              <span className="font-mono text-xs">
                {expanded.cdn_xhttp_upload_path || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="XHTTP Download Path"
            value={
              <span className="font-mono text-xs">
                {expanded.cdn_xhttp_download_path || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="XHTTP Stream Path"
            value={
              <span className="font-mono text-xs">
                {expanded.cdn_xhttp_stream_path || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="Cover Site"
            value={
              <span className="font-mono text-xs">
                {expanded.cdn_cover_site || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="XPorta Enabled"
            value={expanded.cdn_xporta_enabled ? "Yes" : "No"}
          />
        </CardContent>
      </Card>
    </div>
  );
}
