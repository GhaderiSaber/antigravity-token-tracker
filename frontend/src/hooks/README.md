# Frontend Custom Hooks

## Overview
Provides React custom hooks and time-parsing utility functions for managing real-time countdown tickers, periodic UI refresh cycles, and ISO timestamp conversions across the dashboard interface.

## Interface
- `useLiveTicker()`: React hook returning the current `Date` object and re-evaluating consumer component state on a 1000ms tick interval `[useLiveTicker.js:useLiveTicker]`
- `parseResetTimestamp(isoStr, nowObj = new Date())`: Helper returning parsed timestamp representations `{ local, utc, diffSecs, dateObj }` `[useLiveTicker.js:parseResetTimestamp]`
- `formatCountdown(seconds)`: Helper formatting duration seconds into human-readable strings `[useLiveTicker.js:formatCountdown]`

## Invariants
- If `isoStr` is falsy, returns `{ local: 'N/A', utc: 'N/A', diffSecs: 0 }` `[useLiveTicker.js:parseResetTimestamp]`
- If `date.getTime()` is NaN, returns `{ local: isoStr, utc: isoStr, diffSecs: 0 }` `[useLiveTicker.js:parseResetTimestamp]`
- `diffSecs` is guaranteed non-negative via `Math.max(0, Math.floor((date.getTime() - nowObj.getTime()) / 1000))` `[useLiveTicker.js:parseResetTimestamp]`
- If `seconds <= 0`, always returns `'Resetting now'` `[useLiveTicker.js:formatCountdown]`
- Format tiers cascade hierarchically based on duration: `>0 days` -> `${days}d ${hours}h ${minutes}m ${secs}s`, `>0 hours` -> `${hours}h ${minutes}m ${secs}s`, `>0 minutes` -> `${minutes}m ${secs}s`, else `${secs}s` `[useLiveTicker.js:formatCountdown]`

## Side Effects
- Spawns a 1-second recurring interval via `setInterval` within `useEffect` updating React state, cleared on unmount via `clearInterval` `[useLiveTicker.js:useLiveTicker]`

## Verification
- Run: `cd frontend && npm run build`
