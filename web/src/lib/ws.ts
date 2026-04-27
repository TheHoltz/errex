import { browser } from '$app/environment';

import { eventStream } from './eventStream.svelte';
import { connection, issues, load, projects } from './stores.svelte';
import type { ServerMessage } from './types';

// Single live WebSocket per project. `connect(project)` tears down any
// existing socket and brings up a new one. Reconnects use a fixed 5s backoff
// (per spec) — no jittered exponential, the daemon is local and a flat retry
// is easy to reason about.

const RECONNECT_MS = 5000;

let socket: WebSocket | null = null;
let pendingProject: string | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

function url(project: string): string {
  // Same origin as the SPA in both prod and dev. In prod the daemon's axum
  // listener handles the upgrade on the HTTP port; in dev Vite proxies
  // /ws/* to that same listener. Building from `location.host` keeps the
  // SPA portable across deployments without env-time configuration.
  if (!browser) return '';
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${location.host}/ws/${encodeURIComponent(project)}`;
}

export function connect(project: string) {
  if (!browser) return;
  // Idempotent for same-project re-calls: if we're already connecting or
  // connected to this project, leave the socket alone. Without this guard,
  // two callers in the same tick (bootstrap + page-effect on a project
  // route, an HMR re-fire, etc.) would close an in-flight socket and trip
  // the browser's "WebSocket is closed before the connection is
  // established" warning.
  if (
    socket &&
    pendingProject === project &&
    (socket.readyState === WebSocket.CONNECTING || socket.readyState === WebSocket.OPEN)
  ) {
    projects.current = project;
    return;
  }

  // Project switch invalidates the in-memory view; the new project will
  // arrive via Snapshot in a moment.
  if (project !== projects.current) eventStream.clear();
  pendingProject = project;
  projects.current = project;

  if (socket) {
    socket.onclose = null;
    socket.onerror = null;
    socket.onmessage = null;
    socket.onopen = null;
    socket.close();
    socket = null;
  }
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }

  connection.status = 'connecting';
  open();
}

function open() {
  if (pendingProject === null) return;
  const project = pendingProject;

  let ws: WebSocket;
  try {
    ws = new WebSocket(url(project));
  } catch (err) {
    console.warn('ws: construct failed', err);
    scheduleReconnect();
    return;
  }
  socket = ws;

  ws.onopen = () => {
    connection.status = 'connected';
  };

  ws.onmessage = (ev) => {
    let msg: ServerMessage;
    try {
      msg = JSON.parse(ev.data) as ServerMessage;
    } catch {
      return;
    }
    handle(msg);
  };

  ws.onerror = () => {
    // `onclose` always fires after `onerror`; defer the actual reconnect
    // scheduling there so it happens exactly once per disconnect.
  };

  ws.onclose = () => {
    if (socket === ws) socket = null;
    if (pendingProject !== null) {
      connection.status = 'reconnecting';
      scheduleReconnect();
    } else {
      connection.status = 'disconnected';
    }
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    open();
  }, RECONNECT_MS);
}

function handle(msg: ServerMessage) {
  switch (msg.type) {
    case 'hello':
      connection.serverVersion = msg.server_version;
      return;
    case 'snapshot':
      issues.reset(msg.issues);
      // Snapshot doesn't carry per-event timestamps; clear stream rather than
      // pretending we know rates we don't. Live updates rebuild the picture.
      eventStream.clear();
      load.initialLoad = false;
      return;
    case 'issue_created':
    case 'issue_updated':
      issues.upsert(msg.issue);
      eventStream.record(msg.issue.id);
      return;
  }
}

export function disconnect() {
  pendingProject = null;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  if (socket) {
    socket.close();
    socket = null;
  }
  connection.status = 'disconnected';
}
