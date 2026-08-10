'use client';

import Link from 'next/link';
import Image from 'next/image';
import dynamic from 'next/dynamic';

// Dynamic imports for canvas components (client-side only)
const OracleFlowDiagram = dynamic(() => import('@/components/OracleFlowDiagram'), { ssr: false });
const InternalArchDiagram = dynamic(() => import('@/components/InternalArchDiagram'), { ssr: false });

const features = [
  {
    title: 'On-Demand Pricing',
    description: 'Request prices when you need them. Your contract calls oracle_call() and receives fresh data via callback.',
    icon: '⚡',
  },
  {
    title: 'Zero Trust',
    description: 'All price fetching and aggregation happens exclusively inside Intel TDX enclave. No external operator ever touches raw data.',
    icon: '🔒',
  },
  {
    title: 'Yield-Resume Pattern',
    description: 'Uses NEAR yield/resume for async TEE calls. Your contract pauses, TEE fetches prices, contract resumes with data.',
    icon: '🔄',
  },
  {
    title: 'Custom Data Sources',
    description: 'Not just prices — use custom_call() to fetch any JSON from any API directly into your contract.',
    icon: '🌐',
  },
];

const securityPoints = [
  {
    title: 'Intel TDX Enclave',
    description: 'WASI binary runs inside Intel Trust Domain Extensions (TDX) providing hardware-level isolation.',
  },
  {
    title: 'TEE Attestation',
    description: 'Workers verify via DCAP attestation. 5-measurement whitelist (MRTD + RTMR0-3) ensures only approved binaries can run.',
  },
  {
    title: 'Cryptographic Proof',
    description: 'Access keys are bound to TEE instances. If enclave is compromised, keys are lost.',
  },
  {
    title: 'Verifiable Execution',
    description: 'All price fetching happens in auditable, deterministic WASI code. Open source on GitHub.',
  },
];

const tokens = [
  { name: 'NEAR', sources: 8 },
  { name: 'ETH', sources: 8 },
  { name: 'BTC', sources: 8 },
  { name: 'SOL', sources: 8 },
  { name: 'USDT', sources: 2 },
  { name: 'USDC', sources: 5 },
  { name: 'DAI', sources: 6 },
  { name: 'AURORA', sources: 6 },
];

