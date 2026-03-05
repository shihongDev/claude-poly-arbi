"use client";

import { useState, useCallback } from "react";
import {
  Loader2,
  AlertTriangle,
  ArrowRight,
  ShieldAlert,
  Zap,
  TrendingDown,
  Timer,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import { runStressTest } from "@/lib/api";
import type { StressScenarioType, StressTestResult } from "@/lib/types";

const MONO = { fontFamily: "var(--font-jetbrains-mono)" };

// ---------------------------------------------------------------------------
// Scenario definitions
// ---------------------------------------------------------------------------

interface ScenarioConfig {
  type: StressScenarioType;
  label: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  paramLabel: string;
  paramKey: string;
  min: number;
  max: number;
  step: number;
  defaultValue: number;
  unit: string;
  formatValue: (v: number) => string;
}

const SCENARIOS: ScenarioConfig[] = [
  {
    type: "liquidity_shock",
    label: "Liquidity Shock",
    description: "Simulate sudden depth reduction across active orderbooks",
    icon: TrendingDown,
    paramLabel: "Depth Reduction",
    paramKey: "depth_reduction_pct",
    min: 10,
    max: 90,
    step: 5,
    defaultValue: 50,
    unit: "%",
    formatValue: (v) => `${v}%`,
  },
  {
    type: "correlation_spike",
    label: "Correlation Spike",
    description: "Simulate sudden increase in cross-market correlations",
    icon: Zap,
    paramLabel: "Correlation",
    paramKey: "correlation",
    min: 50,
    max: 100,
    step: 5,
    defaultValue: 85,
    unit: "",
    formatValue: (v) => (v / 100).toFixed(2),
  },
  {
    type: "flash_crash",
    label: "Flash Crash",
    description: "Simulate rapid adverse price movement across positions",
    icon: ShieldAlert,
    paramLabel: "Adverse Move",
    paramKey: "adverse_move_pct",
    min: 5,
    max: 30,
    step: 1,
    defaultValue: 15,
    unit: "%",
    formatValue: (v) => `${v}%`,
  },
  {
    type: "kill_switch_delay",
    label: "Kill Switch Delay",
    description: "Simulate delayed kill switch activation under adverse conditions",
    icon: Timer,
    paramLabel: "Delay",
    paramKey: "delay_seconds",
    min: 5,
    max: 60,
    step: 5,
    defaultValue: 30,
    unit: "s",
    formatValue: (v) => `${v}s`,
  },
];

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function ScenarioSelector({
  selected,
  onSelect,
}: {
  selected: StressScenarioType;
  onSelect: (s: StressScenarioType) => void;
}) {
  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      {SCENARIOS.map((s) => {
        const isActive = selected === s.type;
        const Icon = s.icon;
        return (
          <button
            key={s.type}
            onClick={() => onSelect(s.type)}
            className={cn(
              "flex flex-col items-start gap-2 rounded-2xl border p-4 text-left transition-all cursor-pointer",
              isActive
                ? "border-[#2D6A4F] bg-[#DAE9E0]/30"
                : "border-[#E6E4DF] bg-white hover:border-[#9B9B9B]"
            )}
          >
            <div className="flex items-center gap-2">
              <Icon
                className={cn(
                  "h-4 w-4",
                  isActive ? "text-[#2D6A4F]" : "text-[#9B9B9B]"
                )}
              />
              <span
                className={cn(
                  "text-sm font-medium",
                  isActive ? "text-[#2D6A4F]" : "text-[#1A1A19]"
                )}
              >
                {s.label}
              </span>
            </div>
            <span className="text-xs text-[#9B9B9B]">{s.description}</span>
          </button>
        );
      })}
    </div>
  );
}

