"use client";

import { useSystemInfo } from "@/hooks/use-system-info";
import { useI18n } from "@/lib/i18n";
import { SystemCards } from "@/components/system/system-cards";
import { ListenersList } from "@/components/system/listeners-list";

export default function SystemPage() {
  const { t } = useI18n();
  const { data: info, isLoading } = useSystemInfo();

  if (isLoading || !info) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">{t("system.title")}</h2>
      <SystemCards info={info} />
      <ListenersList listeners={info.listeners} />
    </div>
  );
}
