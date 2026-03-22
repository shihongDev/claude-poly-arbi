"use client";

import { useState, useMemo, useCallback } from "react";
import dynamic from "next/dynamic";
import { runStressTest } from "@/lib/api";
import { MONO_FONT } from "@/lib/utils";
import type { StressTestResult, StressScenarioType } from "@/lib/types";

const ReactECharts = dynamic(() => import("echarts-for-react"), { ssr: false });

const SCENARIOS: StressScenarioType[] = [
  "liquidity_shock",
  "correlation_spike",
  "flash_crash",
  "kill_switch_delay",
];

const SCENARIO_LABELS: Record<StressScenarioType, string> = {
  liquidity_shock: "Liquidity Shock",
  correlation_spike: "Correlation Spike",
  flash_crash: "Flash Crash",
  kill_switch_delay: "Kill Switch Delay",
};

const SCENARIO_DESCRIPTIONS: Record<StressScenarioType, string> = {
  liquidity_shock: "50% depth reduction across all active orderbooks",
  correlation_spike: "Correlation increase to 0.85 across correlated pairs",
  flash_crash: "15% adverse move across all positions simultaneously",
  kill_switch_delay: "30 second delay before kill switch activation",
};

function parseDollar(str: string): number {
  return parseFloat(str.replace(/[$,]/g, ""));
}

