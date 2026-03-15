"use client";

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import { ConfigForm } from "@/components/settings/config-form";
import { CamouflageForm } from "@/components/settings/camouflage-form";
import { TrafficForm } from "@/components/settings/traffic-form";
import { SecurityForm } from "@/components/settings/security-form";
import { AlertsForm } from "@/components/settings/alerts-form";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";

export default function SettingsPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [feedback, setFeedback] = useState<{ type: "success" | "error"; message: string } | null>(
    null
  );

  const { data: config, isLoading: configLoading } = useQuery({
    queryKey: ["config"],
    queryFn: api.getConfig,
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

  if (configLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
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

      <Tabs defaultValue="general">
        <TabsList>
          <TabsTrigger value="general">{t("settings.general")}</TabsTrigger>
          <TabsTrigger value="camouflage">{t("settings.camouflage")}</TabsTrigger>
          <TabsTrigger value="traffic">{t("settings.traffic")}</TabsTrigger>
          <TabsTrigger value="security">{t("settings.security")}</TabsTrigger>
          <TabsTrigger value="alerts">{t("settings.alerts")}</TabsTrigger>
        </TabsList>

        <TabsContent value="general">
          <Card>
            <CardHeader>
              <CardTitle>{t("settings.general")}</CardTitle>
            </CardHeader>
            <CardContent>
              {config && (
                <ConfigForm
                  key={`${config.logging_level}-${config.logging_format}-${config.max_connections}-${config.port_forwarding_enabled}`}
                  config={config}
                  onSave={(data) => patchConfig.mutate(data)}
                  isLoading={patchConfig.isPending}
                />
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="camouflage">
          <CamouflageForm />
        </TabsContent>

        <TabsContent value="traffic">
          <TrafficForm />
        </TabsContent>

        <TabsContent value="security">
          <SecurityForm />
        </TabsContent>

        <TabsContent value="alerts">
          <AlertsForm />
        </TabsContent>
      </Tabs>
    </div>
  );
}
