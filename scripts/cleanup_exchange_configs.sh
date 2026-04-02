#!/bin/bash
# Remove broken/unused exchange sources from oracle asset configs.
#
# What this does:
#   1. Creates a council proposal (set_asset_exchange_configs) that removes:
#      - coingecko    — 0/18 assets responded (API key issues or rate limiting)
#      - binance      — 0/15 assets responded (main binance, not US; likely geo-blocked)
#      - chainlink    — 0/7 assets responded (RPC calls to Ethereum too slow/unreliable)
#      - binance_us for ZEC — returns $30 instead of $241 (delisted)
#      - binance_us for DAI — returns $0.985 (stale/incorrect price)
#   2. After proposal executes, calls sync_asset_configs to update WASI storage
#
#   This removes ~50 HTTP requests per WASI execution, cutting runtime significantly.
#
# Usage: ./cleanup_exchange_configs.sh <contract_id> <owner_account> [network]
# Example: ./cleanup_exchange_configs.sh price-oracle.near owner.price-oracle.near mainnet

set -e

CONTRACT_ID="${1:?Usage: $0 <contract_id> <owner_account> [network]}"
OWNER_ACCOUNT="${2:?Usage: $0 <contract_id> <owner_account> [network]}"
NETWORK="${3:-mainnet}"

echo "=== Cleanup exchange configs ==="
echo "Contract: $CONTRACT_ID"
echo "Owner: $OWNER_ACCOUNT"
echo "Network: $NETWORK"
echo ""
echo "Removing: coingecko, binance (main), chainlink from ALL assets"
echo "Removing: binance_us from ZEC (broken price), DAI (broken price)"
echo ""

