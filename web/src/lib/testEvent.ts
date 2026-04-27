// Wire-only helper for the "Send test event" button on the project detail
// page. Posts the same JSON shape the documented curl one-liner uses, so
// browser-button parity with the curl path is exact. Returns a tagged
// result; the component layers state machine, toasts, and timers on top.

export type TestEventResult =
  | { kind: 'ok' }
  | { kind: 'http'; status: number; body: string }
  | { kind: 'blocked' }
  | { kind: 'network'; error: unknown };

const BODY_PREVIEW_LIMIT = 140;

export async function sendTestEvent(dsn: string): Promise<TestEventResult> {
  // The catch covers both `fetch` rejection AND `res.text()` rejection —
  // a body read can fail mid-stream (connection drop, CORS body restrictions,
  // etc.) and the spec promises this helper never throws.
  try {
    const res = await fetch(dsn, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        event_id: 'test',
        level: 'error',
        message: 'errex test event',
      }),
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
    if (await isDaemonReachable(dsn)) return { kind: 'blocked' };
    return { kind: 'network', error };
  }
}

async function isDaemonReachable(dsn: string): Promise<boolean> {
  try {
    const probe = new URL('/health', dsn).toString();
    const res = await fetch(probe, { method: 'GET', cache: 'no-store' });
    return res.ok;
  } catch {
    return false;
  }
}
