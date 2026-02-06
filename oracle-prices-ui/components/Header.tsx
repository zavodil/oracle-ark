'use client';

import Link from 'next/link';
import Image from 'next/image';
import { usePathname } from 'next/navigation';
import { useWallet } from '@/contexts/WalletContext';

const navigation = [
  { name: 'Home', href: '/' },
  { name: 'Playground', href: '/playground' },
  { name: 'Docs', href: '/docs' },
  { name: 'Prices', href: '/prices' },
];

export default function Header() {
  const pathname = usePathname();
  const { accountId, isConnected, isLoading, connect, disconnect } = useWallet();

  const formatAccountId = (id: string) => {
    if (id.length <= 20) return id;
    return `${id.slice(0, 10)}...${id.slice(-8)}`;
  };

  return (
    <header className="fixed top-0 left-0 right-0 z-50 bg-dark-900/80 backdrop-blur-lg border-b border-dark-800">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex items-center justify-between h-16">
          {/* Logo */}
          <Link href="/" className="flex items-center space-x-2">
            <Image
              src="/logo.png"
              alt="Price Oracle"
              width={28}
              height={28}
              className="w-7 h-7"
            />
            <span className="text-lg font-semibold text-white hidden sm:block">
              Price Oracle
            </span>
          </Link>

          {/* Navigation */}
          <nav className="hidden md:flex items-center space-x-1">
            {navigation.map((item) => {
              const isActive = pathname === item.href ||
                (item.href !== '/' && pathname.startsWith(item.href));
              return (
                <Link
                  key={item.name}
                  href={item.href}
                  className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                    isActive
                      ? 'bg-dark-800 text-white'
                      : 'text-dark-300 hover:text-white hover:bg-dark-800/50'
                  }`}
                >
                  {item.name}
                </Link>
              );
            })}
          </nav>

          {/* Wallet / Network */}
          <div className="flex items-center space-x-4">
            {/* Network Badge */}
            <span className="hidden sm:flex items-center space-x-2 px-3 py-1 bg-dark-800 rounded-full text-xs">
              <span className="w-2 h-2 bg-green-400 rounded-full"></span>
              <span className="text-dark-300">Mainnet</span>
            </span>

            {/* Wallet Button */}
            {isLoading ? (
              <div className="btn btn-secondary opacity-50">
                <span className="animate-pulse">Loading...</span>
              </div>
            ) : isConnected ? (
              <div className="flex items-center space-x-2">
                <span className="text-sm text-dark-300 hidden sm:block">
                  {formatAccountId(accountId!)}
                </span>
                <button
                  onClick={disconnect}
                  className="btn btn-secondary text-sm"
                >
                  Disconnect
                </button>
              </div>
            ) : (
              <button
                onClick={connect}
                className="btn btn-primary text-sm"
              >
                Connect Wallet
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Mobile Navigation */}
      <div className="md:hidden border-t border-dark-800">
        <div className="flex justify-around py-2">
          {navigation.map((item) => {
            const isActive = pathname === item.href ||
              (item.href !== '/' && pathname.startsWith(item.href));
            return (
              <Link
                key={item.name}
                href={item.href}
                className={`px-3 py-1.5 rounded text-xs font-medium ${
                  isActive
                    ? 'bg-dark-800 text-white'
                    : 'text-dark-400 hover:text-white'
                }`}
              >
                {item.name}
              </Link>
            );
          })}
        </div>
      </div>
    </header>
  );
}
