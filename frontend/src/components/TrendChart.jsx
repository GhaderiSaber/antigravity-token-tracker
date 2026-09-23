import React from 'react';
import { TrendingUp } from 'lucide-react';

export default function TrendChart({ snapshots }) {
  if (!snapshots || snapshots.length < 2) return null;

  const width = 1000;
  const height = 140;

  const minTs = snapshots[0].timestamp;
  const maxTs = snapshots[snapshots.length - 1].timestamp;
  const tsSpan = Math.max(1, maxTs - minTs);

  const emails = new Set();
  snapshots.forEach((s) => {
    if (s.accounts) Object.keys(s.accounts).forEach((e) => emails.add(e));
  });

  const colors = ['#4285f4', '#34a853', '#fbbc04', '#9b51e0', '#ff6d00', '#00b0ff'];
  let colorIdx = 0;

  const lines = [];
  for (const email of emails) {
    const points = [];
    for (const s of snapshots) {
      const acc = s.accounts && s.accounts[email];
      if (!acc || !acc.groups) continue;
      let pct = null;
      for (const [gName, gInfo] of Object.entries(acc.groups)) {
        if (gName.toLowerCase().includes('gemini') && gInfo.weekly_pct !== null && gInfo.weekly_pct !== undefined) {
          pct = gInfo.weekly_pct;
          break;
        }
      }
      if (pct !== null) {
        const x = ((s.timestamp - minTs) / tsSpan) * (width - 40) + 20;
        const y = (height - 15) - (pct / 100) * (height - 30);
        points.push(`${x.toFixed(1)},${y.toFixed(1)}`);
      }
    }

    if (points.length >= 2) {
      const color = colors[colorIdx % colors.length];
      colorIdx++;
      lines.push({ email, stroke: color, points: points.join(' ') });
    }
  }

  return (
    <section className="mb-6 p-4 rounded-xl border border-surface-border bg-surface shadow-sm">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2 font-bold text-sm text-gray-200">
          <TrendingUp className="w-4 h-4 text-brand-blue" />
          <span>24-Hour Quota Burn Rate Trend</span>
        </div>
        <span className="text-xs text-gray-400 font-mono">
          {snapshots.length} snapshots recorded
        </span>
      </div>

      <div className="w-full overflow-hidden">
        <svg viewBox={`0 0 ${width} ${height}`} className="w-full h-28 sm:h-32">
          {/* Grid lines */}
          <line x1="0" y1={height - 15} x2={width} y2={height - 15} stroke="#232836" strokeWidth="1" />
          <line x1="0" y1={height / 2} x2={width} y2={height / 2} stroke="#232836" strokeDasharray="4" strokeWidth="1" />
          <line x1="0" y1="15" x2={width} y2="15" stroke="#232836" strokeDasharray="4" strokeWidth="1" />

          {/* Account Polylines */}
          {lines.map((l) => (
            <polyline
              key={l.email}
              fill="none"
              stroke={l.stroke}
              strokeWidth="2.5"
              strokeLinejoin="round"
              points={l.points}
              opacity="0.85"
            />
          ))}
        </svg>
      </div>

      {/* Legend */}
      <div className="flex flex-wrap items-center gap-3 mt-2 pt-2 border-t border-white/5 text-[11px] font-mono text-gray-400">
        {lines.map((l) => (
          <div key={l.email} className="flex items-center gap-1.5">
            <span className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: l.stroke }} />
            <span>{l.email.split('@')[0]}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
