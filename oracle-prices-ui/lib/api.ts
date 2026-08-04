// API helpers for OutLayer and oracle interactions

const COORDINATOR_URL = 'https://api.outlayer.ai';
const PROJECT_UUID = 'p0000000000000003'; // price-oracle.near project

export interface StoredPrice {
  price: number;
  timestamp: number;
  sources: Array<{
    name: string;
    price: number;
    timestamp?: number;
  }>;
  aggregation_method: string;
}

export interface BatchStorageResponse {
  results: {
    [key: string]: {
      exists: boolean;
      value?: string;
    };
  };
}

// Fetch prices from OutLayer public storage
export async function fetchPricesFromStorage(
  assetIds: string[]
): Promise<Map<string, StoredPrice | null>> {
  const keys = assetIds.map((id) => `price:${id}`);

  const response = await fetch(`${COORDINATOR_URL}/public/storage/batch`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      project_uuid: PROJECT_UUID,
      keys,
    }),
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch prices: ${response.status}`);
  }

  const data: BatchStorageResponse = await response.json();
  const result = new Map<string, StoredPrice | null>();

  for (const assetId of assetIds) {
    const key = `price:${assetId}`;
    const entry = data.results[key];

    if (entry?.exists && entry.value) {
      try {
        const decoded = atob(entry.value);
        const parsed = JSON.parse(decoded) as StoredPrice;
        result.set(assetId, parsed);
      } catch {
        result.set(assetId, null);
      }
    } else {
      result.set(assetId, null);
    }
  }

  return result;
}

// Format price for display
export function formatPrice(price: number): string {
  if (price >= 1) {
    return price.toLocaleString('en-US', {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  } else if (price >= 0.01) {
    return price.toLocaleString('en-US', {
      minimumFractionDigits: 4,
      maximumFractionDigits: 4,
    });
  } else {
    return price.toLocaleString('en-US', {
      minimumFractionDigits: 6,
      maximumFractionDigits: 8,
    });
  }
}

// Format timestamp for display
export function formatTimestamp(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleString();
}

// Check if price is fresh (within 2 minutes)
export function isPriceFresh(timestamp: number): boolean {
  const now = Math.floor(Date.now() / 1000);
  return now - timestamp < 120; // 2 minutes
}

// Get time since update
export function getTimeSince(timestamp: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - timestamp;

  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

// Convert price multiplier to USD
export function multiplierToUsd(multiplier: string | number, decimals: number = 8): number {
  const value = typeof multiplier === 'string' ? BigInt(multiplier) : BigInt(multiplier);
  return Number(value) / Math.pow(10, decimals);
}

// Get explorer transaction URL
export function getTransactionUrl(hash: string): string {
  return `https://nearblocks.io/txns/${hash}`;
}

// Get explorer account URL
export function getAccountUrl(accountId: string): string {
  return `https://nearblocks.io/address/${accountId}`;
}
