import React, { useState } from 'react';
import { ChevronDown, ChevronUp, Search } from 'lucide-react';
import { parseResetTimestamp } from '../hooks/useLiveTicker';

export default function ModelDrawer({ models, now }) {
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState('');

  if (!models || models.length === 0) return null;

  const filtered = models.filter((m) => {
    if (!query) return true;
    return (m.modelId || '').toLowerCase().includes(query.toLowerCase());
  });

  function getBarColor(pct) {
    if (pct <= 1.0) return 'bg-rose-500';
    if (pct <= 10.0) return 'bg-rose-500';
    if (pct <= 25.0) return 'bg-amber-500';
    if (pct <= 50.0) return 'bg-blue-500';
    return 'bg-emerald-500';
  }

  function getBadgeClass(id) {
    if (id.includes('claude')) return 'bg-purple-50 dark:bg-purple-950/50 text-purple-700 dark:text-purple-300 border-purple-200 dark:border-purple-800';
    if (id.includes('gemini')) return 'bg-blue-50 dark:bg-blue-950/50 text-blue-700 dark:text-blue-300 border-blue-200 dark:border-blue-800';
    if (id.includes('gpt')) return 'bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-300 border-emerald-200 dark:border-emerald-800';
    return 'bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 border-slate-200 dark:border-slate-700';
  }

  return (
    <div className="mt-3 pt-3 border-t border-slate-200 dark:border-slate-800">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-full flex items-center justify-between p-2 rounded-lg bg-slate-50 dark:bg-slate-800/40 hover:bg-slate-100 dark:hover:bg-slate-800 border border-slate-200 dark:border-slate-700/60 text-xs text-slate-600 dark:text-slate-300 transition-all font-medium shadow-sm"
      >
        <span>🔍 Individual Model Quotas ({models.length} Models)</span>
        {isOpen ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
      </button>

      {isOpen && (
        <div className="mt-3 space-y-2">
          {/* Search bar inside drawer */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter models (e.g. 'claude', 'gemini-3', 'flash')..."
              className="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 text-xs rounded-lg pl-8 pr-3 py-1.5 text-slate-900 dark:text-white placeholder-slate-400 outline-none focus:border-blue-500 font-mono shadow-sm"
            />
          </div>

          {/* Model table */}
          <div className="max-h-72 overflow-y-auto rounded-lg border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900/60 shadow-sm">
            <table className="w-full text-left font-mono text-xs">
              <thead className="bg-slate-50 dark:bg-slate-850 sticky top-0 text-[11px] text-slate-500 dark:text-slate-400 border-b border-slate-200 dark:border-slate-800">
                <tr>
                  <th className="py-2 px-3 font-semibold">Model ID</th>
                  <th className="py-2 px-3 font-semibold w-40">Remaining Quota</th>
                  <th className="py-2 px-3 font-semibold">Reset Time</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100 dark:divide-slate-800/60">
                {filtered.map((m) => {
                  const mId = m.modelId || 'unknown';
                  const pct = m.remainingPercent !== undefined ? m.remainingPercent : (m.remainingFraction || 0) * 100;
                  const parsed = parseResetTimestamp(m.resetTime, now);

                  return (
                    <tr key={mId} className="hover:bg-slate-50 dark:hover:bg-slate-800/30">
                      <td className="py-1.5 px-3">
                        <span className={`inline-block px-2 py-0.5 rounded text-[11px] font-semibold border ${getBadgeClass(mId)}`}>
                          {mId}
                        </span>
                      </td>
                      <td className="py-1.5 px-3">
                        <div className="flex items-center gap-2">
                          <div className="flex-1 h-1.5 bg-slate-200 dark:bg-slate-800 rounded-full overflow-hidden">
                            <div
                              className={`h-full rounded-full transition-all ${getBarColor(pct)}`}
                              style={{ width: `${Math.min(100, Math.max(0, pct))}%` }}
                            />
                          </div>
                          <span className="font-bold text-slate-800 dark:text-slate-200 text-[11px]">{pct.toFixed(1)}%</span>
                        </div>
                      </td>
                      <td className="py-1.5 px-3 text-[11px] text-slate-500 dark:text-slate-400">
                        {parsed.local !== 'N/A' ? parsed.local : 'Standard Cycle'}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
