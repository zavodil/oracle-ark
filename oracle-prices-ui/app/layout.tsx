import './globals.css';
import type { Metadata } from 'next';
import { WalletProvider } from '@/contexts/WalletContext';
import Header from '@/components/Header';

export const metadata: Metadata = {
  title: 'TEE-Secured Price Oracle',
  description: 'On-Demand Oracle with Sustainable Economics - Based on OutLayer',
  icons: {
    icon: [
      { url: '/favicon.ico' },
      { url: '/favicon-16x16.png', sizes: '16x16', type: 'image/png' },
      { url: '/favicon-32x32.png', sizes: '32x32', type: 'image/png' },
    ],
    apple: '/apple-touch-icon.png',
  },
  manifest: '/site.webmanifest',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className="min-h-screen bg-dark-950">
        <WalletProvider>
          <Header />
          <main className="pt-16">
            {children}
          </main>
        </WalletProvider>
      </body>
    </html>
  );
}
