import React from 'react';
import { Shield, RefreshCw, Zap, Stethoscope, Globe, Clock } from 'lucide-react';

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
  now
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
    <header className="mb-6 pb-4 border-b border-surface-border">
      <div className="flex flex-wrap items-center justify-between gap-4">
        {/* Brand */}
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 bg-gradient-to-br from-brand-blue to-brand-purple rounded-xl flex items-center justify-center text-xl shadow-lg shadow-brand-blue/20">
            ⚡
          </div>
          <div>
            <h1 className="text-xl font-extrabold tracking-tight text-white flex items-center gap-2">
              Antigravity Token & Quota Commander
            </h1>
            <p className="text-xs text-gray-400 font-mono">
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
                ? 'border-brand-red/60 text-red-300 bg-brand-red/20 animate-pulse'
                : 'border-brand-green/40 text-green-300 bg-brand-green/10 hover:border-brand-blue'
            }`}
          >
            <span>{flag}</span>
            <span>{ipStr}</span>
            {location && <span className="opacity-70">({location})</span>}
          </button>

          {/* Shield Toggle */}
          <button
            onClick={onToggleShield}
            title={isShieldOn ? 'IP Shield ACTIVE: Click to toggle.' : 'IP Shield OFF: Click to enable.'}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-mono font-semibold border transition-all ${
              isShieldOn
                ? 'border-brand-blue/50 text-blue-300 bg-brand-blue/15 hover:border-brand-blue'
                : 'border-gray-700 text-gray-400 bg-white/5 opacity-70 hover:opacity-100'
            }`}
          >
            <Shield className="w-3.5 h-3.5" />
            <span>Shield: {isShieldOn ? 'ACTIVE' : 'OFF'}</span>
          </button>

          {/* Balancer Toggle */}
          <button
            onClick={onToggleBalancer}
            title={isBalOn ? `Auto-Balancer: ${stratName}. Click to toggle.` : 'Auto-Balancer OFF.'}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-mono font-semibold border transition-all ${
              isBalOn
                ? 'border-brand-blue/50 text-blue-300 bg-brand-blue/15 hover:border-brand-blue'
                : 'border-gray-700 text-gray-400 bg-white/5 opacity-70 hover:opacity-100'
            }`}
          >
            <span className="text-sm">🔄</span>
            <span>Balancer: {isBalOn ? stratName : 'OFF'}</span>
          </button>

          {/* Diagnostics Modal Button */}
          <button
            onClick={onOpenDiagnostics}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-surface-card border border-surface-border hover:border-surface-border-light hover:bg-surface-elevated transition-all"
          >
            <Stethoscope className="w-3.5 h-3.5 text-brand-purple" />
            <span>Diagnostics</span>
          </button>

          {/* Refresh Button */}
          <button
            onClick={onRefresh}
            disabled={isRefreshing}
            className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-mono font-semibold bg-surface-elevated border border-surface-border hover:border-brand-blue transition-all disabled:opacity-50"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isRefreshing ? 'animate-spin text-brand-blue' : ''}`} />
            <span>Refresh</span>
          </button>

          {/* Switch Best Button */}
          <button
            onClick={onSwitchBest}
            className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-bold bg-gradient-to-r from-brand-blue to-brand-purple text-white shadow-lg shadow-brand-blue/20 hover:opacity-95 transition-all active:scale-95"
          >
            <Zap className="w-3.5 h-3.5 fill-current" />
            <span>Auto-Switch Best</span>
          </button>
        </div>
      </div>

      {/* Sub-bar with Live Clocks and Interval Control */}
      <div className="flex flex-wrap items-center justify-between gap-4 mt-3 pt-3 text-xs text-gray-400 font-mono">
        <div className="flex items-center gap-4">
          <span className="flex items-center gap-1.5">
            <Clock className="w-3.5 h-3.5 text-gray-500" />
            Local: <strong className="text-gray-200">{now.toLocaleTimeString()}</strong>
          </span>
          <span className="flex items-center gap-1.5">
            <Globe className="w-3.5 h-3.5 text-gray-500" />
            UTC: <strong className="text-gray-200">{now.toISOString().substring(11, 19)}</strong>
          </span>
        </div>

        <div className="flex items-center gap-3">
          <span>{syncedText}</span>
          <label className="flex items-center gap-1.5">
            <span>Auto-Sync:</span>
            <select
              value={syncInterval}
              onChange={(e) => onIntervalChange(Number(e.target.value))}
              className="bg-surface border border-surface-border text-gray-200 text-xs rounded px-2 py-0.5 outline-none focus:border-brand-blue"
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
