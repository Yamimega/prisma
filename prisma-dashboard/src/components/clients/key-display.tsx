"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";

interface KeyDisplayProps {
  clientId: string;
  secretHex: string;
}

export function KeyDisplay({ clientId, secretHex }: KeyDisplayProps) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    await navigator.clipboard.writeText(secretHex);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="space-y-3">
      <div className="rounded-lg border border-yellow-500/50 bg-yellow-500/10 px-4 py-3 text-sm font-medium text-yellow-700 dark:text-yellow-400">
        Save this key now — it won&apos;t be shown again
      </div>
      <div className="rounded-lg border p-4 space-y-3">
        <div>
          <p className="text-xs text-muted-foreground mb-1">Client ID</p>
          <p className="font-mono text-sm break-all">{clientId}</p>
        </div>
        <div>
          <p className="text-xs text-muted-foreground mb-1">Auth Secret</p>
          <div className="flex items-center gap-2">
            <p className="font-mono text-sm break-all flex-1">{secretHex}</p>
            <Button variant="outline" size="sm" onClick={handleCopy}>
              {copied ? "Copied!" : "Copy"}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
