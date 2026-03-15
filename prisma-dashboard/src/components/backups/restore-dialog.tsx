"use client";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n";

interface RestoreDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  backupName: string;
  onConfirm: () => void;
  isPending?: boolean;
}

export function RestoreDialog({
  open,
  onOpenChange,
  backupName,
  onConfirm,
  isPending,
}: RestoreDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{t("backups.restoreTitle")}</DialogTitle>
          <DialogDescription>
            {t("backups.restoreConfirm")}
          </DialogDescription>
        </DialogHeader>
        <div className="rounded-md border bg-muted/30 p-3">
          <p className="text-sm font-mono">{backupName}</p>
        </div>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isPending}
          >
            {t("common.cancel")}
          </Button>
          <Button onClick={onConfirm} disabled={isPending}>
            {isPending ? t("backups.restoring") : t("common.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
