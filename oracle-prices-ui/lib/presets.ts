// Playground presets for testing oracle functionality

export type PresetType = 'view' | 'call' | 'outlayer';

export interface Preset {
  id: string;
  name: string;
  description: string;
  type: PresetType;
  contract?: string;
  method?: string;
  args: Record<string, unknown>;
  deposit?: string;
  gas?: string;
  // For outlayer presets
  command?: string;
  editableFields?: string[];
}

export const PRICE_ORACLE_PRESETS: Preset[] = [
  {
    id: 'oracle-call',
    name: 'Oracle Call with Callback (Recommended)',
    description: 'Get prices with callback to your contract. This is the recommended way to use the oracle.',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'oracle_call',
    args: {
      receiver_id: 'YOUR_CONTRACT.near',
      asset_ids: ['wrap.near', 'aurora'],
      msg: '',
    },
    deposit: '0.02',
    gas: '200000000000000',
  },
  {
    id: 'request-fresh-prices',
    name: 'Request Fresh Prices (Direct)',
    description: 'Fetch fresh prices directly from TEE (useful for scripts and testing)',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'request_price_data',
    args: {
      asset_ids: ['wrap.near'],
    },
    deposit: '0.02',
    gas: '200000000000000',
  },
];

export const PYTH_PRESETS: Preset[] = [
  {
    id: 'pyth-refresh-prices',
    name: 'Pyth: Refresh Prices',
    description: 'Fetch fresh prices from TEE and update cache. Required before get_price will return data.',
    type: 'call',
    contract: 'price-oracle-pyth.near',
    method: 'refresh_prices',
    args: {},
    deposit: '0.02',
    gas: '300000000000000',
  },
  {
    id: 'pyth-get-price',
    name: 'Pyth: Get NEAR Price',
    description: 'Get NEAR price from cache. Call refresh_prices first to populate the cache.',
    type: 'view',
    contract: 'price-oracle-pyth.near',
    method: 'get_price',
    args: {
      price_identifier: 'c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750',
    },
  },
  {
    id: 'pyth-list-prices',
    name: 'Pyth: List Multiple Prices',
    description: 'Get multiple prices from cache. Call refresh_prices first to populate the cache.',
    type: 'view',
    contract: 'price-oracle-pyth.near',
    method: 'list_prices',
    args: {
      price_ids: [
        'c415de8d2efa7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750', // NEAR
        'ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace', // ETH
        'e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43', // BTC
      ],
    },
  },
];

