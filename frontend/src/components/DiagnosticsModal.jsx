import React, { useEffect, useState } from 'react';
import { X, CheckCircle, AlertTriangle, AlertCircle, RefreshCw } from 'lucide-react';

export default function DiagnosticsModal({ isOpen, onClose }) {
  const [loading, setLoading] = useState(true);
  const [diag, setDiag] = useState(null);
  const [error, setError] = useState(null);

  useEffect(() => {
    if (!isOpen) return;
    runAudit();
  }, [isOpen]);

  async function runAudit() {
    setLoading(true);
    setError(null);
    try {
      const resp = await fetch('/api/doctor?validate=true');
      const data = await resp.json();
      setDiag(data);
    } catch (e) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  }

  if (!isOpen) return null;

  const sections = [
    { key: 'system_checks', title: '🖥️ System Environment & D-Bus Services' },
    { key: 'app_checks', title: '🛡️ App Configuration & Network Security' },
    { key: 'account_checks', title: '🔑 Accounts Integrity & Token Validity' }
  ];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-900/60 dark:bg-black/80 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="w-full max-w-2xl max-h-[85vh] bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-6 shadow-2xl overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Header */}
        <div className="flex items-center justify-between pb-4 mb-4 border-b border-slate-200 dark:border-slate-800">
          <div className="flex items-center gap-2.5">
            <span className="text-2xl">🩺</span>
            <div>
              <h2 className="text-base font-bold text-slate-900 dark:text-white">System Diagnostics & Health Check</h2>
              <p className="text-xs text-slate-500 dark:text-slate-400">Auditing Keyring, D-Bus, Daemon, Network, and SQLite</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-white hover:bg-slate-100 dark:hover:bg-slate-800 transition-all"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        {loading && (
          <div className="py-12 text-center text-xs text-slate-500 dark:text-slate-400 flex flex-col items-center gap-3">
            <RefreshCw className="w-6 h-6 animate-spin text-blue-500" />
            <span>Running system diagnostics...</span>
          </div>
        )}

        {error && (
          <div className="p-4 rounded-xl bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-900 text-rose-700 dark:text-rose-300 text-xs">
            Diagnostic failed: {error}
          </div>
        )}

        {!loading && diag && (
          <div className="space-y-5">
            {diag.overall_health && (
              <div className="flex items-center justify-between p-3 rounded-xl bg-slate-50 dark:bg-slate-800/40 border border-slate-200 dark:border-slate-700">
                <span className="text-xs font-semibold text-slate-700 dark:text-slate-300">Overall System Health:</span>
                <span className="text-xs font-bold font-mono px-2.5 py-0.5 rounded-full bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
                  {diag.overall_health}
                </span>
              </div>
            )}

            {sections.map((sec) => {
              const items = diag[sec.key];
              if (!items || items.length === 0) return null;

              return (
                <div key={sec.key} className="space-y-2">
                  <h3 className="text-xs font-bold font-mono uppercase tracking-wider text-slate-500 dark:text-slate-400">
                    {sec.title}
                  </h3>
                  <div className="space-y-2">
                    {items.map((item, idx) => {
                      const status = item.status || 'INFO';
                      const isPass = status === 'PASS';
                      const isWarn = status === 'WARN';

                      return (
                        <div
                          key={idx}
                          className={`p-3 rounded-xl border flex items-start justify-between gap-3 ${
                            isPass
                              ? 'border-emerald-200 dark:border-emerald-900/60 bg-emerald-50/40 dark:bg-emerald-950/20'
                              : isWarn
                              ? 'border-amber-200 dark:border-amber-900/60 bg-amber-50/40 dark:bg-amber-950/20'
                              : 'border-rose-200 dark:border-rose-900/60 bg-rose-50/40 dark:bg-rose-950/20'
                          }`}
                        >
                          <div className="flex items-start gap-2.5">
                            {isPass ? (
                              <CheckCircle className="w-4 h-4 text-emerald-600 dark:text-emerald-400 shrink-0 mt-0.5" />
                            ) : isWarn ? (
                              <AlertTriangle className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" />
                            ) : (
                              <AlertCircle className="w-4 h-4 text-rose-600 dark:text-rose-400 shrink-0 mt-0.5" />
                            )}
                            <div>
                              <div className="text-xs font-bold text-slate-900 dark:text-white">
                                {item.component || item.check || 'Item'}
                              </div>
                              <div className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5 font-mono">
                                {item.details || item.message || ''}
                              </div>
                            </div>
                          </div>
                          <span
                            className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded ${
                              isPass
                                ? 'bg-emerald-100 dark:bg-emerald-900/50 text-emerald-700 dark:text-emerald-300'
                                : isWarn
                                ? 'bg-amber-100 dark:bg-amber-900/50 text-amber-700 dark:text-amber-300'
                                : 'bg-rose-100 dark:bg-rose-900/50 text-rose-700 dark:text-rose-300'
                            }`}
                          >
                            {status}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
