"use client";

import { useState, useEffect } from "react";
import { usePathname } from "next/navigation";
import { Sidebar, MobileSidebarContent } from "@/components/layout/sidebar";
import { Header } from "@/components/layout/header";
import { CommandPalette } from "@/components/layout/command-palette";
import { useI18n } from "@/lib/i18n";
import {
  Sheet,
  SheetContent,
  SheetTitle,
} from "@/components/ui/sheet";

const PAGE_TITLE_KEYS: Record<string, string> = {
  "/dashboard": "sidebar.overview",
  "/dashboard/servers": "sidebar.server",
  "/dashboard/clients": "sidebar.clients",
  "/dashboard/routing": "sidebar.routing",
  "/dashboard/logs": "sidebar.logs",
  "/dashboard/settings": "sidebar.settings",
  "/dashboard/system": "sidebar.system",
  "/dashboard/traffic-shaping": "sidebar.trafficShaping",
  "/dashboard/backups": "sidebar.backups",
};

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const { t } = useI18n();

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);

  // Load collapsed state from localStorage
  useEffect(() => {
    const saved = localStorage.getItem("prisma-sidebar-collapsed");
    if (saved === "true") setSidebarCollapsed(true);
  }, []);

  // Close mobile sheet on navigation
  useEffect(() => {
    setMobileOpen(false);
  }, [pathname]);

  // Resolve page title using i18n
  const titleKey =
    Object.entries(PAGE_TITLE_KEYS)
      .sort(([a], [b]) => b.length - a.length)
      .find(([path]) => pathname.startsWith(path))?.[1] ?? "sidebar.overview";

  const title = t(titleKey);

  return (
    <div className="flex h-screen">
      {/* Desktop sidebar */}
      <div className="hidden md:block">
        <Sidebar
          collapsed={sidebarCollapsed}
          onCollapsedChange={(v) => {
            setSidebarCollapsed(v);
            localStorage.setItem("prisma-sidebar-collapsed", String(v));
          }}
        />
      </div>

      {/* Mobile sidebar (Sheet drawer) */}
      <Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
        <SheetContent
          side="left"
          showCloseButton
          className="w-64 bg-zinc-950 p-0"
        >
          <SheetTitle className="sr-only">Navigation</SheetTitle>
          <div className="flex h-14 items-center border-b border-zinc-800 px-6">
            <span className="text-lg font-semibold tracking-tight text-white">
              Prisma
            </span>
          </div>
          <MobileSidebarContent onNavigate={() => setMobileOpen(false)} />
        </SheetContent>
      </Sheet>

      {/* Main content area */}
      <div className="flex-1 flex flex-col overflow-hidden">
        <Header
          title={title}
          onMobileMenuToggle={() => setMobileOpen(true)}
        />
        <main className="flex-1 overflow-y-auto p-6 bg-zinc-50 dark:bg-zinc-950">
          {children}
        </main>
      </div>

      <CommandPalette />
    </div>
  );
}
