import React from 'react';
import { Search } from 'lucide-react';

export default function FilterBar({
  currentFilter,
  onFilterChange,
  searchQuery,
  onSearchChange,
  counts
}) {
  const chips = [
    { id: 'all', label: `All Accounts (${counts.all || 0})` },
    { id: 'active', label: `Active Sessions (${counts.active || 0})` },
    { id: 'healthy', label: `Ready (>20%) (${counts.healthy || 0})` },
    { id: 'resetting', label: `Resetting Soon (${counts.resetting || 0})` },
    { id: 'depleted', label: `Depleted (${counts.depleted || 0})` }
  ];

  return (
    <div className="flex flex-wrap items-center justify-between gap-3 mb-5">
      <div className="flex flex-wrap gap-2">
        {chips.map((c) => (
          <button
            key={c.id}
            onClick={() => onFilterChange(c.id)}
            className={`px-3 py-1.5 rounded-full text-xs font-semibold border transition-all shadow-sm ${
              currentFilter === c.id
                ? 'border-blue-500 bg-blue-500 text-white shadow-blue-500/20'
                : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white hover:border-slate-300 dark:hover:border-slate-700'
            }`}
          >
            {c.label}
          </button>
        ))}
      </div>

      <div className="relative">
        <Search className="w-3.5 h-3.5 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search email or name..."
          className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 text-slate-900 dark:text-white placeholder-slate-400 text-xs rounded-full pl-8 pr-3.5 py-1.5 outline-none focus:border-blue-500 w-52 focus:w-64 transition-all shadow-sm"
        />
      </div>
    </div>
  );
}
