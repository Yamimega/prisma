"use client";

import { useState } from "react";
import { useAlertConfig, useUpdateAlertConfig } from "@/hooks/use-alerts";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";

export function AlertsForm() {
  const { t } = useI18n();
  const { data: alertConfig, isLoading } = useAlertConfig();
  const updateConfig = useUpdateAlertConfig();

  const [certDays, setCertDays] = useState<number | null>(null);
  const [quotaPercent, setQuotaPercent] = useState<number | null>(null);
  const [handshakeThreshold, setHandshakeThreshold] = useState<number | null>(null);
  const [feedback, setFeedback] = useState<{ type: "success" | "error"; message: string } | null>(null);

  // Use local state if set, otherwise fall back to server data
  const effectiveCertDays = certDays ?? alertConfig?.cert_expiry_days ?? 30;
  const effectiveQuotaPercent = quotaPercent ?? alertConfig?.quota_warn_percent ?? 80;
  const effectiveHandshakeThreshold = handshakeThreshold ?? alertConfig?.handshake_spike_threshold ?? 100;

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    updateConfig.mutate(
      {
        cert_expiry_days: effectiveCertDays,
        quota_warn_percent: effectiveQuotaPercent,
        handshake_spike_threshold: effectiveHandshakeThreshold,
      },
      {
        onSuccess: () => {
          setFeedback({ type: "success", message: "Alert settings saved successfully." });
          setTimeout(() => setFeedback(null), 3000);
        },
        onError: (error: Error) => {
          setFeedback({ type: "error", message: error.message });
          setTimeout(() => setFeedback(null), 5000);
        },
      }
    );
  }

  if (isLoading) {
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

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.alerts")}</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-6">
            <div className="grid gap-1.5">
              <Label htmlFor="cert-expiry-days">
                {t("alerts.certExpiry")} (days)
              </Label>
              <Input
                id="cert-expiry-days"
                type="number"
                value={effectiveCertDays}
                onChange={(e) => setCertDays(parseInt(e.target.value, 10) || 0)}
                min={1}
              />
              <p className="text-xs text-muted-foreground">
                Alert when certificate expires within this many days.
              </p>
            </div>

            <div className="grid gap-1.5">
              <Label htmlFor="quota-warn-percent">
                {t("alerts.quotaThreshold")} (%)
              </Label>
              <Input
                id="quota-warn-percent"
                type="number"
                value={effectiveQuotaPercent}
                onChange={(e) => setQuotaPercent(Math.min(100, Math.max(0, parseInt(e.target.value, 10) || 0)))}
                min={0}
                max={100}
              />
              <p className="text-xs text-muted-foreground">
                Alert when client quota usage exceeds this percentage.
              </p>
            </div>

            <div className="grid gap-1.5">
              <Label htmlFor="handshake-threshold">
                {t("alerts.handshakeSpike")}
              </Label>
              <Input
                id="handshake-threshold"
                type="number"
                value={effectiveHandshakeThreshold}
                onChange={(e) => setHandshakeThreshold(parseInt(e.target.value, 10) || 0)}
                min={1}
              />
              <p className="text-xs text-muted-foreground">
                Alert when handshake failures reach this count.
              </p>
            </div>

            <Button type="submit" disabled={updateConfig.isPending}>
              {updateConfig.isPending ? t("settings.saving") : t("common.save")}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
