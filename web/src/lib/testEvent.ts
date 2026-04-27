// Wire-only helper for the "Send test event" button on the project detail
// page. Posts the same JSON shape the documented curl one-liner uses, so
// browser-button parity with the curl path is exact. Returns a tagged
// result; the component layers state machine, toasts, and timers on top.

export type TestEventResult =
  | { kind: 'ok' }
  | { kind: 'http'; status: number; body: string }
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
    return { kind: 'network', error };
  }
}
