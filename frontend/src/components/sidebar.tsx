"use client";

import { useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  LayoutDashboard,
  Target,
  Wallet,
  TrendingUp,
  Store,
  Settings,
  History,
  FlaskConical,
  Menu,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ConnectionStatus } from "@/components/connection-status";
import { useDashboardStore } from "@/store";

const navItems = [
  { href: "/", label: "Dashboard", icon: LayoutDashboard },
  { href: "/opportunities", label: "Opportunities", icon: Target },
  { href: "/positions", label: "Positions", icon: Wallet },
  { href: "/performance", label: "Performance", icon: TrendingUp },
  { href: "/markets", label: "Markets", icon: Store },
  { href: "/controls", label: "Controls", icon: Settings },
  { href: "/history", label: "History", icon: History },
  { href: "/simulation", label: "Simulation", icon: FlaskConical },
];

export function Sidebar() {
  const pathname = usePathname();
  const status = useDashboardStore((s) => s.status);
  const [mobileOpen, setMobileOpen] = useState(false);

  const mode = status?.mode ?? "Paper";

  const navContent = (
    <>
      {/* Logo */}
      <div className="px-5 py-6">
        <h1 className="text-lg font-bold text-white">
          Polymarket <span className="text-emerald-500">Arb</span>
        </h1>
      </div>

      {/* Nav links */}
      <nav className="flex-1 space-y-1 px-3">
        {navItems.map((item) => {
          const isActive = pathname === item.href;
          return (
            <Link
              key={item.href}
              href={item.href}
              onClick={() => setMobileOpen(false)}
              className={cn(
                "flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors",
                isActive
                  ? "border-l-2 border-emerald-500 bg-zinc-800 text-white"
                  : "text-zinc-400 hover:bg-zinc-800/50 hover:text-white"
              )}
            >
              <item.icon className="h-4 w-4 shrink-0" />
              {item.label}
            </Link>
          );
        })}
      </nav>

      {/* Bottom section */}
      <div className="border-t border-zinc-800 px-5 py-4">
        <div className="space-y-3">
          <ConnectionStatus />
          <Badge
            className={cn(
              "text-xs",
              mode === "Live"
                ? "bg-emerald-500/10 text-emerald-500"
                : "bg-blue-500/10 text-blue-500"
            )}
          >
            {mode} Mode
          </Badge>
        </div>
      </div>
    </>
  );

  return (
    <>
      {/* Mobile hamburger */}
      <div className="fixed left-0 top-0 z-50 flex h-14 w-full items-center bg-zinc-950 px-4 lg:hidden">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setMobileOpen(!mobileOpen)}
          className="text-zinc-400 hover:text-white"
        >
          {mobileOpen ? <X className="h-5 w-5" /> : <Menu className="h-5 w-5" />}
        </Button>
        <h1 className="ml-3 text-lg font-bold text-white">
          Polymarket <span className="text-emerald-500">Arb</span>
        </h1>
      </div>

      {/* Mobile overlay */}
      {mobileOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/60 lg:hidden"
          onClick={() => setMobileOpen(false)}
        />
      )}

      {/* Mobile sidebar */}
      <aside
        className={cn(
          "fixed left-0 top-14 z-40 flex h-[calc(100vh-3.5rem)] w-60 flex-col bg-zinc-950 transition-transform lg:hidden",
          mobileOpen ? "translate-x-0" : "-translate-x-full"
        )}
      >
        {navContent}
      </aside>

      {/* Desktop sidebar */}
      <aside className="hidden w-60 shrink-0 flex-col border-r border-zinc-800 bg-zinc-950 lg:flex">
        {navContent}
      </aside>
    </>
  );
}
