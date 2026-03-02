"use client";

import { useState, useEffect, useCallback } from "react";
import {
  ShieldAlert,
  ShieldCheck,
  Activity,
  Save,
  Loader2,
} from "lucide-react";
import { toast, Toaster } from "sonner";
import { useDashboardStore } from "@/store";
import { fetchApi } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { ConnectionStatus } from "@/components/connection-status";
import type { TradingMode } from "@/lib/types";

// -- Config shape returned by GET /api/config --

interface DaemonConfig {
  general: {
    trading_mode: "paper" | "live";
    log_level: "debug" | "info" | "warn" | "error";
  };
  strategy: {
    min_edge_bps: number;
    intra_market_enabled: boolean;
    cross_market_enabled: boolean;
    multi_outcome_enabled: boolean;
  };
  risk: {
    max_position_per_market: number;
    max_total_exposure: number;
    daily_loss_limit: number;
    max_open_orders: number;
  };
  slippage: {
    max_slippage_bps: number;
    order_split_threshold: number;
    prefer_post_only: boolean;
  };
  alerts: {
    drawdown_warning_pct: number;
    drawdown_critical_pct: number;
  };
}

const defaultConfig: DaemonConfig = {
  general: { trading_mode: "paper", log_level: "info" },
  strategy: {
    min_edge_bps: 50,
    intra_market_enabled: true,
    cross_market_enabled: true,
    multi_outcome_enabled: false,
  },
  risk: {
    max_position_per_market: 500,
    max_total_exposure: 5000,
    daily_loss_limit: 200,
    max_open_orders: 20,
  },
  slippage: {
    max_slippage_bps: 30,
    order_split_threshold: 100,
    prefer_post_only: true,
  },
  alerts: {
    drawdown_warning_pct: 5,
    drawdown_critical_pct: 10,
  },
};

// -- Helpers --

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

// -- Page Component --

