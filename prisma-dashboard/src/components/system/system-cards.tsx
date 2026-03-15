"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useI18n } from "@/lib/i18n";
import type { SystemInfoResponse } from "@/lib/types";
import { Shield, Cpu, MemoryStick } from "lucide-react";

interface SystemCardsProps {
  info: SystemInfoResponse;
}

export function SystemCards({ info }: SystemCardsProps) {
  const { t } = useI18n();

  const memoryPercent =
    info.memory_total_mb > 0
      ? Math.round((info.memory_used_mb / info.memory_total_mb) * 100)
      : 0;

  const cpuPercent = Math.round(info.cpu_usage * 100) / 100;

  const certColor =
    info.cert_expiry_days === null
      ? "text-muted-foreground"
      : info.cert_expiry_days <= 0
        ? "text-red-600 dark:text-red-400"
        : info.cert_expiry_days <= 7
          ? "text-red-600 dark:text-red-400"
          : info.cert_expiry_days <= 30
            ? "text-yellow-600 dark:text-yellow-400"
            : "text-green-600 dark:text-green-400";

  const certBarColor =
    info.cert_expiry_days === null
      ? "bg-muted-foreground"
      : info.cert_expiry_days <= 7
        ? "bg-red-500"
        : info.cert_expiry_days <= 30
          ? "bg-yellow-500"
          : "bg-green-500";

  return (
    <div className="space-y-6">
      {/* Info cards */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">
              {t("system.version")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-bold">{info.version}</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">
              {t("system.platform")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-bold">{info.platform}</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">
              {t("system.pid")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-2xl font-bold font-mono">{info.pid}</p>
          </CardContent>
        </Card>
      </div>

      {/* Resource gauges */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <Cpu className="h-4 w-4" />
              {t("system.cpu")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span className="font-medium">{cpuPercent}%</span>
              </div>
              <div className="h-2 w-full rounded-full bg-muted">
                <div
                  className={`h-full rounded-full transition-all ${
                    cpuPercent >= 90
                      ? "bg-red-500"
                      : cpuPercent >= 70
                        ? "bg-yellow-500"
                        : "bg-blue-500"
                  }`}
                  style={{ width: `${Math.min(100, cpuPercent)}%` }}
                />
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <MemoryStick className="h-4 w-4" />
              {t("system.memory")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span className="font-medium">{memoryPercent}%</span>
                <span className="text-muted-foreground">
                  {info.memory_used_mb} / {info.memory_total_mb} MB
                </span>
              </div>
              <div className="h-2 w-full rounded-full bg-muted">
                <div
                  className={`h-full rounded-full transition-all ${
                    memoryPercent >= 90
                      ? "bg-red-500"
                      : memoryPercent >= 70
                        ? "bg-yellow-500"
                        : "bg-green-500"
                  }`}
                  style={{ width: `${Math.min(100, memoryPercent)}%` }}
                />
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <Shield className="h-4 w-4" />
              {t("system.certExpiry")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              <p className={`text-xl font-bold ${certColor}`}>
                {info.cert_expiry_days === null
                  ? t("system.noCert")
                  : info.cert_expiry_days <= 0
                    ? t("system.certExpired")
                    : t("system.certDays", { days: info.cert_expiry_days })}
              </p>
              {info.cert_expiry_days !== null && info.cert_expiry_days > 0 && (
                <div className="h-2 w-full rounded-full bg-muted">
                  <div
                    className={`h-full rounded-full transition-all ${certBarColor}`}
                    style={{
                      width: `${Math.min(100, Math.max(5, (info.cert_expiry_days / 365) * 100))}%`,
                    }}
                  />
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
