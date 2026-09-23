import React from 'react';
import { Timer, AlertOctagon } from 'lucide-react';

export default function RunoutBanner({ burnrate }) {
  if (!burnrate || !burnrate.active_metrics || !burnrate.active_metrics.primary) {
    return null;
  }

  const pri = burnrate.active_metrics.primary;
  if (!pri.is_depleting) {
    return null;
  }

  const activeEmail = burnrate.active_email || 'Active Account';
  const velocity = pri.velocity_pct_per_hour || 0;
  const etaHuman = pri.eta_human || 'N/A';
  const depletionTime = pri.depletion_time_str || 'N/A';
  const paceStatus = pri.pace_status || 'DEPLETING';
  const currentPct = pri.current_quota_pct || 0;

  return (
    <div className="mb-6 p-4 rounded-xl border border-brand-red/40 bg-gradient-to-r from-brand-red/15 via-brand-yellow/10 to-transparent flex flex-wrap items-center justify-between gap-4 shadow-sm">
      <div>
        <div className="flex items-center gap-2 text-sm font-bold text-red-300">
          <Timer className="w-4 h-4 text-brand-red" />
          <span>Active Session Runout Clock ({activeEmail})</span>
          <span className="text-[10px] font-mono font-bold px-2 py-0.5 rounded bg-brand-red/20 text-red-300 border border-brand-red/30">
            {paceStatus}
          </span>
        </div>
        <p className="text-xs text-gray-200 mt-1">
          Tokens burning at <strong>{velocity.toFixed(1)}%/hr</strong>. Estimated depletion in <strong className="text-brand-yellow">{etaHuman}</strong> (around <strong className="text-white">{depletionTime}</strong>).
        </p>
      </div>

      <div className="flex items-center gap-3">
        <div className="px-3 py-1.5 rounded-lg bg-black/30 border border-white/5 font-mono text-xs">
          <span className="text-gray-400">Current: </span>
          <strong className="text-white">{currentPct.toFixed(1)}%</strong>
        </div>
        <div className="px-3 py-1.5 rounded-lg bg-black/30 border border-white/5 font-mono text-xs">
          <span className="text-gray-400">Rate: </span>
          <strong className="text-red-400">-{velocity.toFixed(1)}%/h</strong>
        </div>
        <div className="px-3 py-1.5 rounded-lg bg-black/30 border border-white/5 font-mono text-xs">
          <span className="text-gray-400">Empty At: </span>
          <strong className="text-yellow-300">{depletionTime}</strong>
        </div>
      </div>
    </div>
  );
}
