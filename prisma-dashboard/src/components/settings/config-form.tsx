"use client";

import { useState } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { ConfigResponse } from "@/lib/types";

interface ConfigFormProps {
  config: ConfigResponse;
  onSave: (data: Record<string, unknown>) => void;
  isLoading: boolean;
}

const loggingLevels = ["trace", "debug", "info", "warn", "error"];
const loggingFormats = ["compact", "full", "pretty", "json"];

export function ConfigForm({ config, onSave, isLoading }: ConfigFormProps) {
  const [loggingLevel, setLoggingLevel] = useState(config.logging_level);
  const [loggingFormat, setLoggingFormat] = useState(config.logging_format);
  const [maxConnections, setMaxConnections] = useState(config.max_connections);
  const [portForwardingEnabled, setPortForwardingEnabled] = useState(
    config.port_forwarding_enabled
  );

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    onSave({
      logging_level: loggingLevel,
      logging_format: loggingFormat,
      max_connections: maxConnections,
      port_forwarding_enabled: portForwardingEnabled,
    });
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-6">
      <div className="space-y-4">
        <h3 className="text-sm font-medium text-muted-foreground">
          Read-only
        </h3>
        <div className="grid gap-1.5">
          <Label>Listen Address</Label>
          <p className="rounded-lg border bg-muted/30 px-2.5 py-1.5 text-sm text-muted-foreground">
            {config.listen_addr}
          </p>
        </div>
        <div className="grid gap-1.5">
          <Label>QUIC Listen Address</Label>
          <p className="rounded-lg border bg-muted/30 px-2.5 py-1.5 text-sm text-muted-foreground">
            {config.quic_listen_addr}
          </p>
        </div>
      </div>

      <div className="space-y-4">
        <h3 className="text-sm font-medium text-muted-foreground">
          Editable Settings
        </h3>

        <div className="grid gap-1.5">
          <Label>Logging Level</Label>
          <Select value={loggingLevel} onValueChange={(v) => v && setLoggingLevel(v)}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {loggingLevels.map((level) => (
                <SelectItem key={level} value={level}>
                  {level}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="grid gap-1.5">
          <Label>Logging Format</Label>
          <Select value={loggingFormat} onValueChange={(v) => v && setLoggingFormat(v)}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {loggingFormats.map((format) => (
                <SelectItem key={format} value={format}>
                  {format}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="grid gap-1.5">
          <Label htmlFor="max-connections">Max Connections</Label>
          <Input
            id="max-connections"
            type="number"
            value={maxConnections}
            onChange={(e) =>
              setMaxConnections(parseInt(e.target.value, 10) || 0)
            }
            min={0}
          />
        </div>

        <div className="flex items-center justify-between">
          <Label htmlFor="port-forwarding">Port Forwarding</Label>
          <Switch
            id="port-forwarding"
            checked={portForwardingEnabled}
            onCheckedChange={(checked: boolean) =>
              setPortForwardingEnabled(checked)
            }
          />
        </div>
      </div>

      <Button type="submit" disabled={isLoading}>
        {isLoading ? "Saving..." : "Save Settings"}
      </Button>
    </form>
  );
}
