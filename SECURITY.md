# Oracle Example Security Best Practices

Security considerations for operating and integrating with the Oracle Example price oracle.

## Architecture Security

### TEE (Trusted Execution Environment)

All price fetching happens inside Intel TDX enclaves via Phala Cloud:
- WASI binary runs in isolated sandbox — cannot access host filesystem or processes
- Prices are fetched from external APIs inside the enclave
- TEE attestation proves the correct code was executed
- Workers cannot tamper with price data

### URL Validation (SSRF Protection)

The WASI module validates all outgoing HTTP requests (`src/security.rs`):
- Blocks localhost and loopback addresses (127.0.0.0/8, ::1)
- Blocks private IPv4 networks (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
- Blocks link-local addresses (169.254.0.0/16)
- Blocks Docker internal DNS (host.docker.internal, gateway.docker.internal)
- Blocks Kubernetes DNS (*.cluster.local, *.svc)
- Blocks file:// and unix:// protocols
- Only allows public internet addresses

This prevents SSRF attacks even if custom URL sources are configured.

### Price Validation

Multiple layers of price validation:
1. **Multi-source aggregation** — Prices fetched from 7+ independent sources
2. **Median aggregation** — Outlier prices don't affect the result
3. **Deviation check** — Alert when sources disagree beyond threshold
4. **Minimum sources** — Configurable `min_sources_num` (reject if too few sources respond)
5. **Freshness check** — Contract rejects stale prices via `recency_duration_sec`

---

## Key Management

### Governance: Council (DAO) + Owner

The contract is governed by a built-in **council (DAO)**. All state changes go through
`create_proposal` and execute only after the approval threshold (>50% of members) is met:
- Adding/removing oracles and assets
- Configuring OutLayer integration
- Enabling/disabling subsidized calls
- Registering push signers
- Contract upgrades

The **owner** role is now bootstrap-only and cannot directly mutate oracle/asset/config
state. It can call `set_council` (set the members) and `upload_upgrade_code` (stage
upgrade bytes for an `UpgradeContract` proposal), plus read-only getters.

**Best practices:**
- Set the owner to a multisig account (e.g., Sputnik DAO), and populate the council with
  independent members.
- Never store the owner or a council-member key on servers running the scheduler.
- Ownership itself is transferred through a council proposal:
  ```bash
  near call price-oracle.near create_proposal \
    '{"action": {"action": "update_owner", "owner_id": "oracle-dao.sputnik-dao.near"}}' \
    --accountId council-member.near --deposit 0.1
  ```

### Payment Key

The payment key funds scheduler WASI calls. It has limited scope:
- Can only trigger WASI executions on the associated project
- Cannot modify contract state or configuration
- Cannot withdraw funds from the contract

**Best practices:**
- Create separate payment keys for each environment (testnet, mainnet)
- Set reasonable funding limits (1-5 NEAR) to limit exposure
- Monitor balance and refill proactively
- Rotate keys periodically — create new key, update scheduler, delete old key
- Store payment key in Docker secrets or environment-specific `.env` files, never in git

### API Keys (Secrets)

API keys for premium data sources (CoinGecko Pro, etc.) are stored encrypted in OutLayer keystore:
- Keys are encrypted at rest
- Only decrypted inside TEE during WASI execution
- Workers cannot extract keys — only the WASI binary can access them as environment variables

**Best practices:**
- Use separate API keys per environment
- Set rate limits on API provider side
- Monitor API usage for anomalies
- Rotate API keys on a regular schedule

---

## Contract Security

### Access Control

| Method | Access | Notes |
|--------|--------|-------|
| `get_price_data`, `get_asset`, `get_oracle` | Public (view) | Free, no state changes |
| `oracle_call`, `request_price_data` | Public (call) | Requires deposit or subsidy |
| `custom_call`, `request_custom_data` | Public (call) | Requires deposit or subsidy |
| `report_prices` | Registered oracles only | Checked via `oracles` map |
| `add_oracle`, `remove_oracle` | Owner only | `assert_one_yocto` + `assert_owner` |
| `add_asset`, `remove_asset` | Owner only | `assert_one_yocto` + `assert_owner` |
| `configure_outlayer` | Owner only | `assert_owner` |
| `set_subsidize_outlayer_calls` | Owner only | `assert_one_yocto` + `assert_owner` |
| `upgrade` | Owner only | Full contract upgrade |
| `oracle_on_call` (callback) | Oracle contract only | Wrapper checks `predecessor_account_id` |

### Subsidy Mode Security

When `subsidize_outlayer_calls` is enabled:
- Contract pays 0.02 NEAR per OutLayer call from its own balance
- Only activates when balance > 20 NEAR (prevents draining)
- Any account can trigger price fetches — this is by design (public good)
- Monitor contract balance to ensure it stays above 20 NEAR

**Risk:** An attacker could drain the contract by making many oracle_call requests.

**Mitigations:**
- 20 NEAR minimum balance acts as a circuit breaker
- Each call costs 0.02 NEAR — draining 20 NEAR requires 1000 calls
- Gas costs for attacker add up quickly
- Consider adding rate limiting per account if this becomes an issue

### Callback Security

The wrapper contract validates callbacks:
```rust
assert_eq!(
    env::predecessor_account_id(),
    self.oracle_contract_id,
    "Callback only from oracle contract"
);
```

**Best practice:** Any contract implementing `oracle_on_call` must verify the caller is the expected oracle contract.

---

## Operational Security

### Scheduler Deployment

**Best practices:**
- Run scheduler in Docker with read-only filesystem where possible
- Use non-root user in container
- Limit container network access to only required endpoints
- Use Docker secrets for sensitive environment variables
- Enable container restart policy (`--restart unless-stopped`)
- Monitor container health

### Monitoring

Set up alerts for:
- **Scheduler failures** — 3+ consecutive failures trigger Telegram alert (built-in)
- **Price staleness** — Check `get_price_data` returns non-null prices
- **Contract balance** — Ensure > 20 NEAR if subsidy enabled
- **Payment key balance** — Refill before it runs out
- **Source failures** — Monitor which external APIs are failing

### Incident Response

1. **Prices are wrong**
   - Check which sources are returning bad data
   - Median aggregation should protect against single source failure
   - If multiple sources compromised: pause scheduler, investigate
   - Worst case: remove the oracle from contract to stop stale prices from being served

2. **Contract drained (subsidy mode)**
   - Contract automatically stops subsidizing when balance < 20 NEAR
   - Users must attach their own deposit to continue using oracle
   - Refill contract and investigate source of excessive calls

3. **Payment key compromised**
   - Delete the compromised key in OutLayer dashboard immediately
   - Create a new payment key
   - Update scheduler `.env` and restart
   - Old key can only spend its remaining balance — limited blast radius

4. **Owner key compromised**
   - Transfer ownership to a safe account immediately
   - Audit all recent owner-only calls (add/remove oracles, configure_outlayer)
   - Consider redeploying contract if configuration was tampered with

---

## Integration Security for DeFi

### Price Manipulation Resistance

The oracle is resistant to price manipulation because:
- Prices come from 7+ independent centralized exchanges
- Median aggregation means an attacker must compromise 4+ sources simultaneously
- TEE ensures the aggregation logic cannot be tampered with
- `min_sources_num` rejects results when too few sources respond

### Recommendations for integrators

1. **Always check for null prices** — `get_price_data` may return `price: null` if data is stale
2. **Set appropriate recency** — Use `recency_duration_sec` matching your needs (30s for DeFi, 300s for less critical use)
3. **Handle callback failures** — `oracle_on_call` might receive partial data; handle gracefully
4. **Don't trust a single source** — Use the multi-source oracle, not `FetchExternal` with one source
5. **Verify the oracle contract address** — Hardcode or use a DAO-controlled config, don't accept as user input
6. **Use 300 TGas for cross-contract calls** — The full call chain (wrapper -> oracle -> OutLayer -> callback) needs enough gas

### Price Format

Prices use integer arithmetic to avoid floating-point issues:
```
Price { multiplier: 500000000, decimals: 8 }  // = $5.00
```

Convert: `value = multiplier / 10^decimals`

Always use the `decimals` field — don't hardcode 8, as this may change for different assets.
