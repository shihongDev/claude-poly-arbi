"use client";

import { useDashboardStore } from "@/store";
import { cn } from "@/lib/utils";

const statusConfig = {
  connected: { color: "bg-emerald-500", label: "Connected" },
  connecting: { color: "bg-amber-500", label: "Connecting..." },
  disconnected: { color: "bg-red-500", label: "Disconnected" },
} as const;

export function ConnectionStatus() {
  const wsStatus = useDashboardStore((s) => s.wsStatus);
  const config = statusConfig[wsStatus];

  return (
    <div className="flex items-center gap-2">
      <span className={cn("h-2 w-2 rounded-full", config.color)} />
      <span className="text-xs text-zinc-400">{config.label}</span>
    </div>
  );
}
