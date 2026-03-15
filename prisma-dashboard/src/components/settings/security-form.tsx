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

export function SecurityForm() {
  const { t } = useI18n();
  const { data: config, isLoading: configLoading } = useQuery({
    queryKey: ["config"],
    queryFn: api.getConfig,
  });

  const { data: tls, isLoading: tlsLoading } = useQuery({
    queryKey: ["tls"],
    queryFn: api.getTlsInfo,
  });

  if (configLoading || tlsLoading || !config) {
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
          <CardTitle>{t("server.tlsInfo")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <KeyValue
            label="TLS Status"
            value={
              <Badge
                className={
                  tls?.enabled
                    ? "bg-green-500/15 text-green-700 dark:text-green-400"
                    : "bg-red-500/15 text-red-700 dark:text-red-400"
                }
              >
                {tls?.enabled ? "Enabled" : "Disabled"}
              </Badge>
            }
          />
          <div>
            <p className="text-muted-foreground">Certificate Path</p>
            <p className="font-mono text-xs mt-1">
              {tls?.cert_path ?? "Not configured"}
            </p>
          </div>
          <div>
            <p className="text-muted-foreground">Key Path</p>
            <p className="font-mono text-xs mt-1">
              {tls?.key_path ?? "Not configured"}
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Security Settings</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <KeyValue
            label="Transport-Only Cipher"
            value={
              <Badge
                className={
                  expanded.allow_transport_only_cipher
                    ? "bg-yellow-500/15 text-yellow-700 dark:text-yellow-400"
                    : "bg-zinc-500/15 text-zinc-700 dark:text-zinc-400"
                }
              >
                {expanded.allow_transport_only_cipher ? "Allowed" : "Disallowed"}
              </Badge>
            }
          />
          <KeyValue
            label="Protocol Version"
            value={
              <span className="font-mono text-xs">
                {expanded.protocol_version || "\u2014"}
              </span>
            }
          />
          <KeyValue
            label="PrismaTLS"
            value={
              <Badge
                className={
                  expanded.prisma_tls_enabled
                    ? "bg-green-500/15 text-green-700 dark:text-green-400"
                    : "bg-zinc-500/15 text-zinc-700 dark:text-zinc-400"
                }
              >
                {expanded.prisma_tls_enabled ? "Enabled" : "Disabled"}
              </Badge>
            }
          />
        </CardContent>
      </Card>
    </div>
  );
}
