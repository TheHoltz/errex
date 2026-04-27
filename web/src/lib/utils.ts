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

export function shortFingerprint(fp: string): string {
  return fp.length > 10 ? `${fp.slice(0, 10)}…` : fp;
}

export function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${d.getMilliseconds().toString().padStart(3, '0')}`;
}