export const CUSTOM_DATA_PRESETS: Preset[] = [
  {
    id: 'steam-game-price',
    name: 'Steam: Game Price',
    description: 'Get Elden Ring price from Steam Store API',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'request_custom_data',
    args: {
      custom_data_request: [
        {
          id: 'elden_ring',
          token_id: '',
          source: {
            custom: {
              url: 'https://store.steampowered.com/api/appdetails?appids=1245620',
              json_path: '1245620.data.price_overview.final_formatted',
              value_type: 'string',
              method: 'GET',
              headers: [],
            },
          },
        },
      ],
    },
    deposit: '0.02',
    gas: '200000000000000',
    editableFields: ['url'],
  },
  {
    id: 'account-nfts',
    name: 'FastNEAR: Account NFTs',
    description: 'Get NFT collection for any NEAR account',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'request_custom_data',
    args: {
      custom_data_request: [
        {
          id: 'nfts',
          token_id: '',
          source: {
            custom: {
              url: 'https://api.fastnear.com/v1/account/root.near/nft',
              json_path: 'nft',
              value_type: 'string',
              method: 'GET',
              headers: [],
            },
          },
        },
      ],
    },
    deposit: '0.02',
    gas: '200000000000000',
    editableFields: ['url'],
  },
  {
    id: 'latest-validator',
    name: 'NearBlocks: Latest Validator',
    description: 'Get the name of a NEAR validator from NearBlocks',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'request_custom_data',
    args: {
      custom_data_request: [
        {
          id: 'validator',
          token_id: '',
          source: {
            custom: {
              url: 'https://api.nearblocks.io/v1/validators',
              json_path: 'validators.0.accountId',
              value_type: 'string',
              method: 'GET',
              headers: [],
            },
          },
        },
      ],
    },
    deposit: '0.02',
    gas: '200000000000000',
  },
  {
    id: 'eur-usd-rate',
    name: 'Forex: EUR/USD Rate',
    description: 'Get current EUR to USD exchange rate',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'request_custom_data',
    args: {
      custom_data_request: [
        {
          id: 'eur_usd',
          token_id: '',
          source: {
            custom: {
              url: 'https://open.er-api.com/v6/latest/EUR',
              json_path: 'rates.USD',
              value_type: 'number',
              method: 'GET',
              headers: [],
            },
          },
        },
      ],
    },
    deposit: '0.02',
    gas: '200000000000000',
  },
  {
    id: 'github-stars',
    name: 'GitHub: Repository Stars',
    description: 'Get star count for a GitHub repository',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'request_custom_data',
    args: {
      custom_data_request: [
        {
          id: 'stars',
          token_id: '',
          source: {
            custom: {
              url: 'https://api.github.com/repos/near/nearcore',
              json_path: 'stargazers_count',
              value_type: 'number',
              method: 'GET',
              headers: [['User-Agent', 'Oracle-Ark/1.0']],
            },
          },
        },
      ],
    },
    deposit: '0.02',
    gas: '200000000000000',
    editableFields: ['url'],
  },
  {
    id: 'weather-data',
    name: 'Weather: Current Temperature',
    description: 'Get current temperature for a location (Open-Meteo)',
    type: 'call',
    contract: 'price-oracle.near',
    method: 'request_custom_data',
    args: {
      custom_data_request: [
        {
          id: 'temperature',
          token_id: '',
          source: {
            custom: {
              url: 'https://api.open-meteo.com/v1/forecast?latitude=40.7128&longitude=-74.0060&current_weather=true',
              json_path: 'current_weather.temperature',
              value_type: 'number',
              method: 'GET',
              headers: [],
            },
          },
        },
      ],
    },
    deposit: '0.02',
    gas: '200000000000000',
    editableFields: ['url'],
  },
];

// View methods (require fresh data first)
export const VIEW_PRESETS: Preset[] = [
  {
    id: 'get-cached-prices',
    name: 'Get Cached Prices',
    description: 'View cached prices. Call request_price_data first for the assets you want to read.',
    type: 'view',
    contract: 'price-oracle.near',
    method: 'get_price_data',
    args: {
      asset_ids: ['wrap.near', 'aurora', 'nbtc.bridge.near'],
    },
  },
];

export const ALL_PRESETS = [
  ...PRICE_ORACLE_PRESETS,
  ...CUSTOM_DATA_PRESETS,
  ...PYTH_PRESETS,
  ...VIEW_PRESETS,
];

export function getPresetById(id: string): Preset | undefined {
  return ALL_PRESETS.find((p) => p.id === id);
}

export const PRESET_CATEGORIES = [
  { id: 'price-oracle', name: 'Price Oracle', presets: PRICE_ORACLE_PRESETS },
  { id: 'custom-data', name: 'Custom Data', presets: CUSTOM_DATA_PRESETS },
  { id: 'pyth', name: 'Pyth Compatible', presets: PYTH_PRESETS },
  { id: 'view', name: 'View Methods', presets: VIEW_PRESETS },
];

// Helper to check if a preset needs a warning about requiring fresh data first
export interface PresetWarning {
  message: string;
  linkPresetId: string;
  linkText: string;
}

export function getPresetWarning(presetId: string): PresetWarning | null {
  if (presetId === 'get-cached-prices') {
    return {
      message: 'This reads from cache. First call',
      linkPresetId: 'request-fresh-prices',
      linkText: 'Request Fresh Prices',
    };
  }
  if (presetId === 'pyth-get-price' || presetId === 'pyth-list-prices') {
    return {
      message: 'This reads from cache. First call',
      linkPresetId: 'pyth-refresh-prices',
      linkText: 'Pyth: Refresh Prices',
    };
  }
  return null;
}
