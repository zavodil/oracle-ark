'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import {
  TokensConfig,
  fetchTokenConfigs,
  getTokenName,
  formatContractId,
  isStablecoin,
  getSourceCount,
  sortTokens,
  DEFAULT_TOKENS,
} from '@/lib/tokens';

const API_URL = 'https://api.outlayer.fastnear.com';
const PROJECT_UUID = 'p0000000000000003';
const RECENCY_DURATION_SEC = 120; // 2 minutes

// Priority assets are refreshed by the scheduler roughly every 16s, so polling slower than that
// shows a price that already moved. This costs one public-storage read per tab — no worker call.
const REFRESH_INTERVAL_SEC = 10;

interface PriceSource {
  name: string;
  price: number;
  // When this venue was observed. Sources are refreshed in tiers at different cadences, so
  // entries in the same record legitimately differ in age — Pyth and Chainlink run on a slower
  // cycle than the all-ticker endpoints. Absent only on records written before tiered refresh.
  timestamp?: number;
}

interface PriceData {
  price: number;
  timestamp: number;
  aggregation_method?: string;
  sources?: PriceSource[];
}

interface PriceResult {
  data?: PriceData;
  error?: string;
}

function formatPrice(price: number | null | undefined): string {
  if (price === null || price === undefined) return '---';
  if (price >= 1) {
    return '$' + price.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 4 });
  }
  return '$' + price.toFixed(6);
}

function formatTimestamp(ts: number | undefined): string {
  if (!ts) return '---';
  const date = new Date(ts * 1000);
  return date.toLocaleTimeString();
}

function getAgeSeconds(ts: number | undefined): number {
  if (!ts) return Infinity;
  return Math.floor(Date.now() / 1000) - ts;
}

function formatAge(ts: number | undefined): string {
  const age = getAgeSeconds(ts);
  if (age === Infinity) return '---';
  if (age < 60) return `${age}s ago`;
  const mins = Math.floor(age / 60);
  const secs = age % 60;
  if (age < 3600) return `${mins}m ${secs}s ago`;
  return `${Math.floor(age / 3600)}h ${mins % 60}m ago`;
}

function isFresh(ts: number | undefined): boolean {
  return getAgeSeconds(ts) <= RECENCY_DURATION_SEC;
}

// Compact per-source age: these appear once per venue, so "12s" reads better than "12s ago"
function formatShortAge(ts: number | undefined): string {
  const age = getAgeSeconds(ts);
  if (age === Infinity) return '—';
  if (age < 60) return `${age}s`;
  if (age < 3600) return `${Math.floor(age / 60)}m ${age % 60}s`;
  return `${Math.floor(age / 3600)}h`;
}

// The freshest source in a record is what a caller with a tight window actually gets, and the
// oldest is the honest bound of the full-window aggregate. Showing both makes the tiering
// visible instead of leaving a two-minute-old Chainlink entry looking like a stale feed.
function sourceAgeRange(sources: PriceSource[] | undefined): { min: number; max: number } | null {
  const ages = (sources ?? [])
    .map((s) => getAgeSeconds(s.timestamp))
    .filter((a) => Number.isFinite(a));
  if (ages.length === 0) return null;
  return { min: Math.min(...ages), max: Math.max(...ages) };
}

// Circumference of the countdown ring (r = 10 in a 24-viewBox), so the dash offset is exact
// rather than eyeballed
const RING_CIRCUMFERENCE = 2 * Math.PI * 10;

/**
 * Refresh control: one component carrying all three jobs — refresh now, show how long until the
 * next automatic refresh, and turn that automation off.
 *
 * Deliberately one control rather than a button plus a floating ring elsewhere on the page: the
 * countdown used to live in a corner widget nobody looked at, which is why "it refreshes" was
 * invisible even though it was happening.
 */
