import React from 'react';
import { Timer } from 'lucide-react';

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
    <div className="mb-6 p-4 rounded-xl border border-rose-200 dark:border-rose-900/60 bg-rose-50/70 dark:bg-rose-950/20 flex flex-wrap items-center justify-between gap-4 shadow-sm transition-colors">
      <div>
        <div className="flex items-center gap-2 text-sm font-bold text-rose-800 dark:text-rose-300">
          <Timer className="w-4 h-4 text-rose-600 dark:text-rose-400" />
          <span>Active Session Runout Clock ({activeEmail})</span>
          <span className="text-[10px] font-mono font-bold px-2 py-0.5 rounded bg-rose-100 dark:bg-rose-900/50 text-rose-700 dark:text-rose-300 border border-rose-200 dark:border-rose-800">
            {paceStatus}
          </span>
        </div>
        <p className="text-xs text-slate-700 dark:text-slate-300 mt-1">
          Tokens burning at <strong className="text-rose-600 dark:text-rose-400">{velocity.toFixed(1)}%/hr</strong>. Estimated depletion in <strong className="text-amber-600 dark:text-amber-400">{etaHuman}</strong> (around <strong className="text-slate-900 dark:text-white">{depletionTime}</strong>).
        </p>
      </div>

      <div className="flex items-center gap-2.5 font-mono text-xs">
        <div className="px-3 py-1.5 rounded-lg bg-white dark:bg-slate-900 border border-rose-200 dark:border-rose-900/50 shadow-sm">
          <span className="text-slate-500">Current: </span>
          <strong className="text-slate-900 dark:text-white">{currentPct.toFixed(1)}%</strong>
        </div>
        <div className="px-3 py-1.5 rounded-lg bg-white dark:bg-slate-900 border border-rose-200 dark:border-rose-900/50 shadow-sm">
          <span className="text-slate-500">Rate: </span>
          <strong className="text-rose-600 dark:text-rose-400">-{velocity.toFixed(1)}%/h</strong>
        </div>
        <div className="px-3 py-1.5 rounded-lg bg-white dark:bg-slate-900 border border-rose-200 dark:border-rose-900/50 shadow-sm">
          <span className="text-slate-500">Empty At: </span>
          <strong className="text-amber-600 dark:text-amber-400">{depletionTime}</strong>
        </div>
      </div>
    </div>
  );
}
