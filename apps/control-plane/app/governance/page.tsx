"use client";

import ClientButton from "../../components/ClientButton";

export default function GovernancePage() {
  const handleAction = async () => {
    await new Promise(resolve => setTimeout(resolve, 1500));
    // Simulate API call
  };

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">Policy Approvals</h2>
        <p className="text-slate-400">Institutional governance and mandate lifecycle management.</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        <div className="lg:col-span-2 space-y-6">
          <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
            <div className="flex justify-between items-start mb-4">
              <div>
                <span className="text-[10px] font-bold text-indigo-400 uppercase tracking-widest bg-indigo-400/10 px-2 py-0.5 rounded mb-2 inline-block">Active Proposal</span>
                <h3 className="text-xl font-bold">CON-POL-005: Multi-Currency Jurisdictional Sharding</h3>
              </div>
              <span className="text-slate-500 text-xs">24h remaining</span>
            </div>
            <p className="text-slate-300 text-sm mb-6 leading-relaxed">
              Enable jurisdictional sharding for native settlements to align with global AML/CFT standards across BRICS and PAPSS regions. This includes mandatory ZSE (Zero Secret Egress) enforcement.
            </p>
            <div className="flex gap-4">
              <div className="flex-1">
                <ClientButton label="Approve Proposal" action={handleAction} variant="success" />
              </div>
              <div className="flex-1">
                <ClientButton label="Abstain" action={handleAction} variant="secondary" />
              </div>
            </div>
          </div>

          <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/10 opacity-60">
            <div className="flex justify-between items-start mb-4">
              <div>
                <span className="text-[10px] font-bold text-slate-400 uppercase tracking-widest bg-slate-400/10 px-2 py-0.5 rounded mb-2 inline-block">Closed Proposal</span>
                <h3 className="text-xl font-bold">CON-POL-004: TEE-Based Enclave Signing Enforcement</h3>
              </div>
              <span className="text-emerald-500 text-xs font-bold uppercase">Passed</span>
            </div>
            <p className="text-slate-400 text-sm leading-relaxed">
              Mandate the use of Trusted Execution Environments for all institutional signing operations.
            </p>
          </div>
        </div>

        <div className="space-y-6">
          <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/30">
            <h4 className="font-bold mb-4 flex items-center gap-2">
              <svg className="w-5 h-5 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
              </svg>
              Quorum Status
            </h4>
            <div className="space-y-4">
              <div>
                <div className="flex justify-between text-xs mb-1">
                  <span>Threshold Reached</span>
                  <span>75%</span>
                </div>
                <div className="w-full bg-slate-700 h-1.5 rounded-full overflow-hidden">
                  <div className="bg-indigo-500 h-full w-[75%]"></div>
                </div>
              </div>
              <div className="text-xs text-slate-500">
                Current participants: 3 of 4 required.
              </div>
            </div>
          </div>

          <div className="p-6 border border-slate-800 rounded-xl bg-indigo-500/5">
            <h4 className="font-bold mb-2">Institutional Guardrails</h4>
            <p className="text-xs text-slate-400 leading-relaxed">
              Governance proposals require a 72-hour review period and 144-block Stacks timelock for finalization once approved.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
