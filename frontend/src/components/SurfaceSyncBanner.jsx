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
    <div className="mb-6 p-4 rounded-xl border border-blue-200 dark:border-blue-900/60 bg-blue-50/70 dark:bg-blue-950/30 flex flex-wrap items-center justify-between gap-3 shadow-sm transition-colors">
      <div className="flex flex-wrap items-center gap-2.5 text-xs font-semibold text-blue-900 dark:text-blue-200">
        <Layers className="w-4 h-4 text-blue-600 dark:text-blue-400" />
        <span>Split Surface Sessions:</span>
        <span className="text-emerald-700 dark:text-emerald-400">
          📱 Desktop App: <strong>{desktop.email}</strong> ({desktop.quota_pct?.toFixed(1)}%)
        </span>
        <span className="text-slate-400">|</span>
        <span className="text-blue-700 dark:text-blue-300">
          💻 IDE: <strong>{ide.email}</strong> ({ide.quota_pct?.toFixed(1)}%)
        </span>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={() => onSyncSurfaces('ide')}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-white dark:bg-slate-900 hover:bg-blue-50 dark:hover:bg-blue-950 border border-blue-200 dark:border-blue-800 text-blue-700 dark:text-blue-300 shadow-sm transition-all"
        >
          <ArrowRightLeft className="w-3 h-3 text-blue-500" />
          <span>Align IDE to App ({desktop.email.split('@')[0]})</span>
        </button>
        <button
          onClick={() => onSyncSurfaces('desktop')}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-white dark:bg-slate-900 hover:bg-purple-50 dark:hover:bg-purple-950 border border-purple-200 dark:border-purple-800 text-purple-700 dark:text-purple-300 shadow-sm transition-all"
        >
          <ArrowRightLeft className="w-3 h-3 text-purple-500" />
          <span>Align App to IDE ({ide.email.split('@')[0]})</span>
        </button>
      </div>
    </div>
  );
}
