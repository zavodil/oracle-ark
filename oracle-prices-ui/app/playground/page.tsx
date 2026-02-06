'use client';

import { useState, useEffect } from 'react';
import { useWallet } from '@/contexts/WalletContext';
import { ALL_PRESETS, PRESET_CATEGORIES, getPresetById, getPresetWarning, type Preset } from '@/lib/presets';
import { getTransactionUrl } from '@/lib/api';
import { actionCreators } from '@near-js/transactions';

interface ExecutionResult {
  success: boolean;
  data?: unknown;
  error?: string;
  transactionHash?: string;
}

export default function PlaygroundPage() {
  const { isConnected, accountId, connect, viewMethod, signAndSendTransaction, config } = useWallet();
  const [selectedPreset, setSelectedPreset] = useState<Preset | null>(null);
  const [args, setArgs] = useState('');
  const [isExecuting, setIsExecuting] = useState(false);
  const [result, setResult] = useState<ExecutionResult | null>(null);

  // Select first preset by default
  useEffect(() => {
    if (!selectedPreset && ALL_PRESETS.length > 0) {
      const preset = ALL_PRESETS[0];
      setSelectedPreset(preset);
      setArgs(JSON.stringify(preset.args, null, 2));
    }
  }, [selectedPreset]);

  // Update args when preset changes
  const handlePresetChange = (presetId: string) => {
    const preset = getPresetById(presetId);
    if (preset) {
      setSelectedPreset(preset);
      setArgs(JSON.stringify(preset.args, null, 2));
      setResult(null);
    }
  };

  // Execute the selected preset
  const handleExecute = async () => {
    if (!selectedPreset) return;

    setIsExecuting(true);
    setResult(null);

    try {
      let parsedArgs: Record<string, unknown>;
      try {
        parsedArgs = JSON.parse(args);
      } catch {
        throw new Error('Invalid JSON in arguments');
      }

      if (selectedPreset.type === 'view') {
        // View call - doesn't require wallet
        const data = await viewMethod({
          contractId: selectedPreset.contract!,
          method: selectedPreset.method!,
          args: parsedArgs,
        });
        setResult({ success: true, data });
      } else {
        // Call method - requires wallet
        if (!isConnected) {
          throw new Error('Please connect your wallet first');
        }

        const deposit = selectedPreset.deposit
          ? BigInt(Math.floor(parseFloat(selectedPreset.deposit) * 1e24))
          : BigInt(1);

        const gas = selectedPreset.gas
          ? BigInt(selectedPreset.gas)
          : BigInt('100000000000000');

        const action = actionCreators.functionCall(
          selectedPreset.method!,
          parsedArgs,
          gas,
          deposit
        );

        const txResult = await signAndSendTransaction({
          receiverId: selectedPreset.contract!,
          actions: [action],
        });

        // Parse transaction result
        let data: unknown = null;
        if (txResult) {
          // Try to extract execution result
          const outcome = txResult.transaction_outcome || txResult;
          const hash = outcome?.id || txResult?.transaction?.hash;

          // Check for function call result in receipts
          if (txResult.receipts_outcome) {
            for (const receipt of txResult.receipts_outcome) {
              if (receipt.outcome?.status?.SuccessValue) {
                try {
                  const decoded = atob(receipt.outcome.status.SuccessValue);
                  data = JSON.parse(decoded);
                } catch {
                  data = receipt.outcome.status.SuccessValue;
                }
                break;
              }
            }
          }

          setResult({
            success: true,
            data,
            transactionHash: hash,
          });
        }
      }
    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      setResult({ success: false, error: errorMessage });
    } finally {
      setIsExecuting(false);
    }
  };

  return (
    <div className="min-h-screen py-8 px-4">
      <div className="max-w-6xl mx-auto">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-white mb-2">Playground</h1>
          <p className="text-dark-400">
            Test oracle methods and custom data requests interactively
          </p>
        </div>

        <div className="grid lg:grid-cols-2 gap-6">
          {/* Left Column - Configuration */}
          <div className="space-y-6">
            {/* Preset Selector */}
            <div className="card">
              <h2 className="text-lg font-semibold text-white mb-4">Select Preset</h2>
              <div className="space-y-4">
                {PRESET_CATEGORIES.map((category) => (
                  <div key={category.id}>
                    <h3 className="text-sm font-medium text-dark-400 mb-2">
                      {category.name}
                    </h3>
                    <div className="grid grid-cols-1 gap-2">
                      {category.presets.map((preset) => (
                        <button
                          key={preset.id}
                          onClick={() => handlePresetChange(preset.id)}
                          className={`text-left px-4 py-3 rounded-lg border transition-colors ${
                            selectedPreset?.id === preset.id
                              ? 'bg-primary/20 border-primary text-white'
                              : 'bg-dark-900 border-dark-700 text-dark-300 hover:border-dark-600'
                          }`}
                        >
                          <div className="flex items-center justify-between">
                            <span className="font-medium">{preset.name}</span>
                            <span className={`badge ${
                              preset.type === 'view' ? 'badge-success' :
                              preset.type === 'call' ? 'badge-warning' : 'badge-info'
                            }`}>
                              {preset.type}
                            </span>
                          </div>
                          <p className="text-xs text-dark-500 mt-1">
                            {preset.description}
                          </p>
                        </button>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Connection Status */}
            {selectedPreset?.type !== 'view' && (
              <div className="card">
                <h2 className="text-lg font-semibold text-white mb-4">Wallet</h2>
                {isConnected ? (
                  <div className="flex items-center space-x-3">
                    <div className="w-3 h-3 bg-green-400 rounded-full"></div>
                    <span className="text-dark-300">{accountId}</span>
                  </div>
                ) : (
                  <div>
                    <p className="text-dark-400 text-sm mb-3">
                      Connect wallet to execute transactions
                    </p>
                    <button onClick={connect} className="btn btn-primary">
                      Connect Wallet
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Right Column - Arguments & Execution */}
          <div className="space-y-6">
            {/* Warning for view methods */}
            {selectedPreset && getPresetWarning(selectedPreset.id) && (
              <div className="card border-yellow-500/30 bg-yellow-500/5">
                <div className="flex items-start gap-3">
                  <div className="text-yellow-400 text-xl">⚠️</div>
                  <div>
                    <p className="text-dark-300 text-sm">
                      {getPresetWarning(selectedPreset.id)!.message}{' '}
                      <button
                        onClick={() => handlePresetChange(getPresetWarning(selectedPreset.id)!.linkPresetId)}
                        className="text-primary hover:underline font-medium"
                      >
                        {getPresetWarning(selectedPreset.id)!.linkText}
                      </button>
                      {' '}to populate the cache.
                    </p>
                  </div>
                </div>
              </div>
            )}

            {/* Arguments Editor */}
            <div className="card">
              <div className="flex items-center justify-between mb-4">
                <h2 className="text-lg font-semibold text-white">Arguments</h2>
                {selectedPreset && (
                  <div className="text-sm text-dark-400">
                    <code>{selectedPreset.contract}</code>
                    <span className="mx-2">→</span>
                    <code className="text-primary">{selectedPreset.method}</code>
                  </div>
                )}
              </div>
              <textarea
                value={args}
                onChange={(e) => setArgs(e.target.value)}
                className="input font-mono text-sm h-64 resize-none"
                placeholder="Enter JSON arguments..."
              />
              {selectedPreset?.type === 'call' && selectedPreset.deposit && (
                <div className="mt-3 flex items-center justify-between text-sm">
                  <span className="text-dark-400">Deposit:</span>
                  <span className="text-yellow-400">{selectedPreset.deposit} NEAR</span>
                </div>
              )}
            </div>

            {/* Execute Button */}
            <button
              onClick={handleExecute}
              disabled={isExecuting || (!isConnected && selectedPreset?.type !== 'view')}
              className={`w-full btn text-lg py-4 ${
                isExecuting
                  ? 'bg-dark-700 text-dark-400 cursor-not-allowed'
                  : 'btn-primary'
              }`}
            >
              {isExecuting ? (
                <span className="flex items-center justify-center">
                  <svg className="animate-spin -ml-1 mr-3 h-5 w-5" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                  </svg>
                  Executing...
                </span>
              ) : selectedPreset?.type === 'view' ? (
                'Query (Free)'
              ) : (
                `Execute (${selectedPreset?.deposit || '0'} NEAR)`
              )}
            </button>

            {/* Result Display */}
            {result && (
              <div className={`card ${result.success ? 'border-green-500/50' : 'border-red-500/50'}`}>
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-lg font-semibold text-white">Result</h2>
                  <span className={`badge ${result.success ? 'badge-success' : 'badge-error'}`}>
                    {result.success ? 'Success' : 'Error'}
                  </span>
                </div>

                {result.error && (
                  <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-4 mb-4">
                    <p className="text-red-400 text-sm">{result.error}</p>
                  </div>
                )}

                {result.data !== undefined && (
                  <pre className="bg-dark-950 rounded-lg p-4 overflow-x-auto text-sm max-h-96">
                    <code className="text-green-400">
                      {JSON.stringify(result.data, null, 2)}
                    </code>
                  </pre>
                )}

                {result.transactionHash && (
                  <div className="mt-4 pt-4 border-t border-dark-700">
                    <a
                      href={getTransactionUrl(result.transactionHash)}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-primary hover:underline text-sm flex items-center"
                    >
                      View on Explorer
                      <svg className="w-4 h-4 ml-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                      </svg>
                    </a>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Help Section */}
        <div className="mt-12">
          <h2 className="text-2xl font-bold text-white mb-6">About Presets</h2>
          <div className="grid md:grid-cols-3 gap-6">
            <div className="card">
              <div className="flex items-center space-x-2 mb-3">
                <span className="badge badge-success">view</span>
                <span className="text-white font-medium">View Methods</span>
              </div>
              <p className="text-dark-400 text-sm">
                Free read-only queries. No wallet needed. Results returned instantly from the blockchain.
              </p>
            </div>
            <div className="card">
              <div className="flex items-center space-x-2 mb-3">
                <span className="badge badge-warning">call</span>
                <span className="text-white font-medium">Call Methods</span>
              </div>
              <p className="text-dark-400 text-sm">
                State-changing transactions. Requires connected wallet and deposit (usually 0.02 NEAR).
              </p>
            </div>
            <div className="card">
              <div className="flex items-center space-x-2 mb-3">
                <span className="badge badge-info">outlayer</span>
                <span className="text-white font-medium">Custom Data</span>
              </div>
              <p className="text-dark-400 text-sm">
                Fetch data from any HTTP API via TEE. Edit URLs and JSON paths to customize queries.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
