"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  LayoutDashboard,
  Server,
  Users,
  Route,
  ScrollText,
  Settings,
  Monitor,
  Activity,
  Archive,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { useI18n } from "@/lib/i18n";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from "@/components/ui/tooltip";

const navItems = [
  { labelKey: "sidebar.overview", href: "/dashboard/", icon: LayoutDashboard, exact: true },
  { labelKey: "sidebar.server", href: "/dashboard/servers/", icon: Server },
  { labelKey: "sidebar.clients", href: "/dashboard/clients/", icon: Users },
  { labelKey: "sidebar.routing", href: "/dashboard/routing/", icon: Route },
  { labelKey: "sidebar.logs", href: "/dashboard/logs/", icon: ScrollText },
  { labelKey: "sidebar.settings", href: "/dashboard/settings/", icon: Settings },
  { labelKey: "sidebar.system", href: "/dashboard/system/", icon: Monitor },
  { labelKey: "sidebar.trafficShaping", href: "/dashboard/traffic-shaping/", icon: Activity },
  { labelKey: "sidebar.backups", href: "/dashboard/backups/", icon: Archive },
];

interface SidebarProps {
  collapsed?: boolean;
  onCollapsedChange?: (collapsed: boolean) => void;
}

export function Sidebar({ collapsed: controlledCollapsed, onCollapsedChange }: SidebarProps) {
  const pathname = usePathname();
  const { t } = useI18n();

  const [internalCollapsed, setInternalCollapsed] = useState(false);

  // Use controlled value if provided, otherwise internal state
  const collapsed = controlledCollapsed ?? internalCollapsed;

  useEffect(() => {
    if (controlledCollapsed === undefined) {
      const saved = localStorage.getItem("prisma-sidebar-collapsed");
      if (saved === "true") setInternalCollapsed(true);
    }
  }, [controlledCollapsed]);

  const toggleCollapsed = useCallback(() => {
    const next = !collapsed;
    if (onCollapsedChange) {
      onCollapsedChange(next);
    } else {
      setInternalCollapsed(next);
    }
    localStorage.setItem("prisma-sidebar-collapsed", String(next));
  }, [collapsed, onCollapsedChange]);

  return (
    <TooltipProvider>
      <aside
        className={`flex h-screen flex-col bg-zinc-950 text-white transition-all duration-200 ${
          collapsed ? "w-16" : "w-64"
        }`}
      >
        {/* Logo / Brand */}
        <div className="flex h-14 items-center border-b border-zinc-800 px-4">
          {!collapsed && (
            <span className="text-lg font-semibold tracking-tight">Prisma</span>
          )}
          <Button
            variant="ghost"
            size="icon-sm"
            className={`text-zinc-400 hover:text-white hover:bg-zinc-800 ${
              collapsed ? "mx-auto" : "ml-auto"
            }`}
            onClick={toggleCollapsed}
          >
            {collapsed ? (
              <PanelLeftOpen className="h-4 w-4" />
            ) : (
              <PanelLeftClose className="h-4 w-4" />
            )}
          </Button>
        </div>

        {/* Navigation */}
        <nav className="flex-1 space-y-1 px-2 py-4 overflow-y-auto">
          {navItems.map(({ labelKey, href, icon: Icon, exact }) => {
            const base = href.replace(/\/$/, "");
            const isActive = exact
              ? pathname === base || pathname === base + "/"
              : pathname === base || pathname.startsWith(base + "/");

            const label = t(labelKey);

            const linkContent = (
              <Link
                key={href}
                href={href}
                className={`flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors ${
                  collapsed ? "justify-center px-2" : ""
                } ${
                  isActive
                    ? "bg-zinc-800 text-white"
                    : "text-zinc-400 hover:bg-zinc-900 hover:text-white"
                }`}
              >
                <Icon className="h-4 w-4 shrink-0" />
                {!collapsed && <span className="truncate">{label}</span>}
              </Link>
            );

            if (collapsed) {
              return (
                <Tooltip key={href}>
                  <TooltipTrigger render={<div />}>
                    {linkContent}
                  </TooltipTrigger>
                  <TooltipContent side="right" sideOffset={8}>
                    {label}
                  </TooltipContent>
                </Tooltip>
              );
            }

            return <div key={href}>{linkContent}</div>;
          })}
        </nav>
      </aside>
    </TooltipProvider>
  );
}

/** Mobile sidebar content (used inside Sheet) */
export function MobileSidebarContent({ onNavigate }: { onNavigate?: () => void }) {
  const pathname = usePathname();
  const { t } = useI18n();

  return (
    <nav className="flex-1 space-y-1 px-3 py-4">
      {navItems.map(({ labelKey, href, icon: Icon, exact }) => {
        const base = href.replace(/\/$/, "");
        const isActive = exact
          ? pathname === base || pathname === base + "/"
          : pathname === base || pathname.startsWith(base + "/");

        return (
          <Link
            key={href}
            href={href}
            onClick={onNavigate}
            className={`flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors ${
              isActive
                ? "bg-zinc-800 text-white"
                : "text-zinc-400 hover:bg-zinc-900 hover:text-white"
            }`}
          >
            <Icon className="h-4 w-4" />
            {t(labelKey)}
          </Link>
        );
      })}
    </nav>
  );
}
