export default function MetricsPage() {
  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">System Metrics</h2>
        <p className="text-slate-400">Institutional telemetry and performance monitoring.</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Settlement Volume</div>
          <div className="text-2xl font-bold">1.24 BTC</div>
          <div className="text-xs text-green-500 mt-2">+12.5% from last cycle</div>
        </div>
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Active Jobs</div>
          <div className="text-2xl font-bold">156</div>
          <div className="text-xs text-slate-500 mt-2">All nodes healthy</div>
        </div>
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">Avg Latency</div>
          <div className="text-2xl font-bold">45ms</div>
          <div className="text-xs text-emerald-500 mt-2">Within SLA</div>
        </div>
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">TAM Capture</div>
          <div className="text-2xl font-bold">0.02%</div>
          <div className="text-xs text-indigo-500 mt-2">Target: 0.1%</div>
        </div>
      </div>

      <div className="p-8 border border-slate-800 rounded-xl bg-slate-800/10 h-64 flex flex-col items-center justify-center">
        <div className="w-full flex items-end gap-2 h-32 mb-4">
            {[40, 60, 45, 80, 55, 90, 70, 85, 65, 95, 75, 100].map((h, i) => (
                <div key={i} className="flex-1 bg-indigo-500/30 rounded-t hover:bg-indigo-500/60 transition-all" style={{height: `${h}%`}}></div>
            ))}
        </div>
        <p className="text-sm text-slate-500">Transaction Throughput (Last 24 Hours)</p>
      </div>
    </div>
  );
}
