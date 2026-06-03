import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Conxian BOS Control-Plane",
  description: "Internal governance and orchestration for Conxian Gateway",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className="bg-slate-900 text-slate-100 font-sans">
        <header className="border-b border-slate-800 p-4">
          <h1 className="text-xl font-bold">Conxian BOS Control-Plane</h1>
        </header>
        <main className="p-8">
          {children}
        </main>
      </body>
    </html>
  );
}
