"use client";

import { useState } from "react";
import Link from "next/link";
import { useCreateClient } from "@/hooks/use-clients";
import { ClientForm } from "@/components/clients/client-form";
import { KeyDisplay } from "@/components/clients/key-display";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { CreateClientResponse } from "@/lib/types";

export default function NewClientPage() {
  const createClient = useCreateClient();
  const [result, setResult] = useState<CreateClientResponse | null>(null);

  function handleSubmit(name: string) {
    createClient.mutate(name || undefined, {
      onSuccess: (data) => {
        setResult(data);
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
        <h2 className="text-lg font-semibold">Add New Client</h2>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{result ? "Client Created" : "Create Client"}</CardTitle>
        </CardHeader>
        <CardContent>
          {result ? (
            <KeyDisplay clientId={result.id} secretHex={result.auth_secret_hex} />
          ) : (
            <ClientForm
              onSubmit={handleSubmit}
              isLoading={createClient.isPending}
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
