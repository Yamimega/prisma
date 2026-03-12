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
              <p className="text-sm text-muted-foreground">
                Coming Soon &mdash; Camouflage settings will allow you to configure traffic
                obfuscation and protocol mimicry to make proxy traffic blend in with normal
                HTTPS traffic.
              </p>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
