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

  it('returns { kind: "network", error } when fetch throws', async () => {
    const boom = new TypeError('Failed to fetch');
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(boom);

    const result = await sendTestEvent(DSN);
    expect(result).toEqual({ kind: 'network', error: boom });
  });
});
