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
            className={`px-3 py-1.5 rounded-full text-xs font-semibold border transition-all ${
              currentFilter === c.id
                ? 'border-brand-blue bg-surface-elevated text-white'
                : 'border-surface-border bg-surface text-gray-400 hover:text-gray-200 hover:border-surface-border-light'
            }`}
          >
            {c.label}
          </button>
        ))}
      </div>

      <div className="relative">
        <Search className="w-3.5 h-3.5 text-gray-400 absolute left-3 top-1/2 -translate-y-1/2" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Search email or name..."
          className="bg-surface border border-surface-border text-gray-100 placeholder-gray-500 text-xs rounded-full pl-8 pr-3.5 py-1.5 outline-none focus:border-brand-blue w-52 focus:w-64 transition-all"
        />
      </div>
    </div>
  );
}