export default function HomePage() {
  return (
    <div className="min-h-screen">
      {/* Hero Section */}
      <section className="relative py-20 px-4 overflow-hidden">
        <div className="absolute inset-0 bg-gradient-to-b from-primary/10 to-transparent"></div>
        <div className="max-w-6xl mx-auto relative">
          <div className="text-center">
            <div className="flex items-center justify-center gap-4 mb-6">
              <Image
                src="/logo.png"
                alt="Price Oracle"
                width={56}
                height={56}
                className="w-12 h-12 md:w-14 md:h-14"
              />
              <h1 className="text-4xl md:text-6xl font-bold text-white">
                TEE-Secured Price Oracle
              </h1>
            </div>
            <p className="text-xl md:text-2xl text-dark-300 mb-4">
              On-Demand Oracle with Sustainable Economics
            </p>
            <p className="text-dark-400 mb-8 max-w-2xl mx-auto">
              Based on <a href="https://outlayer.ai" className="text-primary hover:underline">OutLayer</a> —
              verifiable off-chain computation for NEAR Protocol
            </p>
            <div className="flex flex-wrap justify-center gap-4">
              <Link href="/playground" className="btn btn-primary text-lg px-8 py-3">
                Try Playground
              </Link>
              <Link href="/docs" className="btn btn-secondary text-lg px-8 py-3">
                Read Docs
              </Link>
              <a
                href="https://github.com/out-layer/oracle-example"
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-secondary text-lg px-8 py-3"
              >
                GitHub
              </a>
            </div>
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section className="py-16 px-4 bg-dark-900/50">
        <div className="max-w-6xl mx-auto">
          <h2 className="text-3xl font-bold text-white text-center mb-12">
            How It Works
          </h2>
          <div className="grid md:grid-cols-2 lg:grid-cols-4 gap-6">
            {features.map((feature, index) => (
              <div key={index} className="card hover:border-primary/50 transition-colors">
                <div className="text-4xl mb-4">{feature.icon}</div>
                <h3 className="text-lg font-semibold text-white mb-2">
                  {feature.title}
                </h3>
                <p className="text-dark-400 text-sm">
                  {feature.description}
                </p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Direct OutLayer Integration */}
      <section className="py-16 px-4">
        <div className="max-w-5xl mx-auto">
          <h2 className="text-3xl font-bold text-white text-center mb-4">
            Direct OutLayer Integration
          </h2>
          <p className="text-dark-400 text-center mb-8 max-w-2xl mx-auto">
            Your contract calls OutLayer directly — no intermediary contracts needed
          </p>
          <div className="card bg-dark-900 p-6">
            <OracleFlowDiagram />
          </div>
          <div className="mt-6 grid md:grid-cols-4 gap-4 text-center">
            <div className="card py-4">
              <div className="text-primary text-lg font-semibold mb-1">No middleman</div>
              <div className="text-dark-400 text-xs">Direct contract-to-TEE</div>
            </div>
            <div className="card py-4">
              <div className="text-primary text-lg font-semibold mb-1">Lower gas</div>
              <div className="text-dark-400 text-xs">One less cross-call</div>
            </div>
            <div className="card py-4">
              <div className="text-primary text-lg font-semibold mb-1">Any data</div>
              <div className="text-dark-400 text-xs">Custom WASI workers</div>
            </div>
            <div className="card py-4">
              <div className="text-primary text-lg font-semibold mb-1">Full control</div>
              <div className="text-dark-400 text-xs">You own the flow</div>
            </div>
          </div>
        </div>
      </section>

      {/* Internal Architecture */}
      <section className="py-16 px-4 bg-dark-900/50">
        <div className="max-w-4xl mx-auto">
          <h2 className="text-3xl font-bold text-white text-center mb-4">
            Internal Architecture
          </h2>
          <p className="text-dark-400 text-center mb-8 max-w-2xl mx-auto">
            How the scheduler keeps prices warm in TEE (mainnet only)
          </p>
          <div className="card bg-dark-900 p-6">
            <InternalArchDiagram />
          </div>
        </div>
      </section>

      {/* Security */}
      <section className="py-16 px-4">
        <div className="max-w-6xl mx-auto">
          <h2 className="text-3xl font-bold text-white text-center mb-4">
            Why It&apos;s Secure
          </h2>
          <p className="text-dark-400 text-center mb-12 max-w-2xl mx-auto">
            Trust comes from cryptographic proof, not from counting independent operators.
          </p>
          <div className="grid md:grid-cols-2 gap-6">
            {securityPoints.map((point, index) => (
              <div key={index} className="card">
                <div className="flex items-start space-x-4">
                  <div className="w-8 h-8 rounded-full bg-green-500/20 flex items-center justify-center flex-shrink-0">
                    <svg className="w-4 h-4 text-green-400" fill="currentColor" viewBox="0 0 20 20">
                      <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                    </svg>
                  </div>
                  <div>
                    <h3 className="text-lg font-semibold text-white mb-1">
                      {point.title}
                    </h3>
                    <p className="text-dark-400 text-sm">
                      {point.description}
                    </p>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Supported Tokens */}
      <section className="py-16 px-4 bg-dark-900/50">
        <div className="max-w-6xl mx-auto">
          <h2 className="text-3xl font-bold text-white text-center mb-4">
            Supported Tokens
          </h2>
          <p className="text-dark-400 text-center mb-12">
            13 tokens with multiple price sources for maximum reliability
          </p>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            {tokens.map((token, index) => (
              <div key={index} className="card text-center py-4">
                <div className="text-2xl font-bold text-white mb-1">
                  {token.name}
                </div>
                <div className="text-sm text-dark-400">
                  {token.sources} sources
                </div>
              </div>
            ))}
          </div>
          <p className="text-center mt-6">
            <Link href="/prices" className="text-primary hover:underline">
              View live prices →
            </Link>
          </p>
        </div>
      </section>

      {/* Quick Start */}
      <section className="py-16 px-4">
        <div className="max-w-4xl mx-auto">
          <h2 className="text-3xl font-bold text-white text-center mb-12">
            Quick Start
          </h2>
          <div className="space-y-6">
            {/* Recommended: oracle_call */}
            <div className="card border-green-500/50">
              <div className="flex items-center gap-2 mb-4">
                <span className="px-2 py-1 text-xs bg-green-500/20 text-green-400 rounded">Recommended</span>
                <h3 className="text-lg font-semibold text-white">
                  Get Prices with Callback
                </h3>
              </div>
              <p className="text-dark-400 text-sm mb-4">
                Use <code className="text-primary">oracle_call</code> to request prices. Your contract will receive them via <code className="text-primary">oracle_on_call</code> callback.
              </p>
              <pre className="bg-dark-950 rounded-lg p-4 overflow-x-auto text-sm">
                <code className="text-green-400">
{`near call price-oracle.near oracle_call '{
  "receiver_id": "your-contract.near",
  "asset_ids": ["wrap.near", "eth.bridge.near"],
  "msg": ""
}' --accountId your.near --deposit 0.02 --gas 200000000000000`}
                </code>
              </pre>
            </div>

            {/* Alternative: request_price_data */}
            <div className="card">
              <h3 className="text-lg font-semibold text-white mb-4">
                Direct Price Request
              </h3>
              <p className="text-dark-400 text-sm mb-4">
                Get prices directly without callback. Useful for scripts and testing.
              </p>
              <pre className="bg-dark-950 rounded-lg p-4 overflow-x-auto text-sm">
                <code className="text-green-400">
{`near call price-oracle.near request_price_data '{
  "asset_ids": ["wrap.near"]
}' --accountId your.near --deposit 0.02 --gas 200000000000000`}
                </code>
              </pre>
            </div>

            {/* Direct OutLayer Integration */}
            <div className="card border-purple-500/30">
              <div className="flex items-center gap-2 mb-4">
                <span className="px-2 py-1 text-xs bg-purple-500/20 text-purple-400 rounded">Advanced</span>
                <h3 className="text-lg font-semibold text-white">
                  Direct OutLayer Integration
                </h3>
              </div>
              <p className="text-dark-400 text-sm mb-4">
                Skip the oracle contract — call OutLayer directly from your contract using the price oracle project.
              </p>
              <pre className="bg-dark-950 rounded-lg p-4 overflow-x-auto text-sm">
                <code className="text-purple-400">
{`// Rust: Call OutLayer directly with the price oracle project
let execution_source = serde_json::json!({
    "Project": { "project_id": "price-oracle.near/price-oracle" }
});
let resource_limits = serde_json::json!({
    "max_instructions": 10000000000_u64,
    "max_memory_mb": 128,
    "max_execution_seconds": 60
});
let input_data = serde_json::json!({
    "command": "get_prices",  // OracleCommand enum
    "tokens": ["wrap.near"]   // field name is "tokens"
}).to_string();

ext_outlayer::ext("outlayer.near".parse().unwrap())
    .with_attached_deposit(NearToken::from_millinear(10))
    .request_execution(
        execution_source, Some(resource_limits), Some(input_data),
        None, Some("json".to_string()), None, None
    )`}
                </code>
              </pre>
              <p className="text-dark-500 text-xs mt-3">
                See{' '}
                <Link href="/docs#direct-outlayer" className="text-primary hover:underline">
                  full documentation
                </Link>
                {' '}for callback handling and complete examples.
              </p>
            </div>

            {/* Warning about view methods */}
            <div className="card border-yellow-500/30 bg-yellow-500/5">
              <div className="flex items-start gap-3">
                <div className="text-yellow-400 text-xl">⚠️</div>
                <div>
                  <h3 className="text-lg font-semibold text-white mb-2">
                    About Cached Prices (view methods)
                  </h3>
                  <p className="text-dark-400 text-sm mb-3">
                    <code className="text-yellow-400">get_price_data</code> is a free view method, but it only returns data if someone recently paid for an update.
                    Due to the <strong className="text-white">on-demand nature</strong> of this oracle, the cache is usually empty or stale — prices are fetched when needed for specific operations (liquidations, borrowing, swaps), not stored permanently.
                  </p>
                  <p className="text-dark-400 text-sm">
                    <strong className="text-white">This is by design:</strong> Unlike traditional oracles with a central price feed contract, this oracle delivers prices directly to your contract via callback. Any contract can integrate without intermediaries.
                  </p>
                </div>
              </div>
            </div>
          </div>
          <div className="text-center mt-8">
            <Link href="/docs" className="text-primary hover:underline">
              See full documentation →
            </Link>
          </div>
        </div>
      </section>

      {/* Contracts */}
      <section className="py-16 px-4 bg-dark-900/50">
        <div className="max-w-4xl mx-auto">
          <h2 className="text-3xl font-bold text-white text-center mb-12">
            Deployed Contracts
          </h2>
          <div className="overflow-x-auto">
            <table className="w-full text-left">
              <thead>
                <tr className="border-b border-dark-700">
                  <th className="py-3 px-4 text-dark-300 font-medium">Contract</th>
                  <th className="py-3 px-4 text-dark-300 font-medium">Mainnet</th>
                </tr>
              </thead>
              <tbody>
                <tr className="border-b border-dark-800">
                  <td className="py-3 px-4 text-white">Price Oracle (ex NEAR Native Oracle)</td>
                  <td className="py-3 px-4">
                    <code className="text-primary">price-oracle.near</code>
                  </td>
                </tr>
                <tr className="border-b border-dark-800">
                  <td className="py-3 px-4 text-white">Simple Oracle Wrapper</td>
                  <td className="py-3 px-4">
                    <code className="text-primary">price-oracle-wrapper.near</code>
                  </td>
                </tr>
                <tr className="border-b border-dark-800">
                  <td className="py-3 px-4 text-white">Pyth-Compatible oracle</td>
                  <td className="py-3 px-4">
                    <code className="text-primary">price-oracle-pyth.near</code>
                  </td>
                </tr>
                <tr className="border-b border-dark-800">
                  <td className="py-3 px-4 text-white">OutLayer Project ID</td>
                  <td className="py-3 px-4">
                    <code className="text-purple-400">price-oracle.near/price-oracle</code>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="py-12 px-4 border-t border-dark-800">
        <div className="max-w-6xl mx-auto">
          <div className="flex flex-wrap justify-between items-center gap-4">
            <div className="text-dark-400 text-sm">
              TEE-Secured Price Oracle — Based on{' '}
              <a href="https://outlayer.ai" className="text-primary hover:underline">
                OutLayer
              </a>
            </div>
            <div className="flex space-x-6 text-sm">
              <a
                href="https://github.com/out-layer/oracle-example"
                target="_blank"
                rel="noopener noreferrer"
                className="text-dark-400 hover:text-white"
              >
                GitHub
              </a>
              <Link href="/docs" className="text-dark-400 hover:text-white">
                Documentation
              </Link>
              <Link href="/playground" className="text-dark-400 hover:text-white">
                Playground
              </Link>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
