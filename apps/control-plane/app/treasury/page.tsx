export default function TreasuryPage() {
  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">Treasury Monitor</h2>
        <p className="text-slate-400">Institutional asset tracking and Sovereign Yield Index (SYI).</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Total sBTC Liquidity</div>
          <div className="text-2xl font-bold text-orange-400">42.5 sBTC</div>
        </div>
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Sovereign Yield Index</div>
          <div className="text-2xl font-bold text-emerald-400">4.82%</div>
        </div>
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">TAM Capture Rate</div>
          <div className="text-2xl font-bold text-indigo-400">0.02%</div>
        </div>
      </div>

      <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
        <h3 className="text-lg font-bold mb-4 text-slate-300">Financial Modeling (v1.9.2)</h3>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
          <div className="flex justify-between p-2 border-b border-slate-800">
            <span className="text-slate-500">Structured Tranches</span>
            <span className="text-indigo-400 font-mono">SENIOR / JUNIOR</span>
          </div>
          <div className="flex justify-between p-2 border-b border-slate-800">
            <span className="text-slate-500">Settlement Finality</span>
            <span className="text-emerald-400 font-mono">BITVM2 PROVED</span>
          </div>
        </div>
      </div>
    </div>
  );
}