# Write proposal JSON to temp file (avoids shell escaping issues with near CLI).
# Uses serde internally tagged format: "action" field is the snake_case variant name.
TMPFILE=$(mktemp)
cat > "$TMPFILE" << 'EOJSON'
{"action":{"action":"set_asset_exchange_configs","configs":[["wrap.near","{\"decimals\":24,\"binance_us\":\"NEARUSD\",\"huobi\":\"nearusdt\",\"cryptocom\":\"NEAR_USDT\",\"kucoin\":\"NEAR-USDT\",\"gate\":\"near_usdt\",\"pyth\":\"0xc415de8d2eba7db216527dff4b60e8f3a5311c740dadb233e13e12547e226750\"}"],["aurora","{\"decimals\":18,\"binance_us\":\"ETHUSD\",\"huobi\":\"ethusdt\",\"cryptocom\":\"ETH_USDT\",\"kucoin\":\"ETH-USDT\",\"gate\":\"eth_usdt\",\"pyth\":\"0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace\"}"],["usdt.tether-token.near","{\"decimals\":6,\"stablecoin\":true,\"pyth\":\"0x2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b\"}"],["17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1","{\"decimals\":6,\"stablecoin\":true,\"cryptocom\":\"USDC_USDT\",\"kucoin\":\"USDC-USDT\",\"pyth\":\"0xeaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a\"}"],["nbtc.bridge.near","{\"decimals\":8,\"binance_us\":\"BTCUSD\",\"huobi\":\"btcusdt\",\"cryptocom\":\"BTC_USDT\",\"kucoin\":\"BTC-USDT\",\"gate\":\"btc_usdt\",\"pyth\":\"0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43\"}"],["2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near","{\"decimals\":8,\"huobi\":\"wbtcusdt\",\"cryptocom\":\"WBTC_USDT\",\"kucoin\":\"WBTC-USDT\",\"gate\":\"wbtc_usdt\",\"pyth\":\"0xc9d8b075a5c69303365ae23633d4e085199bf5c520a3b90fed1322a0342ffc33\"}"],["6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near","{\"decimals\":18,\"stablecoin\":true,\"huobi\":\"daiusdt\",\"cryptocom\":\"DAI_USDT\",\"gate\":\"dai_usdt\",\"pyth\":\"0xb0948a5e5313200c632b51bb5ca32f6de0d36e9950a942d19751e833f70dabfd\"}"],["aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near","{\"decimals\":18,\"cryptocom\":\"AURORA_USDT\",\"huobi\":\"aurorausdt\",\"kucoin\":\"AURORA-USDT\",\"gate\":\"aurora_usdt\",\"pyth\":\"0x2f7c4f738d498585065a4b87b637069ec99474597da7f0ca349ba8ac3ba9cac5\"}"],["4691937a7508860f876c9c0a2a617e7d9e945d4b.factory.bridge.near","{\"decimals\":18,\"huobi\":\"woousdt\",\"cryptocom\":\"WOO_USDT\",\"kucoin\":\"WOO-USDT\",\"gate\":\"woo_usdt\",\"pyth\":\"0xb82449fd728133488d2d41131cffe763f9c1693b73c544d9ef6aaa371060dd25\"}"],["853d955acef822db058eb8505911ed77f175b99e.factory.bridge.near","{\"decimals\":18,\"stablecoin\":true,\"pyth\":\"0x7c53208632935ba5122c3cf65a0f4b3e72ba4955b49ad6ba0acf3d9ba405aef3\"}"],["22.contract.portalbridge.near","{\"decimals\":8,\"binance_us\":\"SOLUSD\",\"huobi\":\"solusdt\",\"cryptocom\":\"SOL_USDT\",\"kucoin\":\"SOL-USDT\",\"gate\":\"sol_usdt\",\"pyth\":\"0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d\"}"],["zec.omft.near","{\"decimals\":8,\"huobi\":\"zecusdt\",\"kucoin\":\"ZEC-USDT\",\"gate\":\"zec_usdt\",\"pyth\":\"0xbe9b59d178f0d6a97ab4c343bff2aa69caa1eaae3e9048a65788c529b125bb24\"}"],["token.rhealab.near","{\"decimals\":18,\"binance_alpha\":\"0x4c067de26475e1cefee8b8d1f6e2266b33a2372e\",\"pyth\":\"0xded2a0d2624278a32c56725397cc98b24ddb83d8c4d2ce108b1fc44b1d8de22b\"}"],["xrp.omft.near","{\"decimals\":6,\"binance_us\":\"XRPUSD\",\"huobi\":\"xrpusdt\",\"cryptocom\":\"XRP_USDT\",\"kucoin\":\"XRP-USDT\",\"gate\":\"xrp_usdt\",\"pyth\":\"0xec5d399846a9209f3fe5881d70aae9268c94339ff9817e8d18ff19fa05eea1c8\"}"],["doge.omft.near","{\"decimals\":8,\"binance_us\":\"DOGEUSD\",\"huobi\":\"dogeusdt\",\"cryptocom\":\"DOGE_USDT\",\"kucoin\":\"DOGE-USDT\",\"gate\":\"doge_usdt\",\"pyth\":\"0xdcef50dd0a4cd2dcc17e45df1676dcb336a11a61c69df7a0299b0150c672d25c\"}"],["cardano.omft.near","{\"decimals\":6,\"binance_us\":\"ADAUSD\",\"huobi\":\"adausdt\",\"cryptocom\":\"ADA_USDT\",\"kucoin\":\"ADA-USDT\",\"gate\":\"ada_usdt\",\"pyth\":\"0x2a01deaec9e51a579277b34b122399984d0bbf57e2458a7e42fecd2829867a0d\"}"],["xlm","{\"decimals\":7,\"binance_us\":\"XLMUSD\",\"huobi\":\"xlmusdt\",\"cryptocom\":\"XLM_USDT\",\"kucoin\":\"XLM-USDT\",\"gate\":\"xlm_usdt\",\"pyth\":\"0xb7a8eba68a997cd0210c2e1e4ee811ad2d174b3611c22d9ebf16f4cb7e9ba850\"}"],["ltc.omft.near","{\"decimals\":8,\"binance_us\":\"LTCUSD\",\"huobi\":\"ltcusdt\",\"cryptocom\":\"LTC_USDT\",\"kucoin\":\"LTC-USDT\",\"gate\":\"ltc_usdt\",\"pyth\":\"0x6e3f3fa8253588df9326580180233eb791e03b443a3ba7a1d892e73874e19a54\"}"]]}}}
EOJSON

echo "Step 1: Creating set_asset_exchange_configs proposal..."
near contract call-function as-transaction "$CONTRACT_ID" create_proposal \
  file-args "$TMPFILE" \
  prepaid-gas '300.0 Tgas' attached-deposit '0.1 NEAR' \
  sign-as "$OWNER_ACCOUNT" network-config "$NETWORK" sign-with-keychain send

rm -f "$TMPFILE"

echo ""
echo "Step 2: Syncing asset configs to WASI storage..."
near contract call-function as-transaction "$CONTRACT_ID" sync_asset_configs \
  json-args '{}' \
  prepaid-gas '300.0 Tgas' attached-deposit '0.05 NEAR' \
  sign-as "$OWNER_ACCOUNT" network-config "$NETWORK" sign-with-keychain send

echo ""
echo "Done! WASI will use updated configs on next execution."
