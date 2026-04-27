import type { Issue, IssueStatus, ProjectSummary, StoredEvent } from './types';

// Same-origin in production (SPA is served by the daemon). In dev, Vite proxies
// /api to the daemon on :9090, so this base stays empty either way.
const BASE = '';

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { headers: { accept: 'application/json' } });
  if (res.status === 404) {
    // Surface "not found" as null so callers don't have to inspect status.
    return null as T;
  }
  if (!res.ok) throw new Error(`${path}: HTTP ${res.status}`);
  return res.json() as Promise<T>;
}

async function send<T>(
  method: string,
  path: string,
  body?: unknown,
  extraHeaders?: Record<string, string>
): Promise<T> {
  const headers: Record<string, string> = { 'content-type': 'application/json', ...extraHeaders };
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body)
  });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new HttpError(res.status, text || `${method} ${path}: HTTP ${res.status}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

export class HttpError extends Error {
  /** Optional `Retry-After` value parsed from the response (in seconds).
   *  Only populated for 429 lockout responses. */
  public retryAfterSecs: number | null = null;
  constructor(
    public status: number,
    message: string
  ) {
    super(`${status}: ${message}`);
    this.name = 'HttpError';
  }
}

// ----- auth shapes -----

export type Role = 'admin' | 'viewer';

export interface AuthMe {
  username: string;
  role: Role;
}

export interface SetupStatus {
  /** True iff the daemon has zero users AND a setup token is configured. */
  needs_setup: boolean;
  /** True iff the daemon has zero users but the operator never set
   *  ERREXD_ADMIN_TOKEN — there is no way for the SPA to bootstrap. */
  setup_disabled: boolean;
}

export interface User {
  username: string;
  role: Role;
  created_at: string;
  last_login_at: string | null;
  last_login_ip: string | null;
  /** ISO timestamp when the user was deactivated. `null` = active. */
  deactivated_at: string | null;
}

export interface UserSession {
  id: string;
  username: string;
  created_at: string;
  last_seen_at: string;
  ip: string | null;
  user_agent: string | null;
}

// ----- admin shapes (mirrors AdminProjectView in ingest.rs) -----

export interface AdminProject {
  name: string;
  token: string;
  webhook_url: string | null;
  dsn: string;
  created_at: string;
  last_used_at: string | null;
  /** HTTP status from the most recent webhook delivery, or 0 for transport
   *  failure. `null` until the first delivery attempt fires. */
  last_webhook_status: number | null;
  last_webhook_at: string | null;
}

/** 24-hour activity rollup returned by /api/admin/projects/:name/activity.
 *  Mirrors `ActivityStats` in store.rs. */
export interface ActivityStats {
  events_24h: number;
  unique_issues_24h: number;
  last_event_at: string | null;
  /** 24 hourly counts, oldest → newest. Index 23 contains "now". */
  hourly_buckets: number[];
}

/** What `delete_project` (and its preview sibling) destroys. */
export interface DeleteSummary {
  events_deleted: number;
  issues_deleted: number;
}

// ----- shared admin/auth fetch wrapper -----
//
// All authenticated endpoints rely on the `errex_session` cookie that the
// browser sends automatically. We don't pass any per-call token. The
// wrapper is shared by api.auth.* and api.admin.* — the only thing they
// differ on is the URL path.

async function authedGet<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { accept: 'application/json' }
  });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw httpErrorWithRetryAfter(res, mapAuthError(res.status, text));
  }
  return parseJsonOrFallback<T>(res, path);
}

async function authedSend<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body)
  });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw httpErrorWithRetryAfter(res, mapAuthError(res.status, text));
  }
  if (res.status === 204) return undefined as T;
  return parseJsonOrFallback<T>(res, path);
}

function httpErrorWithRetryAfter(res: Response, message: string): HttpError {
  const err = new HttpError(res.status, message);
  if (res.status === 429) {
    const ra = res.headers.get('retry-after');
    const secs = ra ? Number(ra) : NaN;
    if (!Number.isNaN(secs)) err.retryAfterSecs = secs;
  }
  return err;
}

/** Guard against the daemon returning the SPA's index.html for an unknown
 *  route (status 200, content-type text/html). Without this check, the
 *  caller sees an opaque "Unexpected token '<'" SyntaxError; with it, the
 *  caller sees a clear "endpoint missing" message that matches a typical
 *  daemon/SPA version skew. */
async function parseJsonOrFallback<T>(res: Response, path: string): Promise<T> {
  const ctype = res.headers.get('content-type') ?? '';
  if (!ctype.includes('json')) {
    throw new HttpError(
      404,
      `${path}: daemon returned ${ctype || 'no content-type'} — is the daemon up to date with the SPA?`
    );
  }
  return res.json() as Promise<T>;
}

function mapAuthError(status: number, body: string): string {
  if (status === 401) return body || 'sign-in required';
  if (status === 403) return body || 'admin role required';
  if (status === 429) return body || 'too many attempts — try again later';
  if (status === 503) return body || 'service temporarily unavailable';
  return body || `HTTP ${status}`;
}

export const api = {
  projects: () => get<ProjectSummary[]>('/api/projects'),
  issues: (project?: string) =>
    get<Issue[]>(project ? `/api/issues?project=${encodeURIComponent(project)}` : '/api/issues'),
  latestEvent: (issueId: number) => get<StoredEvent | null>(`/api/issues/${issueId}/event`),

  /** Server-authoritative status mutation. WS will broadcast `IssueUpdated`
   *  to all connected clients including this one — the returned Issue is
   *  authoritative for optimistic-UI rollback if the caller cares. */
  setStatus: (id: number, status: IssueStatus) =>
    send<Issue>('PUT', `/api/issues/${id}/status`, { status }),

  auth: {
    setupStatus: () => authedGet<SetupStatus>('/api/auth/setup-status'),
    setup: (token: string, username: string, password: string) =>
      authedSend<AuthMe>('POST', '/api/auth/setup', { token, username, password }),
    login: (username: string, password: string) =>
      authedSend<AuthMe>('POST', '/api/auth/login', { username, password }),
    logout: () => authedSend<void>('POST', '/api/auth/logout'),
    me: () => authedGet<AuthMe>('/api/auth/me')
  },

  admin: {
    listProjects: () => authedGet<AdminProject[]>('/api/admin/projects'),
    createProject: (name: string) =>
      authedSend<AdminProject>('POST', '/api/admin/projects', { name }),
    setWebhook: (name: string, url: string | null) =>
      authedSend<AdminProject>(
        'PUT',
        `/api/admin/projects/${encodeURIComponent(name)}/webhook`,
        { url }
      ),
    rotateToken: (name: string) =>
      authedSend<AdminProject>('POST', `/api/admin/projects/${encodeURIComponent(name)}/rotate`),
    renameProject: (oldName: string, newName: string) =>
      authedSend<AdminProject>(
        'PATCH',
        `/api/admin/projects/${encodeURIComponent(oldName)}`,
        { name: newName }
      ),
    deleteProject: (name: string) =>
      authedSend<DeleteSummary>('DELETE', `/api/admin/projects/${encodeURIComponent(name)}`),
    destroyPreview: (name: string) =>
      authedGet<DeleteSummary>(`/api/admin/projects/${encodeURIComponent(name)}/destroy-preview`),
    getActivity: (name: string) =>
      authedGet<ActivityStats>(`/api/admin/projects/${encodeURIComponent(name)}/activity`),

    listUsers: () => authedGet<User[]>('/api/admin/users'),
    createUser: (username: string, password: string, role: Role) =>
      authedSend<User>('POST', '/api/admin/users', { username, password, role }),
    getUser: (username: string) =>
      authedGet<User>(`/api/admin/users/${encodeURIComponent(username)}`),
    updateUser: (
      username: string,
      patch: { password?: string; role?: Role; deactivated?: boolean }
    ) => authedSend<User>('PATCH', `/api/admin/users/${encodeURIComponent(username)}`, patch),
    deleteUser: (username: string) =>
      authedSend<void>('DELETE', `/api/admin/users/${encodeURIComponent(username)}`),
    listUserSessions: (username: string) =>
      authedGet<UserSession[]>(
        `/api/admin/users/${encodeURIComponent(username)}/sessions`
      ),
    revokeUserSessions: (username: string) =>
      authedSend<{ sessions_revoked: number }>(
        'POST',
        `/api/admin/users/${encodeURIComponent(username)}/sessions/revoke-all`
      ),

    getRetention: () => authedGet<RetentionSettings>('/api/admin/retention'),
    setRetention: (s: RetentionSettings) =>
      authedSend<RetentionSettings>('PUT', '/api/admin/retention', s)
  }
};

/** Operator-configurable retention limits. `0` means "unlimited" for any
 *  field; the daemon's retention task reads these every hour and trims
 *  excess events / issues / aged payloads accordingly. */
export interface RetentionSettings {
  events_per_issue_max: number;
  issues_per_project_max: number;
  event_retention_days: number;
}
