"use client";

import { useEffect, useRef } from "react";
import type { LogEntry } from "@/lib/types";

interface LogViewerProps {
  logs: LogEntry[];
}

const levelColors: Record<string, string> = {
  ERROR: "bg-red-500/15 text-red-700 dark:text-red-400",
  WARN: "bg-yellow-500/15 text-yellow-700 dark:text-yellow-400",
  INFO: "bg-blue-500/15 text-blue-700 dark:text-blue-400",
  DEBUG: "bg-gray-500/15 text-gray-700 dark:text-gray-400",
  TRACE: "bg-gray-500/10 text-gray-500 dark:text-gray-500",
};

function formatTimestamp(ts: string): string {
  const date = new Date(ts);
  const h = String(date.getHours()).padStart(2, "0");
  const m = String(date.getMinutes()).padStart(2, "0");
  const s = String(date.getSeconds()).padStart(2, "0");
  const ms = String(date.getMilliseconds()).padStart(3, "0");
  return `${h}:${m}:${s}.${ms}`;
}

export function LogViewer({ logs }: LogViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const shouldAutoScroll = useRef(true);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    function handleScroll() {
      if (!container) return;
      const { scrollTop, scrollHeight, clientHeight } = container;
      shouldAutoScroll.current = scrollHeight - scrollTop - clientHeight < 40;
    }

    container.addEventListener("scroll", handleScroll);
    return () => container.removeEventListener("scroll", handleScroll);
  }, []);

  useEffect(() => {
    if (shouldAutoScroll.current && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [logs]);

  if (logs.length === 0) {
    return (
      <p className="py-8 text-center text-sm text-muted-foreground">
        No log entries
      </p>
    );
  }

  return (
    <div
      ref={containerRef}
      className="overflow-y-auto max-h-[600px] rounded-lg border bg-muted/30 p-2 font-mono text-xs"
    >
      {logs.map((entry, index) => {
        const colorClass =
          levelColors[entry.level] ?? levelColors.DEBUG;
        return (
          <div
            key={index}
            className="flex items-start gap-2 px-1 py-0.5 hover:bg-muted/50"
          >
            <span className="shrink-0 text-muted-foreground">
              {formatTimestamp(entry.timestamp)}
            </span>
            <span
              className={`inline-flex shrink-0 items-center justify-center rounded px-1.5 py-0.5 text-[10px] font-semibold leading-none ${colorClass}`}
            >
              {entry.level}
            </span>
            <span className="shrink-0 text-muted-foreground/60">
              {entry.target}
            </span>
            <span className="min-w-0 break-all">{entry.message}</span>
          </div>
        );
      })}
    </div>
  );
}
