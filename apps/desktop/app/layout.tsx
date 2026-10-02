import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Tatami',
  description: 'Client desktop de lecture de manga',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="fr">
      <body>{children}</body>
    </html>
  );
}
