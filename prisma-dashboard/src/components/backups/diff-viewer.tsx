"use client";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";
import type { BackupDiff } from "@/lib/types";

interface DiffViewerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  backupName: string;
  diff: BackupDiff | undefined;
  isLoading: boolean;
}

export function DiffViewer({
  open,
  onOpenChange,
  backupName,
  diff,
  isLoading,
}: DiffViewerProps) {
  const { t } = useI18n();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[80vh]">
        <DialogHeader>
          <DialogTitle>
            {t("backups.diffTitle")}: {backupName}
          </DialogTitle>
        </DialogHeader>
        <div className="overflow-y-auto max-h-[55vh] rounded-md border bg-muted/30 p-3">
          {isLoading ? (
            <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
          ) : diff && diff.changes.length > 0 ? (
            <div className="font-mono text-xs space-y-0.5">
              {diff.changes.map((change, idx) => {
                if (change.tag === "equal") {
                  return (
                    <div key={idx} className="text-muted-foreground px-2 py-0.5">
                      {change.old_value}
                    </div>
                  );
                }
                if (change.tag === "delete") {
                  return (
                    <div
                      key={idx}
                      className="bg-red-500/10 text-red-700 dark:text-red-400 px-2 py-0.5 rounded-sm"
                    >
                      - {change.old_value}
                    </div>
                  );
                }
                if (change.tag === "insert") {
                  return (
                    <div
                      key={idx}
                      className="bg-green-500/10 text-green-700 dark:text-green-400 px-2 py-0.5 rounded-sm"
                    >
                      + {change.new_value}
                    </div>
                  );
                }
                return null;
              })}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">{t("common.noData")}</p>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
