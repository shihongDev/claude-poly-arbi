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

// ── Shared constants ──────────────────────────────────────────

/** Monospace font style object for inline style props. */
export const MONO_STYLE: React.CSSProperties = {
  fontFamily: "var(--font-jetbrains-mono)",
};

/** Monospace font family string for ECharts / non-React contexts. */
export const MONO_FONT =
  "var(--font-jetbrains-mono), JetBrains Mono, monospace";

/** Design-system outcome colors. */
export const OUTCOME_COLORS = [
  "#2D6A4F", // sage green
  "#B44C3F", // brick red
  "#D97706", // amber
  "#6366F1", // indigo
  "#0EA5E9", // sky
  "#8B5CF6", // violet
];

// ── Shared formatting helpers ─────────────────────────────────

/** Format a decimal price string (0.65) as cents (65¢). */
export function formatCents(price: string | null): string {
  if (!price) return "\u2014";
  const num = parseFloat(price);
  if (isNaN(num)) return "\u2014";
  return `${Math.round(num * 100)}\u00a2`;
}

/** Truncate text with trailing ellipsis. */
export function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen - 1) + "\u2026";
}

/** Format a dollar amount with K/M suffixes. */
export function formatUsdCompact(value: string | null): string {
  if (!value) return "\u2014";
  const num = parseFloat(value);
  if (isNaN(num)) return "\u2014";
  if (num >= 1_000_000) return `$${(num / 1_000_000).toFixed(1)}M`;
  if (num >= 1_000) return `$${(num / 1_000).toFixed(1)}K`;
  return `$${num.toFixed(0)}`;
}

/** Classify a spread in bps as good/warning/danger. */
export function spreadSeverity(
  bps: number | null
): "good" | "warning" | "danger" | "unknown" {
  if (bps === null) return "unknown";
  if (bps < 30) return "good";
  if (bps <= 100) return "warning";
  return "danger";
}

/** Map spread severity to text color class. */
export function spreadColorClass(bps: number | null): string {
  const s = spreadSeverity(bps);
  if (s === "good") return "text-[#2D6A4F]";
  if (s === "warning") return "text-[#D97706]";
  if (s === "danger") return "text-[#B44C3F]";
  return "text-[#9B9B9B]";
}

/** Find the longest common prefix of strings, trimmed to word boundary. */
export function longestCommonPrefix(strings: string[]): string {
  if (strings.length === 0) return "";
  if (strings.length === 1) return strings[0];

  let prefix = strings[0];
  for (let i = 1; i < strings.length; i++) {
    while (strings[i].indexOf(prefix) !== 0) {
      prefix = prefix.slice(0, -1);
      if (prefix === "") return "";
    }
  }

  const lastSpace = prefix.lastIndexOf(" ");
  if (lastSpace > 0 && prefix.length < strings[0].length) {
    prefix = prefix.slice(0, lastSpace);
  }

  return prefix.replace(/[\s:,\-?]+$/, "");
}

/** Derive a human-readable group name from market questions. */
export function deriveGroupTitle(
  questions: string[],
  minPrefixLen = 20
): string {
  if (questions.length === 0) return "Unknown Event";
  if (questions.length === 1) return questions[0];

  const prefix = longestCommonPrefix(questions);
  if (prefix.length >= minPrefixLen) return prefix;

  const first = questions[0];
  return first.length > 80 ? first.slice(0, 77) + "..." : first;
}