function ParameterSlider({
  config,
  value,
  onChange,
}: {
  config: ScenarioConfig;
  value: number;
  onChange: (v: number) => void;
}) {
  const pct = ((value - config.min) / (config.max - config.min)) * 100;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label className="text-[#1A1A19]">{config.paramLabel}</Label>
        <span className="text-sm font-semibold text-[#1A1A19]" style={MONO}>
          {config.formatValue(value)}
        </span>
      </div>
      <div className="relative">
        <input
          type="range"
          min={config.min}
          max={config.max}
          step={config.step}
          value={value}
          onChange={(e) => onChange(parseInt(e.target.value))}
          className="w-full appearance-none bg-transparent cursor-pointer
            [&::-webkit-slider-runnable-track]:h-2 [&::-webkit-slider-runnable-track]:rounded-full [&::-webkit-slider-runnable-track]:bg-[#F0EEEA]
            [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:h-5 [&::-webkit-slider-thumb]:w-5 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-[#2D6A4F] [&::-webkit-slider-thumb]:mt-[-6px] [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-white [&::-webkit-slider-thumb]:shadow-md
            [&::-moz-range-track]:h-2 [&::-moz-range-track]:rounded-full [&::-moz-range-track]:bg-[#F0EEEA]
            [&::-moz-range-thumb]:h-5 [&::-moz-range-thumb]:w-5 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-[#2D6A4F] [&::-moz-range-thumb]:border-2 [&::-moz-range-thumb]:border-white [&::-moz-range-thumb]:shadow-md"
        />
        {/* Fill track */}
        <div
          className="pointer-events-none absolute left-0 top-[9px] h-2 rounded-full bg-[#2D6A4F]/20"
          style={{ width: `${pct}%` }}
        />
      </div>
      <div className="flex justify-between text-[10px] text-[#9B9B9B]" style={MONO}>
        <span>
          {config.min}
          {config.unit}
        </span>
        <span>
          {config.max}
          {config.unit}
        </span>
      </div>
    </div>
  );
}

function ResultMetric({
  label,
  value,
  variant = "neutral",
}: {
  label: string;
  value: string | number;
  variant?: "positive" | "negative" | "neutral";
}) {
  const color =
    variant === "negative"
      ? "text-[#B44C3F]"
      : variant === "positive"
        ? "text-[#2D6A4F]"
        : "text-[#1A1A19]";
  return (
    <div>
      <p className="text-xs text-[#9B9B9B]">{label}</p>
      <p className={cn("mt-1 text-lg font-semibold", color)} style={MONO}>
        {value}
      </p>
    </div>
  );
}

function ResultsComparison({ result }: { result: StressTestResult }) {
  return (
    <div className="space-y-4">
      {/* Before / After cards */}
      <div className="grid gap-4 md:grid-cols-2">
        {/* Before */}
        <div className="rounded-2xl border border-[#E6E4DF] bg-white p-5">
          <div className="flex items-center gap-2 mb-4">
            <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
              Before
            </Badge>
            <span className="text-xs text-[#9B9B9B]">Current portfolio state</span>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <ResultMetric label="VaR 95%" value={result.var_before} variant="neutral" />
            <ResultMetric label="Positions at Risk" value="0" variant="neutral" />
            <ResultMetric label="Portfolio Impact" value="$0.00" variant="neutral" />
            <ResultMetric label="Max Loss" value="$0.00" variant="neutral" />
          </div>
        </div>

        {/* After */}
        <div className="rounded-2xl border border-[#B44C3F]/20 bg-[#FDF6F5] p-5">
          <div className="flex items-center gap-2 mb-4">
            <Badge className="bg-[#F5E0DD] text-[#B44C3F] text-[10px]">
              After
            </Badge>
            <span className="text-xs text-[#9B9B9B]">Stressed portfolio state</span>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <ResultMetric
              label="VaR 95%"
              value={result.var_after}
              variant="negative"
            />
            <ResultMetric
              label="Positions at Risk"
              value={result.positions_at_risk}
              variant="negative"
            />
            <ResultMetric
              label="Portfolio Impact"
              value={result.portfolio_impact}
              variant="negative"
            />
            <ResultMetric
              label="Max Loss"
              value={result.max_loss}
              variant="negative"
            />
          </div>
        </div>
      </div>

      {/* VaR transition arrow */}
      <div className="rounded-2xl bg-white border border-[#E6E4DF] p-5">
        <div className="flex items-center justify-center gap-4">
          <div className="text-center">
            <p className="text-xs text-[#9B9B9B]">VaR Before</p>
            <p className="mt-1 text-xl font-semibold text-[#1A1A19]" style={MONO}>
              {result.var_before}
            </p>
          </div>
          <ArrowRight className="h-6 w-6 text-[#B44C3F]" />
          <div className="text-center">
            <p className="text-xs text-[#9B9B9B]">VaR After</p>
            <p className="mt-1 text-xl font-semibold text-[#B44C3F]" style={MONO}>
              {result.var_after}
            </p>
          </div>
          <div className="ml-4 text-center">
            <p className="text-xs text-[#9B9B9B]">Change</p>
            <p className="mt-1 text-xl font-semibold text-[#B44C3F]" style={MONO}>
              {(() => {
                const before = parseFloat(result.var_before.replace(/[$,]/g, ""));
                const after = parseFloat(result.var_after.replace(/[$,]/g, ""));
                const change = after - before;
                const prefix = change >= 0 ? "+" : "";
                return `${prefix}$${Math.abs(change).toFixed(2)}`;
              })()}
            </p>
          </div>
        </div>
      </div>

      {/* Details */}
      <div className="rounded-lg border border-[#E6E4DF] bg-[#F8F7F4] p-4">
        <p className="text-sm text-[#6B6B6B]">{result.details}</p>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page Component
// ---------------------------------------------------------------------------

export default function StressTestPage() {
  const [selectedScenario, setSelectedScenario] =
    useState<StressScenarioType>("liquidity_shock");
  const [paramValues, setParamValues] = useState<Record<string, number>>({
    liquidity_shock: 50,
    correlation_spike: 85,
    flash_crash: 15,
    kill_switch_delay: 30,
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<StressTestResult | null>(null);

  const currentConfig = SCENARIOS.find((s) => s.type === selectedScenario)!;
  const currentParamValue = paramValues[selectedScenario] ?? currentConfig.defaultValue;

  const handleParamChange = useCallback(
    (value: number) => {
      setParamValues((prev) => ({ ...prev, [selectedScenario]: value }));
    },
    [selectedScenario]
  );

  const handleRun = useCallback(async () => {
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const scenarioConfig = SCENARIOS.find((s) => s.type === selectedScenario)!;
      const data = await runStressTest({
        scenario: selectedScenario,
        params: {
          [scenarioConfig.paramKey]:
            scenarioConfig.type === "correlation_spike"
              ? currentParamValue / 100
              : currentParamValue,
        },
      });
      setResult(data);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "An unknown error occurred"
      );
    } finally {
      setLoading(false);
    }
  }, [selectedScenario, currentParamValue]);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-[#1A1A19]">Stress Test</h1>
        <p className="mt-1 text-sm text-[#6B6B6B]">
          Run scenario analysis to evaluate portfolio resilience under adverse
          conditions
        </p>
      </div>

      {/* Scenario Selector */}
      <div className="space-y-3">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Select Scenario
        </h2>
        <ScenarioSelector
          selected={selectedScenario}
          onSelect={(s) => {
            setSelectedScenario(s);
            setResult(null);
            setError(null);
          }}
        />
      </div>

      {/* Parameter Configuration */}
      <div className="rounded-2xl bg-white p-6">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Parameters
        </h2>
        <div className="mt-4 max-w-lg">
          <ParameterSlider
            config={currentConfig}
            value={currentParamValue}
            onChange={handleParamChange}
          />
        </div>

        <div className="mt-6">
          <Button
            onClick={handleRun}
            disabled={loading}
            className="bg-[#2D6A4F] text-white hover:bg-[#245840] disabled:bg-[#E6E4DF] disabled:text-[#9B9B9B]"
          >
            {loading && <Loader2 className="h-4 w-4 animate-spin" />}
            {loading ? "Running Stress Test..." : "Run Stress Test"}
          </Button>
        </div>
      </div>

      {/* Error state */}
      {error && (
        <div className="rounded-lg border border-[#B44C3F]/30 bg-[#F5E0DD] p-5">
          <div className="flex items-start gap-3">
            <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-[#B44C3F]" />
            <div>
              <h3 className="text-sm font-medium text-[#B44C3F]">
                Stress Test Failed
              </h3>
              <p className="mt-1 text-sm text-[#B44C3F]/80">{error}</p>
            </div>
          </div>
        </div>
      )}

      {/* Loading skeleton */}
      {loading && (
        <div className="space-y-4">
          <div className="grid gap-4 md:grid-cols-2">
            <div className="h-52 animate-pulse rounded-2xl bg-white" />
            <div className="h-52 animate-pulse rounded-2xl bg-white" />
          </div>
          <div className="h-24 animate-pulse rounded-2xl bg-white" />
        </div>
      )}

      {/* Results */}
      {result && !loading && (
        <div className="space-y-4">
          <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
            Results
          </h2>
          <ResultsComparison result={result} />
        </div>
      )}
    </div>
  );
}
