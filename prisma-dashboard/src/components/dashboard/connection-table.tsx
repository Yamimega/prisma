"use client";

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { ConnectionInfo } from "@/lib/types";
import { formatBytes } from "@/lib/utils";

interface ConnectionTableProps {
  connections: ConnectionInfo[];
  onDisconnect: (sessionId: string) => void;
}

function formatConnectedAt(connectedAt: string): string {
  const date = new Date(connectedAt);
  return date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function ConnectionTable({
  connections,
  onDisconnect,
}: ConnectionTableProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Active Connections</CardTitle>
      </CardHeader>
      <CardContent>
        {connections.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            No active connections
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Peer</TableHead>
                <TableHead>Transport</TableHead>
                <TableHead>Mode</TableHead>
                <TableHead>Connected</TableHead>
                <TableHead>Bytes Up</TableHead>
                <TableHead>Bytes Down</TableHead>
                <TableHead className="text-right">Action</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {connections.map((conn) => (
                <TableRow key={conn.session_id}>
                  <TableCell className="font-mono text-xs">
                    {conn.peer_addr}
                  </TableCell>
                  <TableCell>{conn.transport}</TableCell>
                  <TableCell>{conn.mode}</TableCell>
                  <TableCell>{formatConnectedAt(conn.connected_at)}</TableCell>
                  <TableCell>{formatBytes(conn.bytes_up)}</TableCell>
                  <TableCell>{formatBytes(conn.bytes_down)}</TableCell>
                  <TableCell className="text-right">
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => onDisconnect(conn.session_id)}
                    >
                      Disconnect
                    </Button>
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
