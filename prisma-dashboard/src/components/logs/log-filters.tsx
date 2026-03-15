"use client";

import { useState, useCallback } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { LOG_LEVELS } from "@/lib/types";
import { useI18n } from "@/lib/i18n";

interface LogFiltersProps {
  onFilterChange: (filter: {
    level?: string;
    target?: string;
    messageSearch?: string;
    messageSearchRegex?: boolean;
  }) => void;
}

// Display order: most severe first
const levels = [...LOG_LEVELS].reverse();

const levelColors: Record<string, string> = {
  ERROR: "border-red-500/50 bg-red-500/10 text-red-700 dark:text-red-400",
  WARN: "border-yellow-500/50 bg-yellow-500/10 text-yellow-700 dark:text-yellow-400",
  INFO: "border-blue-500/50 bg-blue-500/10 text-blue-700 dark:text-blue-400",
  DEBUG: "border-gray-500/50 bg-gray-500/10 text-gray-700 dark:text-gray-400",
  TRACE: "border-gray-400/50 bg-gray-400/10 text-gray-500 dark:text-gray-500",
};

export function LogFilters({ onFilterChange }: LogFiltersProps) {
  const { t } = useI18n();
  const [selectedLevels, setSelectedLevels] = useState<Set<string>>(
    new Set(levels)
  );
  const [target, setTarget] = useState("");
  const [messageSearch, setMessageSearch] = useState("");
  const [useRegex, setUseRegex] = useState(false);

  const emitFilter = useCallback(
    (nextLevels: Set<string>, nextTarget: string, nextMessage: string, nextRegex: boolean) => {
      const allSelected = nextLevels.size === LOG_LEVELS.length;
      // Find the most verbose selected level to use as the minimum filter.
      let minLevel: string | undefined;
      if (!allSelected && nextLevels.size > 0) {
        for (const l of LOG_LEVELS) {
          if (nextLevels.has(l)) {
            minLevel = l.toLowerCase();
            break;
          }
        }
      }
      onFilterChange({
        level: minLevel ?? "",
        target: nextTarget || "",
        messageSearch: nextMessage || "",
        messageSearchRegex: nextRegex,
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
      emitFilter(next, target, messageSearch, useRegex);
      return next;
    });
  }

  function handleTargetChange(value: string) {
    setTarget(value);
    emitFilter(selectedLevels, value, messageSearch, useRegex);
  }

  function handleMessageSearchChange(value: string) {
    setMessageSearch(value);
    emitFilter(selectedLevels, target, value, useRegex);
  }

  function handleRegexToggle() {
    const nextRegex = !useRegex;
    setUseRegex(nextRegex);
    emitFilter(selectedLevels, target, messageSearch, nextRegex);
  }

  return (
    <div className="flex flex-wrap items-end gap-4">
      <div className="space-y-1.5">
        <Label>{t("logs.level")}</Label>
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
        <Label htmlFor="target-filter">{t("logs.target")}</Label>
        <Input
          id="target-filter"
          type="text"
          placeholder="Filter by target..."
          value={target}
          onChange={(e) => handleTargetChange(e.target.value)}
          className="w-48"
        />
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="message-search">{t("logs.message")}</Label>
        <div className="flex gap-1.5">
          <Input
            id="message-search"
            type="text"
            placeholder={t("logs.search")}
            value={messageSearch}
            onChange={(e) => handleMessageSearchChange(e.target.value)}
            className="w-56"
          />
          <Button
            variant={useRegex ? "default" : "outline"}
            size="default"
            onClick={handleRegexToggle}
            title="Toggle regex matching"
            className="font-mono text-xs px-2"
          >
            .*
          </Button>
        </div>
      </div>
    </div>
  );
}
