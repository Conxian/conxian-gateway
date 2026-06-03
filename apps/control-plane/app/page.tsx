export default function Home() {
  return (
    <div className="space-y-6">
      <h2 className="text-3xl font-bold">Dashboard</h2>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div className="p-6 border border-slate-800 rounded-lg bg-slate-800/50">
          <h3 className="text-lg font-semibold mb-2">Release Governance</h3>
          <p className="text-slate-400">Manage release approvals and decision workflows.</p>
        </div>
        <div className="p-6 border border-slate-800 rounded-lg bg-slate-800/50">
          <h3 className="text-lg font-semibold mb-2">Audit Log</h3>
          <p className="text-slate-400">Monitor system events and actor interactions.</p>
        </div>
        <div className="p-6 border border-slate-800 rounded-lg bg-slate-800/50">
          <h3 className="text-lg font-semibold mb-2">Policy Approvals</h3>
          <p className="text-slate-400">Review and enact governance proposals.</p>
        </div>
      </div>
    </div>
  );
}
