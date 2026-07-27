// Token metadata - loaded from public storage (config:assets)
export interface TokenInfo {
  decimals?: number;
  stablecoin?: boolean;
  coingecko?: string;
  binance?: string;
  binance_us?: string;
  huobi?: string;
  cryptocom?: string;
  kucoin?: string;
  gate?: string;
  pyth?: string;
  chainlink?: string;
  binance_alpha?: string;
  kraken?: string;
  coinbase?: string;
  bitstamp?: string;
  okx?: string;
  bitget?: string;
  mexc?: string;
}

export interface TokensConfig {
  [contractId: string]: TokenInfo;
}

const API_URL = 'https://api.outlayer.fastnear.com';
const PROJECT_UUID = 'p0000000000000003';

// Fetch token configs from public storage (config:assets)
export async function fetchTokenConfigs(): Promise<TokensConfig> {
  const response = await fetch(`${API_URL}/public/storage/batch`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      project_uuid: PROJECT_UUID,
      keys: ['config:assets'],
    }),
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }

  const batchResult = await response.json();
  const item = batchResult.results?.['config:assets'];
  if (!item?.exists || !item.value) {
    return {};
  }

  return JSON.parse(atob(item.value));
}

// Token display names
export const TOKEN_NAMES: Record<string, string> = {
  'wrap.near': 'NEAR',
  // ETH is published as eth.bridge.near. The former `aurora` asset was the Aurora EVM
  // account used as an ETH feed; AURORA below is the separate governance token.
  'eth.bridge.near': 'ETH',
  'usdt.tether-token.near': 'USDT',
  '17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1': 'USDC',
  'nbtc.bridge.near': 'BTC',
  '2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near': 'WBTC',
  '6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near': 'DAI',
  'aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near': 'AURORA',
  '4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near': 'WOO',
  '853d955acef822db058eb8505911ed77f175b99e.factory.bridge.near': 'FRAX',
  '22.contract.portalbridge.near': 'SOL',
  'zec.omft.near': 'ZEC',
  'token.rhealab.near': 'RHEA',
  'xrp.omft.near': 'XRP',
  'doge.omft.near': 'DOGE',
  'cardano.omft.near': 'ADA',
  'xlm': 'XLM',
  'ltc.omft.near': 'LTC',
};

// Token icons (single letter abbreviations)
export const TOKEN_ICONS: Record<string, string> = {
  'wrap.near': 'N',
  'eth.bridge.near': 'E',
  'usdt.tether-token.near': 'T',
  '17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1': 'C',
  'nbtc.bridge.near': 'B',
  '2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near': 'W',
  '6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near': 'D',
  'aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near': 'A',
  '4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near': 'W',
  '853d955acef822db058eb8505911ed77f175b99e.factory.bridge.near': 'F',
  '22.contract.portalbridge.near': 'S',
  'zec.omft.near': 'Z',
  'token.rhealab.near': 'R',
  'xrp.omft.near': 'X',
  'doge.omft.near': 'D',
  'cardano.omft.near': 'A',
  'xlm': 'X',
  'ltc.omft.near': 'L',
};

// Get token display name
export function getTokenName(contractId: string): string {
  return TOKEN_NAMES[contractId] || contractId.split('.')[0].toUpperCase();
}

// Get token icon letter
export function getTokenIcon(contractId: string): string {
  return TOKEN_ICONS[contractId] || contractId.charAt(0).toUpperCase();
}

// Check if token is a stablecoin (uses dynamic config)
export function isStablecoin(contractId: string, config: TokensConfig): boolean {
  return config[contractId]?.stablecoin || false;
}

// Get number of price sources for a token (uses dynamic config)
export function getSourceCount(contractId: string, config: TokensConfig): number {
  const token = config[contractId];
  if (!token) return 0;
  let count = 0;
  if (token.coingecko) count++;
  if (token.binance) count++;
  if (token.binance_us) count++;
  if (token.huobi) count++;
  if (token.cryptocom) count++;
  if (token.kucoin) count++;
  if (token.gate) count++;
  if (token.pyth) count++;
  if (token.chainlink) count++;
  if (token.binance_alpha) count++;
  if (token.kraken) count++;
  if (token.coinbase) count++;
  if (token.bitstamp) count++;
  if (token.okx) count++;
  if (token.bitget) count++;
  if (token.mexc) count++;
  return count;
}

// Format contract ID for display (short version)
export function formatContractId(contractId: string): string {
  if (contractId.length <= 25) return contractId;
  return `${contractId.slice(0, 12)}...${contractId.slice(-10)}`;
}

// Pinned assets at the top (in order)
const PINNED_ASSETS = ['wrap.near', 'nbtc.bridge.near', 'eth.bridge.near'];

// Sort tokens: pinned first, then by source count desc, stablecoins last
export function sortTokens(tokens: string[], config: TokensConfig): string[] {
  return [...tokens].sort((a, b) => {
    const aPin = PINNED_ASSETS.indexOf(a);
    const bPin = PINNED_ASSETS.indexOf(b);
    // Pinned assets come first in their defined order
    if (aPin !== -1 && bPin !== -1) return aPin - bPin;
    if (aPin !== -1) return -1;
    if (bPin !== -1) return 1;
    // Stablecoins go last
    const aStable = isStablecoin(a, config);
    const bStable = isStablecoin(b, config);
    if (aStable !== bStable) return aStable ? 1 : -1;
    // Sort by source count descending
    return getSourceCount(b, config) - getSourceCount(a, config);
  });
}

// Default tokens to display (used as loading skeleton)
export const DEFAULT_TOKENS = [
  'wrap.near',
  'eth.bridge.near',
  'nbtc.bridge.near',
  'usdt.tether-token.near',
  '17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1',
  '22.contract.portalbridge.near',
];
