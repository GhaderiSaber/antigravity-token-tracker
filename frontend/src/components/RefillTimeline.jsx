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
          isUrgent: parsed.diffSecs < 86400
        });
        break;
      }
    }
  }

  items.sort((a, b) => a.diffSecs - b.diffSecs);

  return (
    <section className="mb-6 p-4 rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900/60 shadow-sm transition-colors">
      <div className="flex items-center justify-between mb-3.5">
        <div className="flex items-center gap-2 font-bold text-sm text-slate-800 dark:text-slate-200">
          <Calendar className="w-4 h-4 text-blue-500" />
          <span>Upcoming Quota Resets & Refill Schedule</span>
          <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-emerald-50 dark:bg-emerald-950/50 text-emerald-600 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-800 font-semibold">
            {items.length} ACCOUNTS TRACKED
          </span>
        </div>
        <span className="text-xs text-slate-500 dark:text-slate-400 hidden sm:inline font-mono">
          Clocks tick live in browser
        </span>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
        {items.map((it) => {
          const countdownStr = formatCountdown(it.diffSecs);
          return (
            <div
              key={it.email}
              className={`p-3 rounded-xl border transition-all flex flex-col justify-between gap-1.5 shadow-sm ${
                it.isUrgent
                  ? 'border-emerald-300 dark:border-emerald-800/80 bg-emerald-50/50 dark:bg-emerald-950/20'
                  : 'border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-900/80'
              }`}
            >
              <div className="flex items-center justify-between gap-1">
                <span className="text-xs font-bold text-slate-900 dark:text-white truncate" title={it.email}>
                  {it.email.split('@')[0]}
                </span>
                {it.isUrgent ? (
                  <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded bg-emerald-100 dark:bg-emerald-900/50 text-emerald-700 dark:text-emerald-300 flex items-center gap-0.5">
                    <Flame className="w-2.5 h-2.5 fill-current" /> SOON
                  </span>
                ) : (
                  <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-slate-200 dark:bg-slate-800 text-slate-600 dark:text-slate-400">
                    WEEKLY
                  </span>
                )}
              </div>

              <div className={`font-mono text-sm font-bold tracking-tight ${it.isUrgent ? 'text-emerald-600 dark:text-emerald-400' : 'text-blue-600 dark:text-blue-400'}`}>
                ⏱️ {countdownStr}
              </div>

              <div className="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400 font-mono pt-1 border-t border-slate-200/60 dark:border-slate-800">
                <span className="truncate" title={it.localStr}>{it.localStr}</span>
                <span className="font-semibold text-slate-700 dark:text-slate-300">{it.remainingPct.toFixed(1)}%</span>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
