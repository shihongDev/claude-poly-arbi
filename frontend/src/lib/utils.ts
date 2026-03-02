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

export function timeAgo(isoString: string): string {
  const diff = Date.now() - new Date(isoString).getTime();
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  return `${hrs}h ago`;
}
