# Add URL Validation Security Layer for Oracle

## Summary

Add comprehensive URL validation to prevent WASM code from accessing local network resources, protecting against potential security vulnerabilities.

## Problem

While `wasi-http-client` cannot access Unix sockets or file systems, it could theoretically make HTTP requests to:
- Localhost services on the same host
- Private network addresses (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
- Docker/Kubernetes internal services
- Link-local addresses

## Solution

Implemented a security module that validates all URLs before HTTP requests:
- Blocks localhost and loopback addresses (127.0.0.0/8, ::1)
- Blocks private IPv4 networks (RFC 1918)
- Blocks private IPv6 networks (link-local, unique local)
- Blocks file:// and unix:// protocols
- Blocks Docker internal DNS (host.docker.internal)
- Blocks Kubernetes DNS (*.cluster.local, kubernetes)
- Allows only public internet addresses

## Implementation

- Added `security.rs` module with `validate_url()` function
- Integrated validation into all HTTP fetch functions
- Comprehensive test coverage for various attack vectors
- Zero performance impact (simple string/IP checks)

## Security Benefits

- **Defense in depth**: Additional layer even though WASM sandbox provides isolation
- **Prevents lateral movement**: Malicious code cannot scan internal networks
- **Protects other services**: Cannot access coordinator, databases, or other workers
- **Future-proof**: Protection remains even if sandbox has vulnerabilities

## Testing

```bash
cargo test security
```

All public API endpoints remain accessible while internal resources are blocked.