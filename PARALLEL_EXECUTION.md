# Parallel Execution Implementation

## Summary

The oracle-ark WASM application has been updated to execute price fetching operations in parallel instead of sequentially. This significantly reduces the overall execution time when fetching prices from multiple sources.

## Key Changes

### 1. Constants Added (types.rs)
```rust
// Maximum timeout for each HTTP request in seconds
pub const MAX_REQUEST_TIMEOUT_SECS: u64 = 10;

// Maximum number of concurrent HTTP requests
pub const MAX_CONCURRENT_REQUESTS: usize = 5;
```

### 2. New Parallel Module (parallel.rs)
- **`fetch_prices_parallel()`**: Fetches prices from multiple sources in parallel
  - Uses `std::thread` for concurrency (WASI-P2 compatible)
  - Limits concurrent requests to `MAX_CONCURRENT_REQUESTS` (5)
  - Applies timeout of `MAX_REQUEST_TIMEOUT_SECS` (10 seconds) per request
  - Processes sources in batches to control resource usage

- **`process_data_requests_parallel()`**: Processes multiple token requests in parallel
  - Each token request runs in its own thread
  - Maintains original request order in results
  - Aggregates results using shared memory (Arc<Mutex>)

### 3. Main Function Updated (main.rs)
- Replaced sequential processing loop with parallel processing call
- Removed old `process_data_request()` function
- Now calls `parallel::process_data_requests_parallel()`

## Performance Benefits

### Before (Sequential)
- Request 1: Source A (2s) → Source B (1s) → Source C (3s) = 6s
- Request 2: Source D (1s) → Source E (2s) = 3s
- **Total: 9 seconds**

### After (Parallel)
- Request 1 & 2 run concurrently
- Within each request, sources fetch in parallel (max 5 concurrent)
- Request 1: max(2s, 1s, 3s) = 3s
- Request 2: max(1s, 2s) = 2s
- **Total: ~3 seconds** (both requests run in parallel)

## Configuration

The parallel execution behavior can be configured in two ways:

### 1. Default Values (in types.rs)
- `MAX_CONCURRENT_REQUESTS = 10`: Default max concurrent HTTP requests
- `MAX_REQUEST_TIMEOUT_SECS = 10`: Default timeout per request (seconds)

### 2. Per-Request Configuration (via input JSON)
You can override the default values by providing an optional `config` object in your request:

```json
{
  "requests": [...],
  "max_price_deviation_percent": 5.0,
  "config": {
    "max_concurrent_requests": 3,
    "request_timeout_secs": 5
  }
}
```

**Configuration fields:**
- `max_concurrent_requests` (optional): Number of simultaneous HTTP requests (default: 10)
- `request_timeout_secs` (optional): Timeout per request in seconds (default: 10)

If the `config` object is omitted, the system uses default values.

## Compatibility

- **WASI-P2 Compatible**: Uses async/await with tokio single-threaded runtime
- **Concurrent Execution**: Uses futures for concurrent (not parallel) execution
- **Resource-controlled**: Limits concurrent operations to prevent resource exhaustion

### Implementation Details

Due to WASI limitations:
1. **No OS threads**: WASI doesn't support `std::thread::spawn`, so we use async concurrency
2. **Single-threaded tokio runtime**: Uses `#[tokio::main(flavor = "current_thread")]`
3. **Concurrent, not parallel**: Multiple HTTP requests are multiplexed on a single thread
4. **Stream-based concurrency**: Uses `futures::stream` with `buffer_unordered` for controlled concurrency

Despite being single-threaded, the async implementation provides significant performance benefits:
- HTTP requests are non-blocking and interleaved
- While one request waits for network I/O, others can proceed
- Effective utilization of the single thread through cooperative multitasking

## Testing

Test with the provided test files:

### Test with custom configuration (3 concurrent requests, 5 second timeout):
```bash
cat test_config.json | wasmtime oracle-ark.wasm
```

### Test with default configuration (10 concurrent requests, 10 second timeout):
```bash
cat test_default.json | wasmtime oracle-ark.wasm
```

### Test with original parallel test:
```bash
cat test_parallel.json | wasmtime oracle-ark.wasm
```

The tests fetch cryptocurrency prices from multiple sources in parallel.

## Future Improvements

1. Make concurrency limits configurable via environment variables
2. Add request retries with exponential backoff
3. Implement connection pooling for better performance
4. Add circuit breaker pattern for failing sources