export default function AuditPage() {
  const logs = [
    { id: 1, event: "REL_PROMOTION_REQUESTED", actor: "Botshelo Mokoka", timestamp: "2026-04-24 10:15:22", target: "v0.1.4-rc1", status: "SUCCESS" },
    { id: 2, event: "POLICY_ENACTED", actor: "Charlie", timestamp: "2026-04-24 09:30:45", target: "CON-POL-003", status: "SUCCESS" },
    { id: 3, event: "GATEWAY_SYNC_COMPLETE", actor: "SYSTEM", timestamp: "2026-04-24 08:00:10", target: "Block 840,000", status: "SUCCESS" },
    { id: 4, event: "CONFIG_UPDATE_FAILED", actor: "Operator-02", timestamp: "2026-04-23 23:45:12", target: "AuthStore", status: "FAILURE" },
  ];

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">Audit Log</h2>
        <p className="text-slate-400">High-integrity immutable record of all system interactions.</p>
      </div>

      <div className="border border-slate-800 rounded-xl overflow-hidden bg-slate-800/20">
        <div className="p-4 bg-slate-800/50 border-b border-slate-800 flex justify-between items-center">
          <div className="flex gap-4">
            <input type="text" placeholder="sentinel: Filter events..." className="bg-slate-900 border border-slate-700 rounded px-3 py-1 text-sm focus:outline-none focus:border-indigo-500" />
            <select className="bg-slate-900 border border-slate-700 rounded px-3 py-1 text-sm focus:outline-none focus:border-indigo-500">
              <option>All Statuses</option>
              <option>Success</option>
              <option>Failure</option>
            </select>
          </div>
          <button className="text-sm text-indigo-400 hover:text-indigo-300">Export CSV</button>
        </div>
        <table className="w-full text-left">
          <thead className="bg-slate-800/30 text-slate-400 text-xs uppercase tracking-wider">
            <tr>
              <th className="px-6 py-3 font-semibold">Event Type</th>
              <th className="px-6 py-3 font-semibold">Actor</th>
              <th className="px-6 py-3 font-semibold">Timestamp</th>
              <th className="px-6 py-3 font-semibold">Target Resource</th>
              <th className="px-6 py-3 font-semibold">Result</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800 text-sm">
            {logs.map((log) => (
              <tr key={log.id} className="hover:bg-slate-800/30 transition-colors">
                <td className="px-6 py-4 font-mono text-xs">{log.event}</td>
                <td className="px-6 py-4">{log.actor}</td>
                <td className="px-6 py-4 text-slate-400">{log.timestamp}</td>
                <td className="px-6 py-4 font-mono text-xs text-indigo-300">{log.target}</td>
                <td className="px-6 py-4">
                  <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${log.status === 'SUCCESS' ? 'bg-emerald-500/10 text-emerald-400' : 'bg-red-500/10 text-red-400'}`}>
                    {log.status}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
