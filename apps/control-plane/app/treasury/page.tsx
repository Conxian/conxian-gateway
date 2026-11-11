export default function TreasuryPage() {
  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-3xl font-bold mb-2">Treasury Monitor</h2>
        <p className="text-slate-400">
          Treasury balances, liquidity, FX, and yield are not currently published because their provider evidence is not qualified.
        </p>
      </div>

      <div className="rounded-xl border border-amber-700/40 bg-amber-950/20 p-6">
        <div className="text-xs uppercase tracking-wider text-amber-300">Data status</div>
        <div className="mt-2 text-2xl font-semibold text-amber-200">Unavailable — evidence gate open</div>
        <p className="mt-3 max-w-3xl text-sm text-slate-300">
          The previous deterministic treasury simulation has been disabled. No synthetic balance, sBTC liquidity,
          FX rate, Sovereign Yield Index, or tranche-yield value is presented as live data.
        </p>
        <p className="mt-3 text-sm text-slate-400">
          Re-publication requires a verified provider and network, exact asset pair and units, observation timestamp,
          freshness window, provenance, and independent operational acceptance. See Gateway issue #880.
        </p>
      </div>
    </div>
  );
}
