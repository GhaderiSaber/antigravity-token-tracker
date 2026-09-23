import React from 'react';
import { Layers, ArrowRightLeft } from 'lucide-react';

export default function SurfaceSyncBanner({ activeSessions, onSyncSurfaces }) {
  if (!activeSessions) return null;
  const desktop = activeSessions.desktop;
  const ide = activeSessions.ide;

  if (!desktop || !ide || desktop.email?.toLowerCase() === ide.email?.toLowerCase()) {
    return null;
  }

  return (
    <div className="mb-5 p-3.5 rounded-xl border border-brand-blue/30 bg-gradient-to-r from-brand-blue/15 via-brand-purple/10 to-transparent flex flex-wrap items-center justify-between gap-3 shadow-sm">
      <div className="flex items-center gap-2.5 text-xs font-semibold text-blue-300">
        <Layers className="w-4 h-4 text-brand-blue" />
        <span>Split Surface Sessions:</span>
        <span className="text-green-400">
          📱 Desktop App: <strong>{desktop.email}</strong> ({desktop.quota_pct?.toFixed(1)}%)
        </span>
        <span className="text-gray-500">|</span>
        <span className="text-blue-300">
          💻 IDE: <strong>{ide.email}</strong> ({ide.quota_pct?.toFixed(1)}%)
        </span>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={() => onSyncSurfaces('ide')}
          className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-semibold bg-white/10 hover:bg-brand-blue/30 border border-white/15 hover:border-brand-blue text-white transition-all"
        >
          <ArrowRightLeft className="w-3 h-3 text-brand-blue" />
          <span>Align IDE to App ({desktop.email})</span>
        </button>
        <button
          onClick={() => onSyncSurfaces('desktop')}
          className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-semibold bg-white/10 hover:bg-brand-blue/30 border border-white/15 hover:border-brand-blue text-white transition-all"
        >
          <ArrowRightLeft className="w-3 h-3 text-brand-purple" />
          <span>Align App to IDE ({ide.email})</span>
        </button>
      </div>
    </div>
  );
}