function RefreshControl({
  countdown,
  autoRefresh,
  refreshing,
  onRefresh,
  onToggleAuto,
}: {
  countdown: number;
  autoRefresh: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  onToggleAuto: () => void;
}) {
  const progress = autoRefresh ? (REFRESH_INTERVAL_SEC - countdown) / REFRESH_INTERVAL_SEC : 0;
  // Snap instead of animating when the ring wraps back to full, otherwise every cycle ends with
  // a one-second rewind
  const wrapping = countdown >= REFRESH_INTERVAL_SEC;

  return (
    <div className="flex items-stretch rounded-lg border border-dark-700 bg-dark-800 overflow-hidden">
      <button
        type="button"
        onClick={onRefresh}
        title="Refresh now"
        className="flex items-center gap-2.5 pl-2.5 pr-3.5 py-2 hover:bg-dark-700 active:bg-dark-600 transition-colors group"
      >
        <span className="relative w-7 h-7 shrink-0">
          <svg className="w-7 h-7 -rotate-90" viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="10" strokeWidth="2" className="fill-none stroke-dark-700" />
            <circle
              cx="12"
              cy="12"
              r="10"
              strokeWidth="2"
              strokeLinecap="round"
              className={`fill-none transition-colors ${
                refreshing
                  ? 'stroke-green-400'
                  : autoRefresh
                  ? 'stroke-primary group-hover:stroke-green-400'
                  : 'stroke-dark-600'
              }`}
              strokeDasharray={RING_CIRCUMFERENCE}
              strokeDashoffset={RING_CIRCUMFERENCE * (1 - progress)}
              style={{ transition: wrapping ? 'none' : 'stroke-dashoffset 1s linear' }}
            />
          </svg>
          <span
            className={`absolute inset-0 flex items-center justify-center font-mono text-[11px] tabular-nums transition-colors ${
              refreshing
                ? 'text-green-400'
                : autoRefresh
                ? 'text-dark-300 group-hover:text-green-400'
                : 'text-dark-500'
            }`}
          >
            {autoRefresh ? (
              countdown
            ) : (
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                />
              </svg>
            )}
          </span>
        </span>
        <span className="text-sm text-dark-200 group-hover:text-white transition-colors">
          Refresh
        </span>
      </button>

      <div className="w-px bg-dark-700" aria-hidden="true" />

      <button
        type="button"
        onClick={onToggleAuto}
        aria-pressed={autoRefresh}
        title={autoRefresh ? 'Pause auto-refresh' : 'Resume auto-refresh'}
        className={`px-3 hover:bg-dark-700 active:bg-dark-600 transition-colors ${
          autoRefresh ? 'text-dark-400 hover:text-white' : 'text-dark-500 hover:text-primary'
        }`}
      >
        {autoRefresh ? (
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="6" y="5" width="4" height="14" rx="1" />
            <rect x="14" y="5" width="4" height="14" rx="1" />
          </svg>
        ) : (
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M8 5.5v13a1 1 0 001.53.848l10-6.5a1 1 0 000-1.696l-10-6.5A1 1 0 008 5.5z" />
          </svg>
        )}
        <span className="sr-only">{autoRefresh ? 'Pause auto-refresh' : 'Resume auto-refresh'}</span>
      </button>
    </div>
  );
}

