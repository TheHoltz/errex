// Wire types mirror crates/errex-proto. Keep field names in lockstep with the
// Rust serde representation — issue.rs, event.rs, wire.rs.

export type IssueLevel = 'debug' | 'info' | 'warning' | 'error' | 'fatal';

/** Server-side triage state. Mirrors `errex_proto::IssueStatus`. */
export type IssueStatus = 'unresolved' | 'resolved' | 'muted' | 'ignored';

export interface Issue {
  id: number;
  project: string;
  fingerprint: string;
  title: string;
  culprit: string | null;
  level: string | null;
  status: IssueStatus;
  event_count: number;
  first_seen: string;
  last_seen: string;
}

export interface ProjectSummary {
  project: string;
  issue_count: number;
}

// Server → client WebSocket frames. Tag matches `#[serde(tag = "type",
// rename_all = "snake_case")]` on errex_proto::ServerMessage.
export type ServerMessage =
  | { type: 'hello'; server_version: string }
  | { type: 'snapshot'; issues: Issue[] }
  | { type: 'issue_created'; issue: Issue }
  | { type: 'issue_updated'; issue: Issue };

export type ClientMessage = { type: 'ping' };

export type ConnectionStatus = 'connecting' | 'connected' | 'reconnecting' | 'disconnected';

// ----- Event detail -----
//
// Mirrors errex_proto::Event for the fields the UI renders. The daemon
// preserves the entire payload verbatim so we can surface fields the proto
// doesn't model yet (breadcrumbs, tags, contexts) by reading them as
// optional unknowns from the JSON.

export interface Frame {
  filename?: string | null;
  function?: string | null;
  module?: string | null;
  lineno?: number | null;
  colno?: number | null;
  in_app?: boolean | null;
  /** Some SDKs emit `vars` per frame; surfaced verbatim. */
  vars?: Record<string, unknown>;
}

export interface ExceptionInfo {
  type?: string | null;
  value?: string | null;
  module?: string | null;
  stacktrace?: { frames?: Frame[] } | null;
}

export interface Breadcrumb {
  /**
   * Sentry's wire format allows two shapes here: an ISO-8601 string (the
   * spec'd canonical form) and a numeric Unix epoch in seconds (what most
   * JS SDKs actually emit, sometimes with sub-second precision). errex
   * preserves whichever the SDK sent — render-side helpers normalize.
   */
  timestamp?: string | number | null;
  category?: string | null;
  level?: IssueLevel | null;
  message?: string | null;
  type?: string | null;
}

/** Sentry's envelope-event payload, as preserved by errexd. */
export interface EventPayload {
  event_id?: string;
  timestamp?: string;
  platform?: string | null;
  level?: IssueLevel | null;
  environment?: string | null;
  release?: string | null;
  server_name?: string | null;
  message?: string | null;
  exception?: { values?: ExceptionInfo[] } | null;
  breadcrumbs?: { values?: Breadcrumb[] } | Breadcrumb[] | null;
  tags?: Record<string, string> | Array<[string, string]> | null;
}

/** What `/api/issues/:id/event` returns. */
export interface StoredEvent {
  event_id: string;
  received_at: string;
  payload: EventPayload;
}

// ----- View-model shapes the components render -----
//
// Components stay decoupled from the wire format: the `selection.event`
// store is normalized into these shapes by `lib/eventDetail.ts` so the
// templates don't have to dig through optional Sentry-style nesting.

export interface Stack {
  type: string | null;
  value: string | null;
  frames: Frame[];
}

export interface NormalizedEvent {
  event_id: string;
  received_at: string;
  level: IssueLevel | null;
  release: string | null;
  environment: string | null;
  exception: Stack | null;
  breadcrumbs: Breadcrumb[];
  tags: Record<string, string>;
  /** Untouched payload as returned by the daemon. Kept so consumers can
   *  reach fields the normalized view-model intentionally drops (contexts,
   *  user, request, message) — used by the AI-context exporter. */
  raw: StoredEvent;
}
