import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export type WithElementRef<T> = T & { ref?: HTMLElement | null };
export type WithoutChildrenOrChild<T> = Omit<T, 'children' | 'child'>;

const RTF = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });
const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export function relativeTime(iso: string, now = Date.now()): string {
  const t = new Date(iso).getTime();
  const diff = t - now;
  const abs = Math.abs(diff);
  if (abs < MINUTE) return RTF.format(Math.round(diff / SECOND), 'second');
  if (abs < HOUR) return RTF.format(Math.round(diff / MINUTE), 'minute');
  if (abs < DAY) return RTF.format(Math.round(diff / HOUR), 'hour');
  return RTF.format(Math.round(diff / DAY), 'day');
}

/**
 * Compact "5h" / "2d" form for tight columns where the long
 * RelativeTimeFormat output would wrap. Past-only — sign is dropped.
 */
export function relativeTimeShort(iso: string, now = Date.now()): string {
  const t = new Date(iso).getTime();
  const abs = Math.abs(t - now);
  if (abs < MINUTE) return `${Math.max(1, Math.floor(abs / SECOND))}s`;
  if (abs < HOUR) return `${Math.floor(abs / MINUTE)}m`;
  if (abs < DAY) return `${Math.floor(abs / HOUR)}h`;
  return `${Math.floor(abs / DAY)}d`;
}

/**
 * Threshold tier for an event count. Drives the typographic color of the
 * count cell in the issue list — low stays neutral so the row reads as
 * routine, mid amber, high light-red. Boundaries are inclusive at the low
 * end (10 → mid, 100 → high).
 */
export function countTier(n: number): 'low' | 'mid' | 'high' {
  if (n >= 100) return 'high';
  if (n >= 10) return 'mid';
  return 'low';
}

export function shortFingerprint(fp: string): string {
  return fp.length > 10 ? `${fp.slice(0, 10)}…` : fp;
}

export function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${d.getMilliseconds().toString().padStart(3, '0')}`;
}

/**
 * Middle-truncate a path or URL so the last segment (typically the bundle
 * filename) always survives. Used by the issue-list subtitle where the URL
 * column is narrow and the filename carries the most signal.
 *
 *   "/a/b/c/very/long/path/to/bundle-abc123.js" → "/a/b/…/bundle-abc123.js"
 */
export function midTruncatePath(s: string, max = 36): string {
  if (s.length <= max) return s;
  const sep = s.includes('/') ? '/' : '\\';
  const lastSep = s.lastIndexOf(sep);
  if (lastSep === -1) return `…${s.slice(-(max - 1))}`;
  const tail = s.slice(lastSep); // includes the leading sep
  if (tail.length >= max - 1) return `…${tail.slice(-(max - 1))}`;
  return `${s.slice(0, max - tail.length - 1)}…${tail}`;
}