function PriceCard({
  assetId,
  result,
  tokensConfig,
}: {
  assetId: string;
  result: PriceResult;
  tokensConfig: TokensConfig;
}) {
  const { data, error } = result;
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const fresh = data && isFresh(data.timestamp);
  const stablecoin = isStablecoin(assetId, tokensConfig);
  const sourceCount = getSourceCount(assetId, tokensConfig);

  return (
    <div
      className={`card transition-colors ${
        error
          ? 'border-red-500/50'
          : data && !fresh
          ? 'border-yellow-500/50'
          : 'hover:border-primary/50'
      }`}
    >
      {/* Header */}
      <div className="flex justify-between items-start mb-4">
        <div>
          <div className="flex items-center gap-2">
            <span className="font-semibold text-white">{getTokenName(assetId)}</span>
            {stablecoin && (
              <span className="px-1.5 py-0.5 text-xs bg-blue-500/20 text-blue-400 rounded">
                Stable
              </span>
            )}
          </div>
          <div
            className="text-xs text-dark-400 font-mono"
            title={assetId}
          >
            {formatContractId(assetId)}
          </div>
        </div>
        {data && (
          <span
            className={`text-xs px-2 py-1 rounded-full ${
              fresh
                ? 'bg-green-500/20 text-green-400'
                : 'bg-yellow-500/20 text-yellow-400'
            }`}
          >
            {fresh ? 'Fresh' : 'Stale'}
          </span>
        )}
      </div>

      {/* Price */}
      <div
        className={`text-3xl font-semibold font-mono mb-4 ${
          error ? 'text-red-400' : data ? 'text-white' : 'text-dark-500'
        }`}
      >
        {error ? 'Error' : data ? formatPrice(data.price) : '---'}
      </div>

      {/* Details */}
      <div className="grid grid-cols-2 gap-3 text-sm">
        <div>
          <div className="text-xs text-dark-500 uppercase">Updated</div>
          <div className="text-dark-300 font-mono">
            {data ? formatAge(data.timestamp) : '---'}
          </div>
        </div>
        <div>
          <div className="text-xs text-dark-500 uppercase">Time</div>
          <div className="text-dark-300 font-mono">
            {data ? formatTimestamp(data.timestamp) : '---'}
          </div>
        </div>
        <div>
          <div className="text-xs text-dark-500 uppercase">Method</div>
          <div className="text-dark-300 font-mono">
            {data?.aggregation_method || '---'}
          </div>
        </div>
        <div>
          <div className="text-xs text-dark-500 uppercase">Sources</div>
          <div className="text-dark-300 font-mono">
            {data?.sources?.length || 0} / {sourceCount}
          </div>
        </div>
      </div>

      {/* Sources — collapsed by default, the breakdown is detail most visitors do not want.
          Deliberately React state rather than <details>: the page re-fetches every 30s, and the
          browser's own disclosure state is not something React owns, so an expanded card would
          be at the mercy of reconciliation. This also keeps each card strictly independent. */}
      {data && data.sources && data.sources.length > 0 && (
        <div className="mt-4 pt-4 border-t border-dark-700">
          <button
            type="button"
            onClick={() => setSourcesOpen((open) => !open)}
            aria-expanded={sourcesOpen}
            className="w-full flex justify-between items-baseline cursor-pointer select-none group text-left"
          >
            <span className="text-xs text-dark-500 uppercase group-hover:text-dark-400">
              Price Sources
              <span
                className={`ml-1 inline-block transition-transform ${sourcesOpen ? 'rotate-90' : ''}`}
              >
                ›
              </span>
            </span>
            {(() => {
              const range = sourceAgeRange(data.sources);
              if (!range) return null;
              return (
                <span
                  className="text-xs text-dark-500 font-mono"
                  title="Freshest and oldest source behind this price"
                >
                  {range.min}s – {range.max}s old
                </span>
              );
            })()}
          </button>
          <div className={`space-y-1 mt-2 ${sourcesOpen ? '' : 'hidden'}`}>
            {data.sources.map((source, idx) => (
              <div key={idx} className="flex justify-between items-baseline text-sm gap-2">
                <span className="text-dark-400 truncate">{source.name}</span>
                <span className="flex items-baseline gap-2 shrink-0">
                  <span
                    className="font-mono text-xs text-green-400/70"
                    title={
                      source.timestamp
                        ? new Date(source.timestamp * 1000).toLocaleTimeString()
                        : 'no timestamp recorded'
                    }
                  >
                    {formatShortAge(source.timestamp)}
                  </span>
                  <span className="text-dark-300 font-mono">${source.price.toFixed(4)}</span>
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Error message */}
      {error && (
        <div className="mt-4 text-xs text-red-400">{error}</div>
      )}
    </div>
  );
}

export default function PricesPage() {
  const [tokensConfig, setTokensConfig] = useState<TokensConfig>({});
  const [allTokens, setAllTokens] = useState<string[]>(DEFAULT_TOKENS);
  const [prices, setPrices] = useState<Record<string, PriceResult>>({});
  const [loading, setLoading] = useState(true);
  const [countdown, setCountdown] = useState(REFRESH_INTERVAL_SEC);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [freshCount, setFreshCount] = useState(0);
  const lastFetchTimeRef = useRef<number>(Date.now());
  const tokensConfigRef = useRef<TokensConfig>({});
  const allTokensRef = useRef<string[]>(DEFAULT_TOKENS);

  const fetchPrices = useCallback(async () => {
    const tokens = allTokensRef.current;
    const keys = tokens.map(assetId => `price:${assetId}`);
    // Held for a beat below even when the request returns instantly — a refresh nobody can see
    // happen reads as a refresh that did not happen
    setRefreshing(true);

    try {
      const response = await fetch(`${API_URL}/public/storage/batch`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          project_uuid: PROJECT_UUID,
          keys: keys,
        }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const batchResult = await response.json();
      const results: Record<string, PriceResult> = {};

      tokens.forEach((assetId, i) => {
        const key = keys[i];
        const item = batchResult.results[key];
        if (item && item.exists && item.value) {
          try {
            const json = atob(item.value);
            results[assetId] = { data: JSON.parse(json) };
          } catch {
            results[assetId] = { error: 'Parse error' };
          }
        } else {
          results[assetId] = { error: 'Not found' };
        }
      });

      setPrices(results);
      const fresh = Object.values(results).filter(
        r => r.data && isFresh(r.data.timestamp)
      ).length;
      setFreshCount(fresh);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Unknown error';
      const results: Record<string, PriceResult> = {};
      tokens.forEach(assetId => {
        results[assetId] = { error: errorMsg };
      });
      setPrices(results);
      setFreshCount(0);
    } finally {
      setLoading(false);
      setTimeout(() => setRefreshing(false), 400);
    }
  }, []);

  // Load token configs from public storage on mount
  useEffect(() => {
    fetchTokenConfigs()
      .then((config) => {
        const tokens = sortTokens(Object.keys(config), config);
        if (tokens.length > 0) {
          setTokensConfig(config);
          setAllTokens(tokens);
          tokensConfigRef.current = config;
          allTokensRef.current = tokens;
          // Re-fetch prices immediately with the full token list
          fetchPrices();
        }
      })
      .catch((err) => {
        console.error('Failed to load token configs:', err);
      });
  }, [fetchPrices]);

  // First load
  useEffect(() => {
    fetchPrices();
    lastFetchTimeRef.current = Date.now();
  }, [fetchPrices]);

  // Auto-refresh loop. Torn down entirely when paused rather than left ticking and ignored, so
  // "off" means no timer and no requests at all.
  useEffect(() => {
    if (!autoRefresh) return;

    lastFetchTimeRef.current = Date.now();
    setCountdown(REFRESH_INTERVAL_SEC);

    const id = setInterval(() => {
      const elapsed = Math.floor((Date.now() - lastFetchTimeRef.current) / 1000);
      const remaining = REFRESH_INTERVAL_SEC - elapsed;

      if (remaining <= 0) {
        fetchPrices();
        lastFetchTimeRef.current = Date.now();
        setCountdown(REFRESH_INTERVAL_SEC);
      } else {
        setCountdown(remaining);
      }
    }, 1000);

    return () => clearInterval(id);
  }, [autoRefresh, fetchPrices]);

  const handleRefresh = () => {
    setCountdown(REFRESH_INTERVAL_SEC);
    lastFetchTimeRef.current = Date.now();
    fetchPrices();
  };

  return (
    <div className="min-h-screen py-8 px-4">
      <div className="max-w-7xl mx-auto">
        {/* Header — sticky so the countdown and the pause toggle stay reachable while scrolling
            the grid, which is the whole reason the old corner widget existed.
            `top-16` clears the site header, which is `fixed h-16` (see components/Header.tsx);
            at `top-0` this would slide underneath it. */}
        <div className="sticky top-16 z-20 -mx-4 px-4 py-4 mb-4 bg-dark-950/80 backdrop-blur-sm border-b border-dark-800 flex flex-col md:flex-row justify-between items-start md:items-center gap-4">
          <div>
            <h1 className="text-3xl font-bold text-white">Live Prices</h1>
            <p className="text-dark-400 text-sm mt-1">
              {loading ? 'Loading prices...' : `${freshCount}/${allTokens.length} fresh prices`}
            </p>
          </div>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2 text-sm text-dark-400">
              <span className="w-2 h-2 bg-green-400 rounded-full animate-pulse"></span>
              <span>Mainnet</span>
            </div>
            <RefreshControl
              countdown={countdown}
              autoRefresh={autoRefresh}
              refreshing={refreshing}
              onRefresh={handleRefresh}
              onToggleAuto={() => setAutoRefresh(v => !v)}
            />
          </div>
        </div>

        {/* Info panel */}
        <div className="card mb-8 bg-dark-900">
          <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4">
            <div className="text-sm text-dark-400">
              Prices fetched from{' '}
              <a
                href="https://outlayer.fastnear.com"
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline"
              >
                NEAR OutLayer
              </a>{' '}
              public storage. NEAR, BTC and ETH refresh every ~16s; every other asset every ~60s;
              Pyth and Chainlink run on their own 90s cycle. A 1% move refreshes early. Prices
              older than 2 min are marked stale.
            </div>
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-2">
                <span className="w-3 h-3 rounded-full bg-green-500/30 border border-green-500"></span>
                <span className="text-dark-400">Fresh (&lt;2m)</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="w-3 h-3 rounded-full bg-yellow-500/30 border border-yellow-500"></span>
                <span className="text-dark-400">Stale (&gt;2m)</span>
              </div>
            </div>
          </div>
        </div>

        {/* Price grid */}
        {loading ? (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {allTokens.map(assetId => (
              <div key={assetId} className="card animate-pulse">
                <div className="flex items-center gap-3 mb-4">
                  <div className="w-10 h-10 bg-dark-700 rounded-full"></div>
                  <div>
                    <div className="h-5 w-20 bg-dark-700 rounded mb-1"></div>
                    <div className="h-3 w-32 bg-dark-700 rounded"></div>
                  </div>
                </div>
                <div className="h-8 w-28 bg-dark-700 rounded mb-4"></div>
                <div className="grid grid-cols-2 gap-3">
                  <div className="h-10 bg-dark-700 rounded"></div>
                  <div className="h-10 bg-dark-700 rounded"></div>
                </div>
              </div>
            ))}
          </div>
        ) : (
          // items-start: without it grid rows stretch every card to the tallest one, so
          // expanding a single card's sources resizes its whole row and reads as if the
          // others had opened too
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 items-start">
            {allTokens.map(assetId => (
              <PriceCard
                key={assetId}
                assetId={assetId}
                result={prices[assetId] || { error: 'Not loaded' }}
                tokensConfig={tokensConfig}
              />
            ))}
          </div>
        )}
      </div>

    </div>
  );
}
