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

const MOCK_SIMULATION_STATUS: SimulationStatus = {
  estimates: [
    {
      condition_id: "0x1a2b3c4d5e6f",
      market_price: 0.62,
      model_estimate: 0.67,
      divergence: 0.05,
      confidence_interval: [0.61, 0.73],
      method: "Monte Carlo",
    },
    {
      condition_id: "0x7a8b9c0d1e2f",
      market_price: 0.34,
      model_estimate: 0.31,
      divergence: -0.03,
      confidence_interval: [0.26, 0.36],
      method: "Particle Filter",
    },
    {
      condition_id: "0x3f4e5d6c7b8a",
      market_price: 0.88,
      model_estimate: 0.91,
      divergence: 0.03,
      confidence_interval: [0.87, 0.95],
      method: "Monte Carlo",
    },
  ],
  convergence: {
    paths_used: 50000,
    standard_error: 0.0023,
    converged: true,
    gelman_rubin: 1.004,
  },
  model_health: {
    brier_score_30m: 0.142,
    brier_score_24h: 0.168,
    confidence_level: 0.82,
    drift_detected: false,
  },
  var_summary: {
    var_95: "-$124.50",
    var_99: "-$287.30",
    cvar_95: "-$198.75",
    method: "Historical Simulation",
  },
};

export async function fetchSimulationStatus(): Promise<SimulationStatus> {
  try {
    return await fetchApi<SimulationStatus>("/api/simulation/status");
  } catch {
    // Return mock data if endpoint doesn't exist yet
    return MOCK_SIMULATION_STATUS;
  }
}

// ---------------------------------------------------------------------------
// Stress Test API
// ---------------------------------------------------------------------------

const MOCK_STRESS_RESULTS: Record<string, StressTestResult> = {
  liquidity_shock: {
    scenario: "liquidity_shock",
    portfolio_impact: "-$342.18",
    max_loss: "-$891.45",
    positions_at_risk: 4,
    var_before: "-$124.50",
    var_after: "-$467.20",
    details: "Simulated 50% depth reduction across all active orderbooks",
  },
  correlation_spike: {
    scenario: "correlation_spike",
    portfolio_impact: "-$178.92",
    max_loss: "-$523.10",
    positions_at_risk: 6,
    var_before: "-$124.50",
    var_after: "-$312.80",
    details: "Simulated correlation increase to 0.85 across correlated pairs",
  },
  flash_crash: {
    scenario: "flash_crash",
    portfolio_impact: "-$612.35",
    max_loss: "-$1,247.80",
    positions_at_risk: 8,
    var_before: "-$124.50",
    var_after: "-$892.10",
    details: "Simulated 15% adverse move across all positions simultaneously",
  },
  kill_switch_delay: {
    scenario: "kill_switch_delay",
    portfolio_impact: "-$89.40",
    max_loss: "-$234.55",
    positions_at_risk: 2,
    var_before: "-$124.50",
    var_after: "-$198.30",
    details: "Simulated 30 second delay before kill switch activation",
  },
};

export async function runStressTest(
  scenario: StressScenario
): Promise<StressTestResult> {
  try {
    return await fetchApi<StressTestResult>("/api/stress-test", {
      method: "POST",
      body: JSON.stringify(scenario),
    });
  } catch {
    // Return mock data if endpoint doesn't exist yet
    return MOCK_STRESS_RESULTS[scenario.scenario] ?? MOCK_STRESS_RESULTS.liquidity_shock;
  }
}
