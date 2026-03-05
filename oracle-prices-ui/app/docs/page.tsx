'use client';

import { useState } from 'react';
import Link from 'next/link';

const sections = [
  { id: 'overview', name: 'Overview' },
  { id: 'quick-start', name: 'Quick Start' },
  { id: 'governance', name: 'Governance & Security' },
  { id: 'pyth-native', name: 'Pyth Interface' },
  { id: 'direct-outlayer', name: 'Direct OutLayer Integration' },
  { id: 'price-oracle', name: 'Price Oracle Contract' },
  { id: 'wrapper-example', name: 'Integration Example' },
  { id: 'custom-data', name: 'Custom Data Sources' },
  { id: 'code-examples', name: 'Code Examples' },
  { id: 'deposits', name: 'Deposit Requirements' },
];

export default function DocsPage() {
  const [activeSection, setActiveSection] = useState('overview');

  const scrollToSection = (id: string) => {
    setActiveSection(id);
    const element = document.getElementById(id);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth' });
    }
  };

  return (
    <div className="min-h-screen py-8 px-4">
      <div className="max-w-7xl mx-auto">
        <div className="lg:grid lg:grid-cols-[250px_1fr] lg:gap-8">
          {/* Sidebar Navigation */}
          <nav className="hidden lg:block sticky top-24 h-fit">
            <h3 className="text-sm font-semibold text-dark-400 uppercase tracking-wider mb-4">
              Documentation
            </h3>
            <ul className="space-y-1">
              {sections.map((section) => (
                <li key={section.id}>
                  <button
                    onClick={() => scrollToSection(section.id)}
                    className={`w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${
                      activeSection === section.id
                        ? 'bg-primary/20 text-primary'
                        : 'text-dark-400 hover:text-white hover:bg-dark-800'
                    }`}
                  >
                    {section.name}
                  </button>
                </li>
              ))}
            </ul>
            <div className="mt-8 pt-8 border-t border-dark-800">
              <a
                href="https://github.com/zavodil/oracle-ark"
                target="_blank"
                rel="noopener noreferrer"
                className="text-dark-400 hover:text-white text-sm flex items-center"
              >
                View on GitHub
                <svg className="w-4 h-4 ml-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                </svg>
              </a>
            </div>
          </nav>

          {/* Main Content */}
          <main className="prose prose-invert max-w-none">
            {/* Overview */}
            <section id="overview" className="mb-16">
              <h1 className="text-4xl font-bold text-white mb-4">
                TEE-Secured Price Oracle
              </h1>
              <p className="text-xl text-dark-300 mb-8">
                On-Demand Oracle with Sustainable Economics — Based on OutLayer
              </p>

              <div className="card mb-8">
                <h3 className="text-lg font-semibold text-white mb-4">Key Features</h3>
                <ul className="space-y-2 text-dark-300">
                  <li>✓ Proactive price pushing — prices always fresh in contract (30-60s updates)</li>
                  <li>✓ Zero trust — all data processed inside Intel TDX enclave</li>
                  <li>✓ DAO-governed — all configuration managed through council proposals</li>
                  <li>✓ TEE-only signing — only keys generated inside TEE can push prices</li>
                  <li>✓ 10 price sources with median aggregation</li>
                  <li>✓ Native Pyth-compatible API — migrate from pyth-oracle.near by changing one address</li>
                  <li>✓ Custom data fetching from any HTTP API</li>
                  <li>✓ Subsidized mode — free calls when contract has funds</li>
                </ul>
              </div>

              <div className="grid md:grid-cols-3 gap-4">
                <div className="card text-center">
                  <div className="text-3xl font-bold text-primary mb-2">21</div>
                  <div className="text-dark-400 text-sm">Supported Tokens</div>
                </div>
                <div className="card text-center">
                  <div className="text-3xl font-bold text-primary mb-2">10</div>
                  <div className="text-dark-400 text-sm">Price Sources</div>
                </div>
                <div className="card text-center">
                  <div className="text-3xl font-bold text-primary mb-2">&lt;1s</div>
                  <div className="text-dark-400 text-sm">Response Time</div>
                </div>
              </div>
            </section>

            {/* Quick Start */}
            <section id="quick-start" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Quick Start</h2>

              <div className="card border-green-500/50 mb-6">
                <div className="flex items-center gap-2 mb-4">
                  <span className="px-2 py-1 text-xs bg-green-500/20 text-green-400 rounded">Recommended</span>
                  <h3 className="text-lg font-semibold text-white">Get Prices with Callback</h3>
                </div>
                <p className="text-dark-400 text-sm mb-4">
                  Use <code className="text-primary">oracle_call</code> to request prices. Your contract receives data via <code className="text-primary">oracle_on_call</code> callback.
                </p>
                <pre className="bg-dark-950 rounded-lg p-4 overflow-x-auto">
                  <code className="text-green-400">
{`near call price-oracle.near oracle_call '{
  "receiver_id": "your-contract.near",
  "asset_ids": ["wrap.near", "aurora"],
  "msg": ""
}' --accountId your.near --deposit 0.02 --gas 200000000000000`}
                  </code>
                </pre>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Direct Price Request (no callback)</h3>
              <p className="text-dark-400 text-sm mb-4">For scripts and testing — returns prices directly.</p>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-green-400">
{`near call price-oracle.near request_price_data '{
  "asset_ids": ["wrap.near"]
}' --accountId your.near --deposit 0.02 --gas 200000000000000`}
                </code>
              </pre>

              <div className="card border-blue-500/30 bg-blue-500/5 mb-6">
                <div className="flex items-start gap-3">
                  <div className="text-blue-400 text-xl">i</div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-2">About View Methods (get_price_data)</h3>
                    <p className="text-dark-400 text-sm mb-3">
                      <code className="text-primary">get_price_data</code> is a free view method. Prices are <strong className="text-white">proactively pushed</strong> to the contract every 30-60 seconds by TEE workers, so data is always fresh.
                    </p>
                    <p className="text-dark-400 text-sm">
                      You can also call <code className="text-primary">request_price_data</code> to trigger an immediate on-demand update from TEE if needed.
                    </p>
                  </div>
                </div>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Response Format</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto">
                <code className="text-blue-400">
{`{
  "timestamp": "1706889600000000000",
  "recency_duration_sec": 120,
  "prices": [
    {
      "asset_id": "wrap.near",
      "price": { "multiplier": "500000000", "decimals": 8 }
    }
  ]
}
// Price conversion: 500000000 / 10^8 = $5.00`}
                </code>
              </pre>
            </section>

            {/* Governance & Security */}
            <section id="governance" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Governance & Security</h2>

              <div className="card border-green-500/30 bg-green-500/5 mb-6">
                <div className="flex items-start gap-3">
                  <div className="text-green-400 text-xl">✓</div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-2">All Changes Go Through DAO</h3>
                    <p className="text-dark-400 text-sm">
                      Every contract state mutation — adding assets, configuring exchanges, registering push signers, upgrading the contract — requires a DAO council proposal with &gt;50% approval. No single key can modify the oracle.
                    </p>
                  </div>
                </div>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">TEE-Only Price Pushing</h3>
              <p className="text-dark-300 mb-4">
                Prices are pushed to the contract by <strong className="text-white">implicit accounts derived from TEE-generated keys</strong>.
                The private key is created inside the TEE (Intel TDX) and never leaves it — no human, including the project owner, ever sees it.
              </p>
              <div className="card bg-dark-900 mb-6">
                <pre className="text-sm text-dark-300 overflow-x-auto">
{`How PROTECTED_ keys work:

1. Project owner creates a secret (e.g., PROTECTED_KEY_RHEA) in OutLayer dashboard
2. Private key generated INSIDE TEE — never exposed to anyone
3. DAO proposal registers the derived implicit account as trusted oracle
4. Only this account can call report_prices for assigned assets
5. WASI code inside TEE signs transactions with the key

Result: No human holds the signing key. Only verified TEE code can push prices.`}
                </pre>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">DAO Proposal Actions</h3>
              <div className="overflow-x-auto mb-6">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-dark-700">
                      <th className="py-3 px-4 text-dark-300">Action</th>
                      <th className="py-3 px-4 text-dark-300">Description</th>
                    </tr>
                  </thead>
                  <tbody className="text-dark-400">
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">AddAsset / RemoveAsset</code></td>
                      <td className="py-3 px-4">Manage tracked assets</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">SetAssetExchangeConfig</code></td>
                      <td className="py-3 px-4">Configure exchange tickers, Pyth/Chainlink feeds per asset</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">RegisterPushSigner</code></td>
                      <td className="py-3 px-4">Register TEE-derived account as trusted price pusher</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">ConfigureOutlayer</code></td>
                      <td className="py-3 px-4">Set OutLayer integration parameters</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">ProposeUpgrade / ExecuteUpgrade</code></td>
                      <td className="py-3 px-4">Two-phase contract upgrade via DAO vote</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Self-Service for Projects</h3>
              <p className="text-dark-300 mb-4">
                Third-party projects can operate their own push signers:
              </p>
              <ol className="list-decimal list-inside text-dark-300 space-y-2 mb-4">
                <li>Create a TEE secret (<code className="text-primary">PROTECTED_KEY_*</code>) in OutLayer dashboard</li>
                <li>DAO proposal to register the key for specific assets</li>
                <li>Fund the derived implicit account with NEAR</li>
                <li>Scheduler pushes prices autonomously from TEE</li>
              </ol>
            </section>

            {/* Native Pyth Interface */}
            <section id="pyth-native" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Native Pyth Interface</h2>
              <p className="text-dark-300 mb-6">
                <code className="text-primary">price-oracle.near</code> implements Pyth-compatible view methods natively.
                DeFi contracts using <code>pyth-oracle.near</code> can migrate by changing one contract address — no code changes needed.
              </p>

              <div className="card border-green-500/30 bg-green-500/5 mb-6">
                <div className="flex items-start gap-3">
                  <div className="text-green-400 text-xl">✓</div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-2">No refresh_prices Needed</h3>
                    <p className="text-dark-400 text-sm">
                      Unlike the separate Pyth wrapper contract, the native interface reads directly from contract state which is
                      proactively updated every 30-60 seconds by the scheduler. View methods always return fresh data.
                    </p>
                  </div>
                </div>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">View Methods (free, always fresh)</h3>
              <div className="overflow-x-auto mb-8">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-dark-700">
                      <th className="py-3 px-4 text-dark-300">Method</th>
                      <th className="py-3 px-4 text-dark-300">Description</th>
                    </tr>
                  </thead>
                  <tbody className="text-dark-400">
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_price(price_identifier)</code></td>
                      <td className="py-3 px-4">Latest price with staleness check</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_price_unsafe(price_identifier)</code></td>
                      <td className="py-3 px-4">Latest price without staleness check</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_price_no_older_than(price_id, age)</code></td>
                      <td className="py-3 px-4">Price only if published within <code>age</code> seconds</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_ema_price(price_id)</code></td>
                      <td className="py-3 px-4">EMA price with staleness check</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_ema_price_unsafe(price_id)</code></td>
                      <td className="py-3 px-4">EMA price without staleness check</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">list_prices(price_ids)</code></td>
                      <td className="py-3 px-4">Batch: multiple feeds at once</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">price_feed_exists(price_identifier)</code></td>
                      <td className="py-3 px-4">Check if feed is configured</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_update_fee_estimate(data)</code></td>
                      <td className="py-3 px-4">Returns 1 yoctoNEAR (no update needed)</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Migration from Pyth</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-blue-400">
{`// Before (Pyth)
const ORACLE: &str = "pyth-oracle.near";

// After (Oracle-Ark) — no other changes needed!
const ORACLE: &str = "price-oracle.near";`}
                </code>
              </pre>

              <h3 className="text-lg font-semibold text-white mb-4">Response Format</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto">
                <code className="text-blue-400">
{`// PythPrice format (same as pyth-oracle.near)
{
  "price": 525000000,      // price * 10^|expo|
  "conf": 0,               // confidence (always 0 for Oracle-Ark)
  "expo": -8,              // exponent: actual_price = price * 10^expo
  "publish_time": 1706900000  // unix timestamp (seconds)
}
// Example: price=525000000, expo=-8 → $5.25`}
                </code>
              </pre>
            </section>

            {/* Direct OutLayer Integration */}
            <section id="direct-outlayer" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Direct OutLayer Integration</h2>
              <p className="text-dark-300 mb-6">
                You don&apos;t need to use <code className="text-primary">price-oracle.near</code> at all.
                Your contract can call OutLayer directly to fetch prices or any custom data from TEE.
              </p>

              <div className="card border-purple-500/30 bg-purple-500/5 mb-6">
                <div className="flex items-start gap-3">
                  <div className="text-purple-400 text-xl">★</div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-2">Why Go Direct?</h3>
                    <ul className="text-dark-400 text-sm space-y-2">
                      <li>No intermediary contracts — full control over the flow</li>
                      <li>Custom WASI workers — fetch any data you need</li>
                      <li>Lower gas costs — one less cross-contract call</li>
                      <li>Your contract owns the entire integration</li>
                    </ul>
                  </div>
                </div>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Step 1: Call OutLayer request_execution</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-blue-400">
{`use near_sdk::{ext_contract, AccountId, NearToken, Promise, serde_json};

#[ext_contract(ext_outlayer)]
pub trait OutLayer {
    fn request_execution(
        &mut self,
        execution_source: serde_json::Value,
        resource_limits: Option<serde_json::Value>,
        input_data: Option<String>,
        secrets_ref: Option<serde_json::Value>,
        response_format: Option<String>,
        payer_account_id: Option<AccountId>,
        callback_receiver_id: Option<AccountId>,
    ) -> Promise;
}

impl Contract {
    pub fn fetch_price(&mut self, token_id: String) -> Promise {
        // Use the deployed price oracle project
        // Mainnet: "price-oracle.near/price-oracle"
        // Testnet: "price-oracle.testnet/price-oracle"
        let execution_source = serde_json::json!({
            "Project": {
                "project_id": "price-oracle.near/price-oracle"
            }
        });

        // Resource limits (recommended)
        let resource_limits = serde_json::json!({
            "max_instructions": 10000000000_u64,
            "max_memory_mb": 128,
            "max_execution_seconds": 60
        });

        // Input data for the WASI worker (see OracleCommand in types.rs)
        let input_data = serde_json::json!({
            "command": "get_prices",
            "tokens": [token_id]
        }).to_string();

        // Call OutLayer directly
        ext_outlayer::ext("outlayer.near".parse().unwrap())
            .with_attached_deposit(NearToken::from_millinear(10)) // 0.01 NEAR
            .with_unused_gas_weight(1)
            .request_execution(
                execution_source,
                Some(resource_limits),          // resource limits
                Some(input_data),               // your request
                None,                           // no secrets needed
                Some("json".to_string()),       // response format
                Some(env::predecessor_account_id()), // payer
                Some(env::current_account_id()), // callback receiver
            )
    }
}`}
                </code>
              </pre>

              <h3 className="text-lg font-semibold text-white mb-4">Step 2: Handle the Callback</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-blue-400">
{`// OutLayer calls this method with the TEE result
#[private] // Only callable by self (via promise)
pub fn on_outlayer_result(
    &mut self,
    #[callback_result] result: Result<serde_json::Value, near_sdk::PromiseError>,
) {
    match result {
        Ok(data) => {
            // Parse the price data from TEE response
            if let Some(prices) = data.get("prices") {
                // Process your prices here
                log!("Got prices from TEE: {:?}", prices);
            }
        }
        Err(e) => {
            log!("OutLayer call failed: {:?}", e);
        }
    }
}`}
                </code>
              </pre>

              <h3 className="text-lg font-semibold text-white mb-4">Architecture: Direct vs Via Oracle Contract</h3>
              <div className="card bg-dark-900 mb-6">
                <pre className="text-sm text-dark-300 overflow-x-auto">
{`Via price-oracle.near (simpler):
Your Contract → price-oracle.near → OutLayer → TEE → price-oracle.near → Your Contract

Direct OutLayer (more control):
Your Contract → OutLayer → TEE → Your Contract

Both are valid! Use price-oracle.near for quick integration,
or go direct for full customization.`}
                </pre>
              </div>

              <div className="card border-yellow-500/30 bg-yellow-500/5">
                <div className="flex items-start gap-3">
                  <div className="text-yellow-400 text-xl">⚠️</div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-2">Important Notes</h3>
                    <ul className="text-dark-400 text-sm space-y-2">
                      <li>You need to deploy your own WASI worker or use an existing one (like the price oracle WASI)</li>
                      <li>For price fetching, it&apos;s easier to use <code className="text-primary">price-oracle.near</code> — it handles WASI configuration for you</li>
                      <li>Direct integration is best for custom data sources or when you need full control</li>
                      <li>See{' '}
                        <a href="https://github.com/zavodil/oracle-ark/tree/main/contract" target="_blank" rel="noopener noreferrer" className="text-primary hover:underline">
                          price-oracle contract source
                        </a>
                        {' '}for a complete example
                      </li>
                    </ul>
                  </div>
                </div>
              </div>
            </section>

            {/* Price Oracle Contract */}
            <section id="price-oracle" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Price Oracle Contract</h2>

              <div className="card border-blue-500/30 bg-blue-500/5 mb-6">
                <div className="flex items-start gap-3">
                  <div className="text-blue-400 text-xl">i</div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-2">Always-fresh prices</h3>
                    <p className="text-dark-400 text-sm mb-3">
                      <code className="text-primary">price-oracle.near</code> receives proactive price updates every 30-60 seconds from TEE workers.
                      View methods like <code className="text-primary">get_price_data</code> always return fresh data — no paid call needed.
                    </p>
                    <p className="text-dark-400 text-sm">
                      You can also integrate with OutLayer directly from your own contract (see Direct OutLayer Integration section above).
                    </p>
                  </div>
                </div>
              </div>

              <p className="text-dark-300 mb-6">
                Contract address: <code className="text-primary">price-oracle.near</code>
              </p>
              <p className="text-dark-400 text-sm mb-6">
                This contract recreates the interface (with additions) of the original{' '}
                <a href="https://github.com/NearDeFi/price-oracle" target="_blank" rel="noopener noreferrer" className="text-primary hover:underline">
                  NEAR Native Price Oracle
                </a>{' '}
                — existing integrations can migrate with minimal changes.
              </p>

              <h3 className="text-lg font-semibold text-white mb-4">View Methods (free)</h3>
              <div className="card border-blue-500/30 bg-blue-500/5 mb-4">
                <div className="flex items-start gap-3">
                  <div className="text-blue-400 text-lg">i</div>
                  <p className="text-dark-400 text-sm">
                    Prices are proactively pushed every 30-60 seconds. <code className="text-primary">get_price_data</code> always returns fresh data.
                    You can also call <code className="text-primary">request_price_data</code> for an immediate on-demand update.
                  </p>
                </div>
              </div>
              <div className="overflow-x-auto mb-8">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-dark-700">
                      <th className="py-3 px-4 text-dark-300">Method</th>
                      <th className="py-3 px-4 text-dark-300">Arguments</th>
                      <th className="py-3 px-4 text-dark-300">Description</th>
                    </tr>
                  </thead>
                  <tbody className="text-dark-400">
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_price_data</code></td>
                      <td className="py-3 px-4"><code>asset_ids?: string[]</code></td>
                      <td className="py-3 px-4">Get cached prices (returns null if cache empty)</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">can_subsidize_outlayer_calls</code></td>
                      <td className="py-3 px-4">—</td>
                      <td className="py-3 px-4">Check if contract pays for calls</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">get_oracle_price_data</code></td>
                      <td className="py-3 px-4"><code>account_id, asset_ids?</code></td>
                      <td className="py-3 px-4">Get prices from specific oracle</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Call Methods (require deposit)</h3>
              <div className="overflow-x-auto mb-8">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-dark-700">
                      <th className="py-3 px-4 text-dark-300">Method</th>
                      <th className="py-3 px-4 text-dark-300">Deposit</th>
                      <th className="py-3 px-4 text-dark-300">Description</th>
                    </tr>
                  </thead>
                  <tbody className="text-dark-400">
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">request_price_data</code></td>
                      <td className="py-3 px-4 text-yellow-400">0.01+ NEAR</td>
                      <td className="py-3 px-4">Get prices directly</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">oracle_call</code></td>
                      <td className="py-3 px-4 text-yellow-400">0.01+ NEAR</td>
                      <td className="py-3 px-4">Get prices with callback</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">request_custom_data</code></td>
                      <td className="py-3 px-4 text-yellow-400">0.01+ NEAR</td>
                      <td className="py-3 px-4">Fetch custom external data</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code className="text-primary">custom_call</code></td>
                      <td className="py-3 px-4 text-yellow-400">0.01+ NEAR</td>
                      <td className="py-3 px-4">Custom data with callback</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Data Types</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-blue-400">
{`// Price format: multiplier / 10^decimals = USD
struct Price {
    multiplier: u128,  // e.g., 500000000 for $5.00
    decimals: u8,      // usually 8
}

struct PriceData {
    timestamp: u64,              // nanoseconds
    recency_duration_sec: u32,   // max age for "fresh" prices
    prices: Vec<AssetOptionalPrice>,
}

struct AssetOptionalPrice {
    asset_id: String,
    price: Option<Price>,  // None if stale/unavailable
}`}
                </code>
              </pre>

              <h3 className="text-lg font-semibold text-white mb-4">Callback Interface</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto">
                <code className="text-blue-400">
{`// Your contract must implement this for oracle_call
pub fn oracle_on_call(
    &mut self,
    sender_id: AccountId,
    data: PriceData,
    msg: String,
) {
    // Verify caller is the oracle
    assert_eq!(
        env::predecessor_account_id(),
        "price-oracle.near".parse::<AccountId>().unwrap(),
        "Only oracle can call"
    );
    // Process prices...
}`}
                </code>
              </pre>
            </section>

            {/* Wrapper Example */}
            <section id="wrapper-example" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Integration Example: Wrapper Contract</h2>
              <p className="text-dark-300 mb-6">
                A complete example showing how to integrate the oracle with the full callback cycle.
                The wrapper contract self-funds oracle calls and handles callbacks internally.
              </p>
              <p className="text-dark-300 mb-6">
                Contract: <code className="text-primary">price-oracle-wrapper.near</code> |{' '}
                <a
                  href="https://github.com/zavodil/oracle-ark/tree/main/wrapper-contract"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-primary hover:underline"
                >
                  Source on GitHub
                </a>
              </p>

              <h3 className="text-lg font-semibold text-white mb-4">How It Works</h3>
              <div className="card bg-dark-900 mb-6">
                <pre className="text-sm text-dark-300 overflow-x-auto">
{`User calls get_price() on Wrapper
        │
        ▼
Wrapper calls oracle_call() with SELF as receiver_id
(self-funded: 0.02 NEAR attached automatically)
        │
        ▼
Oracle processes request via OutLayer TEE
        │
        ▼
Oracle calls oracle_on_call() on Wrapper
        │
        ▼
Wrapper receives prices in callback, processes them`}
                </pre>
              </div>

              <h3 className="text-lg font-semibold text-white mb-4">Key Pattern: Self-Funding Calls</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-blue-400">
{`// Wrapper pays for oracle calls itself - users don't need to attach deposits
pub fn get_price(&mut self, token_id: String) -> Promise {
    ext_oracle::ext(self.oracle_contract_id.clone())
        .with_attached_deposit(NearToken::from_millinear(20)) // 0.02 NEAR
        .with_unused_gas_weight(1)
        .oracle_call(
            env::current_account_id(), // callback comes back HERE
            Some(vec![token_id]),
            String::new(),
            None,
        )
}

// Callback handler - called by oracle with price data
pub fn oracle_on_call(
    &mut self,
    sender_id: AccountId,
    data: PriceData,
    msg: String,
) -> Option<Price> {
    // IMPORTANT: verify caller is the oracle!
    assert_eq!(env::predecessor_account_id(), self.oracle_contract_id);

    // Extract price from data
    if let Some(asset) = data.prices.first() {
        return asset.price.clone();
    }
    None
}`}
                </code>
              </pre>

              <div className="card border-green-500/30 bg-green-500/5 mb-6">
                <div className="flex items-start gap-3">
                  <div className="text-green-400 text-xl">✓</div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-2">Why This Pattern?</h3>
                    <ul className="text-dark-400 text-sm space-y-2">
                      <li><strong className="text-white">Self-funding:</strong> Users call your contract without deposits — your contract pays for oracle calls</li>
                      <li><strong className="text-white">Full cycle:</strong> Request → TEE → Callback all handled in one user transaction</li>
                      <li><strong className="text-white">Security:</strong> Always verify <code className="text-primary">predecessor_account_id</code> in callbacks</li>
                      <li><strong className="text-white">Context:</strong> Use the <code className="text-primary">msg</code> field to pass context through async chain</li>
                    </ul>
                  </div>
                </div>
              </div>

              <div className="card border-blue-500/30 bg-blue-500/5">
                <div className="flex items-start gap-3">
                  <div className="text-blue-400 text-xl">i</div>
                  <div>
                    <p className="text-dark-400 text-sm">
                      <strong className="text-white">All example contracts are optional!</strong> The contracts we provide (<code className="text-primary">price-oracle.near</code>, <code className="text-primary">price-oracle-wrapper.near</code>, etc.) are just examples.
                      You can integrate with OutLayer directly from your own contract — see the next section.
                    </p>
                  </div>
                </div>
              </div>
            </section>

            {/* Legacy Pyth Wrapper — redirect to native */}
            <section id="pyth-wrapper" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Legacy Pyth Wrapper</h2>
              <div className="card border-blue-500/30 bg-blue-500/5 mb-6">
                <div className="flex items-start gap-3">
                  <div className="text-blue-400 text-xl">i</div>
                  <div>
                    <p className="text-dark-400 text-sm mb-3">
                      <strong className="text-white">Pyth-compatible methods are now built into <code className="text-primary">price-oracle.near</code> directly.</strong>{' '}
                      The separate <code className="text-dark-300">price-oracle-pyth.near</code> wrapper is no longer needed.
                    </p>
                    <p className="text-dark-400 text-sm">
                      See the{' '}
                      <button onClick={() => scrollToSection('pyth-native')} className="text-primary hover:underline">
                        Native Pyth Interface
                      </button>{' '}
                      section for migration instructions. Simply change your contract address to <code className="text-primary">price-oracle.near</code> — all Pyth view methods work natively, with always-fresh prices (no <code className="text-dark-300">refresh_prices</code> call needed).
                    </p>
                  </div>
                </div>
              </div>
            </section>

            {/* Custom Data */}
            <section id="custom-data" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Custom Data Sources</h2>
              <p className="text-dark-300 mb-6">
                Fetch data from any HTTP API via TEE using <code>request_custom_data</code> or <code>custom_call</code>.
              </p>

              <h3 className="text-lg font-semibold text-white mb-4">Request Format</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-blue-400">
{`{
  "custom_data_request": [
    {
      "id": "my_data",           // Identifier for the result
      "token_id": "",            // Optional token identifier
      "source": {
        "custom": {
          "url": "https://api.example.com/data",
          "json_path": "result.value",   // Dot notation path
          "value_type": "number",        // "number", "string", "boolean"
          "method": "GET",               // "GET" or "POST"
          "headers": []                  // Optional headers
        }
      }
    }
  ]
}`}
                </code>
              </pre>

              <h3 className="text-lg font-semibold text-white mb-4">Examples</h3>
              <div className="space-y-4">
                <div className="card">
                  <h4 className="font-medium text-white mb-2">Steam Game Price</h4>
                  <pre className="bg-dark-950 rounded p-3 text-xs overflow-x-auto">
                    <code className="text-green-400">
{`{
  "url": "https://store.steampowered.com/api/appdetails?appids=1245620",
  "json_path": "1245620.data.price_overview.final_formatted"
}`}
                    </code>
                  </pre>
                </div>
                <div className="card">
                  <h4 className="font-medium text-white mb-2">Account NFTs (FastNEAR)</h4>
                  <pre className="bg-dark-950 rounded p-3 text-xs overflow-x-auto">
                    <code className="text-green-400">
{`{
  "url": "https://api.fastnear.com/v1/account/root.near/nft",
  "json_path": "tokens"
}`}
                    </code>
                  </pre>
                </div>
                <div className="card">
                  <h4 className="font-medium text-white mb-2">Weather Data</h4>
                  <pre className="bg-dark-950 rounded p-3 text-xs overflow-x-auto">
                    <code className="text-green-400">
{`{
  "url": "https://api.open-meteo.com/v1/forecast?latitude=40.71&longitude=-74.00&current_weather=true",
  "json_path": "current_weather.temperature"
}`}
                    </code>
                  </pre>
                </div>
              </div>
            </section>

            {/* Code Examples */}
            <section id="code-examples" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Code Examples</h2>

              <h3 className="text-lg font-semibold text-white mb-4">Rust Integration</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto mb-6">
                <code className="text-blue-400">
{`use near_sdk::{ext_contract, AccountId, Gas, NearToken, Promise};

#[ext_contract(ext_oracle)]
pub trait Oracle {
    fn oracle_call(
        &mut self,
        receiver_id: AccountId,
        asset_ids: Option<Vec<String>>,
        msg: String,
        resource_limits: Option<serde_json::Value>,
    ) -> Promise;
}

impl Contract {
    pub fn get_prices_with_callback(&self) -> Promise {
        ext_oracle::ext("price-oracle.near".parse().unwrap())
            .with_attached_deposit(NearToken::from_millinear(20))
            .with_static_gas(Gas::from_tgas(150))
            .oracle_call(
                env::current_account_id(),
                Some(vec!["wrap.near".to_string()]),
                "swap".to_string(),
                None,
            )
    }
}`}
                </code>
              </pre>

              <h3 className="text-lg font-semibold text-white mb-4">JavaScript Integration</h3>
              <pre className="bg-dark-900 rounded-lg p-4 overflow-x-auto">
                <code className="text-blue-400">
{`import { connect, Contract } from 'near-api-js';

const oracle = new Contract(account, 'price-oracle.near', {
  viewMethods: ['get_price_data'],
  changeMethods: ['request_price_data', 'oracle_call'],
});

// View cached prices (free)
const cached = await oracle.get_price_data({
  asset_ids: ['wrap.near', 'aurora'],
});

// Convert price
const price = cached.prices[0].price;
const usd = Number(price.multiplier) / Math.pow(10, price.decimals);
console.log(\`NEAR = $\${usd}\`);`}
                </code>
              </pre>
            </section>

            {/* Deposits */}
            <section id="deposits" className="mb-16">
              <h2 className="text-2xl font-bold text-white mb-6">Deposit Requirements</h2>

              <div className="overflow-x-auto">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-dark-700">
                      <th className="py-3 px-4 text-dark-300">Method</th>
                      <th className="py-3 px-4 text-dark-300">Fresh Cache</th>
                      <th className="py-3 px-4 text-dark-300">Stale (OutLayer)</th>
                      <th className="py-3 px-4 text-dark-300">Subsidized</th>
                    </tr>
                  </thead>
                  <tbody className="text-dark-400">
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code>get_price_data</code></td>
                      <td className="py-3 px-4 text-green-400">Free</td>
                      <td className="py-3 px-4">N/A</td>
                      <td className="py-3 px-4">N/A</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code>request_price_data</code></td>
                      <td className="py-3 px-4 text-green-400">Free</td>
                      <td className="py-3 px-4 text-yellow-400">0.01+ NEAR</td>
                      <td className="py-3 px-4 text-green-400">Free</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code>oracle_call</code></td>
                      <td className="py-3 px-4 text-dark-500">1 yoctoNEAR</td>
                      <td className="py-3 px-4 text-yellow-400">0.01+ NEAR</td>
                      <td className="py-3 px-4 text-green-400">Free</td>
                    </tr>
                    <tr className="border-b border-dark-800">
                      <td className="py-3 px-4"><code>request_custom_data</code></td>
                      <td className="py-3 px-4">N/A</td>
                      <td className="py-3 px-4 text-yellow-400">0.01+ NEAR</td>
                      <td className="py-3 px-4 text-green-400">Free</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <div className="mt-6 card border-green-500/30">
                <h4 className="font-medium text-white mb-2">Subsidized Mode</h4>
                <p className="text-dark-400 text-sm">
                  When contract has {'>'}20 NEAR and subsidy is enabled, all OutLayer calls are free.
                  Check with <code>can_subsidize_outlayer_calls()</code>.
                </p>
              </div>
            </section>

            {/* Try Playground */}
            <section className="mb-16">
              <div className="card bg-gradient-to-r from-primary/20 to-secondary/20 border-primary/30">
                <h2 className="text-2xl font-bold text-white mb-4">Try It Out</h2>
                <p className="text-dark-300 mb-6">
                  Use the interactive playground to test oracle methods without writing code.
                </p>
                <Link href="/playground" className="btn btn-primary">
                  Open Playground
                </Link>
              </div>
            </section>
          </main>
        </div>
      </div>
    </div>
  );
}
