"use client";

import Link from "next/link";
import { useClients, useUpdateClient, useDeleteClient } from "@/hooks/use-clients";
import { ClientTable } from "@/components/clients/client-table";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export default function ClientsPage() {
  const { data: clients, isLoading } = useClients();
  const updateClient = useUpdateClient();
  const deleteClient = useDeleteClient();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Registered Clients</h2>
        <Link href="/dashboard/clients/new">
          <Button>Add Client</Button>
        </Link>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Clients</CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <p className="text-sm text-muted-foreground">Loading clients...</p>
            </div>
          ) : (
            <ClientTable
              clients={clients ?? []}
              onToggle={(id, enabled) =>
                updateClient.mutate({ id, data: { enabled } })
              }
              onDelete={(id) => deleteClient.mutate(id)}
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
