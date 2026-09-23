import React, { useState, useEffect, useMemo } from 'react';
import Header from './components/Header';
import SurfaceSyncBanner from './components/SurfaceSyncBanner';
import RefillTimeline from './components/RefillTimeline';
import RunoutBanner from './components/RunoutBanner';
import TrendChart from './components/TrendChart';
import BalancerCard from './components/BalancerCard';
import FilterBar from './components/FilterBar';
import AccountCard from './components/AccountCard';
import DiagnosticsModal from './components/DiagnosticsModal';
import { useLiveTicker, parseResetTimestamp } from './hooks/useLiveTicker';

export default function App() {
  const [data, setData] = useState(null);
  const [history, setHistory] = useState([]);
  const [loading, setLoading] = useState(true);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastSyncedTime, setLastSyncedTime] = useState(Date.now());
  const [syncInterval, setSyncInterval] = useState(300);

  const [currentFilter, setCurrentFilter] = useState('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [isDiagnosticsOpen, setIsDiagnosticsOpen] = useState(false);

  const now = useLiveTicker();

  // Load quota data
  async function loadData(force = false) {
    if (force) setIsRefreshing(true);
    try {
      const url = force ? '/api/quota?force=true' : '/api/quota';
      const resp = await fetch(url);
      const res = await resp.json();
      setData(res);
      setLastSyncedTime(Date.now());
      loadHistory();
    } catch (e) {
      console.error("Failed to load quota data:", e);
    } finally {
      setLoading(false);
      if (force) setIsRefreshing(false);
    }
  }

  // Load 24-hr historical snapshots
  async function loadHistory() {
    try {
      const resp = await fetch('/api/history?hours=24');
      const res = await resp.json();
      if (res.snapshots) {
        setHistory(res.snapshots);
      }
    } catch (e) {
      console.error("Failed to load history:", e);
    }
  }

  useEffect(() => {
    loadData(false);
  }, []);

  // Auto-sync polling
  useEffect(() => {
    if (syncInterval <= 0) return;
    const timer = setInterval(() => {
      loadData(false);
    }, syncInterval * 1000);
    return () => clearInterval(timer);
  }, [syncInterval]);

  // Actions
  async function handleSwitchAccount(email, surface = 'both') {
    let surfaceName = "App + IDE";
    if (surface === 'desktop' || surface === 'app') surfaceName = "Desktop App only";
    else if (surface === 'ide') surfaceName = "Antigravity IDE only";

    if (!confirm(`Switch ${surfaceName} to ${email}?`)) return;
    try {
      const resp = await fetch(`/api/switch?email=${encodeURIComponent(email)}&surface=${encodeURIComponent(surface)}`);
      const res = await resp.json();
      if (res.success) {
        alert(`✓ ${res.message || 'Switched successfully!'}`);
        loadData(true);
      } else {
        alert(`Switch failed: ${res.message || res.error}`);
      }
    } catch (e) {
      alert(`Error: ${e.message}`);
    }
  }

  async function handleSwitchBest() {
    if (!confirm("Auto-switch to the healthiest backup account?")) return;
    try {
      const resp = await fetch('/api/switch-best');
      const res = await resp.json();
      if (res.success) {
        alert(`✓ Auto-switched to ${res.target}!\n${res.message}`);
        loadData(true);
      } else {
        alert(`Auto-switch failed: ${res.error || res.message}`);
      }
    } catch (e) {
      alert(`Error: ${e.message}`);
    }
  }

  async function handleSyncSurfaces(target) {
    const name = target === 'ide' ? 'IDE to Desktop App' : 'Desktop App to IDE';
    if (!confirm(`Align ${name}?`)) return;
    try {
      const resp = await fetch(`/api/sync-surfaces?target=${encodeURIComponent(target)}`);
      const res = await resp.json();
      if (res.success) {
        alert(`✓ ${res.message}`);
        loadData(true);
      } else {
        alert(`Alignment notice: ${res.message}`);
      }
    } catch (e) {
      alert(`Alignment error: ${e.message}`);
    }
  }

  async function handleToggleShield() {
    try {
      await fetch('/api/shield', { method: 'POST' });
      loadData(false);
    } catch (e) {
      alert('Failed to toggle shield: ' + e.message);
    }
  }

  async function handleToggleBalancer() {
    try {
      const currentResp = await fetch('/api/balancer');
      const currentData = await currentResp.json();
      const newState = !Boolean(currentData.config?.enabled);
      await fetch('/api/balancer', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: newState })
      });
      loadData(false);
    } catch (e) {
      alert('Failed to toggle balancer: ' + e.message);
    }
  }

  async function handleUpdateBalancerStrategy(strategy) {
    try {
      await fetch('/api/balancer', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ strategy })
      });
      loadData(false);
    } catch (e) {
      alert('Failed to update strategy: ' + e.message);
    }
  }

  async function handleRotateBalancer() {
    try {
      const resp = await fetch('/api/balancer/rotate', { method: 'POST' });
      const res = await resp.json();
      alert(res.triggered ? `✓ ${res.message}` : `Notice: ${res.message}`);
      loadData(false);
    } catch (e) {
      alert('Rotation error: ' + e.message);
    }
  }

  // Filter calculations
  const accounts = data?.accounts || {};
  const counts = useMemo(() => {
    let all = 0, active = 0, healthy = 0, resetting = 0, depleted = 0;
    for (const [_, acc] of Object.entries(accounts)) {
      all++;
      const isAct = acc.is_current_desktop_session || acc.is_current_ide_session;
      if (isAct) active++;

      let geminiPct = 0;
      let isResetting = false;
      if (acc.groups) {
        for (const g of acc.groups) {
          if (g.displayName?.toLowerCase().includes('gemini') && g.weekly) {
            geminiPct = g.weekly.remainingPercent || 0;
            if (g.weekly.resetTime) {
              const p = parseResetTimestamp(g.weekly.resetTime, now);
              if (p.diffSecs < 86400) isResetting = true;
            }
          }
        }
      }

      if (geminiPct > 20.0) healthy++;
      if (geminiPct <= 1.0) depleted++;
      if (isResetting) resetting++;
    }
    return { all, active, healthy, resetting, depleted };
  }, [accounts, now]);

  // Filtered accounts list
  const filteredAccounts = useMemo(() => {
    const list = Object.entries(accounts).map(([email, acc]) => ({ email, ...acc }));

    // Priority sort: Active first, then by remaining quota
    list.sort((a, b) => {
      const getPrio = (acc) => {
        if (acc.is_current_desktop_session && acc.is_current_ide_session) return 0;
        if (acc.is_current_desktop_session) return 1;
        if (acc.is_current_ide_session) return 2;
        return 3;
      };
      const pA = getPrio(a);
      const pB = getPrio(b);
      if (pA !== pB) return pA - pB;
      const remA = a.groups?.[0]?.weekly?.remainingPercent || 0;
      const remB = b.groups?.[0]?.weekly?.remainingPercent || 0;
      return remB - remA;
    });

    return list.filter((acc) => {
      const isActive = acc.is_current_desktop_session || acc.is_current_ide_session;
      let geminiPct = 0;
      let isResettingSoon = false;
      if (acc.groups) {
        for (const g of acc.groups) {
          if (g.displayName?.toLowerCase().includes('gemini') && g.weekly) {
            geminiPct = g.weekly.remainingPercent || 0;
            if (g.weekly.resetTime) {
              const p = parseResetTimestamp(g.weekly.resetTime, now);
              if (p.diffSecs < 86400) isResettingSoon = true;
            }
          }
        }
      }

      // Filter tabs
      if (currentFilter === 'active' && !isActive) return false;
      if (currentFilter === 'healthy' && geminiPct <= 20.0) return false;
      if (currentFilter === 'resetting' && !isResettingSoon) return false;
      if (currentFilter === 'depleted' && geminiPct > 1.0) return false;

      // Search query
      if (searchQuery) {
        const q = searchQuery.toLowerCase().trim();
        const matchEmail = (acc.email || '').toLowerCase().includes(q);
        const matchName = (acc.name || '').toLowerCase().includes(q);
        if (!matchEmail && !matchName) return false;
      }

      return true;
    });
  }, [accounts, currentFilter, searchQuery, now]);

  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 py-6 font-sans">
      {/* Header */}
      <Header
        data={data}
        isRefreshing={isRefreshing}
        onRefresh={() => loadData(true)}
        onSwitchBest={handleSwitchBest}
        onToggleShield={handleToggleShield}
        onToggleBalancer={handleToggleBalancer}
        onOpenDiagnostics={() => setIsDiagnosticsOpen(true)}
        lastSyncedTime={lastSyncedTime}
        syncInterval={syncInterval}
        onIntervalChange={setSyncInterval}
        now={now}
      />

      {/* Surface Alignment Banner */}
      <SurfaceSyncBanner
        activeSessions={data?.active_sessions}
        onSyncSurfaces={handleSyncSurfaces}
      />

      {/* Refill Timeline */}
      <RefillTimeline accounts={accounts} now={now} />

      {/* Runout Banner */}
      <RunoutBanner burnrate={data?.burnrate} />

      {/* 24-hr Trend Chart */}
      <TrendChart snapshots={history} />

      {/* Balancer Controls */}
      <BalancerCard
        balancer={data?.balancer}
        onUpdateStrategy={handleUpdateBalancerStrategy}
        onRotate={handleRotateBalancer}
      />

      {/* Smart Recommendations */}
      {data?.recommendation && (
        <div className="mb-6 p-4 rounded-xl border border-brand-yellow/30 bg-brand-yellow/10 text-yellow-300 text-xs flex items-center gap-2.5">
          <span>💡</span>
          <div>{data.recommendation}</div>
        </div>
      )}

      {/* Filter and Search Bar */}
      <FilterBar
        currentFilter={currentFilter}
        onFilterChange={setCurrentFilter}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        counts={counts}
      />

      {/* Account Cards Grid */}
      {loading ? (
        <div className="py-20 text-center text-gray-400 font-mono text-sm animate-pulse">
          ⚡ Loading account quota lifecycle...
        </div>
      ) : filteredAccounts.length === 0 ? (
        <div className="py-20 text-center text-gray-400 text-sm italic">
          No accounts matched the current filter or search query.
        </div>
      ) : (
        <div className="space-y-5">
          {filteredAccounts.map((acc) => (
            <AccountCard
              key={acc.email}
              account={acc}
              now={now}
              onSwitchAccount={handleSwitchAccount}
            />
          ))}
        </div>
      )}

      {/* Diagnostics Modal */}
      <DiagnosticsModal
        isOpen={isDiagnosticsOpen}
        onClose={() => setIsDiagnosticsOpen(false)}
      />
    </div>
  );
}
