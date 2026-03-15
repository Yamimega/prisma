"use client";

import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle, CardAction } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { useI18n } from "@/lib/i18n";
import { formatBytes } from "@/lib/utils";
import type { ClientQuotaInfo } from "@/lib/types";
import { Pencil, Save, X } from "lucide-react";

interface QuotaCardProps {
  quota: ClientQuotaInfo | undefined;
  onSave: (data: { quota_bytes?: number }) => void;
  isPending?: boolean;
}

export function QuotaCard({ quota, onSave, isPending }: QuotaCardProps) {
  const { t } = useI18n();
  const [editing, setEditing] = useState(false);
  const [quotaValueMB, setQuotaValueMB] = useState("");

  const startEdit = () => {
    setQuotaValueMB(
      quota ? String(Math.round(quota.quota_bytes / (1024 * 1024))) : "0"
    );
    setEditing(true);
  };

  const handleSave = () => {
    const mb = parseFloat(quotaValueMB) || 0;
    onSave({ quota_bytes: Math.round(mb * 1024 * 1024) });
    setEditing(false);
  };

  const usedPercent =
    quota && quota.quota_bytes > 0
      ? Math.min(100, (quota.used_bytes / quota.quota_bytes) * 100)
      : 0;

  const barColor =
    usedPercent >= 90
      ? "bg-red-500"
      : usedPercent >= 70
        ? "bg-yellow-500"
        : "bg-green-500";

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("clients.quota")}</CardTitle>
        <CardAction>
          {!editing && (
            <Button variant="ghost" size="icon-sm" onClick={startEdit}>
              <Pencil className="h-4 w-4" />
            </Button>
          )}
        </CardAction>
      </CardHeader>
      <CardContent>
        {editing ? (
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <Label className="text-muted-foreground min-w-24">{t("clients.quotaAmount")}</Label>
              <Input
                type="number"
                min={0}
                value={quotaValueMB}
                onChange={(e) => setQuotaValueMB(e.target.value)}
                className="w-32"
              />
              <span className="text-sm text-muted-foreground">MB</span>
            </div>
            <div className="flex items-center gap-2">
              <Button size="sm" onClick={handleSave} disabled={isPending}>
                <Save className="h-3.5 w-3.5" data-icon="inline-start" />
                {t("common.save")}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setEditing(false)}>
                <X className="h-3.5 w-3.5" data-icon="inline-start" />
                {t("common.cancel")}
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            {quota && quota.quota_bytes > 0 ? (
              <>
                <div className="h-2 w-full rounded-full bg-muted">
                  <div
                    className={`h-full rounded-full transition-all ${barColor}`}
                    style={{ width: `${usedPercent}%` }}
                  />
                </div>
                <div className="grid grid-cols-3 gap-2 text-sm">
                  <div>
                    <p className="text-muted-foreground">{t("quota.used")}</p>
                    <p className="font-medium">{formatBytes(quota.used_bytes)}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">{t("quota.total")}</p>
                    <p className="font-medium">{formatBytes(quota.quota_bytes)}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">{t("quota.remaining")}</p>
                    <p className="font-medium">{formatBytes(quota.remaining_bytes)}</p>
                  </div>
                </div>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">{t("bandwidth.unlimited")}</p>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
