"use client";

import { useState } from "react";
import { useBackups, useCreateBackup, useRestoreBackup, useDeleteBackup, useBackupDiff } from "@/hooks/use-backups";
import { useI18n } from "@/lib/i18n";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { BackupTable } from "@/components/backups/backup-table";
import { DiffViewer } from "@/components/backups/diff-viewer";
import { RestoreDialog } from "@/components/backups/restore-dialog";
import { Plus } from "lucide-react";

export default function BackupsPage() {
  const { t } = useI18n();
  const { data: backups, isLoading } = useBackups();
  const createBackup = useCreateBackup();
  const restoreBackup = useRestoreBackup();
  const deleteBackup = useDeleteBackup();

  const [restoreName, setRestoreName] = useState<string | null>(null);
  const [diffName, setDiffName] = useState<string | null>(null);
  const [deletingName, setDeletingName] = useState<string | null>(null);

  const { data: diffData, isLoading: diffLoading } = useBackupDiff(diffName);

  const handleRestore = (name: string) => {
    setRestoreName(name);
  };

  const handleConfirmRestore = () => {
    if (restoreName) {
      restoreBackup.mutate(restoreName, {
        onSuccess: () => setRestoreName(null),
      });
    }
  };

  const handleDelete = (name: string) => {
    setDeletingName(name);
    deleteBackup.mutate(name, {
      onSettled: () => setDeletingName(null),
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">{t("backups.title")}</h2>
        <Button
          onClick={() => createBackup.mutate()}
          disabled={createBackup.isPending}
        >
          <Plus className="h-4 w-4" data-icon="inline-start" />
          {createBackup.isPending ? t("backups.creating") : t("backups.create")}
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t("backups.title")}</CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
            </div>
          ) : (
            <BackupTable
              backups={backups ?? []}
              onRestore={handleRestore}
              onDiff={(name) => setDiffName(name)}
              onDelete={handleDelete}
              deletingName={deletingName}
            />
          )}
        </CardContent>
      </Card>

      {/* Restore confirmation dialog */}
      <RestoreDialog
        open={restoreName !== null}
        onOpenChange={(open) => {
          if (!open) setRestoreName(null);
        }}
        backupName={restoreName ?? ""}
        onConfirm={handleConfirmRestore}
        isPending={restoreBackup.isPending}
      />

      {/* Diff viewer dialog */}
      <DiffViewer
        open={diffName !== null}
        onOpenChange={(open) => {
          if (!open) setDiffName(null);
        }}
        backupName={diffName ?? ""}
        diff={diffData}
        isLoading={diffLoading}
      />
    </div>
  );
}
