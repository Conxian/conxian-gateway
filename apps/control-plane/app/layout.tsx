import type { Metadata } from "next";
import Link from "next/link";
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
      <body className="bg-slate-900 text-slate-100 font-sans min-h-screen flex flex-col">
        <header className="border-b border-slate-800 p-4 bg-slate-900/80 backdrop-blur sticky top-0 z-50 flex justify-between items-center">
          <Link href="/" className="text-xl font-bold hover:text-indigo-400 transition-colors">
            Conxian BOS Control-Plane
          </Link>
          <div className="flex gap-4 text-sm text-slate-400">
            <span>v0.1.3</span>
            <span className="text-green-500">● Live</span>
          </div>
        </header>
        <div className="flex flex-1">
          <aside className="w-64 border-r border-slate-800 p-6 hidden md:block overflow-y-auto max-h-[calc(100vh-64px)]">
            <nav className="space-y-1">
              <Link href="/" className="flex items-center gap-3 p-2 hover:bg-slate-800 rounded transition-colors text-sm">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg>
                Dashboard
              </Link>
              <Link href="/releases" className="flex items-center gap-3 p-2 hover:bg-slate-800 rounded transition-colors text-sm">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg>
                Release Governance
              </Link>
              <Link href="/audit" className="flex items-center gap-3 p-2 hover:bg-slate-800 rounded transition-colors text-sm">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg>
                Audit Log
              </Link>
              <Link href="/governance" className="flex items-center gap-3 p-2 hover:bg-slate-800 rounded transition-colors text-sm">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg>
                Policy Approvals
              </Link>
              <div className="pt-4 border-t border-slate-800 mt-4 space-y-1">
                <Link href="/metrics" className="flex items-center gap-3 p-2 hover:bg-slate-800 rounded transition-colors text-sm text-slate-400">
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg>
                  System Metrics
                </Link>
                <Link href="/settings" className="flex items-center gap-3 p-2 hover:bg-slate-800 rounded transition-colors text-sm text-slate-400">
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/><path d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg>
                  Settings
                </Link>
              </div>
            </nav>
          </aside>
          <main className="flex-1 p-8 overflow-y-auto max-h-[calc(100vh-64px)]">
            {children}
          </main>
        </div>
        <footer className="border-t border-slate-800 p-4 text-center text-xs text-slate-500 bg-slate-900">
          © 2026 Conxian Labs. Institutional Readiness Certified.
        </footer>
      </body>
    </html>
  );
}
