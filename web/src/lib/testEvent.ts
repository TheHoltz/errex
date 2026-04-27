// Wire-only helper for the "Send test event" button on the project detail
// page. Posts the same Sentry envelope the documented curl one-liner builds,
// so browser-button parity with the curl path is exact. Returns a tagged
// result; the component layers state machine, toasts, and timers on top.

export type TestEventResult =
  | { kind: 'ok' }
  | { kind: 'http'; status: number; body: string }
  | { kind: 'blocked' }
  | { kind: 'network'; error: unknown };

const BODY_PREVIEW_LIMIT = 140;

export const ENVELOPE_CONTENT_TYPE = 'application/x-sentry-envelope';

/** Builds the three-line Sentry envelope body that errexd's ingest parser
 *  expects: envelope header, item header, payload — newline-separated, with
 *  a trailing newline after the payload. Pass `eventId` / `sentAt` for
 *  deterministic tests; production callers leave them undefined. */
export function buildEnvelopeBody(
  opts: { eventId?: string; sentAt?: string } = {}
): string {
  const eventId = opts.eventId ?? randomUuid();
  const sentAt = opts.sentAt ?? new Date().toISOString();
  return [
    JSON.stringify({ event_id: eventId, sent_at: sentAt }),
    JSON.stringify({ type: 'event' }),
    JSON.stringify({
      level: 'error',
      message: 'errex test event',
      exception: {
        values: [{ type: 'TestEvent', value: 'errex test event from curl' }],
      },
    }),
    '',
  ].join('\n');
}

/** Posts directly to the daemon's ingest URL — the curl-style POST URL
 *  with `?sentry_key=` query auth, NOT the Sentry-standard DSN (which
 *  isn't a fetch-able URL on its own; SDKs construct the path from it). */
export async function sendTestEvent(ingestUrl: string): Promise<TestEventResult> {
  // The catch covers both `fetch` rejection AND `res.text()` rejection —
  // a body read can fail mid-stream (connection drop, CORS body restrictions,
  // etc.) and the spec promises this helper never throws.
  try {
    const res = await fetch(ingestUrl, {
      method: 'POST',
      headers: { 'content-type': ENVELOPE_CONTENT_TYPE },
      body: buildEnvelopeBody(),
    });

    if (res.ok) return { kind: 'ok' };

    const text = await res.text();
    return {
      kind: 'http',
      status: res.status,
      body: text.slice(0, BODY_PREVIEW_LIMIT),
    };
  } catch (error) {
    // Adblockers commonly match `/api/*/envelope/*` (Sentry signature) and
    // reject with the same TypeError as a real offline failure. Probe a
    // signature-free endpoint on the same origin to disambiguate: if the
    // daemon answers, the envelope was specifically blocked client-side.
    if (await isDaemonReachable(ingestUrl)) return { kind: 'blocked' };
    return { kind: 'network', error };
  }
}

async function isDaemonReachable(ingestUrl: string): Promise<boolean> {
  try {
    const probe = new URL('/health', ingestUrl).toString();
    const res = await fetch(probe, { method: 'GET', cache: 'no-store' });
    return res.ok;
  } catch {
    return false;
  }
}

function randomUuid(): string {
  // crypto.randomUUID is available in modern browsers and in jsdom 24+.
  // Fall back to a v4-shaped string only if the environment stubs crypto out.
  const c = typeof globalThis !== 'undefined' ? globalThis.crypto : undefined;
  if (c && typeof c.randomUUID === 'function') return c.randomUUID();
  const r = () => Math.floor(Math.random() * 0x10000).toString(16).padStart(4, '0');
  return `${r()}${r()}-${r()}-${r()}-${r()}-${r()}${r()}${r()}`;
}
