import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatDecimal(value: string | null, decimals = 2): string {
  if (!value) return "\u2014";
  return parseFloat(value).toFixed(decimals);
}

export function formatBps(value: string): string {
  return `${(parseFloat(value) * 10000).toFixed(0)} bps`;
}

export function formatUsd(value: string | null): string {
  if (!value) return "\u2014";
  const num = parseFloat(value);
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
  }).format(num);
}

export function formatPnl(value: string): string {
  const num = parseFloat(value);
  const prefix = num >= 0 ? "+" : "";
  return `${prefix}${formatUsd(value)}`;
}

export function formatPercent(value: number): string {
  return `${value.toFixed(2)}%`;
}

export function formatSpreadBps(spread: string | number | null): string {
  if (spread === null || spread === undefined) return "\u2014";
  const num = typeof spread === "string" ? parseFloat(spread) : spread;
  if (isNaN(num)) return "\u2014";
  return `${(num * 10000).toFixed(0)} bps`;
}

export function formatPriceChange(change: string | null): {
  text: string;
  positive: boolean | null;
} {
  if (!change) return { text: "\u2014", positive: null };
  const pct = parseFloat(change) * 100;
  if (isNaN(pct)) return { text: "\u2014", positive: null };
  return {
    text: `${pct >= 0 ? "+" : ""}${pct.toFixed(1)}%`,
    positive: pct >= 0,
  };
}

export function formatEndDate(iso: string | null): string {
  if (!iso) return "\u2014";
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function probSumDeviation(prices: string[]): number {
  const sum = prices.reduce((acc, p) => acc + parseFloat(p || "0"), 0);
  return Math.abs(sum - 1) * 100;
}

export function timeAgo(isoString: string): string {
  const diff = Date.now() - new Date(isoString).getTime();
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  return `${hrs}h ago`;
}
