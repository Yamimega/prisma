"use client";

import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import type { LogEntry } from "@/lib/types";
import { LOG_LEVEL_PRIORITY } from "@/lib/types";
import { createWebSocket } from "@/lib/ws";

const MAX_LOGS = 10000;

let logIdCounter = 0;

export interface LogEntryWithId extends LogEntry {
  _id: number;
}

interface LogFilter {
  level?: string;
  target?: string;
  messageSearch?: string;
  messageSearchRegex?: boolean;
}

export function useLogs() {
  const [allLogs, setAllLogs] = useState<LogEntryWithId[]>([]);
  const [filter, setFilterState] = useState<LogFilter>({});
  const wsRef = useRef<ReturnType<typeof createWebSocket> | null>(null);

  const setFilter = useCallback((f: LogFilter) => {
    setFilterState(f);
    wsRef.current?.send(f);
  }, []);

  const clearLogs = useCallback(() => {
    setAllLogs([]);
  }, []);

  useEffect(() => {
    wsRef.current = createWebSocket<LogEntry>(
      "/api/ws/logs",
      (entry) => {
        const entryWithId: LogEntryWithId = { ...entry, _id: ++logIdCounter };
        setAllLogs((prev) => {
          if (prev.length >= MAX_LOGS) {
            const trimmed = prev.slice(-(MAX_LOGS - 1));
            trimmed.push(entryWithId);
            return trimmed;
          }
          return [...prev, entryWithId];
        });
      }
    );

    return () => {
      wsRef.current?.close();
    };
  }, []);

  // Client-side filtering for level, target, and message search
  const logs = useMemo(() => {
    if (!filter.level && !filter.target && !filter.messageSearch) return allLogs;
    const minPriority = filter.level
      ? LOG_LEVEL_PRIORITY[filter.level.toUpperCase()] ?? 0
      : 0;

    let messageRegex: RegExp | null = null;
    if (filter.messageSearch) {
      if (filter.messageSearchRegex) {
        try {
          messageRegex = new RegExp(filter.messageSearch, "i");
        } catch {
          // Invalid regex, fall back to literal match
          messageRegex = null;
        }
      }
    }

    return allLogs.filter((entry) => {
      if (filter.level && (LOG_LEVEL_PRIORITY[entry.level] ?? 0) < minPriority) {
        return false;
      }
      if (filter.target && !entry.target.includes(filter.target)) {
        return false;
      }
      if (filter.messageSearch) {
        if (messageRegex) {
          if (!messageRegex.test(entry.message)) return false;
        } else {
          if (!entry.message.toLowerCase().includes(filter.messageSearch.toLowerCase())) return false;
        }
      }
      return true;
    });
  }, [allLogs, filter]);

  return { logs, setFilter, clearLogs };
}
