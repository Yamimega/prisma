"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import { useRouter } from "next/navigation";
import { useQueryClient } from "@tanstack/react-query";
import { Search } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/lib/i18n";
import type { ClientInfo, ConfigResponse } from "@/lib/types";

interface SearchResult {
  id: string;
  type: "page" | "client" | "config";
  label: string;
  description?: string;
  href?: string;
}

const PAGES: { label: string; href: string; i18nKey: string }[] = [
  { label: "Overview", href: "/dashboard", i18nKey: "sidebar.overview" },
  { label: "Server", href: "/dashboard/servers", i18nKey: "sidebar.server" },
  { label: "Clients", href: "/dashboard/clients", i18nKey: "sidebar.clients" },
  { label: "Routing Rules", href: "/dashboard/routing", i18nKey: "sidebar.routing" },
  { label: "Logs", href: "/dashboard/logs", i18nKey: "sidebar.logs" },
  { label: "Settings", href: "/dashboard/settings", i18nKey: "sidebar.settings" },
  { label: "System", href: "/dashboard/system", i18nKey: "sidebar.system" },
  { label: "Traffic Shaping", href: "/dashboard/traffic-shaping", i18nKey: "sidebar.trafficShaping" },
  { label: "Config Backup", href: "/dashboard/backups", i18nKey: "sidebar.backups" },
];

const CONFIG_KEYS: { key: string; label: string }[] = [
  { key: "listen_addr", label: "Listen Address" },
  { key: "quic_listen_addr", label: "QUIC Listen Address" },
  { key: "max_connections", label: "Max Connections" },
  { key: "logging_level", label: "Logging Level" },
  { key: "logging_format", label: "Logging Format" },
  { key: "port_forwarding_enabled", label: "Port Forwarding" },
  { key: "camouflage_enabled", label: "Camouflage" },
  { key: "tls_enabled", label: "TLS" },
];

export function CommandPalette() {
  const { t } = useI18n();
  const router = useRouter();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "k") {
      e.preventDefault();
      setOpen((prev) => !prev);
    }
  }, []);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // Reset query on close
  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  const results = useMemo<SearchResult[]>(() => {
    const q = query.toLowerCase().trim();
    if (!q) return [];

    const matches: SearchResult[] = [];

    // Search pages
    for (const page of PAGES) {
      const localizedLabel = t(page.i18nKey);
      if (
        page.label.toLowerCase().includes(q) ||
        localizedLabel.toLowerCase().includes(q) ||
        page.href.toLowerCase().includes(q)
      ) {
        matches.push({
          id: `page-${page.href}`,
          type: "page",
          label: localizedLabel,
          description: page.href,
          href: page.href,
        });
      }
    }

    // Search clients from query cache
    const clients = queryClient.getQueryData<ClientInfo[]>(["clients"]);
    if (clients) {
      for (const client of clients) {
        const name = client.name ?? client.id;
        if (
          name.toLowerCase().includes(q) ||
          client.id.toLowerCase().includes(q)
        ) {
          matches.push({
            id: `client-${client.id}`,
            type: "client",
            label: name,
            description: client.id,
            href: "/dashboard/clients",
          });
        }
      }
    }

    // Search config keys
    const config = queryClient.getQueryData<ConfigResponse>(["config"]);
    if (config) {
      for (const { key, label } of CONFIG_KEYS) {
        if (
          key.toLowerCase().includes(q) ||
          label.toLowerCase().includes(q)
        ) {
          const value = String((config as unknown as Record<string, unknown>)[key] ?? "");
          matches.push({
            id: `config-${key}`,
            type: "config",
            label: label,
            description: value,
            href: "/dashboard/settings",
          });
        }
      }
    }

    return matches.slice(0, 20);
  }, [query, t, queryClient]);

  const grouped = useMemo(() => {
    const groups: Record<string, SearchResult[]> = {};
    for (const result of results) {
      const key = result.type === "page" ? "Pages" : result.type === "client" ? "Clients" : "Config";
      if (!groups[key]) groups[key] = [];
      groups[key].push(result);
    }
    return groups;
  }, [results]);

  function handleSelect(result: SearchResult) {
    if (result.href) {
      router.push(result.href);
    }
    setOpen(false);
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-lg p-0 gap-0" showCloseButton={false}>
        <DialogTitle className="sr-only">Search</DialogTitle>
        <div className="flex items-center border-b px-3">
          <Search className="h-4 w-4 shrink-0 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("search.placeholder")}
            className="border-0 focus-visible:ring-0 focus-visible:border-transparent"
            autoFocus
          />
          <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground">
            ESC
          </kbd>
        </div>
        <div className="max-h-80 overflow-y-auto p-2">
          {query && results.length === 0 && (
            <p className="py-6 text-center text-sm text-muted-foreground">
              {t("search.noResults")}
            </p>
          )}
          {Object.entries(grouped).map(([group, items]) => (
            <div key={group}>
              <p className="px-2 py-1.5 text-xs font-medium text-muted-foreground">
                {group}
              </p>
              {items.map((result) => (
                <button
                  key={result.id}
                  type="button"
                  onClick={() => handleSelect(result)}
                  className="flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground transition-colors text-left"
                >
                  <span className="font-medium">{result.label}</span>
                  {result.description && (
                    <span className="text-xs text-muted-foreground truncate">
                      {result.description}
                    </span>
                  )}
                </button>
              ))}
            </div>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
