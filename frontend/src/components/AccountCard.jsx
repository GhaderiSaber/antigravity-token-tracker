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
    if (pct <= 1.0) return '#ea4335';
    if (pct <= 10.0) return '#ea4335';
    if (pct <= 25.0) return '#fbbc04';
    if (pct <= 50.0) return '#4285f4';
    return '#34a853';
  }

  function getStatusBadge(pct) {
    if (pct <= 1.0) {
      return <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-brand-red/15 text-red-400 border border-brand-red/30">FINISHED / EXHAUSTED</span>;
    }
    if (pct <= 10.0) {
      return <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-brand-red/15 text-red-400 border border-brand-red/30">CRITICAL</span>;
    }
    if (pct <= 25.0) {
      return <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-brand-yellow/15 text-yellow-400 border border-brand-yellow/30">LOW</span>;
    }
    return <span className="text-[10px] font-bold px-2 py-0.5 rounded bg-brand-green/15 text-green-400 border border-brand-green/30">HEALTHY</span>;
  }

  return (
    <div
      className={`p-5 rounded-2xl border transition-all shadow-md ${
        isActive
          ? 'border-brand-blue/40 bg-gradient-to-b from-brand-blue/5 to-surface'
          : 'border-surface-border bg-surface hover:border-surface-border-light'
      }`}
    >
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
        <div className="flex flex-wrap items-center gap-2.5">
          <span className="text-base font-bold text-white tracking-tight">{email}</span>
          {name && <span className="text-xs text-gray-400 font-medium">({name})</span>}
          <span className="text-[11px] font-semibold px-2 py-0.5 rounded-full bg-brand-blue/15 text-blue-300 border border-brand-blue/30">
            {tier}
          </span>

          {/* Active Badges */}
          {isDesktop && isIde ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-bold px-2.5 py-0.5 rounded-full bg-gradient-to-r from-brand-green/20 to-brand-blue/20 text-green-300 border border-brand-green/40">
              <span className="w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse" />
              Active (App + IDE)
            </span>
          ) : isDesktop ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-bold px-2.5 py-0.5 rounded-full bg-brand-green/15 text-green-300 border border-brand-green/30">
              <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
              Active in Desktop App
            </span>
          ) : isIde ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-bold px-2.5 py-0.5 rounded-full bg-brand-blue/15 text-blue-300 border border-brand-blue/30">
              <span className="w-1.5 h-1.5 rounded-full bg-blue-400" />
              Active in IDE
            </span>
          ) : account.is_cached ? (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-full bg-yellow-400/10 text-yellow-300 border border-yellow-400/20">
              <span className="w-1.5 h-1.5 rounded-full bg-yellow-400" />
              Inactive (Last Seen)
            </span>
          ) : (
            <span className="inline-flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-full bg-white/5 text-gray-400 border border-white/10">
              <span className="w-1.5 h-1.5 rounded-full bg-gray-500" />
              Inactive
            </span>
          )}
        </div>

        {/* Surface Switch Actions */}
        <div>
          {isDesktop && isIde ? (
            <span className="text-xs text-brand-green font-bold flex items-center gap-1">
              ✓ Active on Both Surfaces
            </span>
          ) : isDesktop && !isIde ? (
            <div className="inline-flex items-center gap-1.5 p-1 rounded-lg bg-white/5 border border-surface-border">
              <span className="text-[11px] font-bold text-green-300 px-1.5">📱 App Active</span>
              <button
                onClick={() => onSwitchAccount(email, 'ide')}
                title="Switch IDE to this account (Desktop App remains active)"
                className="text-[11px] font-semibold px-2.5 py-1 rounded bg-brand-blue/20 hover:bg-brand-blue/30 text-blue-300 border border-brand-blue/40 transition-all"
              >
                💻 Switch IDE Here
              </button>
            </div>
          ) : !isDesktop && isIde ? (
            <div className="inline-flex items-center gap-1.5 p-1 rounded-lg bg-white/5 border border-surface-border">
              <span className="text-[11px] font-bold text-blue-300 px-1.5">💻 IDE Active</span>
              <button
                onClick={() => onSwitchAccount(email, 'desktop')}
                title="Switch Desktop App to this account (IDE remains active)"
                className="text-[11px] font-semibold px-2.5 py-1 rounded bg-brand-blue/20 hover:bg-brand-blue/30 text-blue-300 border border-brand-blue/40 transition-all"
              >
                📱 Switch App Here
              </button>
            </div>
          ) : (
            <div className="inline-flex items-center gap-1 p-1 rounded-lg bg-white/5 border border-surface-border">
              <button
                onClick={() => onSwitchAccount(email, 'both')}
                title={`Switch both surfaces to ${email}`}
                className="text-[11px] font-bold px-2.5 py-1 rounded bg-gradient-to-r from-brand-blue to-brand-purple text-white hover:opacity-90 transition-all shadow-sm"
              >
                ⚡ Both
              </button>
              <button
                onClick={() => onSwitchAccount(email, 'desktop')}
                title="Switch Desktop App only"
                className="text-[11px] font-semibold px-2 py-1 rounded bg-white/5 hover:bg-brand-blue/20 text-gray-300 hover:text-white border border-transparent hover:border-brand-blue/40 transition-all"
              >
                📱 App
              </button>
              <button
                onClick={() => onSwitchAccount(email, 'ide')}
                title="Switch IDE only"
                className="text-[11px] font-semibold px-2 py-1 rounded bg-white/5 hover:bg-brand-blue/20 text-gray-300 hover:text-white border border-transparent hover:border-brand-blue/40 transition-all"
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
          <div className="col-span-full py-6 text-center text-xs text-gray-400 italic">
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
            <div key={g.displayName} className="p-4 rounded-xl border border-surface-border bg-surface-card flex flex-col justify-between">
              <div>
                <div className="flex items-center justify-between mb-3">
                  <span className="font-bold text-sm text-gray-200">{g.displayName}</span>
                  {w && getStatusBadge(wPct)}
                </div>

                {/* Weekly Limit Meter */}
                <div className="flex items-center justify-between text-xs text-gray-400 mb-1.5">
                  <span>Weekly Quota Limit</span>
                  <strong className="font-mono text-gray-100">{wPct.toFixed(1)}%</strong>
                </div>
                <div className="w-full h-2.5 bg-gray-900 rounded-full overflow-hidden mb-3 border border-white/5">
                  <div
                    className="h-full rounded-full transition-all duration-500"
                    style={{ width: `${Math.min(100, Math.max(0, wPct))}%`, backgroundColor: getBarColor(wPct) }}
                  />
                </div>

                {/* High-Contrast Weekly Reset Countdown Box */}
                <div className="p-3 rounded-lg border border-surface-border-light bg-black/40 mb-3 space-y-1">
                  <div className="flex items-center justify-between text-[10px] font-mono uppercase tracking-wider text-gray-400">
                    <span>Weekly Refill In</span>
                    <span className={isUrgent ? 'text-green-400 font-bold' : ''}>
                      {isUrgent ? '🔥 SOON' : 'WEEKLY CYCLE'}
                    </span>
                  </div>
                  <div className={`font-mono text-base font-bold tracking-tight ${isUrgent ? 'text-green-300' : 'text-blue-300'}`}>
                    ⏱️ {wCountdownStr}
                  </div>
                  <div className="flex items-center justify-between text-[11px] font-mono text-gray-400 pt-0.5">
                    <span>Local: {wReset ? wReset.local : 'N/A'}</span>
                    <span className="opacity-60">{wReset ? wReset.utc : ''}</span>
                  </div>
                </div>
              </div>

              {/* 5-Hour Rolling Limit */}
              <div className="pt-3 border-t border-white/5">
                <div className="flex items-center justify-between text-xs text-gray-400 mb-1">
                  <span>5-Hour Rolling Limit</span>
                  <strong className="font-mono text-gray-100">{fPct.toFixed(1)}%</strong>
                </div>
                <div className="w-full h-2 bg-gray-900 rounded-full overflow-hidden mb-2 border border-white/5">
                  <div
                    className="h-full rounded-full transition-all duration-500"
                    style={{ width: `${Math.min(100, Math.max(0, fPct))}%`, backgroundColor: getBarColor(fPct) }}
                  />
                </div>
                <div className="flex items-center justify-between text-[11px] font-mono text-gray-400">
                  <span>5h Reset In: <strong className="text-blue-300">{fCountdownStr}</strong></span>
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
