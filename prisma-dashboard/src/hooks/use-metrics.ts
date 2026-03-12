"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import type { MetricsSnapshot } from "@/lib/types";
import { createWebSocket } from "@/lib/ws";

const MAX_HISTORY = 120; // 2 minutes at 1s intervals

export function useMetrics() {
  const [current, setCurrent] = useState<MetricsSnapshot | null>(null);
  const [history, setHistory] = useState<MetricsSnapshot[]>([]);
  const wsRef = useRef<ReturnType<typeof createWebSocket> | null>(null);

  useEffect(() => {
    wsRef.current = createWebSocket<MetricsSnapshot>(
      "/api/ws/metrics",
      (snapshot) => {
        setCurrent(snapshot);
        setHistory((prev) => {
          const next = [...prev, snapshot];
          return next.length > MAX_HISTORY ? next.slice(-MAX_HISTORY) : next;
        });
      }
    );

    return () => {
      wsRef.current?.close();
    };
  }, []);

  return { current, history };
}
