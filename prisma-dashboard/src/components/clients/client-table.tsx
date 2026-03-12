"use client";

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import type { ClientInfo } from "@/lib/types";

interface ClientTableProps {
  clients: ClientInfo[];
  onToggle: (id: string, enabled: boolean) => void;
  onDelete: (id: string) => void;
}

export function ClientTable({ clients, onToggle, onDelete }: ClientTableProps) {
  if (clients.length === 0) {
    return (
      <p className="py-8 text-center text-sm text-muted-foreground">
        No clients registered
      </p>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead>Status</TableHead>
          <TableHead className="text-right">Actions</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {clients.map((client) => (
          <TableRow key={client.id}>
            <TableCell className="font-medium">
              {client.name || "Unnamed"}
            </TableCell>
            <TableCell>
              {client.enabled ? (
                <Badge className="bg-green-500/15 text-green-700 dark:text-green-400">
                  Active
                </Badge>
              ) : (
                <Badge className="bg-red-500/15 text-red-700 dark:text-red-400">
                  Disabled
                </Badge>
              )}
            </TableCell>
            <TableCell className="text-right">
              <div className="flex items-center justify-end gap-3">
                <Switch
                  checked={client.enabled}
                  onCheckedChange={(checked: boolean) =>
                    onToggle(client.id, checked)
                  }
                  size="sm"
                />
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={() => onDelete(client.id)}
                >
                  Delete
                </Button>
              </div>
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
