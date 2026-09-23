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
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="w-full max-w-2xl max-h-[85vh] bg-surface border border-surface-border-light rounded-2xl p-6 shadow-2xl overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Header */}
        <div className="flex items-center justify-between pb-4 mb-4 border-b border-surface-border">
          <div className="flex items-center gap-2.5">
            <span className="text-2xl">🩺</span>
            <div>
              <h2 className="text-base font-bold text-white">System Diagnostics & Health Check</h2>
              <p className="text-xs text-gray-400">Auditing Keyring, D-Bus, Daemon, Network, and SQLite</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-lg text-gray-400 hover:text-white hover:bg-white/10 transition-all"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        {loading && (
          <div className="py-12 text-center text-xs text-gray-400 flex flex-col items-center gap-3">
            <RefreshCw className="w-6 h-6 animate-spin text-brand-blue" />
            <span>Running system diagnostics...</span>
          </div>
        )}

        {error && (
          <div className="p-4 rounded-xl bg-brand-red/15 border border-brand-red/30 text-red-300 text-xs">
            Diagnostic failed: {error}
          </div>
        )}

        {!loading && diag && (
          <div className="space-y-5">
            {diag.overall_health && (
              <div className="flex items-center justify-between p-3 rounded-xl bg-white/5 border border-surface-border">
                <span className="text-xs font-semibold text-gray-300">Overall System Health:</span>
                <span className="text-xs font-bold font-mono px-2.5 py-0.5 rounded-full bg-brand-green/20 text-green-300 border border-brand-green/40">
                  {diag.overall_health}
                </span>
              </div>
            )}

            {sections.map((sec) => {
              const items = diag[sec.key];
              if (!items || items.length === 0) return null;

              return (
                <div key={sec.key} className="space-y-2">
                  <h3 className="text-xs font-bold font-mono uppercase tracking-wider text-gray-400">
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
                              ? 'border-brand-green/20 bg-brand-green/[0.04]'
                              : isWarn
                              ? 'border-brand-yellow/30 bg-brand-yellow/[0.05]'
                              : 'border-brand-red/30 bg-brand-red/[0.05]'
                          }`}
                        >
                          <div className="flex items-start gap-2.5">
                            {isPass ? (
                              <CheckCircle className="w-4 h-4 text-brand-green shrink-0 mt-0.5" />
                            ) : isWarn ? (
                              <AlertTriangle className="w-4 h-4 text-brand-yellow shrink-0 mt-0.5" />
                            ) : (
                              <AlertCircle className="w-4 h-4 text-brand-red shrink-0 mt-0.5" />
                            )}
                            <div>
                              <div className="text-xs font-bold text-gray-200">
                                {item.component || item.check || 'Item'}
                              </div>
                              <div className="text-[11px] text-gray-400 mt-0.5 font-mono">
                                {item.details || item.message || ''}
                              </div>
                            </div>
                          </div>
                          <span
                            className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded ${
                              isPass
                                ? 'bg-brand-green/20 text-green-300'
                                : isWarn
                                ? 'bg-brand-yellow/20 text-yellow-300'
                                : 'bg-brand-red/20 text-red-300'
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
