import type { Metadata } from 'next';
import './globals.css';

export const metadata: Metadata = {
  title: 'Tatami',
  description: 'Client desktop de lecture de manga',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="fr">
      <body>
        <div className="app">{children}</div>
      </body>
    </html>
  );
}