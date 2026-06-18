"use client";

import ClientButton from "../../components/ClientButton";

export default function ReleasesPage() {
  const handleAction = async () => {
    await new Promise(resolve => setTimeout(resolve, 1500));
    // Simulate API call
  };

  return (
    <div className="space-y-8">
      <div className="flex justify-between items-end">
        <div>
          <h2 className="text-3xl font-bold mb-2">Release Governance</h2>
          <p className="text-slate-400">Manage release approvals and promotion gates from dev to main.</p>
        </div>
        <ClientButton label="Request Approval" action={handleAction} />
      </div>

      <div className="border border-slate-800 rounded-xl overflow-hidden bg-slate-800/20">
        <table className="w-full text-left">
          <thead className="bg-slate-800/50 text-slate-300 text-xs uppercase tracking-wider">
            <tr>
              <th className="px-6 py-4 font-semibold">Release ID</th>
              <th className="px-6 py-4 font-semibold">Version</th>
              <th className="px-6 py-4 font-semibold">Target</th>
              <th className="px-6 py-4 font-semibold">Status</th>
              <th className="px-6 py-4 font-semibold">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800 text-sm">
            <tr>
              <td className="px-6 py-4 font-mono">REL-2026-004</td>
              <td className="px-6 py-4">v0.1.3</td>
              <td className="px-6 py-4">main</td>
              <td className="px-6 py-4">
                <span className="px-2 py-1 bg-green-500/10 text-green-400 rounded text-xs">RELEASED</span>
              </td>
              <td className="px-6 py-4 text-slate-500">None</td>
            </tr>
            <tr>
              <td className="px-6 py-4 font-mono">REL-2026-005</td>
              <td className="px-6 py-4">v0.1.4-rc1</td>
              <td className="px-6 py-4">staged</td>
              <td className="px-6 py-4">
                <span className="px-2 py-1 bg-amber-500/10 text-amber-400 rounded text-xs">PENDING APPROVAL</span>
              </td>
              <td className="px-6 py-4">
                <div className="flex gap-2">
                  <ClientButton label="Approve" action={handleAction} variant="success" />
                  <ClientButton label="Reject" action={handleAction} variant="danger" />
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/10">
          <h3 className="text-lg font-bold mb-4">Readiness Gates</h3>
          <ul className="space-y-3">
            <li className="flex items-center gap-3 text-sm">
              <span className="w-4 h-4 bg-green-500 rounded-full flex-shrink-0"></span>
              <span>Security Hardening Verified</span>
            </li>
            <li className="flex items-center gap-3 text-sm">
              <span className="w-4 h-4 bg-green-500 rounded-full flex-shrink-0"></span>
              <span>Treasury Control Review Passed</span>
            </li>
            <li className="flex items-center gap-3 text-sm">
              <span className="w-4 h-4 bg-amber-500 rounded-full flex-shrink-0"></span>
              <span>Regulatory Compliance Validation (In Progress)</span>
            </li>
          </ul>
        </div>
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/10">
          <h3 className="text-lg font-bold mb-4">Promotion Policy</h3>
          <p className="text-sm text-slate-400 mb-4 leading-relaxed">
            All releases to the main branch must pass all institutional readiness gates. Manual approval from two authorized officers is required for mainnet deployment.
          </p>
          <div className="text-xs text-slate-500 font-mono">
            POLICY_ID: CON-POL-001
          </div>
        </div>
      </div>
    </div>
  );
}
