"use client";

import { Bell, AlertTriangle, AlertCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import type { Alert } from "@/lib/alerts";

interface AlertBadgeProps {
  alerts: Alert[];
}

export function AlertBadge({ alerts }: AlertBadgeProps) {
  const criticalCount = alerts.filter((a) => a.severity === "critical").length;
  const warningCount = alerts.filter((a) => a.severity === "warning").length;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button variant="ghost" size="icon-sm" className="relative" />
        }
      >
        <Bell className="h-4 w-4" />
        {alerts.length > 0 && (
          <span
            className={`absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-bold text-white ${
              criticalCount > 0 ? "bg-red-500" : "bg-yellow-500"
            }`}
          >
            {alerts.length}
          </span>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={8} className="w-80">
        <DropdownMenuLabel>
          Alerts ({alerts.length})
          {criticalCount > 0 && (
            <span className="ml-1 text-red-500">{criticalCount} critical</span>
          )}
          {warningCount > 0 && (
            <span className="ml-1 text-yellow-600 dark:text-yellow-400">{warningCount} warning</span>
          )}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {alerts.length === 0 ? (
          <div className="px-1.5 py-3 text-center text-sm text-muted-foreground">
            No active alerts
          </div>
        ) : (
          alerts.map((alert) => (
            <DropdownMenuItem key={alert.id} className="cursor-default">
              {alert.severity === "critical" ? (
                <AlertCircle className="h-4 w-4 shrink-0 text-red-500" />
              ) : (
                <AlertTriangle className="h-4 w-4 shrink-0 text-yellow-500" />
              )}
              <span className="text-xs leading-tight">{alert.message}</span>
            </DropdownMenuItem>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
