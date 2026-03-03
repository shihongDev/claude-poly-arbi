"use client";

import { useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  LayoutDashboard,
  FlaskConical,
  Store,
  Settings,
  History,
  Menu,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ConnectionStatus } from "@/components/connection-status";
import { useDashboardStore } from "@/store";

const navItems = [
  { href: "/", label: "Markets", icon: Store },
  { href: "/dashboard", label: "Portfolio", icon: LayoutDashboard },
  { href: "/opportunities", label: "Playground", icon: FlaskConical },
  { href: "/controls", label: "Controls", icon: Settings },
  { href: "/history", label: "History", icon: History },
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
        <h1 className="text-lg font-bold text-[#1A1A19]">
          Polymarket <span className="text-[#2D6A4F]">Arb</span>
        </h1>
      </div>

      {/* Nav links */}
      <nav className="flex-1 space-y-0.5 px-3">
        {navItems.map((item) => {
          const isActive = pathname === item.href;
          return (
            <Link
              key={item.href}
              href={item.href}
              onClick={() => setMobileOpen(false)}
              className={cn(
                "flex items-center gap-3 rounded-[10px] px-3 py-2.5 text-sm font-medium transition-colors",
                isActive
                  ? "bg-[#DAE9E0] text-[#2D6A4F]"
                  : "text-[#6B6B6B] hover:bg-[#F0EEEA] hover:text-[#1A1A19]"
              )}
            >
              <item.icon className="h-4 w-4 shrink-0" />
              {item.label}
            </Link>
          );
        })}
      </nav>

      {/* Bottom section */}
      <div className="border-t border-[#E6E4DF] px-5 py-4">
        <div className="space-y-3">
          <ConnectionStatus />
          <Badge
            className={cn(
              "text-xs rounded-full",
              mode === "Live"
                ? "bg-[#DAE9E0] text-[#2D6A4F]"
                : "bg-blue-50 text-blue-600"
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
      <div className="fixed left-0 top-0 z-50 flex h-14 w-full items-center bg-white px-4 lg:hidden border-b border-[#E6E4DF]">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setMobileOpen(!mobileOpen)}
          className="text-[#6B6B6B] hover:text-[#1A1A19]"
        >
          {mobileOpen ? <X className="h-5 w-5" /> : <Menu className="h-5 w-5" />}
        </Button>
        <h1 className="ml-3 text-lg font-bold text-[#1A1A19]">
          Polymarket <span className="text-[#2D6A4F]">Arb</span>
        </h1>
      </div>

      {/* Mobile overlay */}
      {mobileOpen && (
        <div
          className="fixed inset-0 z-40 bg-black/20 lg:hidden"
          onClick={() => setMobileOpen(false)}
        />
      )}

      {/* Mobile sidebar */}
      <aside
        className={cn(
          "fixed left-0 top-14 z-40 flex h-[calc(100vh-3.5rem)] w-60 flex-col bg-white transition-transform lg:hidden",
          mobileOpen ? "translate-x-0" : "-translate-x-full"
        )}
      >
        {navContent}
      </aside>

      {/* Desktop sidebar */}
      <aside className="hidden w-60 shrink-0 flex-col border-r border-[#E6E4DF] bg-white lg:flex">
        {navContent}
      </aside>
    </>
  );
}
