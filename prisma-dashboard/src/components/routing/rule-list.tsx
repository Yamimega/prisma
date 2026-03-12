"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import type { RoutingRule } from "@/lib/types";

interface RuleListProps {
  rules: RoutingRule[];
  onDelete: (id: string) => void;
  onToggle: (id: string, enabled: boolean) => void;
}

function formatCondition(rule: RoutingRule): string {
  const { condition } = rule;
  switch (condition.type) {
    case "DomainMatch":
      return `Domain matches: ${condition.value}`;
    case "DomainExact":
      return `Domain exact: ${condition.value}`;
    case "IpCidr":
      return `IP CIDR: ${condition.value}`;
    case "PortRange":
      return `Port range: ${condition.value[0]}-${condition.value[1]}`;
    case "All":
      return "All traffic";
  }
}

export function RuleList({ rules, onDelete, onToggle }: RuleListProps) {
  if (rules.length === 0) {
    return (
      <p className="py-8 text-center text-sm text-muted-foreground">
        No routing rules configured
      </p>
    );
  }

  const sorted = [...rules].sort((a, b) => a.priority - b.priority);

  return (
    <div className="space-y-2">
      {sorted.map((rule) => (
        <div
          key={rule.id}
          className="flex items-center gap-4 rounded-lg border px-4 py-3"
        >
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-sm font-semibold">
            {rule.priority}
          </span>
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium truncate">{rule.name}</p>
            <p className="text-xs text-muted-foreground truncate">
              {formatCondition(rule)}
            </p>
          </div>
          {rule.action === "Allow" ? (
            <Badge className="bg-green-500/15 text-green-700 dark:text-green-400">
              Allow
            </Badge>
          ) : (
            <Badge className="bg-red-500/15 text-red-700 dark:text-red-400">
              Block
            </Badge>
          )}
          <Switch
            checked={rule.enabled}
            onCheckedChange={(checked: boolean) => onToggle(rule.id, checked)}
            size="sm"
          />
          <Button
            variant="destructive"
            size="sm"
            onClick={() => onDelete(rule.id)}
          >
            Delete
          </Button>
        </div>
      ))}
    </div>
  );
}