function formatDollar(value: number): string {
  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";
  return `${sign}$${abs.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

export function VarScenarioComparison() {
  const [results, setResults] = useState<StressTestResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedScenario, setSelectedScenario] =
    useState<StressScenarioType | null>(null);

  const runAllScenarios = useCallback(async () => {
    setLoading(true);
    setSelectedScenario(null);
    try {
      const all = await Promise.all(
        SCENARIOS.map((scenario) =>
          runStressTest({ scenario, params: {} })
        )
      );
      setResults(all);
    } catch {
      // silently fail — chart stays empty
    } finally {
      setLoading(false);
    }
  }, []);

  const selectedResult = useMemo(
    () => results.find((r) => r.scenario === selectedScenario) ?? null,
    [results, selectedScenario]
  );

  const option = useMemo(() => {
    if (results.length === 0) return null;

    const categories = results.map((r) => SCENARIO_LABELS[r.scenario]);
    const impacts = results.map((r) => Math.abs(parseDollar(r.portfolio_impact)));
    const maxLosses = results.map((r) => Math.abs(parseDollar(r.max_loss)));
    const varDeltas = results.map((r) =>
      Math.abs(parseDollar(r.var_after) - parseDollar(r.var_before))
    );

    return {
      backgroundColor: "transparent",
      grid: {
        top: 30,
        right: 30,
        bottom: 20,
        left: 120,
      },
      xAxis: {
        type: "value" as const,
        axisLabel: {
          color: "#6B6B6B",
          fontSize: 10,
          fontFamily: MONO_FONT,
          formatter: (v: number) => `$${v}`,
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        splitLine: { lineStyle: { color: "#F0EEEA" } },
        axisTick: { show: false },
      },
      yAxis: {
        type: "category" as const,
        data: categories,
        axisLabel: {
          color: "#1A1A19",
          fontSize: 11,
          fontFamily: MONO_FONT,
        },
        axisLine: { lineStyle: { color: "#E6E4DF" } },
        axisTick: { show: false },
        triggerEvent: true,
      },
      tooltip: {
        trigger: "axis" as const,
        axisPointer: { type: "shadow" as const },
        backgroundColor: "#FFFFFF",
        borderColor: "#E6E4DF",
        textStyle: {
          color: "#1A1A19",
          fontFamily: MONO_FONT,
        },
        formatter: (params: Array<{ seriesName: string; value: number; axisValue: string }>) => {
          if (!params || params.length === 0) return "";
          const scenarioLabel = params[0].axisValue;
          const scenario = SCENARIOS.find(
            (s) => SCENARIO_LABELS[s] === scenarioLabel
          );
          const desc = scenario
            ? SCENARIO_DESCRIPTIONS[scenario]
            : "";
          const lines = params.map(
            (p) =>
              `<span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${
                p.seriesName === "Impact"
                  ? "#D97706"
                  : p.seriesName === "Max Loss"
                    ? "#B44C3F"
                    : "#6366F1"
              };margin-right:6px;"></span>${p.seriesName}: <strong>${formatDollar(p.value)}</strong>`
          );
          return `<div style="font-size:12px;"><strong>${scenarioLabel}</strong><br/><span style="color:#9B9B9B;font-size:10px;">${desc}</span><br/><br/>${lines.join("<br/>")}</div>`;
        },
      },
      legend: {
        top: 0,
        right: 0,
        textStyle: {
          color: "#6B6B6B",
          fontSize: 10,
          fontFamily: MONO_FONT,
        },
        itemWidth: 12,
        itemHeight: 12,
      },
      series: [
        {
          name: "Impact",
          type: "bar" as const,
          data: impacts,
          barWidth: 12,
          itemStyle: { color: "#D97706", borderRadius: [0, 3, 3, 0] },
        },
        {
          name: "Max Loss",
          type: "bar" as const,
          data: maxLosses,
          barWidth: 12,
          itemStyle: { color: "#B44C3F", borderRadius: [0, 3, 3, 0] },
        },
        {
          name: "VaR Delta",
          type: "bar" as const,
          data: varDeltas,
          barWidth: 12,
          itemStyle: { color: "#6366F1", borderRadius: [0, 3, 3, 0] },
        },
      ],
    };
  }, [results]);

  const onChartClick = useCallback(
    (params: { componentType: string; value?: string; name?: string }) => {
      const label =
        params.componentType === "yAxis" ? params.value : params.name;
      if (!label) return;
      const scenario = SCENARIOS.find((s) => SCENARIO_LABELS[s] === label);
      if (scenario) {
        setSelectedScenario((prev) => (prev === scenario ? null : scenario));
      }
    },
    []
  );

  const onEvents = useMemo(
    () => ({
      click: onChartClick,
    }),
    [onChartClick]
  );

  return (
    <div className="rounded-2xl bg-white p-5">
      <div className="flex items-center justify-between">
        <h3 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Stress Scenario Comparison
        </h3>
        <button
          onClick={runAllScenarios}
          disabled={loading}
          className="rounded-[10px] bg-[#1A1A19] px-3 py-1.5 text-[11px] font-medium text-white transition-opacity hover:opacity-80 disabled:opacity-50"
        >
          {loading ? "Running..." : "Run All Scenarios"}
        </button>
      </div>

      {results.length === 0 ? (
        <div className="flex items-center justify-center" style={{ height: 260 }}>
          <span
            className="text-[13px] text-[#9B9B9B]"
            style={{ fontFamily: MONO_FONT }}
          >
            {loading ? "Running stress tests..." : "Awaiting data..."}
          </span>
        </div>
      ) : (
        <>
          <div className="mt-2" style={{ height: 240 }}>
            <ReactECharts
              option={option!}
              style={{ height: "100%", width: "100%" }}
              opts={{ renderer: "canvas" }}
              onEvents={onEvents}
            />
          </div>

          {selectedResult && (
            <div
              className="mt-3 rounded-xl border border-[#E6E4DF] p-4"
              style={{ fontFamily: MONO_FONT }}
            >
              <div className="mb-2 flex items-center justify-between">
                <span className="text-[13px] font-semibold text-[#1A1A19]">
                  {SCENARIO_LABELS[selectedResult.scenario]}
                </span>
                <button
                  onClick={() => setSelectedScenario(null)}
                  className="text-[11px] text-[#9B9B9B] hover:text-[#1A1A19]"
                >
                  Close
                </button>
              </div>
              <p className="mb-3 text-[11px] text-[#9B9B9B]">
                {selectedResult.details}
              </p>
              <div className="grid grid-cols-2 gap-x-6 gap-y-2 text-[12px] sm:grid-cols-3">
                <div>
                  <span className="text-[#9B9B9B]">Impact</span>
                  <p className="font-medium text-[#D97706]">
                    {selectedResult.portfolio_impact}
                  </p>
                </div>
                <div>
                  <span className="text-[#9B9B9B]">Max Loss</span>
                  <p className="font-medium text-[#B44C3F]">
                    {selectedResult.max_loss}
                  </p>
                </div>
                <div>
                  <span className="text-[#9B9B9B]">VaR Before</span>
                  <p className="font-medium text-[#1A1A19]">
                    {selectedResult.var_before}
                  </p>
                </div>
                <div>
                  <span className="text-[#9B9B9B]">VaR After</span>
                  <p className="font-medium text-[#6366F1]">
                    {selectedResult.var_after}
                  </p>
                </div>
                <div>
                  <span className="text-[#9B9B9B]">Positions at Risk</span>
                  <p className="font-medium text-[#1A1A19]">
                    {selectedResult.positions_at_risk}
                  </p>
                </div>
                <div>
                  <span className="text-[#9B9B9B]">VaR Delta</span>
                  <p className="font-medium text-[#6366F1]">
                    {formatDollar(
                      parseDollar(selectedResult.var_after) -
                        parseDollar(selectedResult.var_before)
                    )}
                  </p>
                </div>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
