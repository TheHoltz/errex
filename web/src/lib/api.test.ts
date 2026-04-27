// API client tests. We mock `fetch` and assert the URL/method/body shape
// the daemon expects. The daemon's matching `tests/api.rs` integration
// tests pin the server side; together they form a contract.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { api } from './api';
import type { Issue } from './types';

afterEach(() => {
  vi.restoreAllMocks();
});

function mockFetch(response: {
  ok: boolean;
  status?: number;
  json?: unknown;
  text?: string;
  headers?: Record<string, string>;
}) {
  // Default content-type to application/json so the new
  // `parseJsonOrFallback` guard doesn't reject our mock responses.
  const headerMap = new Map<string, string>(
    Object.entries(response.headers ?? { 'content-type': 'application/json' })
  );
  return vi.spyOn(globalThis, 'fetch').mockResolvedValue({
    ok: response.ok,
    status: response.status ?? (response.ok ? 200 : 500),
    headers: { get: (k: string) => headerMap.get(k.toLowerCase()) ?? null } as Headers,
    json: async () => response.json,
    text: async () => response.text ?? ''
  } as Response);
}

const baseIssue: Issue = {
  id: 1,
  project: 'p',
  fingerprint: 'fp',
  title: 'T',
  culprit: null,
  level: null,
  status: 'unresolved',
  event_count: 1,
  first_seen: '2026-01-01T00:00:00Z',
  last_seen: '2026-01-01T00:00:00Z'
};

describe('api.projects', () => {
  it('GETs /api/projects', async () => {
    const fetch = mockFetch({ ok: true, json: [] });
    await api.projects();
    expect(fetch).toHaveBeenCalledOnce();
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/projects');
    expect(init?.method ?? 'GET').toBe('GET');
  });
});

describe('api.issues', () => {
  it('GETs /api/issues without project filter', async () => {
    mockFetch({ ok: true, json: [] });
    await api.issues();
    const [url] = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(url).toBe('/api/issues');
  });

  it('encodes the project filter', async () => {
    mockFetch({ ok: true, json: [] });
    await api.issues('a name with spaces');
    const [url] = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(url).toBe('/api/issues?project=a%20name%20with%20spaces');
  });
});

describe('api.latestEvent', () => {
  it('returns null on 404 instead of throwing', async () => {
    mockFetch({ ok: false, status: 404 });
    const res = await api.latestEvent(99);
    expect(res).toBeNull();
  });

  it('throws on non-2xx other than 404', async () => {
    mockFetch({ ok: false, status: 500 });
    await expect(api.latestEvent(1)).rejects.toThrow(/500/);
  });

  it('parses JSON on 200', async () => {
    mockFetch({
      ok: true,
      json: { event_id: 'e1', received_at: 't', payload: {} }
    });
    const res = await api.latestEvent(1);
    expect(res?.event_id).toBe('e1');
  });
});

describe('api.setStatus', () => {
  it('PUTs /api/issues/:id/status with the right body', async () => {
    const fetch = mockFetch({ ok: true, json: { ...baseIssue, status: 'resolved' } });
    const res = await api.setStatus(42, 'resolved');
    expect(res.status).toBe('resolved');
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/issues/42/status');
    expect(init?.method).toBe('PUT');
    expect(init?.headers).toMatchObject({ 'content-type': 'application/json' });
    expect(JSON.parse(init?.body as string)).toEqual({ status: 'resolved' });
  });

  it('throws on non-2xx (so callers can roll back optimistic UI)', async () => {
    mockFetch({ ok: false, status: 404 });
    await expect(api.setStatus(99, 'resolved')).rejects.toThrow();
  });
});