export default function ControlsPage() {
  const status = useDashboardStore((s) => s.status);
  const killSwitchActive = useDashboardStore((s) => s.killSwitchActive);
  const killSwitchReason = useDashboardStore((s) => s.killSwitchReason);
  const setKillSwitch = useDashboardStore((s) => s.setKillSwitch);
  const wsStatus = useDashboardStore((s) => s.wsStatus);

  // Kill switch form
  const [killReason, setKillReason] = useState("");
  const [killLoading, setKillLoading] = useState(false);

  // Config
  const [config, setConfig] = useState<DaemonConfig>(defaultConfig);
  const [configLoading, setConfigLoading] = useState(true);
  const [saveLoading, setSaveLoading] = useState(false);

  // Load config on mount
  useEffect(() => {
    async function loadConfig() {
      try {
        const data = await fetchApi<DaemonConfig>("/api/config");
        setConfig(data);
      } catch {
        // Use defaults if backend not available
      } finally {
        setConfigLoading(false);
      }
    }
    loadConfig();
  }, []);

  // -- Kill switch handlers --

  const handleKill = useCallback(async () => {
    setKillLoading(true);
    try {
      await fetchApi("/api/kill", {
        method: "POST",
        body: JSON.stringify({ reason: killReason || "Manual kill switch" }),
      });
      setKillSwitch(true, killReason || "Manual kill switch");
      setKillReason("");
      toast.success("Kill switch activated - all trading halted");
    } catch {
      toast.error("Failed to activate kill switch");
    } finally {
      setKillLoading(false);
    }
  }, [killReason, setKillSwitch]);

  const handleResume = useCallback(async () => {
    setKillLoading(true);
    try {
      await fetchApi("/api/resume", { method: "POST" });
      setKillSwitch(false);
      toast.success("Trading resumed");
    } catch {
      toast.error("Failed to resume trading");
    } finally {
      setKillLoading(false);
    }
  }, [setKillSwitch]);

  // -- Config save handler --

  const handleSaveConfig = useCallback(async () => {
    setSaveLoading(true);
    try {
      await fetchApi("/api/config", {
        method: "PUT",
        body: JSON.stringify(config),
      });
      toast.success("Configuration saved");
    } catch {
      toast.error("Failed to save configuration");
    } finally {
      setSaveLoading(false);
    }
  }, [config]);

  // -- Config updaters --

  function updateGeneral<K extends keyof DaemonConfig["general"]>(
    key: K,
    value: DaemonConfig["general"][K]
  ) {
    setConfig((prev) => ({
      ...prev,
      general: { ...prev.general, [key]: value },
    }));
  }

  function updateStrategy<K extends keyof DaemonConfig["strategy"]>(
    key: K,
    value: DaemonConfig["strategy"][K]
  ) {
    setConfig((prev) => ({
      ...prev,
      strategy: { ...prev.strategy, [key]: value },
    }));
  }

  function updateRisk<K extends keyof DaemonConfig["risk"]>(
    key: K,
    value: DaemonConfig["risk"][K]
  ) {
    setConfig((prev) => ({
      ...prev,
      risk: { ...prev.risk, [key]: value },
    }));
  }

  function updateSlippage<K extends keyof DaemonConfig["slippage"]>(
    key: K,
    value: DaemonConfig["slippage"][K]
  ) {
    setConfig((prev) => ({
      ...prev,
      slippage: { ...prev.slippage, [key]: value },
    }));
  }

  function updateAlerts<K extends keyof DaemonConfig["alerts"]>(
    key: K,
    value: DaemonConfig["alerts"][K]
  ) {
    setConfig((prev) => ({
      ...prev,
      alerts: { ...prev.alerts, [key]: value },
    }));
  }

  // -- Derived --

  const mode: TradingMode = status?.mode ?? "Paper";
  const uptimeStr = status ? formatUptime(status.uptime_secs) : "--:--:--";
  const marketCount = status?.market_count ?? 0;

  return (
    <div className="space-y-6">
      <Toaster
        theme="dark"
        position="top-right"
        toastOptions={{
          style: {
            background: "#18181b",
            border: "1px solid #3f3f46",
            color: "#fafafa",
          },
        }}
      />

      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-white">Controls</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Mission-critical trading controls and daemon configuration
        </p>
      </div>

      {/* =============================== */}
      {/* Section 1: Kill Switch           */}
      {/* =============================== */}
      <Card
        className={cn(
          "border-2 bg-zinc-900",
          killSwitchActive
            ? "border-red-500/60"
            : "border-emerald-500/40"
        )}
      >
        <CardHeader>
          <div className="flex items-center gap-3">
            {killSwitchActive ? (
              <ShieldAlert className="h-6 w-6 text-red-500" />
            ) : (
              <ShieldCheck className="h-6 w-6 text-emerald-500" />
            )}
            <div>
              <CardTitle
                className={cn(
                  "text-lg",
                  killSwitchActive ? "text-red-500" : "text-emerald-500"
                )}
              >
                {killSwitchActive ? "TRADING HALTED" : "Trading Active"}
              </CardTitle>
              <CardDescription className="text-zinc-400">
                {killSwitchActive
                  ? "All trading operations are suspended"
                  : "The kill switch will immediately halt all trading activity"}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {killSwitchActive ? (
            <div className="space-y-4">
              {/* Show reason + timestamp */}
              <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4">
                <p className="text-sm font-medium text-red-400">Reason</p>
                <p className="mt-1 text-sm text-zinc-300">
                  {killSwitchReason || "No reason provided"}
                </p>
                {status && (
                  <p className="mt-2 text-xs text-zinc-500">
                    Uptime at halt: {uptimeStr}
                  </p>
                )}
              </div>
              {/* Resume button */}
              <Button
                onClick={handleResume}
                disabled={killLoading}
                className="h-14 w-full bg-emerald-600 text-base font-bold text-white hover:bg-emerald-700"
              >
                {killLoading ? (
                  <Loader2 className="mr-2 h-5 w-5 animate-spin" />
                ) : (
                  <ShieldCheck className="mr-2 h-5 w-5" />
                )}
                Resume Trading
              </Button>
            </div>
          ) : (
            <div className="space-y-4">
              {/* Reason input */}
              <div className="space-y-2">
                <Label htmlFor="kill-reason" className="text-zinc-400">
                  Reason (optional)
                </Label>
                <Input
                  id="kill-reason"
                  placeholder="e.g. Unusual market conditions, investigating anomaly..."
                  value={killReason}
                  onChange={(e) => setKillReason(e.target.value)}
                  className="border-zinc-700 bg-zinc-800 text-zinc-200 placeholder:text-zinc-600"
                />
              </div>
              {/* Kill button */}
              <Button
                onClick={handleKill}
                disabled={killLoading}
                className="h-14 w-full bg-red-600 text-base font-bold text-white hover:bg-red-700"
              >
                {killLoading ? (
                  <Loader2 className="mr-2 h-5 w-5 animate-spin" />
                ) : (
                  <ShieldAlert className="mr-2 h-5 w-5" />
                )}
                KILL SWITCH &mdash; Stop All Trading
              </Button>
            </div>
          )}
        </CardContent>
      </Card>

      {/* =============================== */}
      {/* Section 2: Daemon Status         */}
      {/* =============================== */}
      <Card className="border-zinc-800 bg-zinc-900">
        <CardHeader>
          <div className="flex items-center gap-3">
            <Activity className="h-5 w-5 text-zinc-400" />
            <CardTitle className="text-white">Daemon Status</CardTitle>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {/* Mode */}
            <div className="space-y-1">
              <p className="text-xs font-medium uppercase tracking-wider text-zinc-500">
                Mode
              </p>
              <Badge
                className={cn(
                  "text-xs",
                  mode === "Live"
                    ? "bg-emerald-500/10 text-emerald-500"
                    : "bg-blue-500/10 text-blue-500"
                )}
              >
                {mode}
              </Badge>
            </div>

            {/* Uptime */}
            <div className="space-y-1">
              <p className="text-xs font-medium uppercase tracking-wider text-zinc-500">
                Uptime
              </p>
              <p
                className="text-lg font-semibold text-white"
                style={{ fontFamily: "var(--font-mono)" }}
              >
                {uptimeStr}
              </p>
            </div>

            {/* Market Count */}
            <div className="space-y-1">
              <p className="text-xs font-medium uppercase tracking-wider text-zinc-500">
                Markets
              </p>
              <p
                className="text-lg font-semibold text-white"
                style={{ fontFamily: "var(--font-mono)" }}
              >
                {marketCount}
              </p>
            </div>

            {/* WebSocket */}
            <div className="space-y-1">
              <p className="text-xs font-medium uppercase tracking-wider text-zinc-500">
                WebSocket
              </p>
              <ConnectionStatus />
            </div>
          </div>
        </CardContent>
      </Card>

      {/* =============================== */}
      {/* Section 3: Configuration Editor  */}
      {/* =============================== */}
      <div className="space-y-4">
        <div>
          <h2 className="text-lg font-semibold text-white">Configuration</h2>
          <p className="text-sm text-zinc-400">
            Daemon parameters. Changes take effect on save.
          </p>
        </div>

        {configLoading ? (
          <div className="flex items-center justify-center rounded-lg border border-zinc-800 bg-zinc-900 py-16">
            <Loader2 className="h-6 w-6 animate-spin text-zinc-500" />
            <span className="ml-3 text-sm text-zinc-500">
              Loading configuration...
            </span>
          </div>
        ) : (
          <div className="space-y-4">
            {/* General */}
            <Card className="border-zinc-800 bg-zinc-900">
              <CardHeader>
                <CardTitle className="text-sm font-medium uppercase tracking-wider text-zinc-400">
                  General
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid gap-6 sm:grid-cols-2">
                  {/* Trading Mode */}
                  <div className="space-y-2">
                    <Label className="text-zinc-400">Trading Mode</Label>
                    <Select
                      value={config.general.trading_mode}
                      onValueChange={(v) =>
                        updateGeneral(
                          "trading_mode",
                          v as "paper" | "live"
                        )
                      }
                    >
                      <SelectTrigger className="w-full border-zinc-700 bg-zinc-800 text-zinc-200">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="paper">Paper</SelectItem>
                        <SelectItem value="live">Live</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>

                  {/* Log Level */}
                  <div className="space-y-2">
                    <Label className="text-zinc-400">Log Level</Label>
                    <Select
                      value={config.general.log_level}
                      onValueChange={(v) =>
                        updateGeneral(
                          "log_level",
                          v as "debug" | "info" | "warn" | "error"
                        )
                      }
                    >
                      <SelectTrigger className="w-full border-zinc-700 bg-zinc-800 text-zinc-200">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="debug">Debug</SelectItem>
                        <SelectItem value="info">Info</SelectItem>
                        <SelectItem value="warn">Warn</SelectItem>
                        <SelectItem value="error">Error</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              </CardContent>
            </Card>

            <Separator className="bg-zinc-800" />

            {/* Strategy */}
            <Card className="border-zinc-800 bg-zinc-900">
              <CardHeader>
                <CardTitle className="text-sm font-medium uppercase tracking-wider text-zinc-400">
                  Strategy
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid gap-6 sm:grid-cols-2">
                  {/* Min Edge */}
                  <div className="space-y-2">
                    <Label htmlFor="min-edge" className="text-zinc-400">
                      Min Edge (bps)
                    </Label>
                    <Input
                      id="min-edge"
                      type="number"
                      min={0}
                      value={config.strategy.min_edge_bps}
                      onChange={(e) =>
                        updateStrategy(
                          "min_edge_bps",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>

                  {/* Spacer for alignment */}
                  <div />

                  {/* Intra-Market */}
                  <div className="flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-800/50 px-4 py-3">
                    <Label className="text-zinc-300">
                      Intra-Market Enabled
                    </Label>
                    <Switch
                      checked={config.strategy.intra_market_enabled}
                      onCheckedChange={(v) =>
                        updateStrategy("intra_market_enabled", v)
                      }
                    />
                  </div>

                  {/* Cross-Market */}
                  <div className="flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-800/50 px-4 py-3">
                    <Label className="text-zinc-300">
                      Cross-Market Enabled
                    </Label>
                    <Switch
                      checked={config.strategy.cross_market_enabled}
                      onCheckedChange={(v) =>
                        updateStrategy("cross_market_enabled", v)
                      }
                    />
                  </div>

                  {/* Multi-Outcome */}
                  <div className="flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-800/50 px-4 py-3">
                    <Label className="text-zinc-300">
                      Multi-Outcome Enabled
                    </Label>
                    <Switch
                      checked={config.strategy.multi_outcome_enabled}
                      onCheckedChange={(v) =>
                        updateStrategy("multi_outcome_enabled", v)
                      }
                    />
                  </div>
                </div>
              </CardContent>
            </Card>

            <Separator className="bg-zinc-800" />

            {/* Risk */}
            <Card className="border-zinc-800 bg-zinc-900">
              <CardHeader>
                <CardTitle className="text-sm font-medium uppercase tracking-wider text-zinc-400">
                  Risk
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid gap-6 sm:grid-cols-2">
                  <div className="space-y-2">
                    <Label
                      htmlFor="max-position"
                      className="text-zinc-400"
                    >
                      Max Position Per Market ($)
                    </Label>
                    <Input
                      id="max-position"
                      type="number"
                      min={0}
                      value={config.risk.max_position_per_market}
                      onChange={(e) =>
                        updateRisk(
                          "max_position_per_market",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label
                      htmlFor="max-exposure"
                      className="text-zinc-400"
                    >
                      Max Total Exposure ($)
                    </Label>
                    <Input
                      id="max-exposure"
                      type="number"
                      min={0}
                      value={config.risk.max_total_exposure}
                      onChange={(e) =>
                        updateRisk(
                          "max_total_exposure",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label
                      htmlFor="daily-loss-limit"
                      className="text-zinc-400"
                    >
                      Daily Loss Limit ($)
                    </Label>
                    <Input
                      id="daily-loss-limit"
                      type="number"
                      min={0}
                      value={config.risk.daily_loss_limit}
                      onChange={(e) =>
                        updateRisk(
                          "daily_loss_limit",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label
                      htmlFor="max-open-orders"
                      className="text-zinc-400"
                    >
                      Max Open Orders
                    </Label>
                    <Input
                      id="max-open-orders"
                      type="number"
                      min={0}
                      value={config.risk.max_open_orders}
                      onChange={(e) =>
                        updateRisk(
                          "max_open_orders",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                </div>
              </CardContent>
            </Card>

            <Separator className="bg-zinc-800" />

            {/* Slippage */}
            <Card className="border-zinc-800 bg-zinc-900">
              <CardHeader>
                <CardTitle className="text-sm font-medium uppercase tracking-wider text-zinc-400">
                  Slippage
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid gap-6 sm:grid-cols-2">
                  <div className="space-y-2">
                    <Label
                      htmlFor="max-slippage"
                      className="text-zinc-400"
                    >
                      Max Slippage (bps)
                    </Label>
                    <Input
                      id="max-slippage"
                      type="number"
                      min={0}
                      value={config.slippage.max_slippage_bps}
                      onChange={(e) =>
                        updateSlippage(
                          "max_slippage_bps",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label
                      htmlFor="order-split"
                      className="text-zinc-400"
                    >
                      Order Split Threshold
                    </Label>
                    <Input
                      id="order-split"
                      type="number"
                      min={0}
                      value={config.slippage.order_split_threshold}
                      onChange={(e) =>
                        updateSlippage(
                          "order_split_threshold",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                  <div className="flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-800/50 px-4 py-3">
                    <Label className="text-zinc-300">
                      Prefer Post-Only
                    </Label>
                    <Switch
                      checked={config.slippage.prefer_post_only}
                      onCheckedChange={(v) =>
                        updateSlippage("prefer_post_only", v)
                      }
                    />
                  </div>
                </div>
              </CardContent>
            </Card>

            <Separator className="bg-zinc-800" />

            {/* Alerts */}
            <Card className="border-zinc-800 bg-zinc-900">
              <CardHeader>
                <CardTitle className="text-sm font-medium uppercase tracking-wider text-zinc-400">
                  Alerts
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid gap-6 sm:grid-cols-2">
                  <div className="space-y-2">
                    <Label
                      htmlFor="dd-warning"
                      className="text-zinc-400"
                    >
                      Drawdown Warning (%)
                    </Label>
                    <Input
                      id="dd-warning"
                      type="number"
                      min={0}
                      max={100}
                      value={config.alerts.drawdown_warning_pct}
                      onChange={(e) =>
                        updateAlerts(
                          "drawdown_warning_pct",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label
                      htmlFor="dd-critical"
                      className="text-zinc-400"
                    >
                      Drawdown Critical (%)
                    </Label>
                    <Input
                      id="dd-critical"
                      type="number"
                      min={0}
                      max={100}
                      value={config.alerts.drawdown_critical_pct}
                      onChange={(e) =>
                        updateAlerts(
                          "drawdown_critical_pct",
                          Number(e.target.value)
                        )
                      }
                      className="border-zinc-700 bg-zinc-800 text-zinc-200"
                    />
                  </div>
                </div>
              </CardContent>
            </Card>

            {/* Save button */}
            <div className="flex justify-end pt-2">
              <Button
                onClick={handleSaveConfig}
                disabled={saveLoading}
                className="h-11 bg-emerald-600 px-8 text-sm font-semibold text-white hover:bg-emerald-700"
              >
                {saveLoading ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Save className="mr-2 h-4 w-4" />
                )}
                Save Configuration
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
