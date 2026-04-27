// Tests the wire-only helper used by the "Send test event" button on the
// project detail page. The component owns the state machine and toasts;
// this helper just talks to the ingest endpoint and reports what happened.

import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ENVELOPE_CONTENT_TYPE,
  buildEnvelopeBody,
  sendTestEvent,
} from './testEvent';

afterEach(() => {
  vi.restoreAllMocks();
});

const DSN = 'http://localhost:9090/api/demo/envelope/?sentry_key=abc123';

describe('sendTestEvent', () => {
  it('POSTs a Sentry envelope to the DSN with the envelope content-type', async () => {
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
      ENVELOPE_CONTENT_TYPE
    );
    // The daemon's `parse_envelope` requires three newline-separated JSON
    // lines (envelope header, item header, payload) plus a trailing newline.
    // Anything else returns 400 "invalid envelope". Lock that shape here.
    const body = init?.body as string;
    const parts = body.split('\n');
    expect(parts).toHaveLength(4);
    expect(parts[3]).toBe('');
    expect(JSON.parse(parts[0] ?? '')).toMatchObject({
      event_id: expect.any(String),
      sent_at: expect.any(String),
    });
    expect(JSON.parse(parts[1] ?? '')).toEqual({ type: 'event' });
    const payload = JSON.parse(parts[2] ?? '');
    expect(payload.level).toBe('error');
    expect(payload.message).toMatch(/errex/i);
    expect(payload.exception?.values?.[0]?.type).toBeDefined();
  });

  it('buildEnvelopeBody produces deterministic output when given fixed eventId/sentAt', () => {
    const body = buildEnvelopeBody({
      eventId: '00000000-0000-0000-0000-000000000001',
      sentAt: '2026-04-26T12:00:00Z',
    });
    expect(body.endsWith('\n')).toBe(true);
    const parts = body.split('\n');
    expect(parts).toHaveLength(4);
    expect(JSON.parse(parts[0] ?? '')).toEqual({
      event_id: '00000000-0000-0000-0000-000000000001',
      sent_at: '2026-04-26T12:00:00Z',
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
