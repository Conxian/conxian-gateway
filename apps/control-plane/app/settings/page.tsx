export default function SettingsPage() {
  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">Settings</h2>
        <p className="text-slate-400">System configuration and security parameters.</p>
      </div>

      <div className="max-w-2xl space-y-6">
        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <h3 className="text-lg font-bold mb-4">Authentication</h3>
          <div className="space-y-4">
            <div className="flex justify-between items-center">
              <div>
                <div className="text-sm font-semibold">API Token Enforcement</div>
                <div className="text-xs text-slate-500">Require Bearer tokens for all private routes.</div>
              </div>
              <div className="w-10 h-6 bg-indigo-600 rounded-full flex items-center justify-end px-1">
                <div className="w-4 h-4 bg-white rounded-full"></div>
              </div>
            </div>
            <div className="flex justify-between items-center">
              <div>
                <div className="text-sm font-semibold">Constant-Time Validation</div>
                <div className="text-xs text-slate-500">Prevent timing attacks using subtle comparison.</div>
              </div>
              <div className="w-10 h-6 bg-indigo-600 rounded-full flex items-center justify-end px-1">
                <div className="w-4 h-4 bg-white rounded-full"></div>
              </div>
            </div>
          </div>
        </div>

        <div className="p-6 border border-slate-800 rounded-xl bg-slate-800/20">
          <h3 className="text-lg font-bold mb-4">Persistence</h3>
          <div className="space-y-4">
            <div>
              <label className="text-xs text-slate-500 uppercase tracking-wider block mb-2">Offline Queue Secret</label>
              <input type="password" value="********************************" readOnly className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-sm text-slate-400" />
            </div>
            <div>
              <label className="text-xs text-slate-500 uppercase tracking-wider block mb-2">Storage Strategy</label>
              <select className="w-full bg-slate-900 border border-slate-700 rounded px-3 py-2 text-sm">
                <option>Atomic JSON (gateway_state.json)</option>
                <option>Sovereign Tableland</option>
                <option>GCP Cloud Storage (Encrypted)</option>
              </select>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
