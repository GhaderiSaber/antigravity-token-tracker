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
    if (pct <= 1.0) return '#ea4335';
    if (pct <= 10.0) return '#ea4335';
    if (pct <= 25.0) return '#fbbc04';
    if (pct <= 50.0) return '#4285f4';
    return '#34a853';
  }

  function getBadgeClass(id) {
    if (id.includes('claude')) return 'bg-brand-purple/15 text-purple-300 border-brand-purple/30';
    if (id.includes('gemini')) return 'bg-brand-blue/15 text-blue-300 border-brand-blue/30';
    if (id.includes('gpt')) return 'bg-brand-green/15 text-green-300 border-brand-green/30';
    return 'bg-white/5 text-gray-300 border-white/10';
  }

  return (
    <div className="mt-3 pt-3 border-t border-surface-border">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-full flex items-center justify-between p-2 rounded-lg bg-white/[0.03] hover:bg-white/[0.06] border border-surface-border text-xs text-gray-400 hover:text-gray-200 transition-all font-medium"
      >
        <span>🔍 Individual Model Quotas ({models.length} Models)</span>
        {isOpen ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
      </button>

      {isOpen && (
        <div className="mt-3 space-y-2 animate-fadeIn">
          {/* Search bar inside drawer */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-gray-500 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter models (e.g. 'claude', 'gemini-3', 'flash')..."
              className="w-full bg-surface-card border border-surface-border text-xs rounded-lg pl-8 pr-3 py-1.5 text-gray-200 placeholder-gray-500 outline-none focus:border-brand-blue font-mono"
            />
          </div>

          {/* Model table */}
          <div className="max-h-72 overflow-y-auto rounded-lg border border-surface-border bg-surface-card/60">
            <table className="w-full text-left font-mono text-xs">
              <thead className="bg-surface sticky top-0 text-[11px] text-gray-400 border-b border-surface-border">
                <tr>
                  <th className="py-2 px-3 font-semibold">Model ID</th>
                  <th className="py-2 px-3 font-semibold w-40">Remaining Quota</th>
                  <th className="py-2 px-3 font-semibold">Reset Time</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/5">
                {filtered.map((m) => {
                  const mId = m.modelId || 'unknown';
                  const pct = m.remainingPercent !== undefined ? m.remainingPercent : (m.remainingFraction || 0) * 100;
                  const parsed = parseResetTimestamp(m.resetTime, now);

                  return (
                    <tr key={mId} className="hover:bg-white/[0.02]">
                      <td className="py-1.5 px-3">
                        <span className={`inline-block px-2 py-0.5 rounded text-[11px] font-semibold border ${getBadgeClass(mId)}`}>
                          {mId}
                        </span>
                      </td>
                      <td className="py-1.5 px-3">
                        <div className="flex items-center gap-2">
                          <div className="flex-1 h-1.5 bg-gray-800 rounded-full overflow-hidden">
                            <div
                              className="h-full rounded-full transition-all"
                              style={{ width: `${Math.min(100, Math.max(0, pct))}%`, backgroundColor: getBarColor(pct) }}
                            />
                          </div>
                          <span className="font-bold text-gray-200 text-[11px]">{pct.toFixed(1)}%</span>
                        </div>
                      </td>
                      <td className="py-1.5 px-3 text-[11px] text-gray-400">
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
