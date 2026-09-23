import React from 'react';
import { Calendar, Flame } from 'lucide-react';
import { parseResetTimestamp, formatCountdown } from '../hooks/useLiveTicker';

export default function RefillTimeline({ accounts, now }) {
  if (!accounts) return null;

  const items = [];
  for (const [email, acc] of Object.entries(accounts)) {
    if (!acc.groups) continue;
    for (const g of acc.groups) {
      if (g.displayName?.toLowerCase().includes('gemini') && g.weekly?.resetTime) {
        const parsed = parseResetTimestamp(g.weekly.resetTime, now);
        items.push({
          email,
          name: acc.name,
          tier: acc.tier,
          group: g.displayName,
          remainingPct: g.weekly.remainingPercent || 0,
          resetIso: g.weekly.resetTime,
          diffSecs: parsed.diffSecs,
          localStr: parsed.local,
          utcStr: parsed.utc,
          isUrgent: parsed.diffSecs < 86400 // Within 24 hours
        });
        break;
      }
    }
  }

  // Sort soonest first
  items.sort((a, b) => a.diffSecs - b.diffSecs);

  return (
    <section className="mb-6 p-4 rounded-xl border border-surface-border bg-surface shadow-sm">
      <div className="flex items-center justify-between mb-3.5">
        <div className="flex items-center gap-2 font-bold text-sm text-gray-200">
          <Calendar className="w-4 h-4 text-brand-blue" />
          <span>Upcoming Quota Resets & Refill Schedule</span>
          <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-brand-green/15 text-green-400 border border-brand-green/30">
            {items.length} ACCOUNTS TRACKED
          </span>
        </div>
        <span className="text-xs text-gray-400 hidden sm:inline">
          Clocks tick live in your local timezone
        </span>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
        {items.map((it) => {
          const countdownStr = formatCountdown(it.diffSecs);
          return (
            <div
              key={it.email}
              className={`p-3 rounded-lg border transition-all flex flex-col justify-between gap-1.5 ${
                it.isUrgent
                  ? 'border-brand-green/40 bg-gradient-to-br from-brand-green/10 to-surface-card hover:border-brand-green'
                  : 'border-surface-border bg-surface-card hover:border-surface-border-light'
              }`}
            >
              <div className="flex items-center justify-between gap-1">
                <span className="text-xs font-bold text-gray-200 truncate" title={it.email}>
                  {it.email.split('@')[0]}
                </span>
                {it.isUrgent ? (
                  <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded bg-brand-green/20 text-green-300 flex items-center gap-0.5">
                    <Flame className="w-2.5 h-2.5" /> REFILLS SOON
                  </span>
                ) : (
                  <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-white/5 text-gray-400">
                    WEEKLY
                  </span>
                )}
              </div>

              <div className={`font-mono text-sm font-bold tracking-tight ${it.isUrgent ? 'text-green-300' : 'text-blue-300'}`}>
                ⏱️ {countdownStr}
              </div>

              <div className="flex items-center justify-between text-[11px] text-gray-400 font-mono pt-1 border-t border-white/5">
                <span className="truncate" title={it.localStr}>{it.localStr}</span>
                <span className="font-semibold text-gray-300">{it.remainingPct.toFixed(1)}%</span>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
