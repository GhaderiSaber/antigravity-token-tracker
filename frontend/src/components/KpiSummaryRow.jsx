import React from 'react';
import { Laptop, Monitor, Clock, ShieldCheck, ShieldAlert, Zap } from 'lucide-react';
import { parseResetTimestamp, formatCountdown } from '../hooks/useLiveTicker';

export default function KpiSummaryRow({ data, now }) {
  const activeSessions = data?.active_sessions || {};
  const desktop = activeSessions.desktop;
  const ide = activeSessions.ide;
  const geo = data?.geo;
  const shield = data?.shield;
  const accounts = data?.accounts || {};

  // Find next upcoming reset
  let nextRefill = null;
  for (const [email, acc] of Object.entries(accounts)) {
    if (!acc.groups) continue;
    for (const g of acc.groups) {
      if (g.displayName?.toLowerCase().includes('gemini') && g.weekly?.resetTime) {
        const parsed = parseResetTimestamp(g.weekly.resetTime, now);
        if (!nextRefill || parsed.diffSecs < nextRefill.diffSecs) {
          nextRefill = {
            email,
            diffSecs: parsed.diffSecs,
            localStr: parsed.local,
            pct: g.weekly.remainingPercent || 0
          };
        }
        break;
      }
    }
  }

  const isShieldOn = Boolean(shield?.enabled);
  const isRestricted = Boolean(geo?.is_restricted);

  function getBarColor(pct) {
    if (pct <= 1.0) return 'bg-rose-500';
    if (pct <= 10.0) return 'bg-rose-500';
    if (pct <= 25.0) return 'bg-amber-500';
    if (pct <= 50.0) return 'bg-blue-500';
    return 'bg-emerald-500';
  }

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      {/* 1. Desktop App Session */}
      <div className="p-4 rounded-xl bg-white dark:bg-slate-900/70 border border-slate-200/80 dark:border-slate-800 shadow-sm hover:shadow transition-all">
        <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-2">
          <span className="flex items-center gap-1.5 font-semibold">
            <Monitor className="w-3.5 h-3.5 text-blue-500" />
            Desktop App
          </span>
          <span className="text-[10px] font-mono font-bold px-1.5 py-0.5 rounded bg-blue-50 dark:bg-blue-950/50 text-blue-600 dark:text-blue-400 border border-blue-200/60 dark:border-blue-900/60">
            SURFACE
          </span>
        </div>
        <div className="text-sm font-bold text-slate-900 dark:text-white truncate" title={desktop?.email || 'None'}>
          {desktop?.email ? desktop.email.split('@')[0] : 'Not Signed In'}
        </div>
        <div className="flex items-center justify-between mt-2 pt-2 border-t border-slate-100 dark:border-slate-800/80 text-xs">
          <span className="text-slate-500 dark:text-slate-400">Gemini Quota:</span>
          <div className="flex items-center gap-2">
            <div className="w-12 h-1.5 bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden">
              <div
                className={`h-full rounded-full ${getBarColor(desktop?.quota_pct || 0)}`}
                style={{ width: `${Math.min(100, Math.max(0, desktop?.quota_pct || 0))}%` }}
              />
            </div>
            <strong className="font-mono text-slate-900 dark:text-white">
              {(desktop?.quota_pct || 0).toFixed(1)}%
            </strong>
          </div>
        </div>
      </div>

      {/* 2. IDE Session */}
      <div className="p-4 rounded-xl bg-white dark:bg-slate-900/70 border border-slate-200/80 dark:border-slate-800 shadow-sm hover:shadow transition-all">
        <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-2">
          <span className="flex items-center gap-1.5 font-semibold">
            <Laptop className="w-3.5 h-3.5 text-purple-500" />
            Antigravity IDE
          </span>
          <span className="text-[10px] font-mono font-bold px-1.5 py-0.5 rounded bg-purple-50 dark:bg-purple-950/50 text-purple-600 dark:text-purple-400 border border-purple-200/60 dark:border-purple-900/60">
            EDITOR
          </span>
        </div>
        <div className="text-sm font-bold text-slate-900 dark:text-white truncate" title={ide?.email || 'None'}>
          {ide?.email ? ide.email.split('@')[0] : 'Not Signed In'}
        </div>
        <div className="flex items-center justify-between mt-2 pt-2 border-t border-slate-100 dark:border-slate-800/80 text-xs">
          <span className="text-slate-500 dark:text-slate-400">Gemini Quota:</span>
          <div className="flex items-center gap-2">
            <div className="w-12 h-1.5 bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden">
              <div
                className={`h-full rounded-full ${getBarColor(ide?.quota_pct || 0)}`}
                style={{ width: `${Math.min(100, Math.max(0, ide?.quota_pct || 0))}%` }}
              />
            </div>
            <strong className="font-mono text-slate-900 dark:text-white">
              {(ide?.quota_pct || 0).toFixed(1)}%
            </strong>
          </div>
        </div>
      </div>

      {/* 3. Next Upcoming Refill */}
      <div className="p-4 rounded-xl bg-white dark:bg-slate-900/70 border border-slate-200/80 dark:border-slate-800 shadow-sm hover:shadow transition-all">
        <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-2">
          <span className="flex items-center gap-1.5 font-semibold">
            <Clock className="w-3.5 h-3.5 text-emerald-500" />
            Next Quota Refill
          </span>
          <span className="text-[10px] font-mono font-bold px-1.5 py-0.5 rounded bg-emerald-50 dark:bg-emerald-950/50 text-emerald-600 dark:text-emerald-400 border border-emerald-200/60 dark:border-emerald-900/60">
            LIVE CLOCK
          </span>
        </div>
        <div className="text-sm font-mono font-bold text-emerald-600 dark:text-emerald-400">
          {nextRefill ? `⏱️ ${formatCountdown(nextRefill.diffSecs)}` : 'N/A'}
        </div>
        <div className="flex items-center justify-between mt-2 pt-2 border-t border-slate-100 dark:border-slate-800/80 text-xs text-slate-500 dark:text-slate-400">
          <span className="truncate" title={nextRefill?.email}>
            {nextRefill ? nextRefill.email.split('@')[0] : 'None'}
          </span>
          <span className="font-mono">{nextRefill ? nextRefill.localStr : ''}</span>
        </div>
      </div>

      {/* 4. Security & Egress Status */}
      <div className="p-4 rounded-xl bg-white dark:bg-slate-900/70 border border-slate-200/80 dark:border-slate-800 shadow-sm hover:shadow transition-all">
        <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mb-2">
          <span className="flex items-center gap-1.5 font-semibold">
            {isRestricted ? (
              <ShieldAlert className="w-3.5 h-3.5 text-rose-500" />
            ) : (
              <ShieldCheck className="w-3.5 h-3.5 text-blue-500" />
            )}
            Network Security
          </span>
          <span
            className={`text-[10px] font-mono font-bold px-1.5 py-0.5 rounded ${
              isShieldOn
                ? 'bg-blue-50 dark:bg-blue-950/50 text-blue-600 dark:text-blue-400 border border-blue-200 dark:border-blue-900'
                : 'bg-slate-100 dark:bg-slate-800 text-slate-500'
            }`}
          >
            SHIELD: {isShieldOn ? 'ON' : 'OFF'}
          </span>
        </div>
        <div className="text-sm font-bold text-slate-900 dark:text-white flex items-center gap-1.5">
          <span>{geo?.flag || '🌐'}</span>
          <span>{geo?.ip || 'Checking IP...'}</span>
        </div>
        <div className="flex items-center justify-between mt-2 pt-2 border-t border-slate-100 dark:border-slate-800/80 text-xs text-slate-500 dark:text-slate-400">
          <span>{geo?.city ? `${geo.city}, ${geo.country_code}` : (geo?.country_name || 'Direct')}</span>
          <span className={isRestricted ? 'text-rose-500 font-bold' : 'text-emerald-500 font-medium'}>
            {isRestricted ? '⚠️ RESTRICTED' : '✓ Safe'}
          </span>
        </div>
      </div>
    </div>
  );
}
