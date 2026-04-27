// Tests the wire-only helper used by the "Send test event" button on the
// project detail page. The component owns the state machine and toasts;
// this helper just talks to the ingest endpoint and reports what happened.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { sendTestEvent } from './testEvent';

afterEach(() => {
  vi.restoreAllMocks();
});

const DSN = 'http://localhost:9090/api/demo/envelope/?sentry_key=abc123';

describe('sendTestEvent', () => {
  it('POSTs the documented JSON body to the DSN', async () => {
    const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => '',
    } as Response);

    await sendTestEvent(DSN);

    expect(fetch).toHaveBeenCalledOnce();
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe(DSN);
    expect(init?.method).toBe('POST');
    expect((init?.headers as Record<string, string>)['content-type']).toBe(
      'application/json'
    );
    expect(JSON.parse(init?.body as string)).toEqual({
      event_id: 'test',
      level: 'error',
      message: 'errex test event',
    });
  });

  it('returns { kind: "ok" } on a 2xx response', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      text: async () => '',
    } as Response);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({ kind: 'ok' });
  });

  it('returns { kind: "http", status, body } on a non-2xx response, body truncated to 140 chars', async () => {
    const longBody = 'x'.repeat(500);
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: false,
      status: 401,
      text: async () => longBody,
    } as Response);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({
      kind: 'http',
      status: 401,
      body: 'x'.repeat(140),
    });
  });

  it('returns { kind: "network", error } when both the envelope fetch and the /health probe fail', async () => {
    // mockRejectedValue (not mockRejectedValueOnce) → BOTH calls reject:
    // first the envelope POST, then the /health probe. With the daemon
    // genuinely unreachable, the helper falls through to the network kind.
    const boom = new TypeError('Failed to fetch');
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(boom);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({ kind: 'network', error: boom });
  });

  it('returns { kind: "blocked" } when the envelope fetch fails but a /health probe succeeds', async () => {
    // Adblockers (uBlock Origin et al.) match `/api/*/envelope/*` against
    // their Sentry rules and reject the request with the same TypeError as
    // a real network failure. We disambiguate by probing /health on the
    // same origin — that path has no Sentry signature and gets through.
    const boom = new TypeError('Failed to fetch');
    const fetch = vi
      .spyOn(globalThis, 'fetch')
      .mockRejectedValueOnce(boom)
      .mockResolvedValueOnce({ ok: true, status: 200 } as Response);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({ kind: 'blocked' });

    expect(fetch).toHaveBeenCalledTimes(2);
    const probeUrl = fetch.mock.calls[1]![0];
    expect(probeUrl).toBe('http://localhost:9090/health');
  });
});
