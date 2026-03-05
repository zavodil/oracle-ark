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
const REFRESH_INTERVAL_SEC = 30;

interface PriceSource {
  name: string;
  price: number;
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

      {/* Sources list */}
      {data && data.sources && data.sources.length > 0 && (
        <div className="mt-4 pt-4 border-t border-dark-700">
          <div className="text-xs text-dark-500 uppercase mb-2">Price Sources</div>
          <div className="space-y-1">
            {data.sources.map((source, idx) => (
              <div key={idx} className="flex justify-between text-sm">
                <span className="text-dark-400">{source.name}</span>
                <span className="text-dark-300 font-mono">
                  ${source.price.toFixed(4)}
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
  const [freshCount, setFreshCount] = useState(0);
  const countdownRef = useRef<NodeJS.Timeout | null>(null);
  const lastFetchTimeRef = useRef<number>(Date.now());
  const tokensConfigRef = useRef<TokensConfig>({});
  const allTokensRef = useRef<string[]>(DEFAULT_TOKENS);

  const fetchPrices = useCallback(async () => {
    const tokens = allTokensRef.current;
    const keys = tokens.map(assetId => `price:${assetId}`);

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

  // Initial fetch and auto-refresh
  useEffect(() => {
    fetchPrices();
    lastFetchTimeRef.current = Date.now();

    countdownRef.current = setInterval(() => {
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

    return () => {
      if (countdownRef.current) {
        clearInterval(countdownRef.current);
      }
    };
  }, [fetchPrices]);

  const handleRefresh = () => {
    setCountdown(REFRESH_INTERVAL_SEC);
    lastFetchTimeRef.current = Date.now();
    fetchPrices();
  };

  const progressPercent = ((REFRESH_INTERVAL_SEC - countdown) / REFRESH_INTERVAL_SEC) * 100;

  return (
    <div className="min-h-screen py-8 px-4">
      <div className="max-w-7xl mx-auto">
        {/* Header */}
        <div className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4 mb-8">
          <div>
            <h1 className="text-3xl font-bold text-white mb-2">Live Prices</h1>
            <p className="text-dark-400">
              {loading ? 'Loading prices...' : `${freshCount}/${allTokens.length} fresh prices`}
            </p>
          </div>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2 text-sm text-dark-400">
              <span className="w-2 h-2 bg-green-400 rounded-full animate-pulse"></span>
              <span>Mainnet</span>
            </div>
            <button
              onClick={handleRefresh}
              className="btn btn-secondary flex items-center gap-2"
            >
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
                href="https://outlayer.fastnear.com"
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline"
              >
                NEAR OutLayer
              </a>{' '}
              public storage. Scheduler updates every ~60s or on 1% price change. Prices older than 2 min are marked stale.
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
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
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

      {/* Floating refresh ring */}
      <div
        className="fixed bottom-6 right-6 w-16 h-16 cursor-pointer group"
        onClick={handleRefresh}
        title="Click to refresh now"
      >
        <svg className="w-full h-full -rotate-90" viewBox="0 0 36 36">
          <circle
            className="fill-none stroke-dark-700"
            strokeWidth="3"
            cx="18"
            cy="18"
            r="16"
          />
          <circle
            className="fill-none stroke-primary group-hover:stroke-green-400 transition-colors"
            strokeWidth="3"
            strokeLinecap="round"
            cx="18"
            cy="18"
            r="16"
            strokeDasharray="100.53"
            strokeDashoffset={100.53 * (1 - progressPercent / 100)}
            style={{ transition: 'stroke-dashoffset 0.5s linear' }}
          />
        </svg>
        <div className="absolute inset-0 flex items-center justify-center text-sm font-semibold text-dark-400 group-hover:text-green-400 transition-colors">
          {countdown}s
        </div>
      </div>
    </div>
  );
}
