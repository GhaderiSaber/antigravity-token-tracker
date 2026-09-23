import React from 'react';
import { RotateCcw } from 'lucide-react';

export default function BalancerCard({ balancer, onUpdateStrategy, onRotate }) {
  if (!balancer) return null;

  const isBalOn = Boolean(balancer.enabled);
  const strat = balancer.strategy || 'watermark';

  return (
    <div className="mb-6 p-4 rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900/60 flex flex-wrap items-center justify-between gap-4 shadow-sm transition-colors">
      <div className="flex items-center gap-3">
        <span className="text-2xl">🔄</span>
        <div>
          <div className="text-sm font-bold text-slate-800 dark:text-slate-200 flex items-center gap-2">
            <span>Auto-Balancer & Account Pool Rotation</span>
            <span
              className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded-full ${
                isBalOn
                  ? 'bg-emerald-50 dark:bg-emerald-950/50 text-emerald-600 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800'
                  : 'bg-slate-100 dark:bg-slate-800 text-slate-500'
              }`}
            >
              {isBalOn ? 'ACTIVE' : 'OFF'}
            </span>
          </div>
          <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
            Optimizes token consumption across all registered accounts.
          </p>
        </div>
      </div>

      <div className="flex items-center gap-2.5">
        <select
          value={strat}
          onChange={(e) => onUpdateStrategy(e.target.value)}
          className="bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-800 text-slate-700 dark:text-slate-200 text-xs rounded-lg px-3 py-1.5 outline-none focus:border-blue-500 cursor-pointer shadow-sm"
        >
          <option value="watermark">Watermark Leveling</option>
          <option value="expiry_first">Reset Optimizer (Zero Waste)</option>
          <option value="round_robin">Round-Robin Interval</option>
          <option value="reactive">Reactive Fallback Only</option>
        </select>
        <button
          onClick={onRotate}
          className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 hover:border-blue-400 text-slate-700 dark:text-slate-200 shadow-sm transition-all"
        >
          <RotateCcw className="w-3.5 h-3.5 text-blue-500" />
          <span>Rotate Pool Now</span>
        </button>
      </div>
    </div>
  );
}
