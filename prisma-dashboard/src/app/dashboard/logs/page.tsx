"use client";

import { useLogs } from "@/hooks/use-logs";
import { LogViewer } from "@/components/logs/log-viewer";
import { LogFilters } from "@/components/logs/log-filters";
import { Button } from "@/components/ui/button";

export default function LogsPage() {
  const { logs, setFilter, clearLogs } = useLogs();

  return (
    <div className="flex h-full flex-col space-y-4">
      <div className="flex items-center justify-between">
        <LogFilters onFilterChange={setFilter} />
        <Button variant="outline" size="sm" onClick={clearLogs}>
          Clear
        </Button>
      </div>
      <div className="flex-1 min-h-0">
        <LogViewer logs={logs} />
      </div>
    </div>
  );
}
