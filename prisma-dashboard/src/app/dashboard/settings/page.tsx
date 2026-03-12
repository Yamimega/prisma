"use client";

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { ConfigForm } from "@/components/settings/config-form";
import { TlsInfo } from "@/components/settings/tls-info";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export default function SettingsPage() {
  const queryClient = useQueryClient();
  const [feedback, setFeedback] = useState<{ type: "success" | "error"; message: string } | null>(
    null
  );

  const { data: config, isLoading: configLoading } = useQuery({
    queryKey: ["config"],
    queryFn: api.getConfig,
  });

  const { data: tls, isLoading: tlsLoading } = useQuery({
    queryKey: ["tls"],
    queryFn: api.getTlsInfo,
  });

  const patchConfig = useMutation({
    mutationFn: (data: Record<string, unknown>) => api.patchConfig(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["config"] });
      setFeedback({ type: "success", message: "Settings saved successfully." });
      setTimeout(() => setFeedback(null), 3000);
    },
    onError: (error: Error) => {
      setFeedback({ type: "error", message: error.message });
      setTimeout(() => setFeedback(null), 5000);
    },
  });

  if (configLoading || tlsLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">Loading settings...</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {feedback && (
        <div
          className={`rounded-lg border px-4 py-3 text-sm font-medium ${
            feedback.type === "success"
              ? "border-green-500/50 bg-green-500/10 text-green-700 dark:text-green-400"
              : "border-red-500/50 bg-red-500/10 text-red-700 dark:text-red-400"
          }`}
        >
          {feedback.message}
        </div>
      )}

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Configuration</CardTitle>
          </CardHeader>
          <CardContent>
            {config && (
              <ConfigForm
                config={config}
                onSave={(data) => patchConfig.mutate(data)}
                isLoading={patchConfig.isPending}
              />
            )}
          </CardContent>
        </Card>

        <div className="space-y-6">
          {tls && <TlsInfo tls={tls} />}

          <Card>
            <CardHeader>
              <CardTitle>Camouflage</CardTitle>
            </CardHeader>
            <CardContent>
              {config ? (
                <div className="space-y-3 text-sm">
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">Status</span>
                    <span className={config.camouflage_enabled ? "text-green-600 dark:text-green-400 font-medium" : "text-muted-foreground"}>
                      {config.camouflage_enabled ? "Enabled" : "Disabled"}
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">TLS on TCP</span>
                    <span>{config.camouflage_tls_on_tcp ? "Yes" : "No"}</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">Fallback</span>
                    <span className="font-mono text-xs">{config.camouflage_fallback_addr || "—"}</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground">ALPN</span>
                    <span className="font-mono text-xs">{config.camouflage_alpn?.join(", ") || "—"}</span>
                  </div>
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">Loading...</p>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
