# Custom POST Body Support

## Summary

Added support for POST requests with JSON body in custom sources. This allows integration with APIs like Alchemy, Infura, and other RPC endpoints that require POST requests with JSON-RPC payloads.

## Changes

### 1. Types (`src/types.rs`)

Added optional `body` field to `CustomSourceConfig`:

```rust
pub struct CustomSourceConfig {
    pub url: String,
    pub json_path: String,
    #[serde(default = "default_value_type")]
    pub value_type: String,
    pub method: String,
    pub headers: Vec<(String, String)>,

    /// NEW: Optional JSON body for POST/PUT requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}
```

### 2. Sources (`src/sources.rs`)

Updated `fetch_custom()` to handle POST body:

```rust
"POST" => {
    let mut req = Client::new().post(&config.url);

    // Add body if provided
    if let Some(body) = &config.body {
        let body_str = serde_json::to_string(body)?;
        req = req.body(body_str.as_bytes());
        // Auto-add Content-Type: application/json
        if !config.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            req = req.header("Content-Type", "application/json");
        }
    }

    req
}
```

## Usage Example

### Alchemy Ethereum Balance (JSON-RPC)

```json
{
  "requests": [
    {
      "id": "eth_balance_wei",
      "sources": [
        {
          "name": "custom",
          "custom": {
            "url": "https://eth-mainnet.g.alchemy.com/v2",
            "method": "POST",
            "headers": [],
            "body": {
              "method": "eth_getBalance",
              "params": ["0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045", "latest"],
              "id": 1,
              "jsonrpc": "2.0"
            },
            "json_path": "result",
            "value_type": "string"
          }
        }
      ],
      "aggregation_method": "average",
      "min_sources_num": 1
    }
  ],
  "max_price_deviation_percent": 10.0
}
```

### With API Key (via secrets)

Set `API_KEY` environment variable (via NEAR OutLayer encrypted secrets):

```json
{
  "API_KEY": "your-alchemy-api-key-here"
}
```

The worker will automatically:
1. Read `API_KEY` from environment
2. Add `Authorization: Bearer {API_KEY}` header (GET and POST alike) **only when the URL points at an allowlisted provider over HTTPS** — see below
3. Send the request (POST with JSON body, or GET)
4. Extract value from response using `json_path`

### Where the API key is allowed to go

`API_KEY` is a credential OutLayer holds for us, while the custom-source URL comes from the
caller. It is therefore sent only to the hosts listed in `API_KEY_HOSTS`
(`src/security.rs`) — today `pro-api.coingecko.com` and `g.alchemy.com` — matched exactly or
on a dot boundary, and only over `https://`. Every other URL is fetched **without** the
header; a custom source pointing anywhere else must carry its own credential in `headers`.

Adding a host to that list hands our key to its operator, so it is a deliberate decision
rather than a config change.

### JSON Path

`json_path` supports dot notation and numeric array indices, e.g. `result`, `blocks.0.author_account_id`, or `1245620.data.price_overview.final`.

## Testing

```bash
# Build
env RUSTFLAGS="--cfg wasmedge --cfg tokio_unstable" cargo build --target wasm32-wasip1 --release

# Test with wasi-test-runner (requires wasi-test-runner to be built)
cd ../wasi-test-runner
cargo build --release

# Run Alchemy example (replace with your API key in secrets)
API_KEY="your-key" ./target/release/wasi-test \
  --wasm ../oracle-ark/target/wasm32-wasip1/release/oracle-ark.wasm \
  --input-file ../oracle-ark/example_alchemy_eth_balance.json \
  --max-instructions 50000000000
```

## Compatibility

- **WASI HTTP Client**: Requires `wasi-http-client` 0.2+ with POST body support
- **Content-Type**: Automatically set to `application/json` if not specified
- **Serialization**: Body is serialized using `serde_json::to_string()`
- **Backward Compatible**: GET requests work exactly as before

## Files Changed

1. `src/types.rs` - Added `body: Option<serde_json::Value>` field
2. `src/sources.rs` - Updated `fetch_custom()` to handle POST body
3. `README.md` - Added Custom Sources documentation section
4. `example_alchemy_eth_balance.json` - Example JSON-RPC request

## Notes

- Body is only sent for POST requests (ignored for GET)
- If `body` is provided, `Content-Type: application/json` is auto-added (unless already specified)
- API keys can be passed via `API_KEY` environment variable (added as a Bearer token to GET and POST requests, but only for the allowlisted HTTPS hosts above)
- JSON path extraction works the same for both GET and POST responses