describe('api admin client (cookie auth)', () => {
  it('listProjects GETs without an Authorization header (cookie is automatic)', async () => {
    const fetch = mockFetch({ ok: true, json: [] });
    await api.admin.listProjects();
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/admin/projects');
    const headers = (init?.headers ?? {}) as Record<string, string>;
    expect(headers['authorization']).toBeUndefined();
  });

  it('createProject POSTs JSON body', async () => {
    const fetch = mockFetch({
      ok: true,
      status: 201,
      json: {
        name: 'new',
        token: 'tok',
        dsn: 'http://x',
        webhook_url: null,
        created_at: 't',
        last_used_at: null,
        last_webhook_status: null,
        last_webhook_at: null
      }
    });
    const res = await api.admin.createProject('new');
    expect(res.name).toBe('new');
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/admin/projects');
    expect(init?.method).toBe('POST');
    expect(JSON.parse(init?.body as string)).toEqual({ name: 'new' });
  });

  it('setWebhook accepts null to clear', async () => {
    mockFetch({
      ok: true,
      json: {
        name: 'p',
        token: 't',
        webhook_url: null,
        dsn: 'd',
        created_at: 't',
        last_used_at: null,
        last_webhook_status: null,
        last_webhook_at: null
      }
    });
    await api.admin.setWebhook('p', null);
    const init = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[0]![1]!;
    expect(JSON.parse(init.body as string)).toEqual({ url: null });
  });

  it('rotateToken POSTs to the rotate endpoint', async () => {
    mockFetch({
      ok: true,
      json: {
        name: 'p',
        token: 'new-token',
        webhook_url: null,
        dsn: 'd',
        created_at: 't',
        last_used_at: null,
        last_webhook_status: null,
        last_webhook_at: null
      }
    });
    await api.admin.rotateToken('p');
    const [url, init] = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(url).toBe('/api/admin/projects/p/rotate');
    expect(init?.method).toBe('POST');
  });

  it('throws on 401 (callers route to /login)', async () => {
    mockFetch({ ok: false, status: 401 });
    await expect(api.admin.listProjects()).rejects.toThrowError(/401|sign-in/i);
  });

  it('throws on 403 (viewer hitting an admin endpoint)', async () => {
    mockFetch({ ok: false, status: 403 });
    await expect(api.admin.listProjects()).rejects.toThrowError(/403|admin/i);
  });
});

describe('api auth client', () => {
  it('login POSTs the credentials to /api/auth/login', async () => {
    const fetch = mockFetch({ ok: true, json: { username: 'daisy', role: 'admin' } });
    await api.auth.login('daisy', 'pw12345678901');
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/auth/login');
    expect(init?.method).toBe('POST');
    expect(JSON.parse(init?.body as string)).toEqual({
      username: 'daisy',
      password: 'pw12345678901'
    });
  });

  it('login surfaces Retry-After on 429', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: false,
      status: 429,
      headers: {
        get: (k: string) => (k.toLowerCase() === 'retry-after' ? '900' : null)
      } as Headers,
      json: async () => ({}),
      text: async () => 'too many'
    } as Response);
    try {
      await api.auth.login('daisy', 'wrong');
      throw new Error('expected reject');
    } catch (err) {
      if (!(err instanceof Error)) throw err;
      // The HttpError is exported but we test its retryAfterSecs via duck-typing
      // to keep this test independent of the import.
      const ra = (err as unknown as { retryAfterSecs: number | null }).retryAfterSecs;
      expect(ra).toBe(900);
    }
  });

  it('me GETs /api/auth/me', async () => {
    const fetch = mockFetch({ ok: true, json: { username: 'daisy', role: 'viewer' } });
    const me = await api.auth.me();
    expect(me.role).toBe('viewer');
    const [url] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/auth/me');
  });

  it('logout POSTs /api/auth/logout and tolerates 204', async () => {
    const fetch = mockFetch({ ok: true, status: 204 });
    await api.auth.logout();
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/auth/logout');
    expect(init?.method).toBe('POST');
  });
});

describe('api.admin.retention', () => {
  it('getRetention GETs /api/admin/retention', async () => {
    const fetch = mockFetch({
      ok: true,
      json: { events_per_issue_max: 0, issues_per_project_max: 0, event_retention_days: 0 }
    });
    const s = await api.admin.getRetention();
    expect(s.event_retention_days).toBe(0);
    const [url] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/admin/retention');
  });

  it('setRetention PUTs the full payload', async () => {
    const fetch = mockFetch({
      ok: true,
      json: { events_per_issue_max: 50, issues_per_project_max: 1000, event_retention_days: 7 }
    });
    const s = await api.admin.setRetention({
      events_per_issue_max: 50,
      issues_per_project_max: 1000,
      event_retention_days: 7
    });
    expect(s.events_per_issue_max).toBe(50);
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe('/api/admin/retention');
    expect(init?.method).toBe('PUT');
    expect(JSON.parse(init?.body as string)).toEqual({
      events_per_issue_max: 50,
      issues_per_project_max: 1000,
      event_retention_days: 7
    });
  });
});
