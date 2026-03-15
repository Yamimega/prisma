"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { useI18n } from "@/lib/i18n";
import type { ListenerInfo } from "@/lib/types";

interface ListenersListProps {
  listeners: ListenerInfo[];
}

export function ListenersList({ listeners }: ListenersListProps) {
  const { t } = useI18n();

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.listeners")}</CardTitle>
      </CardHeader>
      <CardContent>
        {listeners.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            {t("common.noData")}
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("system.listenerAddr")}</TableHead>
                <TableHead>{t("system.listenerProtocol")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {listeners.map((listener, idx) => (
                <TableRow key={`${listener.addr}-${listener.protocol}-${idx}`}>
                  <TableCell className="font-mono text-xs">
                    {listener.addr}
                  </TableCell>
                  <TableCell>
                    <Badge variant="secondary">{listener.protocol}</Badge>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
