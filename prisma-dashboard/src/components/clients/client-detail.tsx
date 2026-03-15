"use client";

import Link from "next/link";
import { useClients } from "@/hooks/use-clients";
import { useClientBandwidth, useUpdateClientBandwidth, useClientQuota, useUpdateClientQuota } from "@/hooks/use-bandwidth";
import { useConnections } from "@/hooks/use-connections";
import { useI18n } from "@/lib/i18n";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { BandwidthCard } from "@/components/clients/bandwidth-card";
import { QuotaCard } from "@/components/clients/quota-card";
import { ClientTrafficChart } from "@/components/clients/client-traffic-chart";
import { formatBytes } from "@/lib/utils";
import { ArrowLeft } from "lucide-react";

export default function ClientDetailPage({ clientId }: { clientId: string }) {
  const id = clientId;
  const { t } = useI18n();

  const { data: clients, isLoading: clientsLoading } = useClients();
  const { data: bandwidth } = useClientBandwidth(id);
  const { data: quota } = useClientQuota(id);
  const { data: connections } = useConnections();
  const updateBandwidth = useUpdateClientBandwidth();
  const updateQuota = useUpdateClientQuota();

  const client = clients?.find((c) => c.id === id);
  const clientConnections = connections?.filter((c) => c.client_id === id) ?? [];

  if (clientsLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      </div>
    );
  }

  if (!client) {
    return (
      <div className="space-y-4">
        <Link href="/dashboard/clients/">
          <Button variant="ghost" size="sm">
            <ArrowLeft className="h-4 w-4" data-icon="inline-start" />
            {t("sidebar.clients")}
          </Button>
        </Link>
        <div className="flex items-center justify-center py-12">
          <p className="text-sm text-muted-foreground">{t("clients.notFound")}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Link href="/dashboard/clients/">
          <Button variant="ghost" size="icon-sm">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold">
            {client.name || "Unnamed"}
          </h2>
          {client.enabled ? (
            <Badge className="bg-green-500/15 text-green-700 dark:text-green-400">
              {t("clients.active")}
            </Badge>
          ) : (
            <Badge className="bg-red-500/15 text-red-700 dark:text-red-400">
              {t("clients.disabled")}
            </Badge>
          )}
        </div>
      </div>

      <div className="text-xs font-mono text-muted-foreground">
        {t("clients.clientId")}: {client.id}
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <BandwidthCard
          bandwidth={bandwidth}
          onSave={(data) => updateBandwidth.mutate({ id, data })}
          isPending={updateBandwidth.isPending}
        />
        <QuotaCard
          quota={quota}
          onSave={(data) => updateQuota.mutate({ id, data })}
          isPending={updateQuota.isPending}
        />
      </div>

      <ClientTrafficChart
        connections={connections ?? []}
        clientId={id}
      />

      <Card>
        <CardHeader>
          <CardTitle>{t("clients.connections")}</CardTitle>
        </CardHeader>
        <CardContent>
          {clientConnections.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">
              {t("connections.noConnections")}
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("connections.peer")}</TableHead>
                  <TableHead>{t("connections.transport")}</TableHead>
                  <TableHead>{t("connections.mode")}</TableHead>
                  <TableHead>{t("connections.bytesUp")}</TableHead>
                  <TableHead>{t("connections.bytesDown")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {clientConnections.map((conn) => (
                  <TableRow key={conn.session_id}>
                    <TableCell className="font-mono text-xs">
                      {conn.peer_addr}
                    </TableCell>
                    <TableCell>{conn.transport}</TableCell>
                    <TableCell>{conn.mode}</TableCell>
                    <TableCell>{formatBytes(conn.bytes_up)}</TableCell>
                    <TableCell>{formatBytes(conn.bytes_down)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
