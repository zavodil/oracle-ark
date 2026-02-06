'use client';

import React, { createContext, useContext, useState, useEffect, useCallback } from 'react';
import { setupWalletSelector } from '@near-wallet-selector/core';
import type { WalletSelector, AccountState, Wallet } from '@near-wallet-selector/core';
import { setupModal } from '@near-wallet-selector/modal-ui';
import type { WalletSelectorModal } from '@near-wallet-selector/modal-ui';
import { setupMyNearWallet } from '@near-wallet-selector/my-near-wallet';
import { setupMeteorWallet } from '@near-wallet-selector/meteor-wallet';
import { setupHereWallet } from '@near-wallet-selector/here-wallet';
import { setupIntearWallet } from '@near-wallet-selector/intear-wallet';
import '@near-wallet-selector/modal-ui/styles.css';

// Mainnet only config
const NETWORK_CONFIG = {
  networkId: 'mainnet',
  contractId: 'price-oracle.near',
  outlayerContractId: 'outlayer.near',
  rpcUrl: 'https://rpc.mainnet.near.org',
  explorerUrl: 'https://nearblocks.io',
  coordinatorUrl: 'https://api.outlayer.fastnear.com',
};

interface WalletContextType {
  selector: WalletSelector | null;
  modal: WalletSelectorModal | null;
  accountId: string | null;
  isConnected: boolean;
  isLoading: boolean;
  connect: () => void;
  disconnect: () => Promise<void>;
  signAndSendTransaction: (params: {
    receiverId: string;
    actions: any[];
  }) => Promise<any>;
  viewMethod: (params: {
    contractId: string;
    method: string;
    args?: Record<string, unknown>;
  }) => Promise<any>;
  config: typeof NETWORK_CONFIG;
}

const WalletContext = createContext<WalletContextType | null>(null);

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const [selector, setSelector] = useState<WalletSelector | null>(null);
  const [modal, setModal] = useState<WalletSelectorModal | null>(null);
  const [accountId, setAccountId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const init = async () => {
      try {
        const walletSelector = await setupWalletSelector({
          network: 'mainnet',
          modules: [
            setupMyNearWallet(),
            setupMeteorWallet(),
            setupHereWallet(),
            setupIntearWallet(),
          ],
        });

        const walletModal = setupModal(walletSelector, {
          contractId: '',
        });

        const state = walletSelector.store.getState();
        const accounts = state.accounts;

        setSelector(walletSelector);
        setModal(walletModal);
        setAccountId(accounts.length > 0 ? accounts[0].accountId : null);

        // Subscribe to account changes
        walletSelector.store.observable.subscribe((state: { accounts: AccountState[] }) => {
          const accounts = state.accounts;
          setAccountId(accounts.length > 0 ? accounts[0].accountId : null);
        });
      } catch (error) {
        console.error('Failed to initialize wallet selector:', error);
      } finally {
        setIsLoading(false);
      }
    };

    init();
  }, []);

  const connect = useCallback(() => {
    if (modal) {
      modal.show();
    }
  }, [modal]);

  const disconnect = useCallback(async () => {
    if (selector && accountId) {
      const wallet = await selector.wallet();
      await wallet.signOut();
      setAccountId(null);
    }
  }, [selector, accountId]);

  const signAndSendTransaction = useCallback(async (params: {
    receiverId: string;
    actions: any[];
  }) => {
    if (!selector || !accountId) {
      throw new Error('Wallet not connected');
    }

    const wallet = await selector.wallet();
    const result = await wallet.signAndSendTransaction({
      receiverId: params.receiverId,
      actions: params.actions,
    });

    return result;
  }, [selector, accountId]);

  const viewMethod = useCallback(async (params: {
    contractId: string;
    method: string;
    args?: Record<string, unknown>;
  }) => {
    const response = await fetch(NETWORK_CONFIG.rpcUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 'dontcare',
        method: 'query',
        params: {
          request_type: 'call_function',
          finality: 'final',
          account_id: params.contractId,
          method_name: params.method,
          args_base64: btoa(JSON.stringify(params.args || {})),
        },
      }),
    });

    const data = await response.json();

    if (data.error) {
      throw new Error(data.error.message || 'RPC error');
    }

    const resultBytes = data.result?.result;
    if (!resultBytes) {
      return null;
    }

    const resultStr = new TextDecoder().decode(new Uint8Array(resultBytes));
    return JSON.parse(resultStr);
  }, []);

  return (
    <WalletContext.Provider
      value={{
        selector,
        modal,
        accountId,
        isConnected: !!accountId,
        isLoading,
        connect,
        disconnect,
        signAndSendTransaction,
        viewMethod,
        config: NETWORK_CONFIG,
      }}
    >
      {children}
    </WalletContext.Provider>
  );
}

export function useWallet() {
  const context = useContext(WalletContext);
  if (!context) {
    throw new Error('useWallet must be used within a WalletProvider');
  }
  return context;
}
