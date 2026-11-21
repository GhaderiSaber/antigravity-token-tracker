# Frontend UI Components

## Overview
Provides the modular React presentation and interaction components for the Antigravity Token Tracker web dashboard, including account quota cards, load balancer controls, timeline schedules, diagnostic modals, and status meters.

## Interface
- `AccountCard({ account, now, onSwitchAccount })`: Renders individual account quota cards, weekly/5-hour reset countdowns, and quick-switch action buttons `[AccountCard.jsx:AccountCard]`
- `BalancerCard({ balancer, onUpdateStrategy, onRotate })`: Renders quota pool balancing strategy selector and manual rotation trigger `[BalancerCard.jsx:BalancerCard]`
- `DiagnosticsModal({ isOpen, onClose })`: System diagnostic modal displaying doctor audit checks `[DiagnosticsModal.jsx:DiagnosticsModal]`
- `FilterBar({ currentFilter, onFilterChange, searchQuery, onSearchChange, counts })`: Filter chips and search input for account list filtering `[FilterBar.jsx:FilterBar]`
- `Header({ data, isRefreshing, onRefresh, onSwitchBest, onToggleShield, onToggleBalancer, onOpenDiagnostics, lastSyncedTime, syncInterval, onIntervalChange, now, theme, onToggleTheme, sseConnected })`: Top navigation header containing system indicators, SSE status, IP egress pill, and global control toggles `[Header.jsx:Header]`
- `KpiSummaryRow({ data, now })`: Key performance indicators row showing total active quota, fleet capacity, and earliest reset `[KpiSummaryRow.jsx:KpiSummaryRow]`
- `ModelDrawer({ models, now })`: Detailed slide-out drawer rendering individual model quotas and reset windows `[ModelDrawer.jsx:ModelDrawer]`
- `RefillTimeline({ accounts, now })`: Visual timeline displaying upcoming weekly quota reset events across registered accounts `[RefillTimeline.jsx:RefillTimeline]`
- `RunoutBanner({ burnrate })`: Alert banner warning when active account velocity indicates imminent quota depletion `[RunoutBanner.jsx:RunoutBanner]`
- `SurfaceSyncBanner({ activeSessions, onSyncSurfaces })`: Warning banner displayed when Desktop App and IDE sessions are logged into different accounts `[SurfaceSyncBanner.jsx:SurfaceSyncBanner]`
- `TrendChart({ snapshots })`: Historical quota consumption trend chart rendered as an SVG polyline `[TrendChart.jsx:TrendChart]`

## Invariants
- Returns `null` if `!account` `[AccountCard.jsx:AccountCard]`
- Categorizes quota badge status: `<= 1.0%` -> `FINISHED / EXHAUSTED`, `<= 10.0%` -> `CRITICAL`, `<= 25.0%` -> `LOW`, else -> `HEALTHY` `[AccountCard.jsx:getStatusBadge]`
- Maps bar color: `<= 10.0%` -> `bg-rose-500`, `<= 25.0%` -> `bg-amber-500`, `<= 50.0%` -> `bg-blue-500`, else -> `bg-emerald-500` `[AccountCard.jsx:getBarColor]`
- Flags `isUrgent` if weekly reset `parsed.diffSecs < 86400` `[AccountCard.jsx:AccountCard]`
- Returns `null` if `!balancer` `[BalancerCard.jsx:BalancerCard]`
- Defaults balancer strategy `strat` to `'watermark'` if undefined `[BalancerCard.jsx:strat]`
- Returns `null` if `!isOpen` `[DiagnosticsModal.jsx:DiagnosticsModal]`
- Classifies diagnostics check status against `PASS` and `WARN`, defaulting to `INFO` if omitted `[DiagnosticsModal.jsx:DiagnosticsModal]`
- Maintains fixed filter category definitions: `all`, `active`, `healthy`, `resetting`, `depleted` `[FilterBar.jsx:chips]`
- Defaults balancer strategy display name to `'WATERMARK'` with underscores replaced by spaces `[Header.jsx:stratName]`
- Renders synced text as `'Synced: just now'` if `diffSec < 5`, else `'Synced: ${diffSec}s ago'` `[Header.jsx:syncedText]`
- Disables refresh button while `isRefreshing` is true `[Header.jsx:Header]`
- Calculates earliest next refill by matching accounts with groups containing `'gemini'` and `weekly.resetTime` `[KpiSummaryRow.jsx:nextRefill]`
- Clamps quota meter width percentage between 0 and 100 via `Math.min(100, Math.max(0, ...))` `[KpiSummaryRow.jsx:KpiSummaryRow]`
- Returns `null` if `!models` or `models.length === 0` `[ModelDrawer.jsx:ModelDrawer]`
- Calculates model remaining percentage from `remainingPercent` or `(remainingFraction || 0) * 100` `[ModelDrawer.jsx:ModelDrawer]`
- Returns `null` if `!accounts` `[RefillTimeline.jsx:RefillTimeline]`
- Extracts accounts with gemini quota groups, sorts timeline items ascending by `diffSecs`, and flags `isUrgent` when `diffSecs < 86400` `[RefillTimeline.jsx:RefillTimeline]`
- Returns `null` if `!burnrate || !burnrate.active_metrics || !burnrate.active_metrics.primary` `[RunoutBanner.jsx:RunoutBanner]`
- Returns `null` if `!pri.is_depleting` `[RunoutBanner.jsx:RunoutBanner]`
- Returns `null` if `!activeSessions` or desktop/ide sessions missing or desktop.email equals ide.email `[SurfaceSyncBanner.jsx:SurfaceSyncBanner]`
- Returns `null` if `!snapshots` or `snapshots.length < 2` `[TrendChart.jsx:TrendChart]`
- Prevents division by zero in timeline calculation via `Math.max(1, maxTs - minTs)` `[TrendChart.jsx:tsSpan]`
- Only renders trend polylines for accounts with at least 2 snapshot data points `[TrendChart.jsx:lines]`

## Side Effects
- Invokes `onSwitchAccount(email, surface)` callback with surface `'both'`, `'desktop'`, or `'ide'` on button clicks `[AccountCard.jsx:onSwitchAccount]`
- Invokes `onUpdateStrategy(e.target.value)` on dropdown change and `onRotate` on button click `[BalancerCard.jsx:BalancerCard]`
- Executes asynchronous `fetch('/api/doctor?validate=true')` in `useEffect` on modal opening `[DiagnosticsModal.jsx:runAudit]`
- Invokes `onClose` callback on backdrop or close button click `[DiagnosticsModal.jsx:DiagnosticsModal]`
- Invokes `onFilterChange` and `onSearchChange` callbacks on user interaction `[FilterBar.jsx:FilterBar]`
- Calls `window.open('https://ipwho.is', '_blank')` on egress IP badge click `[Header.jsx:Header]`
- Invokes `onToggleShield`, `onToggleBalancer`, `onOpenDiagnostics`, `onToggleTheme`, `onRefresh`, `onSwitchBest`, and `onIntervalChange` callbacks `[Header.jsx:Header]`
- Invokes `onSyncSurfaces('ide')` or `onSyncSurfaces('desktop')` on alignment button clicks `[SurfaceSyncBanner.jsx:onSyncSurfaces]`

## Verification
- Run: `cd frontend && npm run build`
