import ClientButton from "../../components/ClientButton";

export default function IdentityPage() {
  const handleAction = async () => {
    await new Promise(resolve => setTimeout(resolve, 1500));
  };

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">Identity Resolution</h2>
        <p className="text-slate-400">Manage BNS, ENS, and World ID resolution paths.</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <h3 className="text-lg font-bold mb-4">Resolver Status</h3>
          <div className="space-y-4">
            <div className="flex justify-between items-center p-3 bg-slate-900/50 rounded-lg">
              <span className="text-sm">Stacks BNS (call_read_only)</span>
              <span className="text-xs text-green-400 font-mono">OPERATIONAL</span>
            </div>
            <div className="flex justify-between items-center p-3 bg-slate-900/50 rounded-lg">
              <span className="text-sm">Ethereum ENS</span>
              <span className="text-xs text-green-400 font-mono">OPERATIONAL</span>
            </div>
            <div className="flex justify-between items-center p-3 bg-slate-900/50 rounded-lg">
              <span className="text-sm">World ID (WIF)</span>
              <span className="text-xs text-indigo-400 font-mono">ZSE ACTIVE</span>
            </div>
          </div>
        </div>

        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <h3 className="text-lg font-bold mb-4">Manual Resolution</h3>
          <div className="space-y-4">
            <input type="text" placeholder="sentinel: Enter identifier (e.g. jules.btc)" className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-sm focus:border-indigo-500 outline-none" />
            <ClientButton label="Resolve Identity" action={handleAction} />
          </div>
        </div>
      </div>
    </div>
  );
}
