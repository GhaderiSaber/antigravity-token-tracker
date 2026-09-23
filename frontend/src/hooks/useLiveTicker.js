import { useState, useEffect } from 'react';

export function useLiveTicker() {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = setInterval(() => {
      setNow(new Date());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  return now;
}

export function parseResetTimestamp(isoStr, nowObj = new Date()) {
  if (!isoStr) return { local: 'N/A', utc: 'N/A', diffSecs: 0 };
  const date = new Date(isoStr);
  if (isNaN(date.getTime())) return { local: isoStr, utc: isoStr, diffSecs: 0 };
  
  const diffSecs = Math.max(0, Math.floor((date.getTime() - nowObj.getTime()) / 1000));
  
  const localStr = date.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' }) + 
                   ' at ' + date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  const utcStr = date.toISOString().substring(0, 10) + ' ' + date.toISOString().substring(11, 16) + ' UTC';
  
  return { local: localStr, utc: utcStr, diffSecs, dateObj: date };
}

export function formatCountdown(seconds) {
  if (seconds <= 0) return 'Resetting now';
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  
  if (days > 0) return `${days}d ${hours}h ${minutes}m ${secs}s`;
  if (hours > 0) return `${hours}h ${minutes}m ${secs}s`;
  if (minutes > 0) return `${minutes}m ${secs}s`;
  return `${secs}s`;
}
