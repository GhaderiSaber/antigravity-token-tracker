import React from 'react';
import { parseResetTimestamp, formatCountdown } from '../hooks/useLiveTicker';
import ModelDrawer from './ModelDrawer';

export default function AccountCard({ account, now, onSwitchAccount }) {
  if (!account) return null;

  const email = account.email || 'Unknown';
  const name = account.name || '';
  const tier = account.tier || 'Standard';
  const isDesktop = Boolean(account.is_current_desktop_session);
  const isIde = Boolean(account.is_current_ide_session);
  const isActive = isDesktop || isIde;

  function getBarColor(pct) {
    if (pct <= 1.0) return 'bg-rose-500';
    if (pct <= 10.0) return 'bg-rose-500';
    if (pct <= 25.0) return 'bg-amber-500';
    if (pct <= 50.0) return 'bg-blue-500';
    return 'bg-emerald-500';
  }

  function getStatusBadge(pct) {
    if (pct <= 1.0) {
      return (
        <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-rose-50 dark:bg-rose-950/50 text-rose-700 dark:text-rose-300 border border-rose-200 dark:border-rose-900/60">
          FINISHED / EXHAUSTED
        </span>
      );
    }
    if (pct <= 10.0) {
      return (
        <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-rose-50 dark:bg-rose-950/50 text-rose-700 dark:text-rose-300 border border-rose-200 dark:border-rose-900/60">
          CRITICAL
        </span>
      );
    }
    if (pct <= 25.0) {
      return (
        <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-amber-50 dark:bg-amber-950/50 text-amber-700 dark:text-amber-300 border border-amber-200 dark:border-amber-900/60">
          LOW
        </span>
      );
    }
    return (
      <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-900/60">
        HEALTHY
      </span>
    );
  }

  return (
    <div
      className={`p-5 rounded-2xl border transition-all shadow-sm ${
        isActive
          ? 'border-blue-300 dark:border-blue-900/60 bg-white dark:bg-slate-900/80 ring-1 ring-blue-500/20'
          : 'border-slate-200/90 dark:border-slate-800 bg-white dark:bg-slate-900/60 hover:shadow'
      }`}
    >
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
        <div className="flex flex-wrap items-center gap-2.5">
          <span className="text-base font-bold text-slate-900 dark:text-white tracking-tight">{email}</span>
          {name && <span className="text-xs text-slate-500 dark:text-slate-400 font-medium">({name})</span>}
          <span className="text-[11px] font-semibold px-2 py-0.5 rounded-full bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-900/60">
            {tier}
          </span>

          {/* Active Badges */}
          {isDesktop && isIde ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-bold px-2.5 py-0.5 rounded-full bg-gradient-to-r from-emerald-50 to-blue-50 dark:from-emerald-950/40 dark:to-blue-950/40 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
              Active (App + IDE)
            </span>
          ) : isDesktop ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-bold px-2.5 py-0.5 rounded-full bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
              Active in Desktop App
            </span>
          ) : isIde ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-bold px-2.5 py-0.5 rounded-full bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800">
              <span className="w-1.5 h-1.5 rounded-full bg-blue-500" />
              Active in IDE
            </span>
          ) : account.is_cached ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-full bg-amber-50 dark:bg-amber-950/30 text-amber-700 dark:text-amber-300 border border-amber-200 dark:border-amber-900">
              <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
              Inactive (Last Seen)
            </span>
          ) : (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400">
              <span className="w-1.5 h-1.5 rounded-full bg-slate-400" />
              Inactive
            </span>
          )}
        </div>

        {/* Surface Switch Actions */}
        <div>
          {isDesktop && isIde ? (
            <span className="text-xs text-emerald-600 dark:text-emerald-400 font-bold flex items-center gap-1">
              ✓ Active on Both Surfaces
            </span>
          ) : isDesktop && !isIde ? (
            <div className="inline-flex items-center gap-1.5 p-1 rounded-lg bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-800">
              <span className="text-[11px] font-bold text-emerald-600 dark:text-emerald-400 px-1.5">📱 App Active</span>
              <button
                onClick={() => onSwitchAccount(email, 'ide')}
                title="Switch IDE to this account (Desktop App remains active)"
                className="text-[11px] font-semibold px-2.5 py-1 rounded bg-blue-50 dark:bg-blue-950/60 hover:bg-blue-100 dark:hover:bg-blue-900/60 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800 transition-all shadow-sm"
              >
                💻 Switch IDE Here
              </button>
            </div>
          ) : !isDesktop && isIde ? (
            <div className="inline-flex items-center gap-1.5 p-1 rounded-lg bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-800">
              <span className="text-[11px] font-bold text-blue-600 dark:text-blue-400 px-1.5">💻 IDE Active</span>
              <button
                onClick={() => onSwitchAccount(email, 'desktop')}
                title="Switch Desktop App to this account (IDE remains active)"
                className="text-[11px] font-semibold px-2.5 py-1 rounded bg-blue-50 dark:bg-blue-950/60 hover:bg-blue-100 dark:hover:bg-blue-900/60 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800 transition-all shadow-sm"
              >
                📱 Switch App Here
              </button>
            </div>
          ) : (
            <div className="inline-flex items-center gap-1 p-1 rounded-lg bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-800">
              <button
                onClick={() => onSwitchAccount(email, 'both')}
                title={`Switch both surfaces to ${email}`}
                className="text-[11px] font-bold px-2.5 py-1 rounded bg-gradient-to-r from-blue-600 to-indigo-600 text-white hover:opacity-95 transition-all shadow-sm"
              >
                ⚡ Both
              </button>
              <button
                onClick={() => onSwitchAccount(email, 'desktop')}
                title="Switch Desktop App only"
                className="text-[11px] font-semibold px-2 py-1 rounded bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-700 transition-all"
              >
                📱 App
              </button>
              <button
                onClick={() => onSwitchAccount(email, 'ide')}
                title="Switch IDE only"
                className="text-[11px] font-semibold px-2 py-1 rounded bg-white dark:bg-slate-800 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-700 transition-all"
              >
                💻 IDE
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Quota Groups Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {(!account.groups || account.groups.length === 0) && (
          <div className="col-span-full py-6 text-center text-xs text-slate-400 italic">
            No quota data recorded yet. Log into Antigravity with this account to snapshot session & quota.
          </div>
        )}

        {(account.groups || []).map((g) => {
          const w = g.weekly;
          const f = g.fiveHour;
          const wPct = w?.remainingPercent || 0;
          const fPct = f?.remainingPercent || 0;

          const wReset = w ? parseResetTimestamp(w.resetTime, now) : null;
          const wCountdownStr = wReset ? formatCountdown(wReset.diffSecs) : 'N/A';
          const isUrgent = wReset && wReset.diffSecs < 86400;

          const fReset = f ? parseResetTimestamp(f.resetTime, now) : null;
          const fCountdownStr = fReset ? formatCountdown(fReset.diffSecs) : 'N/A';

          return (
            <div key={g.displayName} className="p-4 rounded-xl border border-slate-200/80 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-900/60 flex flex-col justify-between">
              <div>
                <div className="flex items-center justify-between mb-3">
                  <span className="font-bold text-sm text-slate-900 dark:text-white">{g.displayName}</span>
                  {w && getStatusBadge(wPct)}
                </div>

                {/* Weekly Limit Meter */}
                <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-1.5">
                  <span>Weekly Quota Limit</span>
                  <strong className="font-mono text-slate-900 dark:text-white">{wPct.toFixed(1)}%</strong>
                </div>
                <div className="w-full h-2.5 bg-slate-200 dark:bg-slate-800 rounded-full overflow-hidden mb-3">
                  <div
                    className={`h-full rounded-full transition-all duration-500 ${getBarColor(wPct)}`}
                    style={{ width: `${Math.min(100, Math.max(0, wPct))}%` }}
                  />
                </div>

                {/* High-Contrast Weekly Reset Countdown Box */}
                <div className="p-3 rounded-xl border border-slate-200 dark:border-slate-800/80 bg-white dark:bg-slate-950/60 mb-3 space-y-1 shadow-sm">
                  <div className="flex items-center justify-between text-[10px] font-mono uppercase tracking-wider text-slate-500 dark:text-slate-400">
                    <span>Weekly Refill In</span>
                    <span className={isUrgent ? 'text-emerald-600 dark:text-emerald-400 font-bold' : ''}>
                      {isUrgent ? '🔥 SOON' : 'WEEKLY CYCLE'}
                    </span>
                  </div>
                  <div className={`font-mono text-base font-bold tracking-tight ${isUrgent ? 'text-emerald-600 dark:text-emerald-400' : 'text-blue-600 dark:text-blue-400'}`}>
                    ⏱️ {wCountdownStr}
                  </div>
                  <div className="flex items-center justify-between text-[11px] font-mono text-slate-500 dark:text-slate-400 pt-0.5">
                    <span>Local: {wReset ? wReset.local : 'N/A'}</span>
                    <span className="opacity-70">{wReset ? wReset.utc : ''}</span>
                  </div>
                </div>
              </div>

              {/* 5-Hour Rolling Limit */}
              <div className="pt-3 border-t border-slate-200/60 dark:border-slate-800/80">
                <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-1">
                  <span>5-Hour Rolling Limit</span>
                  <strong className="font-mono text-slate-900 dark:text-white">{fPct.toFixed(1)}%</strong>
                </div>
                <div className="w-full h-2 bg-slate-200 dark:bg-slate-800 rounded-full overflow-hidden mb-2">
                  <div
                    className={`h-full rounded-full transition-all duration-500 ${getBarColor(fPct)}`}
                    style={{ width: `${Math.min(100, Math.max(0, fPct))}%` }}
                  />
                </div>
                <div className="flex items-center justify-between text-[11px] font-mono text-slate-500 dark:text-slate-400">
                  <span>5h Reset In: <strong className="text-blue-600 dark:text-blue-400">{fCountdownStr}</strong></span>
                  <span>{fReset ? fReset.local : ''}</span>
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Model Breakdown Accordion */}
      <ModelDrawer models={account.models} now={now} />
    </div>
  );
}
