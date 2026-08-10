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

const API_URL = 'https://api.outlayer.ai';
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
 * Auto-refresh toggle. Its only job is on/off; while on it doubles as the countdown to the next
 * automatic refresh, so the seconds sit right next to the Refresh button instead of in a corner
 * widget nobody looked at. Clicking it never refreshes — that is the separate button's job, and
 * that button works whether this is on or off.
 */
function AutoRefreshToggle({
  countdown,
  enabled,
  refreshing,
  onToggle,
}: {
  countdown: number;
  enabled: boolean;
  refreshing: boolean;
  onToggle: () => void;
}) {
  const progress = enabled ? (REFRESH_INTERVAL_SEC - countdown) / REFRESH_INTERVAL_SEC : 0;
  // Snap instead of animating when the ring wraps back to full, otherwise every cycle ends with
  // a one-second rewind
  const wrapping = countdown >= REFRESH_INTERVAL_SEC;

  return (
    <button
      type="button"
      onClick={onToggle}
      aria-pressed={enabled}
      title={enabled ? 'Auto-refresh on — click to turn off' : 'Auto-refresh off — click to turn on'}
      className={`flex items-center gap-2.5 pl-2 pr-3.5 py-1.5 rounded-lg border transition-colors ${
        enabled
          ? 'border-dark-700 bg-dark-800 hover:border-dark-600'
          : 'border-dark-800 bg-dark-900 hover:border-dark-700'
      }`}
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
              refreshing ? 'stroke-green-400' : enabled ? 'stroke-primary' : 'stroke-transparent'
            }`}
            strokeDasharray={RING_CIRCUMFERENCE}
            strokeDashoffset={RING_CIRCUMFERENCE * (1 - progress)}
            style={{ transition: wrapping ? 'none' : 'stroke-dashoffset 1s linear' }}
          />
        </svg>
        <span
          className={`absolute inset-0 flex items-center justify-center font-mono text-[11px] tabular-nums transition-colors ${
            refreshing ? 'text-green-400' : enabled ? 'text-dark-200' : 'text-dark-600'
          }`}
        >
          {enabled ? (
            countdown
          ) : (
            // A struck-through circle reads as "off" without needing a word for it
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
              <circle cx="12" cy="12" r="8" strokeWidth="2" />
              <path strokeLinecap="round" strokeWidth="2" d="M6.5 6.5l11 11" />
            </svg>
          )}
        </span>
      </span>
      <span
        className={`text-sm transition-colors ${enabled ? 'text-dark-200' : 'text-dark-500'}`}
      >
        Auto
      </span>
      <span className="sr-only">
        {enabled ? `Auto-refresh on, next in ${countdown} seconds` : 'Auto-refresh off'}
      </span>
    </button>
  );
}

function PriceCard({
  assetId,
  result,
  tokensConfig,
  sourcesOpen,
  onToggleSources,
}: {
  assetId: string;
  result: PriceResult;
  tokensConfig: TokensConfig;
  // Shared across every card: opening one breakdown opens them all, which is what makes the
  // grid comparable — reading one venue's price against the others is the reason to open it
  sourcesOpen: boolean;
  onToggleSources: () => void;
}) {
  const { data, error } = result;
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
          React state rather than <details>: the page re-fetches on a timer, and the browser's
          own disclosure state is not something React owns, so an expanded card would be at the
          mercy of reconciliation. The state lives on the page, not here — see the props. */}
      {data && data.sources && data.sources.length > 0 && (
        <div className="mt-4 pt-4 border-t border-dark-700">
          <button
            type="button"
            onClick={onToggleSources}
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
  const [sourcesOpen, setSourcesOpen] = useState(false);
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
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2 text-sm text-dark-400 mr-1">
              <span className="w-2 h-2 bg-green-400 rounded-full animate-pulse"></span>
              <span>Mainnet</span>
            </div>
            <AutoRefreshToggle
              countdown={countdown}
              enabled={autoRefresh}
              refreshing={refreshing}
              onToggle={() => setAutoRefresh(v => !v)}
            />
            <button
              onClick={handleRefresh}
              title="Refresh now — works whether auto-refresh is on or off"
              className="btn btn-secondary flex items-center gap-2"
            >
              <svg
                className={`w-4 h-4 ${refreshing ? 'animate-spin' : ''}`}
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              Refresh
            </button>
          </div>
        </div>

        {/* Info panel */}
        <div className="card mb-8 bg-dark-900">
          <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4">
            <div className="text-sm text-dark-400">
              Prices fetched from{' '}
              <a
                href="https://outlayer.ai"
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline"
              >
                OutLayer
              </a>{' '}
              public storage. The most-used assets are refreshed on the fastest cycle, the rest on
              a wider one, and a 1% move refreshes early regardless. Every price carries the
              observation time of each source behind it — open <em>Price Sources</em> on any card
              to see them. Prices older than 2 min are marked stale.
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
                sourcesOpen={sourcesOpen}
                onToggleSources={() => setSourcesOpen(open => !open)}
              />
            ))}
          </div>
        )}
      </div>

    </div>
  );
}
