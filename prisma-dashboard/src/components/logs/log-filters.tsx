"use client";

import { useState, useCallback } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface LogFiltersProps {
  onFilterChange: (filter: { level?: string; target?: string }) => void;
}

const levels = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"] as const;

const levelColors: Record<string, string> = {
  ERROR: "border-red-500/50 bg-red-500/10 text-red-700 dark:text-red-400",
  WARN: "border-yellow-500/50 bg-yellow-500/10 text-yellow-700 dark:text-yellow-400",
  INFO: "border-blue-500/50 bg-blue-500/10 text-blue-700 dark:text-blue-400",
  DEBUG: "border-gray-500/50 bg-gray-500/10 text-gray-700 dark:text-gray-400",
  TRACE: "border-gray-400/50 bg-gray-400/10 text-gray-500 dark:text-gray-500",
};

export function LogFilters({ onFilterChange }: LogFiltersProps) {
  const [selectedLevels, setSelectedLevels] = useState<Set<string>>(
    new Set(levels)
  );
  const [target, setTarget] = useState("");

  const emitFilter = useCallback(
    (nextLevels: Set<string>, nextTarget: string) => {
      const allSelected = nextLevels.size === levels.length;
      onFilterChange({
        level: allSelected ? undefined : Array.from(nextLevels).join(","),
        target: nextTarget || undefined,
      });
    },
    [onFilterChange]
  );

  function toggleLevel(level: string) {
    setSelectedLevels((prev) => {
      const next = new Set(prev);
      if (next.has(level)) {
        next.delete(level);
      } else {
        next.add(level);
      }
      emitFilter(next, target);
      return next;
    });
  }

  function handleTargetChange(value: string) {
    setTarget(value);
    emitFilter(selectedLevels, value);
  }

  return (
    <div className="flex flex-wrap items-end gap-4">
      <div className="space-y-1.5">
        <Label>Log Levels</Label>
        <div className="flex gap-1.5">
          {levels.map((level) => {
            const isActive = selectedLevels.has(level);
            const colorClass = isActive
              ? levelColors[level]
              : "border-border bg-transparent text-muted-foreground opacity-50";
            return (
              <button
                key={level}
                type="button"
                onClick={() => toggleLevel(level)}
                className={`inline-flex items-center rounded-md border px-2 py-1 text-xs font-medium transition-colors ${colorClass}`}
              >
                {level}
              </button>
            );
          })}
        </div>
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="target-filter">Target</Label>
        <Input
          id="target-filter"
          type="text"
          placeholder="Filter by target..."
          value={target}
          onChange={(e) => handleTargetChange(e.target.value)}
          className="w-48"
        />
      </div>
    </div>
  );
}
