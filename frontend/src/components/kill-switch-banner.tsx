"use client";

import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useDashboardStore } from "@/store";
import { fetchApi } from "@/lib/api";

export function KillSwitchBanner() {
  const killSwitchActive = useDashboardStore((s) => s.killSwitchActive);
  const killSwitchReason = useDashboardStore((s) => s.killSwitchReason);

  if (!killSwitchActive) return null;

  const handleResume = async () => {
    try {
      await fetchApi("/api/resume", { method: "POST" });
    } catch {
      /* toast error in future */
    }
  };

  return (
    <div className="flex w-full items-center justify-between bg-[#B44C3F] px-6 py-3">
      <div className="flex items-center gap-3">
        <AlertTriangle className="h-5 w-5 text-white" />
        <span className="text-sm font-semibold text-white">
          KILL SWITCH ACTIVE
        </span>
        {killSwitchReason && (
          <span className="text-sm text-white/80">
            &mdash; {killSwitchReason}
          </span>
        )}
      </div>
      <Button
        variant="outline"
        size="sm"
        onClick={handleResume}
        className="border-white/30 bg-transparent text-white hover:bg-[#9E3F33] hover:text-white"
      >
        Resume Trading
      </Button>
    </div>
  );
}
