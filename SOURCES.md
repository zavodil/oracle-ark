# Available Price Sources

## Supported Sources

### Exchanges

1. **binance** - Binance global exchange
   - Example: `{"name": "binance", "id": "BTCUSDT"}`
   - Note: May return HTTP 451 in some regions due to geo-blocking

2. **binance-us** - Binance US exchange
   - Example: `{"name": "binance-us", "id": "BTCUSDT"}`
   - Alternative for US users

3. **huobi** - Huobi exchange
   - Example: `{"name": "huobi", "id": "btcusdt"}`

4. **cryptocom** - Crypto.com exchange
   - Example: `{"name": "cryptocom", "id": "BTC_USDT"}`

5. **kucoin** - KuCoin exchange
   - Example: `{"name": "kucoin", "id": "BTC-USDT"}`

6. **gate** - Gate.io exchange
   - Example: `{"name": "gate", "id": "btc_usdt"}`

### Price Aggregators

7. **coingecko** - CoinGecko API
   - Example: `{"name": "coingecko", "id": "bitcoin"}`
   - Optional API key via COINGECKO_API_KEY env

8. **coinmarketcap** - CoinMarketCap API
   - Example: `{"name": "coinmarketcap", "id": "BTC"}`
   - Requires API key via COINMARKETCAP_API_KEY env

### Oracle Networks

9. **pyth** - Pyth Network oracle
   - Example: `{"name": "pyth", "id": "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43"}`
   - Uses Hermes API endpoint

### Forex/Commodities

10. **twelvedata** - TwelveData API
    - Example: `{"name": "twelvedata", "id": "EUR/USD"}`
    - Optional API key via TWELVEDATA_API_KEY env

11. **exchangerate-api** - Exchange Rate API (free)
    - Example: `{"name": "exchangerate-api", "id": "EUR/USD"}`
    - No API key required

### Custom Sources

12. **custom** - User-defined HTTP endpoint
    - Requires `custom` configuration object
    - Example:
    ```json
    {
      "name": "custom",
      "custom": {
        "url": "https://api.example.com/price",
        "json_path": "data.price",
        "value_type": "number",
        "method": "GET",
        "headers": []
      }
    }
    ```

## Example Usage

```json
{
  "requests": [
    {
      "id": "bitcoin",
      "sources": [
        {"name": "coingecko", "id": "bitcoin"},
        {"name": "binance", "id": "BTCUSDT"},
        {"name": "binance-us", "id": "BTCUSDT"},
        {"name": "kucoin", "id": "BTC-USDT"}
      ],
      "aggregation_method": "median",
      "min_sources_num": 2
    }
  ],
  "max_price_deviation_percent": 5.0
}
```