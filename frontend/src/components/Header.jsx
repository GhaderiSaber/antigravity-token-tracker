import React from 'react';
import { Shield, RefreshCw, Zap, Stethoscope, Globe, Clock, Sun, Moon } from 'lucide-react';

export default function Header({
  data,
  isRefreshing,
  onRefresh,
  onSwitchBest,
  onToggleShield,
  onToggleBalancer,
  onOpenDiagnostics,
  lastSyncedTime,
  syncInterval,
  onIntervalChange,
  now,
  theme,
  onToggleTheme
}) {
  const geo = data?.geo;
  const shield = data?.shield;
  const balancer = data?.balancer;

  const isShieldOn = Boolean(shield?.enabled);
  const isBalOn = Boolean(balancer?.enabled);
  const stratName = (balancer?.strategy || 'watermark').replace('_', ' ').toUpperCase();

  const ipStr = geo?.ip || 'Checking...';
  const flag = geo?.flag || '🌐';
  const location = geo?.city ? `${geo.city}, ${geo.country_code}` : (geo?.country_name || '');
  const isRestricted = Boolean(geo?.is_restricted);

  const diffSec = Math.floor((Date.now() - (lastSyncedTime || Date.now())) / 1000);
  const syncedText = diffSec < 5 ? 'Synced: just now' : `Synced: ${diffSec}s ago`;

  return (
    <header className="mb-6 pb-4 border-b border-slate-200 dark:border-slate-800/80 transition-colors">
      <div className="flex flex-wrap items-center justify-between gap-4">
        {/* Brand */}
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 bg-gradient-to-br from-blue-600 to-indigo-600 text-white rounded-xl flex items-center justify-center text-xl shadow-md shadow-blue-500/20">
            ⚡
          </div>
          <div>
            <h1 className="text-xl font-extrabold tracking-tight text-slate-900 dark:text-white flex items-center gap-2">
              Antigravity Token & Quota Commander
            </h1>
            <p className="text-xs text-slate-500 dark:text-slate-400 font-mono">
              Weekly Account Token & Quota Lifecycle Monitor
            </p>
          </div>
        </div>

        {/* Controls */}
        <div className="flex flex-wrap items-center gap-2.5">
          {/* IP Egress Badge */}
          <button
            onClick={() => window.open('https://ipwho.is', '_blank')}
            title={`Egress: ${ipStr} | ${geo?.country_name || ''} (${geo?.isp || 'Direct'})\nClick to test IP`}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-mono font-medium border transition-all ${
              isRestricted
                ? 'border-rose-400 text-rose-700 bg-rose-50 dark:bg-rose-950/40 dark:text-rose-300 animate-pulse'
                : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 text-slate-700 dark:text-slate-300 hover:border-blue-400 shadow-sm'
            }`}
          >
            <span>{flag}</span>
            <span className="font-semibold">{ipStr}</span>
            {location && <span className="opacity-70 text-[11px]">({location})</span>}
          </button>

          {/* Shield Toggle */}
          <button
            onClick={onToggleShield}
            title={isShieldOn ? 'IP Shield ACTIVE: Click to toggle.' : 'IP Shield OFF: Click to enable.'}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-mono font-semibold border transition-all shadow-sm ${
              isShieldOn
                ? 'border-blue-200 dark:border-blue-900 bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300 hover:border-blue-400'
                : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 text-slate-400 hover:text-slate-700'
            }`}
          >
            <Shield className="w-3.5 h-3.5 text-blue-500" />
            <span>Shield: {isShieldOn ? 'ACTIVE' : 'OFF'}</span>
          </button>

          {/* Balancer Toggle */}
          <button
            onClick={onToggleBalancer}
            title={isBalOn ? `Auto-Balancer: ${stratName}. Click to toggle.` : 'Auto-Balancer OFF.'}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-mono font-semibold border transition-all shadow-sm ${
              isBalOn
                ? 'border-purple-200 dark:border-purple-900 bg-purple-50 dark:bg-purple-950/40 text-purple-700 dark:text-purple-300 hover:border-purple-400'
                : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 text-slate-400 hover:text-slate-700'
            }`}
          >
            <span className="text-sm">🔄</span>
            <span>Balancer: {isBalOn ? stratName : 'OFF'}</span>
          </button>

          {/* Diagnostics Modal Button */}
          <button
            onClick={onOpenDiagnostics}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 text-slate-700 dark:text-slate-300 hover:border-slate-300 dark:hover:border-slate-700 shadow-sm transition-all"
          >
            <Stethoscope className="w-3.5 h-3.5 text-purple-500" />
            <span>Diagnostics</span>
          </button>

          {/* Theme Switcher Button */}
          <button
            onClick={onToggleTheme}
            title={`Switch to ${theme === 'dark' ? 'Light' : 'Dark'} Mode`}
            className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-semibold bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 text-slate-700 dark:text-slate-300 hover:border-slate-300 dark:hover:border-slate-700 shadow-sm transition-all"
          >
            {theme === 'dark' ? (
              <Sun className="w-3.5 h-3.5 text-amber-400" />
            ) : (
              <Moon className="w-3.5 h-3.5 text-indigo-500" />
            )}
            <span className="hidden sm:inline">{theme === 'dark' ? 'Light' : 'Dark'}</span>
          </button>

          {/* Refresh Button */}
          <button
            onClick={onRefresh}
            disabled={isRefreshing}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-mono font-semibold bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 text-slate-700 dark:text-slate-300 hover:border-blue-400 shadow-sm transition-all disabled:opacity-50"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isRefreshing ? 'animate-spin text-blue-500' : ''}`} />
            <span>Refresh</span>
          </button>

          {/* Switch Best Button */}
          <button
            onClick={onSwitchBest}
            className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-bold bg-gradient-to-r from-blue-600 to-indigo-600 text-white shadow-md shadow-blue-500/20 hover:opacity-95 transition-all active:scale-95"
          >
            <Zap className="w-3.5 h-3.5 fill-current" />
            <span>Auto-Switch Best</span>
          </button>
        </div>
      </div>

      {/* Sub-bar with Live Clocks and Interval Control */}
      <div className="flex flex-wrap items-center justify-between gap-4 mt-3 pt-3 text-xs text-slate-500 dark:text-slate-400 font-mono">
        <div className="flex items-center gap-4">
          <span className="flex items-center gap-1.5">
            <Clock className="w-3.5 h-3.5 text-slate-400" />
            Local: <strong className="text-slate-700 dark:text-slate-200">{now.toLocaleTimeString()}</strong>
          </span>
          <span className="flex items-center gap-1.5">
            <Globe className="w-3.5 h-3.5 text-slate-400" />
            UTC: <strong className="text-slate-700 dark:text-slate-200">{now.toISOString().substring(11, 19)}</strong>
          </span>
        </div>

        <div className="flex items-center gap-3">
          <span>{syncedText}</span>
          <label className="flex items-center gap-1.5">
            <span>Auto-Sync:</span>
            <select
              value={syncInterval}
              onChange={(e) => onIntervalChange(Number(e.target.value))}
              className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 text-slate-700 dark:text-slate-300 text-xs rounded px-2 py-0.5 outline-none focus:border-blue-500 shadow-sm"
            >
              <option value={60}>1m</option>
              <option value={120}>2m</option>
              <option value={300}>5m (Safe)</option>
              <option value={600}>10m (Ultra-Safe)</option>
              <option value={0}>Off</option>
            </select>
          </label>
        </div>
      </div>
    </header>
  );
}
