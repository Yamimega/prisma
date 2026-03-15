"use client";

import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle, CardAction } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useI18n } from "@/lib/i18n";
import type { ClientBandwidthInfo } from "@/lib/types";
import { Pencil, Save, X } from "lucide-react";

type BandwidthUnit = "bps" | "kbps" | "mbps" | "gbps";

const UNIT_MULTIPLIERS: Record<BandwidthUnit, number> = {
  bps: 1,
  kbps: 1_000,
  mbps: 1_000_000,
  gbps: 1_000_000_000,
};

function detectUnit(bps: number): BandwidthUnit {
  if (bps >= 1_000_000_000) return "gbps";
  if (bps >= 1_000_000) return "mbps";
  if (bps >= 1_000) return "kbps";
  return "bps";
}

function toDisplayValue(bps: number, unit: BandwidthUnit): string {
  if (bps <= 0) return "0";
  return (bps / UNIT_MULTIPLIERS[unit]).toFixed(2).replace(/\.?0+$/, "");
}

function toBps(value: number, unit: BandwidthUnit): number {
  return Math.round(value * UNIT_MULTIPLIERS[unit]);
}

interface BandwidthCardProps {
  bandwidth: ClientBandwidthInfo | undefined;
  onSave: (data: { upload_bps?: number; download_bps?: number }) => void;
  isPending?: boolean;
}

export function BandwidthCard({ bandwidth, onSave, isPending }: BandwidthCardProps) {
  const { t } = useI18n();
  const [editing, setEditing] = useState(false);
  const [unit, setUnit] = useState<BandwidthUnit>(() =>
    bandwidth ? detectUnit(Math.max(bandwidth.upload_bps, bandwidth.download_bps)) : "mbps"
  );
  const [uploadValue, setUploadValue] = useState("");
  const [downloadValue, setDownloadValue] = useState("");

  const startEdit = () => {
    const u = bandwidth
      ? detectUnit(Math.max(bandwidth.upload_bps, bandwidth.download_bps))
      : "mbps";
    setUnit(u);
    setUploadValue(bandwidth ? toDisplayValue(bandwidth.upload_bps, u) : "0");
    setDownloadValue(bandwidth ? toDisplayValue(bandwidth.download_bps, u) : "0");
    setEditing(true);
  };

  const handleSave = () => {
    onSave({
      upload_bps: toBps(parseFloat(uploadValue) || 0, unit),
      download_bps: toBps(parseFloat(downloadValue) || 0, unit),
    });
    setEditing(false);
  };

  const displayUnit = bandwidth
    ? detectUnit(Math.max(bandwidth.upload_bps, bandwidth.download_bps))
    : "mbps";

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("clients.bandwidth")}</CardTitle>
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
              <Label className="text-muted-foreground min-w-20">{t("bandwidth.upload")}</Label>
              <Select value={unit} onValueChange={(v) => setUnit(v as BandwidthUnit)}>
                <SelectTrigger size="sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="bps">{t("clients.unitBps")}</SelectItem>
                  <SelectItem value="kbps">{t("clients.unitKbps")}</SelectItem>
                  <SelectItem value="mbps">{t("clients.unitMbps")}</SelectItem>
                  <SelectItem value="gbps">{t("clients.unitGbps")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <Label className="text-muted-foreground min-w-20">{t("clients.uploadLimit")}</Label>
                <Input
                  type="number"
                  min={0}
                  value={uploadValue}
                  onChange={(e) => setUploadValue(e.target.value)}
                  className="w-32"
                />
              </div>
              <div className="flex items-center gap-2">
                <Label className="text-muted-foreground min-w-20">{t("clients.downloadLimit")}</Label>
                <Input
                  type="number"
                  min={0}
                  value={downloadValue}
                  onChange={(e) => setDownloadValue(e.target.value)}
                  className="w-32"
                />
              </div>
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
          <div className="space-y-3 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("bandwidth.upload")}</span>
              <span className="font-medium">
                {bandwidth
                  ? `${toDisplayValue(bandwidth.upload_bps, displayUnit)} ${displayUnit}`
                  : t("bandwidth.unlimited")}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("bandwidth.download")}</span>
              <span className="font-medium">
                {bandwidth
                  ? `${toDisplayValue(bandwidth.download_bps, displayUnit)} ${displayUnit}`
                  : t("bandwidth.unlimited")}
              </span>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
