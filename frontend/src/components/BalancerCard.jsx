import React from 'react';
import { RotateCcw } from 'lucide-react';

export default function BalancerCard({ balancer, onUpdateStrategy, onRotate }) {
  if (!balancer) return null;

  const isBalOn = Boolean(balancer.enabled);
  const strat = balancer.strategy || 'watermark';

  return (
    <div className="mb-6 p-4 rounded-xl border border-surface-border bg-surface flex flex-wrap items-center justify-between gap-4 shadow-sm">
      <div className="flex items-center gap-3">
        <span className="text-2xl">🔄</span>
        <div>
          <div className="text-sm font-bold flex items-center gap-2">
            <span>Auto-Balancer & Account Pool Rotation</span>
            <span
              className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded-full ${
                isBalOn ? 'bg-brand-green/15 text-green-400 border border-brand-green/30' : 'bg-white/5 text-gray-400'
              }`}
            >
              {isBalOn ? 'ACTIVE' : 'OFF'}
            </span>
          </div>
          <p className="text-xs text-gray-400 mt-0.5">
            Optimizes token consumption across all registered accounts.
          </p>
        </div>
      </div>

      <div className="flex items-center gap-2.5">
        <select
          value={strat}
          onChange={(e) => onUpdateStrategy(e.target.value)}
          className="bg-surface-card border border-surface-border text-gray-200 text-xs rounded-lg px-3 py-1.5 outline-none focus:border-brand-blue cursor-pointer"
        >
          <option value="watermark">Watermark Leveling</option>
          <option value="expiry_first">Reset Optimizer (Zero Waste)</option>
          <option value="round_robin">Round-Robin Interval</option>
          <option value="reactive">Reactive Fallback Only</option>
        </select>
        <button
          onClick={onRotate}
          className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold bg-surface-elevated border border-surface-border hover:border-brand-blue transition-all"
        >
          <RotateCcw className="w-3.5 h-3.5 text-brand-blue" />
          <span>Rotate Pool Now</span>
        </button>
      </div>
    </div>
  );
}
