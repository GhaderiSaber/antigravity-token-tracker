# Frontend Source (`frontend/src`)

## Overview
Root application package for the frontend React single-page application. Manages client state, Server-Sent Events (SSE) connections, theme switching, polling intervals, account filtering, and coordination between child UI components and custom hooks.

## Interface
- `export default function App()`: Root React application component orchestrating the dashboard interface `[App.jsx:App]`
- [Components](./components/README.md): Modular presentation and control components
- [Hooks](./hooks/README.md): React custom hooks for live countdown tickers and timestamp formatting

## Invariants
- Default theme state falls back to `'light'` if `'agy_theme'` is absent from localStorage `[App.jsx:theme]`
- Auto-sync polling interval is disabled when `syncInterval <= 0` `[App.jsx:syncInterval]`
- Account classification: active if `acc.is_current_desktop_session || acc.is_current_ide_session`; healthy if `geminiPct > 20.0`; depleted if `geminiPct <= 1.0`; resetting if resetTime parses to `diffSecs < 86400` `[App.jsx:counts]`
- Priority sort ordering: dual active sessions (priority 0) precede desktop-only (priority 1), IDE-only (priority 2), and inactive sessions (priority 3); tie-breaks ordered descending by remaining weekly quota percentage `[App.jsx:filteredAccounts]`
- Tab filtering logic: `'active'` filter excludes inactive accounts; `'healthy'` filter excludes accounts with `geminiPct <= 20.0`; `'resetting'` filter excludes accounts where `isResettingSoon` is false; `'depleted'` filter excludes accounts with `geminiPct > 1.0`; search query performs case-insensitive substring match against `acc.email` or `acc.name` `[App.jsx:filteredAccounts]`

## Side Effects
- Mounts React component tree into DOM node `document.getElementById('root')` `[main.jsx:ReactDOM.createRoot]`
- Mutates `document.documentElement.classList` and persists theme preference to `localStorage.setItem('agy_theme', theme)` `[App.jsx:useEffect:theme]`
- Dispatches HTTP fetch requests to `/api/quota` and `/api/history` `[App.jsx:loadData]`
- Establishes persistent `EventSource('/api/events')` SSE connection for real-time updates `[App.jsx:useEffect:connectSSE]`
- Sets recurring polling interval via `window.setInterval` `[App.jsx:useEffect:syncInterval]`
- Dispatches mutation API requests (`/api/switch`, `/api/switch-best`, `/api/sync-surfaces`, `/api/shield`, `/api/balancer`, `/api/balancer/rotate`) `[App.jsx:handlers]`

## Verification
- Run: `cd frontend && npm run build`
