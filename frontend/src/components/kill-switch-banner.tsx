"use client";

import { useState } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useDashboardStore } from "@/store";
import { fetchApi } from "@/lib/api";

export function KillSwitchBanner() {
  const killSwitchActive = useDashboardStore((s) => s.killSwitchActive);
  const killSwitchReason = useDashboardStore((s) => s.killSwitchReason);
  const setKillSwitch = useDashboardStore((s) => s.setKillSwitch);
  const [resuming, setResuming] = useState(false);

  if (!killSwitchActive) return null;

  const handleResume = async () => {
    setResuming(true);
    try {
      await fetchApi("/api/resume", { method: "POST" });
      setKillSwitch(false);
    } catch (error) {
      console.error("Failed to resume trading:", error);
    } finally {
      setResuming(false);
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
        disabled={resuming}
        className="border-white/30 bg-transparent text-white hover:bg-[#9E3F33] hover:text-white"
      >
        {resuming && <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />}
        {resuming ? "Resuming..." : "Resume Trading"}
      </Button>
    </div>
  );
}
