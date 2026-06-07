import Link from "next/link";

export default function Home() {
  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">Dashboard</h2>
        <p className="text-slate-400">Sovereign Business Operations System (BOS) management interface.</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <Link href="/releases" className="group p-6 border border-slate-800 rounded-xl bg-slate-800/30 hover:bg-slate-800/50 hover:border-indigo-500/50 transition-all">
          <div className="w-12 h-12 bg-indigo-500/10 rounded-lg flex items-center justify-center mb-4 group-hover:bg-indigo-500/20">
            <svg className="w-6 h-6 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          </div>
          <h3 className="text-lg font-semibold mb-2 group-hover:text-indigo-400">Release Governance</h3>
          <p className="text-sm text-slate-400">Manage release approvals, promotion gates, and decision workflows.</p>
        </Link>

        <Link href="/audit" className="group p-6 border border-slate-800 rounded-xl bg-slate-800/30 hover:bg-slate-800/50 hover:border-emerald-500/50 transition-all">
          <div className="w-12 h-12 bg-emerald-500/10 rounded-lg flex items-center justify-center mb-4 group-hover:bg-emerald-500/20">
            <svg className="w-6 h-6 text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" />
            </svg>
          </div>
          <h3 className="text-lg font-semibold mb-2 group-hover:text-emerald-400">Audit Log</h3>
          <p className="text-sm text-slate-400">Monitor high-integrity system events and actor interactions.</p>
        </Link>

        <Link href="/governance" className="group p-6 border border-slate-800 rounded-xl bg-slate-800/30 hover:bg-slate-800/50 hover:border-amber-500/50 transition-all">
          <div className="w-12 h-12 bg-amber-500/10 rounded-lg flex items-center justify-center mb-4 group-hover:bg-amber-500/20">
            <svg className="w-6 h-6 text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
          </div>
          <h3 className="text-lg font-semibold mb-2 group-hover:text-amber-400">Policy Approvals</h3>
          <p className="text-sm text-slate-400">Review and enact institutional governance proposals and mandates.</p>
        </Link>
      </div>

      <div className="p-8 border border-slate-800 rounded-xl bg-slate-800/10">
        <h3 className="text-xl font-bold mb-4">System Status</h3>
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          <div className="p-4 bg-slate-800/40 rounded-lg">
            <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Gateway</div>
            <div className="text-lg font-mono text-green-400">CONNECTED</div>
          </div>
          <div className="p-4 bg-slate-800/40 rounded-lg">
            <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Persistence</div>
            <div className="text-lg font-mono text-green-400">ATOMIC</div>
          </div>
          <div className="p-4 bg-slate-800/40 rounded-lg">
            <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Sync Health</div>
            <div className="text-lg font-mono text-indigo-400">100%</div>
          </div>
          <div className="p-4 bg-slate-800/40 rounded-lg">
            <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Compliance</div>
            <div className="text-lg font-mono text-emerald-400">ZSE ACTIVE</div>
          </div>
        </div>
      </div>
    </div>
  );
}
