const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8080";

// ---------------------------------------------------------------------------
// In-flight deduplication: concurrent GETs to the same URL share one Promise
// ---------------------------------------------------------------------------
const inflight = new Map<string, Promise<unknown>>();

// ---------------------------------------------------------------------------
// TTL response cache: avoids re-fetching on tab switches / page transitions
// ---------------------------------------------------------------------------
interface CacheEntry {
  data: unknown;
  expires: number;
}
const responseCache = new Map<string, CacheEntry>();

/** Default TTL per path prefix (seconds). Longest match wins. */
const TTL_RULES: [string, number][] = [
  ["/api/config", 30],
  ["/api/status", 5],
  ["/api/markets", 5],
  ["/api/opportunities", 5],
  ["/api/positions", 5],
  ["/api/metrics", 5],
  ["/api/history", 10],
];

function ttlForPath(path: string): number {
  for (const [prefix, ttl] of TTL_RULES) {
    if (path.startsWith(prefix)) return ttl;
  }
  return 5; // default 5s
}

export async function fetchApi<T>(
  path: string,
  options?: RequestInit
): Promise<T> {
  const method = (options?.method ?? "GET").toUpperCase();
  const isGet = method === "GET";

  // Only cache/deduplicate GET requests
  if (isGet) {
    // Check TTL cache first
    const cached = responseCache.get(path);
    if (cached && cached.expires > Date.now()) {
      return cached.data as T;
    }

    // Check in-flight deduplication
    const pending = inflight.get(path);
    if (pending) {
      return pending as Promise<T>;
    }
  }

  const promise = doFetch<T>(path, options);

  if (isGet) {
    inflight.set(path, promise);
    promise
      .then((data) => {
        // Cache successful response
        const ttl = ttlForPath(path);
        responseCache.set(path, {
          data,
          expires: Date.now() + ttl * 1000,
        });
      })
      .finally(() => {
        inflight.delete(path);
      });
  }

  return promise;
}

async function doFetch<T>(
  path: string,
  options?: RequestInit
): Promise<T> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 10_000);

  try {
    const res = await fetch(`${API_BASE}${path}`, {
      headers: { "Content-Type": "application/json" },
      signal: controller.signal,
      ...options,
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    return res.json();
  } finally {
    clearTimeout(timeout);
  }
}

export function getWsUrl(): string {
  const base = process.env.NEXT_PUBLIC_WS_URL || "ws://localhost:8080";
  return `${base}/ws`;
}

// ---------------------------------------------------------------------------
// Position Management API
// ---------------------------------------------------------------------------

import type { ExecutionReport } from "./types";

export function closePosition(tokenId: string): Promise<ExecutionReport> {
  return fetchApi(`/api/positions/${encodeURIComponent(tokenId)}/close`, {
    method: "POST",
  });
}

export function closeAllPositions(): Promise<{
  closed: number;
  reports: ExecutionReport[];
}> {
  return fetchApi("/api/positions/close-all", { method: "POST" });
}

export function reducePosition(
  tokenId: string,
  size: string
): Promise<ExecutionReport> {
  return fetchApi(`/api/positions/${encodeURIComponent(tokenId)}/reduce`, {
    method: "POST",
    body: JSON.stringify({ size }),
  });
}

// ---------------------------------------------------------------------------
// Sandbox / Playground API
// ---------------------------------------------------------------------------

import type {
  SandboxConfigOverrides,
  DetectResponse,
  BacktestResponse,
  SimulateParams,
  SimulationStatus,
  StressScenario,
  StressTestResult,
} from "./types";

export function sandboxDetect(
  overrides: SandboxConfigOverrides
): Promise<DetectResponse> {
  return fetchApi("/api/sandbox/detect", {
    method: "POST",
    body: JSON.stringify({ config_overrides: overrides }),
  });
}

export function sandboxBacktest(
  overrides: SandboxConfigOverrides
): Promise<BacktestResponse> {
  return fetchApi("/api/sandbox/backtest", {
    method: "POST",
    body: JSON.stringify({ config_overrides: overrides }),
  });
}

export function runSimulation(
  conditionId: string,
  params: SimulateParams
): Promise<unknown> {
  return fetchApi(`/api/simulate/${conditionId}`, {
    method: "POST",
    body: JSON.stringify(params),
  });
}

// ---------------------------------------------------------------------------
// Simulation Status API
// ---------------------------------------------------------------------------

export async function fetchSimulationStatus(): Promise<SimulationStatus> {
  return fetchApi<SimulationStatus>("/api/simulation/status");
}

// ---------------------------------------------------------------------------
// Stress Test API
// ---------------------------------------------------------------------------

export async function runStressTest(
  scenario: StressScenario
): Promise<StressTestResult> {
  return fetchApi<StressTestResult>("/api/stress-test", {
    method: "POST",
    body: JSON.stringify(scenario),
  });
}
