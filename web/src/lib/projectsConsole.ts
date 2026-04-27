// Pure helpers for the /projects console. Lifted out of the Svelte
// components so they can be unit-tested without mounting anything.
//
// All time-dependent helpers take an explicit `now` (epoch ms) so tests
// don't need to mock `Date.now`.

import { ENVELOPE_CONTENT_TYPE, buildEnvelopeBody } from './testEvent';

export type ProjectActivityStatus = {
  /** A Tailwind background class for the dot. */
  tone: string;
  label: 'live' | 'recent' | 'idle' | 'never used';
};

const MIN = 60_000;
const HOUR = 60 * MIN;

/** Status dot for the rail. Same buckets the old card UI used. */
export function projectActivityStatus(
  lastUsedAt: string | null | undefined,
  now: number = Date.now()
): ProjectActivityStatus {
  if (!lastUsedAt) return { tone: 'bg-muted-foreground/40', label: 'never used' };
  const ms = now - new Date(lastUsedAt).getTime();
  if (ms < 5 * MIN) return { tone: 'bg-emerald-500', label: 'live' };
  if (ms < HOUR) return { tone: 'bg-amber-500', label: 'recent' };
  return { tone: 'bg-muted-foreground/60', label: 'idle' };
}

export type WebhookHealth = {
  tone: 'ok' | 'fail' | 'never';
  /** Human-readable line, e.g. "200 · 12s ago" or "never delivered". */
  label: string;
};

/** Surfaces the most recent webhook delivery as a single-line health badge.
 *  Status 0 is the transport-failure sentinel from the daemon. */
export function formatWebhookHealth(
  status: number | null | undefined,
  at: string | null | undefined,
  now: number = Date.now()
): WebhookHealth {
  if (status === null || status === undefined) {
    return { tone: 'never', label: 'never delivered' };
  }
  const rel = at ? formatRelative(now - new Date(at).getTime()) : 'just now';
  if (status === 0) return { tone: 'fail', label: `transport error · ${rel}` };
  if (status >= 200 && status < 300) {
    return { tone: 'ok', label: `${status} · ${rel}` };
  }
  return { tone: 'fail', label: `${status} · ${rel}` };
}

/** Compact relative time for inline labels. Always past tense, always short. */
function formatRelative(diffMs: number): string {
  const abs = Math.max(0, diffMs);
  if (abs < MIN) return `${Math.max(1, Math.round(abs / 1000))}s ago`;
  if (abs < HOUR) return `${Math.round(abs / MIN)}m ago`;
  if (abs < 24 * HOUR) return `${Math.round(abs / HOUR)}h ago`;
  return `${Math.round(abs / (24 * HOUR))}d ago`;
}

/** Type-to-confirm gate for Delete. Trims whitespace; case-sensitive (project
 *  names ARE case-sensitive in errex). Empty input never matches. */
export function isDeleteConfirmed(typed: string, projectName: string): boolean {
  return typed.trim().length > 0 && typed.trim() === projectName;
}

/** Builds a copy-pasteable curl that posts a Sentry envelope to the project's
 *  ingest endpoint. The body comes from `buildEnvelopeBody` (shared with the
 *  click-to-test button), wrapped in `$'…'` so the shell preserves the
 *  literal newlines the parser requires. */
export function buildTestEventCurl(
  dsn: string,
  opts: { eventId?: string; sentAt?: string } = {}
): string {
  const body = buildEnvelopeBody(opts).replace(/\n/g, '\\n');
  // POSIX single-quote escape: project names allow apostrophes (e.g. "O'Brien"),
  // and the daemon's DSN format embeds the name verbatim — without this, an
  // apostrophe terminates the shell quoting and corrupts the request URL.
  return [
    `curl -X POST '${shellEscapeSingleQuoted(dsn)}' \\`,
    `  -H 'content-type: ${ENVELOPE_CONTENT_TYPE}' \\`,
    `  --data-binary $'${body}'`
  ].join('\n');
}

function shellEscapeSingleQuoted(value: string): string {
  return value.replace(/'/g, "'\\''");
}

/** Validation for the inline rename input. Mirrors the daemon's accepted
 *  shape: non-empty, no leading/trailing whitespace after trim, max 64 chars,
 *  and not equal to the current name (no-op rename has nothing to do). */
export function validateNewProjectName(
  newName: string,
  currentName: string
): { ok: true } | { ok: false; reason: string } {
  const trimmed = newName.trim();
  if (trimmed.length === 0) return { ok: false, reason: 'name is required' };
  if (trimmed.length > 64) return { ok: false, reason: 'name is too long (max 64)' };
  if (trimmed === currentName) return { ok: false, reason: 'name unchanged' };
  return { ok: true };
}
