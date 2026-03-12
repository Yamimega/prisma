"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import type { LogEntry } from "@/lib/types";
import { createWebSocket } from "@/lib/ws";

const MAX_LOGS = 10000;

export function useLogs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const wsRef = useRef<ReturnType<typeof createWebSocket> | null>(null);

  const setFilter = useCallback((filter: { level?: string; target?: string }) => {
    wsRef.current?.send(filter);
  }, []);

  const clearLogs = useCallback(() => {
    setLogs([]);
  }, []);

  useEffect(() => {
    wsRef.current = createWebSocket<LogEntry>(
      "/api/ws/logs",
      (entry) => {
        setLogs((prev) => {
          const next = [...prev, entry];
          return next.length > MAX_LOGS ? next.slice(-MAX_LOGS) : next;
        });
      }
    );

    return () => {
      wsRef.current?.close();
    };
  }, []);

  return { logs, setFilter, clearLogs };
}
