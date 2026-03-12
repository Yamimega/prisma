"use client";

import { useState } from "react";
import { useParams, useRouter } from "next/navigation";
import Link from "next/link";
import { useClients, useUpdateClient, useDeleteClient } from "@/hooks/use-clients";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export default function ClientDetailPage() {
  const params = useParams<{ id: string }>();
  const router = useRouter();
  const { data: clients, isLoading } = useClients();
  const updateClient = useUpdateClient();
  const deleteClient = useDeleteClient();

  const client = clients?.find((c) => c.id === params.id);

  const [name, setName] = useState<string | null>(null);
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  // Use local state if set, otherwise fall back to server data
  const currentName = name ?? client?.name ?? "";
  const currentEnabled = enabled ?? client?.enabled ?? true;

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">Loading client...</p>
      </div>
    );
  }

  if (!client) {
    return (
      <div className="space-y-4">
        <p className="text-sm text-muted-foreground">Client not found.</p>
        <Link href="/dashboard/clients">
          <Button variant="outline" size="sm">
            Back to clients
          </Button>
        </Link>
      </div>
    );
  }

  function handleSave(e: React.FormEvent) {
    e.preventDefault();
    updateClient.mutate(
      { id: params.id, data: { name: currentName, enabled: currentEnabled } },
      {
        onSuccess: () => {
          setName(null);
          setEnabled(null);
        },
      }
    );
  }

  function handleDelete() {
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    deleteClient.mutate(params.id, {
      onSuccess: () => {
        router.push("/dashboard/clients");
      },
    });
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Link href="/dashboard/clients">
          <Button variant="outline" size="sm">
            Back to clients
          </Button>
        </Link>
        <h2 className="text-lg font-semibold">Client Details</h2>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Edit Client</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSave} className="space-y-6">
            <div>
              <p className="text-xs text-muted-foreground mb-1">Client ID</p>
              <p className="font-mono text-sm break-all">{client.id}</p>
            </div>

            <div className="grid gap-1.5">
              <Label htmlFor="client-name">Name</Label>
              <Input
                id="client-name"
                type="text"
                placeholder="Client name"
                value={currentName}
                onChange={(e) => setName(e.target.value)}
              />
            </div>

            <div className="flex items-center justify-between">
              <Label htmlFor="client-enabled">Enabled</Label>
              <Switch
                id="client-enabled"
                checked={currentEnabled}
                onCheckedChange={(checked: boolean) => setEnabled(checked)}
              />
            </div>

            <div className="flex items-center gap-3">
              <Button type="submit" disabled={updateClient.isPending}>
                {updateClient.isPending ? "Saving..." : "Save Changes"}
              </Button>

              <Button
                type="button"
                variant="destructive"
                onClick={handleDelete}
                disabled={deleteClient.isPending}
              >
                {deleteClient.isPending
                  ? "Deleting..."
                  : confirmDelete
                    ? "Confirm Delete"
                    : "Delete Client"}
              </Button>

              {confirmDelete && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setConfirmDelete(false)}
                >
                  Cancel
                </Button>
              )}
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
